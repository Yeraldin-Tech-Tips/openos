mod ethernet_intel;
mod hid;
mod wifi_intel;

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
}

pub fn init() {
    let drivers: [&dyn KernelDriver; 3] = [
        &ethernet_intel::IntelEthernet,
        &wifi_intel::IntelWifi,
        &hid::HidStack,
    ];

    for driver in drivers {
        if driver.probe() {
            let _ = driver.init();
        }
    }
}

pub fn ethernet_transmit(payload: &[u8]) -> Result<usize, DriverError> {
    ethernet_intel::transmit(payload)
}
