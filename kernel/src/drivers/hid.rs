#[cfg(not(test))]
use core::arch::asm;

use super::{DriverError, KernelDriver};
use crate::{arch::x86_64::serial, sync::IrqSafeLock};

const PCI_CONFIG_ADDR_PORT: u16 = 0xCF8;
const PCI_CONFIG_DATA_PORT: u16 = 0xCFC;

const PS2_DATA_PORT: u16 = 0x60;
const PS2_STATUS_PORT: u16 = 0x64;

const USB_CLASS_SERIAL_BUS: u8 = 0x0C;
const USB_SUBCLASS_USB: u8 = 0x03;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UsbHostKind {
    Uhci,
    Ohci,
    Ehci,
    Xhci,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct PciFunction {
    bus: u8,
    slot: u8,
    function: u8,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Ps2Controller {
    data_port: u16,
    status_port: u16,
    keyboard_irq: Option<u8>,
    mouse_irq: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct UsbHostController {
    function: PciFunction,
    kind: UsbHostKind,
    io_base: Option<u16>,
    mmio_base: Option<u64>,
    irq: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct HidState {
    detected: bool,
    ready: bool,
    ps2: Option<Ps2Controller>,
    usb: Option<UsbHostController>,
    keyboard_path_ready: bool,
    mouse_path_ready: bool,
}

impl HidState {
    const fn new() -> Self {
        Self {
            detected: false,
            ready: false,
            ps2: None,
            usb: None,
            keyboard_path_ready: false,
            mouse_path_ready: false,
        }
    }
}

static HID_STATE: IrqSafeLock<HidState> = IrqSafeLock::new(HidState::new());

pub struct HidStack;

impl KernelDriver for HidStack {
    fn name(&self) -> &'static str {
        "hid-stack"
    }

    fn probe(&self) -> bool {
        let mut state = HID_STATE.lock();
        *state = HidState::new();

        state.ps2 = detect_ps2_controller();
        state.usb = detect_usb_host_controller();

        if state.ps2.is_some() {
            serial::write_line("[openos-kernel] hid.probe ps2 detected");
        } else {
            serial::write_line("[openos-kernel] hid.probe ps2 unavailable");
        }

        if let Some(controller) = state.usb {
            serial::write_line("[openos-kernel] hid.probe usb-host detected");
            serial::write_line(usb_host_kind_name(controller.kind));
            serial::write_hex_u64(
                "[openos-kernel] hid.usb.pci.bus=",
                controller.function.bus as u64,
            );
            serial::write_hex_u64(
                "[openos-kernel] hid.usb.pci.slot=",
                controller.function.slot as u64,
            );
            serial::write_hex_u64(
                "[openos-kernel] hid.usb.pci.function=",
                controller.function.function as u64,
            );
        } else {
            serial::write_line("[openos-kernel] hid.probe usb-host unavailable");
        }

        state.detected = state.ps2.is_some() || state.usb.is_some();
        state.detected
    }

    fn init(&self) -> Result<(), DriverError> {
        let mut state = HID_STATE.lock();
        if !state.detected {
            serial::write_line("[openos-kernel] hid.init no controllers detected");
            return Err(DriverError::ProbeFailed);
        }

        state.ready = false;
        state.keyboard_path_ready = false;
        state.mouse_path_ready = false;

        if let Some(ps2) = state.ps2 {
            let ps2_has_ports = ps2.data_port != 0 && ps2.status_port != 0;
            let ps2_has_irqs = ps2.keyboard_irq.is_some() && ps2.mouse_irq.is_some();
            if ps2_has_ports && ps2_has_irqs {
                serial::write_hex_u64("[openos-kernel] hid.ps2.data_port=", ps2.data_port as u64);
                serial::write_hex_u64(
                    "[openos-kernel] hid.ps2.status_port=",
                    ps2.status_port as u64,
                );
                serial::write_hex_u64(
                    "[openos-kernel] hid.ps2.keyboard_irq=",
                    ps2.keyboard_irq.unwrap_or(0) as u64,
                );
                serial::write_hex_u64(
                    "[openos-kernel] hid.ps2.mouse_irq=",
                    ps2.mouse_irq.unwrap_or(0) as u64,
                );
                state.keyboard_path_ready = true;
                state.mouse_path_ready = true;
            } else {
                serial::write_line("[openos-kernel] hid.init ps2 resource unavailable");
            }
        }

        if let Some(usb) = state.usb {
            if usb.irq.is_none() {
                serial::write_line("[openos-kernel] hid.init usb irq unavailable");
            }
            if usb.io_base.is_none() && usb.mmio_base.is_none() {
                serial::write_line("[openos-kernel] hid.init usb host resource unavailable");
            } else {
                if let Some(io_base) = usb.io_base {
                    serial::write_hex_u64("[openos-kernel] hid.usb.io_base=", io_base as u64);
                }
                if let Some(mmio_base) = usb.mmio_base {
                    serial::write_hex_u64("[openos-kernel] hid.usb.mmio_base=", mmio_base);
                }
                if let Some(irq) = usb.irq {
                    serial::write_hex_u64("[openos-kernel] hid.usb.irq=", irq as u64);
                }
            }
        }

        state.ready =
            state.keyboard_path_ready || state.mouse_path_ready || usb_event_path_ready(state.usb);
        if state.ready {
            serial::write_line("[openos-kernel] hid.init ready");
            Ok(())
        } else {
            serial::write_line("[openos-kernel] hid.init not ready");
            Err(DriverError::NotReady)
        }
    }
}

pub fn on_ps2_scancode(byte: u8) {
    let state = HID_STATE.lock();
    if !state.ready || !state.keyboard_path_ready {
        return;
    }
    drop(state);
    crate::input::on_ps2_scancode(byte);
}

pub fn on_ps2_mouse_byte(byte: u8) {
    let state = HID_STATE.lock();
    if !state.ready || !state.mouse_path_ready {
        return;
    }
    drop(state);
    crate::input::on_ps2_mouse_byte(byte);
}

fn usb_event_path_ready(usb: Option<UsbHostController>) -> bool {
    let Some(usb) = usb else {
        return false;
    };
    (usb.io_base.is_some() || usb.mmio_base.is_some()) && usb.irq.is_some()
}

fn detect_ps2_controller() -> Option<Ps2Controller> {
    let status = read_port_u8(PS2_STATUS_PORT);
    if status == 0xFF {
        return None;
    }

    Some(Ps2Controller {
        data_port: PS2_DATA_PORT,
        status_port: PS2_STATUS_PORT,
        keyboard_irq: Some(1),
        mouse_irq: Some(12),
    })
}

fn detect_usb_host_controller() -> Option<UsbHostController> {
    // Limit to bus 0 for boot performance; QEMU and most systems put devices on bus 0
    let mut bus = 0u16;
    while bus <= 0 {
        let mut slot = 0u8;
        while slot < 32 {
            let mut function = 0u8;
            while function < 8 {
                let class = pci_config_read_u32(bus as u8, slot, function, 0x08);
                let base_class = ((class >> 24) & 0xFF) as u8;
                let subclass = ((class >> 16) & 0xFF) as u8;
                let prog_if = ((class >> 8) & 0xFF) as u8;

                if base_class == USB_CLASS_SERIAL_BUS && subclass == USB_SUBCLASS_USB {
                    let bar0 = pci_config_read_u32(bus as u8, slot, function, 0x10);
                    let (io_base, mmio_base) = decode_pci_bar(bar0);
                    let irq_line =
                        (pci_config_read_u32(bus as u8, slot, function, 0x3C) & 0xFF) as u8;
                    let irq = if irq_line == 0 || irq_line == 0xFF {
                        None
                    } else {
                        Some(irq_line)
                    };

                    return Some(UsbHostController {
                        function: PciFunction {
                            bus: bus as u8,
                            slot,
                            function,
                        },
                        kind: decode_usb_host_kind(prog_if),
                        io_base,
                        mmio_base,
                        irq,
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

fn decode_usb_host_kind(prog_if: u8) -> UsbHostKind {
    match prog_if {
        0x00 => UsbHostKind::Uhci,
        0x10 => UsbHostKind::Ohci,
        0x20 => UsbHostKind::Ehci,
        0x30 => UsbHostKind::Xhci,
        _ => UsbHostKind::Other,
    }
}

fn usb_host_kind_name(kind: UsbHostKind) -> &'static str {
    match kind {
        UsbHostKind::Uhci => "[openos-kernel] hid.usb.kind=uhci",
        UsbHostKind::Ohci => "[openos-kernel] hid.usb.kind=ohci",
        UsbHostKind::Ehci => "[openos-kernel] hid.usb.kind=ehci",
        UsbHostKind::Xhci => "[openos-kernel] hid.usb.kind=xhci",
        UsbHostKind::Other => "[openos-kernel] hid.usb.kind=other",
    }
}

fn decode_pci_bar(raw: u32) -> (Option<u16>, Option<u64>) {
    if raw == 0 || raw == 0xFFFF_FFFF {
        return (None, None);
    }
    if (raw & 0x1) != 0 {
        let io_base = (raw & 0xFFFC) as u16;
        if io_base == 0 {
            return (None, None);
        }
        return (Some(io_base), None);
    }

    let mmio_base = (raw & 0xFFFF_FFF0) as u64;
    if mmio_base == 0 {
        return (None, None);
    }
    (None, Some(mmio_base))
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

fn read_port_u8(port: u16) -> u8 {
    #[cfg(test)]
    {
        return test_read_port_u8(port);
    }

    #[cfg(not(test))]
    unsafe {
        inb(port)
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

#[cfg(not(test))]
unsafe fn inb(port: u16) -> u8 {
    let mut value: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") value,
        options(nomem, nostack, preserves_flags)
    );
    value
}

#[cfg(test)]
fn test_read_port_u8(port: u16) -> u8 {
    if port == PS2_STATUS_PORT {
        0x14
    } else {
        0xFF
    }
}

#[cfg(test)]
fn test_pci_config_read_u32(bus: u8, slot: u8, function: u8, offset: u8) -> u32 {
    if bus == 0 && slot == 2 && function == 0 {
        match offset {
            0x08 => {
                ((USB_CLASS_SERIAL_BUS as u32) << 24)
                    | ((USB_SUBCLASS_USB as u32) << 16)
                    | (0x30 << 8)
            }
            0x10 => 0xFEBF_0000,
            0x3C => 11,
            _ => 0,
        }
    } else {
        0xFFFF_FFFF
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_detects_ps2_and_usb() {
        let driver = HidStack;
        assert!(driver.probe());

        let state = HID_STATE.lock();
        assert!(state.detected);
        assert!(state.ps2.is_some());
        assert!(state.usb.is_some());
    }

    #[test]
    fn init_marks_hid_stack_ready() {
        let driver = HidStack;
        assert!(driver.probe());
        assert!(driver.init().is_ok());

        let state = HID_STATE.lock();
        assert!(state.ready);
        assert!(state.keyboard_path_ready);
        assert!(state.mouse_path_ready);
    }
}
