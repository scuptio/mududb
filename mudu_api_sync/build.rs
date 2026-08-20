use mudu_build_common::{
    collect_universal_files, copy_file_if_changed, generate_demo_manifest, generate_sdk_manifest,
    generate_universal_mod, read_workspace_versions, remove_stale_files, repo_root,
    rerun_if_changed, write_if_changed,
};
use mudu_sys_impl::fs::sync::{
    sync_create_dir_all as create_dir_all, sync_read_to_string as read_to_string,
};
use std::path::Path;

/// The SDK mirrors only the self-contained universal type definitions.
///
/// `*_impl.rs` conversion helpers and the `*_test.rs` / `test_uni.rs` unit
/// tests depend on workspace crates (`mudu`, `mudu_type`, `mudu_contract`)
/// that the standalone SDK does not have, so they are excluded from the
/// mirror.
fn is_sdk_mirror_file(file: &str) -> bool {
    !file.ends_with("_impl.rs") && !file.ends_with("_test.rs") && file != "test_uni.rs"
}

/// Copies one universal source file into the SDK mirror, dropping any
/// trailing `#[cfg(test)]` modules.
///
/// The universal sources keep their unit tests at the end of the file and
/// those tests use workspace-only crates, so the mirrored copy strips
/// everything from the first `#[cfg(test)]` marker onward. The type
/// definitions above the marker are mirrored verbatim.
fn copy_universal_file_if_changed(
    source: &Path,
    target: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let content = read_to_string(source)?;
    let stripped = match content.find("#[cfg(test)]") {
        Some(index) => format!("{}\n", content[..index].trim_end()),
        None => content,
    };
    write_if_changed(target, &stripped)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = repo_root()?;

    let root_manifest_path = repo_root.join("Cargo.toml");
    let root_manifest = read_to_string(&root_manifest_path)?;
    rerun_if_changed(&root_manifest_path);

    let keys = [
        "rmp-serde",
        "serde",
        "serde_repr",
        "tokio",
        "wit-bindgen",
        "rusqlite",
    ];
    let workspace_versions = read_workspace_versions(&root_manifest, &keys)?;

    let sdk_root = repo_root.join("mudu_api").join("rust");
    let sdk_src_universal = sdk_root.join("src").join("universal");
    let sdk_wit_dir = sdk_root.join("wit");
    let demo_dir = sdk_root.join("demo");

    create_dir_all(&sdk_src_universal)?;
    create_dir_all(&sdk_wit_dir)?;
    create_dir_all(demo_dir.join("src"))?;

    let universal_src_root = repo_root.join("mudu_binding").join("src").join("universal");
    rerun_if_changed(&universal_src_root);
    let universal_files = collect_universal_files(&universal_src_root)?
        .into_iter()
        .filter(|file| is_sdk_mirror_file(file))
        .collect::<Vec<_>>();
    for file in &universal_files {
        let source = universal_src_root.join(file);
        rerun_if_changed(&source);
        let target = sdk_src_universal.join(file);
        copy_universal_file_if_changed(&source, &target)?;
    }
    remove_stale_files(&sdk_src_universal, &universal_files, Some("mod.rs"))?;

    let generated_mod = generate_universal_mod(&universal_files);
    write_if_changed(&sdk_src_universal.join("mod.rs"), &generated_mod)?;

    let wit_source = repo_root
        .join("sys_interface")
        .join("wit")
        .join("async")
        .join("async-api.wit");
    rerun_if_changed(&wit_source);
    copy_file_if_changed(&wit_source, &sdk_wit_dir.join("async-api.wit"))?;

    let sdk_manifest = generate_sdk_manifest(&workspace_versions);
    write_if_changed(&sdk_root.join("Cargo.toml"), &sdk_manifest)?;

    let demo_manifest = generate_demo_manifest(&workspace_versions);
    write_if_changed(&demo_dir.join("Cargo.toml"), &demo_manifest)?;

    Ok(())
}
