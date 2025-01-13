use core::num::NonZeroUsize;

use alloc::sync::Arc;
use context::{DeviceContextList, ScratchpadBufferArray};
use event_ring::EventRing;
use ring::Ring;
use xhci::{accessor::Mapper, extended_capabilities::XhciSupportedProtocol};

use crate::abstractions::{PlatformAbstractions, USBSystemConfig};

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

pub struct XHCIInner<O, const _DEVICE_REQUEST_BUFFER_SIZE: usize>
where
    O: PlatformAbstractions,
{
    config: Arc<USBSystemConfig<O, _DEVICE_REQUEST_BUFFER_SIZE>>,
    pub regs: RegistersBase,
    pub ext_list: Option<RegistersExtList>,
    max_slots: u8,
    max_ports: u8,
    max_irqs: u16,
    scratchpad_buf_arr: Option<ScratchpadBufferArray<O>>,
    cmd: Ring<O>,
    event: EventRing<O>,
    pub dev_ctx: DeviceContextList<O, _DEVICE_REQUEST_BUFFER_SIZE>,
}
