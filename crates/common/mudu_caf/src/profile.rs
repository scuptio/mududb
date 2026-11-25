//! The MuduDB CAF Profile: maps a MuduDB application package (5 payload
//! files + WASM modules) onto the CAF container. See
//! `doc/zh-CN/contract/caf_mududb_profile_v1.md`.

use crate::build::{BuildInput, ComponentInput, PayloadInput};
use crate::layout;
use crate::manifest;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::{ensure, mudu_error};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// `package.cfg.json`.
pub const PACKAGE_CFG: &str = "package.cfg.json";
/// `package.desc.json`.
pub const PROCEDURE_DESC: &str = "package.desc.json";
/// `ddl.sql`.
pub const DDL_SQL: &str = "ddl.sql";
/// `initdb.sql`.
pub const INIT_DB_SQL: &str = "initdb.sql";
/// `abi.json`.
pub const ABI_JSON: &str = "abi.json";

/// The default payload name.
pub const PAYLOAD_NAME: &str = "mududb";

/// The structure of `abi.json`.
#[derive(Serialize, Deserialize, Debug)]
pub struct AbiManifest {
    /// The guest→host syscall payload ABI version.
    pub syscall_abi_version: u32,
}

/// The parsed raw MuduDB package contents (JSON parsing is left to the caller).
#[derive(Debug)]
pub struct RawPackage {
    /// The `package.cfg.json` text.
    pub cfg_text: String,
    /// The `package.desc.json` text.
    pub desc_text: String,
    /// The `ddl.sql` text.
    pub ddl_sql: String,
    /// The `initdb.sql` text (may be empty).
    pub initdb_sql: String,
    /// The syscall payload ABI version.
    pub syscall_abi_version: u32,
    /// The modules (module name, wasm bytes), in manifest order.
    pub modules: Vec<(String, Vec<u8>)>,
}

/// The build input.
pub struct PackageSpec {
    /// The application name (`AppInfo.name`).
    pub name: String,
    /// The application version (`AppInfo.version`).
    pub version: String,
    /// The `package.cfg.json` text.
    pub cfg_text: String,
    /// The `package.desc.json` text.
    pub desc_text: String,
    /// The `ddl.sql` text.
    pub ddl_sql: String,
    /// The `initdb.sql` text.
    pub initdb_sql: String,
    /// The syscall payload ABI version.
    pub syscall_abi_version: u32,
    /// The modules (module name, wasm bytes).
    pub modules: Vec<(String, Vec<u8>)>,
    /// The optional signing seed.
    pub sign_seed: Option<[u8; 32]>,
}

/// Builds the flat `.mpk` bytes.
pub fn build(spec: &PackageSpec) -> RS<Vec<u8>> {
    let abi = AbiManifest {
        syscall_abi_version: spec.syscall_abi_version,
    };
    let abi_json = serde_json::to_string(&abi)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "encode abi.json", e))?;

    let entries = vec![
        (PACKAGE_CFG.to_string(), spec.cfg_text.as_bytes().to_vec()),
        (
            PROCEDURE_DESC.to_string(),
            spec.desc_text.as_bytes().to_vec(),
        ),
        (DDL_SQL.to_string(), spec.ddl_sql.as_bytes().to_vec()),
        (INIT_DB_SQL.to_string(), spec.initdb_sql.as_bytes().to_vec()),
        (ABI_JSON.to_string(), abi_json.into_bytes()),
    ];

    let mut modules = spec.modules.clone();
    modules.sort_by(|a, b| a.0.cmp(&b.0));

    let input = BuildInput {
        name: spec.name.clone(),
        version: spec.version.clone(),
        interface_bytes: Vec::new(),
        payloads: vec![PayloadInput {
            name: PAYLOAD_NAME.to_string(),
            media_type: "application/octet-stream".to_string(),
            entries,
        }],
        components: modules
            .into_iter()
            .map(|(name, wasm)| ComponentInput { name, wasm })
            .collect(),
        sign_seed: spec.sign_seed,
    };
    Ok(crate::build::assemble(&input)?.bytes)
}

/// Verifies and parses a flat `.mpk` into the raw package contents.
pub fn parse(flat: &[u8]) -> RS<RawPackage> {
    crate::verify::verify_bytes(flat)?;
    let header = layout::read_header(flat)?;
    let entries = layout::decode_directory(layout::slice(
        flat,
        header.directory_offset,
        header.directory_len,
    )?)?;
    let manifest_bytes = layout::slice(flat, header.manifest_offset, header.manifest_len)?;
    let manifest: manifest::Manifest = crate::cbor_from_slice(manifest_bytes)?;

    ensure!(
        manifest.payloads.len() == 1,
        ErrorCode::InvalidArgument,
        "mududb package must have exactly one payload"
    );
    let payload = &manifest.payloads[0];

    // Reads the payload files (DFS order matches the directory's TAG_PAYLOAD
    // entries).
    let payload_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.tag == layout::TAG_PAYLOAD)
        .collect();
    let mut file_list: Vec<(String, String, u64)> = Vec::new();
    crate::payload::collect_files(&payload.root, "", &mut file_list);
    ensure!(
        payload_entries.len() == file_list.len(),
        ErrorCode::CorruptedData,
        "payload file count mismatch"
    );
    let mut files: HashMap<String, Vec<u8>> = HashMap::new();
    for (i, (path, _, _)) in file_list.iter().enumerate() {
        let e = payload_entries[i];
        let data = layout::slice(flat, e.offset, e.length)?;
        let key = path.trim_start_matches('/').to_string();
        files.insert(key, data.to_vec());
    }

    let cfg_text = read_string(&files, PACKAGE_CFG)?;
    let desc_text = read_string(&files, PROCEDURE_DESC)?;
    let ddl_sql = read_string(&files, DDL_SQL)?;
    let initdb_sql = read_string(&files, INIT_DB_SQL).unwrap_or_default();
    let abi: AbiManifest = serde_json::from_str(&read_string(&files, ABI_JSON)?)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "parse abi.json", e))?;

    // The modules.
    let component_entries: Vec<_> = entries
        .iter()
        .filter(|e| e.tag == layout::TAG_COMPONENT)
        .collect();
    ensure!(
        component_entries.len() == manifest.components.len(),
        ErrorCode::CorruptedData,
        "component count mismatch"
    );
    let mut modules = Vec::with_capacity(manifest.components.len());
    for (i, comp) in manifest.components.iter().enumerate() {
        let e = component_entries[i];
        let data = layout::slice(flat, e.offset, e.length)?;
        modules.push((comp.name.clone(), data.to_vec()));
    }

    Ok(RawPackage {
        cfg_text,
        desc_text,
        ddl_sql,
        initdb_sql,
        syscall_abi_version: abi.syscall_abi_version,
        modules,
    })
}

fn read_string(files: &HashMap<String, Vec<u8>>, name: &str) -> RS<String> {
    let bytes = files
        .get(name)
        .ok_or_else(|| mudu_error!(ErrorCode::NotFound, format!("no {name} file in package")))?;
    String::from_utf8(bytes.clone()).map_err(|e| mudu_error!(ErrorCode::Decode, "invalid utf-8", e))
}
