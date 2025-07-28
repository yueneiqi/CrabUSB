use alloc::sync::Arc;

use crate::xhci::event::RingWait;

use super::Ring;

pub struct TransferRing {
    ring: Ring,
    wait: RingWait<>,
}
