use core::{hint::spin_loop, num::NonZeroUsize, ptr::NonNull, time::Duration};

use alloc::{boxed::Box, vec::Vec};
use context::ScratchpadBufferArray;
use future::LocalBoxFuture;
use futures::{prelude::*, task::AtomicWaker};
use log::*;
use xhci::{
    ExtendedCapability,
    accessor::Mapper,
    extended_capabilities::{self, usb_legacy_support_capability::UsbLegacySupport},
    registers::doorbell,
    ring::trb::{command, event::CommandCompletion},
};

mod context;
mod def;
mod event;
mod ring;
mod root;

use super::{Controller, Slot};
use crate::{err::*, sleep, xhci::root::Root};
use def::*;

type Registers = xhci::Registers<MemMapper>;
// type RegistersExtList = xhci::extended_capabilities::List<MemMapper>;
// type SupportedProtocol = xhci::extended_capabilities::XhciSupportedProtocol<MemMapper>;

#[derive(Clone)]
pub(crate) struct XhciRegisters {
    mmio_base: usize,
}
impl XhciRegisters {
    pub fn new(mmio_base: NonNull<u8>) -> Self {
        Self {
            mmio_base: mmio_base.as_ptr() as usize,
        }
    }

    pub fn reg(&self) -> Registers {
        let mapper = MemMapper {};
        unsafe { Registers::new(self.mmio_base, mapper) }
    }

    pub fn disable_irq_guard(&self) -> DisableIrqGuard {
        let mut reg = self.reg();
        let mut enable = true;
        reg.operational.usbcmd.update_volatile(|r| {
            enable = r.interrupter_enable();
            r.clear_interrupter_enable();
        });
        DisableIrqGuard { reg, enable }
    }
}

pub struct DisableIrqGuard {
    reg: Registers,
    enable: bool,
}
impl Drop for DisableIrqGuard {
    fn drop(&mut self) {
        if self.enable {
            self.reg.operational.usbcmd.update_volatile(|r| {
                r.set_interrupter_enable();
            });
        }
    }
}

pub struct Xhci {
    reg_base: XhciRegisters,
    root: Option<Root>,
    port_wake: AtomicWaker,
}

unsafe impl Send for Xhci {}

impl Controller for Xhci {
    fn init(&mut self) -> LocalBoxFuture<'_, Result> {
        async {
            self.init_ext_caps().await?;
            self.chip_hardware_reset().await?;
            let max_slots = self.setup_max_device_slots();
            self.root = Some(Root::new(max_slots as _, self.reg_base.clone())?);
            self.setup_dcbaap()?;
            self.set_cmd_ring()?;
            self.init_irq()?;
            self.setup_scratchpads()?;
            self.start().await?;
            self.reset_ports();

            Ok(())
        }
        .boxed_local()
    }

    fn test_cmd(&mut self) -> LocalBoxFuture<'_, Result> {
        async {
            self.post_cmd(command::Allowed::Noop(command::Noop::new()))
                .await?;
            Ok(())
        }
        .boxed_local()
    }

    fn handle_irq(&mut self) {
        let mut sts = self.regs().operational.usbsts.read_volatile();
        if sts.event_interrupt() {
            if let Some(root) = self.root.as_mut() {
                root.handle_event();
            } else {
                warn!("[XHCI] Not initialized, cannot handle event");
            }

            sts.clear_event_interrupt();
        }
        if sts.port_change_detect() {
            debug!("Port Change Detected");
            if let Some(data) = self.port_wake.take() {
                data.wake();
            }

            sts.clear_port_change_detect();
        }

        if sts.host_system_error() {
            debug!("Host System Error");
            sts.clear_host_system_error();
        }

        self.regs().operational.usbsts.write_volatile(sts);
    }

    fn probe(&mut self) -> LocalBoxFuture<'_, Result<Vec<Box<dyn Slot>>>> {
        async {
            let mut slots = Vec::new();
            let port_idx_list = self.port_idx_list();

            for idx in port_idx_list {
                let slot = self.root()?.new_slot(idx).await?;
                slots.push(slot);
            }

            Ok(slots)
        }
        .boxed_local()
    }
}

impl Xhci {
    pub fn new(mmio_base: NonNull<u8>) -> Self {
        Self {
            reg_base: XhciRegisters::new(mmio_base),
            root: None,
            port_wake: AtomicWaker::new(),
        }
    }

    fn regs(&self) -> Registers {
        self.reg_base.reg()
    }

