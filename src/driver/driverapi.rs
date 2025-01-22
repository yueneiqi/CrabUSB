use core::future::Future;

use alloc::sync::Arc;
use async_lock::RwLock;

use crate::{
    abstractions::{PlatformAbstractions, USBSystemConfig},
    host::device::USBDevice,
};

pub trait USBSystemDriverModule<'a, O>: Send + Sync
where
    O: PlatformAbstractions,
{
    fn should_active(
        &self,
        independent_dev: &Arc<USBDevice<'a, O>>,
        config: &Arc<USBSystemConfig<O>>,
    ) -> Option<Arc<RwLock<dyn USBSystemDriverModuleInstanceFunctionalInterface<'a, O>>>>
    where
        [(); O::RING_BUFFER_SIZE]:;

    fn preload_module(&self);
}

pub trait USBSystemDriverModuleInstanceFunctionalInterface<'a, O>:
    Send + Sync + Future<Output = ()>
where
    O: PlatformAbstractions,
{
}
