use xhci::ring::trb::event::TransferEvent;

use crate::wait::{WaitMap, WaitMapWeak};

use super::Ring;

pub struct TransferRing {
    ring: Ring,
    wait: WaitMap<TransferEvent>,
}

pub type TransferRingWaitWeak = WaitMapWeak<TransferEvent>;