    async fn chip_hardware_reset(&mut self) -> Result {
        debug!("Reset begin ...");
        let mut regs = self.regs();
        regs.operational.usbcmd.update_volatile(|c| {
            c.clear_run_stop();
        });

        while !regs.operational.usbsts.read_volatile().hc_halted() {
            sleep(Duration::from_millis(10)).await;
        }

        debug!("Halted");
        let o = &mut regs.operational;
        debug!("Wait for ready...");
        while o.usbsts.read_volatile().controller_not_ready() {
            sleep(Duration::from_millis(10)).await;
        }
        debug!("Ready");

        o.usbcmd.update_volatile(|f| {
            f.set_host_controller_reset();
        });

        debug!("Reset HC");
        while o.usbcmd.read_volatile().host_controller_reset()
            || o.usbsts.read_volatile().controller_not_ready()
        {
            sleep(Duration::from_millis(10)).await;
        }
        debug!("Reset finish");

        debug!("Is 64 bit {}", self.is_64_byte());
        Ok(())
    }

    fn setup_max_device_slots(&mut self) -> u8 {
        let mut regs = self.regs();
        let max_slots = regs
            .capability
            .hcsparams1
            .read_volatile()
            .number_of_device_slots();

        regs.operational.config.update_volatile(|r| {
            r.set_max_device_slots_enabled(max_slots);
        });

        debug!("Max device slots: {max_slots}");

        max_slots
    }

    fn setup_dcbaap(&mut self) -> Result {
        let dcbaa_addr = self.root()?.dev_list.dcbaa.bus_addr();
        debug!("DCBAAP: {dcbaa_addr:X}");
        self.regs().operational.dcbaap.update_volatile(|r| {
            r.set(dcbaa_addr);
        });

        Ok(())
    }

    fn set_cmd_ring(&mut self) -> Result {
        let crcr = self.root()?.cmd.trbs.bus_addr();
        let cycle = self.root()?.cmd.cycle;

        debug!("CRCR: {crcr:X}");
        self.regs().operational.crcr.update_volatile(|r| {
            r.set_command_ring_pointer(crcr);
            if cycle {
                r.set_ring_cycle_state();
            } else {
                r.clear_ring_cycle_state();
            }
        });

        Ok(())
    }

    fn init_irq(&mut self) -> Result {
        debug!("Disable interrupts");
        let mut regs = self.regs();

        regs.operational.usbcmd.update_volatile(|r| {
            r.clear_interrupter_enable();
        });

        let erstz = self.root()?.event_ring.len();
        let erdp = self.root()?.event_ring.erdp();
        let erstba = self.root()?.event_ring.erstba();

        {
            let mut ir0 = regs.interrupter_register_set.interrupter_mut(0);

            debug!("ERDP: {erdp:x}");

            ir0.erdp.update_volatile(|r| {
                r.set_event_ring_dequeue_pointer(erdp);
                r.set_dequeue_erst_segment_index(0);
                r.clear_event_handler_busy();
            });

            debug!("ERSTZ: {erstz:x}");
            ir0.erstsz.update_volatile(|r| r.set(erstz as _));
            debug!("ERSTBA: {erstba:X}");
            ir0.erstba.update_volatile(|r| {
                r.set(erstba);
            });

            ir0.imod.update_volatile(|im| {
                im.set_interrupt_moderation_interval(0x1F);
                im.set_interrupt_moderation_counter(0);
            });
        }

        {
            debug!("Enabling primary interrupter.");
            regs.interrupter_register_set
                .interrupter_mut(0)
                .iman
                .update_volatile(|im| {
                    im.set_interrupt_enable();
                    im.clear_interrupt_pending();
                });
        }

        /* Set the HCD state before we enable the irqs */
        regs.operational.usbcmd.update_volatile(|r| {
            r.set_interrupter_enable();
            r.set_host_system_error_enable();
            r.set_enable_wrap_event();
        });
        Ok(())
    }

    fn setup_scratchpads(&mut self) -> Result {
        let scratchpad_buf_arr = {
            let buf_count = {
                let count = self
                    .regs()
                    .capability
                    .hcsparams2
                    .read_volatile()
                    .max_scratchpad_buffers();
                debug!("Scratch buf count: {count}");
                count
            };
            if buf_count == 0 {
                return Ok(());
            }
            let scratchpad_buf_arr = ScratchpadBufferArray::new(buf_count as _)?;

            let bus_addr = scratchpad_buf_arr.bus_addr();

            self.root()?.dev_list.dcbaa.set(0, bus_addr);

            debug!("Setting up {buf_count} scratchpads, at {bus_addr:#0x}");
            scratchpad_buf_arr
        };

        self.root()?.scratchpad_buf_arr = Some(scratchpad_buf_arr);

        Ok(())
    }

    async fn start(&mut self) -> Result {
        let mut regs = self.regs();
        debug!("Start run");

        regs.operational.usbcmd.update_volatile(|r| {
            r.set_run_stop();
        });

        while regs.operational.usbsts.read_volatile().hc_halted() {
            sleep(Duration::from_millis(10)).await;
        }

        info!("Running");

        regs.doorbell
            .write_volatile_at(0, doorbell::Register::default());

        Ok(())
    }

