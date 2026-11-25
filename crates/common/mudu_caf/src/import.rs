//! Byte-identical reconstruction of a flat `.mpk` from a ZIP export. Every
//! entry is verified against the preserved `manifest.cbor` (the sole ordering
//! authority), and the rebuilt artifact is returned only after the recomputed
//! root hash matches `export.json` and strict verification passes.

use crate::layout;
use crate::manifest::Manifest;
use crate::sha256_hex;
use crate::sha256_raw;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::{ensure, mudu_error};
use std::io::{Cursor, Read};
use zip::ZipArchive;

/// Rebuilds a byte-identical cleartext `.mpk` from an export ZIP.
pub fn import(zip_bytes: &[u8]) -> RS<Vec<u8>> {
    let mut zip = ZipArchive::new(Cursor::new(zip_bytes.to_vec()))
        .map_err(|e| mudu_error!(ErrorCode::Decode, "read ZIP archive", e))?;

    let meta = read_export_meta(&mut zip)?;
    let manifest_bytes = read_entry(&mut zip, "manifest.cbor")?;
    let manifest: Manifest = crate::cbor_from_slice(&manifest_bytes)?;
    let interface_bytes = read_interface(&mut zip, &manifest)?;
    let component_blobs = read_components(&mut zip, &manifest)?;
    let (payload_blobs, payload_root_hashes) = read_payloads(&mut zip, &manifest)?;
    let signature = read_signature(&mut zip)?;
    let signed = signature.is_some();

    reassemble(
        &manifest_bytes,
        &manifest,
        &interface_bytes,
        &component_blobs,
        &payload_blobs,
        &payload_root_hashes,
        signature.as_deref(),
        signed,
        &meta,
    )
}

/// Reads and validates export.json (the format gate and identity anchor,
/// never an ordering source).
fn read_export_meta(zip: &mut ZipArchive<Cursor<Vec<u8>>>) -> RS<serde_json::Value> {
    let meta_bytes = read_entry(zip, "export.json")?;
    let meta: serde_json::Value = serde_json::from_slice(&meta_bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "parse export.json", e))?;
    let export_version = meta["export_version"]
        .as_u64()
        .ok_or_else(|| mudu_error!(ErrorCode::Decode, "export.json: missing export_version"))?;
    ensure!(
        (crate::export::EXPORT_FORMAT_MIN..=crate::export::EXPORT_FORMAT_CURRENT)
            .contains(&export_version),
        ErrorCode::UnsupportedFormatVersion,
        "unsupported export_version"
    );
    ensure!(
        meta["source_encrypted"] == false,
        ErrorCode::UnsupportedFormatVersion,
        "sealed artifacts cannot be reconstructed"
    );
    Ok(meta)
}

/// Reads the interface section (empty when there is no interface).
fn read_interface(zip: &mut ZipArchive<Cursor<Vec<u8>>>, manifest: &Manifest) -> RS<Vec<u8>> {
    let interface_bytes = if manifest.interface.length == 0 {
        Vec::new()
    } else {
        read_entry(zip, "interfaces.cbor")?
    };
    ensure!(
        interface_bytes.len() as u64 == manifest.interface.length
            && sha256_hex(&interface_bytes) == manifest.interface.hash,
        ErrorCode::CorruptedData,
        "interfaces.cbor does not match the manifest"
    );
    Ok(interface_bytes)
}

/// Reads the components, in manifest order.
fn read_components(zip: &mut ZipArchive<Cursor<Vec<u8>>>, manifest: &Manifest) -> RS<Vec<Vec<u8>>> {
    let mut blobs = Vec::with_capacity(manifest.components.len());
    for (i, comp) in manifest.components.iter().enumerate() {
        let bytes = read_entry(zip, &format!("components/{i:04}.wasm"))?;
        ensure!(
            bytes.len() as u64 == comp.length && sha256_hex(&bytes) == comp.hash,
            ErrorCode::CorruptedData,
            "component does not match the manifest"
        );
        blobs.push(bytes);
    }
    Ok(blobs)
}

