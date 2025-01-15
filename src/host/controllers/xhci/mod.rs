use core::{mem, num::NonZeroUsize};

use alloc::{sync::Arc, vec::Vec};
use context::{DeviceContextList, ScratchpadBufferArray};
use event_ring::EventRing;
use log::{debug, trace};
use ring::Ring;
use xhci::{accessor::Mapper, extended_capabilities::XhciSupportedProtocol};

use crate::{
    abstractions::{PlatformAbstractions, USBSystemConfig},
    host::device::USBDevice,
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
}

impl<'a, O, const _DEVICE_REQUEST_BUFFER_SIZE: usize>
    XHCIController<'a, O, _DEVICE_REQUEST_BUFFER_SIZE>
where
    O: PlatformAbstractions,
{
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
            let ext_list =
                RegistersExtList::new(mmio_base, regs.capability.hccparams1.read(), MemMapper);

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
            }
        }
    }

    async fn init(&mut self) {
        todo!()
    }

    async fn probe(&mut self) {
        todo!()
    }

    fn device_accesses(&self) -> Vec<Arc<USBDevice<_DEVICE_REQUEST_BUFFER_SIZE>>> {
        todo!()
    }

    async fn post_emerge_request(&mut self, req: crate::usb::operations::USBRequest) {
        todo!()
    }
}
