use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use log::{debug, trace, warn};
use mbarrier::wmb;
use xhci::{
    context::{Input32Byte, InputHandler},
    registers::doorbell,
    ring::trb::{
        command,
        event::{Allowed, CommandCompletion, CompletionCode},
    },
};

use crate::{
    Slot,
    err::USBError,
    standard::trans::{
        Direction,
        control::{ControlTransfer, Recipient, Request, RequestType, TransferType},
    },
    wait::WaitMap,
    xhci::{
        Registers, XhciRegisters, append_port_to_route_string,
        context::{DeviceContextList, ScratchpadBufferArray, XhciSlot},
        def::{Dci, SlotId},
        event::EventRing,
        parse_default_max_packet_size_from_port_speed,
        ring::{Ring, TransferRingWaitWeak, TrbData},
    },
};

pub struct Root {
    mmio: XhciRegisters,
    pub event_ring: EventRing,
    pub dev_list: DeviceContextList,
    pub cmd: Ring,
    cmd_wait: WaitMap<CommandCompletion>,
    pub scratchpad_buf_arr: Option<ScratchpadBufferArray>,

    transfer_ring_map: SlotRingMap,
}

impl Root {
    pub fn new(max_slots: usize, reg: XhciRegisters) -> Result<Self, USBError> {
        let cmd = Ring::new_with_len(
            0x1000 / size_of::<TrbData>(),
            true,
            dma_api::Direction::Bidirectional,
        )?;
        let event_ring = EventRing::new()?;

        let cmd_wait = WaitMap::new(cmd.trb_bus_addr_list());

        Ok(Self {
            dev_list: DeviceContextList::new(max_slots)?,
            cmd,
            event_ring,
            scratchpad_buf_arr: None,
            mmio: reg,
            transfer_ring_map: Default::default(),
            cmd_wait,
        })
    }

    fn regs(&self) -> Registers {
        self.mmio.reg()
    }

    async fn wait_cmd_completion(
        &mut self,
        cmd_trb_addr: u64,
    ) -> Result<CommandCompletion, USBError> {
        let c = self.cmd_wait.wait_for_result(cmd_trb_addr).await;
        match c.completion_code() {
            Ok(code) => {
                if matches!(code, CompletionCode::Success) {
                    Ok(c)
                } else {
                    Err(USBError::TransferEventError(code))
                }
            }
            Err(_e) => Err(USBError::Unknown),
        }
    }

    pub fn handle_event(&mut self) {
        let erdp = {
            self.clean_events();
            self.event_ring.erdp()
        };
        {
            let mut regs = self.regs();
            let mut irq = regs.interrupter_register_set.interrupter_mut(0);

            irq.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.clear_event_handler_busy();
            });

