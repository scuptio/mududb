use crate::service::file_name;
use mudu::common::app_info::AppInfo;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::others::io_error;
use mudu::m_error;
use mudu::utils::json::from_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;

#[derive(Debug)]
pub struct MuduPackage {
    pub package_cfg: AppInfo,
    pub ddl_sql: String,
    pub package_desc: ModProcDesc,
    pub initdb_sql: String,
    pub modules: HashMap<String, Vec<u8>>,
}

impl MuduPackage {
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

    pub fn name(&self) -> &String {
        &self.package_cfg.name
    }
}

fn load_and_extract_package<P: AsRef<Path>>(package_path: P) -> RS<MuduPackage> {
    // Open the archive file
    let file = fs::File::open(package_path.as_ref()).map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("no such package file {:?}", package_path.as_ref()),
            e
        )
    })?;

    // Create a ZipArchive from the file
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| m_error!(EC::IOErr, "read achieve file failed", e))?;
    let mut ddl_sql = String::new();
    let mut initdb_sql = String::new();
    let mut app_cfg_text = String::new();
    let mut app_proc_desc_text = String::new();
    let mut modules = HashMap::new();
    // Iterate through all files in the archive
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| m_error!(EC::IOErr, "zip archive by_index error", e))?;

        // Get the file name
        let file_name = file.name().to_string();
        if file_name == file_name::PACKAGE_CFG {
            file.read_to_string(&mut app_cfg_text).map_err(io_error)?;
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
                return Err(m_error!(EC::InternalErr, "read byte code error"));
            }
            modules.insert(mod_name.to_string(), bytes);
        }
    }
    if app_cfg_text.is_empty() {
        return Err(m_error!(
            EC::IOErr,
            format!("no {} file in package", file_name::PACKAGE_CFG)
        ));
    }
    if ddl_sql.is_empty() {
        return Err(m_error!(EC::IOErr, "no ddl.sql file in package"));
    }
    if app_proc_desc_text.is_empty() {
        return Err(m_error!(
            EC::IOErr,
            format!("no {} file in package", file_name::PROCEDURE_DESC)
        ));
    }
    let app_cfg: AppInfo = from_json_str(app_cfg_text.as_str())
        .map_err(|e| m_error!(EC::DecodeErr, "parse app configuration error", e))?;
    let app_proc_desc: ModProcDesc = from_json_str(app_proc_desc_text.as_str())
        .map_err(|e| m_error!(EC::DecodeErr, "parse app procedure description error", e))?;
    let modules = align_single_module_name(modules, &app_proc_desc);

    Ok(MuduPackage {
        package_cfg: app_cfg,
        ddl_sql,
        package_desc: app_proc_desc,
        initdb_sql,
        modules,
    })
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
    let (_, byte_code) = only_module_iter.next().unwrap();
    let mut aligned_modules = HashMap::with_capacity(1);
    aligned_modules.insert(expected_module_name, byte_code);
    aligned_modules
}

#[cfg(test)]
mod tests {
    use crate::service::file_name;
    use crate::service::mudu_package::MuduPackage;
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use std::collections::HashMap;
    use std::env::temp_dir;
    use std::fs;
    use std::io::Write;
    use uuid::Uuid;

    #[test]
    fn test_app_package() {
        let package_file = temp_dir().join(format!("app_json_desc_{}.mpk", Uuid::new_v4()));
        let file = fs::File::create(&package_file).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file(file_name::PACKAGE_CFG, options).unwrap();
        zip.write_all(br#"{"name":"app-json","lang":"rust","version":"0.1.0","use_async":true}"#)
            .unwrap();

        zip.start_file(file_name::PROCEDURE_DESC, options).unwrap();
        let desc = serde_json::to_vec(&ModProcDesc::new(HashMap::new())).unwrap();
        zip.write_all(&desc).unwrap();

        zip.start_file(file_name::DDL_SQL, options).unwrap();
        zip.write_all(b"create table t(id integer);\n").unwrap();

        zip.start_file(file_name::INIT_DB_SQL, options).unwrap();
        zip.write_all(b"").unwrap();

        zip.start_file("module.wasm", options).unwrap();
        zip.write_all(b"\0asm").unwrap();

        zip.finish().unwrap();

        let package = MuduPackage::load(&package_file).unwrap();
        assert_eq!(package.name(), "app-json");

        fs::remove_file(package_file).unwrap();
    }

    #[test]
    fn test_single_module_package_aligns_desc_module_name() {
        let package_file = temp_dir().join(format!("app_json_align_{}.mpk", Uuid::new_v4()));
        let file = fs::File::create(&package_file).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        zip.start_file(file_name::PACKAGE_CFG, options).unwrap();
        zip.write_all(br#"{"name":"app-json","lang":"rust","version":"0.1.0","use_async":true}"#)
            .unwrap();

        zip.start_file(file_name::PROCEDURE_DESC, options).unwrap();
        zip.write_all(
            br#"{"modules":{"module":[{"module_name":"module","proc_name":"proc","param_desc":{"fields":[]},"return_desc":{"fields":[]}}]}}"#,
        )
        .unwrap();

        zip.start_file(file_name::DDL_SQL, options).unwrap();
        zip.write_all(b"create table t(id integer);\n").unwrap();

        zip.start_file(file_name::INIT_DB_SQL, options).unwrap();
        zip.write_all(b"").unwrap();

        zip.start_file("key_value.wasm", options).unwrap();
        zip.write_all(b"\0asm").unwrap();
        zip.finish().unwrap();

        let package = MuduPackage::load(&package_file).unwrap();
        assert!(package.modules.contains_key("module"));
        assert!(!package.modules.contains_key("key_value"));

        fs::remove_file(package_file).unwrap();
    }
}
