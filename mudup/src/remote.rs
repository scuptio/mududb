use anyhow::{Context, Result, anyhow};
use serde::Deserialize;
use std::fs;
use std::path::PathBuf;

use crate::config::Config;

#[derive(Debug, Deserialize)]
pub(crate) struct ChannelManifest {
    latest: String,
    releases: Vec<Release>,
}

#[derive(Debug, Deserialize)]
struct Release {
    version: String,
    artifacts: Vec<ReleaseArtifact>,
}

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct ReleaseArtifact {
    pub(crate) host: String,
    pub(crate) url: String,
    pub(crate) sha256: String,
}

pub(crate) async fn fetch_channel_manifest(cfg: &Config) -> Result<ChannelManifest> {
    let url = format!("{}/{}.toml", cfg.base_url, cfg.channel);
    let text = reqwest::get(&url)
        .await
        .with_context(|| format!("download channel manifest {url}"))?
        .error_for_status()
        .with_context(|| format!("channel manifest request failed: {url}"))?
        .text()
        .await?;
    toml::from_str(&text).with_context(|| format!("parse channel manifest {url}"))
}

pub(crate) async fn fetch_sha256(url: &str) -> Result<String> {
    let text = reqwest::get(url)
        .await
        .with_context(|| format!("download checksum {url}"))?
        .error_for_status()
        .with_context(|| format!("checksum request failed: {url}"))?
        .text()
        .await?;
    text.split_whitespace()
        .next()
        .map(str::to_string)
        .ok_or_else(|| anyhow!("empty checksum response from {url}"))
}

pub(crate) fn select_channel_artifact(
    channel: &ChannelManifest,
    host: &str,
) -> Result<ReleaseArtifact> {
    let latest = channel
        .releases
        .iter()
        .find(|release| release.version == channel.latest)
        .ok_or_else(|| anyhow!("latest release {} not found in channel", channel.latest))?;

    latest
        .artifacts
        .iter()
        .find(|artifact| artifact.host == host)
        .cloned()
        .ok_or_else(|| anyhow!("no artifact for host {host} in release {}", latest.version))
}

pub(crate) async fn download_artifact(
    cfg: &Config,
    artifact: &ReleaseArtifact,
    version: &str,
) -> Result<PathBuf> {
    let filename = artifact
        .url
        .rsplit('/')
        .next()
        .filter(|name| !name.is_empty())
        .ok_or_else(|| anyhow!("artifact URL has no filename: {}", artifact.url))?;
    let archive_path = cfg.root.join("downloads").join(filename);

    let bytes = reqwest::get(&artifact.url)
        .await
        .with_context(|| format!("download artifact {}", artifact.url))?
        .error_for_status()
        .with_context(|| format!("artifact request failed: {}", artifact.url))?
        .bytes()
        .await?;
    fs::write(&archive_path, bytes)?;
    println!("downloaded {version} to {}", archive_path.display());
    Ok(archive_path)
}

pub(crate) fn artifact_version(url: &str) -> Option<String> {
    url.rsplit('/')
        .nth(1)
        .filter(|version| version.starts_with('v'))
        .map(str::to_string)
}
