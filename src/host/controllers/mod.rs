///host layer
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use async_lock::RwLock;

use crate::abstractions::{PlatformAbstractions, USBSystemConfig};

use super::device::USBDevice;

pub trait Controller<'a, O, const DEVICE_BUFFER_SIZE: usize>: Send
where
    O: PlatformAbstractions,
{
    fn new(config: Arc<USBSystemConfig<O, DEVICE_BUFFER_SIZE>>) -> Self
    where
        Self: Sized;

    fn init(&self);

    /// each device should able to access actual transfer function in controller
    fn device_accesses(&self) -> &Vec<Arc<USBDevice<'a, DEVICE_BUFFER_SIZE>>>;
}

cfg_match! {
    cfg(feature = "backend-xhci")=>{
        mod xhci;

        pub fn initialize_controller<'a, O, const DEVICE_BUFFER_SIZE: usize>(
            config: Arc<USBSystemConfig<O, DEVICE_BUFFER_SIZE>>,
        ) -> Box<dyn Controller<'a, O, DEVICE_BUFFER_SIZE>>
        where
            O: PlatformAbstractions+'static,
            'a:'static//wtf
        {
            Box::new(xhci::XHCIController::new(config))
        }
    }
    _=>{
        pub fn initialize_controller<'a, O, const DEVICE_BUFFER_SIZE: usize>(
            config: Arc<USBSystemConfig<O, DEVICE_BUFFER_SIZE>>,
        ) -> Box<dyn Controller<'a, O, DEVICE_BUFFER_SIZE>>
        where
            O: PlatformAbstractions+'static,
            'a:'static//wtf
        {
            Box::new(DummyController::new(config))
        }
    }
}

struct DummyController;

impl<'a, O, const DEVICE_BUFFER_SIZE: usize> Controller<'a, O, DEVICE_BUFFER_SIZE>
    for DummyController
where
    O: PlatformAbstractions,
{
    fn new(_config: Arc<USBSystemConfig<O, DEVICE_BUFFER_SIZE>>) -> Self
    where
        Self: Sized,
    {
        panic!("dummy controller")
    }

    fn init(&self) {
        panic!("dummy controller")
    }

    fn device_accesses(&self) -> &Vec<Arc<USBDevice<'a, DEVICE_BUFFER_SIZE>>> {
        panic!("dummy controller")
    }
}
