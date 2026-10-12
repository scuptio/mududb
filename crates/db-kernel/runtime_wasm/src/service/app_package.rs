use crate::service::file_name;
use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu::compat::SYSCALL_PAYLOAD_CURRENT_VERSION;
use mudu::error::ErrorCode;
use mudu::error::others::io_error;
use mudu::mudu_error;
use mudu::utils::json::from_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_sys::fs;
use serde::Deserialize;
use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

fn default_syscall_abi_version() -> u32 {
    SYSCALL_PAYLOAD_CURRENT_VERSION
}

#[derive(Debug, Clone, Deserialize)]
struct PackageManifest {
    format_version: u16,
    /// Guest→host syscall payload ABI version the package's modules were
    /// built against. Defaults to the current ABI version so packages built
    /// before this field existed still load.
    #[serde(default = "default_syscall_abi_version")]
    syscall_abi_version: u32,
    #[serde(default)]
    files: Vec<String>,
}

/// An extracted Mudu application package (`.mpk` ZIP archive).
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
    /// In a Mudu APP package archive file, there are the following files
    ///     1 `package.cfg.json`
    ///     1 `package.desc.json`
    ///     1 `ddl.sql`
    ///     1 `initdb.sql`
    ///     1 or more `*.wasm`
    ///
    /// Load package
    ///
    /// # Arguments
    /// * `package_path` - Path to the package ZIP archive file
    ///
    /// # Returns
    /// * `Ok(Package)` if operation completed successfully, return the package
    /// * `Err` if any error occurred during extraction
    pub fn load<P: AsRef<Path>>(path: P) -> RS<Self> {
        load_and_extract_package(path)
    }

    /// Returns the application name declared in the package configuration.
    pub fn name(&self) -> &String {
        &self.package_cfg.name
    }
}

fn load_and_extract_package<P: AsRef<Path>>(package_path: P) -> RS<AppPackage> {
    // Open the archive file
    let file = fs::sync::SFile::open(package_path.as_ref())?;

    // Create a ZipArchive from the file
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "read achieve file failed", e))?;
    let contents = read_package_contents(&mut archive)?;

    let app_cfg: AppInfo = from_json_str(contents.app_cfg_text.as_str())
        .map_err(|e| mudu_error!(ErrorCode::Decode, "parse app configuration error", e))?;
    let app_proc_desc: ModProcDesc =
        from_json_str(contents.app_proc_desc_text.as_str()).map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                "parse app procedure description error",
                e
            )
        })?;

    check_package_manifest(contents.manifest_text.as_str())?;
    let modules = align_single_module_name(contents.modules, &app_proc_desc);

    Ok(AppPackage {
        package_cfg: app_cfg,
        ddl_sql: contents.ddl_sql,
        package_desc: app_proc_desc,
        initdb_sql: contents.initdb_sql,
        modules,
    })
}

/// The raw payload of one package archive: the well-known text entries and
/// the WebAssembly bytecode of each module, keyed by module name.
struct PackageContents {
    app_cfg_text: String,
    app_proc_desc_text: String,
    ddl_sql: String,
    initdb_sql: String,
    manifest_text: String,
    modules: HashMap<String, Vec<u8>>,
}

/// Reads every entry of a package archive and validates the presence of the
/// required files.
fn read_package_contents(archive: &mut zip::ZipArchive<fs::sync::SFile>) -> RS<PackageContents> {
    let mut ddl_sql = String::new();
    let mut initdb_sql = String::new();
    let mut app_cfg_text = String::new();
    let mut app_proc_desc_text = String::new();
    let mut manifest_text = String::new();
    let mut modules = HashMap::new();
    // Iterate through all files in the archive
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| mudu_error!(ErrorCode::Decode, "zip archive by_index error", e))?;

        // Get the file name
        let file_name = file
            .name()
            .map_err(|e| mudu_error!(ErrorCode::Io, "zip walk error", e))?
            .to_string();
        if file_name == file_name::PACKAGE_CFG {
            file.read_to_string(&mut app_cfg_text).map_err(io_error)?;
        } else if file_name == file_name::PACKAGE_MANIFEST {
            file.read_to_string(&mut manifest_text).map_err(io_error)?;
        } else if file_name == file_name::DDL_SQL {
            file.read_to_string(&mut ddl_sql).map_err(io_error)?;
        } else if file_name == file_name::INIT_DB_SQL {
            file.read_to_string(&mut initdb_sql).map_err(io_error)?;
        } else if file_name == file_name::PROCEDURE_DESC {
            file.read_to_string(&mut app_proc_desc_text)
                .map_err(io_error)?;
        } else if file_name.ends_with(file_name::BYTE_CODE_MOD_SUFFIX) {
            let mod_name = &file_name[0..file_name.len() - file_name::BYTE_CODE_MOD_SUFFIX.len()];
            // if file has one of the extensions, it is byte code file
            let mut bytes = Vec::new();
            let read_bytes = file.read_to_end(&mut bytes).map_err(io_error)?;
            if bytes.len() != read_bytes {
                return Err(mudu_error!(ErrorCode::Internal, "read byte code error"));
            }
            modules.insert(mod_name.to_string(), bytes);
        }
    }
    if app_cfg_text.is_empty() {
        return Err(mudu_error!(
            ErrorCode::NotFound,
            format!("no {} file in package", file_name::PACKAGE_CFG)
        ));
    }
    if ddl_sql.is_empty() {
        return Err(mudu_error!(
            ErrorCode::NotFound,
            "no ddl.sql file in package"
        ));
    }
    if app_proc_desc_text.is_empty() {
        return Err(mudu_error!(
            ErrorCode::NotFound,
            format!("no {} file in package", file_name::PROCEDURE_DESC)
        ));
    }
    Ok(PackageContents {
        app_cfg_text,
        app_proc_desc_text,
        ddl_sql,
        initdb_sql,
        manifest_text,
        modules,
    })
}

/// Validates the optional package manifest: the format and syscall ABI
/// versions must be supported, and the manifest must list the required
/// package entries.
fn check_package_manifest(manifest_text: &str) -> RS<()> {
    if manifest_text.is_empty() {
        return Ok(());
    }
    let manifest: PackageManifest = from_json_str(manifest_text)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "parse package manifest error", e))?;
    if manifest.format_version != 1 {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "unsupported package manifest format_version {}",
                manifest.format_version
            )
        ));
    }
    if manifest.syscall_abi_version != SYSCALL_PAYLOAD_CURRENT_VERSION {
        return Err(mudu_error!(
            ErrorCode::UnsupportedFormatVersion,
            format!(
                "unsupported syscall ABI version {}, expected {}",
                manifest.syscall_abi_version, SYSCALL_PAYLOAD_CURRENT_VERSION
            )
        ));
    }
    // Basic sanity check: if a manifest is present, it should at least list
    // the required entries; otherwise leave behavior unchanged.
    let required = [
        file_name::PACKAGE_CFG,
        file_name::PROCEDURE_DESC,
        file_name::DDL_SQL,
        file_name::INIT_DB_SQL,
    ];
    for req in required {
        if !manifest.files.iter().any(|f| f == req) {
            return Err(mudu_error!(
                ErrorCode::Decode,
                format!("package manifest missing required file {}", req)
            ));
        }
    }
    Ok(())
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
