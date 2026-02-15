use super::{DriverError, KernelDriver};

pub struct IntelWifi;

impl KernelDriver for IntelWifi {
    fn name(&self) -> &'static str {
        "intel-wifi"
    }

    fn probe(&self) -> bool {
        // TODO: Replace with PCI scan for supported Intel WLAN IDs.
        false
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO: Implement firmware loading and WPA2 station mode support.
        Err(DriverError::Unsupported)
    }
}
