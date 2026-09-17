//! Round-trip test: compile the mgen-generated Rust MSSP codec in a scratch
//! crate and verify it against the committed golden corpus.
//!
//! The test generates Rust message + func codecs from the canonical
//! `mudu_binding` WIT directory into a scratch crate laid out as
//! `src/universal/*.rs` (matching the `use crate:universal:..` WIT import
//! paths), copies the `syscall_golden_test.rs` fixture and the committed
//! golden corpus `syscall_payload_v1_all.bin` into it, and runs
//! `cargo test --offline` there. The scratch crate's tests rebuild the 47
//! canonical frames documented in `compat_golden.rs` with the generated codec
//! and compare them byte-for-byte.

#![allow(missing_docs)]
#![allow(clippy::panic)]

use crate::src_gen::codegen_cfg::CodegenCfg;
use crate::src_gen::gen_message::gen_message_with_cfg;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::fs::sync::{
    sync_copy, sync_create_dir_all, sync_path_exists, sync_remove_dir_all, sync_write,
};
use std::path::PathBuf;

const SCRATCH_CARGO_TOML: &str = r#"
[package]
name = "mgen_rt"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = { version = "1", features = ["derive"] }
rmp = "0.8"
rmp-serde = "1.3"
serde_repr = "0.1"
"#;

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

// Miri cannot execute FFI calls into the tree-sitter C parser, and spawning
// cargo makes no sense under Miri either.
#[test]
#[cfg_attr(miri, ignore)]
fn generated_rust_codec_reproduces_golden_corpus() -> RS<()> {
    let manifest = manifest_dir();
    let wit_dir = manifest.join("../../common/mudu_binding/wit");
    let golden_bin =
        manifest.join("../../db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin");
    let golden_test = manifest.join("src/src_gen/fixtures/syscall_golden_test.rs");
    if !sync_path_exists(&golden_bin) {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("missing golden corpus {}", golden_bin.display())
        ));
    }

    let root = mudu_sys::env_var::temp_dir().join("mudu_gen_syscall_roundtrip");
    let _ = sync_remove_dir_all(&root);
    let universal_dir = root.join("src").join("universal");
    sync_create_dir_all(&universal_dir)?;

    let mut cfg = CodegenCfg::new();
    cfg.with_func_codec = true;
    gen_message_with_cfg(
        &wit_dir,
        &universal_dir,
        "rust".to_string(),
        None,
        cfg,
        None,
    )?;

    // The generated code converts through the hand-written wire runtime;
    // mirror it into the scratch crate's `universal` module and register it.
    let mp_wire_src = manifest.join("../../common/mudu_binding/src/universal/mp_wire.rs");
    sync_copy(&mp_wire_src, universal_dir.join("mp_wire.rs"))?;
    let mod_rs = universal_dir.join("mod.rs");
    let mut mod_content = mudu_sys::fs::sync::sync_read_to_string(&mod_rs)?;
    mod_content.push_str("pub mod mp_wire;\n");
    sync_write(&mod_rs, &mod_content)?;

    sync_write(root.join("src").join("lib.rs"), "pub mod universal;\n")?;
    sync_write(root.join("Cargo.toml"), SCRATCH_CARGO_TOML)?;
    sync_create_dir_all(root.join("tests"))?;
    sync_copy(&golden_test, root.join("tests").join("golden.rs"))?;
    sync_create_dir_all(root.join("golden"))?;
    sync_copy(
        &golden_bin,
        root.join("golden").join("syscall_payload_v1_all.bin"),
    )?;

    let output = mudu_sys::process::Command::new("cargo")
        .args(["test", "--offline"])
        .current_dir(&root)
        // The CI sanitizer job runs the outer tests with
        // RUSTFLAGS="-Z sanitizer=address". The scratch crate must build
        // uninstrumented: an ASAN-instrumented proc-macro .so cannot be
        // dlopened by the plain rustc that the inner cargo invokes
        // (undefined __asan_* symbols).
        .env_remove("RUSTFLAGS")
        .env_remove("CARGO_ENCODED_RUSTFLAGS")
        .env_remove("RUSTDOCFLAGS")
        .output()
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Internal,
                format!("failed to spawn cargo for the scratch crate: {}", e)
            )
        })?;
    if !output.status.success() {
        panic!(
            "scratch crate cargo test failed with status {:?}\nstdout:\n{}\nstderr:\n{}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let _ = sync_remove_dir_all(&root);
    Ok(())
}
