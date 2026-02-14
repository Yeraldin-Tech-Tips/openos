use super::{DriverError, KernelDriver};

pub struct IntelWifi;

impl KernelDriver for IntelWifi {
    fn name(&self) -> &'static str {
        "intel-wifi"
    }

    fn probe(&self) -> bool {
        // TODO: PCI scan for supported Intel WLAN IDs.
        true
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO: Firmware loading and WPA2 station mode support.
        Ok(())
    }
}
