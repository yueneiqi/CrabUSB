use core::ptr::null_mut;

use libusb1_sys::{
    libusb_device_handle, libusb_fill_bulk_transfer, libusb_fill_interrupt_transfer,
    libusb_fill_iso_transfer, libusb_transfer,
};
use usb_if::host::{
    EndpintIsoIn, EndpintIsoOut, EndpointBulkIn, EndpointBulkOut, EndpointInterruptIn,
    EndpointInterruptOut,
};

use crate::host::libusb::queue::Queue;

pub struct EndpointImpl {
    dev: *mut libusb_device_handle,
    queue: Queue,
    address: u8,
}

impl EndpointImpl {
    pub fn new(raw: *mut libusb_device_handle, address: u8, queue_size: usize) -> Box<Self> {
        Box::new(Self {
            dev: raw,
            queue: Queue::new(queue_size),
            address,
        })
    }
}

unsafe impl Send for EndpointImpl {}

impl usb_if::host::TEndpoint for EndpointImpl {}

impl EndpointBulkIn for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_bulk_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_mut_ptr(),
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpointBulkOut for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_bulk_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_ptr() as *mut u8,
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpointInterruptIn for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a mut [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_interrupt_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_mut_ptr(),
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpointInterruptOut for EndpointImpl {
    fn submit<'a>(&mut self, data: &'a [u8]) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_interrupt_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_ptr() as *mut u8,
                data.len() as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpintIsoIn for EndpointImpl {
    fn submit<'a>(
        &mut self,
        data: &'a mut [u8],
        num_iso_packets: usize,
    ) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_iso_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_mut_ptr(),
                data.len() as _,
                num_iso_packets as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

impl EndpintIsoOut for EndpointImpl {
    fn submit<'a>(
        &mut self,
        data: &'a [u8],
        num_iso_packets: usize,
    ) -> usb_if::host::ResultTransfer<'a> {
        self.queue.submit(|transfer, _| unsafe {
            libusb_fill_iso_transfer(
                transfer.ptr,
                self.dev,
                self.address,
                data.as_ptr() as *mut u8,
                data.len() as _,
                num_iso_packets as _,
                transfer_callback,
                null_mut(),
                1000,
            );
        })
    }
}

extern "system" fn transfer_callback(_transfer: *mut libusb_transfer) {}
