use super::{DriverError, KernelDriver};

pub struct HidStack;

impl KernelDriver for HidStack {
    fn name(&self) -> &'static str {
        "hid-stack"
    }

    fn probe(&self) -> bool {
        // TODO(DRV-HID-001): Replace with USB/PS2 controller presence checks.
        // Tracking: docs/hardware/driver-roadmap.md#hid-stack-kernelsrcdrivershidrs
        false
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO(DRV-HID-003): Implement USB HID + PS/2 compatibility init path.
        // Tracking: docs/hardware/driver-roadmap.md#hid-stack-kernelsrcdrivershidrs
        Err(DriverError::NotReady)
    }
}
