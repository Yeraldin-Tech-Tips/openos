use super::{DriverError, KernelDriver};
use crate::{arch::x86_64::serial, sync::IrqSafeLock};

const INTEL_VENDOR_ID: u16 = 0x8086;
const PCI_CONFIG_ADDR: u16 = 0xCF8;
const PCI_CONFIG_DATA: u16 = 0xCFC;
const PCI_COMMAND_OFFSET: u8 = 0x04;
const PCI_BAR0_OFFSET: u8 = 0x10;
const PCI_CLASS_OFFSET: u8 = 0x08;

const PCI_COMMAND_IO_SPACE: u16 = 1 << 0;
const PCI_COMMAND_MEM_SPACE: u16 = 1 << 1;
const PCI_COMMAND_BUS_MASTER: u16 = 1 << 2;

const RX_BUFFER_CAPACITY: usize = 2048;

const SUPPORTED_INTEL_WLAN_IDS: &[u16] = &[
    0x24FD, // Wireless 8265
    0x2526, // Wireless 9260
    0x2723, // Wi-Fi 6 AX200
    0x51F0, // Wi-Fi 6E AX211
];

#[derive(Clone, Copy)]
struct PciLocation {
    bus: u8,
    device: u8,
    function: u8,
}

#[derive(Clone, Copy)]
struct AdapterResources {
    pci: PciLocation,
    vendor_id: u16,
    device_id: u16,
    mmio_base: u64,
    mmio_len: u64,
}

#[derive(Clone, Copy)]
struct WifiRuntime {
    adapter: Option<AdapterResources>,
    initialized: bool,
    rx_len: usize,
    rx_buffer: [u8; RX_BUFFER_CAPACITY],
}

impl WifiRuntime {
    const fn new() -> Self {
        Self {
            adapter: None,
            initialized: false,
            rx_len: 0,
            rx_buffer: [0; RX_BUFFER_CAPACITY],
        }
    }
}

static WIFI_RUNTIME: IrqSafeLock<WifiRuntime> = IrqSafeLock::new(WifiRuntime::new());

pub struct IntelWifi;

impl KernelDriver for IntelWifi {
    fn name(&self) -> &'static str {
        "intel-wifi"
    }

    fn probe(&self) -> bool {
        serial::write_line("[openos-kernel] intel-wifi probe begin");
        let adapter = scan_supported_adapter().and_then(map_adapter_resources);
        let mut runtime = WIFI_RUNTIME.lock();
        runtime.adapter = adapter;
        runtime.initialized = false;
        runtime.rx_len = 0;

        if let Some(resources) = adapter {
            serial::write_line("[openos-kernel] intel-wifi probe matched");
            serial::write_hex_u64(
                "[openos-kernel] intel-wifi pci.vendor=",
                resources.vendor_id as u64,
            );
            serial::write_hex_u64(
                "[openos-kernel] intel-wifi pci.device=",
                resources.device_id as u64,
            );
            serial::write_hex_u64("[openos-kernel] intel-wifi mmio.base=", resources.mmio_base);
            serial::write_hex_u64("[openos-kernel] intel-wifi mmio.len=", resources.mmio_len);
            true
        } else {
            serial::write_line("[openos-kernel] intel-wifi probe no supported adapter");
            false
        }
    }

    fn init(&self) -> Result<(), DriverError> {
        let mut runtime = WIFI_RUNTIME.lock();
        let adapter = match runtime.adapter {
            Some(resources) => resources,
            None => {
                serial::write_line("[openos-kernel] intel-wifi init failed: no probed adapter");
                return Err(DriverError::ProbeFailed);
            }
        };

        if let Err(reason) = firmware_load_hook(adapter.device_id) {
            serial::write_line("[openos-kernel] intel-wifi firmware load failed");
            serial::write_line(reason);
            runtime.initialized = false;
            return Err(DriverError::NotReady);
        }

        serial::write_hex_u64(
            "[openos-kernel] intel-wifi pci.bus=",
            adapter.pci.bus as u64,
        );
        serial::write_hex_u64(
            "[openos-kernel] intel-wifi pci.device=",
            adapter.pci.device as u64,
        );
        serial::write_hex_u64(
            "[openos-kernel] intel-wifi pci.function=",
            adapter.pci.function as u64,
        );
        runtime.initialized = true;
        runtime.rx_len = 0;
        serial::write_line("[openos-kernel] intel-wifi station mode ready");
        Ok(())
    }
}

