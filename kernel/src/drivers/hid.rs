use super::{DriverError, KernelDriver};

pub struct HidStack;

impl KernelDriver for HidStack {
    fn name(&self) -> &'static str {
        "hid-stack"
    }

    fn probe(&self) -> bool {
        // TODO: Replace with USB/PS2 controller presence checks.
        false
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO: Implement USB HID + PS/2 compatibility layer.
        Err(DriverError::NotReady)
    }
}
