use core::{
    cell::UnsafeCell,
    error::Error,
    future::{self, Future, IntoFuture},
    mem,
    num::NonZeroUsize,
    sync::atomic::{fence, Ordering},
};

use ::futures::{stream, FutureExt, Stream, StreamExt};
use alloc::{borrow::ToOwned, collections::btree_map::BTreeMap, format, sync::Arc, vec::Vec};
use async_lock::{futures, Mutex, MutexGuardArc, OnceCell, RwLock};
use async_ringbuf::{
    traits::{AsyncConsumer, AsyncProducer},
    AsyncStaticCons,
};
use axhid::hidreport::hid::Item;
use context::{DeviceContextList, ScratchpadBufferArray};
use embassy_futures::join::{self, join_array, JoinArray};
use event_ring::EventRing;
use log::{debug, error, info, trace, warn};
use ring::Ring;
use ringbuf::traits::Consumer;
use xhci::{
    accessor::Mapper,
    context::Slot,
    extended_capabilities::XhciSupportedProtocol,
    ring::trb::{
        command,
        event::{self, CommandCompletion, CompletionCode},
        transfer::{self, TransferType},
    },
};

use crate::{
    abstractions::{PlatformAbstractions, USBSystemConfig},
    host::device::USBDevice,
    usb::operations::{
        control::ControlTransfer, CallbackValue, CompleteAction, Direction, ExtraAction,
        RequestResult, USBRequest,
    },
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

pub struct Receiver<'a, const RINGBUF_SIZE: usize> {
    pub slot: u8,
    pub receiver: AsyncStaticCons<'a, USBRequest<'a, RINGBUF_SIZE>, RINGBUF_SIZE>,
}

pub struct XHCIController<'a, O, const RINGBUF_SIZE: usize>
//had to poll controller it self!
where
    O: PlatformAbstractions,
{
    config: Arc<USBSystemConfig<O, RINGBUF_SIZE>>,
    //safety:regs MUST exist in mem otherwise would panic when construct
    regs: UnsafeCell<RegistersBase>,
    ext_list: Option<RegistersExtList>,
    max_slots: u8,
    max_ports: u8,
    max_irqs: u16,
    scratchpad_buf_arr: OnceCell<ScratchpadBufferArray<O>>,
    cmd: Ring<O>,
    event: EventRing<O>,
    dev_ctx: RwLock<DeviceContextList<O, RINGBUF_SIZE>>,
    devices: Vec<Arc<USBDevice<'a, RINGBUF_SIZE>>>,
    requests: UnsafeCell<Vec<Receiver<'a, RINGBUF_SIZE>>>,
    finish_jobs: RwLock<BTreeMap<usize, CompleteAction<'a, RINGBUF_SIZE>>>,
    extra_works: BTreeMap<usize, USBRequest<'a, RINGBUF_SIZE>>,
}

