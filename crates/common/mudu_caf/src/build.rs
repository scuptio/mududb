//! Assembles a cleartext + signed `.mpk` artifact.
//!
//! Ported from `caf-tool`'s `assemble.rs` (the `assemble_plaintext` path),
//! without the WIT / component-metadata embedding / encryption. This crate is
//! pure: every input is in memory.

use crate::layout::{self, DirectoryEntry};
use crate::manifest::*;
use crate::{FORMAT_VERSION, decode_hash, sha256_hex};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Build input: one payload tree.
pub struct PayloadInput {
    /// The payload name (the first one is the default payload).
    pub name: String,
    /// The media type.
    pub media_type: String,
    /// The files (relative path, bytes).
    pub entries: Vec<(String, Vec<u8>)>,
}

/// Build input: one WASM component.
pub struct ComponentInput {
    /// The component name.
    pub name: String,
    /// The component bytes (raw core module).
    pub wasm: Vec<u8>,
}

/// The build input.
pub struct BuildInput {
    /// The artifact name.
    pub name: String,
    /// The artifact version (semver).
    pub version: String,
    /// The interface section bytes (empty for mududb).
    pub interface_bytes: Vec<u8>,
    /// The payload trees (mududb has exactly one).
    pub payloads: Vec<PayloadInput>,
    /// The WASM components.
    pub components: Vec<ComponentInput>,
    /// Optional signing seed (ed25519, 32 bytes).
    pub sign_seed: Option<[u8; 32]>,
}

/// The assembly result.
pub struct BuildOutput {
    /// The complete `.mpk` bytes.
    pub bytes: Vec<u8>,
    /// The cleartext-form manifest.
    pub manifest: Manifest,
    /// The root hash (hex).
    pub root_hash: String,
}

/// A hashed payload tree and its file blobs.
struct PreparedPayload {
    name: String,
    media_type: String,
    root: PayloadNode,
    blobs: Vec<Vec<u8>>,
    file_list: Vec<(String, String, u64)>,
}

/// The hashed component set.
struct Components {
    refs: Vec<ComponentRef>,
    blobs: Vec<Vec<u8>>,
    hashes: Vec<String>,
}

/// Every hashed section before assembly.
struct Prepared {
    interface_hash: String,
    interface_len: u64,
    payloads: Vec<PreparedPayload>,
    components: Components,
}

/// Assembles a `.mpk` artifact (deterministic; only `FORMAT_VERSION` is supported).
pub fn assemble(input: &BuildInput) -> RS<BuildOutput> {
    let prepared = prepare(input)?;
    let manifest = build_manifest(input, &prepared)?;
    let manifest_bytes = crate::cbor_bytes(&manifest)?;
    let manifest_hash = sha256_hex(&manifest_bytes);

    let signed = input.sign_seed.is_some();
    let (layout, mut entries) =
        build_directory(&manifest_bytes, &manifest_hash, &prepared, signed)?;

    let payload_root_hashes: Vec<String> = prepared
        .payloads
        .iter()
        .map(|p| p.root.hash.clone())
        .collect();
    let root_hash = crate::identity::root_hash(
        &manifest_hash,
        &prepared.interface_hash,
        &prepared.components.hashes,
        &payload_root_hashes,
    )?;

    let signature_bytes = match input.sign_seed {
        Some(seed) => Some(crate::sign::sign_root_hash(&seed, &root_hash)?),
        None => None,
    };
    if let Some(bytes) = &signature_bytes {
        let last = entries
            .last_mut()
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "signature entry missing"))?;
        last.length = bytes.len() as u64;
    }
    let directory_bytes = layout::encode_directory(&entries);

    let bytes = write_sections(
        input,
        &layout,
        &manifest_bytes,
        &directory_bytes,
        &prepared,
        signature_bytes.as_deref(),
        signed,
        &root_hash,
    );

    Ok(BuildOutput {
        bytes,
        manifest,
        root_hash: hex::encode(root_hash),
    })
}

