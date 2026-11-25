//! `mudu_caf` 端到端往返测试：build → verify → export → import → parse。

use mudu_caf::profile::{PackageSpec, RawPackage};

fn seed() -> [u8; 32] {
    [7u8; 32]
}

fn spec() -> PackageSpec {
    PackageSpec {
        name: "wallet".to_string(),
        version: "1.0.0".to_string(),
        cfg_text: r#"{"name":"wallet","lang":"rust","version":"1.0.0","use_async":false}"#
            .to_string(),
        desc_text: r#"{"modules":{},"procedures":[]}"#.to_string(),
        ddl_sql: "CREATE TABLE t(id INTEGER);\n".to_string(),
        initdb_sql: "INSERT INTO t VALUES (1);\n".to_string(),
        syscall_abi_version: 1,
        modules: vec![("wallet".to_string(), vec![0x00, 0x61, 0x73, 0x6d])],
        sign_seed: Some(seed()),
    }
}

fn build_flat() -> Vec<u8> {
    mudu_caf::profile::build(&spec()).unwrap()
}

#[test]
fn build_verify_roundtrip() {
    let flat = build_flat();
    let report = mudu_caf::verify::verify_bytes(&flat).unwrap();
    assert_eq!(report.name, "wallet");
    assert_eq!(report.version, "1.0.0");
    assert_eq!(report.components, 1);
    assert_eq!(report.payload_files, 5);
    assert!(report.signed);
    assert_eq!(report.signature_valid, Some(true));
    assert_eq!(report.signer_fingerprints.len(), 1);
}

#[test]
fn parse_roundtrip() {
    let flat = build_flat();
    let pkg: RawPackage = mudu_caf::profile::parse(&flat).unwrap();
    assert_eq!(pkg.cfg_text, spec().cfg_text);
    assert_eq!(pkg.desc_text, spec().desc_text);
    assert_eq!(pkg.ddl_sql, spec().ddl_sql);
    assert_eq!(pkg.initdb_sql, spec().initdb_sql);
    assert_eq!(pkg.syscall_abi_version, 1);
    assert_eq!(pkg.modules.len(), 1);
    assert_eq!(pkg.modules[0].0, "wallet");
    assert_eq!(pkg.modules[0].1, vec![0x00, 0x61, 0x73, 0x6d]);
}

#[test]
fn export_import_byte_identical() {
    let flat = build_flat();
    let zip = mudu_caf::export::export(&flat).unwrap();
    let rebuilt = mudu_caf::import::import(&zip).unwrap();
    assert_eq!(rebuilt, flat);
}

#[test]
fn tamper_is_rejected() {
    let mut flat = build_flat();
    flat[96] ^= 0xff;
    assert!(mudu_caf::verify::verify_bytes(&flat).is_err());
}

#[test]
fn unsigned_verify_has_no_signature() {
    let mut s = spec();
    s.sign_seed = None;
    let flat = mudu_caf::profile::build(&s).unwrap();
    let report = mudu_caf::verify::verify_bytes(&flat).unwrap();
    assert!(!report.signed);
    assert_eq!(report.signature_valid, None);
}

#[test]
fn hex_root_hash_is_stable() {
    let flat1 = build_flat();
    let flat2 = build_flat();
    assert_eq!(flat1, flat2);
}
