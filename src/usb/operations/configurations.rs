use crate::host::device::ConfigureSemaphore;

use super::{control::ControlTransfer, OptionalCallback, USBRequest};

pub type ConfigurationID = u16;
pub type InterfaceNumber = u16;
pub type AltnativeNumber = u16;

impl USBRequest {
    #[inline]
    pub fn switch_interface(
        i: InterfaceNumber,
        a: AltnativeNumber,
        sem: ConfigureSemaphore,
        callback: OptionalCallback,
    ) -> Self {
        Self {
            operation: super::RequestedOperation::Control(
                ControlTransfer::set_configuration(i, a),
                sem,
            ),
            callback,
        }
    }
}
