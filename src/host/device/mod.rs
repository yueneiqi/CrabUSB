use core::mem;

use alloc::{borrow::ToOwned, sync::Arc};

use async_lock::{OnceCell, RwLock, Semaphore, SemaphoreGuardArc};
use async_ringbuf::{traits::AsyncProducer, AsyncRb};
use futures::FutureExt;
use usb_descriptor_decoder::descriptors::topological_desc::TopologicalUSBDescriptorRoot;

use crate::usb::{
    operations::{
        CallbackFn, ChannelNumber, CompleteAction, ExtraAction, RequestResult, RequestedOperation,
        USBRequest,
    },
    standards::TopologyRoute,
};

pub struct USBDevice<'a, const BUFFER_SIZE: usize> {
    pub state: Arc<RwLock<DeviceState>>,
    pub slot_id: OnceCell<u8>,
    pub vendor_id: OnceCell<u16>,
    pub product_id: OnceCell<u16>,
    pub descriptor: OnceCell<Arc<TopologicalUSBDescriptorRoot>>,
    pub topology_path: TopologyRoute,
    configure_sem: Arc<Semaphore>,
    request_channel: RwLock<ArcAsyncRingBufPord<USBRequest<'a, BUFFER_SIZE>, BUFFER_SIZE>>,
}

pub enum DeviceState {
    Probed,
    Assigned,
    Configured,
    PreDrop, //for hot plug,  how to design drop all working functions mechanism?
}

pub type ArcAsyncRingBufPord<T, const N: usize> = async_ringbuf::wrap::AsyncWrap<
    Arc<AsyncRb<ringbuf::storage::Owning<[mem::MaybeUninit<T>; N]>>>,
    true,
    false,
>;
pub type ArcAsyncRingBufCons<T, const N: usize> = async_ringbuf::wrap::AsyncWrap<
    Arc<AsyncRb<ringbuf::storage::Owning<[mem::MaybeUninit<T>; N]>>>,
    false,
    true,
>;

#[derive(Debug)]
pub struct ConfigureSemaphore(SemaphoreGuardArc);

impl<'a, const BUFFER_SIZE: usize> USBDevice<'a, BUFFER_SIZE> {
    pub fn new(sender: ArcAsyncRingBufPord<USBRequest<'a, BUFFER_SIZE>, BUFFER_SIZE>) -> Self {
        USBDevice {
            state: RwLock::new(DeviceState::Probed).into(),
            vendor_id: OnceCell::new(),
            product_id: OnceCell::new(),
            descriptor: OnceCell::new(),
            request_channel: sender.into(),
            configure_sem: Semaphore::new(1).into(),
            topology_path: TopologyRoute::new(),
            slot_id: OnceCell::new(),
        }
    }

    pub fn acquire_cfg_sem(&self) -> Option<ConfigureSemaphore> {
        self.configure_sem.try_acquire_arc().map(ConfigureSemaphore)
    }

    async fn post_usb_request(&self, request: USBRequest<'a, BUFFER_SIZE>) {
        self.request_channel
            .write()
            .await
            .push(request)
            .await
            .unwrap_or_else(|_| {
                panic!(
                    "device channel droped, wtf? vendor-{:?}|product-{:?}",
                    self.vendor_id.get(),
                    self.product_id.get()
                )
            })
    }

    pub async fn request_oneshot(&self, request: RequestedOperation) {
        self.check_self_status().await;
        self.post_usb_request(USBRequest {
            operation: request,
            extra_action: ExtraAction::default(),
            complete_action: CompleteAction::default(),
        })
        .await;
    }

    ///I must lost my mind...
    pub async fn request_once(&self, request: RequestedOperation) -> Result<RequestResult, u8> {
        self.check_self_status().await;
        let cell = Arc::new(OnceCell::new());
        self.post_usb_request(USBRequest {
            extra_action: ExtraAction::NOOP,
            operation: request,
            complete_action: CompleteAction::SimpleResponse(cell.clone()),
        })
        .await;

        cell.wait().await.to_owned()
    }

    pub async fn keep_request(
        &self,
        request: RequestedOperation,
        callback: CallbackFn,
        channel_number: ChannelNumber,
    ) {
        self.check_self_status().await;
        self.post_usb_request(USBRequest {
            extra_action: ExtraAction::KeepFill(channel_number),
            operation: request,
            complete_action: CompleteAction::Callback(callback),
        })
        .await
    }

    async fn check_self_status(&self) -> &Self {
        match *self.state.read().await {
            DeviceState::Probed => {
                self.request_assign().await;
            }
            DeviceState::PreDrop => todo!(),
            _ => (),
        };

        self
    }

    pub async fn request_assign(&self) {
        {
            let sem = self.configure_sem.acquire_arc().await;
            self.post_usb_request(USBRequest {
                operation: RequestedOperation::Assign_Address(self.topology_path.clone()),
                extra_action: ExtraAction::default(),
                complete_action: CompleteAction::DropSem(ConfigureSemaphore(sem)),
            })
            .await;
        }

        let sem = self.configure_sem.acquire().await;

        //TODO: set self transfer speed

        drop(sem);
        *self.state.write().await = DeviceState::Assigned;
    }
}
