use core::{
    error::Error,
    future::Future,
    mem,
    num::NonZeroUsize,
    sync::atomic::{fence, Ordering},
};

use alloc::{borrow::ToOwned, collections::btree_map::BTreeMap, sync::Arc, vec::Vec};
use async_lock::{Mutex, MutexGuardArc, OnceCell, RwLock};
use context::{DeviceContextList, ScratchpadBufferArray};
use event_ring::EventRing;
use log::{debug, error, info, trace, warn};
use ring::Ring;
use xhci::{
    accessor::Mapper,
    extended_capabilities::XhciSupportedProtocol,
    ring::trb::{
        command,
        event::{self, CommandCompletion, CompletionCode},
    },
};

use crate::{
    abstractions::{PlatformAbstractions, USBSystemConfig},
    host::device::USBDevice,
    usb::operations::{CallbackValue, RequestResult, USBRequest},
};

use super::Controller;

mod context;
mod event_ring;
mod ring;

pub type RegistersBase = xhci::Registers<MemMapper>;
pub type RegistersExtList = xhci::extended_capabilities::List<MemMapper>;
pub type SupportedProtocol = XhciSupportedProtocol<MemMapper>;

const TAG: &str = "[XHCI]";

#[derive(Clone)]
pub struct MemMapper;
impl Mapper for MemMapper {
    unsafe fn map(&mut self, phys_start: usize, _bytes: usize) -> NonZeroUsize {
        return NonZeroUsize::new_unchecked(phys_start);
    }
    fn unmap(&mut self, _virt_start: usize, _bytes: usize) {}
}

pub struct XHCIController<'a, O, const _DEVICE_REQUEST_BUFFER_SIZE: usize>
//had to poll controller it self!
where
    O: PlatformAbstractions,
{
    config: Arc<USBSystemConfig<O, _DEVICE_REQUEST_BUFFER_SIZE>>,
    regs: RegistersBase,
    ext_list: Option<RegistersExtList>,
    max_slots: u8,
    max_ports: u8,
    max_irqs: u16,
    scratchpad_buf_arr: Option<ScratchpadBufferArray<O>>,
    cmd: Ring<O>,
    event: EventRing<O>,
    dev_ctx: DeviceContextList<O, _DEVICE_REQUEST_BUFFER_SIZE>,
    devices: Vec<Arc<USBDevice<'a, _DEVICE_REQUEST_BUFFER_SIZE>>>,
    jobs: BTreeMap<usize, CallbackValue>,
}

impl<'a, O, const _DEVICE_REQUEST_BUFFER_SIZE: usize>
    XHCIController<'a, O, _DEVICE_REQUEST_BUFFER_SIZE>
