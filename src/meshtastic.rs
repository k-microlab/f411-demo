use bitfield::bitfield;
use byteorder::LittleEndian;
use byteorder_cursor::Cursor;
use defmt::{Format, Formatter};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use crate::proto::{ProtoRead, ProtoWrite, Wire};

#[derive(Default, Clone, Copy, PartialEq, Eq, Debug)]
pub struct NodeId(pub u32);

impl NodeId {
    pub const NONE: NodeId = NodeId(0);
    pub const BROADCAST: NodeId = NodeId(u32::MAX);
}

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
    pub fn read(cursor: &mut Cursor<&[u8]>) -> Self {
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
        cursor.write_u32::<LittleEndian>(self.to.0);
        cursor.write_u32::<LittleEndian>(self.from.0);
        cursor.write_u32::<LittleEndian>(self.packet_id);
        cursor.write_u8(self.flags.0);
        cursor.write_u8(self.channel);
        cursor.write_u8(self.next_hop);
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

#[derive(Default, Format)]
pub struct Data<'a> {
    pub port_num: PortNum,
    pub payload: &'a [u8],
    pub want_response: bool,
    pub dest: NodeId,
    pub source: NodeId,
    pub request_id: u32,
    pub reply_id: u32,
    pub emoji: u32,
    pub bitfield: Option<u32>,
}

impl<'a> Data<'a> {
    pub fn read(cursor: &mut Cursor<&'a [u8]>) -> Self {
        let mut this = Self::default();
        while cursor.remaining() > 0 {
            let (id, wire) = cursor.read_wire();
            match id {
                1 => this.port_num = PortNum::from_i32(wire.expect_var_int()).expect("unknown port number"),
                2 => this.payload = wire.expect_len(),
                3 => this.want_response = wire.expect_var_int() != 0,
                4 => this.dest = NodeId(wire.expect_fixed32()),
                5 => this.source = NodeId(wire.expect_fixed32()),
                6 => this.request_id = wire.expect_fixed32(),
                7 => this.reply_id = wire.expect_fixed32(),
                8 => this.emoji = wire.expect_fixed32(),
                9 => this.bitfield = Some(wire.expect_var_int() as u32),
                _ => defmt::panic!("unknown proto field #{}: {}", id, wire),
            }
        }
        this
    }

    pub fn write(&self, cursor: &mut Cursor<&'a mut [u8]>) {
        cursor.write_wire(1, Wire::VarInt(self.port_num as i32));
        if self.payload.len() > 0 {
            cursor.write_wire(2, Wire::Len(self.payload));
        }
        if self.want_response {
            cursor.write_wire(3, Wire::VarInt(self.want_response as i32));
        }
        if self.dest != NodeId::NONE {
            cursor.write_wire(4, Wire::Fixed32(self.dest.0));
        }
        if self.source != NodeId::NONE {
            cursor.write_wire(5, Wire::Fixed32(self.source.0));
        }
        if self.request_id != 0 {
            cursor.write_wire(6, Wire::Fixed32(self.request_id));
        }
        if self.reply_id != 0 {
            cursor.write_wire(7, Wire::Fixed32(self.reply_id));
        }
        if self.emoji != 0 {
            cursor.write_wire(8, Wire::Fixed32(self.emoji));
        }
        if let Some(bitfield) = self.bitfield {
            cursor.write_wire(9, Wire::VarInt(bitfield as i32));
        }
    }
}