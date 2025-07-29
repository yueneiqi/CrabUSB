use core::{
    cell::UnsafeCell,
    sync::atomic::{AtomicBool, Ordering},
    task::Poll,
};

use alloc::{
    collections::btree_map::BTreeMap,
    sync::{Arc, Weak},
};
use futures::task::AtomicWaker;

pub struct WaitMap<T>(Arc<UnsafeCell<WaitMapRaw<T>>>);

unsafe impl<T> Send for WaitMap<T> {}
unsafe impl<T> Sync for WaitMap<T> {}

impl<T> WaitMap<T> {
    pub fn new(id_list: impl Iterator<Item = u64>) -> Self {
        Self(Arc::new(UnsafeCell::new(WaitMapRaw::new(id_list))))
    }

    pub fn empty() -> Self {
        Self(Arc::new(UnsafeCell::new(WaitMapRaw(BTreeMap::new()))))
    }

    pub fn append(&self, id_ls: impl Iterator<Item = u64>) {
        let raw = unsafe { &mut *self.0.get() };
        for id in id_ls {
            raw.0.insert(id, Elem::new());
        }
    }

    pub unsafe fn set_result(&self, id: u64, result: T) {
        unsafe { (&mut *self.0.get()).set_result(id, result) };
    }

    pub fn try_wait_for_result(&self, id: u64) -> Option<Waiter<T>> {
        unsafe {
            (&mut *self.0.get()).try_lock(id)?;
        }
        Some(Waiter {
            id,
            wait: self.weak(),
        })
    }

    pub fn weak(&self) -> WaitMapWeak<T> {
        WaitMapWeak(Arc::downgrade(&self.0))
    }
}

pub struct WaitMapWeak<T>(Weak<UnsafeCell<WaitMapRaw<T>>>);

impl<T> Clone for WaitMapWeak<T> {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl<T> WaitMapWeak<T> {
    pub fn try_wait_for_result(&self, id: u64) -> Option<Waiter<T>> {
        let arc = self.0.upgrade()?;
        unsafe {
            (&mut *arc.get()).try_lock(id)?;
        }
        Some(Waiter {
            id,
            wait: self.clone(),
        })
    }
}

unsafe impl<T> Send for WaitMapWeak<T> {}
unsafe impl<T> Sync for WaitMapWeak<T> {}

pub struct WaitMapRaw<T>(BTreeMap<u64, Elem<T>>);

struct Elem<T> {
    result: Option<T>,
    waker: AtomicWaker,
    using: AtomicBool,
    result_ok: AtomicBool,
}

impl<T> Elem<T> {
    fn new() -> Self {
        Self {
            result: None,
            waker: AtomicWaker::new(),
            using: AtomicBool::new(false),
            result_ok: AtomicBool::new(false),
        }
    }
}

impl<T> WaitMapRaw<T> {
    pub fn new(id_list: impl Iterator<Item = u64>) -> Self {
        let mut map = BTreeMap::new();
        for id in id_list {
            map.insert(id, Elem::new());
        }
        Self(map)
    }

    pub unsafe fn set_result(&mut self, id: u64, result: T) {
        let entry = match self.0.get_mut(&id) {
            Some(entry) => entry,
            None => {
                let id_0 = self.0.keys().next();
                let id_end = self.0.keys().last();
                panic!(
                    "WaitMap: set_result called with unknown id {id:X}, known ids: [{id_0:X?},{id_end:X?}]"
                );
            }
        };
        entry.result.replace(result);
        entry.result_ok.store(true, Ordering::Release);
        if let Some(wake) = entry.waker.take() {
            wake.wake();
        }
    }

    fn try_lock(&mut self, id: u64) -> Option<()> {
        let elem = self
            .0
            .get_mut(&id)
            .expect("WaitMap: wait_for_result called with unknown id");

        if elem
            .using
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
        {
            Some(())
        } else {
            None
        }
    }
}

pub struct Waiter<T> {
    id: u64,
    wait: WaitMapWeak<T>,
}

impl<T> Future for Waiter<T> {
    type Output = T;

    fn poll(
        self: core::pin::Pin<&mut Self>,
        cx: &mut core::task::Context<'_>,
    ) -> core::task::Poll<Self::Output> {
        let elem = self.wait.0.upgrade().unwrap();
        let elem = unsafe { &mut *elem.get() }.0.get_mut(&self.id).unwrap();

        if elem.result_ok.load(Ordering::Acquire) {
            let result = elem
                .result
                .take()
                .expect("Waiter polled after result was set");
            elem.using.store(false, Ordering::Release);
            return Poll::Ready(result);
        }
        elem.waker.register(cx.waker());
        Poll::Pending
    }
}