/// Hashes the interface section, the payload trees and the components.
fn prepare(input: &BuildInput) -> RS<Prepared> {
    Ok(Prepared {
        interface_hash: sha256_hex(&input.interface_bytes),
        interface_len: input.interface_bytes.len() as u64,
        payloads: prepare_payloads(input)?,
        components: prepare_components(input),
    })
}

fn prepare_payloads(input: &BuildInput) -> RS<Vec<PreparedPayload>> {
    let mut payloads = Vec::with_capacity(input.payloads.len());
    for p in &input.payloads {
        let root = crate::payload::build_tree_from_entries(&p.entries)?;
        let mut file_list: Vec<(String, String, u64)> = Vec::new();
        crate::payload::collect_files(&root, "", &mut file_list);
        let mut blobs: Vec<Vec<u8>> = Vec::new();
        let mut by_path: std::collections::HashMap<String, &[u8]> = p
            .entries
            .iter()
            .map(|(path, bytes)| (path.clone(), bytes.as_slice()))
            .collect();
        for (path, _, _) in &file_list {
            let key = path.trim_start_matches('/').to_string();
            let data = by_path
                .remove(&key)
                .ok_or_else(|| mudu_error!(ErrorCode::Internal, "payload blob missing for path"))?;
            blobs.push(data.to_vec());
        }
        payloads.push(PreparedPayload {
            name: p.name.clone(),
            media_type: p.media_type.clone(),
            root,
            blobs,
            file_list,
        });
    }
    Ok(payloads)
}

fn prepare_components(input: &BuildInput) -> Components {
    let mut refs = Vec::with_capacity(input.components.len());
    let mut blobs = Vec::with_capacity(input.components.len());
    let mut hashes = Vec::with_capacity(input.components.len());
    for c in &input.components {
        let hash = sha256_hex(&c.wasm);
        let exports = crate::exports::extract_exports(&c.wasm);
        hashes.push(hash.clone());
        blobs.push(c.wasm.clone());
        refs.push(ComponentRef {
            name: c.name.clone(),
            world: String::new(),
            length: c.wasm.len() as u64,
            hash,
            exports,
            access: Vec::new(),
        });
    }
    Components {
        refs,
        blobs,
        hashes,
    }
}

fn build_manifest(input: &BuildInput, prepared: &Prepared) -> RS<Manifest> {
    let payload_root_hashes: Vec<String> = prepared
        .payloads
        .iter()
        .map(|p| p.root.hash.clone())
        .collect();
    Ok(Manifest {
        name: input.name.clone(),
        version: input.version.clone(),
        format_version: FORMAT_VERSION,
        interface: InterfaceRef {
            inline: false,
            length: prepared.interface_len,
            hash: prepared.interface_hash.clone(),
        },
        components: prepared.components.refs.clone(),
        capabilities: Vec::new(),
        execution: ExecutionProfile {
            profile: "mududb-proc".to_string(),
            deterministic: false,
        },
        resources: ResourceLimits {
            memory_max: 256 * 1024 * 1024,
            cpu_fuel: 100_000_000,
            read_max: 1024 * 1024 * 1024,
            output_max: 64 * 1024 * 1024,
        },
        policy: CapabilityPolicy {
            llm: None,
            self_read: false,
            network: false,
            filesystem: false,
            clock: false,
            random: false,
        },
        provenance: Provenance {
            data: Publisher::empty(),
            algorithm: Publisher::empty(),
            artifact: Publisher {
                publisher: input.name.clone(),
                signature: None,
            },
        },
        payloads: prepared
            .payloads
            .iter()
            .map(|p| PayloadRef {
                name: p.name.clone(),
                root: p.root.clone(),
                media_type: p.media_type.clone(),
                length: 0,
                hash: p.root.hash.clone(),
                compressed: None,
                padded_length: None,
                chunks: Vec::new(),
            })
            .collect(),
        hashes: SectionHashes {
            interface: prepared.interface_hash.clone(),
            wasm: prepared.components.hashes.clone(),
            payload: payload_root_hashes,
        },
    })
}

