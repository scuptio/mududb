//! Payload tree construction and validation: files and directories form a
//! content-hashed Merkle tree, and files are split into fixed chunks to
//! support range reads and per-chunk verification.
//!
//! Consistent with caf-core's `payload.rs`; the difference is that this crate
//! is pure: the tree input is an in-memory `(path, bytes)` list rather than
//! filesystem paths.

use crate::manifest::{CHUNK_SIZE, EntryKind, PayloadLayout, PayloadNode, PayloadRef};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::{bail, ensure, mudu_error};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// Builds a payload directory tree from an in-memory `(relative path, bytes)`
/// list. Paths are `/`-separated, relative (no leading `/`), with no empty
/// component.
pub fn build_tree_from_entries(entries: &[(String, Vec<u8>)]) -> RS<PayloadNode> {
    let mut root = BTreeMap::new();
    for (path, bytes) in entries {
        let components: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(str::to_string)
            .collect();
        if components.is_empty() {
            bail!(ErrorCode::InvalidArgument, "empty payload path");
        }
        insert(&mut root, &components, bytes)?;
    }
    let mut node = to_payload_node(&RawNode::Dir(root), String::new())?;
    node.name = String::new();
    Ok(node)
}

/// Splits `data` into fixed chunks and returns `(root hash, chunk hash list)`.
/// The root hash of a single-chunk (or empty) file equals the SHA-256 of the
/// whole file.
pub fn file_hash(data: &[u8]) -> RS<(String, Vec<String>)> {
    let mut chunks = Vec::new();
    for chunk in data.chunks(CHUNK_SIZE) {
        chunks.push(sha256_hex(chunk));
    }
    let root = merkle_root(&chunks)?;
    Ok((root, chunks))
}

/// Recomputes a file hash from the raw bytes (for verification).
pub fn verify_file(data: &[u8]) -> RS<String> {
    let (root, _) = file_hash(data)?;
    Ok(root)
}

/// SHA-256 (hex) — the same digest as a chunk hash.
pub fn chunk_hash(data: &[u8]) -> String {
    sha256_hex(data)
}

/// The number of fixed-size chunks.
pub fn chunk_count(length: u64) -> u64 {
    length.div_ceil(CHUNK_SIZE as u64)
}

/// The byte range `[start, end)` of chunk `index`.
pub fn chunk_range(length: u64, index: u64) -> (u64, u64) {
    let start = index * CHUNK_SIZE as u64;
    let end = (start + CHUNK_SIZE as u64).min(length);
    (start, end)
}

/// The Merkle root of the chunk hashes. An odd level duplicates its last
/// element.
pub fn merkle_root(hex_hashes: &[String]) -> RS<String> {
    if hex_hashes.is_empty() {
        return Ok(sha256_hex(b""));
    }
    let mut level: Vec<Vec<u8>> = hex_hashes
        .iter()
        .map(|h| hex::decode(h).map_err(|e| mudu_error!(ErrorCode::Decode, "bad hex hash", e)))
        .collect::<RS<_>>()?;
    while level.len() > 1 {
        let mut next = Vec::with_capacity(level.len().div_ceil(2));
        for pair in level.chunks(2) {
            let mut h = Sha256::new();
            h.update(&pair[0]);
            h.update(pair.get(1).unwrap_or(&pair[0]));
            next.push(h.finalize().to_vec());
        }
        level = next;
    }
    Ok(hex::encode(&level[0]))
}

/// Lists every file as `(path, hash, length)` depth-first, in the same order
/// as build/layout.
pub fn collect_files(node: &PayloadNode, parent: &str, out: &mut Vec<(String, String, u64)>) {
    let path = if node.name.is_empty() {
        parent.to_string()
    } else {
        format!("{parent}/{}", node.name)
    };
    if node.kind == EntryKind::File {
        out.push((path, node.hash.clone(), node.length));
    } else {
        for c in &node.entries {
            collect_files(c, &path, out);
        }
    }
}

/// Finds a payload tree node by its `/` path.
pub fn find<'a>(root: &'a PayloadNode, path: &str) -> Option<&'a PayloadNode> {
    if path == "/" || path.is_empty() {
        return Some(root);
    }
    let mut node = root;
    for part in path.split('/').filter(|s| !s.is_empty()) {
        node = node.entries.iter().find(|e| e.name == part)?;
    }
    Some(node)
}

/// Recomputes every directory hash bottom-up (structural consistency only;
/// payload bytes are not involved).
pub fn verify_tree(node: &PayloadNode) -> RS<()> {
    if node.kind == EntryKind::File {
        return Ok(());
    }
    for child in &node.entries {
        verify_tree(child)?;
    }
    let recomputed = dir_hash(&node.entries)?;
    if recomputed != node.hash {
        bail!(ErrorCode::CorruptedData, "directory hash mismatch");
    }
    Ok(())
}

