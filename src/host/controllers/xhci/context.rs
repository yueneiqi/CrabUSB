use core::usize;

use crate::abstractions::dma::DMA;
use crate::abstractions::{PlatformAbstractions, USBSystemConfig};

use alloc::collections::BTreeMap;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use async_lock::RwLock;
use xhci::context::Device64Byte;
use xhci::context::{Device, Input64Byte};
use xhci::ring::trb::transfer;

use super::ring::Ring;
const NUM_EPS: usize = 32;

pub struct DeviceContextList<O, const _DEVICE_REQUEST_BUFFER_SIZE: usize>
where
    O: PlatformAbstractions,
{
    config: Arc<USBSystemConfig<O, _DEVICE_REQUEST_BUFFER_SIZE>>,
    pub dcbaa: DMA<[u64; 256], O::DMA>,
    pub device_ctx_inners: BTreeMap<usize, RwLock<DeviceCtxInner<O>>>,
}

pub struct DeviceCtxInner<O>
where
    O: PlatformAbstractions,
{
    pub out_ctx: DMA<Device64Byte, O::DMA>,
    pub in_ctx: DMA<Input64Byte, O::DMA>,
    pub transfer_rings: Vec<Ring<O>>,
}

impl<O, const _DEVICE_REQUEST_BUFFER_SIZE: usize> DeviceContextList<O, _DEVICE_REQUEST_BUFFER_SIZE>
where
    O: PlatformAbstractions,
{
    pub fn new(cfg: Arc<USBSystemConfig<O, _DEVICE_REQUEST_BUFFER_SIZE>>) -> Self {
        Self {
            config: cfg.clone(),
            dcbaa: DMA::new([0u64; 256], 4096, cfg.os.dma_alloc()),
            device_ctx_inners: BTreeMap::new(),
        }
    }

    pub fn dcbaap(&self) -> usize {
        self.dcbaa.as_ptr() as _
    }

    pub fn new_slot(
        &mut self,
        slot: usize,
        num_ep: usize, // cannot lesser than 0, and consider about alignment, use usize
    ) {
        let os = &self.config.os;

        self.device_ctx_inners.insert(
            slot,
            RwLock::new(DeviceCtxInner {
                out_ctx: { DMA::new(Device::new_64byte(), 4096, os.dma_alloc()) },
                in_ctx: { DMA::new(Input64Byte::new_64byte(), 4096, os.dma_alloc()) },
                transfer_rings: {
                    (0..num_ep)
                        .into_iter()
                        .map(|_| Ring::new(os.clone(), 32, true))
                        .map(Self::prepare_transfer_ring)
                        .collect()
                },
            }),
        );
    }

    fn prepare_transfer_ring(mut r: Ring<O>) -> Ring<O> {
        //in our code, the init state of transfer ring always has ccs = 0, so we use ccs =1 to fill transfer ring
        let mut normal = transfer::Normal::default();
        normal.set_cycle_bit();
        r.enque_trbs(vec![normal.into_raw(); r.len() - 1]); //the n'th is link trb
        r
    }
}

use tock_registers::interfaces::Writeable;
use tock_registers::register_structs;
use tock_registers::registers::ReadWrite;

register_structs! {
    pub ScratchpadBufferEntry{
        (0x000 => value_low: ReadWrite<u32>),
        (0x004 => value_high: ReadWrite<u32>),
        (0x008 => @END),
    }
}

impl ScratchpadBufferEntry {
    pub fn set_addr(&mut self, addr: u64) {
        self.value_low.set(addr as u32);
        self.value_high.set((addr >> 32) as u32);
    }
}

pub struct ScratchpadBufferArray<O>
where
    O: PlatformAbstractions,
{
    pub entries: DMA<[ScratchpadBufferEntry], O::DMA>,
    pub pages: Vec<DMA<[u8], O::DMA>>,
}

unsafe impl<O: PlatformAbstractions> Sync for ScratchpadBufferArray<O> {}

impl<O> ScratchpadBufferArray<O>
where
    O: PlatformAbstractions,
{
    pub fn new(entries: u32, os: O) -> Self {
        let page_size = O::PAGE_SIZE;
        let align = 64;

        let mut entries: DMA<[ScratchpadBufferEntry], O::DMA> =
            DMA::zeroed(entries as usize, align, os.dma_alloc());

        let pages = entries
            .iter_mut()
            .map(|entry| {
                let dma = DMA::zeroed(page_size, align, os.dma_alloc());

                assert_eq!(dma.addr() % page_size, 0);
                entry.set_addr(dma.addr() as u64);
                dma
            })
            .collect();

        Self { entries, pages }
    }
    pub fn register(&self) -> usize {
        self.entries.addr()
    }
}
