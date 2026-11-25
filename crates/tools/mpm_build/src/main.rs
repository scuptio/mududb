//! Command-line binary for the `mpm_build` crate.
//!
//! This binary implements the `mpm-build` tool, which can create Mudu APP package
//! archives (flat `.mpk` + `.mpk.zip`) and merge procedure-description files.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

use anyhow::{Result, anyhow};
use clap::{Arg, Command};
use mpm_build::merge_desc::merge_desc_files;
use mudu::common::app_info::AppInfo;
use mudu::compat::SYSCALL_PAYLOAD_CURRENT_VERSION;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_caf::profile::PackageSpec;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_sys::fs::sync::{SFile, sync_create_dir_all, sync_path_exists};
use mudu_utils::json::read_json;
use mudu_utils::toml::read_toml;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(test)]
use std::ffi::OsString;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Serialize, Deserialize)]
enum MpmBuildCommand {
    Package(MpmBuildPackage),
    MergeDesc(MpmBuildMergeDesc),
}

#[derive(Debug, Serialize, Deserialize)]
struct MpmBuildMergeDesc {
    input_folder: String,
    output_desc_file: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MpmBuildPackage {
    package_cfg: String,
    package_desc: String,
    ddl_sql: String,
    initdb_sql: String,
    wasm_files: Vec<String>,
    output_path: String,
    /// Optional ed25519 signing seed (64 hex chars). Defaults to the
    /// `MUDUDB_MPK_SIGN_SEED` environment variable when unset.
    #[serde(default)]
    sign_seed: Option<String>,
}

impl MpmBuildPackage {
    fn validate(&self) -> Result<()> {
        // Check if required files exist
        let required_files = [
            (&self.package_cfg, "package.cfg.json"),
            (&self.package_desc, "package.desc.json"),
            (&self.ddl_sql, "ddl.sql"),
            (&self.initdb_sql, "initdb.sql"),
        ];

        for (path, name) in required_files {
            if !sync_path_exists(path) {
                return Err(anyhow!("Required file '{}' not found at: {}", name, path));
            }
        }

        // Check if we have at least one WASM file
        if self.wasm_files.is_empty() {
            return Err(anyhow!("At least one bytecode file is required"));
        }

        // Check if all WASM files exist and have correct extension
        for wasm_path in &self.wasm_files {
            if !sync_path_exists(wasm_path) {
                return Err(anyhow!("WASM file not found: {}", wasm_path));
            }
            if PathBuf::from(wasm_path)
                .extension()
                .map(|ext| ext != "wasm")
                .unwrap_or(true)
            {
                return Err(anyhow!(
                    "WASM file must have .wasm extension: {}",
                    wasm_path
                ));
            }
        }

        validate_desc_matches_wasm_modules(&self.package_desc, &self.wasm_files)?;

        Ok(())
    }
}

fn validate_desc_matches_wasm_modules(
    package_desc_path: &str,
    wasm_files: &[String],
) -> Result<()> {
    let package_desc: ModProcDesc = read_json(package_desc_path)?;
    let desc_modules = package_desc
        .modules()
        .keys()
        .cloned()
        .collect::<HashSet<_>>();
    let wasm_modules = wasm_files
        .iter()
        .map(|path| {
            PathBuf::from(path)
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(str::to_string)
                .ok_or_else(|| anyhow!("Invalid WASM file name: {}", path))
        })
        .collect::<Result<HashSet<_>>>()?;

    if desc_modules != wasm_modules {
        return Err(anyhow!(
            "package.desc.json modules {:?} do not match wasm file names {:?}",
            desc_modules,
            wasm_modules
        ));
    }
    Ok(())
}

fn build_command() -> Command {
    Command::new("mpm-build")
        .version("0.1.0")
        .about("Package management tool for creating Mudu APP packages")
        .subcommand_required(true)
        .subcommand(
            Command::new("create")
                .about("Create package from argument vector")
                .arg(
                    Arg::new("package-cfg")
                        .long("package-cfg")
                        .short('a')
                        .value_name("FILE")
                        .help("Path to package configuration file")
                        .required(true),
                )
                .arg(
                    Arg::new("package-desc")
                        .long("package-desc")
                        .short('p')
                        .value_name("FILE")
                        .help("Path to package description(the list of procedural function signature) file")
                        .required(true),
                )
                .arg(
                    Arg::new("ddl-sql")
                        .long("ddl-sql")
                        .short('d')
                        .value_name("FILE")
                        .help("Path to data definition language SQL file")
                        .required(true),
                )
                .arg(
                    Arg::new("initdb-sql")
                        .long("initdb-sql")
                        .short('i')
                        .value_name("FILE")
                        .help("Path to database initializing SQL file")
                        .required(true),
                )
                .arg(
                    Arg::new("wasm-files")
                        .long("wasm-files")
                        .short('w')
                        .value_name("FILES")
                        .help("List of wasm files (space separated)")
                        .required(true)
                        .num_args(1..),
                )
                .arg(
                    Arg::new("output")
                        .long("output")
                        .short('o')
                        .value_name("FILE")
                        .help("Output package archive file path")
                        .required(false),
                )
                .arg(
                    Arg::new("sign-seed")
                        .long("sign-seed")
                        .value_name("HEX")
                        .help("ed25519 signing seed (64 hex chars); falls back to MUDUDB_MPK_SIGN_SEED")
                        .required(false),
                ),
        )
        .subcommand(
            Command::new("create-from-toml")
                .about("Create package from argument vector")
                .arg(
                    Arg::new("toml")
                        .long("toml")
                        .short('t')
                        .value_name("FILE")
                        .help("Path to argument list toml file")
                        .required(true),
                ),
        )
        .subcommand(
            Command::new("merge-desc")
                .about("Merge description files into one description file from a collection of a description files")
                .arg(
                    Arg::new("input-folder")
                        .long("input-folder")
                        .short('f')
                        .value_name("FOLDER")
                        .help("Path to folder contains procedure description files")
                        .required(true),
                )
                .arg(
                    Arg::new("output-desc-file")
                        .long("output-desc-file")
                        .short('d')
                        .value_name("FILE")
                        .help("Path to the output description file")
                        .required(true),
                ),
        )
}

fn parse_from_matches(matches: &clap::ArgMatches) -> Result<MpmBuildCommand> {
    let mut mpk_cmd = match matches.subcommand() {
        Some(("create", sub_matches)) => {
            let cmd = MpmBuildPackage {
                package_cfg: sub_matches
                    .get_one::<String>("package-cfg")
                    .ok_or_else(|| anyhow!("No package-cfg specified"))?
                    .clone(),
                package_desc: sub_matches
                    .get_one::<String>("package-desc")
                    .ok_or_else(|| anyhow!("No package-desc specified"))?
                    .clone(),
                ddl_sql: sub_matches
                    .get_one::<String>("ddl-sql")
                    .ok_or_else(|| anyhow!("No ddl-sql specified"))?
                    .clone(),
                initdb_sql: sub_matches
                    .get_one::<String>("initdb-sql")
                    .ok_or_else(|| anyhow!("No initdb-sql specified"))?
                    .clone(),
                wasm_files: sub_matches
                    .get_many::<String>("wasm-files")
                    .ok_or_else(|| anyhow!("No wasm-files specified"))?
                    .cloned()
                    .collect(),
                output_path: sub_matches
                    .get_one::<String>("output")
                    .cloned()
                    .unwrap_or(Default::default()),
                sign_seed: sub_matches.get_one::<String>("sign-seed").cloned(),
            };
            MpmBuildCommand::Package(cmd)
        }
        Some(("create-from-toml", sub_matches)) => {
            let toml_path = PathBuf::from(
                sub_matches
                    .get_one::<String>("toml")
                    .ok_or_else(|| anyhow!("No toml argument file specified"))?,
            );
            let cmd: MpmBuildPackage = read_toml::<MpmBuildPackage, _>(&toml_path)?;
            MpmBuildCommand::Package(cmd)
        }
        Some(("merge-desc", sub_matches)) => {
            let desc = MpmBuildMergeDesc {
                input_folder: sub_matches
                    .get_one::<String>("input-folder")
                    .ok_or_else(|| anyhow!("No input-folder specified"))?
                    .clone(),
                output_desc_file: sub_matches
                    .get_one::<String>("output-desc-file")
                    .ok_or_else(|| anyhow!("No output-desc-file specified"))?
                    .clone(),
            };
            MpmBuildCommand::MergeDesc(desc)
        }
        _ => return Err(anyhow!("No valid subcommand specified")),
    };
    if let MpmBuildCommand::Package(pkg) = &mut mpk_cmd {
        if pkg.output_path.is_empty() {
            let app_cfg: AppInfo = read_json(&pkg.package_cfg)
                .map_err(|e| anyhow!("Error parsing app-cfg file: {}", e))?;
            let default_output = format!("{}.mpk", app_cfg.name);
            pkg.output_path = default_output;
        }
        if pkg.sign_seed.is_none() {
            pkg.sign_seed = mudu_sys::env_var::var("MUDUDB_MPK_SIGN_SEED");
        }
        pkg.validate()?;
    }

    Ok(mpk_cmd)
}

fn parse_arguments() -> Result<MpmBuildCommand> {
    let matches = build_command().get_matches();
    parse_from_matches(&matches)
}

#[cfg(test)]
fn parse_arguments_from<I, T>(args: I) -> Result<MpmBuildCommand>
where
    I: IntoIterator<Item = T>,
    T: Into<OsString> + Clone,
{
    let matches = build_command().try_get_matches_from(args)?;
    parse_from_matches(&matches)
}

fn read_text_file<P: AsRef<Path>>(path: P) -> Result<String> {
    let mut file = SFile::open(path.as_ref())?;
    let mut text = String::new();
    file.read_to_string(&mut text).map_err(io_error)?;
    Ok(text)
}

fn read_bytes_file<P: AsRef<Path>>(path: P) -> Result<Vec<u8>> {
    let mut file = SFile::open(path.as_ref())?;
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes).map_err(io_error)?;
    Ok(bytes)
}

