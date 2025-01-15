use crate::host::device::ConfigureSemaphore;

use super::{control::ControlTransfer, RequestedOperation, USBRequest};

pub type ConfigurationID = u16;
pub type InterfaceNumber = u16;
pub type AltnativeNumber = u16;
