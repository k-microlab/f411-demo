use ccm::{AeadInPlace, KeyInit};
use aes::cipher::{KeyIvInit, StreamCipher};
use aes::cipher::generic_array::GenericArray;
use arrayvec::ArrayVec;
use ccm::consts::{U13, U8};
use crate::meshtastic::Nonce;

type Aes128Ctr = ctr::Ctr32BE<aes::Aes128>;
type Aes256Ctr = ctr::Ctr32BE<aes::Aes256>;
type Aes128CcmL2 = ccm::Ccm<aes::Aes128, U8, U13>;
type Aes256CcmL2 = ccm::Ccm<aes::Aes256, U8, U13>;

pub enum Key<'a> {
    Key128(&'a [u8; 16]),
    Key256(&'a [u8; 32]),
}

pub fn aes_256_ctr<'buffer, 'key>(data: &'buffer mut [u8], nonce: &Nonce, key: Key<'key>) -> Option<&'buffer [u8]> {
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

pub fn aes_256_ccm_decrypt<'buffer, 'key>(data: &'buffer [u8], nonce: &Nonce, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
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

pub fn aes_256_ccm_encrypt<'buffer, 'key>(data: &'buffer [u8], nonce: &Nonce, key: Key<'key>, out: &'buffer mut ArrayVec<u8, 256>) -> Option<&'buffer [u8]> {
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