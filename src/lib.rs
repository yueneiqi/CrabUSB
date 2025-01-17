#![no_std]
#![feature(
    allocator_api,
    let_chains,
    exclusive_wrapper,
    async_closure,
    ptr_as_ref_unchecked,
    fn_traits
)]

use abstractions::{PlatformAbstractions, USBSystemConfig};
use alloc::sync::Arc;

extern crate alloc;

pub mod abstractions;
pub mod driver;
pub mod host;
pub mod usb;

pub struct USBSystem<O, const _DEVICE_REQUEST_BUFFER_SIZE: usize>
where
    O: PlatformAbstractions,
{
    config: Arc<USBSystemConfig<O, _DEVICE_REQUEST_BUFFER_SIZE>>,
}
