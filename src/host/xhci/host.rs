use alloc::collections::btree_map::BTreeMap;

use crate::xhci::XhciRegisters;

pub struct HostData {
    pub reg: XhciRegisters,
}
