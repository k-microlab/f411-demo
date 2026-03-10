#![no_std]
#![no_main]

use ccm::{AeadInPlace, KeyInit};
use aes::cipher::{KeyIvInit, StreamCipher};
use aes::cipher::generic_array::GenericArray;
use arrayvec::ArrayVec;
use x25519_nostd::{public_key, diffie_hellman};
use byteorder::LittleEndian;
use byteorder_cursor::Cursor;
use ccm::consts::{U13, U8};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::{bind_interrupts, exti, interrupt};
use num_derive::FromPrimitive;
use num_traits::FromPrimitive;
use {defmt_rtt as _, panic_probe as _};
use crate::meshtastic::{MestasticHeader, NodeId, Nonce, PacketFlags};
use crate::radio::{LoraBandwidth, LoraCodingRate, LoraHeaderType, LoraSpreadingFactor, OutputPower, Radio, RadioConfig, RampTime};

type Aes128Ctr = ctr::Ctr32LE<aes::Aes128>;
type Aes256Ctr = ctr::Ctr32LE<aes::Aes256>;
type Aes256CcmL2 = ccm::Ccm<aes::Aes256, U8, U13>;

const PRIV: [u8; 32] = [0x00; 32];
const DEFAULT_PSK: [u8; 16] = [0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69, 0x01];
const PUB: [u8; 32] = [0x06, 0xD8, 0x72, 0xFE, 0x4C, 0xB1, 0x45, 0x1E, 0xC3, 0x3F, 0x78, 0xCA, 0x62, 0xA8, 0x7A, 0x76, 0x1E, 0x73, 0x49, 0xFC, 0xC2, 0x3B, 0xC2, 0xD7, 0x31, 0x65, 0x13, 0x8F, 0x22, 0x58, 0x2B, 0x41];

bind_interrupts!(struct Irqs {
    EXTI2 => exti::InterruptHandler<interrupt::typelevel::EXTI2>;
    EXTI4 => exti::InterruptHandler<interrupt::typelevel::EXTI4>;
});

pub mod radio;
pub mod varint;
pub mod proto;
pub mod meshtastic;

use varint::VarIntRead;
use sha2::{Digest, Sha256};
use crate::proto::ProtoRead;

fn hash_256(data: &[u8]) -> [u8; 32] {
    // Create a new Sha256 object
    let mut hasher = Sha256::new();

    // Input data to hash (can be called repeatedly)
    hasher.update(data);

    // Read hash digest and consume hasher
    let result = hasher.finalize();

    // Convert GenericArray<u8, U32> to a fixed size array [u8; 32]
    // The result is a GenericArray, which can be safely turned into a fixed size array of 32 bytes
    result.into()
}

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    let config = Default::default();
    let p = embassy_stm32::init(config);

    let cs = Output::new(p.PB0, Level::High, Speed::VeryHigh);
    let reset = Output::new(p.PB1, Level::High, Speed::VeryHigh);
    let busy = ExtiInput::new(p.PB2, p.EXTI2, Pull::Down, Irqs);
    let dio1 = ExtiInput::new(p.PA4, p.EXTI4, Pull::Down, Irqs);
    let dio2 = Input::new(p.PA3, Pull::Down);
    let dio3 = Input::new(p.PA2, Pull::Down);
    let dio4 = Input::new(p.PA1, Pull::Down);

    let mut spi_config = Config::default();
    spi_config.frequency = Hertz(1_000_000);
    let spi = Spi::new(p.SPI1, p.PA5, p.PA7, p.PA6, p.DMA2_CH3, p.DMA2_CH2, spi_config);
    info!("SPI setup!");

    let config = RadioConfig {
        power: OutputPower::Db22,
        bandwidth: LoraBandwidth::BW_250,
        spread_factor: LoraSpreadingFactor::SF11,
        ramp_time: RampTime::R200,
        coding_rate: LoraCodingRate::CR_4_5,
        header_type: LoraHeaderType::VariableLength,
        sync_word: 0x24B4, // Meshtastic
        preamble_len: 16,
        frequency: 869_075_000,
    };
    let mut radio = Radio::new(spi, cs, busy, reset, dio1, dio2, dio3, dio4, config).await.unwrap();

    // let size = radio.transmit(b"Hello", 10_000.0, true).await.unwrap();
    //
    // info!("Transmitted {} bytes!", size);

    let mut buffer = [0; 255];

    let shared_key = hash_256(&diffie_hellman(&PRIV, &PUB));

    loop {
        buffer.fill(0);
        let size = radio.receive(&mut buffer, None, true).await.unwrap();
        if size > 0 {
            let data = &mut buffer[..size];
            info!("Received bytes: {:02x}", data);
            let mut out = ArrayVec::<u8, 256>::new();
            if let Some(packet) = try_decode(data, &shared_key, &mut out) {
                info!("decrypted: {:02x}", packet);
                let mut cursor = Cursor::new(packet);
                let data = Data::read(&mut cursor);
                info!("data: {}", data);

                /*let len = cursor.read_var_i32() as usize;
                let mut buf = [0; 256];
                cursor.read_bytes(&mut buf[..len]);
                let s = unsafe { core::str::from_utf8_unchecked(&buf[..len]) };
                info!("kind = {}, text = {}", tag, s);*/
            }
        }
    }

    loop {
        cortex_m::asm::nop();
    }
}