impl<'a, O, const RINGBUF_SIZE: usize> XHCIController<'a, O, RINGBUF_SIZE>
where
    O: PlatformAbstractions,
{
    fn chip_hardware_reset(&self) -> &Self {
        debug!("{TAG} Reset begin");
        debug!("{TAG} Stop");
        let regs = unsafe { self.regs.get().as_mut_unchecked() };

        regs.operational.usbcmd.update_volatile(|c| {
            c.clear_run_stop();
        });
        debug!("{TAG} Until halt");
        while !regs.operational.usbsts.read_volatile().hc_halted() {}
        debug!("{TAG} Halted");

        let o = &mut regs.operational;
        // debug!("xhci stat: {:?}", o.usbsts.read_volatile());

        debug!("{TAG} Wait for ready...");
        while o.usbsts.read_volatile().controller_not_ready() {}
        debug!("{TAG} Ready");

        o.usbcmd.update_volatile(|f| {
            f.set_host_controller_reset();
        });

        while o.usbcmd.read_volatile().host_controller_reset() {}

        debug!("{TAG} Reset HC");

        while regs
            .operational
            .usbcmd
            .read_volatile()
            .host_controller_reset()
            || regs
                .operational
                .usbsts
                .read_volatile()
                .controller_not_ready()
        {}

        info!("{TAG} XCHI reset ok");
        self
    }

    fn set_max_device_slots(&self) -> &Self {
        let max_slots = self.max_slots;
        debug!("{TAG} Setting enabled slots to {}.", max_slots);
        unsafe { self.regs.get().as_mut_unchecked() }
            .operational
            .config
            .update_volatile(|r| {
                r.set_max_device_slots_enabled(max_slots);
            });
        self
    }

    fn set_dcbaap(&self) -> &Self {
        let dcbaap = self
            .dev_ctx
            .try_read()
            .expect("should gurantee exclusive access here")
            .dcbaap();
        debug!("{TAG} Writing DCBAAP: {:X}", dcbaap);
        unsafe { self.regs.get().as_mut_unchecked() }
            .operational
            .dcbaap
            .update_volatile(|r| {
                r.set(dcbaap as u64);
            });
        self
    }

    fn set_cmd_ring(&self) -> &Self {
        let crcr = self.cmd.register();
        let cycle = self.cmd.cycle;

        debug!("{TAG} Writing CRCR: {:X}", crcr);
        unsafe { self.regs.get().as_mut_unchecked() }
            .operational
            .crcr
            .update_volatile(|r| {
                r.set_command_ring_pointer(crcr);
                if cycle {
                    r.set_ring_cycle_state();
                } else {
                    r.clear_ring_cycle_state();
                }
            });

        self
    }

    fn init_ir(&self) -> &Self {
        debug!("{TAG} Disable interrupts");
        let regs = unsafe { self.regs.get().as_mut_unchecked() };

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

    fn setup_scratchpads(&self) -> &Self {
        let scratchpad_buf_arr = {
            let buf_count = {
                let count = unsafe { self.regs.get().as_mut_unchecked() }
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

            {
                let mut read_volatile = unsafe {
                    self.dev_ctx
                        .try_read()
                        .expect("should garantee exclusive access here")
                        .dcbaa
                        .get()
                        .read_volatile()
                };
                read_volatile[0] = scratchpad_buf_arr.register() as u64;
            }

            debug!(
                "{TAG} Setting up {} scratchpads, at {:#0x}",
                buf_count,
                scratchpad_buf_arr.register()
            );
            scratchpad_buf_arr
        };

        self.scratchpad_buf_arr.set(scratchpad_buf_arr);
        self
    }

    fn reset_ports(&self) -> &Self {
        //TODO: reset usb 3 port
        let regs = unsafe { self.regs.get().as_mut_unchecked() };
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

    fn start(&self) -> &Self {
        let regs = unsafe { self.regs.get().as_mut_unchecked() };
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

    fn ring_db(&self, slot: u8, stream: Option<u16>, target: Option<u8>) {
        // might waste efficient? or actually low cost compare to actual transfer(in hardware)
        unsafe { self.regs.get().as_mut_unchecked() }
            .doorbell
            .update_volatile_at(slot as _, |r| {
                stream.map(|stream| r.set_doorbell_stream_id(stream));
                target.map(|target| r.set_doorbell_target(target));
            });
    }

    fn update_erdp(&self) {
        unsafe { self.regs.get().as_mut_unchecked() }
            .interrupter_register_set
            .interrupter_mut(0)
            .erdp
            .update_volatile(|f| {
                f.set_event_ring_dequeue_pointer(self.event.erdp());
            });
    }

    async fn post_cmd(&mut self, trb: command::Allowed) -> Result<RequestResult, u8> {
        let addr = self.cmd.enque_command(trb);
        let cell = Arc::new(OnceCell::new());

        self.finish_jobs
            .write()
            .await
            .insert(addr, CompleteAction::SimpleResponse(cell.clone()));

        self.ring_db(0, 0.into(), 0.into());
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

                    self.mark_completed(transfer_event.completion_code(), addr)
                        .await;
                }
                event::Allowed::CommandCompletion(command_completion) => {
                    let addr = command_completion.command_trb_pointer() as _;

                    self.mark_completed(command_completion.completion_code(), addr)
                        .await;
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

        self.update_erdp();
    }

    async fn mark_completed(&mut self, code: Result<CompletionCode, u8>, addr: usize) {
        //should compile to jump table?
        if self.finish_jobs.read().await.contains_key(&addr) {
            self.finish_jobs
                .write()
                .then(|mut write| async move { write.remove(&addr).unwrap() })
                .then(|action| async move {
                    match action {
                        CompleteAction::NOOP => {}
                        CompleteAction::Callback(_fn_once) => {
                            // fn_once({
                            //     transfer_event
                            //         .completion_code()
                            //         .map(|a| a.into())
                            //         .map_err(|a| a as _)
                            // });had size issue, to solve.
                        }
                        CompleteAction::SimpleResponse(once_cell) => {
                            once_cell
                                .set(code.map(|a| a.into()).map_err(|a| a as _))
                                .await;
                        }
                        CompleteAction::DropSem(configure_semaphore) => {
                            drop(configure_semaphore);
                        }
                        _ => {
                            panic!("keep working request should not appear at here!")
                        }
                    };
                })
                .await
        } else if self.extra_works.contains_key(&addr) {
            match &mut self.extra_works.get_mut(&addr).unwrap().complete_action {
                CompleteAction::KeepResponse(async_wrap) => {
                    async_wrap
                        .push(code.map(|a| a.into()).map_err(|a| a as _))
                        .await;
                }
                _ => panic!("oneshot job appeared at extra works zone!"),
            }
        }
    }

    async fn run_once(&mut self) {
        stream::select_all(unsafe {
            //TODO: rewrite this into yield stream. but not just post once
            self.requests.get().read_volatile().iter_mut().map(|r| {
                r.receiver
                    .pop()
                    .map(|res| res.map(|req| (req, r.slot)))
                    .into_stream()
            })
        })
        .filter_map(|a| async move { a })
        .for_each(|a| self.post_transfer(a.0, a.1))
        .await;
    }

    #[allow(unused_variables)]
    async fn post_transfer(&self, req: USBRequest<'a, RINGBUF_SIZE>, slot: u8) {
        match req.operation {
            crate::usb::operations::RequestedOperation::Control(control_transfer) => {
                let key = self.control_transfer(slot, control_transfer).await;
                self.finish_jobs
                    .write()
                    .await
                    .insert(key, req.complete_action);
            }
            crate::usb::operations::RequestedOperation::Bulk(bulk_transfer) => todo!(),
            crate::usb::operations::RequestedOperation::Interrupt(interrupt_transfer) => todo!(),
            crate::usb::operations::RequestedOperation::Isoch(isoch_transfer) => todo!(),
            crate::usb::operations::RequestedOperation::Assign => todo!(),
            crate::usb::operations::RequestedOperation::NOOP => {
                debug!("{TAG}-device {slot} transfer nope!")
            }
        }
    }

    #[inline]
    async fn control_transfer(&self, slot: u8, urb_req: ControlTransfer) -> usize {
        let direction = urb_req.request_type.direction;
        let buffer = urb_req.data;

        let mut len = 0;
        let data = if let Some((addr, length)) = buffer {
            let mut data = transfer::DataStage::default();
            len = length;
            data.set_data_buffer_pointer(addr as u64)
                .set_trb_transfer_length(len as _)
                .set_direction(direction.into());
            Some(data)
        } else {
            None
        };

        let setup = *transfer::SetupStage::default()
            .set_request_type(urb_req.request_type.into())
            .set_request(urb_req.request.into())
            .set_value(urb_req.value)
            .set_index(urb_req.index)
            .set_transfer_type({
                if buffer.is_some() {
                    match direction {
                        Direction::In => TransferType::In,
                        Direction::Out => TransferType::Out,
                    }
                } else {
                    TransferType::No
                }
            })
            .set_length(len as u16);
        trace!("{:#?}", setup);

        let mut status = *transfer::StatusStage::default().set_interrupt_on_completion();

        if urb_req.response {
            status.set_direction();
        }

        //=====post!=======
        let mut trbs: Vec<transfer::Allowed> = Vec::new();

        trbs.push(setup.into());
        if let Some(data) = data {
            trbs.push(data.into());
        }
        trbs.push(status.into());

        let trb_pointers: Vec<usize> = {
            let mut writer = self.dev_ctx.write().await;
            let ring = writer
                .write_transfer_ring(slot, 0)
                .expect("initialization on transfer rings got some issue, fixit.");
            trbs.into_iter()
                .map(|trb| ring.enque_transfer(trb))
                .collect()
        };

        if trb_pointers.len() == 2 {
            trace!(
                "[Transfer] >> setup@{:#X}, status@{:#X}",
                trb_pointers[0],
                trb_pointers[1]
            );
        } else {
            trace!(
                "[Transfer] >> setup@{:#X}, data@{:#X}, status@{:#X}",
                trb_pointers[0],
                trb_pointers[1],
                trb_pointers[2]
            );
        }

        fence(Ordering::Release);
        self.ring_db(slot, None, 1.into());

        trb_pointers.last().unwrap().to_owned()
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
        return unsafe {
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
                regs: regs.into(),
                ext_list,
                config: config.clone(),
                max_slots,
                max_ports,
                max_irqs,
                scratchpad_buf_arr: OnceCell::new(),
                cmd,
                event,
                dev_ctx: dev_ctx.into(),
                devices: Vec::new(),
                finish_jobs: BTreeMap::new().into(),
                requests: UnsafeCell::new(Vec::new()), //safety: only controller itself could fetch, all acccess via run_once
                extra_works: BTreeMap::new(),
            }
        };
    }

    fn init(&self) {
        self.chip_hardware_reset()
            .set_max_device_slots()
            .set_dcbaap()
            .set_cmd_ring()
            .init_ir()
            .setup_scratchpads()
            .start();
    }

    async fn probe(&self) {
        todo!()
    }

    fn device_accesses(
        &self,
    ) -> Arc<async_lock::RwLock<Vec<USBDevice<_DEVICE_REQUEST_BUFFER_SIZE>>>> {
        todo!()
    }
}
