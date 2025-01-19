#![no_std]
#![feature(
    allocator_api,
    let_chains,
    exclusive_wrapper,
    async_closure,
    ptr_as_ref_unchecked,
    fn_traits,
    cfg_match
)]

use abstractions::{PlatformAbstractions, USBSystemConfig};
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use host::{controllers::Controller, device::USBDevice};

extern crate alloc;

pub mod abstractions;
pub mod driver;
pub mod host;
pub mod usb;

pub struct USBSystem<'a, O, const RINGBUFFER_SIZE: usize>
where
    O: PlatformAbstractions + 'static,
    'a: 'static,
{
    config: Arc<USBSystemConfig<O, RINGBUFFER_SIZE>>,
    controller: Box<dyn Controller<'a, O, RINGBUFFER_SIZE>>,
}

impl<'a, O, const RINGBUFFER_SIZE: usize> USBSystem<'a, O, RINGBUFFER_SIZE>
where
    O: PlatformAbstractions + 'static,
    'a: 'static,
{
    pub async fn new(config: USBSystemConfig<O, RINGBUFFER_SIZE>) -> Self {
        let config: Arc<USBSystemConfig<O, RINGBUFFER_SIZE>> = config.into();
        let controller: Box<dyn Controller<'a, O, RINGBUFFER_SIZE>> =
            host::controllers::initialize_controller(config.clone());

        USBSystem { config, controller }
    }
}