/// Computes the layout and fills the section directory (without the signature
/// section length, backfilled later).
fn build_directory(
    manifest_bytes: &[u8],
    manifest_hash: &str,
    prepared: &Prepared,
    signed: bool,
) -> RS<(layout::Layout, Vec<DirectoryEntry>)> {
    let component_lens: Vec<u64> = prepared
        .components
        .blobs
        .iter()
        .map(|b| b.len() as u64)
        .collect();
    let file_lens: Vec<u64> = prepared
        .payloads
        .iter()
        .flat_map(|p| p.file_list.iter().map(|(_, _, len)| *len))
        .collect();

    let entry_count = 1 + component_lens.len() + file_lens.len() + usize::from(signed);
    let directory_len = 4 + (entry_count * layout::DIRECTORY_ENTRY_SIZE) as u64;
    let lay = layout::compute(
        manifest_bytes.len() as u64,
        prepared.interface_len,
        directory_len,
        &component_lens,
        &file_lens,
        signed,
        None,
    );

    let mut entries: Vec<DirectoryEntry> = Vec::with_capacity(entry_count);
    entries.push(dir_entry(
        layout::TAG_MANIFEST,
        lay.manifest_offset,
        manifest_bytes.len() as u64,
        decode_hash(manifest_hash)?,
    ));
    for (i, blob) in prepared.components.blobs.iter().enumerate() {
        entries.push(dir_entry(
            layout::TAG_COMPONENT,
            lay.component_offsets[i],
            blob.len() as u64,
            decode_hash(&prepared.components.hashes[i])?,
        ));
    }
    let mut file_index = 0;
    for p in &prepared.payloads {
        for (_, hash, len) in &p.file_list {
            entries.push(dir_entry(
                layout::TAG_PAYLOAD,
                lay.file_offsets[file_index],
                *len,
                decode_hash(hash)?,
            ));
            file_index += 1;
        }
    }
    if signed {
        entries.push(dir_entry(
            layout::TAG_SIGNATURE,
            lay.signature_offset,
            0,
            [0u8; 32],
        ));
    }
    Ok((lay, entries))
}

/// Writes out the header, manifest, interface, directory, components,
/// payloads and signature per the layout.
#[allow(clippy::too_many_arguments)]
fn write_sections(
    input: &BuildInput,
    lay: &layout::Layout,
    manifest_bytes: &[u8],
    directory_bytes: &[u8],
    prepared: &Prepared,
    signature_bytes: Option<&[u8]>,
    signed: bool,
    root_hash: &[u8; 32],
) -> Vec<u8> {
    let mut out = Vec::new();
    let flags = if signed { layout::FLAG_SIGNED } else { 0 };
    layout::write_header(&mut out, lay, FORMAT_VERSION, root_hash, flags);
    pad_to(&mut out, lay.manifest_offset);
    out.extend_from_slice(manifest_bytes);
    pad_to(&mut out, lay.interface_offset);
    out.extend_from_slice(&input.interface_bytes);
    pad_to(&mut out, lay.directory_offset);
    out.extend_from_slice(directory_bytes);
    for (i, blob) in prepared.components.blobs.iter().enumerate() {
        pad_to(&mut out, lay.component_offsets[i]);
        out.extend_from_slice(blob);
    }
    let mut file_index = 0;
    for p in &prepared.payloads {
        for data in &p.blobs {
            pad_to(&mut out, lay.file_offsets[file_index]);
            out.extend_from_slice(data);
            file_index += 1;
        }
    }
    if let Some(bytes) = signature_bytes {
        pad_to(&mut out, lay.signature_offset);
        out.extend_from_slice(bytes);
    }
    out
}

fn dir_entry(tag: u8, offset: u64, length: u64, hash: [u8; 32]) -> DirectoryEntry {
    DirectoryEntry {
        tag,
        flags: 0,
        hash_alg: layout::HASH_ALG_SHA256,
        offset,
        length,
        hash,
    }
}

fn pad_to(out: &mut Vec<u8>, offset: u64) {
    let cur = out.len() as u64;
    if cur < offset {
        out.resize(offset as usize, 0);
    }
}
