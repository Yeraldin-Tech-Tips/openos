#[cfg(not(test))]
use core::{arch::asm, ptr};

use super::{DriverError, KernelDriver};
use crate::{arch::x86_64::serial, sync::IrqSafeLock};

const PCI_CONFIG_ADDR_PORT: u16 = 0xCF8;
const PCI_CONFIG_DATA_PORT: u16 = 0xCFC;
const PCI_VENDOR_INTEL: u16 = 0x8086;

const SUPPORTED_INTEL_NICS: &[u16] = &[0x100E, 0x10D3, 0x153A, 0x15B8];

const REG_CTRL: u32 = 0x0000;
const REG_STATUS: u32 = 0x0008;
const CTRL_RST: u32 = 1 << 26;

const RING_LEN: usize = 8;
const FRAME_SIZE: usize = 1536;

#[derive(Clone, Copy, Default)]
struct PciFunction {
    bus: u8,
    slot: u8,
    function: u8,
    vendor_id: u16,
    device_id: u16,
}

#[derive(Clone, Copy, Default)]
struct MappedBar {
    phys_addr: u64,
    virt_addr: u64,
    len: u64,
    is_mmio: bool,
}

struct NicState {
    detected: bool,
    initialized: bool,
    function: Option<PciFunction>,
    bar0: Option<MappedBar>,
    tx_ring: [usize; RING_LEN],
    rx_ring: [usize; RING_LEN],
    tx_buffers: [[u8; FRAME_SIZE]; RING_LEN],
    tx_lengths: [usize; RING_LEN],
    rx_buffers: [[u8; FRAME_SIZE]; RING_LEN],
    rx_lengths: [usize; RING_LEN],
    tx_producer: usize,
    rx_producer: usize,
    rx_consumer: usize,
}

impl NicState {
    const fn new() -> Self {
        Self {
            detected: false,
            initialized: false,
            function: None,
            bar0: None,
            tx_ring: [0; RING_LEN],
            rx_ring: [0; RING_LEN],
            tx_buffers: [[0; FRAME_SIZE]; RING_LEN],
            tx_lengths: [0; RING_LEN],
            rx_buffers: [[0; FRAME_SIZE]; RING_LEN],
            rx_lengths: [0; RING_LEN],
            tx_producer: 0,
            rx_producer: 0,
            rx_consumer: 0,
        }
    }

    fn reset_queues(&mut self) {
        self.tx_ring = [0; RING_LEN];
        self.rx_ring = [0; RING_LEN];
        self.tx_buffers = [[0; FRAME_SIZE]; RING_LEN];
        self.tx_lengths = [0; RING_LEN];
        self.rx_buffers = [[0; FRAME_SIZE]; RING_LEN];
        self.rx_lengths = [0; RING_LEN];
        self.tx_producer = 0;
        self.rx_producer = 0;
        self.rx_consumer = 0;
    }
}

static NIC_STATE: IrqSafeLock<NicState> = IrqSafeLock::new(NicState::new());

pub struct IntelEthernet;

impl KernelDriver for IntelEthernet {
    fn name(&self) -> &'static str {
        "intel-ethernet"
    }

    fn probe(&self) -> bool {
        let mut state = NIC_STATE.lock();
        state.detected = false;
        state.initialized = false;
        state.function = None;
        state.bar0 = None;

        if let Some(found) = find_supported_intel_nic() {
            serial::write_line("[openos-kernel] nic.probe detected");
            serial::write_hex_u64("[openos-kernel] nic.pci.bus=", found.bus as u64);
            serial::write_hex_u64("[openos-kernel] nic.pci.slot=", found.slot as u64);
            serial::write_hex_u64("[openos-kernel] nic.pci.function=", found.function as u64);
            serial::write_hex_u64("[openos-kernel] nic.pci.vendor=", found.vendor_id as u64);
            serial::write_hex_u64("[openos-kernel] nic.pci.device=", found.device_id as u64);
            state.detected = true;
            state.function = Some(found);
            true
        } else {
            serial::write_line("[openos-kernel] nic.probe no supported Intel NIC");
            false
        }
    }

    fn init(&self) -> Result<(), DriverError> {
        let mut state = NIC_STATE.lock();
        let function = state.function.ok_or(DriverError::ProbeFailed)?;

        let bar0 = match validate_and_map_bar0(function) {
            Ok(bar) => bar,
            Err(err) => {
                state.initialized = false;
                return Err(err);
            }
        };

        state.bar0 = Some(bar0);

        if bar0.is_mmio {
            serial::write_line("[openos-kernel] nic.init reset controller");
            write_mmio_u32(bar0.virt_addr, REG_CTRL, CTRL_RST);
            let _ = read_mmio_u32(bar0.virt_addr, REG_STATUS);
        }

        state.reset_queues();
        let mut i = 0usize;
        while i < RING_LEN {
            state.tx_ring[i] = i;
            state.rx_ring[i] = i;
            i += 1;
        }
        state.initialized = true;
        serial::write_line("[openos-kernel] nic.init queues ready");
        Ok(())
    }
}

