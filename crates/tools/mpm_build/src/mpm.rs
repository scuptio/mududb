//! Portable project build entry point; delegates to an explicit Python script.
use anyhow::{Context, Result, bail};
use clap::{Parser, Subcommand};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "mpm", about = "Build a MuduDB project")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}
#[derive(Subcommand)]
enum Commands {
    Build {
        #[arg(long, default_value = "mpm.toml")]
        manifest_path: PathBuf,
        #[arg(long)]
        python: Option<String>,
    },
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    build: Build,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Build {
    script: PathBuf,
    #[serde(default)]
    args: Vec<String>,
}

fn main() -> Result<()> {
    let Commands::Build {
        manifest_path,
        python,
    } = Cli::parse().command;
    let manifest_path = if manifest_path.is_absolute() {
        manifest_path
    } else {
        mudu_sys::env_var::current_dir()?.join(manifest_path)
    };
    let manifest: Manifest = mudu_utils::toml::read_toml(&manifest_path)
        .with_context(|| format!("read {}", manifest_path.display()))?;
    let directory = manifest_path.parent().context("manifest has no parent")?;
    let script = directory.join(&manifest.build.script);
    if !script.is_file() {
        bail!("build script not found: {}", script.display());
    }
    let python = python
        .or_else(|| mudu_sys::env_var::var("MPM_PYTHON"))
        .unwrap_or_else(|| if cfg!(windows) { "python" } else { "python3" }.into());
    let status = mudu_sys::process::Command::new(&python)
        .arg(script)
        .args(&manifest.build.args)
        .current_dir(directory)
        .status()
        .with_context(|| format!("start {python}; install Python 3.11+ or set MPM_PYTHON"))?;
    if !status.success() {
        bail!("project build failed: {status}");
    }
    Ok(())
}
