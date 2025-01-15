use alloc::{sync::Arc, vec::Vec};

use crate::{
    abstractions::{PlatformAbstractions, USBSystemConfig},
    usb::operations::USBRequest,
};

use super::device::USBDevice;

///host layer

#[cfg(feature = "backend-xhci")]
mod xhci;

pub trait Controller<O, const DEVICE_BUFFER_SIZE: usize>: Send
where
    O: PlatformAbstractions,
{
    fn new(config: Arc<USBSystemConfig<O, DEVICE_BUFFER_SIZE>>) -> Self
    where
        Self: Sized;

    async fn init(&mut self);

    async fn probe(&mut self);

    /// each device should able to access actual transfer function in controller
    fn device_accesses(&self) -> Vec<Arc<USBDevice<DEVICE_BUFFER_SIZE>>>;

    async fn post_emerge_request(&mut self, req: USBRequest);
}

// #[cfg(feature = "cotton-frontend")]
// impl<T, O> cotton_usb_host::host_controller::HostController for T
// where
//     T: Controller<O>,
//     O: PlatformAbstractions,
// {
//     type InterruptPipe;

//     type DeviceDetect;

//     fn device_detect(&self) -> Self::DeviceDetect {
//         todo!()
//     }

//     fn reset_root_port(&self, rst: bool) {
//         todo!()
//     }

//     fn control_transfer(
//         &self,
//         address: u8,
//         packet_size: u8,
//         setup: cotton_usb_host::wire::SetupPacket,
//         data_phase: cotton_usb_host::usb_bus::DataPhase<'_>,
//     ) -> impl core::future::Future<Output = Result<usize, cotton_usb_host::usb_bus::UsbError>> {
//         todo!()
//     }

//     fn bulk_in_transfer(
//         &self,
//         address: u8,
//         endpoint: u8,
//         packet_size: u16,
//         data: &mut [u8],
//         transfer_type: cotton_usb_host::usb_bus::TransferType,
//         data_toggle: &core::cell::Cell<bool>,
//     ) -> impl core::future::Future<Output = Result<usize, cotton_usb_host::usb_bus::UsbError>> {
//         todo!()
//     }

//     fn bulk_out_transfer(
//         &self,
//         address: u8,
//         endpoint: u8,
//         packet_size: u16,
//         data: &[u8],
//         transfer_type: cotton_usb_host::usb_bus::TransferType,
//         data_toggle: &core::cell::Cell<bool>,
//     ) -> impl core::future::Future<Output = Result<usize, cotton_usb_host::usb_bus::UsbError>> {
//         todo!()
//     }

//     fn alloc_interrupt_pipe(
//         &self,
//         address: u8,
//         endpoint: u8,
//         max_packet_size: u16,
//         interval_ms: u8,
//     ) -> impl core::future::Future<Output = Self::InterruptPipe> {
//         todo!()
//     }

//     fn try_alloc_interrupt_pipe(
//         &self,
//         address: u8,
//         endpoint: u8,
//         max_packet_size: u16,
//         interval_ms: u8,
//     ) -> Result<Self::InterruptPipe, cotton_usb_host::usb_bus::UsbError> {
//         todo!()
//     }
// }
