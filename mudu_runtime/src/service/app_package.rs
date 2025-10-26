use crate::service::app_cfg::AppCfg;
use crate::service::app_proc_desc::AppProcDesc;
use crate::service::file_name;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::error::others::io_error;
use mudu::m_error;
use std::collections::HashMap;
use std::fs;
use std::io::Read;
use std::path::Path;


#[derive(Debug)]
pub struct AppPackage {
    pub app_cfg: AppCfg,
    pub ddl_sql: String,
    pub app_proc_desc: AppProcDesc,
    pub initdb_sql: String,
    pub modules: HashMap<String, Vec<u8>>,
}

impl AppPackage {
    /// In a Mudu APP package archive file, there are following files
    ///     1 `app.cfg.toml`
    ///     1 `procedure.desc.toml`
    ///     1 `ddl.sql`
    ///     1 `initdb.sql`
    ///     1 or more `*.mod.wasm`
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
        &self.app_cfg.name
    }
}


fn load_and_extract_package<P: AsRef<Path>>(
    package_path: P
) -> RS<AppPackage> {
    // Open the archive file
    let file = fs::File::open(package_path.as_ref())
        .map_err(|e| { m_error!(EC::IOErr, format!("no such package file {:?}", package_path.as_ref()), e) })?;

    // Create a ZipArchive from the file
    let mut archive = zip::ZipArchive::new(file)
        .map_err(|e| { m_error!(EC::IOErr, "read achieve file failed", e) })?;
    let mut ddl_sql = String::new();
    let mut initdb_sql = String::new();
    let mut app_cfg_text = String::new();
    let mut app_proc_desc_text = String::new();
    let mut modules = HashMap::new();
    // Iterate through all files in the archive
    for i in 0..archive.len() {
        let mut file = archive.by_index(i)
            .map_err(|e| m_error!(EC::IOErr, "zip archive by_index error", e))?;

        // Get the file name
        let file_name = file.name().to_string();
        if file_name == file_name::APP_CFG {
            file.read_to_string(&mut app_cfg_text).map_err(io_error)?;
        } else if file_name == file_name::DDL_SQL {
            file.read_to_string(&mut ddl_sql).map_err(io_error)?;
        } else if file_name == file_name::INIT_DB_SQL {
            file.read_to_string(&mut initdb_sql).map_err(io_error)?;
        } else if file_name == file_name::PROCEDURE_DESC {
            file.read_to_string(&mut app_proc_desc_text).map_err(io_error)?;
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
        return Err(m_error!(EC::IOErr, "no app.cfg.toml file in package"));
    }
    if ddl_sql.is_empty() {
        return Err(m_error!(EC::IOErr, "no ddl.sql file in package"));
    }
    let app_cfg: AppCfg = toml::from_str(app_cfg_text.as_str())
        .map_err(|e| { m_error!(EC::DecodeErr, "parse app configuration error", e) })?;
    let app_proc_desc: AppProcDesc = toml::from_str(app_proc_desc_text.as_str())
        .map_err(|e| { m_error!(EC::DecodeErr, "parse app config error", e) })?;

    Ok(AppPackage {
        app_cfg,
        ddl_sql,
        app_proc_desc,
        initdb_sql,
        modules,
    })
}

#[cfg(test)]
mod tests {
    use crate::service::app_package::AppPackage;
    use mudu::this_file;
    use std::path::PathBuf;
    use std::str::FromStr;

    #[test]
    fn test_app_package() {
        let package_file = PathBuf::from_str(&this_file!()).unwrap().parent().unwrap()
            .join("test/package1.mpk").to_str().unwrap().to_string();
        let package = AppPackage::load(&package_file).unwrap();
        assert_eq!(package.name(), "app1");
    }
}
