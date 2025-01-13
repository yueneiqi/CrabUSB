use core::{marker::PhantomData, task::Waker};

use alloc::sync::Arc;
use embassy_sync::{
    blocking_mutex::{
        raw::{NoopRawMutex, RawMutex},
        NoopMutex,
    },
    zerocopy_channel::{self, Receiver, Sender},
};
use usb_descriptor_decoder::descriptors::topological_desc::TopologicalUSBDescriptorRoot;

use crate::{
    abstractions::{dma::DMA, PlatformAbstractions, USBSystemConfig},
    usb::operations::USBRequest,
};

pub struct USBDevice<'a, O>
where
    O: PlatformAbstractions,
{
    pub state: DeviceState,
    pub vendor_id: u16,
    pub product_id: u16,
    pub descriptor: Arc<TopologicalUSBDescriptorRoot>,
    request_channel: Sender<'a, NoopRawMutex, USBRequest>,
    _actual_buffer: DMA<[USBRequest], O::DMA>,
}

pub enum DeviceState {
    Probed,
    Assigned,
    Configured,
    Working,
}
impl<'a, O> USBDevice<'a, O>
where
    O: PlatformAbstractions,
{
    pub fn new(
        descriptor: Arc<TopologicalUSBDescriptorRoot>,
        o: Arc<USBSystemConfig<O>>,
    ) -> Result<(Self, Receiver<'a, NoopRawMutex, USBRequest>), ()> {
        let buffer: DMA<[USBRequest], O::DMA> = DMA::new_vec(
            USBRequest::default(),
            o.device_request_buffer_size,
            O::PAGE_SIZE,
            o.os.dma_alloc(),
        );

        let (sender, receiver) = zerocopy_channel::Channel::new(&mut buffer).split();
        Ok((
            USBDevice {
                state: DeviceState::Probed,
                vendor_id: descriptor.device.data.vendor,
                product_id: descriptor.device.data.product_id,
                descriptor,
                request_channel: sender,
                _actual_buffer: buffer,
            },
            receiver,
        ));

        Err(())
    }
}
