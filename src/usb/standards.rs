use tock_registers::interfaces::{Readable, Writeable};
use tock_registers::register_bitfields;
///utils according to USB standard
use tock_registers::registers::InMemoryRegister;

/// The Route String is a 20-bit field in downstream directed packets that the hub uses to route
/// each packet to the designated downstream port.  It is composed of a concatenation of the
/// downstream port numbers (4 bits per hub) for each hub traversed to reach a device.  The
/// hub uses a Hub Depth value multiplied by four as an offset into the Route String to locate the
/// bits it uses to determine the downstream port number.  The Hub Depth value is determined
/// and assigned to every hub during the enumeration process.  
register_bitfields![u32,
    RouteString_REG [
        HUB_TIER0 OFFSET(0) NUMBITS(4) [],
        HUB_TIER1 OFFSET(4) NUMBITS(4) [],
        HUB_TIER2 OFFSET(8) NUMBITS(4) [],
        HUB_TIER3 OFFSET(12) NUMBITS(4) [],
        HUB_TIER4 OFFSET(16) NUMBITS(4) [],
        HUB_TIER5 OFFSET(20) NUMBITS(4) [],
        HUB_TIER6 OFFSET(24) NUMBITS(4) [],
        HUB_TIER7 OFFSET(28) NUMBITS(4) [],
    ]
];

#[repr(C)]
pub struct RouteString(InMemoryRegister<u32, RouteString_REG::Register>);

unsafe impl Send for RouteString {}
unsafe impl Sync for RouteString {}

impl RouteString {
    pub fn new() -> Self {
        Self(InMemoryRegister::new(0))
    }

    pub fn append_port_number(&mut self, port_number: u8) {
        for i in 0..5 {
            if self.get_hub_tier(i) == 0 {
                self.write_hub_tier(i, port_number);
                return;
            }
        }
    }

    pub fn get_hub_tier(&self, i: u8) -> u8 {
        let ret = match i {
            0 => self.0.read(RouteString_REG::HUB_TIER0),
            1 => self.0.read(RouteString_REG::HUB_TIER1),
            2 => self.0.read(RouteString_REG::HUB_TIER2),
            3 => self.0.read(RouteString_REG::HUB_TIER3),
            4 => self.0.read(RouteString_REG::HUB_TIER4),
            _ => unimplemented!("reserved"),
        };
        ret as u8
    }

    pub fn write_hub_tier(&mut self, i: u8, u: u8) {
        match i {
            0 => self.0.write(RouteString_REG::HUB_TIER0.val(u as _)),
            1 => self.0.write(RouteString_REG::HUB_TIER1.val(u as _)),
            2 => self.0.write(RouteString_REG::HUB_TIER2.val(u as _)),
            3 => self.0.write(RouteString_REG::HUB_TIER3.val(u as _)),
            4 => self.0.write(RouteString_REG::HUB_TIER4.val(u as _)),
            _ => unimplemented!("reserved"),
        };
    }

    pub fn get_as_u32(&self) -> u32 {
        self.0.get()
    }
}

// fn append_port_to_route_string(route_string: u32, port_id: usize) -> u32 {
//     let mut route_string = route_string;
//     for tier in 0..5 {
//         if route_string & (0x0f << (tier * 4)) == 0 {
//             if tier < 5 {
//                 route_string |= (port_id as u32) << (tier * 4);
//                 return route_string;
//             }
//         }
//     }

//     route_string
// }
