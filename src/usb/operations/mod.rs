pub mod configurations;
pub mod isoch;
use core::{fmt::Debug, future::Future};

use alloc::sync::Arc;
use async_lock::SemaphoreGuardArc;
use bulk::BulkTransfer;
use control::ControlTransfer;
use interrupt::InterruptTransfer;
use isoch::IsochTransfer;
use num_derive::FromPrimitive;
use usb_descriptor_decoder::descriptors::desc_configuration::Configuration;

use crate::host::device::ConfigureSemaphore;

pub mod bulk;
pub mod control;
pub mod interrupt;

type OptionalCallback = Option<Arc<dyn Fn(Result<(), OperationErrors>)>>;

#[derive(Default)]
pub struct USBRequest {
    pub operation: RequestedOperation,
    pub callback: OptionalCallback,
}

impl USBRequest {
    pub fn is_control(&self) -> bool {
        match self.operation {
            RequestedOperation::Control(_, _) => true,
            _ => false,
        }
    }
}

impl Debug for USBRequest {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("USBRequest")
            .field("operation", &self.operation)
            .finish()
    }
}

#[derive(Debug, Default)]
pub enum RequestedOperation {
    Control(ControlTransfer, ConfigureSemaphore),
    Bulk(BulkTransfer),
    Interrupt(InterruptTransfer),
    Isoch(IsochTransfer),
    Assign(ConfigureSemaphore),
    #[default]
    NOOP,
}

#[derive(Clone, Debug)]
pub enum OperationErrors {}

/// The direction of the data transfer.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, FromPrimitive)]
pub enum Direction {
    /// Out (Write Data)
    Out = 0,
    /// In (Read Data)
    In = 1,
}
