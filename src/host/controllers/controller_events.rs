use alloc::{boxed::Box, sync::Arc, vec::Vec};

use crate::{abstractions::PlatformAbstractions, host::device::USBDevice};

pub type DeviceEventHandler<'a, const RING_BUFFER_SIZE: usize> =
    dyn Fn(Arc<USBDevice<'a, RING_BUFFER_SIZE>>) + Sync + Send;

pub enum EventHandler<'controller_life, O>
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    NewAssignedDevice(Box<DeviceEventHandler<'controller_life, { O::RING_BUFFER_SIZE }>>),
    DeviceDrop(Box<DeviceEventHandler<'controller_life, { O::RING_BUFFER_SIZE }>>),
}

pub struct EventHandlerTable<'controller_life, O>
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    new_assigned_device_event_handlers:
        Vec<Box<DeviceEventHandler<'controller_life, { O::RING_BUFFER_SIZE }>>>,
    drop_device_event_handlers:
        Vec<Box<DeviceEventHandler<'controller_life, { O::RING_BUFFER_SIZE }>>>,
}

impl<'controller_life, O> EventHandlerTable<'controller_life, O>
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    pub fn new() -> Self {
        Self {
            new_assigned_device_event_handlers: Vec::new(),
            drop_device_event_handlers: Vec::new(),
        }
    }

    pub fn register(&mut self, handler: EventHandler<'controller_life, O>) {
        match handler {
            EventHandler::NewAssignedDevice(f) => self.new_assigned_device_event_handlers.push(f),
            EventHandler::DeviceDrop(f) => self.drop_device_event_handlers.push(f),
        }
    }

    pub fn new_assigned_device(
        &self,
        dev: Arc<USBDevice<'controller_life, { O::RING_BUFFER_SIZE }>>,
    ) {
        self.new_assigned_device_event_handlers
            .iter()
            .for_each(|f| f(dev.clone()));
    }
}
