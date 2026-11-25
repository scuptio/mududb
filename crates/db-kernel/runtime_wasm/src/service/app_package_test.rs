#[cfg(test)]
mod tests {
    use crate::service::app_package::AppPackage;
    use mudu_caf::build::{BuildInput, ComponentInput, PayloadInput};
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    use mudu_sys::env_var::temp_dir;

    fn package_file(name: &str) -> PathBuf {
        temp_dir().join(format!("{}_{}.mpk", name, mudu_sys::random::uuid_v4()))
    }

    fn write_package(
        path: &Path,
        package_cfg: Option<&[u8]>,
        procedure_desc: Option<&[u8]>,
        ddl_sql: Option<&[u8]>,
        initdb_sql: Option<&[u8]>,
        abi: Option<&[u8]>,
        modules: &[(&str, &[u8])],
    ) {
        let mut entries = Vec::new();
        if let Some(package_cfg) = package_cfg {
            entries.push(("package.cfg.json".to_string(), package_cfg.to_vec()));
        }
        if let Some(procedure_desc) = procedure_desc {
            entries.push(("package.desc.json".to_string(), procedure_desc.to_vec()));
        }
        if let Some(ddl_sql) = ddl_sql {
            entries.push(("ddl.sql".to_string(), ddl_sql.to_vec()));
        }
        if let Some(initdb_sql) = initdb_sql {
            entries.push(("initdb.sql".to_string(), initdb_sql.to_vec()));
        }
        if let Some(abi) = abi {
            entries.push(("abi.json".to_string(), abi.to_vec()));
        }
        let input = BuildInput {
            name: "app-json".to_string(),
            version: "0.1.0".to_string(),
            interface_bytes: Vec::new(),
            payloads: vec![PayloadInput {
                name: "mududb".to_string(),
                media_type: "application/octet-stream".to_string(),
                entries,
            }],
            components: modules
                .iter()
                .map(|(name, bytes)| ComponentInput {
                    name: (*name).to_string(),
                    wasm: bytes.to_vec(),
                })
                .collect(),
            sign_seed: None,
        };
        let bytes = mudu_caf::build::assemble(&input).unwrap().bytes;
        mudu_sys::fs::sync::sync_write(path, bytes).unwrap();
    }

