use alloc::sync::Weak;
use xhci::ring::trb::event::TransferEvent;

use crate::{
    wait::WaitMap,
    xhci::{event::RingWait, ring::UnsafeShare},
};

use super::Ring;

pub struct TransferRing {
    ring: Ring,
    wait: WaitMap<TransferEvent>,
}

pub type WeakTransferRing = Weak<UnsafeShare<TransferRing>>;
