//! flat `.mpk` → ZIP export (`.mpk.zip`, `stored`). Byte-deterministic.
//!
//! Ported from `caf-tool`'s `export.rs`, but pure: flat bytes in, zip bytes
//! out. The entry order is fixed: `manifest.cbor` / `manifest.json` /
//! `components/NNNN.wasm` / `payloads/<name>/<path>` / `signatures.cbor` /
//! `export.json`.

use crate::layout;
use crate::manifest;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::{bail, ensure, mudu_error};
use std::io::{Cursor, Write};
use zip::ZipWriter;
use zip::write::SimpleFileOptions;

/// The schema version of export.json (per the CAF round-trip spec §3).
pub const EXPORT_FORMAT_CURRENT: u64 = 3;
/// The oldest export.json schema version import accepts.
pub const EXPORT_FORMAT_MIN: u64 = 3;

/// Exports a flat `.mpk` as a ZIP.
pub fn export(flat: &[u8]) -> RS<Vec<u8>> {
    let header = layout::read_header(flat)?;
    if header.flags & layout::FLAG_ENCRYPTED != 0 {
        bail!(
            ErrorCode::UnsupportedFormatVersion,
            "sealed artifacts are not supported"
        );
    }
    let entries = layout::decode_directory(layout::slice(
        flat,
        header.directory_offset,
        header.directory_len,
    )?)?;
    let manifest_bytes = layout::slice(flat, header.manifest_offset, header.manifest_len)?;
    let manifest: manifest::Manifest = crate::cbor_from_slice(manifest_bytes)?;
    ensure!(
        manifest.format_version == header.format_version,
        ErrorCode::UnsupportedFormatVersion,
        "format version mismatch"
    );

    let mut zip = ZipWriter::new(Cursor::new(Vec::new()));
    add(&mut zip, "manifest.cbor", manifest_bytes)?;
    let manifest_json = serde_json::to_vec_pretty(&manifest)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "encode manifest.json", e))?;
    add(&mut zip, "manifest.json", &manifest_json)?;

    // Components, in manifest order.
    let component_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.tag == layout::TAG_COMPONENT)
        .collect();
    ensure!(
        component_entries.len() == manifest.components.len(),
        ErrorCode::CorruptedData,
        "component count mismatch"
    );
    for (i, e) in component_entries.iter().enumerate() {
        let data = layout::slice(flat, e.offset, e.length)?;
        add(&mut zip, &format!("components/{i:04}.wasm"), data)?;
    }

    // Payload files, in manifest order, DFS per tree.
    write_payloads(&mut zip, flat, &entries, &manifest)?;

    // The signature section (stored verbatim as CBOR).
    let signed = header.flags & layout::FLAG_SIGNED != 0;
    if signed {
        let e = entries
            .iter()
            .find(|e| e.tag == layout::TAG_SIGNATURE)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::CorruptedData,
                    "signed artifact has no signature entry"
                )
            })?;
        let data = layout::slice(flat, e.offset, e.length)?;
        add(&mut zip, "signatures.cbor", data)?;
    }

    // export.json: metadata and the verification anchor, never the ordering
    // authority.
    let components_json: Vec<serde_json::Value> = manifest
        .components
        .iter()
        .enumerate()
        .map(|(i, c)| serde_json::json!({ "name": c.name, "path": format!("components/{i:04}.wasm") }))
        .collect();
    let export_json = serde_json::json!({
        "export_version": EXPORT_FORMAT_CURRENT,
        "root_hash": hex::encode(header.root_hash),
        "source_encrypted": false,
        "format_version": header.format_version,
        "signed": signed,
        "components": components_json,
    });
    let export_json_bytes = serde_json::to_vec_pretty(&export_json)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "encode export.json", e))?;
    add(&mut zip, "export.json", &export_json_bytes)?;

    let cursor = zip
        .finish()
        .map_err(|e| mudu_error!(ErrorCode::Decode, "finish zip", e))?;
    Ok(cursor.into_inner())
}

fn write_payloads(
    zip: &mut ZipWriter<Cursor<Vec<u8>>>,
    flat: &[u8],
    entries: &[layout::DirectoryEntry],
    manifest: &manifest::Manifest,
) -> RS<()> {
    let payload_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.tag == layout::TAG_PAYLOAD)
        .collect();
    let mut next = 0;
    for p in &manifest.payloads {
        let mut file_list: Vec<(String, String, u64)> = Vec::new();
        crate::payload::collect_files(&p.root, "", &mut file_list);
        for (path, _, _) in &file_list {
            let e = payload_entries[next];
            next += 1;
            let data = layout::slice(flat, e.offset, e.length)?;
            let zip_path = format!("payloads/{}{}", p.name, path);
            add(zip, &zip_path, data)?;
        }
    }
    Ok(())
}

fn add(zip: &mut ZipWriter<Cursor<Vec<u8>>>, name: &str, bytes: &[u8]) -> RS<()> {
    zip.start_file(name, options(bytes.len() as u64))
        .map_err(|e| mudu_error!(ErrorCode::Decode, "zip start_file", e))?;
    zip.write_all(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Io, "zip write", e))?;
    Ok(())
}

fn options(length: u64) -> SimpleFileOptions {
    SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Stored)
        .last_modified_time(zip::DateTime::default())
        .large_file(length >= u32::MAX as u64 / 2)
}
