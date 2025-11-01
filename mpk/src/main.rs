use anyhow::{anyhow, Result};
use clap::{Arg, Command};
use mudu::common::app_cfg::AppCfg;
use mudu::utils::toml::read_toml;
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Debug)]
struct PackageConfig {
    app_config_path: PathBuf,
    procedure_desc_path: PathBuf,
    ddl_sql_path: PathBuf,
    initdb_sql_path: PathBuf,
    wasm_files: Vec<PathBuf>,
    output_path: PathBuf,
}

impl PackageConfig {
    fn validate(&self) -> Result<()> {
        // Check if required files exist
        let required_files = [
            (&self.app_config_path, "app.cfg.toml"),
            (&self.procedure_desc_path, "procedure.desc.toml"),
            (&self.ddl_sql_path, "ddl.sql"),
            (&self.initdb_sql_path, "initdb.sql"),
        ];

        for (path, name) in required_files {
            if !path.exists() {
                return Err(anyhow!(
                    "Required file '{}' not found at: {}",
                    name,
                    path.display()
                ));
            }
        }

        // Check if we have at least one WASM file
        if self.wasm_files.is_empty() {
            return Err(anyhow!("At least one bytecode file is required"));
        }

        // Check if all WASM files exist and have correct extension
        for wasm_path in &self.wasm_files {
            if !wasm_path.exists() {
                return Err(anyhow!("WASM file not found: {}", wasm_path.display()));
            }
            if wasm_path
                .extension()
                .map(|ext| ext != "wasm")
                .unwrap_or(true)
            {
                return Err(anyhow!(
                    "WASM file must have .wasm extension: {}",
                    wasm_path.display()
                ));
            }
        }

        Ok(())
    }
}

fn parse_arguments() -> Result<PackageConfig> {
    let matches = Command::new("mudu-package-tool")
        .version("0.1.0")
        .about("Package management tool for creating Mudu APP packages")
        .arg(
            Arg::new("app-cfg")
                .long("app-cfg")
                .short('a')
                .value_name("FILE")
                .help("Path to app.cfg.toml file")
                .required(true),
        )
        .arg(
            Arg::new("procedure-desc")
                .long("procedure-desc")
                .short('p')
                .value_name("FILE")
                .help("Path to procedure.desc.toml file")
                .required(true),
        )
        .arg(
            Arg::new("ddl-sql")
                .long("ddl-sql")
                .short('d')
                .value_name("FILE")
                .help("Path to ddl.sql file")
                .required(true),
        )
        .arg(
            Arg::new("initdb-sql")
                .long("initdb-sql")
                .short('i')
                .value_name("FILE")
                .help("Path to initdb.sql file")
                .required(true),
        )
        .arg(
            Arg::new("bytecode-files")
                .long("bytecode-files")
                .short('w')
                .value_name("FILES")
                .help("List of bytecode file paths (space separated)")
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
        .get_matches();

    let bytecode_files: Vec<PathBuf> = matches
        .get_many::<String>("bytecode-files")
        .ok_or_else(|| anyhow!("No bytecode-files specified"))?
        .map(PathBuf::from)
        .collect();
    let app_cfg_file = PathBuf::from(matches.get_one::<String>("app-cfg")
        .ok_or_else(|| anyhow!("No app-cfg specified"))?);
    let app_cfg: AppCfg = read_toml(&app_cfg_file)
        .map_err(|e| anyhow!("Error parsing app-cfg file: {}", e))?;
    let default_output = format!("{}.mpk", app_cfg.name);
    let output_path = matches
        .get_one::<String>("output")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(default_output));

    let config = PackageConfig {
        app_config_path: app_cfg_file,
        procedure_desc_path: PathBuf::from(matches.get_one::<String>("procedure-desc")
            .ok_or_else(||anyhow!("No procedure-desc specified"))?),
        ddl_sql_path: PathBuf::from(matches.get_one::<String>("ddl-sql")
            .ok_or_else(||anyhow!("No ddl-sql specified"))?),
        initdb_sql_path: PathBuf::from(matches.get_one::<String>("initdb-sql")
            .ok_or_else(|| anyhow!("No initdb-sql specified"))?),
        wasm_files: bytecode_files,
        output_path,
    };
    config.validate()?;
    Ok(config)
}

fn add_file_to_zip(
    zip_writer: &mut ZipWriter<File>,
    file_path: &Path,
    zip_path: &str,
) -> Result<()> {
    let mut file = File::open(file_path)?;
    zip_writer.start_file(
        zip_path,
        SimpleFileOptions::default().compression_method(CompressionMethod::Stored),
    )?;
    io::copy(&mut file, zip_writer)?;
    Ok(())
}

fn create_package(config: &PackageConfig) -> Result<()> {
    // Create output directory if it doesn't exist
    if let Some(parent) = config.output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Create zip file
    let file = File::create(&config.output_path)?;
    let mut zip = ZipWriter::new(file);

    // Add required files with their specific names
    add_file_to_zip(&mut zip, &config.app_config_path, "app.cfg.toml")?;
    add_file_to_zip(&mut zip, &config.procedure_desc_path, "procedure.desc.toml")?;
    add_file_to_zip(&mut zip, &config.ddl_sql_path, "ddl.sql")?;
    add_file_to_zip(&mut zip, &config.initdb_sql_path, "initdb.sql")?;

    // Add WASM files with their original names
    for wasm_path in &config.wasm_files {
        if let Some(file_name) = wasm_path.file_name() {
            if let Some(file_name_str) = file_name.to_str() {
                add_file_to_zip(&mut zip, wasm_path, file_name_str)?;
            } else {
                return Err(anyhow!("Invalid WASM file name: {}", wasm_path.display()));
            }
        } else {
            return Err(anyhow!("Invalid WASM file path: {}", wasm_path.display()));
        }
    }

    zip.finish()?;
    Ok(())
}

fn main() -> Result<()> {
    let config = parse_arguments()?;

    println!("Creating Mudu APP package...");
    println!("App configuration: {}", config.app_config_path.display());
    println!("Procedure desc: {}", config.procedure_desc_path.display());
    println!("DDL SQL: {}", config.ddl_sql_path.display());
    println!("InitDB SQL: {}", config.initdb_sql_path.display());
    println!("WASM files: {}", config.wasm_files.len());
    for wasm_file in &config.wasm_files {
        println!("  - {}", wasm_file.display());
    }
    println!("Output: {}", config.output_path.display());

    create_package(&config)?;

    println!(
        "Package created successfully: {}",
        config.output_path.display()
    );

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
            ("app.cfg.toml", "[app]\nname = \"test\"\n"),
            ("procedure.desc.toml", "[procedure]\nversion = \"1.0\"\n"),
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

        let config = PackageConfig {
            app_config_path: temp_dir.path().join("app.cfg.toml"),
            procedure_desc_path: temp_dir.path().join("procedure.desc.toml"),
            ddl_sql_path: temp_dir.path().join("ddl.sql"),
            initdb_sql_path: temp_dir.path().join("initdb.sql"),
            wasm_files: vec![
                temp_dir.path().join("test1.wasm"),
                temp_dir.path().join("test2.wasm"),
            ],
            output_path: temp_dir.path().join("test.mudu"),
        };

        config.validate()?;
        create_package(&config)?;

        // Verify the package was created and contains expected files
        assert!(config.output_path.exists());

        let package_file = File::open(&config.output_path)?;
        let mut zip_archive = zip::ZipArchive::new(package_file)?;

        let expected_files = [
            "app.cfg.toml",
            "procedure.desc.toml",
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
