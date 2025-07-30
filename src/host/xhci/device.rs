use alloc::{sync::Arc, vec::Vec};
use dma_api::DSliceMut;
use log::{debug, trace};
use mbarrier::mb;
use xhci::{
    context::{Input32Byte, InputHandler},
    registers::doorbell,
    ring::trb::{
        command,
        event::{CompletionCode, TransferEvent},
        transfer,
    },
};

use crate::{
    BusAddr, IDevice, PortId,
    err::USBError,
    standard::{
        descriptors::{
            DESCRIPTOR_LEN_DEVICE, DESCRIPTOR_TYPE_DEVICE, DeviceDescriptor,
            language_id::US_ENGLISH,
        },
        transfer::{
            Direction,
            control::{Control, ControlRaw, ControlType, Recipient, Request, RequestType},
        },
    },
    wait::WaitMap,
    xhci::{
        append_port_to_route_string,
        context::DeviceContext,
        def::{Dci, SlotId},
        parse_default_max_packet_size_from_port_speed,
        ring::Ring,
        root::RootHub,
    },
};

pub struct Device {
    id: SlotId,
    root: RootHub,
    ctx: Arc<DeviceContext>,
    wait: WaitMap<TransferEvent>,
    port_id: PortId,
    desc: Option<DeviceDescriptor>,
}

impl IDevice for Device {}

impl Device {
    pub(crate) fn new(
        id: SlotId,
        root: &RootHub,
        ctx: Arc<DeviceContext>,
        port_id: PortId,
    ) -> Self {
        Self {
            id,
            root: root.clone(),
            ctx,
            wait: root.transfer_waiter(),
            port_id,
            desc: None,
        }
    }

    pub(crate) async fn init(&mut self) -> Result<(), USBError> {
        trace!("Initializing device with ID: {}", self.id.as_u8());
        // Perform initialization logic here
        self.address().await?;
        let max_packet_size = self.query_packet_size().await?;

        trace!("Max packet size: {max_packet_size}");
        self.set_ep_packet_size(Dci::CTRL, max_packet_size).await?;
        Ok(())
    }

    async fn address(&mut self) -> Result<(), USBError> {
        trace!("Addressing device with ID: {}", self.id.as_u8());
        let port_speed = self.root.lock().port_speed(self.port_id);
        let max_packet_size = parse_default_max_packet_size_from_port_speed(port_speed);

        let ctrl_ring_addr = self.ctx.ctrl_ring().bus_addr();
        // ctrl dci
        let dci = 1;
        trace!(
            "ctrl ring: {ctrl_ring_addr:#x?}, port speed: {port_speed}, max packet size: {max_packet_size}"
        );

        let ring_cycle_bit = self.ctx.ctrl_ring().cycle;

        let mut input = Input32Byte::default();
        let control_context = input.control_mut();

        // 清除所有flags
        for i in 2..32 {
            control_context.clear_add_context_flag(i);
            control_context.clear_drop_context_flag(i);
        }
        // 只设置需要的flags：slot context (0) 和 endpoint 0 context (1)
        control_context.set_add_context_flag(0);
        control_context.set_add_context_flag(1);

        let slot_context = input.device_mut().slot_mut();
        slot_context.clear_multi_tt();
        slot_context.clear_hub();
        slot_context.set_route_string(append_port_to_route_string(0, 0)); // for now, not support more hub ,so hardcode as 0.//TODO: generate route string
        slot_context.set_context_entries(1);
        slot_context.set_max_exit_latency(0);
        slot_context.set_root_hub_port_number(self.port_id.raw() as _); //todo: to use port number
        slot_context.set_number_of_ports(0);
        slot_context.set_parent_hub_slot_id(0);
        slot_context.set_tt_think_time(0);
        slot_context.set_interrupter_target(0);
        slot_context.set_speed(port_speed);

        let endpoint_0 = input.device_mut().endpoint_mut(dci);
        endpoint_0.set_endpoint_type(xhci::context::EndpointType::Control);
        endpoint_0.set_max_packet_size(max_packet_size);
        endpoint_0.set_max_burst_size(0);
        endpoint_0.set_error_count(3);
        endpoint_0.set_tr_dequeue_pointer(ctrl_ring_addr.raw());
        if ring_cycle_bit {
            endpoint_0.set_dequeue_cycle_state();
        } else {
            endpoint_0.clear_dequeue_cycle_state();
        }
        endpoint_0.set_interval(0);
        endpoint_0.set_max_primary_streams(0);
        endpoint_0.set_mult(0);

        self.set_input(input);

        mb();

        let input_bus_addr = self.input_bus_addr();
        trace!("Input context bus address: {input_bus_addr:#x?}");
        let result = self
            .root
            .post_cmd(command::Allowed::AddressDevice(
                *command::AddressDevice::new()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(input_bus_addr),
            ))
            .await?;

        debug!("Address slot ok {result:?}");

        Ok(())
    }

