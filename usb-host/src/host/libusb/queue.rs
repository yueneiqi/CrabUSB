use libusb1_sys::{constants::LIBUSB_TRANSFER_COMPLETED, *};
use log::*;
use usb_if::{
    err::TransferError,
    host::ResultTransfer,
    transfer::wait::{CallbackOnReady, WaitMap},
};

pub struct Queue {
    elems: Vec<USBTransfer>,
    wait_map: WaitMap<usize, Result<usize, TransferError>>,
    iter: usize,
}

impl Queue {
    pub fn new(size: usize) -> Self {
        let mut elems = Vec::with_capacity(size);
        for _ in 0..size {
            elems.push(USBTransfer::new(0));
        }
        let wait_map = WaitMap::new(0..size);
        Self {
            iter: 0,
            elems,
            wait_map,
        }
    }

    pub fn submit<'a, F>(&mut self, f: F) -> ResultTransfer<'a>
    where
        F: FnOnce(&mut USBTransfer, &mut CallbackOnReady),
    {
        let id = self.iter;

        let user_data = Box::new(UserData {
            id,
            wait_map: &self.wait_map,
        });
        let user_data_ptr = Box::into_raw(user_data) as *mut std::ffi::c_void;

        let transfer = self.elems.get_mut(id).unwrap();
        let mut on_ready = CallbackOnReady {
            on_ready: |_, _, _| {},
            param1: std::ptr::null_mut(),
            param2: std::ptr::null_mut(),
            param3: std::ptr::null_mut(),
        };
        unsafe {
            f(transfer, &mut on_ready);
            (*transfer.ptr).user_data = user_data_ptr;
            (*transfer.ptr).callback = transfer_callback;
        }
        usb!(libusb_submit_transfer(transfer.ptr)).map_err(|e| {
            TransferError::Other(format!("Failed to submit transfer: {e:?}").into())
        })?;

        trace!("Submitted transfer id {id}");

        let res = self
            .wait_map
            .try_wait_for_result(id, Some(on_ready))
            .ok_or(TransferError::Other("Failed to get waiter".into()))?;
        self.iter = (self.iter + 1) % self.elems.len();
        Ok(res)
    }
}

struct UserData<'a> {
    id: usize,
    wait_map: &'a WaitMap<usize, Result<usize, TransferError>>,
}

extern "system" fn transfer_callback(transfer: *mut libusb_transfer) {
    trace!("Transfer callback called");
    unsafe {
        let user_data_ptr = (*transfer).user_data as *mut UserData;
        if user_data_ptr.is_null() {
            return;
        }
        let user_data = Box::from_raw(user_data_ptr);
        let id = user_data.id;
        let wait_map = user_data.wait_map;

        let result = if (*transfer).status == LIBUSB_TRANSFER_COMPLETED {
            Ok((*transfer).actual_length as usize)
        } else {
            Err(TransferError::Other(
                format!("Transfer failed with status: {:?}", (*transfer).status).into(),
            ))
        };
        wait_map.set_result(id, result);
    }
}

pub struct USBTransfer {
    pub ptr: *mut libusb_transfer,
    pub buff: Vec<u8>,
}

unsafe impl Send for USBTransfer {}

impl USBTransfer {
    pub fn new(iso_packets: usize) -> Self {
        let transfer = unsafe { libusb_alloc_transfer(iso_packets as i32) };
        Self {
            ptr: transfer,
            buff: vec![0u8; 512],
        }
    }
}

impl Drop for USBTransfer {
    fn drop(&mut self) {
        unsafe {
            libusb_free_transfer(self.ptr);
        }
    }
}