            irq.iman.update_volatile(|r| {
                r.clear_interrupt_pending();
            });
        }
    }

    fn clean_events(&mut self) -> usize {
        let mut count = 0;

        while let Some(allowed) = self.event_ring.next() {
            unsafe {
                match allowed {
                    Allowed::CommandCompletion(c) => {
                        let addr = c.command_trb_pointer();
                        trace!("[Command] << {allowed:?} @{addr:X}");
                        self.cmd_wait.set_result(addr, c);
                    }
                    Allowed::PortStatusChange(st) => {
                        debug!("port change: {}", st.port_id());
                    }
                    Allowed::TransferEvent(c) => {
                        let addr = c.trb_pointer();
                        trace!("[Transfer] << {allowed:?} @{addr:X}");
                        debug!("transfer event: {c:?}");
                        let slot_id = c.slot_id().into();
                        let dci = c.endpoint_id().into();
                        if let Some(wait) = self
                            .transfer_ring_map
                            .0
                            .get_mut(&slot_id)
                            .and_then(|map| map.0.get_mut(&dci))
                            && let Err(e) = wait.set_result(c.trb_pointer(), c)
                        {
                            warn!("err: {e}")
                        }
                    }
                    _ => {
                        debug!("unhandled event {allowed:?}");
                    }
                }
            }
            count += 1;
        }

        count
    }

    pub async fn post_cmd(&mut self, trb: command::Allowed) -> Result<CommandCompletion, USBError> {
        let trb_addr = self.cmd.enque_command(trb);
        wmb();
        self.regs()
            .doorbell
            .write_volatile_at(0, doorbell::Register::default());

        self.wait_cmd_completion(trb_addr).await
    }

    async fn device_slot_assignment(&mut self) -> Result<SlotId, USBError> {
        // enable slot
        let result = self
            .post_cmd(command::Allowed::EnableSlot(command::EnableSlot::default()))
            .await?;

        let slot_id = result.slot_id();
        trace!("assigned slot id: {slot_id}");
        Ok(slot_id.into())
    }

    pub async fn new_slot(&mut self, port_idx: usize) -> Result<Box<dyn Slot>, USBError> {
        let slot_id = self.device_slot_assignment().await?;
        debug!("Slot {slot_id} assigned");

        let ctx = self.dev_list.new_ctx(slot_id, 32)?;

        let mut slot = {
            let g = self.mmio.disable_irq_guard();
            let slot = XhciSlot::new(slot_id, ctx, self.mmio.clone());
            let ctrl_dci = 1.into();
            self.transfer_ring_map
                .0
                .entry(slot.id)
                .or_default()
                .0
                .insert(ctrl_dci, slot.ring_wait_weak(ctrl_dci).unwrap());
            drop(g);
            slot
        };

        self.address(&mut slot, port_idx).await?;

        debug!("Slot {slot_id} address complete");
        trace!("control_fetch_control_point_packet_size");

        let data = [0u8; 8];

        slot.control_transfer(ControlTransfer {
            request_type: RequestType::new(
                Direction::In,
                TransferType::Standard,
                Recipient::Device,
            ),
            request: Request::GetDescriptor,
            index: 0,
            value: 1 << 8,
            data: Some((data.as_ptr() as usize, data.len() as _)),
        })
        .await?;

        let packet_size = data.last().map(|&len| if len == 0 { 8u8 } else { len });
        trace!("packet_size: {packet_size:?}");

        Ok(Box::new(slot))
    }

    fn port_speed(&self, port: usize) -> u8 {
        self.regs()
            .port_register_set
            .read_volatile_at(port)
            .portsc
            .port_speed()
    }

    async fn address(&mut self, slot: &mut XhciSlot, port_idx: usize) -> Result<(), USBError> {
        let port_speed = self.port_speed(port_idx);
        let max_packet_size = parse_default_max_packet_size_from_port_speed(port_speed);

        let port_id = port_idx + 1;
        let dci = 1.into();

        let transfer_ring_0_addr = slot.ep_ring_ref(dci).bus_addr();

        trace!("ring0: {transfer_ring_0_addr:#x}");

        let ring_cycle_bit = slot.ep_ring_ref(dci).cycle;

        let mut input = Input32Byte::default();
        let control_context = input.control_mut();
        control_context.set_add_context_flag(0);
        control_context.set_add_context_flag(1);
        for i in 2..32 {
            control_context.clear_drop_context_flag(i);
        }

        let slot_context = input.device_mut().slot_mut();
        slot_context.clear_multi_tt();
        slot_context.clear_hub();
        slot_context.set_route_string(append_port_to_route_string(0, 0)); // for now, not support more hub ,so hardcode as 0.//TODO: generate route string
        slot_context.set_context_entries(1);
        slot_context.set_max_exit_latency(0);
        slot_context.set_root_hub_port_number(port_id as _); //todo: to use port number
        slot_context.set_number_of_ports(0);
        slot_context.set_parent_hub_slot_id(0);
        slot_context.set_tt_think_time(0);
        slot_context.set_interrupter_target(0);
        slot_context.set_speed(port_speed);

        let endpoint_0 = input.device_mut().endpoint_mut(dci.as_usize());
        endpoint_0.set_endpoint_type(xhci::context::EndpointType::Control);
        endpoint_0.set_max_packet_size(max_packet_size);
        endpoint_0.set_max_burst_size(0);
        endpoint_0.set_error_count(3);
        endpoint_0.set_tr_dequeue_pointer(transfer_ring_0_addr);
        if ring_cycle_bit {
            endpoint_0.set_dequeue_cycle_state();
        } else {
            endpoint_0.clear_dequeue_cycle_state();
        }
        endpoint_0.set_interval(0);
        endpoint_0.set_max_primary_streams(0);
        endpoint_0.set_mult(0);
        endpoint_0.set_error_count(3);

        slot.set_input(input);

        wmb();

        let result = self
            .post_cmd(command::Allowed::AddressDevice(
                *command::AddressDevice::new()
                    .set_slot_id(slot.id.into())
                    .set_input_context_pointer(slot.input_bus_addr()),
            ))
            .await?;

        debug!("Address slot ok {result:?}");

        Ok(())
    }
}

#[derive(Default)]
struct EpRingMap(BTreeMap<Dci, TransferRingWaitWeak>);

#[derive(Default)]
struct SlotRingMap(BTreeMap<SlotId, EpRingMap>);
