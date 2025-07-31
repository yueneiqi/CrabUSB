use core::cell::UnsafeCell;

use alloc::{collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use dma_api::{DBox, DVec};
use xhci::context::{Device32Byte, Device64Byte, Input32Byte, Input64Byte, InputHandler};

use super::ring::Ring;
use crate::{
    err::*,
    xhci::{SlotId, def::Dci},
};

pub struct DeviceContextList {
    pub dcbaa: DVec<u64>,
    pub ctx_list: Vec<Option<Arc<DeviceContext>>>,
    max_slots: usize,
}

struct Context32 {
    out: DBox<Device32Byte>,
    input: DBox<Input32Byte>,
}

struct Context64 {
    out: DBox<Device64Byte>,
    input: DBox<Input64Byte>,
}

pub struct ContextData {
    ctx64: Option<Context64>,
    ctx32: Option<Context32>,
    pub transfer_rings: BTreeMap<Dci, Ring>,
}

pub struct DeviceContext {
    pub data: UnsafeCell<ContextData>,
}

unsafe impl Send for DeviceContext {}
unsafe impl Sync for DeviceContext {}

impl ContextData {
    pub fn dcbaa(&self) -> u64 {
        if let Some(ctx64) = &self.ctx64 {
            ctx64.out.bus_addr()
        } else if let Some(ctx32) = &self.ctx32 {
            ctx32.out.bus_addr()
        } else {
            panic!("No context available");
        }
    }

    pub fn with_empty_input<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn InputHandler),
    {
        if let Some(ctx64) = &mut self.ctx64 {
            let mut input = Input64Byte::new_64byte();
            f(&mut input);
            ctx64.input.write(input);
        } else if let Some(ctx32) = &mut self.ctx32 {
            let mut input = Input32Byte::new_32byte();
            f(&mut input);
            ctx32.input.write(input);
        } else {
            panic!("No context available");
        }
    }

    pub fn with_input<F>(&mut self, f: F)
    where
        F: FnOnce(&mut dyn InputHandler),
    {
        if let Some(ctx64) = &mut self.ctx64 {
            let mut input = ctx64.input.read();
            f(&mut input);
            ctx64.input.write(input);
        } else if let Some(ctx32) = &mut self.ctx32 {
            let mut input = ctx32.input.read();
            f(&mut input);
            ctx32.input.write(input);
        } else {
            panic!("No context available");
        }
    }

    pub fn input_bus_addr(&self) -> u64 {
        if let Some(ctx64) = &self.ctx64 {
            ctx64.input.bus_addr()
        } else if let Some(ctx32) = &self.ctx32 {
            ctx32.input.bus_addr()
        } else {
            panic!("No context available");
        }
    }
}

impl DeviceContext {
    fn new(is_64: bool) -> Result<Self> {
        let ctx64;
        let ctx32;
        if is_64 {
            ctx64 = Some(Context64 {
                out: DBox::zero_with_align(dma_api::Direction::FromDevice, 64)
                    .ok_or(USBError::NoMemory)?,
                input: DBox::zero_with_align(dma_api::Direction::ToDevice, 64)
                    .ok_or(USBError::NoMemory)?,
            });
            ctx32 = None;
        } else {
            ctx32 = Some(Context32 {
                out: DBox::zero_with_align(dma_api::Direction::FromDevice, 64)
                    .ok_or(USBError::NoMemory)?,
                input: DBox::zero_with_align(dma_api::Direction::ToDevice, 64)
                    .ok_or(USBError::NoMemory)?,
            });
            ctx64 = None;
        }

        Ok(Self {
            data: UnsafeCell::new(ContextData {
                ctx64,
                ctx32,
                transfer_rings: Default::default(),
            }),
        })
    }

    pub fn ctrl_ring(&self) -> &Ring {
        unsafe {
            let data = &*self.data.get();
            &data.transfer_rings[&Dci::CTRL]
        }
    }

    pub fn input_bus_addr(&self) -> u64 {
        unsafe {
            let data = &*self.data.get();
            data.input_bus_addr()
        }
    }

    pub fn new_ring(&self, dci: Dci) -> Result<&Ring> {
        let ring = Ring::new(true, dma_api::Direction::Bidirectional)?;
        unsafe {
            let data = &mut *self.data.get();
            data.transfer_rings.insert(dci, ring);
            Ok(&data.transfer_rings[&dci])
        }
    }
}

impl DeviceContextList {
    pub fn new(max_slots: usize) -> Result<Self> {
        let dcbaa =
            DVec::zeros(256, 0x1000, dma_api::Direction::ToDevice).ok_or(USBError::NoMemory)?;

        Ok(Self {
            dcbaa,
            ctx_list: alloc::vec![ None; max_slots],
            max_slots,
        })
    }

    pub fn new_ctx(&mut self, slot_id: SlotId, is_64: bool) -> Result<Arc<DeviceContext>> {
        if slot_id.as_usize() > self.max_slots {
            Err(USBError::SlotLimitReached)?;
        }

        let ctx = Arc::new(DeviceContext::new(is_64)?);

        let ctx_mut = unsafe { &mut *ctx.data.get() };

        self.dcbaa.set(slot_id.as_usize(), ctx_mut.dcbaa());

        // With control transfer, we need at least one transfer ring
        ctx_mut.transfer_rings.insert(
            Dci::CTRL,
            Ring::new(true, dma_api::Direction::Bidirectional)?,
        );

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
        let mut entries_vec = DVec::zeros(entries, 64, dma_api::Direction::Bidirectional)
            .ok_or(USBError::NoMemory)?;

        let pages: Vec<DVec<u8>> = (0..entries_vec.len())
            .map(|_| {
                DVec::<u8>::zeros(0x1000, 0x1000, dma_api::Direction::Bidirectional)
                    .ok_or(USBError::NoMemory)
            })
            .try_collect()?;

        // 将每个页面的地址写入到 entries 数组中
        for (i, page) in pages.iter().enumerate() {
            entries_vec.set(i, page.bus_addr());
        }

        Ok(Self {
            entries: entries_vec,
            _pages: pages,
        })
    }

    pub fn bus_addr(&self) -> u64 {
        self.entries.bus_addr()
    }
}
