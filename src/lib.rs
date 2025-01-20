#![no_std]
#![feature(
    allocator_api,
    let_chains,
    exclusive_wrapper,
    async_closure,
    ptr_as_ref_unchecked,
    fn_traits,
    cfg_match,
    sync_unsafe_cell,
    generic_const_exprs
)]

use abstractions::{PlatformAbstractions, USBSystemConfig};
use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use driver::driverapi::USBSystemDriverModule;
use futures::{future::join_all, stream, FutureExt};
use host::controllers::{controller_events::EventHandler, Controller};

extern crate alloc;

pub mod abstractions;
pub mod driver;
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
    driver_modules: BTreeMap<String, Box<dyn driver::driverapi::USBSystemDriverModule<'a, O>>>,
    functional_interfaces: BTreeMap<
        String,
        Vec<Box<dyn driver::driverapi::USBSystemDriverModuleInstanceFunctionalInterface<'a, O>>>,
    >,
}

impl<'a, O> USBSystem<'a, O>
where
    O: PlatformAbstractions + 'static,
    'a: 'static,
    [(); O::RING_BUFFER_SIZE]:,
{
    pub fn new(config: USBSystemConfig<O>) -> Self {
        let config: Arc<USBSystemConfig<O>> = config.into();
        let controller: Box<dyn Controller<'a, O>> =
            host::controllers::initialize_controller(config.clone());

        USBSystem {
            config,
            controller,
            driver_modules: BTreeMap::new(),
            functional_interfaces: BTreeMap::new(),
        }
    }

    pub fn plug_driver_module(
        &mut self,
        name: String,
        mut module: Box<dyn driver::driverapi::USBSystemDriverModule<'a, O>>,
    ) -> &mut Self {
        module.as_mut().preload_module(); //add some hooks?
        self.driver_modules.insert(name, module);

        self
    }

    pub fn stage_1_start_controller(&self) -> &Self {
        self.controller.init();
        self
    }

    pub fn stage_2_register_basic_event_handlers_aka_glue_between_use_layer_and_controller_layer(
        &'a self,
    ) -> &'a Self {
        self.controller
            .register_event_handler(EventHandler::NewAssignedDevice(Box::new(|dev| {
                self.driver_modules
                    .values()
                    .filter_map(|module| module.should_active(&dev, &self.config))
                    .for_each(|function| {
                        //todo!
                    });
            })));
        self
    }

    pub async fn stage_3_initial_controller_polling_and_deivces(&self) -> &Self {
        join_all(
            self.controller
                .device_accesses()
                .iter()
                .map(|device| device.request_assign())
                .collect::<Vec<_>>(),
        )
        .await;

        self
    }

    pub async fn run(&self) {}
}
