use super::{DriverError, KernelDriver};
use crate::arch::x86_64::serial;

pub struct IntelEthernet;

impl KernelDriver for IntelEthernet {
    fn name(&self) -> &'static str {
        "intel-ethernet"
    }

    fn probe(&self) -> bool {
        // TODO: PCI scan for Intel Ethernet IDs.
        true
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO: e1000/e1000e-style init path.
        Ok(())
    }
}

pub fn transmit(payload: &[u8]) -> Result<usize, DriverError> {
    if payload.is_empty() {
        return Ok(0);
    }

    serial::write_line("[openos-kernel] nic.tx");
    serial::write_bytes(payload);
    serial::write_line("");
    Ok(payload.len())
}
