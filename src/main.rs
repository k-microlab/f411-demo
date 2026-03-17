#![no_std]
#![no_main]

use arrayvec::ArrayVec;
use x25519_nostd::{public_key, diffie_hellman};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::{bind_interrupts, exti, interrupt};
use embassy_stm32::adc::{Adc, SampleTime};
use rand_chacha::rand_core::{Rng, SeedableRng};
use {defmt_rtt as _, panic_probe as _};
use crate::crypto::aes::Key;
use crate::meshtastic::{Data, MestasticHeader, NodeId, Nonce, PacketFlags, PortNum, Position, User};
use crate::radio::{LoraBandwidth, LoraCodingRate, LoraHeaderType, LoraSpreadingFactor, OutputPower, Radio, RadioConfig, RampTime};

extern crate alloc;

const ID_COUNTER_MASK: u32 = u32::MAX >> 22;

const NODE_ID: NodeId = NodeId(0x01020304);
const HOP_LIMIT: u8 = 7;
const WANT_ACK: bool = true;
const CHANNEL: u8 = 8;

const PRIV: [u8; 32] = [0x00; 32];
const DEFAULT_PSK: [u8; 16] = [0xd4, 0xf1, 0xbb, 0x3a, 0x20, 0x29, 0x07, 0x59, 0xf0, 0xbc, 0xff, 0xab, 0xcf, 0x4e, 0x69, 0x01];
const PUB: [u8; 32] = [0x06, 0xD8, 0x72, 0xFE, 0x4C, 0xB1, 0x45, 0x1E, 0xC3, 0x3F, 0x78, 0xCA, 0x62, 0xA8, 0x7A, 0x76, 0x1E, 0x73, 0x49, 0xFC, 0xC2, 0x3B, 0xC2, 0xD7, 0x31, 0x65, 0x13, 0x8F, 0x22, 0x58, 0x2B, 0x41];

bind_interrupts!(struct Irqs {
    EXTI2 => exti::InterruptHandler<interrupt::typelevel::EXTI2>;
    EXTI4 => exti::InterruptHandler<interrupt::typelevel::EXTI4>;
});

pub mod crypto;
pub mod radio;
pub mod varint;
pub mod cursor;
pub mod proto;
pub mod meshtastic;

use crate::cursor::Cursor;
use crate::proto::{FromWire, ToWire};

#[embassy_executor::main]
async fn main(spawner: Spawner) {
    {
        use core::mem::MaybeUninit;
        use embedded_alloc::LlffHeap as Heap;

        const HEAP_SIZE: usize = 32 * 1024; // 32 KB
        static mut HEAP_MEM: [MaybeUninit<u8>; HEAP_SIZE] = [MaybeUninit::uninit(); HEAP_SIZE];

        #[global_allocator]
        static HEAP: Heap = Heap::empty();
        unsafe { HEAP.init(&raw mut HEAP_MEM as usize, HEAP_SIZE) }
    }

    let config = Default::default();
    let p = embassy_stm32::init(config);

    let mut adc = Adc::new(p.ADC1);
    let mut pin = p.PA0; // Assuming PA0 is floating
    let hi = adc.blocking_read(&mut pin, SampleTime::CYCLES3);
    let lo = adc.blocking_read(&mut pin, SampleTime::CYCLES15);
    let seed = (hi as u32) << 16 | (lo as u32);
    let mut rng = rand_chacha::ChaChaRng::seed_from_u64(seed as u64);
    let mut packet_id = rng.next_u32() & 0x7fffffff;

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

    async fn send_packet<'r, 'd>(radio: &mut Radio<'r>, data: &Data<'d>, target: Option<NodeId>, packet_id: &mut u32, rand: &mut dyn Rng) {
        *packet_id += 1;
        *packet_id &= ID_COUNTER_MASK;

        let id = *packet_id | (rand.next_u32() << 10);

        let header = MestasticHeader {
            to: target.unwrap_or(NodeId::BROADCAST),
            from: NODE_ID,
            packet_id: id & 0x7fffffff,
            flags: PacketFlags::new(HOP_LIMIT, WANT_ACK, false, 0),
            channel: CHANNEL,
            next_hop: 0,
            relay_node: 253,
        };
        let mut buffer = [0; 255];
        let shared_key = crypto::sha::hash_256(&diffie_hellman(&PRIV, &PUB));
        let mut out = ArrayVec::<u8, 256>::new();
        if let Some(payload) = try_encode(&mut buffer, &data, &header, Key::Key256(&shared_key), &mut out) {
            warn!("sending encrypted payload: {:02x}", payload);
            radio.transmit(payload, 60_000.0, true).await.expect("transmit failed");
        }
    }

    send_packet(&mut radio, &Data {
        port_num: PortNum::TextMessageApp,
        payload: b"Hello from faketastic!",
        want_response: true,
        .. Default::default()
    }, None, &mut packet_id, &mut rng).await;

    let shared_key = crypto::sha::hash_256(&diffie_hellman(&PRIV, &PUB));
    let mut buffer = [0; 255];
    loop {
        buffer.fill(0);
        let size = radio.receive(&mut buffer, None, true).await.unwrap();
        if size > 0 {
            let data = &mut buffer[..size];
            info!("Received bytes: {:02x}", data);
            let mut out = ArrayVec::<u8, 256>::new();
            if let Some((header, data)) = try_decode(data, Key::Key256(&shared_key), &mut out) {
                info!("data: {}", data);

                match data.port_num {
                    PortNum::TextMessageApp => {
                        if let Some(text) = core::str::from_utf8(data.payload).ok() {
                            info!("text message: '{}'", text);
                        }
                    }
                    PortNum::NodeInfoApp => {
                        let user = User::from_payload(data.payload);
                        info!("node info: {}", user);
                    }
                    PortNum::PositionApp => {
                        let pos = Position::from_payload(data.payload);
                        info!("position: {}", pos);
                    }
                    _ => {}
                }
            }
        }
    }

    loop {
        cortex_m::asm::nop();
    }
}

