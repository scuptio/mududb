//! ed25519 signatures (RFC 8032, deterministic).

use crate::manifest::{SignatureAlgorithm, SignatureEntry};
use mudu::common::result::RS;

/// Decodes a 64-character hex seed into 32 bytes.
pub fn decode_seed(hex_str: &str) -> RS<[u8; 32]> {
    crate::decode_hash(hex_str)
}

/// Derives the ed25519 public key from a 32-byte seed.
pub fn public_key(seed: &[u8; 32]) -> [u8; 32] {
    use ed25519_dalek::SigningKey;
    SigningKey::from_bytes(seed).verifying_key().to_bytes()
}

/// The signer fingerprint of a public key (SHA-256, hex).
pub fn fingerprint(pubkey: &[u8]) -> String {
    crate::payload::signer_fingerprint(pubkey)
}

/// Signs `root_hash` with `seed`, returning the signature section bytes (a
/// CBOR list of signature entries).
pub fn sign_root_hash(seed: &[u8; 32], root_hash: &[u8; 32]) -> RS<Vec<u8>> {
    use ed25519_dalek::Signer;
    use ed25519_dalek::SigningKey;

    let signing_key = SigningKey::from_bytes(seed);
    let pubkey = signing_key.verifying_key().to_bytes();
    let sig = signing_key.sign(root_hash);
    let entry = SignatureEntry {
        alg: SignatureAlgorithm::Ed25519,
        key_id: fingerprint(&pubkey),
        pubkey: serde_bytes::ByteBuf::from(pubkey.to_vec()),
        sig: serde_bytes::ByteBuf::from(sig.to_bytes().to_vec()),
    };
    crate::cbor_bytes(&vec![entry])
}
