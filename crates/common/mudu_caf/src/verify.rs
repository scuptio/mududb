//! Full verification of a `.mpk` artifact: every section hash, the manifest
//! hash, the domain-separated root hash, and the optional ed25519 signatures.
//! Sealed artifacts (`FLAG_ENCRYPTED`) are not supported by this
//! implementation and are explicitly rejected.

use crate::layout::{
    self, FLAG_ENCRYPTED, FLAG_SIGNED, TAG_COMPONENT, TAG_MANIFEST, TAG_PAYLOAD, TAG_SIGNATURE,
};
use crate::manifest::*;
use crate::{decode_hash, sha256_hex};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::{bail, ensure, mudu_error};

/// The summary of a successful verification.
pub struct VerifyReport {
    /// The artifact name from the manifest.
    pub name: String,
    /// The artifact version from the manifest.
    pub version: String,
    /// The domain-separated root hash (hex) — the content identity.
    pub root_hash: String,
    /// SHA-256 of the manifest bytes (hex).
    pub manifest_hash: String,
    /// The number of embedded WASM components.
    pub components: usize,
    /// The number of payload files.
    pub payload_files: usize,
    /// The capabilities the artifact exposes.
    pub capabilities: Vec<CapabilityId>,
    /// Whether the artifact is sealed (always false here: sealed artifacts are rejected).
    pub encrypted: bool,
    /// Whether the artifact carries a signature section.
    pub signed: bool,
    /// Signature validity (`None` when unsigned).
    pub signature_valid: Option<bool>,
    /// The signer fingerprints (one per signature entry).
    pub signer_fingerprints: Vec<String>,
}

/// Verifies the artifact bytes.
pub fn verify_bytes(bytes: &[u8]) -> RS<VerifyReport> {
    let header = layout::read_header(bytes)?;
    if header.flags & FLAG_ENCRYPTED != 0 {
        bail!(
            ErrorCode::UnsupportedFormatVersion,
            "sealed artifacts are not supported"
        );
    }
    let entries = layout::decode_directory(layout::slice(
        bytes,
        header.directory_offset,
        header.directory_len,
    )?)?;

    let mut report = verify_plaintext(bytes, &header, &entries)?;

    report.signed = header.flags & FLAG_SIGNED != 0;
    if report.signed {
        let sig_entry = entries
            .iter()
            .find(|e| e.tag == TAG_SIGNATURE)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::CorruptedData,
                    "signed artifact has no signature entry"
                )
            })?;
        let sig_bytes = layout::slice(bytes, sig_entry.offset, sig_entry.length)?;
        let signatures: Vec<SignatureEntry> = crate::cbor_from_slice(sig_bytes)?;
        let mut all_valid = !signatures.is_empty();
        for entry in &signatures {
            let valid = match entry.alg {
                SignatureAlgorithm::Ed25519 => {
                    entry.key_id == crate::payload::signer_fingerprint(entry.pubkey.as_ref())
                        && crate::payload::verify_signature(
                            entry.pubkey.as_ref(),
                            entry.sig.as_ref(),
                            &header.root_hash,
                        )
                }
            };
            all_valid &= valid;
            report.signer_fingerprints.push(entry.key_id.clone());
        }
        report.signature_valid = Some(all_valid);
    }
    Ok(report)
}

/// A cleartext artifact: the complete hash chain over manifest / components /
/// payload trees / access scopes.
fn verify_plaintext(
    bytes: &[u8],
    header: &layout::Header,
    entries: &[layout::DirectoryEntry],
) -> RS<VerifyReport> {
    let manifest_bytes = layout::slice(bytes, header.manifest_offset, header.manifest_len)?;
    let interface_bytes = layout::slice(bytes, header.interface_offset, header.interface_len)?;

    let manifest: Manifest = crate::cbor_from_slice(manifest_bytes)?;
    ensure!(
        manifest.format_version == header.format_version,
        ErrorCode::UnsupportedFormatVersion,
        "format version mismatch"
    );

    let manifest_hash = verify_manifest_hash(manifest_bytes, entries)?;
    let interface_hash = verify_interface(interface_bytes, &manifest)?;
    let wasm_hashes = verify_components(bytes, entries, &manifest)?;
    let (payload_root_hashes, total_files) = verify_payloads(bytes, entries, &manifest)?;
    check_access_scopes(&manifest.components, &manifest.payloads)?;

    let root_hash = crate::identity::root_hash(
        &manifest_hash,
        &interface_hash,
        &wasm_hashes,
        &payload_root_hashes,
    )?;
    ensure!(
        root_hash == header.root_hash,
        ErrorCode::CorruptedData,
        "root hash mismatch"
    );

    Ok(VerifyReport {
        name: manifest.name.clone(),
        version: manifest.version.clone(),
        root_hash: hex::encode(root_hash),
        manifest_hash,
        components: manifest.components.len(),
        payload_files: total_files,
        capabilities: manifest.capabilities.clone(),
        encrypted: false,
        signed: false,
        signature_valid: None,
        signer_fingerprints: Vec::new(),
    })
}

