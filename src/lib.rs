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
    generic_const_exprs
)]

#[macro_use(match_cfg)]
extern crate match_cfg;

use abstractions::{PlatformAbstractions, USBSystemConfig};
use alloc::{boxed::Box, collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use async_lock::RwLock;
use driver::driverapi::USBSystemDriverModule;
use futures::{
    future::{join, join_all},
    join, FutureExt,
};
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
    functional_interfaces: RwLock<
        BTreeMap<
            &'a str,
            Vec<
                Arc<
                    RwLock<
                        dyn driver::driverapi::USBSystemDriverModuleInstanceFunctionalInterface<
                            'a,
                            O,
                        >,
                    >,
                >,
            >,
        >,
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
            functional_interfaces: BTreeMap::new().into(),
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
            .register_event_handler(EventHandler::NewInitializedDevice(Box::new(|dev| {
                self.driver_modules
                    .values()
                    .filter_map(|module| {
                        module
                            .should_active(&dev, &self.config)
                            .map(|a| (a, module.name()))
                    })
                    .for_each(|(function, name)| {
                        //todo!
                        embassy_futures::block_on(self.functional_interfaces.write())
                            .entry(name)
                            .or_insert(Vec::new())
                            .push(function);
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

    async fn run_inner(&self) {
        //TODO structure run logic
        // join(self.controller.workaround())
    }

    pub async fn run(&self) {
        join!(self.run_inner());
    }
}