    fn standard_cfg() -> &'static [u8] {
        br#"{"name":"app-json","lang":"rust","version":"0.1.0","use_async":true}"#
    }

    fn standard_desc() -> Vec<u8> {
        serde_json::to_vec(&ModProcDesc::new(HashMap::new())).unwrap()
    }

    fn standard_abi() -> &'static [u8] {
        br#"{"syscall_abi_version":1}"#
    }

    #[test]
    fn loads_valid_package() {
        let package_file = package_file("app_json_desc");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[("module", b"\0asm\x01\0\0\0")],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert_eq!(package.name(), "app-json");

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn single_module_package_aligns_desc_module_name() {
        let package_file = package_file("app_json_align");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(br#"{"modules":{"module":[{"module_name":"module","proc_name":"proc","param_desc":{"fields":[]},"return_desc":{"fields":[]}}]}}"#),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[("key_value", b"\0asm\x01\0\0\0")],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert!(package.modules.contains_key("module"));
        assert!(!package.modules.contains_key("key_value"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_requires_package_cfg() {
        let package_file = package_file("missing_cfg");
        write_package(
            &package_file,
            None,
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("package.cfg.json"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_requires_ddl_sql() {
        let package_file = package_file("missing_ddl");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(&standard_desc()),
            None,
            Some(b""),
            Some(standard_abi()),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("ddl.sql"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_requires_procedure_desc() {
        let package_file = package_file("missing_desc");
        write_package(
            &package_file,
            Some(standard_cfg()),
            None,
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("package.desc.json"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_rejects_invalid_procedure_desc_json() {
        let package_file = package_file("invalid_desc");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(br#"{"modules":"bad"}"#),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
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
    fn load_package_rejects_invalid_package_cfg_json() {
        let package_file = package_file("invalid_cfg");
        write_package(
            &package_file,
            Some(br#"{"name":1}"#),
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("parse app configuration error"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_rejects_corrupt_archive() {
        let package_file = package_file("corrupt");
        mudu_sys::fs::sync::write(&package_file, [0x00u8; 100]).unwrap();

        let err = AppPackage::load(&package_file).unwrap_err();
        assert!(err.to_string().contains("bad magic"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_with_matching_syscall_abi_version() {
        let package_file = package_file("abi_ok");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[("module", b"\0asm\x01\0\0\0")],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert_eq!(package.name(), "app-json");

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_rejects_mismatched_syscall_abi_version() {
        let package_file = package_file("abi_bad");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(&standard_desc()),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(br#"{"syscall_abi_version":2}"#),
            &[("module", b"\0asm\x01\0\0\0")],
        );

        let err = AppPackage::load(&package_file).unwrap_err();
        let message = err.to_string();
        assert!(
            message.contains("unsupported syscall ABI version 2"),
            "unexpected error: {message}"
        );
        assert!(
            message.contains("expected 1"),
            "unexpected error: {message}"
        );

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn multi_module_package_does_not_align_names() {
        let package_file = package_file("multi_mod");
        write_package(
            &package_file,
            Some(standard_cfg()),
            Some(br#"{"modules":{"module_a":[{"module_name":"module_a","proc_name":"proc_a","param_desc":{"fields":[]},"return_desc":{"fields":[]}}],"module_b":[{"module_name":"module_b","proc_name":"proc_b","param_desc":{"fields":[]},"return_desc":{"fields":[]}}]}}"#),
            Some(b"create table t(id integer);\n"),
            Some(b""),
            Some(standard_abi()),
            &[
                ("first", b"\0asm\x01\0\0\0"),
                ("second", b"\0asm\x01\0\0\0"),
            ],
        );

        let package = AppPackage::load(&package_file).unwrap();
        assert!(package.modules.contains_key("first"));
        assert!(package.modules.contains_key("second"));
        assert!(!package.modules.contains_key("module_a"));
        assert!(!package.modules.contains_key("module_b"));

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }

    #[test]
    fn load_package_enforces_trusted_signers() {
        let seed = [9u8; 32];
        let pubkey = mudu_caf::sign::public_key(&seed);
        let fingerprint = mudu_caf::sign::fingerprint(&pubkey);

        let package_file = package_file("trusted");
        let mut entries = vec![
            ("package.cfg.json".to_string(), standard_cfg().to_vec()),
            ("package.desc.json".to_string(), standard_desc()),
            (
                "ddl.sql".to_string(),
                b"create table t(id integer);\n".to_vec(),
            ),
            ("initdb.sql".to_string(), b"".to_vec()),
            ("abi.json".to_string(), standard_abi().to_vec()),
        ];
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        let input = BuildInput {
            name: "app-json".to_string(),
            version: "0.1.0".to_string(),
            interface_bytes: Vec::new(),
            payloads: vec![PayloadInput {
                name: "mududb".to_string(),
                media_type: "application/octet-stream".to_string(),
                entries,
            }],
            components: vec![ComponentInput {
                name: "module".to_string(),
                wasm: b"\0asm\x01\0\0\0".to_vec(),
            }],
            sign_seed: Some(seed),
        };
        let bytes = mudu_caf::build::assemble(&input).unwrap().bytes;
        mudu_sys::fs::sync::sync_write(&package_file, bytes).unwrap();

        // No trust list: any valid signature is accepted.
        AppPackage::load(&package_file).unwrap();
        // Matching trusted fingerprint: accepted.
        AppPackage::load_trusted(&package_file, std::slice::from_ref(&fingerprint)).unwrap();
        // Non-trusted fingerprint: rejected.
        let err = AppPackage::load_trusted(&package_file, &["00".repeat(32)]).unwrap_err();
        assert!(err.to_string().contains("not trusted"), "{err}");

        mudu_sys::fs::sync::remove_file(package_file).unwrap();
    }
}