/// Verifies the manifest bytes hash against its directory entry.
fn verify_manifest_hash(manifest_bytes: &[u8], entries: &[layout::DirectoryEntry]) -> RS<String> {
    let manifest_hash = sha256_hex(manifest_bytes);
    let manifest_entry = entries
        .iter()
        .find(|e| e.tag == TAG_MANIFEST)
        .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "directory has no manifest entry"))?;
    ensure!(
        manifest_entry.hash == decode_hash(&manifest_hash)?,
        ErrorCode::CorruptedData,
        "manifest hash mismatch"
    );
    Ok(manifest_hash)
}

/// Verifies the interface section hash.
fn verify_interface(interface_bytes: &[u8], manifest: &Manifest) -> RS<String> {
    let interface_hash = sha256_hex(interface_bytes);
    ensure!(
        interface_hash == manifest.hashes.interface,
        ErrorCode::CorruptedData,
        "interface hash mismatch"
    );
    ensure!(
        interface_hash == manifest.interface.hash,
        ErrorCode::CorruptedData,
        "interface hash mismatch (manifest ref)"
    );
    Ok(interface_hash)
}

/// Verifies each component's hash against its directory entry.
fn verify_components(
    bytes: &[u8],
    entries: &[layout::DirectoryEntry],
    manifest: &Manifest,
) -> RS<Vec<String>> {
    let component_entries: Vec<_> = entries.iter().filter(|e| e.tag == TAG_COMPONENT).collect();
    ensure!(
        component_entries.len() == manifest.components.len(),
        ErrorCode::CorruptedData,
        "component count mismatch"
    );
    ensure!(
        manifest.hashes.wasm.len() == manifest.components.len(),
        ErrorCode::CorruptedData,
        "wasm hash count mismatch"
    );
    let mut wasm_hashes = Vec::with_capacity(manifest.components.len());
    for (i, comp) in manifest.components.iter().enumerate() {
        let e = component_entries[i];
        let data = layout::slice(bytes, e.offset, e.length)?;
        let h = sha256_hex(data);
        ensure!(
            h == comp.hash,
            ErrorCode::CorruptedData,
            "component hash mismatch"
        );
        ensure!(
            h == manifest.hashes.wasm[i],
            ErrorCode::CorruptedData,
            "component hash mismatch (section hashes)"
        );
        ensure!(
            e.hash == decode_hash(&h)?,
            ErrorCode::CorruptedData,
            "component directory hash mismatch"
        );
        wasm_hashes.push(h);
    }
    Ok(wasm_hashes)
}

/// Verifies the payload trees' file hashes, directory hashes and root hashes,
/// returning each tree's root hash and the total file count.
fn verify_payloads(
    bytes: &[u8],
    entries: &[layout::DirectoryEntry],
    manifest: &Manifest,
) -> RS<(Vec<String>, usize)> {
    let payload_entries: Vec<_> = entries.iter().filter(|e| e.tag == TAG_PAYLOAD).collect();
    let total_files: usize = manifest
        .payloads
        .iter()
        .map(|p| {
            let mut files = Vec::new();
            crate::payload::collect_files(&p.root, "", &mut files);
            files.len()
        })
        .sum();
    ensure!(
        payload_entries.len() == total_files,
        ErrorCode::CorruptedData,
        "payload file count mismatch"
    );
    let mut payload_root_hashes = Vec::with_capacity(manifest.payloads.len());
    let mut next_entry = 0;
    for payload_ref in &manifest.payloads {
        let mut file_list: Vec<(String, String, u64)> = Vec::new();
        crate::payload::collect_files(&payload_ref.root, "", &mut file_list);
        for (_path, expected_hash, _len) in &file_list {
            let e = payload_entries[next_entry];
            next_entry += 1;
            let data = layout::slice(bytes, e.offset, e.length)?;
            let h = crate::payload::verify_file(data)?;
            ensure!(
                &h == expected_hash,
                ErrorCode::CorruptedData,
                "payload hash mismatch"
            );
            ensure!(
                e.hash == decode_hash(&h)?,
                ErrorCode::CorruptedData,
                "payload directory hash mismatch"
            );
        }
        crate::payload::verify_tree(&payload_ref.root)?;
        payload_root_hashes.push(payload_ref.root.hash.clone());
    }
    crate::payload::validate_payloads(&manifest.payloads)?;
    ensure!(
        manifest.hashes.payload == payload_root_hashes,
        ErrorCode::CorruptedData,
        "payload root hash mismatch"
    );
    Ok((payload_root_hashes, total_files))
}

fn check_access_scopes(components: &[ComponentRef], payloads: &[PayloadRef]) -> RS<()> {
    for comp in components {
        for scope in &comp.access {
            let payload_ref = resolve_payload(payloads, &scope.payload)?;
            let node = crate::payload::find(&payload_ref.root, &scope.path)
                .ok_or_else(|| mudu_error!(ErrorCode::CorruptedData, "access scope not found"))?;
            ensure!(
                node.hash == scope.hash,
                ErrorCode::CorruptedData,
                "access scope hash mismatch"
            );
        }
    }
    Ok(())
}

fn resolve_payload<'a>(payloads: &'a [PayloadRef], name: &str) -> RS<&'a PayloadRef> {
    if name.is_empty() {
        return payloads.first().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                "access scope on artifact without payloads"
            )
        });
    }
    payloads
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "unknown payload"))
}
