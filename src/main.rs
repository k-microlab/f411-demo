#![no_std]
#![no_main]

use ccm::{AeadInPlace, KeyInit};
use aes::cipher::{KeyIvInit, StreamCipher};
use aes::cipher::generic_array::GenericArray;
use arrayvec::ArrayVec;
use x25519_nostd::{public_key, diffie_hellman};
use ccm::consts::{U13, U8};
use defmt::*;
use embassy_executor::Spawner;
use embassy_stm32::exti::ExtiInput;
use embassy_stm32::spi::{Config, Spi};
use embassy_stm32::time::Hertz;
use embassy_stm32::gpio::{Input, Level, Output, Pull, Speed};
use embassy_stm32::{bind_interrupts, exti, interrupt};

use {defmt_rtt as _, panic_probe as _};
use crate::meshtastic::{Data, MestasticHeader, NodeId, Nonce, PacketFlags, PortNum};
use crate::radio::{LoraBandwidth, LoraCodingRate, LoraHeaderType, LoraSpreadingFactor, OutputPower, Radio, RadioConfig, RampTime};

type Aes128Ctr = ctr::Ctr32BE<aes::Aes128>;
type Aes256Ctr = ctr::Ctr32BE<aes::Aes256>;
type Aes128CcmL2 = ccm::Ccm<aes::Aes128, U8, U13>;
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
pub mod cursor;
pub mod proto;
pub mod meshtastic;

use sha2::{Digest, Sha256};
use crate::cursor::Cursor;
use crate::proto::{FromWire, ToWire, Wire};

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

    let data = Data {
        port_num: PortNum::TextMessageApp,
        payload: b"",
        want_response: true,
        .. Default::default()
    };
    let header = MestasticHeader {
        to: NodeId::BROADCAST,
        from: NodeId(0x01020304),
        packet_id: 131,
        flags: PacketFlags::new(7, true, false, 0),
        channel: 8,
        next_hop: 0,
        relay_node: 253,
    };
    let mut out = ArrayVec::<u8, 256>::new();
    if let Some(payload) = try_encode(&mut buffer, &data, &header, Key::Key256(&shared_key), &mut out) {
        warn!("sending encrypted payload: {:02x}", payload);
        radio.transmit(payload, 60_000.0, true).await.expect("transmit failed");
    }


    loop {
        buffer.fill(0);
        let size = radio.receive(&mut buffer, None, true).await.unwrap();
        if size > 0 {
            let data = &mut buffer[..size];
            info!("Received bytes: {:02x}", data);
            let mut out = ArrayVec::<u8, 256>::new();
            if let Some((header, data)) = try_decode(data, Key::Key256(&shared_key), &mut out) {
                info!("data: {}", data);

                if data.port_num == PortNum::TextMessageApp && let Some(text) = core::str::from_utf8(data.payload).ok() {
                    info!("text message: \"{}\"", text);
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
    aes_256_ctr(data, &nonce, key)
}

fn aes_256_ctr<'buffer, 'key>(data: &'buffer mut [u8], nonce: &Nonce, key: Key<'key>) -> Option<&'buffer [u8]> {
    let nonce = GenericArray::from_slice(nonce.as_ctr_bytes());
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

fn aes_256_ccm_decrypt<'buffer, 'key>(data: &'buffer [u8], nonce: &Nonce, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
    let nonce = GenericArray::from_slice(nonce.as_ccm_bytes());

    unsafe {
        out.set_len(data.len());
    }
    out.copy_from_slice(data);

    match key {
        Key::Key128(key) => {
            let cipher = Aes128CcmL2::new(&GenericArray::from_slice(key));

            if let Ok(()) = cipher.decrypt_in_place(&nonce, &[], out) {
                Some(out.as_slice())
            } else {
                None
            }
        }
        Key::Key256(key) => {
            let cipher = Aes256CcmL2::new(&GenericArray::from_slice(key));

            if let Ok(()) = cipher.decrypt_in_place(&nonce, &[], out) {
                Some(out.as_slice())
            } else {
                None
            }
        }
    }
}

fn aes_256_ccm_encrypt<'buffer, 'key>(data: &'buffer [u8], nonce: &Nonce, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
    let nonce = GenericArray::from_slice(nonce.as_ccm_bytes());

    unsafe {
        out.set_len(data.len());
    }
    out.copy_from_slice(data);

    match key {
        Key::Key128(key) => {
            let cipher = Aes128CcmL2::new(&GenericArray::from_slice(key));

            if let Ok(()) = cipher.encrypt_in_place(&nonce, &[], out) {
                Some(out.as_slice())
            } else {
                None
            }
        }
        Key::Key256(key) => {
            let cipher = Aes256CcmL2::new(&GenericArray::from_slice(key));

            if let Ok(()) = cipher.encrypt_in_place(&nonce, &[], out) {
                Some(out.as_slice())
            } else {
                None
            }
        }
    }
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
    aes_256_ccm_decrypt(data, &nonce, key, out)
}

fn try_decode<'buffer, 'key>(data: &'buffer mut [u8], key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<(MestasticHeader, Data<'buffer>)> {
    let mut cursor = Cursor::<&mut [u8]>::new(&mut *data);
    let header = MestasticHeader::read(&mut cursor);
    let start = size_of_val(&header);
    let data = cursor.into_inner_mut();
    let dataptr = data.as_mut_ptr();

    {
        info!("recv: {} ({} bytes payload)", header, data.len());

        // CCM used only for personal messages
        if !header.is_broadcast() && let Some(packet) = try_decode_ccm(data, &header, key, out) {
            return Some((header, Data::from_wire(Wire::Len(packet), "packet")));
        }
    }

    // This hack is needed to workaround borrowck bug (see conditional borrow return)
    let data = unsafe {
        core::slice::from_raw_parts_mut(dataptr, data.len())
    };

    if let Some(packet) = try_decode_ctr(data, &header, Key::Key128(&DEFAULT_PSK)) {
        return Some((header, Data::from_wire(Wire::Len(packet), "packet")));
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
    aes_256_ccm_encrypt(data, &nonce, key, out)
}

fn try_encode_ctr<'buffer, 'key>(data: &'buffer mut [u8], header: &MestasticHeader, key: Key<'key>) -> Option<&'buffer [u8]> {
    let nonce = Nonce {
        packet_id: header.packet_id,
        extra: 0,
        from: header.from,
        pad: 0,
    };
    aes_256_ctr(data, &nonce, key)
}

fn try_encode<'buffer, 'key>(buffer: &'buffer mut [u8], data: &Data<'buffer>, header: &MestasticHeader, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
    let total = buffer.len();
    let mut len = 0;
    let mut cursor = Cursor::<&mut [u8]>::new(&mut *buffer);
    info!("bef!");
    header.write(&mut cursor);
    len += total - cursor.len();
    info!("header written!");
    let dw = data.to_wire(&mut cursor).unwrap();
    let data = dw.expect_len_mut("packet");
    if let Some(data) = try_encode_ctr(data, header, Key::Key128(&DEFAULT_PSK)) {
        len += data.len();
        Some(&buffer[..len])
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

