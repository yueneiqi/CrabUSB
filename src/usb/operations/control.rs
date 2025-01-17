use num_derive::FromPrimitive;

use super::{
    configurations::{AltnativeNumber, ConfigurationID, InterfaceNumber},
    Direction,
};

#[derive(Debug, Clone)]
pub struct ControlTransfer {
    //TODO: restruct control transfer to usb standard but not xhci standard
    pub request_type: bmRequestType,
    pub request: bRequest,
    pub index: u16,
    pub value: u16,
    pub data: Option<(usize, usize)>,
    pub response: bool,
}

#[allow(non_camel_case_types)]
#[derive(Debug, Clone)]
pub enum bRequest {
    Standard(bRequestStandard),
    Spec(u8),
}

impl From<bRequest> for u8 {
    fn from(value: bRequest) -> Self {
        match value {
            bRequest::Standard(b_request_standard) => b_request_standard as u8,
            bRequest::Spec(a) => a,
        }
    }
}

#[allow(non_camel_case_types)]
#[repr(u8)]
#[derive(Debug, Clone)]
pub enum bRequestStandard {
    GetStatus = 0,
    ClearFeature = 1,
    SetFeature = 3,
    SetAddress = 5,
    GetDescriptor = 6,
    SetDescriptor = 7,
    GetConfiguration = 8,
    SetConfiguration = 9,
    GetInterface = 10,
    SetInterface = 11,
    SynchFrame = 12,
    SetEncryption = 13,
    GetEncryption = 14,
    SetHandshake = 15,
    GetHandshake = 16,
    SetConnection = 17,
    SetSecurityData = 18,
    GetSecurityData = 19,
    SetWusbData = 20,
    LoopbackDataWrite = 21,
    LoopbackDataRead = 22,
    SetInterfaceDs = 23,
    GetFwStatus = 26,
    SetFwStatus = 27,
    SetSel = 48,
    SetIsochDelay = 49,
    RESERVED,
}

impl From<bRequestStandard> for bRequest {
    fn from(value: bRequestStandard) -> Self {
        Self::Standard(value)
    }
}

#[allow(non_camel_case_types)]
#[repr(C, packed)]
#[derive(Debug, Clone)]
pub struct bmRequestType {
    pub direction: Direction,
    pub transfer_type: DataTransferType,
    pub recipient: Recipient,
}

impl bmRequestType {
    pub fn new(
        direction: Direction,
        transfer_type: DataTransferType,
        recipient: Recipient,
    ) -> bmRequestType {
        bmRequestType {
            direction,
            transfer_type,
            recipient,
        }
    }
}

impl From<bmRequestType> for u8 {
    fn from(value: bmRequestType) -> Self {
        (value.direction as u8) << 7 | (value.transfer_type as u8) << 5 | value.recipient as u8
    }
}

#[derive(FromPrimitive, Copy, Clone, Debug)]
#[repr(u8)]
pub enum DataTransferType {
    Standard = 0,
    Class = 1,
    Vendor = 2,
    Reserved = 3,
}

#[derive(Copy, Clone, Debug, FromPrimitive)]
#[repr(u8)]
pub enum Recipient {
    Device = 0,
    Interface = 1,
    Endpoint = 2,
    Other = 3,
}

impl ControlTransfer {
    #[inline]
    pub(super) fn set_configuration(c: ConfigurationID, i: InterfaceNumber) -> Self {
        Self {
            request_type: bmRequestType::new(
                Direction::Out,
                DataTransferType::Standard,
                Recipient::Device,
            ),
            request: bRequestStandard::SetConfiguration.into(),
            index: i as _,
            value: c as _,
            data: None,
            response: true,
        }
    }

    #[inline]
    pub(super) fn switch_interface(i: InterfaceNumber, a: AltnativeNumber) -> Self {
        Self {
            request_type: bmRequestType::new(
                Direction::Out,
                DataTransferType::Standard,
                Recipient::Interface,
            ),
            request: bRequestStandard::SetInterface.into(),
            index: a as _,
            value: i as _,
            data: None,
            response: true,
        }
    }
}