/// Reads the payload files, per payload and DFS per tree; returns the blobs
/// and each tree's root hash.
fn read_payloads(
    zip: &mut ZipArchive<Cursor<Vec<u8>>>,
    manifest: &Manifest,
) -> RS<(Vec<Vec<u8>>, Vec<String>)> {
    let mut blobs = Vec::new();
    let mut root_hashes = Vec::with_capacity(manifest.payloads.len());
    for payload_ref in &manifest.payloads {
        let mut file_list: Vec<(String, String, u64)> = Vec::new();
        crate::payload::collect_files(&payload_ref.root, "", &mut file_list);
        for (path, hash, len) in &file_list {
            let zip_path = format!("payloads/{}{}", payload_ref.name, path);
            let bytes = read_entry(zip, &zip_path)?;
            ensure!(
                bytes.len() as u64 == *len && crate::payload::verify_file(&bytes)? == *hash,
                ErrorCode::CorruptedData,
                "payload does not match the manifest"
            );
            blobs.push(bytes);
        }
        root_hashes.push(payload_ref.root.hash.clone());
    }
    Ok((blobs, root_hashes))
}

/// Reads the optional signature section.
fn read_signature(zip: &mut ZipArchive<Cursor<Vec<u8>>>) -> RS<Option<Vec<u8>>> {
    let mut entry = match zip.by_name("signatures.cbor") {
        Ok(entry) => entry,
        Err(_) => return Ok(None),
    };
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "read signatures.cbor", e))?;
    Ok(Some(bytes))
}

/// Replays the canonical layout to rebuild the flat bytes, then applies the
/// root-hash gate and strict verification.
#[allow(clippy::too_many_arguments)]
fn reassemble(
    manifest_bytes: &[u8],
    manifest: &Manifest,
    interface_bytes: &[u8],
    component_blobs: &[Vec<u8>],
    payload_blobs: &[Vec<u8>],
    payload_root_hashes: &[String],
    signature: Option<&[u8]>,
    signed: bool,
    meta: &serde_json::Value,
) -> RS<Vec<u8>> {
    let manifest_hash = sha256_hex(manifest_bytes);
    let (lay, directory_bytes) = build_entries_and_layout(
        manifest_bytes,
        interface_bytes,
        component_blobs,
        payload_blobs,
        signature,
        signed,
    )?;

    // Identity gate: the recomputed root hash must match the exported value.
    let component_hashes: Vec<String> =
        manifest.components.iter().map(|c| c.hash.clone()).collect();
    let root_hash = crate::identity::root_hash(
        &manifest_hash,
        &manifest.interface.hash,
        &component_hashes,
        payload_root_hashes,
    )?;
    if let Some(expected) = meta["root_hash"].as_str() {
        ensure!(
            hex::encode(root_hash) == expected,
            ErrorCode::CorruptedData,
            "reconstructed root hash does not match export.json"
        );
    }

    let out = write_flat(
        manifest,
        manifest_bytes,
        interface_bytes,
        &directory_bytes,
        component_blobs,
        payload_blobs,
        signature,
        signed,
        &lay,
        &root_hash,
    );

    // Final oracle: strict verification of the rebuilt bytes (signatures
    // included).
    let report = crate::verify::verify_bytes(&out)?;
    ensure!(
        !signed || report.signature_valid == Some(true),
        ErrorCode::CorruptedData,
        "signatures.cbor does not verify against the reconstructed artifact"
    );

    Ok(out)
}

