use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu::compat::SYSCALL_PAYLOAD_CURRENT_VERSION;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::from_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_sys::fs;
use std::collections::HashMap;
use std::path::Path;

/// An extracted Mudu application package (a flat `.mpk` archive).
#[derive(Debug)]
pub struct AppPackage {
    /// Application configuration from `package.cfg.json`.
    pub package_cfg: AppInfo,
    /// DDL text from `ddl.sql` defining the application schema.
    pub ddl_sql: String,
    /// Module and procedure descriptors from `package.desc.json`.
    pub package_desc: ModProcDesc,
    /// Initialization SQL from `initdb.sql`, applied on first install.
    pub initdb_sql: String,
    /// WebAssembly bytecode of each module, keyed by module name.
    pub modules: HashMap<String, Vec<u8>>,
}

impl AppPackage {
    /// Loads a Mudu application package from a flat `.mpk` archive file.
    ///
    /// The archive is fully verified (section hashes, root hash, and — when
    /// present — the ed25519 signature) before its contents are parsed.
    pub fn load<P: AsRef<Path>>(path: P) -> RS<Self> {
        Self::load_trusted(path, &[])
    }

    /// Loads a Mudu application package, enforcing the given trusted signer
    /// fingerprints when non-empty (at least one valid signature from a
    /// trusted fingerprint is then required).
    pub fn load_trusted<P: AsRef<Path>>(path: P, trusted: &[String]) -> RS<Self> {
        let bytes = fs::sync::sync_read_all(path.as_ref())?;
        Self::from_bytes_trusted(&bytes, trusted)
    }

    /// Loads a Mudu application package from in-memory `.mpk` bytes.
    pub fn from_bytes(bytes: &[u8]) -> RS<Self> {
        Self::from_bytes_trusted(bytes, &[])
    }

    /// Loads a Mudu application package from in-memory `.mpk` bytes, enforcing
    /// the given trusted signer fingerprints when non-empty.
    pub fn from_bytes_trusted(bytes: &[u8], trusted: &[String]) -> RS<Self> {
        if !trusted.is_empty() {
            let report = mudu_caf::verify::verify_bytes(bytes)?;
            if report.signature_valid != Some(true) {
                return Err(mudu_error!(
                    ErrorCode::PermissionDenied,
                    "package has no valid signature"
                ));
            }
            let any_trusted = report
                .signer_fingerprints
                .iter()
                .any(|fp| trusted.contains(fp));
            if !any_trusted {
                return Err(mudu_error!(
                    ErrorCode::PermissionDenied,
                    "package signer is not trusted"
                ));
            }
        }

        let raw = mudu_caf::profile::parse(bytes)?;

        let app_cfg: AppInfo = from_json_str(&raw.cfg_text)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "parse app configuration error", e))?;
        let app_proc_desc: ModProcDesc = from_json_str(&raw.desc_text).map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "parse app procedure description error",
                e
            )
        })?;

        if raw.syscall_abi_version != SYSCALL_PAYLOAD_CURRENT_VERSION {
            return Err(mudu_error!(
                ErrorCode::UnsupportedFormatVersion,
                format!(
                    "unsupported syscall ABI version {}, expected {}",
                    raw.syscall_abi_version, SYSCALL_PAYLOAD_CURRENT_VERSION
                )
            ));
        }

        let modules: HashMap<String, Vec<u8>> = raw.modules.into_iter().collect();
        let modules = align_single_module_name(modules, &app_proc_desc);

        Ok(AppPackage {
            package_cfg: app_cfg,
            ddl_sql: raw.ddl_sql,
            package_desc: app_proc_desc,
            initdb_sql: raw.initdb_sql,
            modules,
        })
    }

    /// Returns the application name declared in the package configuration.
    pub fn name(&self) -> &String {
        &self.package_cfg.name
    }
}

fn align_single_module_name(
    modules: HashMap<String, Vec<u8>>,
    app_proc_desc: &ModProcDesc,
) -> HashMap<String, Vec<u8>> {
    if modules.len() != 1 || app_proc_desc.modules().len() != 1 {
        return modules;
    }

    let expected_module_name = match app_proc_desc.modules().keys().next() {
        Some(name) => name.clone(),
        None => return modules,
    };
    if modules.contains_key(&expected_module_name) {
        return modules;
    }

    let mut only_module_iter = modules.into_iter();
    #[expect(
        clippy::unwrap_used,
        reason = "modules has exactly one element due to the length check above"
    )]
    let (_, byte_code) = only_module_iter.next().unwrap();
    let mut aligned_modules = HashMap::with_capacity(1);
    aligned_modules.insert(expected_module_name, byte_code);
    aligned_modules
}
