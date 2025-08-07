use crab_usb::{
    Class, Device, DeviceInfo, Direction, EndpointInterruptIn, EndpointType, Interface,
    err::USBError,
};
use log::debug;

pub struct KeyBoard {
    _device: Device,
    _interface: Interface,
    ep_in: EndpointInterruptIn,
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
        for config in device.configurations.iter() {
            debug!("Configuration: {config:?}");
        }

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

        for endpoint in interface.descriptor.endpoints.clone().into_iter() {
            match (endpoint.transfer_type, endpoint.direction) {
                (EndpointType::Interrupt, Direction::In) => {
                    debug!("Found interrupt IN endpoint: {endpoint:?}");
                    ep_in = Some(interface.endpoint_interrupt_in(endpoint.address)?);
                }

                _ => {
                    debug!("Ignoring endpoint: {endpoint:?}");
                }
            }
        }

        Ok(Self {
            _device: device,
            _interface: interface,
            ep_in: ep_in.ok_or(USBError::NotFound)?,
        })
    }

    pub async fn recv(&mut self) -> Result<Vec<u8>, USBError> {
        let mut buf = vec![0u8; 8];
        let n = self.ep_in.submit(&mut buf)?.await?;
        if n == 0 {
            return Err(USBError::NotFound);
        }
        buf.truncate(n);
        Ok(buf.to_vec())
    }
}


