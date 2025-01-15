use core::num::NonZeroUsize;

use alloc::{sync::Arc, vec::Vec};
use context::{DeviceContextList, ScratchpadBufferArray};
use event_ring::EventRing;
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
        todo!()
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
