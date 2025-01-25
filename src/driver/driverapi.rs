use core::future::Future;

use alloc::boxed::Box;
use alloc::{string::String, sync::Arc};
use async_lock::RwLock;
use async_trait::async_trait;
use embassy_futures::select;
use futures::task::FutureObj;

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
        device: &Arc<USBDevice<'a, O>>,
        config: &Arc<USBSystemConfig<O>>,
    ) -> Option<Arc<RwLock<dyn USBSystemDriverModuleInstanceFunctionalInterface<'a, O>>>>
    where
        [(); O::RING_BUFFER_SIZE]:;

    fn preload_module(&self);

    fn name(&self) -> &'a str;
}

#[async_trait]
pub trait USBSystemDriverModuleInstanceFunctionalInterface<'a, O>: Send + Sync
where
    O: PlatformAbstractions,
{
    async fn run(&mut self);
}
