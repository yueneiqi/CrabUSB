use core::{cell::UnsafeCell, task::Poll};

use alloc::{
    collections::btree_map::BTreeMap,
    format,
    sync::{Arc, Weak},
    vec::Vec,
};
use futures::task::AtomicWaker;

use crate::err::USBError;

pub struct WaitMap<T>(Arc<UnsafeCell<WaitMapRaw<T>>>);

unsafe impl<T> Send for WaitMap<T> {}
unsafe impl<T> Sync for WaitMap<T> {}

impl<T> WaitMap<T> {
    pub fn new(id_list: impl Iterator<Item = u64>) -> Self {
        Self(Arc::new(UnsafeCell::new(WaitMapRaw::new(id_list))))
    }

    pub unsafe fn set_result(&self, id: u64, result: T) {
        unsafe { (&mut *self.0.get()).set_result(id, result) };
    }

    pub fn wait_for_result(&mut self, id: u64) -> Waiter<'_, T> {
        let m = unsafe { &mut *self.0.get() };
        Waiter { id, wait: m }
    }

    pub fn weak(&self) -> WaitMapWeak<T> {
        WaitMapWeak(Arc::downgrade(&self.0))
    }
}

#[derive(Clone)]
pub struct WaitMapWeak<T>(Weak<UnsafeCell<WaitMapRaw<T>>>);

impl<T> WaitMapWeak<T> {
    pub unsafe fn set_result(&self, id: u64, result: T) -> Option<()> {
        let r = self.0.upgrade()?;
        unsafe {
            (&mut *r.get()).set_result(id, result);
        }
        Some(())
    }
}

unsafe impl<T> Send for WaitMapWeak<T> {}
unsafe impl<T> Sync for WaitMapWeak<T> {}

pub struct WaitMapRaw<T>(BTreeMap<u64, Elem<T>>);

struct Elem<T> {
    result: Option<T>,
    waker: AtomicWaker,
}

impl<T> WaitMapRaw<T> {
    pub fn new(id_list: impl Iterator<Item = u64>) -> Self {
        let mut map = BTreeMap::new();
        for id in id_list {
            map.insert(
                id,
                Elem {
                    result: None,
                    waker: AtomicWaker::new(),
                },
            );
        }
        Self(map)
    }

    pub unsafe fn set_result(&mut self, id: u64, result: T) {
        let entry = match self.0.get_mut(&id) {
            Some(entry) => entry,
            None => {
                let ls = self
                    .0
                    .keys()
                    .map(|k| format!("{k:X}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                panic!("WaitMap: set_result called with unknown id {id:X}, known ids: {ls}");
            }
        };
        entry.result.replace(result);
        if let Some(wake) = entry.waker.take() {
            wake.wake();
        }
    }

    fn poll(&mut self, id: u64, cx: &mut core::task::Context<'_>) -> Poll<T> {
        let entry = self.0.get_mut(&id).unwrap();

        match entry.result.take() {
            Some(v) => Poll::Ready(v),
            None => {
                entry.waker.register(cx.waker());
                Poll::Pending
            }
        }
    }
}

pub struct Waiter<'a, T> {
    id: u64,
    wait: &'a mut WaitMapRaw<T>,
}

impl<T> Future for Waiter<'_, T> {
    type Output = T;

    fn poll(
        mut self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let addr = self.id;
        self.wait.poll(addr, cx)
    }
}