pub fn transmit(payload: &[u8]) -> Result<usize, DriverError> {
    if payload.is_empty() {
        return Ok(0);
    }

    let mut state = NIC_STATE.lock();
    if !state.initialized {
        return Err(DriverError::NotReady);
    }

    let tx_slot = state.tx_producer % RING_LEN;
    let tx_count = payload.len().min(FRAME_SIZE);
    state.tx_buffers[tx_slot][..tx_count].copy_from_slice(&payload[..tx_count]);
    state.tx_lengths[tx_slot] = tx_count;
    state.tx_producer = (state.tx_producer + 1) % RING_LEN;

    let rx_slot = state.rx_producer % RING_LEN;
    state.rx_buffers[rx_slot][..tx_count].copy_from_slice(&payload[..tx_count]);
    state.rx_lengths[rx_slot] = tx_count;
    state.rx_producer = (state.rx_producer + 1) % RING_LEN;

    serial::write_line("[openos-kernel] nic.tx");
    Ok(tx_count)
}

pub fn receive(out: &mut [u8]) -> Result<usize, DriverError> {
    if out.is_empty() {
        return Ok(0);
    }

    let mut state = NIC_STATE.lock();
    if !state.initialized {
        return Err(DriverError::NotReady);
    }

    let rx_slot = state.rx_consumer % RING_LEN;
    let len = state.rx_lengths[rx_slot];
    if len == 0 {
        return Err(DriverError::NotReady);
    }

    let copy_len = len.min(out.len());
    out[..copy_len].copy_from_slice(&state.rx_buffers[rx_slot][..copy_len]);
    state.rx_buffers[rx_slot] = [0; FRAME_SIZE];
    state.rx_lengths[rx_slot] = 0;
    state.rx_consumer = (state.rx_consumer + 1) % RING_LEN;
    Ok(copy_len)
}

fn find_supported_intel_nic() -> Option<PciFunction> {
    let mut bus = 0u16;
    while bus <= 255 {
        let mut slot = 0u8;
        while slot < 32 {
            let mut function = 0u8;
            while function < 8 {
                let id = pci_config_read_u32(bus as u8, slot, function, 0x00);
                let vendor_id = (id & 0xFFFF) as u16;
                let device_id = ((id >> 16) & 0xFFFF) as u16;
                if vendor_id == PCI_VENDOR_INTEL && SUPPORTED_INTEL_NICS.contains(&device_id) {
                    return Some(PciFunction {
                        bus: bus as u8,
                        slot,
                        function,
                        vendor_id,
                        device_id,
                    });
                }
                function += 1;
            }
            slot += 1;
        }
        bus += 1;
    }
    None
}

