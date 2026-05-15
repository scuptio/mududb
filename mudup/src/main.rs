mod archive;
mod checksum;
mod cli;
mod config;
mod libs;
mod remote;
mod toolchain;
mod util;

use anyhow::Result;
use clap::Parser;

use crate::archive::{extract_toolchain, validate_toolchain};
use crate::checksum::verify_sha256;
use crate::cli::{Cli, Commands};
use crate::config::{Config, host_triple};
use crate::libs::check_system_libraries;
use crate::remote::{
    ReleaseArtifact, artifact_version, download_artifact, fetch_channel_manifest, fetch_sha256,
    select_channel_artifact,
};
use crate::toolchain::{
    activate_toolchain, ensure_layout, list_toolchains, print_path_hint, refresh_proxies, uninstall,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

async fn run() -> Result<()> {
    let cli = Cli::parse();
    let cfg = Config::new(cli.root, cli.base_url, cli.channel)?;

    match cli.command {
        Commands::Install(args) => install(&cfg, &args.version).await,
        Commands::Update => install(&cfg, "stable").await,
        Commands::List => list_toolchains(&cfg),
        Commands::Uninstall(args) => uninstall(&cfg, &args.version),
    }
}

async fn install(cfg: &Config, requested: &str) -> Result<()> {
    ensure_layout(&cfg.root)?;

    let host = host_triple()?;
    let artifact = if requested == "stable" {
        let channel = fetch_channel_manifest(cfg).await?;
        select_channel_artifact(&channel, &host)?
    } else {
        let url = format!(
            "{}/{}/mududb-{}-{}.tar.gz",
            cfg.base_url, requested, requested, host
        );
        let sha256_url = format!("{url}.sha256");
        let sha256 = fetch_sha256(&sha256_url).await?;
        ReleaseArtifact { host, url, sha256 }
    };

    let version = artifact_version(&artifact.url).unwrap_or_else(|| requested.to_string());
    let archive_path = download_artifact(cfg, &artifact, &version).await?;
    verify_sha256(&archive_path, &artifact.sha256)?;
    check_system_libraries(cfg, &archive_path, &version)?;

    let install_dir = extract_toolchain(cfg, &archive_path, &version)?;
    validate_toolchain(&install_dir)?;
    activate_toolchain(cfg, &version)?;
    refresh_proxies(cfg)?;

    println!("installed {version} for {}", artifact.host);
    print_path_hint(cfg);
    Ok(())
}