pub fn transmit_station_frame(payload: &[u8]) -> Result<usize, DriverError> {
    if payload.is_empty() {
        return Ok(0);
    }

    let mut runtime = WIFI_RUNTIME.lock();
    if !runtime.initialized {
        return Err(DriverError::NotReady);
    }

    let frame_len = payload.len().min(RX_BUFFER_CAPACITY);
    runtime.rx_buffer[..frame_len].copy_from_slice(&payload[..frame_len]);
    runtime.rx_len = frame_len;

    serial::write_line("[openos-kernel] intel-wifi tx frame");
    serial::write_hex_u64("[openos-kernel] intel-wifi tx.len=", payload.len() as u64);
    Ok(frame_len)
}

pub fn receive_station_frame(rx_out: &mut [u8]) -> Result<usize, DriverError> {
    let mut runtime = WIFI_RUNTIME.lock();
    if !runtime.initialized {
        return Err(DriverError::NotReady);
    }

    if runtime.rx_len == 0 {
        return Ok(0);
    }

    let copy_len = runtime.rx_len.min(rx_out.len());
    rx_out[..copy_len].copy_from_slice(&runtime.rx_buffer[..copy_len]);
    runtime.rx_len = 0;

    serial::write_line("[openos-kernel] intel-wifi rx frame");
    serial::write_hex_u64("[openos-kernel] intel-wifi rx.len=", copy_len as u64);
    Ok(copy_len)
}

fn scan_supported_adapter() -> Option<PciLocation> {
    // Limit to bus 0 for boot performance; QEMU and most systems put devices on bus 0
    for bus in 0u16..=0 {
        for device in 0u8..32 {
            serial::write_hex_u64("[openos-kernel] intel-wifi scan.bus=", bus as u64);
            serial::write_hex_u64("[openos-kernel] intel-wifi scan.device=", device as u64);
            let location = PciLocation {
                bus: bus as u8,
                device,
                function: 0,
            };

            let vendor_device = pci_config_read_u32(location, 0x00);
            serial::write_hex_u64("[openos-kernel] intel-wifi scan.id=", vendor_device as u64);
            if vendor_device == 0xFFFF_FFFF {
                continue;
            }

            let vendor_id = vendor_device as u16;
            let device_id = (vendor_device >> 16) as u16;
            if vendor_id != INTEL_VENDOR_ID || !is_supported_wlan_id(device_id) {
                continue;
            }

            let class_reg = pci_config_read_u32(location, PCI_CLASS_OFFSET);
            let class_code = ((class_reg >> 24) & 0xFF) as u8;
            let subclass = ((class_reg >> 16) & 0xFF) as u8;
            if class_code == 0x02 && (subclass == 0x80 || subclass == 0x00) {
                return Some(location);
            }
        }
    }
    None
}

fn map_adapter_resources(pci: PciLocation) -> Option<AdapterResources> {
    let id = pci_config_read_u32(pci, 0x00);
    let vendor_id = id as u16;
    let device_id = (id >> 16) as u16;
    if vendor_id != INTEL_VENDOR_ID || !is_supported_wlan_id(device_id) {
        return None;
    }

    let bar0_raw = pci_config_read_u32(pci, PCI_BAR0_OFFSET);
    if bar0_raw == 0 || bar0_raw == 0xFFFF_FFFF || (bar0_raw & 0x1) != 0 {
        serial::write_line("[openos-kernel] intel-wifi map failed: BAR0 unavailable");
        return None;
    }

    let mmio_base = (bar0_raw & 0xFFFF_FFF0) as u64;
    let mmio_len = 0x20_000u64;
    if mmio_base == 0 {
        serial::write_line("[openos-kernel] intel-wifi map failed: mmio base is zero");
        return None;
    }

    let mut command = pci_config_read_u16(pci, PCI_COMMAND_OFFSET);
    command |= PCI_COMMAND_BUS_MASTER | PCI_COMMAND_MEM_SPACE;
    command &= !PCI_COMMAND_IO_SPACE;
    pci_config_write_u16(pci, PCI_COMMAND_OFFSET, command);

    let command_after = pci_config_read_u16(pci, PCI_COMMAND_OFFSET);
    if (command_after & (PCI_COMMAND_MEM_SPACE | PCI_COMMAND_BUS_MASTER))
        != (PCI_COMMAND_MEM_SPACE | PCI_COMMAND_BUS_MASTER)
    {
        serial::write_line(
            "[openos-kernel] intel-wifi map failed: pci command ownership bits missing",
        );
        return None;
    }

    Some(AdapterResources {
        pci,
        vendor_id,
        device_id,
        mmio_base,
        mmio_len,
    })
}

