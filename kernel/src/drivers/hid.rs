use super::{DriverError, KernelDriver};

pub struct HidStack;

impl KernelDriver for HidStack {
    fn name(&self) -> &'static str {
        "hid-stack"
    }

    fn probe(&self) -> bool {
        true
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO: USB HID + PS/2 compatibility layer.
        Ok(())
    }
}