    pub async fn control_in(&mut self, param: Control, buff: &mut [u8]) -> Result<(), USBError> {
        self.control_transfer(ControlRaw {
            request_type: RequestType {
                direction: Direction::In,
                control_type: param.transfer_type,
                recipient: param.recipient,
            },
            request: param.request,
            index: param.index,
            value: param.value,
            data: if buff.is_empty() {
                None
            } else {
                Some((buff.as_mut_ptr() as usize, buff.len() as _))
            },
        })
        .await
    }

    pub async fn control_out(&mut self, param: Control, buff: &[u8]) -> Result<(), USBError> {
        self.control_transfer(ControlRaw {
            request_type: RequestType {
                direction: Direction::Out,
                control_type: param.transfer_type,
                recipient: param.recipient,
            },
            request: param.request,
            index: param.index,
            value: param.value,
            data: if buff.is_empty() {
                None
            } else {
                Some((buff.as_ptr() as usize, buff.len() as _))
            },
        })
        .await
    }

    async fn control_transfer(&mut self, urb: ControlRaw) -> Result<(), USBError> {
        let mut trbs: Vec<transfer::Allowed> = Vec::new();
        let mut setup = transfer::SetupStage::default();

        setup
            .set_request_type(urb.request_type.clone().into())
            .set_request(urb.request.into())
            .set_value(urb.value)
            .set_index(urb.index)
            .set_transfer_type(transfer::TransferType::No);

        let mut data = None;

        if let Some((addr, len)) = urb.data {
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };

            let dm = DSliceMut::from(data_slice, dma_api::Direction::Bidirectional);

            if matches!(urb.request_type.direction, Direction::Out) {
                dm.confirm_write_all();
            }

            setup
                .set_transfer_type(urb.request_type.direction.into())
                .set_length(len);

            let mut raw_data = transfer::DataStage::default();
            raw_data
                .set_data_buffer_pointer(dm.bus_addr() as _)
                .set_trb_transfer_length(len as _)
                .set_direction(urb.request_type.direction.into());

            data = Some(raw_data)
        }

        let mut status = transfer::StatusStage::default();
        status.set_interrupt_on_completion();

        if matches!(urb.request_type.direction, Direction::In) {
            status.set_direction();
        }

        trbs.push(setup.into());
        if let Some(data) = data {
            trbs.push(data.into());
        }
        trbs.push(status.into());

        let ring = self.ctrl_ring_mut();

        let mut trb_ptr = BusAddr(0);

        for trb in trbs {
            trb_ptr = ring.enque_transfer(trb);
        }

        trace!("trb : {trb_ptr:#x?}");

        mb();

        let mut bell = doorbell::Register::default();
        bell.set_doorbell_target(Dci::CTRL.raw());

        self.root.doorbell(self.id, bell);

        let ret = self.wait.try_wait_for_result(trb_ptr.raw()).unwrap().await;

        match ret.completion_code() {
            Ok(code) => {
                if !matches!(code, CompletionCode::Success) {
                    return Err(USBError::TransferEventError(code));
                }
            }
            Err(_e) => return Err(USBError::Unknown),
        }

