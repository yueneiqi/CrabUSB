#![no_std]
#![feature(
    allocator_api,
    let_chains,
    exclusive_wrapper,
    async_closure,
    ptr_as_ref_unchecked,
    fn_traits,
    sync_unsafe_cell,
    future_join,
    generic_const_exprs,
    never_type
)]

#[macro_use(match_cfg)]
extern crate match_cfg;

use abstractions::{PlatformAbstractions, USBSystemConfig};
use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use async_lock::{OnceCell, RwLock};
use driver::driverapi::USBSystemDriverModule;
use event::EventBus;
use futures::{
    future::{join, join_all},
    join, FutureExt,
};
use host::controllers::Controller;
use lazy_static::lazy_static;
use usb::functional_interface::USBLayer;

extern crate alloc;

pub mod abstractions;
pub mod driver;
pub mod event;
mod host;
pub mod usb;

pub struct USBSystem<'a, O>
where
    O: PlatformAbstractions + 'static,
    'a: 'static,
    [(); O::RING_BUFFER_SIZE]:,
{
    config: Arc<USBSystemConfig<O>>,
    controller: Box<dyn Controller<'a, O>>,
    usb_layer: USBLayer<'a, O>,
    event_bus: Arc<EventBus<'a, O>>,
}

impl<'a, O> USBSystem<'a, O>
where
    O: PlatformAbstractions + 'static,
    'a: 'static,
    [(); O::RING_BUFFER_SIZE]:,
{
    pub fn new(config: USBSystemConfig<O>) -> Self {
        let config: Arc<USBSystemConfig<O>> = config.into();
        let event_bus = Arc::new(EventBus::new());
        let controller: Box<dyn Controller<'a, O>> =
            host::controllers::initialize_controller(config.clone(), event_bus.clone());
        let usb_layer = USBLayer::new(config.clone(), event_bus.clone());

        USBSystem {
            config,
            controller,
            usb_layer,
            event_bus,
        }
    }

    pub fn plug_driver_module(
        &mut self,
        name: String,
        mut module: Box<dyn driver::driverapi::USBSystemDriverModule<'a, O>>,
    ) -> &mut Self {
        module.as_mut().preload_module(); //add some hooks?
        self.usb_layer.driver_modules.insert(name, module);

        self
    }

    pub fn stage_1_start_controller(&self) -> &Self {
        self.controller.init();
        self
    }

    pub fn stage_2_initialize_usb_layer(&'a self) -> &'a Self {
        self.event_bus.new_initialized_device.subscribe(|dev| {
            self.usb_layer.new_device_initialized(dev);
            squeak::Response::StaySubscribed
        });

        //TODO: more, like device descruction.etc
        self
    }

    pub async fn stage_3_initial_controller_polling_and_deivces(&self) -> &Self {
        join_all(
            self.controller
                .device_accesses()
                .iter()
                .map(|device| {
                    device.request_assign().then(|_| async {
                        self.event_bus
                            .new_initialized_device
                            .broadcast(device.clone());
                    })
                })
                .collect::<Vec<_>>(),
        )
        .await;

        self
    }

    pub async fn async_run(&'a self) {
        //TODO structure run logic
        // join(self.controller.workaround(), self.usb_layer.workaround()).await
        self.controller.workaround().await;
    }
}
