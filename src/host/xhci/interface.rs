use alloc::collections::btree_map::BTreeMap;

use crate::{
    standard::descriptors::InterfaceDescriptor,
    xhci::{def::Dci, endpoint::EndpointRaw},
};

pub struct Interface {
    desc: InterfaceDescriptor,
    ep_map: BTreeMap<Dci, EndpointRaw>,
}

impl Interface {
    pub(crate) fn new(desc: InterfaceDescriptor, ep_map: BTreeMap<Dci, EndpointRaw>) -> Self {
        Self { desc, ep_map }
    }

}
