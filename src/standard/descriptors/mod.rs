use core::num::NonZero;

use alloc::vec::Vec;

use crate::standard::transfer::Direction;

pub(crate) mod parser;

/// Endpoint type.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
#[allow(dead_code)]
pub enum EndpointType {
    /// Control endpoint.
    Control = 0,

    /// Isochronous endpoint.
    Isochronous = 1,

    /// Bulk endpoint.
    Bulk = 2,

    /// Interrupt endpoint.
    Interrupt = 3,
}

#[derive(Debug, Clone)]
pub struct EndpointDescriptor {
    pub address: u8,
    pub max_packet_size: u16,
    pub direction: Direction,
    pub transfer_type: EndpointType,
    pub packets_per_microframe: usize,
    pub interval: u8,
}

impl From<parser::EndpointDescriptor<'_>> for EndpointDescriptor {
    fn from(desc: parser::EndpointDescriptor) -> Self {
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

#[derive(Debug, Clone)]
pub struct InterfaceDescriptor {
    pub interface_number: u8,
    pub alternate_setting: u8,
    pub class: u8,
    pub subclass: u8,
    pub protocol: u8,
    pub string_index: Option<NonZero<u8>>,
    pub num_endpoints: u8,
    pub endpoints: Vec<EndpointDescriptor>,
}

impl From<parser::InterfaceDescriptor<'_>> for InterfaceDescriptor {
    fn from(desc: parser::InterfaceDescriptor) -> Self {
        InterfaceDescriptor {
            interface_number: desc.interface_number(),
            alternate_setting: desc.alternate_setting(),
            class: desc.class(),
            subclass: desc.subclass(),
            protocol: desc.protocol(),
            string_index: desc.string_index(),
            num_endpoints: desc.num_endpoints(),
            endpoints: desc.endpoints().map(EndpointDescriptor::from).collect(),
        }
    }
}
#[derive(Debug, Clone)]
pub struct InterfaceDescriptors {
    pub interface_number: u8,
    pub alt_settings: Vec<InterfaceDescriptor>,
}

impl InterfaceDescriptors {
    pub fn first_alt_setting(&self) -> InterfaceDescriptor {
        self.alt_settings.first().cloned().unwrap()
    }
}

impl From<parser::InterfaceDescriptors<'_>> for InterfaceDescriptors {
    fn from(desc: parser::InterfaceDescriptors) -> Self {
        InterfaceDescriptors {
            interface_number: desc.interface_number(),
            alt_settings: desc.alt_settings().map(InterfaceDescriptor::from).collect(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ConfigurationDescriptor {
    pub num_interfaces: u8,
    pub configuration_value: u8,
    pub attributes: u8,
    pub max_power: u8,
    pub string_index: Option<NonZero<u8>>,
    pub interfaces: Vec<InterfaceDescriptors>,
}

impl From<parser::ConfigurationDescriptor<'_>> for ConfigurationDescriptor {
    fn from(desc: parser::ConfigurationDescriptor) -> Self {
        ConfigurationDescriptor {
            num_interfaces: desc.num_interfaces(),
            configuration_value: desc.configuration_value(),
            attributes: desc.attributes(),
            max_power: desc.max_power(),
            string_index: desc.string_index(),
            interfaces: desc.interfaces().map(InterfaceDescriptors::from).collect(),
        }
    }
}
