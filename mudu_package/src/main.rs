mod merge_desc;

use crate::merge_desc::merge_desc_files;
use anyhow::{anyhow, Result};
use clap::{Arg, Command};
use mudu::common::package_cfg::PackageCfg;
use mudu::utils::toml::read_toml;
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Debug, Serialize, Deserialize)]
enum MPKCommand {
    Package(MPKPackage),
    MergeDesc(MPKMergeDesc),
}

#[derive(Debug, Serialize, Deserialize)]
struct MPKMergeDesc {
    input_folder: String,
    output_desc_file: String,
}

#[derive(Debug, Serialize, Deserialize)]
struct MPKPackage {
    package_cfg: String,
    package_desc: String,
    ddl_sql: String,
    initdb_sql: String,
    wasm_files: Vec<String>,
    output_path: String,
}

impl MPKPackage {
    fn validate(&self) -> Result<()> {
        // Check if required files exist
        let required_files = [
            (&self.package_cfg, "package.cfg.toml"),
            (&self.package_desc, "package.desc.json"),
            (&self.ddl_sql, "ddl.sql"),
            (&self.initdb_sql, "initdb.sql"),
        ];

        for (path, name) in required_files {
            if !fs::exists(path)? {
                return Err(anyhow!("Required file '{}' not found at: {}", name, path));
            }
        }

        // Check if we have at least one WASM file
        if self.wasm_files.is_empty() {
            return Err(anyhow!("At least one bytecode file is required"));
        }

        // Check if all WASM files exist and have correct extension
        for wasm_path in &self.wasm_files {
            if !fs::exists(wasm_path)? {
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

        Ok(())
    }
}

fn parse_arguments() -> Result<MPKCommand> {
    let matches = Command::new("mudu-package-tool")
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
                )
        )
        .subcommand(
            Command::new("merge-desc")
                .about("Merge description files into one description file from a collection of a description files")
                .arg(
                    Arg::new("input-folder")
                        .long("input-folder")
                        .short('f')
                        .value_name("FOLDER")
                        .help("Path to folder contains *.desc.toml list toml file")
                        .required(true),
                )
                .arg(
                    Arg::new("output-desc-file")
                        .long("output-desc-file")
                        .short('d')
                        .value_name("FILE")
                        .help("Path to the output description file")
                        .required(true),
                )
        )
        .get_matches();
    let mut mpk_cmd = match matches.subcommand() {
        Some(("create", sub_matches)) => {
            let cmd = MPKPackage {
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
            MPKCommand::Package(cmd)
        }
        Some(("create-from-toml", sub_matches)) => {
            let toml_path = PathBuf::from(
                sub_matches
                    .get_one::<String>("toml")
                    .ok_or_else(|| anyhow!("No toml argument file specified"))?,
            );
            let cmd: MPKPackage = read_toml::<MPKPackage, _>(&toml_path)?;
            MPKCommand::Package(cmd)
        }
        Some(("merge-desc", sub_matches)) => {
            let desc = MPKMergeDesc {
                input_folder: sub_matches
                    .get_one::<String>("input-folder")
                    .ok_or_else(|| anyhow!("No input-folder specified"))?
                    .clone(),
                output_desc_file: sub_matches
                    .get_one::<String>("output-desc-file")
                    .ok_or_else(|| anyhow!("No output-desc-file specified"))?
                    .clone(),
            };
            MPKCommand::MergeDesc(desc)
        }
        _ => return Err(anyhow!("No valid subcommand specified")),
    };
    if let MPKCommand::Package(pkg) = &mut mpk_cmd {
        if pkg.output_path.is_empty() {
            let app_cfg: PackageCfg = read_toml(&pkg.package_cfg)
                .map_err(|e| anyhow!("Error parsing app-cfg file: {}", e))?;
            let default_output = format!("{}.mpk", app_cfg.name);
            pkg.output_path = default_output;
        }
        pkg.validate()?;
    }

    Ok(mpk_cmd)
}

fn add_file_to_zip<P: AsRef<Path>>(
    zip_writer: &mut ZipWriter<File>,
    file_path: P,
    zip_path: &str,
) -> Result<()> {
    let mut file = File::open(file_path.as_ref())?;
    zip_writer.start_file(
        zip_path,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    io::copy(&mut file, zip_writer)?;
    Ok(())
}

fn create_package(config: &MPKPackage) -> Result<()> {
    // Create output directory if it doesn't exist
    if let Some(parent) = PathBuf::from(&config.output_path).parent() {
        fs::create_dir_all(parent)?;
    }

    // Create zip file
    let file = File::create(&config.output_path)?;
    let mut zip = ZipWriter::new(file);

    // Add required files with their specific names
    add_file_to_zip(&mut zip, &config.package_cfg, "package.cfg.toml")?;
    add_file_to_zip(&mut zip, &config.package_desc, "package.desc.json")?;
    add_file_to_zip(&mut zip, &config.ddl_sql, "ddl.sql")?;
    add_file_to_zip(&mut zip, &config.initdb_sql, "initdb.sql")?;

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
    let mpk_cmd = parse_arguments()?;
    match mpk_cmd {
        MPKCommand::Package(package) => create_mpk_package(package),
        MPKCommand::MergeDesc(description) => build_desc(description),
    }
}

fn build_desc(desc: MPKMergeDesc) -> Result<()> {
    merge_desc_files(desc.input_folder, desc.output_desc_file)
}

fn create_mpk_package(config: MPKPackage) -> Result<()> {
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

    create_package(&config)?;

    println!("Package created successfully: {}", config.output_path);

    // Print package contents
    println!("\nPackage contents:");
    let package_file = File::open(&config.output_path)?;
    let zip_archive = zip::ZipArchive::new(package_file)?;
    for file_name in zip_archive.file_names() {
        println!("  - {}", file_name);
    }

    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_files(dir: &Path) -> Result<()> {
        let files = [
            ("package.cfg.toml", "[app]\nname = \"test\"\n"),
            ("package.desc.json", "[procedure]\nversion = \"1.0\"\n"),
            ("ddl.sql", "CREATE TABLE test (id INT);"),
            ("initdb.sql", "INSERT INTO test VALUES (1);"),
            ("test1.wasm", "mock wasm content"),
            ("test2.wasm", "mock wasm content 2"),
        ];

        for (filename, content) in files {
            let mut file = File::create(dir.join(filename))?;
            write!(file, "{}", content)?;
        }

        Ok(())
    }

    #[test]
    fn test_package_creation() -> Result<()> {
        let temp_dir = TempDir::new()?;
        create_test_files(temp_dir.path())?;

        let config = MPKPackage {
            package_cfg: temp_dir
                .path()
                .join("package.cfg.toml")
                .to_str()
                .unwrap()
                .to_string(),
            package_desc: temp_dir
                .path()
                .join("package.desc.json")
                .to_str()
                .unwrap()
                .to_string(),
            ddl_sql: temp_dir
                .path()
                .join("ddl.sql")
                .to_str()
                .unwrap()
                .to_string(),
            initdb_sql: temp_dir
                .path()
                .join("initdb.sql")
                .to_str()
                .unwrap()
                .to_string(),
            wasm_files: vec![
                temp_dir
                    .path()
                    .join("test1.wasm")
                    .to_str()
                    .unwrap()
                    .to_string(),
                temp_dir
                    .path()
                    .join("test2.wasm")
                    .to_str()
                    .unwrap()
                    .to_string(),
            ],
            output_path: temp_dir
                .path()
                .join("test.mudu")
                .to_str()
                .unwrap()
                .to_string(),
        };

        config.validate()?;
        create_package(&config)?;

        // Verify the package was created and contains expected files
        assert!(PathBuf::from(&config.output_path).exists());

        let package_file = File::open(&config.output_path)?;
        let mut zip_archive = zip::ZipArchive::new(package_file)?;

        let expected_files = [
            "package.cfg.toml",
            "package.desc.json",
            "ddl.sql",
            "initdb.sql",
            "test1.wasm",
            "test2.wasm",
        ];

        for expected_file in expected_files {
            assert!(zip_archive.by_name(expected_file).is_ok());
        }

        Ok(())
    }
}
