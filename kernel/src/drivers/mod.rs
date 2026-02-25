//! Kernel driver stack status.
//!
//! Current in-tree drivers are scaffolding only and intentionally conservative:
//! probes return `false` until hardware detection is implemented.
//! `init()` methods return structured `DriverError` values to make readiness and
//! unsupported states explicit in early boot logs.

mod ethernet_intel;
mod hid;
mod wifi_intel;

use crate::arch::x86_64::serial;

pub trait KernelDriver {
    fn name(&self) -> &'static str;
    fn probe(&self) -> bool;
    fn init(&self) -> Result<(), DriverError>;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverError {
    ProbeFailed,
    InitFailed,
    Unsupported,
    NotReady,
}

pub fn init() {
    let drivers: [&dyn KernelDriver; 3] = [
        &ethernet_intel::IntelEthernet,
        &wifi_intel::IntelWifi,
        &hid::HidStack,
    ];

    for driver in drivers {
        let detected = driver.probe();
        if detected {
            serial::write_line("[openos-kernel] driver.probe ok");
            serial::write_line(driver.name());
            match driver.init() {
                Ok(()) => {
                    serial::write_line("[openos-kernel] driver.init ok");
                    serial::write_line(driver.name());
                }
                Err(error) => {
                    serial::write_line("[openos-kernel] driver.init failed");
                    serial::write_line(driver.name());
                    serial::write_line(match error {
                        DriverError::ProbeFailed => "ProbeFailed",
                        DriverError::InitFailed => "InitFailed",
                        DriverError::Unsupported => "Unsupported",
                        DriverError::NotReady => "NotReady",
                    });
                }
            }
        } else {
            serial::write_line("[openos-kernel] driver.probe failed");
            serial::write_line(driver.name());
        }
    }
}

pub fn ethernet_transmit(payload: &[u8]) -> Result<usize, DriverError> {
    ethernet_intel::transmit(payload)
}

pub fn wifi_transmit(payload: &[u8]) -> Result<usize, DriverError> {
    wifi_intel::transmit_station_frame(payload)
}

pub fn wifi_receive(rx_out: &mut [u8]) -> Result<usize, DriverError> {
    wifi_intel::receive_station_frame(rx_out)
}

pub fn ethernet_receive(out: &mut [u8]) -> Result<usize, DriverError> {
    ethernet_intel::receive(out)
}

pub fn hid_on_ps2_scancode(byte: u8) {
    hid::on_ps2_scancode(byte);
}

pub fn hid_on_ps2_mouse_byte(byte: u8) {
    hid::on_ps2_mouse_byte(byte);
}
