//! Tests for project scaffolding: name validation, template rendering and
//! file materialization.
#![allow(missing_docs)]

use crate::scaffold::{Lang, ScaffoldArgs, derive_names, scaffold, validate_project_name};
use anyhow::{Result, bail};
use mudu_sys::fs::sync::{sync_create_dir_all, sync_path_exists, sync_read_to_string, sync_write};
use std::path::Path;
use tempfile::TempDir;

fn args(name: &str, lang: Lang, path: &TempDir) -> ScaffoldArgs {
    ScaffoldArgs {
        name: name.to_string(),
        lang,
        path: Some(path.path().to_path_buf()),
        sdk_path: None,
    }
}

fn slash(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

fn read(project_dir: &Path, relative: &str) -> Result<String> {
    Ok(sync_read_to_string(project_dir.join(relative))?)
}

#[test]
fn validate_accepts_valid_names() -> Result<()> {
    for name in ["demo", "my-app", "a", "app2", "a".repeat(64).as_str()] {
        validate_project_name(name)?;
    }
    Ok(())
}

#[test]
fn validate_rejects_invalid_names() {
    for name in [
        "",
        "2app",
        "-app",
        "App",
        "my_app",
        "my.app",
        "my app",
        "äpp",
        "a".repeat(65).as_str(),
    ] {
        assert!(
            validate_project_name(name).is_err(),
            "expected '{name}' to be rejected"
        );
    }
}

#[test]
fn derive_names_converts_case_styles() -> Result<()> {
    let names = derive_names("my-app")?;
    assert_eq!(names.project_name, "my-app");
    assert_eq!(names.module_name, "my_app");
    assert_eq!(names.kebab_name, "my-app");
    assert_eq!(names.pascal_name, "MyApp");

    let names = derive_names("demo")?;
    assert_eq!(names.module_name, "demo");
    assert_eq!(names.pascal_name, "Demo");

    let names = derive_names("app2")?;
    assert_eq!(names.module_name, "app2");
    assert_eq!(names.pascal_name, "App2");
    Ok(())
}

#[test]
fn scaffold_rust_renders_without_leftover_placeholders() -> Result<()> {
    let dir = TempDir::new()?;
    let report = scaffold(&args("my-app", Lang::Rust, &dir))?;
    assert_eq!(report.names.module_name, "my_app");

    let cargo_toml = read(&report.project_dir, "Cargo.toml")?;
    assert!(cargo_toml.contains("name = \"my_app\""));
    assert!(cargo_toml.contains("mududb = { version = \"0.1\", features = [\"async\"] }"));
    assert!(!cargo_toml.contains("workspace"));

    let cfg = read(&report.project_dir, "package/package.cfg.json")?;
    assert!(cfg.contains("\"name\": \"my_app\""));
    assert!(cfg.contains("\"use_async\": true"));

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    assert!(makefile.contains("my_app"));
    assert!(!makefile.contains("WORKSPACE_ROOT"));
    assert!(!makefile.contains("transpiler.py"));

    let procedures = read(&report.project_dir, "src/rust/procedures.rs")?;
    assert!(procedures.contains("/**mudu-proc**/"));
    assert!(procedures.contains("pub fn create_item(xid: OID"));

    for expected in [
        ".cargo/config.toml",
        ".gitignore",
        "Cargo.toml",
        "Makefile.toml",
        "readme.md",
        "build-cfg/transpiler-cfg.toml",
        "package/package.cfg.json",
        "sql/ddl.sql",
        "sql/init.sql",
        "src/lib.rs",
        "src/rust/mod.rs",
        "src/rust/procedures.rs",
    ] {
        assert!(
            sync_path_exists(report.project_dir.join(expected)),
            "missing {expected}"
        );
    }
    Ok(())
}

#[test]
fn scaffold_assemblyscript_renders_npm_dependency() -> Result<()> {
    let dir = TempDir::new()?;
    let report = scaffold(&args("demo", Lang::AssemblyScript, &dir))?;

    let package_json = read(&report.project_dir, "package.json")?;
    assert!(package_json.contains("\"@mududb/mududb\": \"^0.1.0\""));

    let procedures = read(&report.project_dir, "assembly/procedures.ts")?;
    assert!(procedures.contains("/**mudu-proc*/"));
    assert!(procedures.contains("from \"@mududb/mududb\""));
    assert!(procedures.contains("update_item_info"));

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    assert!(makefile.contains("WORLD_NAME = \"demo\""));
    assert!(makefile.contains("--type-wit"));
    assert!(!makefile.contains("WORKSPACE_ROOT"));

    for expected in [
        ".gitignore",
        "Makefile.toml",
        "asconfig.json",
        "package.json",
        "readme.md",
        "assembly/procedures.ts",
        "build-cfg/abort-adapter.wat",
        "build-cfg/gentypes-mpack.ts",
        "build-cfg/transpiler-cfg.toml",
        "package/package.cfg.json",
        "scripts/patch-component-exports.py",
        "sql/ddl.sql",
        "sql/init.sql",
        "wit/deps/mududb-api/api.wit",
        "wit/types.wit",
    ] {
        assert!(
            sync_path_exists(report.project_dir.join(expected)),
            "missing {expected}"
        );
    }
    Ok(())
}

#[test]
fn scaffold_csharp_renames_csproj() -> Result<()> {
    let dir = TempDir::new()?;
    let report = scaffold(&args("my-app", Lang::Csharp, &dir))?;

    // The csproj template file name is rendered with the PascalCase name.
    assert!(sync_path_exists(report.project_dir.join("MyApp.csproj")));
    assert!(!sync_path_exists(report.project_dir.join("Project.csproj")));

    let csproj = read(&report.project_dir, "MyApp.csproj")?;
    assert!(csproj.contains("<AssemblyName>my_app</AssemblyName>"));
    assert!(csproj.contains("<RootNamespace>MyApp</RootNamespace>"));
    assert!(csproj.contains("World=\"my-app\""));

    // The world WIT, the WorldExportsImpl wiring and package.desc.json are
    // produced by mtp at build time, not scaffolded.
    let procedures = read(&report.project_dir, "src/Procedures.cs")?;
    assert!(procedures.contains("// mudu-proc"));
    assert!(procedures.contains("UpdateItemInfo"));

    let mudu_sys = read(&report.project_dir, "src/MuduSys.cs")?;
    assert!(mudu_sys.contains("RecordFieldValues"));
    assert!(mudu_sys.contains("EncodeProcedureOkRecord"));

    let csproj_content = read(&report.project_dir, "MyApp.csproj")?;
    assert!(csproj_content.contains("MessagePack"));

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    assert!(makefile.contains("mtp"));
    assert!(makefile.contains("--type-wit"));

    let nuget = read(&report.project_dir, "nuget.config")?;
    assert!(nuget.contains("https://api.nuget.org/v3/index.json"));
    assert!(nuget.contains("dotnet-experimental"));

    for expected in [
        ".gitignore",
        "Makefile.toml",
        "nuget.config",
        "readme.md",
        "package/package.cfg.json",
        "sql/ddl.sql",
        "sql/init.sql",
        "src/MiniMsgPack.cs",
        "src/MuduSys.cs",
        "src/Procedures.cs",
        "wit/deps/mududb-api/api.wit",
        "wit/types.wit",
    ] {
        assert!(
            sync_path_exists(report.project_dir.join(expected)),
            "missing {expected}"
        );
    }
    Ok(())
}

#[test]
fn scaffold_python_renders_sdk_path_placeholder() -> Result<()> {
    let dir = TempDir::new()?;
    let report = scaffold(&args("demo", Lang::Python, &dir))?;

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    assert!(makefile.contains("PY_BINDINGS = \"CHANGE_ME_SITE_PACKAGES\""));
    assert!(!makefile.contains("WORKSPACE_ROOT"));

    let procedures = read(&report.project_dir, "procedures.py")?;
    assert!(procedures.contains("# mudu-proc"));
    assert!(procedures.contains("def create_item(session: UniOid"));

    for expected in [
        ".gitignore",
        "Makefile.toml",
        "procedures.py",
        "readme.md",
        "package/package.cfg.json",
        "sql/ddl.sql",
        "sql/init.sql",
        "wit/deps/api/api.wit",
        "wit/types.wit",
    ] {
        assert!(
            sync_path_exists(report.project_dir.join(expected)),
            "missing {expected}"
        );
    }
    Ok(())
}

#[test]
fn scaffold_rejects_non_empty_destination() -> Result<()> {
    let dir = TempDir::new()?;
    let project_dir = dir.path().join("demo");
    sync_create_dir_all(&project_dir)?;
    sync_write(project_dir.join("existing.txt"), "occupied")?;

    let result = scaffold(&args("demo", Lang::Rust, &dir));
    let Err(err) = result else {
        bail!("expected scaffolding into a non-empty directory to fail");
    };
    assert!(format!("{err:?}").contains("already exists and is not empty"));
    Ok(())
}

#[test]
fn scaffold_sdk_path_rewrites_rust_dependency() -> Result<()> {
    let dir = TempDir::new()?;
    let sdk = dir.path().join("mududb-checkout");
    sync_create_dir_all(sdk.join("crates/sdk/mududb"))?;

    let mut scaffold_args = args("demo", Lang::Rust, &dir);
    scaffold_args.sdk_path = Some(sdk.clone());
    let report = scaffold(&scaffold_args)?;

    let cargo_toml = read(&report.project_dir, "Cargo.toml")?;
    let expected_dep = format!(
        "mududb = {{ path = \"{}\", features = [\"async\"] }}",
        slash(&sdk.join("crates/sdk/mududb"))
    );
    assert!(cargo_toml.contains(&expected_dep), "got:\n{cargo_toml}");
    Ok(())
}

#[test]
fn scaffold_sdk_path_rewrites_as_dependency() -> Result<()> {
    let dir = TempDir::new()?;
    let sdk = dir.path().join("mududb-checkout");
    sync_create_dir_all(sdk.join("crates/sdk/bindings/assemblyscript"))?;

    let mut scaffold_args = args("demo", Lang::AssemblyScript, &dir);
    scaffold_args.sdk_path = Some(sdk.clone());
    let report = scaffold(&scaffold_args)?;

    let package_json = read(&report.project_dir, "package.json")?;
    let expected_dep = format!(
        "\"@mududb/mududb\": \"file:{}\"",
        slash(&sdk.join("crates/sdk/bindings/assemblyscript"))
    );
    assert!(package_json.contains(&expected_dep));
    Ok(())
}

#[test]
fn scaffold_sdk_path_rewrites_python_bindings_path() -> Result<()> {
    let dir = TempDir::new()?;
    let sdk = dir.path().join("mududb-checkout");
    sync_create_dir_all(sdk.join("crates/sdk/bindings/python"))?;

    let mut scaffold_args = args("demo", Lang::Python, &dir);
    scaffold_args.sdk_path = Some(sdk.clone());
    let report = scaffold(&scaffold_args)?;

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    let expected = format!(
        "PY_BINDINGS = \"{}\"",
        slash(&sdk.join("crates/sdk/bindings/python"))
    );
    assert!(makefile.contains(&expected));
    Ok(())
}

#[test]
fn scaffold_sdk_path_rejects_invalid_checkout() -> Result<()> {
    let dir = TempDir::new()?;
    let sdk = dir.path().join("not-a-mududb-checkout");
    sync_create_dir_all(&sdk)?;

    let mut scaffold_args = args("demo", Lang::Rust, &dir);
    scaffold_args.sdk_path = Some(sdk);
    let result = scaffold(&scaffold_args);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn scaffold_csharp_rejects_sdk_path() -> Result<()> {
    let dir = TempDir::new()?;
    let mut scaffold_args = args("demo", Lang::Csharp, &dir);
    scaffold_args.sdk_path = Some(dir.path().to_path_buf());
    let result = scaffold(&scaffold_args);
    assert!(result.is_err());
    Ok(())
}

#[test]
fn scaffold_uses_existing_empty_directory() -> Result<()> {
    let dir = TempDir::new()?;
    sync_create_dir_all(dir.path().join("demo"))?;
    let report = scaffold(&args("demo", Lang::Rust, &dir))?;
    assert!(sync_path_exists(report.project_dir.join("Cargo.toml")));
    Ok(())
}

#[test]
fn scaffold_c_renders_freestanding_project() -> Result<()> {
    let dir = TempDir::new()?;
    let report = scaffold(&args("my-app", Lang::C, &dir))?;

    let cfg = read(&report.project_dir, "package/package.cfg.json")?;
    assert!(cfg.contains("\"name\": \"my_app\""));
    assert!(cfg.contains("\"lang\": \"c\""));

    let world_wit_missing = report.project_dir.join("wit/world.wit");
    assert!(!sync_path_exists(world_wit_missing));

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    assert!(makefile.contains("WASI_SDK"));
    assert!(makefile.contains("mtp"));
    assert!(!makefile.contains("WORKSPACE_ROOT"));

    let procedures = read(&report.project_dir, "src/procedures.c")?;
    assert!(procedures.contains("// mudu-proc (item_id: i64, name: string) -> i64"));

    for expected in [
        ".gitignore",
        "Makefile.toml",
        "readme.md",
        "package/package.cfg.json",
        "sql/ddl.sql",
        "sql/init.sql",
        "src/mudu_sys.c",
        "src/mudu_sys.h",
        "src/mududb/codec/mpack.c",
        "src/mududb/codec/mpack.h",
        "src/mududb/codec/mpack_alloc.c",
        "src/mududb/codec/record_bridge.c",
        "src/mududb/codec/record_bridge.h",
        "src/mududb/types/UniDataValue.h",
        "src/mududb/types/UniScalarValue.h",
        "src/mududb/types/UniSyscall.h",
        "src/procedures.c",
        "wit/deps/mududb-api/api.wit",
        "wit/types.wit",
    ] {
        assert!(
            sync_path_exists(report.project_dir.join(expected)),
            "missing {expected}"
        );
    }
    Ok(())
}

#[test]
fn scaffold_go_renders_pre_generated_bindings() -> Result<()> {
    let dir = TempDir::new()?;
    let report = scaffold(&args("my-app", Lang::Go, &dir))?;

    let go_mod = read(&report.project_dir, "go.mod")?;
    assert!(go_mod.contains("module my_app"));

    let readme = read(&report.project_dir, "readme.md")?;
    assert!(readme.contains("Experimental"));

    // The Exports wiring (main_gen.go), the world WIT and package.desc.json
    // are produced by mtp at build time, not scaffolded.
    let procedures = read(&report.project_dir, "procedures.go")?;
    assert!(procedures.contains("// mudu-proc"));

    let makefile = read(&report.project_dir, "Makefile.toml")?;
    assert!(makefile.contains("mtp"));

    for expected in [
        ".gitignore",
        "Makefile.toml",
        "go.mod",
        "mpack.go",
        "mudusys.go",
        "procedures.go",
        "readme.md",
        "binding/mududb/api/system/system.wit.go",
        "binding/mududb/my-app/my-app/my-app.exports.go",
        "binding/mududb/my-app/my-app/my-app.wasm.go",
        "binding/mududb/my-app/my-app/my-app.wit.go",
        "package/package.cfg.json",
        "sql/ddl.sql",
        "sql/init.sql",
        "vendor/modules.txt",
        "vendor/go.bytecodealliance.org/cm/list.go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/codec/mpack.go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/UniDataValue.go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/bridge.go",
        "vendor/github.com/ybbh/mududb_p/bindings/go/types/wire.go",
        "wit/deps/cli/imports.wit",
        "wit/deps/mududb-api/api.wit",
        "wit/types.wit",
    ] {
        assert!(
            sync_path_exists(report.project_dir.join(expected)),
            "missing {expected}"
        );
    }
    Ok(())
}

#[test]
fn scaffold_c_and_go_reject_sdk_path() -> Result<()> {
    for lang in [Lang::C, Lang::Go] {
        let dir = TempDir::new()?;
        let mut scaffold_args = args("demo", lang, &dir);
        scaffold_args.sdk_path = Some(dir.path().to_path_buf());
        assert!(
            scaffold(&scaffold_args).is_err(),
            "expected --sdk-path to be rejected for {lang}"
        );
    }
    Ok(())
}
