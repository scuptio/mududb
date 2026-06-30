use std::env;
use std::fs;
use std::path::{Path, PathBuf};

const FORBIDDEN_DEPENDENCY: &str = "tokio";

fn main() {
    println!("cargo:rerun-if-changed=Cargo.toml");

    let manifest_path = manifest_path();
    let manifest = read_manifest(&manifest_path);
    let manifest: toml::Table = toml::from_str(&manifest)
        .unwrap_or_else(|error| panic!("failed to parse {}: {error}", manifest_path.display()));

    check_dependency_sections(&manifest, "root");

    if let Some(targets) = manifest.get("target").and_then(toml::Value::as_table) {
        for (target, target_manifest) in targets {
            if let Some(target_manifest) = target_manifest.as_table() {
                check_dependency_sections(target_manifest, target);
            }
        }
    }
}

// Build scripts run before mudu_sys is available, so they must use std for manifest access.
#[allow(clippy::disallowed_methods)]
fn manifest_path() -> PathBuf {
    PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("Cargo.toml")
}

// Build scripts run before mudu_sys is available, so they must use std for manifest access.
#[allow(clippy::disallowed_methods)]
fn read_manifest(manifest_path: &Path) -> String {
    fs::read_to_string(manifest_path)
        .unwrap_or_else(|error| panic!("failed to read {}: {error}", manifest_path.display()))
}

fn check_dependency_sections(manifest: &toml::Table, location: &str) {
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        let Some(dependencies) = manifest.get(section).and_then(toml::Value::as_table) else {
            continue;
        };

        for (dependency_name, specification) in dependencies {
            let package_name = specification
                .get("package")
                .and_then(toml::Value::as_str)
                .unwrap_or(dependency_name);

            assert_ne!(
                package_name, FORBIDDEN_DEPENDENCY,
                "mudu must remain runtime-agnostic: `{FORBIDDEN_DEPENDENCY}` is forbidden in {location}.{section}"
            );
        }
    }
}