/// Computes the layout and fills the section directory, returning the layout
/// and the directory bytes.
fn build_entries_and_layout(
    manifest_bytes: &[u8],
    interface_bytes: &[u8],
    component_blobs: &[Vec<u8>],
    payload_blobs: &[Vec<u8>],
    signature: Option<&[u8]>,
    signed: bool,
) -> RS<(layout::Layout, Vec<u8>)> {
    let entry_count = 1 + component_blobs.len() + payload_blobs.len() + usize::from(signed);
    let directory_len = 4 + (entry_count * layout::DIRECTORY_ENTRY_SIZE) as u64;
    let component_lens: Vec<u64> = component_blobs.iter().map(|b| b.len() as u64).collect();
    let file_lens: Vec<u64> = payload_blobs.iter().map(|b| b.len() as u64).collect();
    let lay = layout::compute(
        manifest_bytes.len() as u64,
        interface_bytes.len() as u64,
        directory_len,
        &component_lens,
        &file_lens,
        signed,
        None,
    );

    let mut entries = Vec::with_capacity(entry_count);
    entries.push(dir_entry(
        layout::TAG_MANIFEST,
        lay.manifest_offset,
        manifest_bytes.len() as u64,
        sha256_raw(manifest_bytes),
    ));
    for (i, blob) in component_blobs.iter().enumerate() {
        entries.push(dir_entry(
            layout::TAG_COMPONENT,
            lay.component_offsets[i],
            blob.len() as u64,
            sha256_raw(blob),
        ));
    }
    for (i, blob) in payload_blobs.iter().enumerate() {
        let hash = crate::payload::verify_file(blob)?;
        entries.push(dir_entry(
            layout::TAG_PAYLOAD,
            lay.file_offsets[i],
            blob.len() as u64,
            crate::decode_hash(&hash)?,
        ));
    }
    if signed {
        let sig_len = signature.map(|s| s.len() as u64).unwrap_or(0);
        entries.push(dir_entry(
            layout::TAG_SIGNATURE,
            lay.signature_offset,
            sig_len,
            [0u8; 32],
        ));
    }
    let directory_bytes = layout::encode_directory(&entries);
    Ok((lay, directory_bytes))
}

/// Writes out the header, manifest, interface, directory, components,
/// payloads and signature per the layout.
#[allow(clippy::too_many_arguments)]
fn write_flat(
    manifest: &Manifest,
    manifest_bytes: &[u8],
    interface_bytes: &[u8],
    directory_bytes: &[u8],
    component_blobs: &[Vec<u8>],
    payload_blobs: &[Vec<u8>],
    signature: Option<&[u8]>,
    signed: bool,
    lay: &layout::Layout,
    root_hash: &[u8; 32],
) -> Vec<u8> {
    let mut out = Vec::new();
    let flags = if signed { layout::FLAG_SIGNED } else { 0 };
    layout::write_header(&mut out, lay, manifest.format_version, root_hash, flags);
    pad_to(&mut out, lay.manifest_offset);
    out.extend_from_slice(manifest_bytes);
    pad_to(&mut out, lay.interface_offset);
    out.extend_from_slice(interface_bytes);
    pad_to(&mut out, lay.directory_offset);
    out.extend_from_slice(directory_bytes);
    for (i, blob) in component_blobs.iter().enumerate() {
        pad_to(&mut out, lay.component_offsets[i]);
        out.extend_from_slice(blob);
    }
    for (i, blob) in payload_blobs.iter().enumerate() {
        pad_to(&mut out, lay.file_offsets[i]);
        out.extend_from_slice(blob);
    }
    if let Some(signature) = signature {
        pad_to(&mut out, lay.signature_offset);
        out.extend_from_slice(signature);
    }
    out
}

fn read_entry(zip: &mut ZipArchive<Cursor<Vec<u8>>>, name: &str) -> RS<Vec<u8>> {
    let mut entry = zip
        .by_name(name)
        .map_err(|e| mudu_error!(ErrorCode::NotFound, "ZIP is missing entry", e))?;
    let mut bytes = Vec::with_capacity(entry.size() as usize);
    entry
        .read_to_end(&mut bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "read zip entry", e))?;
    Ok(bytes)
}

fn dir_entry(tag: u8, offset: u64, length: u64, hash: [u8; 32]) -> layout::DirectoryEntry {
    layout::DirectoryEntry {
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