where
    O: PlatformAbstractions,
{
    fn chip_hardware_reset(&mut self) -> &mut Self {
        debug!("{TAG} Reset begin");
        debug!("{TAG} Stop");

        self.regs.operational.usbcmd.update_volatile(|c| {
            c.clear_run_stop();
        });
        debug!("{TAG} Until halt");
        while !self.regs.operational.usbsts.read_volatile().hc_halted() {}
        debug!("{TAG} Halted");

        let mut o = &mut self.regs.operational;
        // debug!("xhci stat: {:?}", o.usbsts.read_volatile());

        debug!("{TAG} Wait for ready...");
        while o.usbsts.read_volatile().controller_not_ready() {}
        debug!("{TAG} Ready");

        o.usbcmd.update_volatile(|f| {
            f.set_host_controller_reset();
        });

        while o.usbcmd.read_volatile().host_controller_reset() {}

        debug!("{TAG} Reset HC");

        while self
            .regs
            .operational
            .usbcmd
            .read_volatile()
            .host_controller_reset()
            || self
                .regs
                .operational
                .usbsts
                .read_volatile()
                .controller_not_ready()
        {}

        info!("{TAG} XCHI reset ok");
        self
    }

    fn set_max_device_slots(&mut self) -> &mut Self {
        let max_slots = self.max_slots;
        debug!("{TAG} Setting enabled slots to {}.", max_slots);
        self.regs.operational.config.update_volatile(|r| {
            r.set_max_device_slots_enabled(max_slots);
        });
        self
    }

    fn set_dcbaap(&mut self) -> &mut Self {
        let dcbaap = self.dev_ctx.dcbaap();
        debug!("{TAG} Writing DCBAAP: {:X}", dcbaap);
        self.regs.operational.dcbaap.update_volatile(|r| {
            r.set(dcbaap as u64);
        });
        self
    }

    fn set_cmd_ring(&mut self) -> &mut Self {
        let crcr = self.cmd.register();
        let cycle = self.cmd.cycle;

        let regs = &mut self.regs;

        debug!("{TAG} Writing CRCR: {:X}", crcr);
        regs.operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(crcr);
            if cycle {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });

        self
    }

    fn init_ir(&mut self) -> &mut Self {
        debug!("{TAG} Disable interrupts");
        let regs = &mut self.regs;

        regs.operational.usbcmd.update_volatile(|r| {
            r.clear_interrupter_enable();
        });

        let mut ir0 = regs.interrupter_register_set.interrupter_mut(0);
        {
            debug!("{TAG} Writing ERSTZ");
            ir0.erstsz.update_volatile(|r| r.set(1));

            let erdp = self.event.erdp();
            debug!("{TAG} Writing ERDP: {:X}", erdp);

            ir0.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
            });

            let erstba = self.event.erstba();
            debug!("{TAG} Writing ERSTBA: {:X}", erstba);

            ir0.erstba.update_volatile(|r| {
                r.set(erstba);
            });
            ir0.imod.update_volatile(|im| {
                im.set_interrupt_moderation_interval(0);
                im.set_interrupt_moderation_counter(0);
            });

            debug!("{TAG} Enabling primary interrupter.");
            ir0.iman.update_volatile(|im| {
                im.set_interrupt_enable();
            });
        }

        self
    }

    fn setup_scratchpads(&mut self) -> &mut Self {
        let scratchpad_buf_arr = {
            let buf_count = {
                let count = self
                    .regs
                    .capability
                    .hcsparams2
                    .read_volatile()
                    .max_scratchpad_buffers();
                debug!("{TAG} Scratch buf count: {}", count);
                count
            };
            if buf_count == 0 {
                error!("buf count=0,is it a error?");
                return self;
            }
            let scratchpad_buf_arr = ScratchpadBufferArray::new(buf_count, self.config.os.clone());

            self.dev_ctx.dcbaa[0] = scratchpad_buf_arr.register() as u64;

            debug!(
                "{TAG} Setting up {} scratchpads, at {:#0x}",
                buf_count,
                scratchpad_buf_arr.register()
            );
            scratchpad_buf_arr
        };

        self.scratchpad_buf_arr = Some(scratchpad_buf_arr);
        self
    }

    fn reset_ports(&mut self) -> &mut Self {
        //TODO: reset usb 3 port
        let regs = &mut self.regs;
        let port_len = regs.port_register_set.len();

        for i in 0..port_len {
            debug!("{TAG} Port {} start reset", i,);
            regs.port_register_set.update_volatile_at(i, |port| {
                port.portsc.set_0_port_enabled_disabled();
                port.portsc.set_port_reset();
            });

            while regs
                .port_register_set
                .read_volatile_at(i)
                .portsc
                .port_reset()
            {}

            debug!("{TAG} Port {} reset ok", i);
        }
        self
    }

    fn start(&mut self) -> &mut Self {
        let regs = &mut self.regs;
        debug!("{TAG} Start run");
        regs.operational.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });

        while regs.operational.usbsts.read_volatile().hc_halted() {}

        info!("{TAG} Is running");

        regs.doorbell.update_volatile_at(0, |r| {
            r.set_doorbell_stream_id(0);
            r.set_doorbell_target(0);
        });

        self
    }

    // async fn test_cmd(&mut self) -> &mut Self {
    //     //TODO:assert like this in runtime if build with debug mode?
    //     debug!("{TAG} Test command ring");
    //     for _ in 0..3 {
    //         let completion = self
    //             .post_cmd(command::Allowed::Noop(command::Noop::new()))
    //             .unwrap();
    //     }
    //     debug!("{TAG} Command ring ok");
    //     self
    // }

    async fn post_cmd(&mut self, trb: command::Allowed) -> Result<RequestResult, usize> {
        let addr = self.cmd.enque_command(trb);
        let cell = Arc::new(OnceCell::new());

        self.jobs.insert(addr, cell.clone());

        self.regs.doorbell.update_volatile_at(0, |r| {
            r.set_doorbell_stream_id(0);
            r.set_doorbell_target(0);
        });

        fence(Ordering::Release);

        cell.wait().await.to_owned()
    }

    #[allow(unused_variables)]
    async fn on_event_arrived(&mut self) {
        while let Some((event, cycle)) = self.event.next() {
            debug!("{TAG}:[CMD] received event:{:?},cycle{cycle}", event);

            match event {
                event::Allowed::TransferEvent(transfer_event) => {
                    let addr = transfer_event.trb_pointer() as _;
                    //todo: transfer event trb had extra info compare to command event., should we split these two?

                    match self.jobs.remove(&addr) {
                        Some(v) => {
                            let _ = v
                                .set(
                                    transfer_event
                                        .completion_code()
                                        .map(|a| a.into())
                                        .map_err(|a| a as _),
                                )
                                .await;
                        }
                        None => {
                            warn!("{addr} is unhandled transfer completeion!")
                        }
                    }
                }
                event::Allowed::CommandCompletion(command_completion) => {
                    let addr = command_completion.command_trb_pointer() as _;

                    match self.jobs.remove(&addr) {
                        Some(v) => {
                            let _ = v
                                .set(
                                    command_completion
                                        .completion_code()
                                        .map(|a| a.into())
                                        .map_err(|a| a as _),
                                )
                                .await;
                        }
                        None => {
                            warn!("{addr} is unhandled command completeion!")
                        }
                    }
                }
                event::Allowed::PortStatusChange(port_status_change) => {
                    warn!("{TAG} port status changed! {:#?}", port_status_change);
                }
                event::Allowed::BandwidthRequest(bandwidth_request) => todo!(),
                event::Allowed::Doorbell(doorbell) => todo!(),
                event::Allowed::HostController(host_controller) => todo!(),
                event::Allowed::DeviceNotification(device_notification) => todo!(),
                event::Allowed::MfindexWrap(mfindex_wrap) => todo!(),
            }
        }

        self.regs
            .interrupter_register_set
            .interrupter_mut(0)
            .erdp
            .update_volatile(|f| {
                f.set_event_ring_dequeue_pointer(self.event.erdp());
            });
    }
}

