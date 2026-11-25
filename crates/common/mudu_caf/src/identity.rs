//! The content identity of a CAF artifact: a single domain-separated root
//! hash.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use sha2::{Digest, Sha256};

/// The root-hash domain separator (CAF identity v1).
pub const ROOT_DOMAIN: &[u8] = b"CAF1";

fn write_prefixed(h: &mut Sha256, hex_hash: &str) -> RS<()> {
    let raw =
        hex::decode(hex_hash).map_err(|e| mudu_error!(ErrorCode::Decode, "bad hex hash", e))?;
    h.update((raw.len() as u64).to_le_bytes());
    h.update(raw);
    Ok(())
}

/// Computes the domain-separated artifact root hash from the per-section
/// hashes.
pub fn root_hash(
    manifest_hash: &str,
    interface_hash: &str,
    wasm_hashes: &[String],
    payload_root_hashes: &[String],
) -> RS<[u8; 32]> {
    let mut h = Sha256::new();
    h.update(ROOT_DOMAIN);
    write_prefixed(&mut h, manifest_hash)?;
    write_prefixed(&mut h, interface_hash)?;
    h.update((wasm_hashes.len() as u64).to_le_bytes());
    for w in wasm_hashes {
        write_prefixed(&mut h, w)?;
    }
    h.update((payload_root_hashes.len() as u64).to_le_bytes());
    for p in payload_root_hashes {
        write_prefixed(&mut h, p)?;
    }
    let digest = h.finalize();
    let mut raw = [0u8; 32];
    raw.copy_from_slice(&digest);
    Ok(raw)
}
