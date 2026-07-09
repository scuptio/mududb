#[cfg(test)]
mod tests {
    use crate::service::app_package::AppPackage;
    use crate::service::file_name;
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use std::collections::HashMap;
    use std::io::Write;
    use std::path::{Path, PathBuf};

    use mudu_sys::env_var::temp_dir;
    use mudu_sys::fs::sync::SFile;

    fn package_file(name: &str) -> PathBuf {
        temp_dir().join(format!("{}_{}.mpk", name, mudu_sys::random::uuid_v4()))
    }

    fn write_package(
        path: &Path,
        package_cfg: Option<&[u8]>,
        procedure_desc: Option<&[u8]>,
        ddl_sql: Option<&[u8]>,
        initdb_sql: Option<&[u8]>,
        modules: &[(&str, &[u8])],
    ) {
        let file = SFile::create(path).unwrap();
        let mut zip = zip::ZipWriter::new(file);
        let options = zip::write::SimpleFileOptions::default();

        if let Some(package_cfg) = package_cfg {
            zip.start_file(file_name::PACKAGE_CFG, options).unwrap();
            zip.write_all(package_cfg).unwrap();
        }
        if let Some(procedure_desc) = procedure_desc {
            zip.start_file(file_name::PROCEDURE_DESC, options).unwrap();
            zip.write_all(procedure_desc).unwrap();
        }
        if let Some(ddl_sql) = ddl_sql {
            zip.start_file(file_name::DDL_SQL, options).unwrap();
            zip.write_all(ddl_sql).unwrap();
        }
        if let Some(initdb_sql) = initdb_sql {
            zip.start_file(file_name::INIT_DB_SQL, options).unwrap();
            zip.write_all(initdb_sql).unwrap();
        }
        for (name, bytes) in modules {
            zip.start_file(*name, options).unwrap();
            zip.write_all(bytes).unwrap();
        }
        zip.finish().unwrap();
    }

    fn standard_cfg() -> &'static [u8] {
        br#"{"name":"app-json","lang":"rust","version":"0.1.0","use_async":true}"#
    }

    fn standard_desc() -> Vec<u8> {
        serde_json::to_vec(&ModProcDesc::new(HashMap::new())).unwrap()
    }

    // These tests write and read zip archives through flate2/zlib-rs, which
    // triggers Miri stacked-borrows/UB warnings in the dependency and is not
    // representative of application logic. They are ignored under Miri and run
    // only on native builds.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn loads_valid_package() {
        let package_file = package_file("app_json_desc");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[("module.wasm", b"\0asm\x01\0\0\0")],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert_eq!(package.name(), "app-json");

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn single_module_package_aligns_desc_module_name() {
        let package_file = package_file("app_json_align");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(br#"{"modules":{"module":[{"module_name":"module","proc_name":"proc","param_desc":{"fields":[]},"return_desc":{"fields":[]}}]}}"#),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[("key_value.wasm", b"\0asm\x01\0\0\0")],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert!(package.modules.contains_key("module"));
        assert!(!package.modules.contains_key("key_value"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn load_package_requires_package_cfg() {
        let package_file = package_file("missing_cfg");
        write_package(
            &package_file,
            None,
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains(file_name::PACKAGE_CFG));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn load_package_requires_ddl_sql() {
        let package_file = package_file("missing_ddl");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(&standard_desc()),
            None,
            Some(b""),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("ddl.sql"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn load_package_requires_procedure_desc() {
        let package_file = package_file("missing_desc");
        write_package(
            &package_file,
            Some(standard_cfg()),
            None,
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains(file_name::PROCEDURE_DESC));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn load_package_rejects_invalid_procedure_desc_json() {
        let package_file = package_file("invalid_desc");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(br#"{"modules":"bad"}"#),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(
            err.to_string()
                .contains("parse app procedure description error")
        );

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn load_package_rejects_invalid_package_cfg_json() {
        let package_file = package_file("invalid_cfg");
        write_package(
            &package_file,
            Some(br#"{"name":1}"#),
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("parse app configuration error"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn load_package_rejects_corrupt_zip_archive() {
        let package_file = package_file("corrupt_zip");
        mudu_sys::fs::sync::write(&package_file, b"not-a-zip").unwrap();

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("read achieve file failed"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn multi_module_package_does_not_align_names() {
        let package_file = package_file("multi_mod");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(br#"{"modules":{"module_a":[{"module_name":"module_a","proc_name":"proc_a","param_desc":{"fields":[]},"return_desc":{"fields":[]}}],"module_b":[{"module_name":"module_b","proc_name":"proc_b","param_desc":{"fields":[]},"return_desc":{"fields":[]}}]}}"#),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            &[
                ("first.wasm", b"\0asm\x01\0\0\0"),
                ("second.wasm", b"\0asm\x01\0\0\0"),
            ],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert!(package.modules.contains_key("first"));
        assert!(package.modules.contains_key("second"));
        assert!(!package.modules.contains_key("module_a"));
        assert!(!package.modules.contains_key("module_b"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }
}