        if let Some((addr, len)) = urb.data {
            let data_slice =
                unsafe { core::slice::from_raw_parts_mut(addr as *mut u8, len as usize) };

            let dm = DSliceMut::from(data_slice, dma_api::Direction::Bidirectional);

            if matches!(urb.request_type.direction, Direction::In) {
                dm.preper_read_all();
            }
        }
        Ok(())
    }

    pub(crate) fn ctrl_ring_mut(&mut self) -> &mut Ring {
        unsafe {
            let data = &mut *self.ctx.data.get();
            &mut data.transfer_rings[0]
        }
    }

    // pub fn id(&self) -> SlotId {
    //     self.id
    // }

    fn set_input(&self, input: Input32Byte) {
        unsafe {
            let data = &mut *self.ctx.data.get();
            data.input.write(input);
        }
    }

    fn input_bus_addr(&self) -> u64 {
        unsafe {
            let data = &*self.ctx.data.get();
            data.input.bus_addr()
        }
    }

    async fn query_packet_size(&mut self) -> Result<u16, USBError> {
        trace!("control_fetch_control_point_packet_size");

        let mut data = [0u8; 8];

        self.control_in(
            Control {
                request: Request::GetDescriptor,
                index: 0,
                value: 1 << 8,
                transfer_type: ControlType::Standard,
                recipient: Recipient::Device,
            },
            &mut data,
        )
        .await?;

        let packet_size = data
            .last()
            .map(|&len| if len == 0 { 8u8 } else { len })
            .unwrap();
        trace!("packet_size: {packet_size:?}");

        Ok(packet_size as _)
    }

    async fn set_ep_packet_size(&self, dci: Dci, max_packet_size: u16) -> Result<(), USBError> {
        self.modify_input(|input| {
            // 清除所有flags
            let control_context = input.control_mut();
            for i in 0..32 {
                control_context.clear_add_context_flag(i);
            }
            // Drop context flags 只能清除索引 2..=31
            for i in 2..32 {
                control_context.clear_drop_context_flag(i);
            }
            // 设置slot context和要修改的endpoint context flag
            control_context.set_add_context_flag(0); // slot context
            control_context.set_add_context_flag(dci.as_usize()); // endpoint context

            let endpoint = input.device_mut().endpoint_mut(dci.as_usize());
            endpoint.set_max_packet_size(max_packet_size);
        });

        mb();

        let result = self
            .root
            .post_cmd(command::Allowed::EvaluateContext(
                *command::EvaluateContext::default()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(self.input_bus_addr()),
            ))
            .await?;

        debug!("Set packet size ok {result:?}");

        Ok(())
    }

    fn modify_input(&self, f: impl FnOnce(&mut Input32Byte)) {
        unsafe {
            let data = &mut *self.ctx.data.get();
            data.input.modify(f);
        }
    }

    pub async fn descriptor(&mut self) -> Result<DeviceDescriptor, USBError> {
        if let Some(desc) = &self.desc {
            return Ok(desc.clone());
        }
        let mut buff = alloc::vec![0u8; DESCRIPTOR_LEN_DEVICE as usize];
        self.get_descriptor(DESCRIPTOR_TYPE_DEVICE, 0, US_ENGLISH, &mut buff)
            .await?;
        let desc = DeviceDescriptor::new(&buff).ok_or(USBError::Unknown)?;
        self.desc = Some(desc.clone());
        Ok(desc)
    }

    async fn get_descriptor(
        &mut self,
        desc_type: u8,
        desc_index: u8,
        language_id: u16,
        buff: &mut [u8],
    ) -> Result<(), USBError> {
        self.control_in(
            Control {
                request: Request::GetDescriptor,
                index: language_id,
                value: ((desc_type as u16) << 8) | desc_index as u16,
                transfer_type: ControlType::Standard,
                recipient: Recipient::Device,
            },
            buff,
        )
        .await?;
        Ok(())
    }

    // pub async fn string_descriptor(
    //     &mut self,
    //     index: u8,
    //     language_id: u16,
    // ) -> Result<String, USBError> {
    //     let data = self
    //         .get_descriptor(DESCRIPTOR_TYPE_STRING, index, language_id)
    //         .await?;
    //     decode_string_descriptor(&data).map_err(|_| USBError::Unknown)
    // }
}
