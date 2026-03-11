use bitfield::bitfield;
use byteorder::LittleEndian;
use defmt::{info, Format, Formatter};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use crate::cursor::Cursor;
use crate::proto;
use crate::proto::{ReadWire, WriteWire, Wire, FromWire, ToWire};

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NONE: NodeId = NodeId(0);
    pub const BROADCAST: NodeId = NodeId(u32::MAX);
}

impl Format for NodeId {
    fn format(&self, fmt: Formatter<'_>) {
        if *self == NodeId::NONE {
            defmt::write!(fmt, "NONE");
        } else if *self == NodeId::BROADCAST {
            defmt::write!(fmt, "EVERYONE");
        } else {
            defmt::write!(fmt, "!{:08X}", self.0);
        }
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
    pub fn read(cursor: &mut Cursor<&mut [u8]>) -> Self {
        Self {
            to: NodeId(cursor.read_u32::<LittleEndian>()),
            from: NodeId(cursor.read_u32::<LittleEndian>()),
            packet_id: cursor.read_u32::<LittleEndian>(),
            flags: PacketFlags(cursor.read_u8()),
            channel: cursor.read_u8(),
            next_hop: cursor.read_u8(),
            relay_node: cursor.read_u8(),
        }
    }

    pub fn write(&self, cursor: &mut Cursor<&mut [u8]>) {
        info!("to!");
        cursor.write_u32::<LittleEndian>(self.to.0);
        info!("from!");
        cursor.write_u32::<LittleEndian>(self.from.0);
        info!("packet_id!");
        cursor.write_u32::<LittleEndian>(self.packet_id);
        info!("flags!");
        cursor.write_u8(self.flags.0);
        info!("channel!");
        cursor.write_u8(self.channel);
        info!("next_hop!");
        cursor.write_u8(self.next_hop);
        info!("relay_node!");
        cursor.write_u8(self.relay_node);
    }

    pub fn is_broadcast(&self) -> bool {
        self.to == NodeId(u32::MAX)
    }
}

bitfield! {
    pub struct PacketFlags(u8);
    u8;
    pub get_hop_limit, set_hop_limit: 3, 0;
    pub get_want_ack, set_want_ack: 4, 3;
    pub get_via_mqtt, set_via_mqtt: 5, 4;
    pub get_hop_start, set_hop_start: 8, 5;
}

impl PacketFlags {
    pub fn new(hop_limit: u8, want_ack: bool, via_mqtt: bool, hop_start: u8) -> Self {
        let mut this = Self(0);
        this.set_hop_limit(hop_limit);
        this.set_want_ack(want_ack as u8);
        this.set_via_mqtt(via_mqtt as u8);
        this.set_hop_start(hop_start);
        this
    }
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

#[repr(u32)]
#[derive(FromPrimitive, Format, Default, Clone, Copy, PartialEq, Eq)]
pub enum PortNum {
    #[default]
    UnknownApp = 0,
    TextMessageApp = 1,
    RemoteHardwareApp = 2,
    PositionApp = 3,
    NodeInfoApp = 4,
    RoutingApp = 5,
    AdminApp = 6,
    TextMessageCompressedApp = 7,
    WaypointApp = 8,
    AudioApp = 9,
    DetectionSensorApp = 10,
    AlertApp = 11,
    KeyVerificationApp = 12,
    ReplyApp = 32,
    IpTunnelApp = 33,
    PaxCounterApp = 34,
    StoreForwardPlusPlusApp = 35,
    NodeStatusApp = 36,
    SerialApp = 64,
    StoreForwardApp = 65,
    RangeTestApp = 66,
    TelemetryApp = 67,
    ZpsApp = 68,
    SimulatorApp = 69,
    TracerouteApp = 70,
    NeighborInfoApp = 71,
    AtakPlugin = 72,
    MapReportApp = 73,
    PowerStressApp = 74,
    ReticulumTunnelApp = 76,
    CayenneApp = 77,
    PrivateApp = 256,
    AtakForwarder = 257,
    Max = 511,
}

impl<'a> FromWire<'a> for PortNum {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        PortNum::from_i32(wire.expect_var_int(field)).expect("unknown port number")
    }
}

impl<'a> ToWire<'a> for PortNum {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == PortNum::UnknownApp { None } else { Some(Wire::VarInt(*self as i32)) }
    }

    fn wire_len(&self) -> usize {
        if *self == PortNum::UnknownApp { 0 } else { crate::varint::len_of(*self as i32) }
    }
}

impl<'a> FromWire<'a> for NodeId {
    fn from_wire(wire: Wire<'a>, field: &'static str) -> Self {
        Self(wire.expect_fixed32(field))
    }
}

impl<'a> ToWire<'a> for NodeId {
    fn to_wire(&self, cursor: &mut Cursor<&'a mut [u8]>) -> Option<Wire<'a>> {
        if *self == NodeId::NONE { None } else { Some(Wire::Fixed32(self.0)) }
    }

    fn wire_len(&self) -> usize {
        if *self == NodeId::NONE { 0 } else { 4 }
    }
}


proto! {
    pub struct Data<'a> {
        pub port_num: PortNum = 1,
        pub payload: &'a [u8] = 2,
        pub want_response: bool = 3,
        pub dest: NodeId = 4,
        pub source: NodeId = 5,
        pub request_id: u32 = 6,
        pub reply_id: u32 = 7,
        pub emoji: u32 = 8,
        pub bitfield: Option<u32> = 9,
    }
}