impl<'a, O, const _DEVICE_REQUEST_BUFFER_SIZE: usize> Controller<O, _DEVICE_REQUEST_BUFFER_SIZE>
    for XHCIController<'a, O, _DEVICE_REQUEST_BUFFER_SIZE>
where
    O: PlatformAbstractions,
{
    fn new(config: Arc<USBSystemConfig<O, _DEVICE_REQUEST_BUFFER_SIZE>>) -> Self
    where
        Self: Sized,
    {
        let mmio_base = config.base_addr.clone().into();
        unsafe {
            let regs = RegistersBase::new(mmio_base, MemMapper);
            let ext_list = RegistersExtList::new(
                mmio_base,
                regs.capability.hccparams1.read_volatile(),
                MemMapper,
            );

            let hcsp1 = regs.capability.hcsparams1.read_volatile();
            let max_slots = hcsp1.number_of_device_slots();
            let max_ports = hcsp1.number_of_ports();
            let max_irqs = hcsp1.number_of_interrupts();
            let page_size = regs.operational.pagesize.read_volatile().get();
            debug!(
                "{TAG} Max_slots: {}, max_ports: {}, max_irqs: {}, page size: {}",
                max_slots, max_ports, max_irqs, page_size
            );

            trace!("new dev ctx!");
            let dev_ctx = DeviceContextList::new(config.clone());

            // Create the command ring with 4096 / 16 (TRB size) entries, so that it uses all of the
            // DMA allocation (which is at least a 4k page).
            let entries_per_page = O::PAGE_SIZE / mem::size_of::<ring::TrbData>();
            trace!("new cmd ring");
            let cmd = Ring::new(config.os.clone(), entries_per_page, true);
            trace!("new evt ring");
            let event = EventRing::new(config.os.clone());

            debug!("{TAG} ring size {}", cmd.len());

            Self {
                regs,
                ext_list,
                config: config.clone(),
                max_slots,
                max_ports,
                max_irqs,
                scratchpad_buf_arr: None,
                cmd,
                event,
                dev_ctx,
                devices: Vec::new(),
                jobs: BTreeMap::new(),
            }
        }
    }

    async fn init(&mut self) {
        let controller = self
            .chip_hardware_reset()
            .set_max_device_slots()
            .set_dcbaap()
            .set_cmd_ring()
            .init_ir()
            .setup_scratchpads()
            .start();
    }

    async fn probe(&mut self) {
        todo!()
    }

    fn device_accesses(
        &self,
    ) -> Arc<async_lock::RwLock<Vec<USBDevice<_DEVICE_REQUEST_BUFFER_SIZE>>>> {
        todo!()
    }
}
