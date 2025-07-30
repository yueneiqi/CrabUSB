use alloc::sync::Arc;
use dma_api::Impl;
use mbarrier::{mb, wmb};
use spin::Mutex;
use xhci::{
    registers::doorbell,
    ring::trb::{
        command,
        event::{CommandCompletion, CompletionCode},
    },
};

use crate::{
    err::*,
    wait::WaitMap,
    xhci::{
        reg::XhciRegisters,
        ring::{Ring, TrbData},
    },
};

pub struct CommandRing {
    inner: Arc<Mutex<Inner>>,
}

impl Clone for CommandRing {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
        }
    }
}

struct Inner {
    reg: XhciRegisters,
    ring: Ring,
    wait: WaitMap<CommandCompletion>,
}

unsafe impl Send for Inner {}

impl CommandRing {
    pub fn new(reg: XhciRegisters) -> Result<Self> {
        let ring = Ring::new(true, dma_api::Direction::Bidirectional)?;
        let wait = WaitMap::new(ring.trb_bus_addr_list().map(|a| a.raw()));

        Ok(Self {
            inner: Arc::new(Mutex::new(Inner { reg, ring, wait })),
        })
    }

    fn post_raw(&mut self, trb: command::Allowed) -> impl Future<Output = CommandCompletion> {
        let mut inner = self.inner.lock();
        let g = inner.reg.disable_irq_guard();
        let trb_addr = inner.ring.enque_command(trb);
        mb();
        inner
            .reg
            .doorbell
            .write_volatile_at(0, doorbell::Register::default());

        let f = inner.wait.try_wait_for_result(trb_addr.raw()).unwrap();
        drop(g);
        f
    }

    pub async fn post(&mut self, trb: command::Allowed) -> Result<CommandCompletion> {
        let ret = self.post_raw(trb).await;

        match ret.completion_code() {
            Ok(code) => {
                if matches!(code, CompletionCode::Success) {
                    Ok(ret)
                } else {
                    Err(USBError::TransferEventError(code))
                }
            }
            Err(_e) => Err(USBError::Unknown),
        }
    }
}
