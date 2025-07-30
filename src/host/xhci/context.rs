use core::cell::UnsafeCell;

use alloc::{sync::Arc, vec::Vec};
use dma_api::{DBox, DVec};
use xhci::context::{Device32Byte, Input32Byte};

use super::ring::Ring;
use crate::{err::*, xhci::SlotId};

pub struct DeviceContextList {
    pub dcbaa: DVec<u64>,
    pub ctx_list: Vec<Option<Arc<DeviceContext>>>,
    max_slots: usize,
}

pub struct ContextData {
    pub out: DBox<Device32Byte>,
    pub input: DBox<Input32Byte>,
    pub transfer_rings: Vec<Ring>,
}

pub struct DeviceContext {
    pub data: UnsafeCell<ContextData>,
}

unsafe impl Send for DeviceContext {}
unsafe impl Sync for DeviceContext {}

impl ContextData {}

impl DeviceContext {
    fn new() -> Result<Self> {
        let out =
            DBox::zero_with_align(dma_api::Direction::FromDevice, 64).ok_or(USBError::NoMemory)?;
        let input =
            DBox::zero_with_align(dma_api::Direction::ToDevice, 64).ok_or(USBError::NoMemory)?;
        Ok(Self {
            data: UnsafeCell::new(ContextData {
                out,
                input,
                transfer_rings: Vec::new(),
            }),
        })
    }

    pub fn ctrl_ring(&self) -> &Ring {
        unsafe {
            let data = &*self.data.get();
            &data.transfer_rings[0]
        }
    }
}

impl DeviceContextList {
    pub fn new(max_slots: usize) -> Result<Self> {
        let dcbaa =
            DVec::zeros(256, 0x1000, dma_api::Direction::FromDevice).ok_or(USBError::NoMemory)?;

        Ok(Self {
            dcbaa,
            ctx_list: alloc::vec![ None; max_slots],
            max_slots,
        })
    }

    pub fn new_ctx(&mut self, slot_id: SlotId) -> Result<Arc<DeviceContext>> {
        if slot_id.as_usize() > self.max_slots {
            Err(USBError::SlotLimitReached)?;
        }

        let ctx = Arc::new(DeviceContext::new()?);

        let ctx_mut = unsafe { &mut *ctx.data.get() };

        self.dcbaa.set(slot_id.as_usize(), ctx_mut.out.bus_addr());

        // With control transfer, we need at least one transfer ring
        ctx_mut.transfer_rings = alloc::vec![Ring::new(true, dma_api::Direction::Bidirectional)?];

        self.ctx_list[slot_id.as_usize()] = Some(ctx.clone());

        Ok(ctx)
    }
}

pub struct ScratchpadBufferArray {
    pub entries: DVec<u64>,
    pub _pages: Vec<DVec<u8>>,
}

impl ScratchpadBufferArray {
    pub fn new(entries: usize) -> Result<Self> {
        let entries = DVec::zeros(entries, 64, dma_api::Direction::Bidirectional)
            .ok_or(USBError::NoMemory)?;

        let pages = (0..entries.len())
            .map(|_| {
                DVec::<u8>::zeros(0x1000, 0x1000, dma_api::Direction::Bidirectional)
                    .ok_or(USBError::NoMemory)
            })
            .try_collect()?;

        Ok(Self {
            entries,
            _pages: pages,
        })
    }

    pub fn bus_addr(&self) -> u64 {
        self.entries.bus_addr()
    }
}
