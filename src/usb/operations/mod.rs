pub mod configurations;
pub mod isoch;
use core::fmt::Debug;

use alloc::sync::Arc;
use async_lock::{
    Mutex, MutexGuardArc, OnceCell,
};
use bulk::BulkTransfer;
use control::ControlTransfer;
use interrupt::InterruptTransfer;
use isoch::IsochTransfer;
use num_derive::FromPrimitive;

use crate::host::device::ConfigureSemaphore;

pub mod bulk;
pub mod control;
pub mod interrupt;

#[derive(Default)]
pub struct USBRequest {
    pub extra_action: ExtraAction,
    pub operation: RequestedOperation,
    pub complete_action: CompleteAction,
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

pub type ChannelNumber = u16;

#[derive(Debug, Default)]
pub enum ExtraAction {
    #[default]
    NOOP,
    KeepFill(ChannelNumber),
}

pub type CallbackFn = Arc<dyn FnOnce(RequestResult) + Send + Sync>;
pub type CallbackValueSetter = MutexGuardArc<OnceCell<RequestResult>>;
pub struct CallbackValue(Arc<Mutex<OnceCell<RequestResult>>>);

impl CallbackValue {
    pub fn new() -> (CallbackValue, CallbackValueSetter) {
        let mutex = Arc::new(Mutex::new(OnceCell::new()));
        let lock_arc = mutex.try_lock_arc().unwrap();
        (CallbackValue(mutex), lock_arc)
    }

    pub async fn get(self) -> RequestResult {
        self.0
            .lock()
            .await
            .take()
            .expect("value not been set! check your code")
    }
}

#[derive(Default)]
pub enum CompleteAction {
    #[default]
    NOOP,
    Callback(CallbackFn),
    SimpleResponse(MutexGuardArc<OnceCell<RequestResult>>),
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
pub enum RequestResult {
    Success,
}

/// The direction of the data transfer.
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq, Hash, FromPrimitive)]
pub enum Direction {
    /// Out (Write Data)
    Out = 0,
    /// In (Read Data)
    In = 1,
}