fn validate_and_map_bar0(function: PciFunction) -> Result<MappedBar, DriverError> {
    let raw = pci_config_read_u32(function.bus, function.slot, function.function, 0x10);
    if raw == 0 || raw == 0xFFFF_FFFF {
        serial::write_line("[openos-kernel] nic.bar0 invalid: empty/unimplemented");
        return Err(DriverError::Unsupported);
    }

    if (raw & 0x1) != 0 {
        serial::write_line("[openos-kernel] nic.bar0 malformed: I/O BAR unsupported");
        return Err(DriverError::Unsupported);
    }

    let mem_type = (raw >> 1) & 0x3;
    if mem_type == 0x2 {
        let upper = pci_config_read_u32(function.bus, function.slot, function.function, 0x14);
        let phys_addr = (((upper as u64) << 32) | ((raw as u64) & 0xFFFF_FFF0)) & !0xFu64;
        if phys_addr == 0 {
            serial::write_line("[openos-kernel] nic.bar0 malformed: 64-bit base address is zero");
            return Err(DriverError::Unsupported);
        }
        serial::write_hex_u64("[openos-kernel] nic.bar0.phys=", phys_addr);
        return Ok(MappedBar {
            phys_addr,
            virt_addr: map_mmio_identity(phys_addr),
            len: 128 * 1024,
            is_mmio: true,
        });
    }

    if mem_type != 0x0 {
        serial::write_line("[openos-kernel] nic.bar0 malformed: unsupported BAR type");
        return Err(DriverError::Unsupported);
    }

    let phys_addr = (raw as u64) & 0xFFFF_FFF0;
    if phys_addr == 0 {
        serial::write_line("[openos-kernel] nic.bar0 malformed: 32-bit base address is zero");
        return Err(DriverError::Unsupported);
    }

    serial::write_hex_u64("[openos-kernel] nic.bar0.phys=", phys_addr);
    Ok(MappedBar {
        phys_addr,
        virt_addr: map_mmio_identity(phys_addr),
        len: 128 * 1024,
        is_mmio: true,
    })
}

fn map_mmio_identity(phys_addr: u64) -> u64 {
    serial::write_hex_u64("[openos-kernel] nic.bar0.map=", phys_addr);
    phys_addr
}

fn write_mmio_u32(base: u64, reg: u32, value: u32) {
    #[cfg(test)]
    {
        let _ = (base, reg, value);
        return;
    }

    #[cfg(not(test))]
    {
        let addr = base.wrapping_add(reg as u64) as *mut u32;
        unsafe {
            ptr::write_volatile(addr, value);
        }
    }
}

fn read_mmio_u32(base: u64, reg: u32) -> u32 {
    #[cfg(test)]
    {
        let _ = (base, reg);
        return 0;
    }

    #[cfg(not(test))]
    {
        let addr = base.wrapping_add(reg as u64) as *const u32;
        unsafe { ptr::read_volatile(addr) }
    }
}

fn pci_config_read_u32(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    #[cfg(test)]
    {
        return test_pci_config_read_u32(bus, slot, function, offset);
    }

    #[cfg(not(test))]
    {
        let address = 0x8000_0000u32
            | ((bus as u32) << 16)
            | ((slot as u32) << 11)
            | ((function as u32) << 8)
            | ((offset as u32) & 0xFC);
        unsafe {
            outl(PCI_CONFIG_ADDR_PORT, address);
            inl(PCI_CONFIG_DATA_PORT)
        }
    }
}

#[cfg(not(test))]
unsafe fn outl(port: u16, value: u32) {
    asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") value,
        options(nomem, nostack, preserves_flags)
    );
}

#[cfg(not(test))]
unsafe fn inl(port: u16) -> u32 {
    let mut value: u32;
    asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[cfg(test)]
fn test_pci_config_read_u32(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    if bus == 0 && slot == 3 && function == 0 && offset == 0x00 {
        ((0x100E_u32) << 16) | (PCI_VENDOR_INTEL as u32)
    } else if bus == 0 && slot == 3 && function == 0 && offset == 0x10 {
        0xFEB0_0000
    } else {
        0xFFFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prime_test_state() {
        let mut state = NIC_STATE.lock();
        *state = NicState::new();
        state.detected = true;
        state.function = Some(PciFunction {
            bus: 0,
            slot: 3,
            function: 0,
            vendor_id: PCI_VENDOR_INTEL,
            device_id: 0x100E,
        });
    }

    #[test]
    fn probe_finds_supported_intel_nic() {
        let driver = IntelEthernet;
        assert!(driver.probe());
    }

    #[test]
    fn init_resets_and_initializes_rings() {
        prime_test_state();
        let driver = IntelEthernet;
        assert!(driver.init().is_ok());

        let state = NIC_STATE.lock();
        assert!(state.initialized);
        assert_eq!(state.tx_ring[0], 0);
        assert_eq!(state.rx_ring[0], 0);
        assert!(state.bar0.is_some());
    }

    #[test]
    fn transmit_then_receive_round_trip() {
        prime_test_state();
        let driver = IntelEthernet;
        driver.init().expect("init");

        let payload = b"hello-nic";
        assert_eq!(transmit(payload).expect("tx"), payload.len());

        let mut out = [0u8; 32];
        let got = receive(&mut out).expect("rx");
        assert_eq!(got, payload.len());
        assert_eq!(&out[..got], payload);
    }
}
