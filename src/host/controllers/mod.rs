use alloc::{sync::Arc, vec::Vec};

use crate::abstractions::{PlatformAbstractions, USBSystemConfig};

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

    // fn devices(&self) -> &Vec<USBDevice<DEVICE_BUFFER_SIZE>>;

    // fn device<'a>(&'a self) -> &'a USBDevice<'a, DEVICE_BUFFER_SIZE>;

    // fn control_transfer(
    //     &mut self,
    //     dev_slot_id: usize,
    //     urb_req: ControlTransfer,
    // ) -> crate::err::Result<UCB<O>>;

    // fn interrupt_transfer(
    //     &mut self,
    //     dev_slot_id: usize,
    //     urb_req: InterruptTransfer,
    // ) -> crate::err::Result<UCB<O>>;

    // fn bulk_transfer(
    //     &mut self,
    //     dev_slot_id: usize,
    //     urb_req: BulkTransfer,
    // ) -> crate::err::Result<UCB<O>>;

    // fn configure_device(
    //     &mut self,
    //     dev_slot_id: usize,
    //     urb_req: Configuration,
    // ) -> crate::err::Result<UCB<O>>;

    // fn extra_step(&mut self, dev_slot_id: usize, urb_req: ExtraStep) -> crate::err::Result<UCB<O>>;

    // fn device_slot_assignment(&mut self) -> usize;
    // fn address_device(&mut self, slot_id: usize, port_id: usize);
    // fn control_fetch_control_point_packet_size(&mut self, slot_id: usize) -> u8;
    // fn set_ep0_packet_size(&mut self, dev_slot_id: usize, max_packet_size: u16);
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
