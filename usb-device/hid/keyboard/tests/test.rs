#![cfg(test)]

use std::{hint::spin_loop, thread};

use crab_usb::{Class, DeviceInfo, USBHost};
use keyboard::KeyBoard;
use log::info;

#[tokio::test]
async fn test() {
    env_logger::builder()
        .filter_level(log::LevelFilter::Debug)
        .is_test(true)
        .init();

    let mut host = USBHost::new_libusb();
    let event_handler = host.event_handler();
    let ls = host.device_list().await.unwrap();

    thread::spawn(move || {
        while event_handler.handle_event() {
            spin_loop();
        }
    });

    let mut info: Option<DeviceInfo> = None;

    for device in ls {
        println!("{device}");

        for iface in device.interface_descriptors() {
            println!("  Interface: {:?}", iface.class());

            if KeyBoard::check(&device) {
                info!("Found video interface: {iface:?}");
                info = Some(device);
                break;
            }
        }
    }

    let mut info = info.expect("No device found with HID keyboard interface");

    let device = info.open().await.unwrap();
    info!("Opened device: {device}");

    let mut keyboard = KeyBoard::new(device).await.unwrap();

    

}
