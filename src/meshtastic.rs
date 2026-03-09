use bitfield::bitfield;
use defmt::{Format, Formatter};

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct NodeId(pub u32);

impl Format for NodeId {
    fn format(&self, fmt: Formatter<'_>) {
        defmt::write!(fmt, "{:04X}", self.0);
    }
}

#[derive(Format)]
pub struct MestasticHeader {
    pub to: NodeId,
    pub from: NodeId,
    pub packet_id: u32,
    pub flags: PacketFlags,
    pub channel: u8,
    pub next_hop: u8,
    pub relay_node: u8,
}

impl MestasticHeader {
    pub fn is_broadcast(&self) -> bool {
        self.to == NodeId(u32::MAX)
    }
}

bitfield! {
    pub struct PacketFlags(u8);
    u8;
    pub get_hop_limit, _: 3, 0;
    pub get_want_ack, _: 4, 3;
    pub get_via_mqtt, _: 5, 4;
    pub get_hop_start, _: 8, 5;
}

impl Format for PacketFlags {
    fn format(&self, fmt: Formatter<'_>) {
        defmt::write!(fmt, "{{ hop_limit={}, want_ack={}, via_mqtt={}, hop_start={} }}", self.get_hop_limit(), self.get_want_ack() != 0, self.get_via_mqtt() != 0, self.get_hop_start());
    }
}

#[repr(packed)]
pub struct Nonce {
    pub packet_id: u32,
    pub extra: u32,
    pub from: NodeId,
    pub pad: u32
}

impl Nonce {
    pub fn as_ccm_bytes(&self) -> &[u8; 13] {
        unsafe { core::mem::transmute(self) }
    }
    
    pub fn as_ctr_bytes(&self) -> &[u8; 16] {
        unsafe { core::mem::transmute(self) }
    }
}