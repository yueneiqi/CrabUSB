use xhci::ring::trb::event::TransferEvent;

use crate::wait::WaitMapWeak;

pub type TransferRingWaitWeak = WaitMapWeak<TransferEvent>;