fn write_bytes_file<P: AsRef<Path>>(path: P, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = PathBuf::from(path.as_ref()).parent() {
        sync_create_dir_all(parent)?;
    }
    let mut file = SFile::create(path.as_ref())?;
    file.write_all(bytes).map_err(io_error)?;
    Ok(())
}

fn io_error(e: std::io::Error) -> anyhow::Error {
    anyhow::Error::new(mudu_error!(ErrorCode::Io, "file io error", e))
}

/// Builds the flat `.mpk` archive plus its `.mpk.zip` export from the package
/// inputs, signed when a seed is configured.
fn write_package_archive(config: &MpmBuildPackage) -> Result<()> {
    let app_cfg: AppInfo = read_json(&config.package_cfg)?;
    let cfg_text = read_text_file(&config.package_cfg)?;
    let desc_text = read_text_file(&config.package_desc)?;
    let ddl_sql = read_text_file(&config.ddl_sql)?;
    let initdb_sql = read_text_file(&config.initdb_sql)?;

    let mut modules = Vec::with_capacity(config.wasm_files.len());
    for wasm_path in &config.wasm_files {
        let name = PathBuf::from(wasm_path)
            .file_stem()
            .and_then(|stem| stem.to_str())
            .map(str::to_string)
            .ok_or_else(|| anyhow!("Invalid WASM file name: {}", wasm_path))?;
        let bytes = read_bytes_file(wasm_path)?;
        modules.push((name, bytes));
    }

    let sign_seed = match &config.sign_seed {
        Some(hex) => Some(mudu_caf::sign::decode_seed(hex)?),
        None => None,
    };

    let spec = PackageSpec {
        name: app_cfg.name.clone(),
        version: app_cfg.version.clone(),
        cfg_text,
        desc_text,
        ddl_sql,
        initdb_sql,
        syscall_abi_version: SYSCALL_PAYLOAD_CURRENT_VERSION,
        modules,
        sign_seed,
    };

    let flat = mudu_caf::profile::build(&spec)?;
    let zip = mudu_caf::export::export(&flat)?;

    write_bytes_file(&config.output_path, &flat)?;
    let zip_path = format!("{}.zip", config.output_path);
    write_bytes_file(&zip_path, &zip)?;

    Ok(())
}

fn main() -> Result<()> {
    let build_cmd = parse_arguments()?;
    match build_cmd {
        MpmBuildCommand::Package(package) => build_package(package),
        MpmBuildCommand::MergeDesc(description) => build_desc(description),
    }
}

fn build_desc(desc: MpmBuildMergeDesc) -> Result<()> {
    merge_desc_files(desc.input_folder, desc.output_desc_file)
}

fn build_package(config: MpmBuildPackage) -> Result<()> {
    println!("Creating Mudu APP package...");
    println!("Package configuration: {}", config.package_cfg);
    println!("Procedure desc: {}", config.package_desc);
    println!("DDL SQL: {}", config.ddl_sql);
    println!("DB initializing SQL: {}", config.initdb_sql);
    println!("WASM files: {}", config.wasm_files.len());
    for wasm_file in &config.wasm_files {
        println!("  - {}", wasm_file);
    }
    println!("Output: {}", config.output_path);

    write_package_archive(&config)?;

    println!("Package created successfully: {}", config.output_path);
    println!("ZIP export:             {}.zip", config.output_path);

    Ok(())
}

#[cfg(test)]
mod main_test;

#[cfg(test)]
mod cli_test;
