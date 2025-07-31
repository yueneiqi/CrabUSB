use core::num::NonZero;

use alloc::{string::String, sync::Arc, vec::Vec};
use dma_api::DSliceMut;
use log::{debug, trace};
use mbarrier::mb;
use xhci::{
    context::InputHandler,
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
            DESCRIPTOR_LEN_DEVICE, DESCRIPTOR_TYPE_DEVICE, DESCRIPTOR_TYPE_STRING,
            DeviceDescriptor, decode_string_descriptor, language_id::US_ENGLISH,
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
        let max_packet_size = self.control_max_packet_size().await?;

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
            "ctrl ring: {ctrl_ring_addr:x?}, port speed: {port_speed}, max packet size: {max_packet_size}"
        );

        let ring_cycle_bit = self.ctx.ctrl_ring().cycle;

        // 1. Allocate an Input Context data structure (6.2.5) and initialize all fields to
        // ‘0’.
        self.with_empty_input(|input| {
            let control_context = input.control_mut();
            // Initialize the Input Control Context (6.2.5.1) of the Input Context by
            // setting the A0 and A1 flags to ‘1’. These flags indicate that the Slot
            // Context and the Endpoint 0 Context of the Input Context are affected by
            // the command.
            control_context.set_add_context_flag(0);
            control_context.set_add_context_flag(1);
            for i in 2..32 {
                control_context.clear_drop_context_flag(i);
            }

            // Initialize the Input Slot Context data structure (6.2.2).
            // • Root Hub Port Number = Topology defined.
            // • Route String = Topology defined. Refer to section 8.9 in the USB3 spec. Note
            // that the Route String does not include the Root Hub Port Number.
            // • Context Entries = 1.
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

            // Initialize the Input default control Endpoint 0 Context (6.2.3).
            let endpoint_0 = input.device_mut().endpoint_mut(dci);
            // • EP Type = Control.
            endpoint_0.set_endpoint_type(xhci::context::EndpointType::Control);
            // • Max Packet Size = The default maximum packet size for the Default Control Endpoint,
            //   as function of the PORTSC Port Speed field.
            endpoint_0.set_max_packet_size(max_packet_size);
            // • Max Burst Size = 0.
            endpoint_0.set_max_burst_size(0);
            // • TR Dequeue Pointer = Start address of first segment of the Default Control
            //   Endpoint Transfer Ring.
            endpoint_0.set_tr_dequeue_pointer(ctrl_ring_addr.raw());
            // • Dequeue Cycle State (DCS) = 1. Reflects Cycle bit state for valid TRBs written
            //   by software.
            if ring_cycle_bit {
                endpoint_0.set_dequeue_cycle_state();
            } else {
                endpoint_0.clear_dequeue_cycle_state();
            }
            // • Interval = 0.
            endpoint_0.set_interval(0);
            // • Max Primary Streams (MaxPStreams) = 0.
            endpoint_0.set_max_primary_streams(0);
            // • Mult = 0.
            endpoint_0.set_mult(0);
            // • Error Count (CErr) = 3.
            endpoint_0.set_error_count(3);
        });

        mb();

        let input_bus_addr = self.ctx.input_bus_addr();
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

    fn modify_input<F>(&self, f: F)
    where
        F: FnOnce(&mut dyn InputHandler),
    {
        unsafe {
            let data = &mut *self.ctx.data.get();
            data.with_input(f);
        }
    }

    fn with_empty_input<F>(&self, f: F)
    where
        F: FnOnce(&mut dyn InputHandler),
    {
        unsafe {
            let data = &mut *self.ctx.data.get();
            data.with_empty_input(f);
        }
    }

    async fn control_max_packet_size(&mut self) -> Result<u16, USBError> {
        trace!("control_fetch_control_point_packet_size");

        let mut data = [0u8; 8];

        self.get_descriptor(DESCRIPTOR_TYPE_DEVICE, 0, 0, &mut data)
            .await?;

        // USB 设备描述符的 bMaxPacketSize0 字段（偏移 7）
        // 对于控制端点，这是直接的字节数值，不需要解码
        let packet_size = data
            .get(7) // bMaxPacketSize0 在设备描述符的第8个字节（索引7）
            .map(|&len| if len == 0 { 8u8 } else { len })
            .unwrap_or(8);

        trace!("Device descriptor bMaxPacketSize0: {packet_size} bytes");

        Ok(packet_size as _)
    }

    async fn set_ep_packet_size(&self, dci: Dci, max_packet_size: u16) -> Result<(), USBError> {
        self.modify_input(|input| {
            let endpoint = input.device_mut().endpoint_mut(dci.as_usize());
            endpoint.set_max_packet_size(max_packet_size);
        });

        mb();

        let _ = self
            .root
            .post_cmd(command::Allowed::EvaluateContext(
                *command::EvaluateContext::default()
                    .set_slot_id(self.id.into())
                    .set_input_context_pointer(self.ctx.input_bus_addr()),
            ))
            .await?;

        debug!(
            "EvaluateContext success, packet size {max_packet_size:?}{}",
            if max_packet_size == 9 { "(512)" } else { "" }
        );

        Ok(())
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

    pub async fn string_descriptor(
        &mut self,
        index: NonZero<u8>,
        language_id: u16,
    ) -> Result<String, USBError> {
        let mut data = alloc::vec![0u8; 256];
        self.get_descriptor(DESCRIPTOR_TYPE_STRING, index.get(), language_id, &mut data)
            .await?;
        decode_string_descriptor(&data).map_err(|_| USBError::Unknown)
    }
}
