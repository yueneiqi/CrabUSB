use crab_usb::{
    Class, Device, DeviceInfo, Direction, EndpointInterruptIn, EndpointInterruptOut, EndpointType,
    Interface, err::USBError,
};
use log::debug;

pub struct KeyBoard {
    device: Device,
    interface: Interface,
    ep_in: EndpointInterruptIn,
    ep_out: EndpointInterruptOut,
}

impl KeyBoard {
    pub fn check(info: &DeviceInfo) -> bool {
        for iface in info.interface_descriptors() {
            if matches!(iface.class(), Class::Hid) && iface.subclass == 1 && iface.protocol == 1 {
                return true;
            }
        }
        false
    }

    pub async fn new(mut device: Device) -> Result<Self, USBError> {
        let config = &device.configurations[0];
        let interface = config
            .interfaces
            .iter()
            .find(|iface| {
                let iface = iface.first_alt_setting();
                matches!(iface.class(), Class::Hid) && iface.subclass == 1 && iface.protocol == 1
            })
            .ok_or(USBError::NotFound)?
            .first_alt_setting();

        debug!("Using interface: {interface:?}");

        let mut interface = device
            .claim_interface(interface.interface_number, interface.alternate_setting)
            .await?;

        let mut ep_in = None;
        let mut ep_out = None;

        for endpoint in interface.descriptor.endpoints.clone().into_iter() {
            match (endpoint.transfer_type, endpoint.direction) {
                (EndpointType::Interrupt, Direction::In) => {
                    debug!("Found interrupt IN endpoint: {endpoint:?}");
                    ep_in = Some(interface.endpoint_interrupt_in(endpoint.address)?);
                }
                (EndpointType::Interrupt, Direction::Out) => {
                    debug!("Found interrupt OUT endpoint: {endpoint:?}");
                    ep_out = Some(interface.endpoint_interrupt_out(endpoint.address)?);
                }
                _ => {
                    debug!("Ignoring endpoint: {endpoint:?}");
                }
            }
        }

        Ok(Self {
            device,
            interface,
            ep_in: ep_in.ok_or(USBError::NotFound)?,
            ep_out: ep_out.ok_or(USBError::NotFound)?,
        })
    }
}