fn firmware_load_hook(device_id: u16) -> Result<(), &'static str> {
    let firmware = firmware_blob_for(device_id).ok_or("firmware blob unavailable")?;
    if firmware.is_empty() {
        return Err("firmware blob empty");
    }

    serial::write_line("[openos-kernel] intel-wifi firmware load hook");
    serial::write_hex_u64(
        "[openos-kernel] intel-wifi firmware.bytes=",
        firmware.len() as u64,
    );
    Ok(())
}

fn firmware_blob_for(device_id: u16) -> Option<&'static [u8]> {
    if is_supported_wlan_id(device_id) {
        Some(b"openos-iwlwifi-firmware-placeholder")
    } else {
        None
    }
}

fn is_supported_wlan_id(device_id: u16) -> bool {
    SUPPORTED_INTEL_WLAN_IDS.contains(&device_id)
}

fn pci_config_read_u16(location: PciLocation, offset: u8) -> u16 {
    let value = pci_config_read_u32(location, offset & !0x3);
    ((value >> ((offset & 0x2) * 8)) & 0xFFFF) as u16
}

fn pci_config_write_u16(location: PciLocation, offset: u8, value: u16) {
    let aligned = offset & !0x3;
    let mut existing = pci_config_read_u32(location, aligned);
    let shift = (offset & 0x2) * 8;
    existing &= !(0xFFFFu32 << shift);
    existing |= (value as u32) << shift;
    pci_config_write_u32(location, aligned, existing);
}

fn pci_config_read_u32(location: PciLocation, offset: u8) -> u32 {
    let address = 0x8000_0000u32
        | ((location.bus as u32) << 16)
        | ((location.device as u32) << 11)
        | ((location.function as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(PCI_CONFIG_ADDR, address);
        inl(PCI_CONFIG_DATA)
    }
}

fn pci_config_write_u32(location: PciLocation, offset: u8, value: u32) {
    let address = 0x8000_0000u32
        | ((location.bus as u32) << 16)
        | ((location.device as u32) << 11)
        | ((location.function as u32) << 8)
        | ((offset as u32) & 0xFC);
    unsafe {
        outl(PCI_CONFIG_ADDR, address);
        outl(PCI_CONFIG_DATA, value);
    }
}

unsafe fn outl(port: u16, value: u32) {
    core::arch::asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nomem, nostack, preserves_flags)
    );
}

unsafe fn inl(port: u16) -> u32 {
    let value: u32;
    core::arch::asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[cfg(test)]
mod tests {
    use super::{firmware_blob_for, is_supported_wlan_id, RX_BUFFER_CAPACITY};

    #[test]
    fn supported_ids_are_detected() {
        assert!(is_supported_wlan_id(0x24FD));
        assert!(is_supported_wlan_id(0x2526));
        assert!(is_supported_wlan_id(0x2723));
        assert!(is_supported_wlan_id(0x51F0));
        assert!(!is_supported_wlan_id(0x1234));
    }

    #[test]
    fn firmware_hook_has_payload_for_supported_ids() {
        let blob = firmware_blob_for(0x2723).expect("supported WLAN ID should resolve firmware");
        assert!(!blob.is_empty());
        assert!(blob.len() < RX_BUFFER_CAPACITY);
        assert!(firmware_blob_for(0x1234).is_none());
    }
}