fn try_decode_ctr<'a>(data: &'a mut [u8], header: &MestasticHeader, key: Key) -> Option<&'a [u8]> {
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: 0,
        from: header.from,
        pad: 0,
    };
    aes_256_ctr(data, nonce.as_ctr_bytes(), key)
}

fn aes_256_ctr<'a>(data: &'a mut [u8], nonce: &[u8; 16], key: Key) -> Option<&'a [u8]> {
    let nonce = GenericArray::from_slice(nonce);
    match key {
        Key::Key128(key) => {
            let mut cipher = Aes128Ctr::new(&GenericArray::from_slice(key), nonce);

            if let Ok(()) = cipher.try_apply_keystream(data) {
                Some(data)
            } else {
                None
            }
        }
        Key::Key256(key) => {
            let mut cipher = Aes256Ctr::new(&GenericArray::from_slice(key), nonce);

            if let Ok(()) = cipher.try_apply_keystream(data) {
                Some(data)
            } else {
                None
            }
        }
    }
}

fn try_decode_ccm<'a>(data: &[u8], header: &MestasticHeader, key: &[u8; 32], out: &'a mut ArrayVec<u8, 256>) -> Option<&'a [u8]> {
    let (data, extra_nonce) = data.split_last_chunk::<4>().expect("data too short");
    let extra_nonce = u32::from_le_bytes(*extra_nonce);
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: extra_nonce,
        from: header.from,
        pad: 0,
    };
    let nonce = nonce.as_ccm_bytes();
    let nonce = GenericArray::from_slice(nonce);

    let cipher = Aes256CcmL2::new(&GenericArray::from_slice(key));

    unsafe {
        out.set_len(data.len());
    }
    out.copy_from_slice(data);

    if let Ok(()) = cipher.decrypt_in_place(&nonce, &[], out) {
        Some(out.as_slice())
    } else {
        None
    }
}

fn try_decode<'a>(data: &'a mut [u8], key: &[u8; 32], out: &'a mut ArrayVec<u8, 256>) -> Option<&'a [u8]> {
    let size = data.len();
    let mut cursor = Cursor::new(&*data);
    let header = MestasticHeader {
        to: NodeId(cursor.read_u32::<LittleEndian>()),
        from: NodeId(cursor.read_u32::<LittleEndian>()),
        packet_id: cursor.read_u32::<LittleEndian>(),
        flags: PacketFlags(cursor.read_u8()),
        channel: cursor.read_u8(),
        next_hop: cursor.read_u8(),
        relay_node: cursor.read_u8(),
    };
    let pos = cursor.position();
    let data = &mut data[pos..];
    info!("recv: {} ({} bytes payload)", header, data.len());

    // CCM used only for personal messages
    if !header.is_broadcast() && let Some(packet) = try_decode_ccm(data, &header, key, out) {
        Some(packet)
    } else if let Some(packet) = try_decode_ctr(data, &header, Key::Key128(&DEFAULT_PSK)) {
        Some(packet)
    } else {
        None
    }
}

