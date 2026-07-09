//! Command-line binary for the `mpm_build` crate.
//!
//! This binary implements the `mpm-build` tool, which can create Mudu APP package
//! archives and merge procedure-description files.

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
use mudu::utils::json::to_json_str;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_sys::fs::sync::{SFile, sync_create_dir_all, sync_path_exists};
use mudu_utils::json::read_json;
use mudu_utils::toml::read_toml;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
#[cfg(test)]
use std::ffi::OsString;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

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

fn add_file_to_zip<P: AsRef<Path>>(
    zip_writer: &mut ZipWriter<SFile>,
    file_path: P,
    zip_path: &str,
) -> Result<()> {
    let mut file = SFile::open(file_path.as_ref())?;
    zip_writer.start_file(
        zip_path,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    io::copy(&mut file, zip_writer)?;
    Ok(())
}

fn add_bytes_to_zip(zip_writer: &mut ZipWriter<SFile>, bytes: &[u8], zip_path: &str) -> Result<()> {
    zip_writer.start_file(
        zip_path,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    zip_writer.write_all(bytes)?;
    Ok(())
}

#[derive(Debug, Serialize)]
struct PackageManifest {
    format_version: u16,
    files: Vec<String>,
}

fn write_package_archive(config: &MpmBuildPackage) -> Result<()> {
    // Create output directory if it doesn't exist
    if let Some(parent) = PathBuf::from(&config.output_path).parent() {
        sync_create_dir_all(parent)?;
    }

    // Create zip file
    let file = SFile::create(&config.output_path)?;
    let mut zip = ZipWriter::new(file);

    // Build and embed a manifest for forward/backward-compatible extensions.
    let mut file_list = vec![
        "package.cfg.json".to_string(),
        "package.desc.json".to_string(),
        "ddl.sql".to_string(),
        "initdb.sql".to_string(),
    ];
    for wasm_path in &config.wasm_files {
        let wasm_path = PathBuf::from(wasm_path);
        let file_name = wasm_path
            .file_name()
            .and_then(|v| v.to_str())
            .ok_or_else(|| anyhow!("Invalid WASM file path: {}", wasm_path.display()))?;
        file_list.push(file_name.to_string());
    }
    file_list.push("package.manifest.json".to_string());
    let manifest = PackageManifest {
        format_version: 1,
        files: file_list,
    };
    let manifest_text =
        to_json_str(&manifest).map_err(|e| anyhow!("encode package manifest error: {e}"))?;

    // Add required files with their specific names
    add_file_to_zip(&mut zip, &config.package_cfg, "package.cfg.json")?;
    add_file_to_zip(&mut zip, &config.package_desc, "package.desc.json")?;
    add_file_to_zip(&mut zip, &config.ddl_sql, "ddl.sql")?;
    add_file_to_zip(&mut zip, &config.initdb_sql, "initdb.sql")?;
    add_bytes_to_zip(&mut zip, manifest_text.as_bytes(), "package.manifest.json")?;

    // Add WASM files with their original names
    for wasm_path in &config.wasm_files {
        if let Some(file_name) = PathBuf::from(wasm_path).file_name() {
            if let Some(file_name_str) = file_name.to_str() {
                add_file_to_zip(&mut zip, wasm_path, file_name_str)?;
            } else {
                return Err(anyhow!("Invalid WASM file name: {}", wasm_path));
            }
        } else {
            return Err(anyhow!("Invalid WASM file path: {}", wasm_path));
        }
    }

    zip.finish()?;
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

    // Print package contents
    println!("\nPackage contents:");
    let package_file = SFile::open(&config.output_path)?;
    let zip_archive = zip::ZipArchive::new(package_file)?;
    for file_name in zip_archive.file_names() {
        println!("  - {}", file_name);
    }

    Ok(())
}
#[cfg(test)]
mod main_test;

#[cfg(test)]
mod cli_test;
