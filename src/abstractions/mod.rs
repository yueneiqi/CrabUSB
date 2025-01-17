use core::alloc::Allocator;

use alloc::sync::Arc;

pub mod dma;

pub trait PlatformAbstractions: Clone + Send + Sync + Sized {
    type VirtAddr: Into<Self::PhysAddr> + From<usize> + Into<usize> + Clone + Send + Sync;
    type PhysAddr: Into<Self::VirtAddr> + From<usize> + Into<usize> + Clone + Send + Sync;
    type DMA: Allocator + Send + Sync + Clone;
    const PAGE_SIZE: usize;
    fn dma_alloc(&self) -> Self::DMA;
}

pub type InterruptRegister = dyn Fn(dyn Fn()) + Send + Sync;

#[derive(Clone)]
pub struct USBSystemConfig<O, const DEVICE_REQUEST_BUFFER_SIZE: usize>
where
    O: PlatformAbstractions,
{
    pub base_addr: O::VirtAddr,
    pub interrupt_register: Option<Arc<InterruptRegister>>,
    pub os: O,
}
