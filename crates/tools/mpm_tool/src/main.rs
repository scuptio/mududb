//! `mpm-tool`: MuduDB 应用包（`.mpk` / `.mpk.zip`）查看、转换与签名工具。
//!
//! 参考 `caf-tool` 的命令面实现，但完全自研，不依赖 caf 仓库。

use anyhow::{Result, bail};
use clap::{Parser, Subcommand};
use std::path::{Path, PathBuf};

#[derive(Parser)]
#[command(name = "mpm-tool", version, about = "MuduDB .mpk package tool")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Verify a flat `.mpk` artifact.
    Verify {
        /// Flat `.mpk` input.
        input: PathBuf,
    },
    /// Show a human-readable summary of a flat `.mpk` artifact.
    Inspect {
        /// Flat `.mpk` input.
        input: PathBuf,
    },
    /// List payload files and components of a flat `.mpk` artifact.
    Ls {
        /// Flat `.mpk` input.
        input: PathBuf,
    },
    /// Export a flat `.mpk` to `.mpk.zip`.
    Export {
        /// Flat `.mpk` input.
        input: PathBuf,
        /// `.mpk.zip` output.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Import a `.mpk.zip` back to a flat `.mpk`.
    Import {
        /// `.mpk.zip` input.
        input: PathBuf,
        /// Flat `.mpk` output.
        #[arg(short, long)]
        output: PathBuf,
    },
    /// Generate a fresh ed25519 signing seed and print its fingerprint.
    Keygen,
    /// Scaffold a new package project directory.
    New {
        /// Project name.
        name: String,
        /// Output directory (defaults to the project name).
        #[arg(short, long)]
        dir: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Verify { input } => verify(&input),
        Command::Inspect { input } => inspect(&input),
        Command::Ls { input } => ls(&input),
        Command::Export { input, output } => export(&input, &output),
        Command::Import { input, output } => import(&input, &output),
        Command::Keygen => keygen(),
        Command::New { name, dir } => scaffold(&name, dir.as_deref()),
    }
}

fn read_file(path: &Path) -> Result<Vec<u8>> {
    Ok(mudu_sys::fs::sync::sync_read_all(path)?)
}

fn write_file(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(parent) = path.parent() {
        mudu_sys::fs::sync::sync_create_dir_all(parent)?;
    }
    mudu_sys::fs::sync::sync_write(path, bytes)?;
    Ok(())
}

fn verify(input: &Path) -> Result<()> {
    let bytes = read_file(input)?;
    let report = mudu_caf::verify::verify_bytes(&bytes)?;
    println!("OK {} {}", report.name, report.version);
    println!("  root hash:      {}", report.root_hash);
    println!("  manifest hash:  {}", report.manifest_hash);
    println!("  components:     {}", report.components);
    println!("  payload files:  {}", report.payload_files);
    match report.signature_valid {
        Some(true) => {
            println!("  signatures:     valid");
            for fp in &report.signer_fingerprints {
                println!("  signer:         {fp}");
            }
        }
        Some(false) => bail!("signature verification failed"),
        None => println!("  signatures:     none"),
    }
    Ok(())
}

fn inspect(input: &Path) -> Result<()> {
    verify(input)?;
    Ok(())
}

fn ls(input: &Path) -> Result<()> {
    let bytes = read_file(input)?;
    let pkg = mudu_caf::profile::parse(&bytes)?;
    println!("package.cfg.json  (name: {})", pkg.cfg_text);
    println!("package.desc.json");
    println!("ddl.sql");
    if pkg.initdb_sql.is_empty() {
        println!("initdb.sql        (empty)");
    } else {
        println!("initdb.sql");
    }
    println!(
        "abi.json          (syscall_abi_version: {})",
        pkg.syscall_abi_version
    );
    for (name, wasm) in &pkg.modules {
        println!("{name}.wasm         ({} bytes)", wasm.len());
    }
    Ok(())
}

fn export(input: &Path, output: &Path) -> Result<()> {
    let flat = read_file(input)?;
    let zip_bytes = mudu_caf::export::export(&flat)?;
    write_file(output, &zip_bytes)?;
    println!("wrote {}", output.display());
    Ok(())
}

fn import(input: &Path, output: &Path) -> Result<()> {
    let zip_bytes = read_file(input)?;
    let flat = mudu_caf::import::import(&zip_bytes)?;
    write_file(output, &flat)?;
    println!("wrote {}", output.display());
    Ok(())
}

fn keygen() -> Result<()> {
    let mut seed = [0u8; 32];
    let a = mudu_sys::random::uuid_v4().as_u128().to_be_bytes();
    let b = mudu_sys::random::uuid_v4().as_u128().to_be_bytes();
    seed[..16].copy_from_slice(&a);
    seed[16..].copy_from_slice(&b);
    let pubkey = mudu_caf::sign::public_key(&seed);
    let fingerprint = mudu_caf::sign::fingerprint(&pubkey);
    println!("seed:        {}", mudu_caf::hex_encode(&seed));
    println!("public key:  {}", mudu_caf::hex_encode(&pubkey));
    println!("fingerprint: {fingerprint}");
    Ok(())
}

fn scaffold(name: &str, dir: Option<&Path>) -> Result<()> {
    if name.is_empty() {
        bail!("project name must not be empty");
    }
    let base = dir
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from(name));
    mudu_sys::fs::sync::sync_create_dir_all(&base)?;

    let cfg = format!(
        "{{\"name\":\"{name}\",\"lang\":\"rust\",\"version\":\"0.1.0\",\"use_async\":false}}"
    );
    let desc = "{\"modules\":{},\"procedures\":[]}";
    let ddl = "-- schema DDL\n";
    let initdb = "-- initial data SQL\n";

    write_file(&base.join("package.cfg.json"), cfg.as_bytes())?;
    write_file(&base.join("package.desc.json"), desc.as_bytes())?;
    write_file(&base.join("ddl.sql"), ddl.as_bytes())?;
    write_file(&base.join("initdb.sql"), initdb.as_bytes())?;
    println!("scaffolded package project in {}", base.display());
    println!("  build your wasm module, then run:");
    println!(
        "  mpm-build create -a package.cfg.json -p package.desc.json -d ddl.sql -i initdb.sql -w <module>.wasm -o {name}.mpk"
    );
    Ok(())
}