/// Validates the multi-payload mount rules (unique names, safe path
/// segments, no collision with the default payload's top level).
pub fn validate_payloads(payloads: &[PayloadRef]) -> RS<()> {
    let pairs: Vec<(&str, &PayloadNode)> = payloads
        .iter()
        .map(|p| (p.name.as_str(), &p.root))
        .collect();
    validate_mounts(&pairs)
}

/// The `(name, tree)` pair variant of [`validate_payloads`].
pub fn validate_mounts(pairs: &[(&str, &PayloadNode)]) -> RS<()> {
    let mut names = std::collections::HashSet::with_capacity(pairs.len());
    for (name, _) in pairs {
        let name = *name;
        if name.is_empty() || name == "." || name == ".." || name.contains(['/', '\\']) {
            bail!(ErrorCode::InvalidArgument, "invalid payload name");
        }
        if !names.insert(name) {
            bail!(ErrorCode::InvalidArgument, "duplicate payload name");
        }
    }
    if let Some((_, default_root)) = pairs.first() {
        for entry in &default_root.entries {
            if pairs[1..].iter().any(|(name, _)| *name == entry.name) {
                bail!(
                    ErrorCode::InvalidArgument,
                    "payload name collides with default payload entry"
                );
            }
        }
    }
    Ok(())
}

/// ed25519 signature verification.
pub fn verify_signature(pubkey: &[u8], sig: &[u8], msg: &[u8]) -> bool {
    use ed25519_dalek::{Signature, Verifier, VerifyingKey};

    let pubkey_arr: Result<[u8; 32], _> = pubkey.try_into();
    let sig_arr: Result<[u8; 64], _> = sig.try_into();
    let (Ok(pubkey_arr), Ok(sig_arr)) = (pubkey_arr, sig_arr) else {
        return false;
    };
    let Ok(verifying_key) = VerifyingKey::from_bytes(&pubkey_arr) else {
        return false;
    };
    verifying_key
        .verify(msg, &Signature::from_bytes(&sig_arr))
        .is_ok()
}

/// SHA-256 of the public key — the signer fingerprint.
pub fn signer_fingerprint(pubkey: &[u8]) -> String {
    sha256_hex(pubkey)
}

// ---------------------------------------------------------------------------
// Internals
// ---------------------------------------------------------------------------

enum RawNode {
    Dir(BTreeMap<String, RawNode>),
    File(Vec<u8>),
}

fn insert(map: &mut BTreeMap<String, RawNode>, path: &[String], bytes: &[u8]) -> RS<()> {
    let head = path[0].clone();
    if path.len() == 1 {
        ensure!(
            map.get(&head).is_none(),
            ErrorCode::InvalidArgument,
            "duplicate payload path"
        );
        map.insert(head, RawNode::File(bytes.to_vec()));
    } else {
        let child = map
            .entry(head)
            .or_insert_with(|| RawNode::Dir(BTreeMap::new()));
        match child {
            RawNode::Dir(m) => insert(m, &path[1..], bytes)?,
            RawNode::File(_) => bail!(
                ErrorCode::InvalidArgument,
                "path component is both file and dir"
            ),
        }
    }
    Ok(())
}

fn to_payload_node(raw: &RawNode, name: String) -> RS<PayloadNode> {
    match raw {
        RawNode::File(bytes) => {
            let (hash, chunks) = file_hash(bytes)?;
            let layout = if chunks.len() > 1 {
                Some(PayloadLayout::Range)
            } else {
                Some(PayloadLayout::Opaque)
            };
            Ok(PayloadNode {
                kind: EntryKind::File,
                name,
                hash,
                length: bytes.len() as u64,
                layout,
                chunks,
                entries: Vec::new(),
            })
        }
        RawNode::Dir(map) => {
            let mut entries = Vec::with_capacity(map.len());
            for (child_name, child) in map {
                entries.push(to_payload_node(child, child_name.clone())?);
            }
            let hash = dir_hash(&entries)?;
            Ok(PayloadNode {
                kind: EntryKind::Dir,
                name,
                hash,
                length: 0,
                layout: None,
                chunks: Vec::new(),
                entries,
            })
        }
    }
}

fn dir_hash(entries: &[PayloadNode]) -> RS<String> {
    let mut h = Sha256::new();
    for e in entries {
        h.update(e.kind.as_str().as_bytes());
        h.update([0x00]);
        h.update((e.name.len() as u64).to_le_bytes());
        h.update(e.name.as_bytes());
        h.update(
            hex::decode(&e.hash).map_err(|e| mudu_error!(ErrorCode::Decode, "bad hex hash", e))?,
        );
    }
    Ok(hex::encode(h.finalize()))
}

fn sha256_hex(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}
