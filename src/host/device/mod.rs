use core::{
    cell::{Cell, RefCell, UnsafeCell},
    future::IntoFuture,
    marker::PhantomData,
    ops::DerefMut,
    sync::Exclusive,
    task::Waker,
};

use alloc::{format, sync::Arc};

use async_lock::{futures::LockArc, Mutex, OnceCell, RwLock, Semaphore, SemaphoreGuardArc};
use async_ringbuf::{traits::AsyncProducer, AsyncStaticProd};
use usb_descriptor_decoder::descriptors::{
    desc_configuration::Configuration, topological_desc::TopologicalUSBDescriptorRoot,
};

use crate::usb::operations::{
    CallbackFn, CallbackValue, ChannelNumber, CompleteAction, ExtraAction, RequestResult,
    RequestedOperation, USBRequest,
};

pub struct USBDevice<'a, const SIZE: usize> {
    pub state: Arc<RwLock<DeviceState>>,
    pub vendor_id: u16,
    pub product_id: u16,
    pub topology_path: USBTopologyPath,
    pub descriptor: Arc<TopologicalUSBDescriptorRoot>,
    configure_sem: Arc<Semaphore>,
    request_channel: RwLock<AsyncStaticProd<'a, USBRequest, SIZE>>,
}

#[derive(Default, Debug, Clone)]
pub struct USBTopologyPath(u32);

pub enum DeviceState {
    Probed,
    Assigned,
    Configured(usize),
}

#[derive(Debug)]
pub struct ConfigureSemaphore(SemaphoreGuardArc);

impl<'a, const CHANNEL_SIZE: usize> USBDevice<'a, CHANNEL_SIZE> {
    pub fn new(
        descriptor: Arc<TopologicalUSBDescriptorRoot>,
        sender: AsyncStaticProd<'a, USBRequest, CHANNEL_SIZE>,
    ) -> Result<Self, ()> {
        Ok(USBDevice {
            state: RwLock::new(DeviceState::Probed).into(),
            vendor_id: descriptor.device.data.vendor,
            product_id: descriptor.device.data.product_id,
            descriptor,
            request_channel: sender.into(),
            configure_sem: Semaphore::new(1).into(),
            topology_path: todo!(),
        })
    }

    pub fn acquire_cfg_sem(&self) -> Option<ConfigureSemaphore> {
        self.configure_sem
            .try_acquire_arc()
            .map(|sem| ConfigureSemaphore(sem))
    }

    async fn post_usb_request(&self, request: USBRequest) {
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

    pub async fn request_oneshot(&self, request: RequestedOperation) {
        self.post_usb_request(USBRequest {
            operation: request,
            extra_action: ExtraAction::default(),
            complete_action: CompleteAction::default(),
        })
        .await;
    }

    ///I must lost my mind...
    pub async fn request_once(&self, request: RequestedOperation) -> RequestResult {
        let (val, setter) = CallbackValue::new();
        self.post_usb_request(USBRequest {
            extra_action: ExtraAction::NOOP,
            operation: request,
            complete_action: CompleteAction::SimpleResponse(setter),
        })
        .await;

        val.get().await
    }

    pub async fn keep_request(
        &self,
        request: RequestedOperation,
        callback: CallbackFn,
        channel_number: ChannelNumber,
    ) {
        self.post_usb_request(USBRequest {
            extra_action: ExtraAction::KeepFill(channel_number),
            operation: request,
            complete_action: CompleteAction::Callback(callback),
        })
        .await
    }

    pub async fn request_assign(&self) -> &Self {
        {
            let sem = self.configure_sem.acquire_arc().await;
            self.post_usb_request(USBRequest {
                operation: RequestedOperation::Assign(ConfigureSemaphore(sem)),
                extra_action: ExtraAction::default(),
                complete_action: CompleteAction::default(),
            })
            .await;
        }

        self.configure_sem.acquire().await;
        *self.state.write().await = DeviceState::Assigned;
        self
    }
}
