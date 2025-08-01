use dma_api::DSliceMut;
use log::trace;
use mbarrier::mb;
use xhci::{
    registers::doorbell,
    ring::trb::{
        event::CompletionCode,
        transfer::{self},
    },
};

use crate::{
    BusAddr,
    err::USBError,
    standard::{self, descriptors::EndpointDescriptor, transfer::Direction},
    xhci::{def::Dci, device::DeviceState, ring::Ring},
};

pub(crate) struct EndpointRaw {
    dci: Dci,
    pub ring: Ring,
    device: DeviceState,
}

unsafe impl Send for EndpointRaw {}

impl EndpointRaw {
    pub fn new(dci: Dci, device: &DeviceState) -> Result<Self, USBError> {
        Ok(Self {
            dci,
            ring: Ring::new(true, dma_api::Direction::Bidirectional)?,
            device: device.clone(),
        })
    }

    pub async fn enque(
        &mut self,
        trbs: impl Iterator<Item = transfer::Allowed>,
        direction: Direction,
        buff_addr: usize,
        buff_len: usize,
    ) -> Result<(), USBError> {
        if buff_len > 0 {
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(buff_addr as *mut u8, buff_len) };
            let dm = DSliceMut::from(
                data_slice,
                match direction {
                    Direction::Out => dma_api::Direction::ToDevice,
                    Direction::In => dma_api::Direction::FromDevice,
                },
            );
            dm.confirm_write_all();
        }

        let mut trb_ptr = BusAddr(0);

        for trb in trbs {
            trb_ptr = self.ring.enque_transfer(trb);
        }

        trace!("trb : {trb_ptr:#x?}");

        mb();

        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(self.dci.into());

        self.device.doorbell(bell);

        let ret = unsafe { self.device.root.try_wait_for_transfer(trb_ptr).unwrap() }.await;

        match ret.completion_code() {
            Ok(code) => {
                if !matches!(code, CompletionCode::Success) {
                    return Err(USBError::TransferEventError(code));
                }
            }
            Err(_e) => return Err(USBError::Unknown),
        }

        if buff_len > 0 {
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(buff_addr as *mut u8, buff_len) };

            let dm = DSliceMut::from(
                data_slice,
                match direction {
                    Direction::Out => dma_api::Direction::ToDevice,
                    Direction::In => dma_api::Direction::FromDevice,
                },
            );
            dm.preper_read_all();
        }
        Ok(())
    }

    pub fn bus_addr(&self) -> BusAddr {
        self.ring.bus_addr()
    }
}

impl EndpointDescriptor {
    pub fn endpoint_type(&self) -> xhci::context::EndpointType {
        match self.transfer_type {
            standard::descriptors::EndpointType::Control => xhci::context::EndpointType::Control,
            standard::descriptors::EndpointType::Isochronous => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::IsochOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::IsochIn,
            },
            standard::descriptors::EndpointType::Bulk => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::BulkOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::BulkIn,
            },
            standard::descriptors::EndpointType::Interrupt => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::InterruptOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::InterruptIn,
            },
        }
    }

    pub fn dci(&self) -> u8 {
        // DCI = (endpoint_number * 2) + direction
        // Control endpoint always has DCI 1
        let endpoint_number = self.address & 0x0F; // 提取端点号（低4位）
        (endpoint_number * 2)
            + match self.endpoint_type() {
                xhci::context::EndpointType::Control => 1, // Control endpoint always has DCI 1
                _ => {
                    if self.direction == standard::transfer::Direction::In {
                        1
                    } else {
                        0
                    }
                }
            }
    }
}
