use core::{
    cell::{Cell, UnsafeCell},
    marker::PhantomData,
    ops::DerefMut,
    sync::Exclusive,
    task::Waker,
};

use alloc::{format, sync::Arc};

use async_lock::{RwLock, Semaphore, SemaphoreGuardArc};
use async_ringbuf::{traits::AsyncProducer, AsyncStaticProd};
use usb_descriptor_decoder::descriptors::{
    desc_configuration::Configuration, topological_desc::TopologicalUSBDescriptorRoot,
};

use crate::{
    abstractions::{dma::DMA, PlatformAbstractions, USBSystemConfig},
    usb::operations::{OperationErrors, RequestedOperation, USBRequest},
};

pub struct USBDevice<'a, const SIZE: usize> {
    pub state: RwLock<Cell<DeviceState>>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub descriptor: Arc<TopologicalUSBDescriptorRoot>,
    configure_sem: Arc<Semaphore>,
    request_channel: RwLock<AsyncStaticProd<'a, USBRequest, SIZE>>,
}

pub enum DeviceState {
    Probed,
    Assigned,
    Configured(usize),
    Working,
}

#[derive(Debug)]
pub struct ConfigureSemaphore(SemaphoreGuardArc);

impl<'a, const CHANNEL_SIZE: usize> USBDevice<'a, CHANNEL_SIZE> {
    pub fn new(
        descriptor: Arc<TopologicalUSBDescriptorRoot>,
        sender: AsyncStaticProd<'a, USBRequest, CHANNEL_SIZE>,
    ) -> Result<Self, ()> {
        Ok(USBDevice {
            state: RwLock::new(Cell::new(DeviceState::Probed)),
            vendor_id: descriptor.device.data.vendor,
            product_id: descriptor.device.data.product_id,
            descriptor,
            request_channel: sender.into(),
            configure_sem: Semaphore::new(1).into(),
        })
    }

    pub fn acquire_cfg_sem(&self) -> Option<ConfigureSemaphore> {
        self.configure_sem
            .try_acquire_arc()
            .map(|sem| ConfigureSemaphore(sem))
    }

    pub async fn post_request(&self, request: USBRequest) {
        self.request_channel
            .write()
            .await
            .push(request)
            .await
            .expect(&format!(
                "device channel droped, wtf? vendor-{}|product-{}",
                self.vendor_id, self.product_id,
            ))
    }

    pub async fn request_assign(&self) -> &Self {
        {
            let sem = self.configure_sem.acquire_arc().await;
            self.post_request(USBRequest {
                operation: RequestedOperation::Assign(ConfigureSemaphore(sem)),
                callback: None,
            })
            .await;
        }

        self.configure_sem.acquire().await;
        self.state.write().await.replace(DeviceState::Assigned);
        self
    }
}