fn try_decode_ctr<'buffer, 'key>(data: &'buffer mut [u8], header: &MestasticHeader, key: Key<'key>) -> Option<&'buffer [u8]> {
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: 0,
        from: header.from,
        pad: 0,
    };
    crypto::aes::aes_256_ctr(data, &nonce, key)
}

fn try_decode_ccm<'buffer, 'key>(data: &'buffer [u8], header: &MestasticHeader, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
    let (data, extra_nonce) = data.split_last_chunk::<4>().expect("data too short");
    let extra_nonce = u32::from_le_bytes(*extra_nonce);
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: extra_nonce,
        from: header.from,
        pad: 0,
    };
    crypto::aes::aes_256_ccm_decrypt(data, &nonce, key, out)
}

fn try_decode<'buffer, 'key>(data: &'buffer mut [u8], key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<(MestasticHeader, Data<'buffer>)> {
    let mut cursor = Cursor::<&mut [u8]>::new(&mut *data);
    let header = MestasticHeader::read(&mut cursor);
    let start = size_of_val(&header);
    let data = cursor.into_inner_mut();
    let dataptr = data.as_mut_ptr();

    {
        info!("recv: {}", header);
        info!("payload is {} bytes: {:02x}", data.len(), data);

        // CCM used only for personal messages
        if !header.is_broadcast() && let Some(packet) = try_decode_ccm(data, &header, key, out) {
            return Some((header, Data::from_payload(packet)));
        }
    }

    // This hack is needed to workaround borrowck bug (see conditional borrow return)
    let data = unsafe {
        core::slice::from_raw_parts_mut(dataptr, data.len())
    };

    if let Some(packet) = try_decode_ctr(data, &header, Key::Key128(&DEFAULT_PSK)) {
        info!("decoded bytes: {:02x}", packet);
        return Some((header, Data::from_payload(packet)));
    }

    None
}

fn try_encode_ccm<'buffer, 'key>(data: &'buffer [u8], header: &MestasticHeader, extra_nonce: u32, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: extra_nonce,
        from: header.from,
        pad: 0,
    };
    crypto::aes::aes_256_ccm_encrypt(data, &nonce, key, out)
}

fn try_encode_ctr<'buffer, 'key>(data: &'buffer mut [u8], header: &MestasticHeader, key: Key<'key>) -> Option<&'buffer [u8]> {
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: 0,
        from: header.from,
        pad: 0,
    };
    crypto::aes::aes_256_ctr(data, &nonce, key)
}

fn try_encode<'buffer, 'key>(buffer: &'buffer mut [u8], data: &Data<'buffer>, header: &MestasticHeader, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer mut [u8]> {
    let total = buffer.len();
    let mut len = 0;
    let mut cursor = Cursor::<&mut [u8]>::new(&mut *buffer);
    header.write(&mut cursor);
    len += total - cursor.len();
    let data = data.to_payload(&mut cursor);
    if let Some(data) = try_encode_ctr(data, header, Key::Key128(&DEFAULT_PSK)) {
        len += data.len();
        Some(&mut buffer[..len])
    } else {
        None
    }
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

