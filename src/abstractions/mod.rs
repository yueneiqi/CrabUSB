use core::alloc::Allocator;

pub mod dma;

pub trait PlatformAbstractions: Clone + Send + Sync + Sized {
    type VirtAddr: Into<Self::PhysAddr> + From<usize> + Into<usize> + Clone + Send + Sync;
    type PhysAddr: Into<Self::VirtAddr> + From<usize> + Into<usize> + Clone + Send + Sync;
    type DMA: Allocator + Send + Sync + Clone;
    const PAGE_SIZE: usize;
    fn dma_alloc(&self) -> Self::DMA;
}

#[derive(Clone, Debug)]
pub struct USBSystemConfig<O>
where
    O: PlatformAbstractions,
{
    pub base_addr: O::VirtAddr,
    pub irq_num: u32,
    pub irq_priority: u32,
    pub device_request_buffer_size: usize,
    pub os: O,
}
