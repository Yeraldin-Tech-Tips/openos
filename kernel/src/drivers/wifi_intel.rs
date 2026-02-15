use super::{DriverError, KernelDriver};

pub struct IntelWifi;

impl KernelDriver for IntelWifi {
    fn name(&self) -> &'static str {
        "intel-wifi"
    }

    fn probe(&self) -> bool {
        // TODO(DRV-WIFI-001): Replace with PCI scan for supported Intel WLAN IDs.
        // Tracking: docs/hardware/driver-roadmap.md#intel-wi-fi-kernelsrcdriverswifi_intelrs
        false
    }

    fn init(&self) -> Result<(), DriverError> {
        // TODO(DRV-WIFI-003): Implement firmware loading + WPA2 station mode init.
        // Tracking: docs/hardware/driver-roadmap.md#intel-wi-fi-kernelsrcdriverswifi_intelrs
        Err(DriverError::Unsupported)
    }
}
