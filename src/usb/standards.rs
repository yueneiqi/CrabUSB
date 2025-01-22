use core::fmt::Display;

use bit_field::BitField;
///utils according to USB standard

/// The Route String is a 20-bit field in downstream directed packets that the hub uses to route
/// each packet to the designated downstream port.  It is composed of a concatenation of the
/// downstream port numbers (4 bits per hub) for each hub traversed to reach a device.  The
/// hub uses a Hub Depth value multiplied by four as an offset into the Route String to locate the
/// bits it uses to determine the downstream port number.  The Hub Depth value is determined
/// and assigned to every hub during the enumeration process.  

#[derive(Clone, Debug, PartialEq, Eq)]
#[repr(C)]
pub struct TopologyRoute(u32);

impl Default for TopologyRoute {
    fn default() -> Self {
        Self::new()
    }
}

impl TopologyRoute {
    pub fn new() -> Self {
        Self(0)
    }

    pub fn append_port_number(&mut self, port_number: u8) {
        for i in 0..5 {
            if self.get_hub_index_at_tier(i) == 0 {
                self.write_hub_tier(i as _, port_number);
                return;
            }
        }
    }

    pub fn get_hub_index_at_tier(&self, i: usize) -> u8 {
        assert!(i <= 4, "for tier > 4, it's un implemented");
        // let ret = match i {
        //     0 => self.0.read(RouteString_REG::HUB_TIER0),
        //     1 => self.0.read(RouteString_REG::HUB_TIER1),
        //     2 => self.0.read(RouteString_REG::HUB_TIER2),
        //     3 => self.0.read(RouteString_REG::HUB_TIER3),
        //     4 => self.0.read(RouteString_REG::HUB_TIER4),
        //     _ => unimplemented!("reserved"),
        // };

        let from = i * 4;
        let to = i * 4 + 4;
        self.0.get_bits(from..to) as _
    }

    pub fn write_hub_tier(&mut self, i: usize, u: u8) {
        assert!(i <= 4, "for tier > 4, it's un implemented");
        let from = i * 4;
        let to = i * 4 + 4;
        self.0.set_bits(from..to, u as _);
    }

    pub fn port_idx(&self) -> u8 {
        self.get_hub_index_at_tier(0) - 1
    }

    pub fn port_number(&self) -> u8 {
        self.get_hub_index_at_tier(0)
    }

    pub fn route_string(&self) -> u32 {
        let mut r = self.clone();
        for idx in 0..4 {
            let get_hub_index_at_tier = r.get_hub_index_at_tier(idx);
            if get_hub_index_at_tier != 0 {
                r.write_hub_tier(idx, get_hub_index_at_tier - 1);
            }
        }
        r.0
    }
}
impl Display for TopologyRoute {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "topology: {:x}", self.0)
    }
}
