///host layer
use alloc::{boxed::Box, sync::Arc, vec::Vec};
use controller_events::EventHandler;

use crate::abstractions::{PlatformAbstractions, USBSystemConfig};

use super::device::USBDevice;

pub mod controller_events;

pub trait Controller<'a, O>: Send + Sync
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    fn new(config: Arc<USBSystemConfig<O>>) -> Self
    where
        Self: Sized;

    fn init(&self);

    /// each device should able to access actual transfer function in controller
    fn device_accesses(&self) -> &Vec<Arc<USBDevice<'a, O>>>;

    fn register_event_handler(&self, register: EventHandler<'a, O>);
}

cfg_match! {
    feature = "backend-xhci"=>{
        mod xhci;

        pub fn initialize_controller<'a, O>(
            config: Arc<USBSystemConfig<O>>,
        ) -> Box<dyn Controller<'a, O>>
        where
        //wtf
            O: PlatformAbstractions+'static,
            'a:'static,
             [(); O::RING_BUFFER_SIZE]:
        {
            Box::new(xhci::XHCIController::new(config))
        }
    }
    _=>{
        pub fn initialize_controller<'a, O>(
            config: Arc<USBSystemConfig<O>>,
        ) -> Box<dyn Controller<'a, O>>
        where
            O: PlatformAbstractions+'static,
            'a:'static//wtf
        {
            Box::new(DummyController::new(config))
        }
    }
}

struct DummyController;

impl<'a, O> Controller<'a, O> for DummyController
where
    O: PlatformAbstractions,
    [(); O::RING_BUFFER_SIZE]:,
{
    fn new(_config: Arc<USBSystemConfig<O>>) -> Self
    where
        Self: Sized,
    {
        panic!("dummy controller")
    }

    fn init(&self) {
        panic!("dummy controller")
    }

    fn device_accesses(&self) -> &Vec<Arc<USBDevice<'a, O>>> {
        panic!("dummy controller")
    }

    fn register_event_handler(&self, register: EventHandler<'a, O>) {
        panic!("dummy controller")
    }
}
