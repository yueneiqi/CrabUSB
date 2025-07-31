use crate::standard;

pub(crate) struct EndpointDescriptor {
    pub address: u8,
    pub max_packet_size: u16,
    pub direction: standard::transfer::Direction,
    pub transfer_type: standard::descriptors::TransferType,
    pub packets_per_microframe: usize,
    pub interval: u8,
}

impl From<standard::descriptors::EndpointDescriptor<'_>> for EndpointDescriptor {
    fn from(desc: standard::descriptors::EndpointDescriptor) -> Self {
        EndpointDescriptor {
            address: desc.address(),
            max_packet_size: desc.max_packet_size() as _,
            direction: desc.direction(),
            transfer_type: desc.transfer_type(),
            packets_per_microframe: desc.packets_per_microframe() as usize,
            interval: desc.interval(),
        }
    }
}

impl EndpointDescriptor {
    pub fn endpoint_type(&self) -> xhci::context::EndpointType {
        match self.transfer_type {
            standard::descriptors::TransferType::Control => xhci::context::EndpointType::Control,
            standard::descriptors::TransferType::Isochronous => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::IsochOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::IsochIn,
            },
            standard::descriptors::TransferType::Bulk => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::BulkOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::BulkIn,
            },
            standard::descriptors::TransferType::Interrupt => match self.direction {
                standard::transfer::Direction::Out => xhci::context::EndpointType::InterruptOut,
                standard::transfer::Direction::In => xhci::context::EndpointType::InterruptIn,
            },
        }
    }

    pub fn dci(&self) -> u8 {
        // DCI = (endpoint_number * 2) + direction
        // direction: OUT=0, IN=1
        let endpoint_number = self.address & 0x0F; // 提取端点号（低4位）
        (endpoint_number * 2)
            + 1
            + if self.direction == standard::transfer::Direction::In {
                1
            } else {
                0
            }
    }
}
