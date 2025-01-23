use alloc::sync::Arc;
use futures::channel::oneshot::{Receiver, Sender};
use xhci::ring::trb::{
    command,
    event::{self, CommandCompletion},
};

use crate::usb::operations::{
    bulk::BulkTransfer, control::ControlTransfer, interrupt::InterruptTransfer,
    isoch::IsochTransfer, CompleteAction, ExtraAction, RequestedOperation, USBRequest,
};

pub type XHCICommandCallbackValue = Sender<CommandCompletion>;

pub enum XHCICompleteAction {
    CommandCallback(XHCICommandCallbackValue),
    STANDARD(CompleteAction),
}

impl From<CompleteAction> for XHCICompleteAction {
    fn from(value: CompleteAction) -> Self {
        Self::STANDARD(value)
    }
}
