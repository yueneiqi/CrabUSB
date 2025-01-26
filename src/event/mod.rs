use alloc::{boxed::Box, collections::btree_map::BTreeMap, sync::Arc};
use async_lock::RwLock;
use squeak::Delegate;

use crate::{
    abstractions::PlatformAbstractions,
    driver::{self, driverapi::USBSystemDriverModuleInstanceFunctionalInterface},
    host::device::USBDevice,
};

pub struct EventBus<'a, O>
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    pub new_initialized_device: Delegate<'a, Arc<USBDevice<'a, O>>>,
    pub pre_drop_device: Delegate<'a, Arc<USBDevice<'a, O>>>,
    pub new_interface: Delegate<
        'a,
        (
            Box<dyn driver::driverapi::USBSystemDriverModule<'a, O>>,
            Arc<RwLock<dyn USBSystemDriverModuleInstanceFunctionalInterface<'a, O>>>,
        ),
    >,
}

unsafe impl<'a, O> Sync for EventBus<'a, O>
//safety: it wont mut
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
}

unsafe impl<'a, O> Send for EventBus<'a, O>
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
}

impl<'a, O> EventBus<'a, O>
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    pub fn new() -> Self {
        Self {
            new_initialized_device: Delegate::new(),
            pre_drop_device: Delegate::new(),
            new_interface: Delegate::new(),
        }
    }
}