    async fn post_cmd(&mut self, trb: command::Allowed) -> Result<CommandCompletion> {
        self.root()?.post_cmd(trb).await
    }

    fn reset_ports(&mut self) {
        let regs = &mut self.regs();
        let port_len = regs.port_register_set.len();

        for i in 0..port_len {
            debug!("Port {i} start reset",);
            regs.port_register_set.update_volatile_at(i, |port| {
                port.portsc.set_0_port_enabled_disabled();
                port.portsc.set_port_reset();
            });
        }
        for i in 0..port_len {
            while regs
                .port_register_set
                .read_volatile_at(i)
                .portsc
                .port_reset()
            {
                spin_loop();
            }
        }
    }

    fn extended_capabilities(&self) -> Vec<ExtendedCapability<MemMapper>> {
        let hccparams1 = self.regs().capability.hccparams1.read_volatile();
        let mapper = MemMapper {};
        let mut out = Vec::new();
        let mut l = match unsafe {
            extended_capabilities::List::new(self.reg_base.mmio_base, hccparams1, mapper)
        } {
            Some(v) => v,
            None => return out,
        };

        for one in &mut l {
            if let Ok(cap) = one {
                out.push(cap);
            } else {
                break;
            }
        }
        out
    }

    async fn init_ext_caps(&mut self) -> Result {
        let caps = self.extended_capabilities();
        debug!("Extended capabilities: {:?}", caps.len());

        for cap in self.extended_capabilities() {
            if let ExtendedCapability::UsbLegacySupport(usb_legacy_support) = cap {
                self.legacy_init(usb_legacy_support).await?;
            }
        }

        Ok(())
    }

    async fn legacy_init(&mut self, mut usb_legacy_support: UsbLegacySupport<MemMapper>) -> Result {
        debug!("legacy init");
        usb_legacy_support.usblegsup.update_volatile(|r| {
            r.set_hc_os_owned_semaphore();
        });

        loop {
            sleep(Duration::from_millis(100)).await;
            let up = usb_legacy_support.usblegsup.read_volatile();
            if up.hc_os_owned_semaphore() && !up.hc_bios_owned_semaphore() {
                break;
            }
        }

        debug!("claimed ownership from BIOS");

        usb_legacy_support.usblegctlsts.update_volatile(|r| {
            r.clear_usb_smi_enable();
            r.clear_smi_on_host_system_error_enable();
            r.clear_smi_on_os_ownership_enable();
            r.clear_smi_on_pci_command_enable();
            r.clear_smi_on_bar_enable();

            r.clear_smi_on_bar();
            r.clear_smi_on_pci_command();
            r.clear_smi_on_os_ownership_change();
        });

        Ok(())
    }

    fn port_idx_list(&self) -> Vec<usize> {
        let mut port_idx_list = Vec::new();
        let port_len = self.regs().port_register_set.len();
        for i in 0..port_len {
            let portsc = &self.regs().port_register_set.read_volatile_at(i).portsc;
            info!(
                "Port {}: Enabled: {}, Connected: {}, Speed {}, Power {}",
                i,
                portsc.port_enabled_disabled(),
                portsc.current_connect_status(),
                portsc.port_speed(),
                portsc.port_power()
            );

            if !portsc.port_enabled_disabled() {
                continue;
            }

            port_idx_list.push(i);
        }

        port_idx_list
    }

    fn is_64_byte(&self) -> bool {
        self.regs()
            .capability
            .hccparams1
            .read_volatile()
            .addressing_capability()
    }

    fn root(&mut self) -> Result<&mut Root> {
        self.root.as_mut().ok_or(USBError::NotInitialized)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct MemMapper;
impl Mapper for MemMapper {
    unsafe fn map(&mut self, phys_start: usize, _bytes: usize) -> NonZeroUsize {
        unsafe { NonZeroUsize::new_unchecked(phys_start) }
    }
    fn unmap(&mut self, _virt_start: usize, _bytes: usize) {}
}

fn parse_default_max_packet_size_from_port_speed(speed: u8) -> u16 {
    match speed {
        1 | 3 => 64,
        2 => 8,
        4 => 512,
        v => unimplemented!("PSI: {}", v),
    }
}
fn append_port_to_route_string(route_string: u32, port_id: usize) -> u32 {
    let mut route_string = route_string;
    for tier in 0..5 {
        if route_string & (0x0f << (tier * 4)) == 0 && tier < 5 {
            route_string |= (port_id as u32) << (tier * 4);
            return route_string;
        }
    }

    route_string
}
