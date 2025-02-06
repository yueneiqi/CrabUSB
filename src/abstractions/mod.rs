use core::{alloc::Allocator, task::Waker};

use alloc::sync::Arc;
use async_lock::Semaphore;

pub mod dma;

pub trait PlatformAbstractions: Clone + Send + Sync + Sized {
    type VirtAddr: Into<Self::PhysAddr> + From<usize> + Into<usize> + Clone + Send + Sync;
    type PhysAddr: Into<Self::VirtAddr> + From<usize> + Into<usize> + Clone + Send + Sync;
    type DMA: Allocator + Send + Sync + Clone;
    const PAGE_SIZE: usize;
    const RING_BUFFER_SIZE: usize;
    fn dma_alloc(&self) -> Self::DMA;
}

pub type InterruptRegister = dyn Fn(&dyn Fn()) + Send + Sync;

#[derive(Clone)]
pub struct USBSystemConfig<O, const RING_BUFFER_SIZE: usize>
where
    O: PlatformAbstractions,
{
    pub base_addr: O::VirtAddr,
    pub wake_method: WakeMethod,
    pub os: O,
}

#[derive(Clone)]
pub enum WakeMethod {
    Interrupt(Arc<InterruptRegister>),
    ///remember: increase permit on event. consumer side would drop every permit but not return it!
    Timer(Arc<Semaphore>),
    Yield,
}
