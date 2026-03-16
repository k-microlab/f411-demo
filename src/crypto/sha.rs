use sha2::{Digest, Sha256};

pub fn hash_256(data: &[u8]) -> [u8; 32] {
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