enum Key<'a> {
    Key128(&'a [u8; 16]),
    Key256(&'a [u8; 32]),
}

/*mod tests {
    use crate::aes_256_ctr;

    #[test]
    fn test_ccm() {
        let shared_key = crate::hash_256(&x25519_nostd::diffie_hellman(b"\xa0\x03\x30\x63\x3e\x63\x52\x2f\x8a\x4d\x81\xec\x6d\x9d\x1e\x66\x17\xf6\xc8\xff\xd3\xa4\xc6\x98\x22\x95\x37\xd4\x4e\x52\x22\x77", b"\xdb\x18\xfc\x50\xee\xa4\x7f\x00\x25\x1c\xb7\x84\x81\x9a\x3c\xf5\xfc\x36\x18\x82\x59\x7f\x58\x9f\x0d\x7f\xf8\x20\xe8\x06\x44\x57"));
        defmt::info!("shared_key = {:02x}", shared_key);
        crate::try_decode(b"\x8c\x64\x6d\x7a\x29\x09\x00\x00\x62\xd6\xb2\x13\x6b\x00\x00\x00\x40\xdf\x24\xab\xfc\xc3\x0a\x17\xa3\xd9\x04\x67\x26\x09\x9e\x79\x6a\x1c\x03\x6a\x79\x2b", &shared_key);
    }

    #[test]
    fn test_ctr() {
        let plain = b"Single block msg";
        let key = b"\x77\x6B\xEF\xF2\x85\x1D\xB0\x6F\x4C\x8A\x05\x42\xC8\x69\x6F\x6C\x6A\x81\xAF\x1E\xEC\x96\xB4\xD3\x7F\xC1\xD6\x89\xE6\xC1\xC1\x04";;
        let nonce = b"\x00\x00\x00\x60\xDB\x56\x72\xC9\x7A\xA8\xF0\xB2\x00\x00\x00\x01";

        aes_256_ctr(plain, nonce, key);
    }
}*/

#[repr(u32)]
#[derive(FromPrimitive, Format)]
pub enum PortNum {
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
struct Data<'a> {
    port_num: Option<PortNum>,
    payload: Option<&'a [u8]>,
    want_response: Option<bool>,
    dest: Option<NodeId>,
    source: Option<NodeId>,
    request_id: Option<u32>,
    reply_id: Option<u32>,
    emoji: Option<u32>,
    bitfield: Option<u32>,
}

impl<'a> Data<'a> {
    pub fn read(cursor: &mut Cursor<&'a [u8]>) -> Self {
        let mut this = Self::default();
        while cursor.remaining() > 0 {
            let (id, wire) = cursor.read_wire();
            match id {
                1 => this.port_num = Some(PortNum::from_i32(wire.expect_var_int()).expect("unknown port number")),
                2 => this.payload = Some(wire.expect_len()),
                3 => this.want_response = Some(wire.expect_var_int() != 0),
                4 => this.dest = Some(NodeId(wire.expect_fixed32())),
                5 => this.source = Some(NodeId(wire.expect_fixed32())),
                6 => this.request_id = Some(wire.expect_fixed32()),
                7 => this.reply_id = Some(wire.expect_fixed32()),
                8 => this.emoji = Some(wire.expect_fixed32()),
                9 => this.bitfield = Some(wire.expect_var_int() as u32),
                _ => defmt::panic!("unknown proto field #{}: {}", id, wire),
            }
        }
        this
    }
}