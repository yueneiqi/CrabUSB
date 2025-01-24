use core::future::Future;
use core::sync::atomic::{fence, Ordering};
use core::task::{Poll, Waker};

pub use super::ring::Ring;
use crate::abstractions::dma::DMA;
use crate::abstractions::PlatformAbstractions;

use async_ringbuf::consumer::PopFuture;
use futures::future::FusedFuture;
use futures::task::AtomicWaker;
use tock_registers::interfaces::Writeable;
use tock_registers::register_structs;
use tock_registers::registers::ReadWrite;
use xhci::ring::trb::event::Allowed;
use xhci::ring::trb::event::CompletionCode;

register_structs! {
    pub EventRingSte {
        (0x000 => addr_low: ReadWrite<u32>),
        (0x004 => addr_high: ReadWrite<u32>),
        (0x008 => size: ReadWrite<u16>),
        (0x00A => _reserved),
        (0x010 => @END),
    }
}

pub struct EventRing<O>
where
    O: PlatformAbstractions,
{
    pub waker: AtomicWaker,
    pub ring: Ring<O>,
    pub ste: DMA<[EventRingSte], O::DMA>,
}

impl<O> EventRing<O>
where
    O: PlatformAbstractions,
{
    pub fn new(os: O) -> Self {
        let a = os.dma_alloc();
        let mut ring = EventRing {
            ste: DMA::zeroed(1, 64, a),
            ring: Ring::new(os, 256, false),
            waker: AtomicWaker::new(),
        };
        ring.ring.cycle = true;
        ring.ste[0].addr_low.set(ring.ring.register() as u32);
        ring.ste[0]
            .addr_high
            .set((ring.ring.register() >> 32) as u32);
        ring.ste[0].size.set(ring.ring.trbs.len() as u16);

        ring
    }

    /// 完成一次循环返回 true
    pub fn next(&mut self) -> Option<(Allowed, bool)> {
        let (data, flag) = self.ring.current_data();
        let data = unsafe {
            let mut out = [0u32; 4];
            for i in 0..out.len() {
                out[i] = data.as_ptr().add(i).read_volatile();
            }
            out
        };

        let allowed = Allowed::try_from(data).ok()?;

        if flag != allowed.cycle_bit() {
            return None;
        }
        if let Allowed::TransferEvent(c) = allowed
            && let Ok(CompletionCode::Invalid) = c.completion_code()
        {
            return None;
        }

        fence(Ordering::SeqCst);

        let cycle = self.ring.inc_deque();
        Some((allowed, cycle))
    }

    pub fn wake(&self) {
        self.waker.wake();
    }

    pub fn has_next(&self) -> bool {
        let (data, flag) = self.ring.current_data();
        let data = unsafe {
            let mut out = [0u32; 4];
            for i in 0..out.len() {
                out[i] = data.as_ptr().add(i).read_volatile();
            }
            out
        };

        Allowed::try_from(data).is_ok_and(|allowed| {
            flag == allowed.cycle_bit() && {
                if let Allowed::TransferEvent(c) = allowed
                    && let Ok(CompletionCode::Invalid) = c.completion_code()
                {
                    return false;
                } else {
                    true
                }
            }
        })
    }

    pub fn async_next<'a>(&'a mut self) -> NextFuture<'a, O> {
        NextFuture {
            owner: self,
            done: false,
        }
    }

    pub fn erdp(&self) -> u64 {
        self.ring.register() & 0xFFFF_FFFF_FFFF_FFF0
    }
    pub fn erstba(&self) -> u64 {
        let ptr = &self.ste[0];
        ptr as *const EventRingSte as usize as u64
    }

    fn register_waker(&self, waker: &Waker)
    where
        O: PlatformAbstractions,
    {
        self.waker.register(waker);
    }
}

pub struct NextFuture<'a, O>
where
    O: PlatformAbstractions,
{
    owner: &'a mut EventRing<O>,
    done: bool,
}

impl<'a, O> FusedFuture for NextFuture<'a, O>
where
    O: PlatformAbstractions,
{
    fn is_terminated(&self) -> bool {
        self.done
    }
}

impl<'a, O> Unpin for NextFuture<'a, O> where O: PlatformAbstractions {}

impl<'a, O> Future for NextFuture<'a, O>
where
    O: PlatformAbstractions,
{
    type Output = (Allowed, bool);

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let mut waker_registered = false;
        loop {
            assert!(!self.done);
            if let Some(item) = self.owner.next() {
                self.done = true;
                break Poll::Ready(item);
            }
            if waker_registered {
                break Poll::Pending;
            }
            self.owner.register_waker(cx.waker());
            waker_registered = true;
        }
    }
}
