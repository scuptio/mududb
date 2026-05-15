use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

use crate::config::{DEFAULT_BASE_URL, DEFAULT_CHANNEL};

#[derive(Parser, Debug)]
#[command(name = "mudup")]
#[command(version)]
#[command(about = "MuduDB toolchain installer and version manager")]
pub(crate) struct Cli {
    #[arg(long, global = true, help = "Override the mudup root directory.")]
    pub(crate) root: Option<PathBuf>,
    #[arg(
        long,
        global = true,
        default_value = DEFAULT_BASE_URL,
        help = "Base URL for release artifacts."
    )]
    pub(crate) base_url: String,
    #[arg(
        long,
        global = true,
        default_value = DEFAULT_CHANNEL,
        help = "Release channel used by update and install stable."
    )]
    pub(crate) channel: String,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Install a version such as v20260514.1144, or install the channel with stable.
    Install(InstallArgs),
    /// Install the latest version from the configured channel.
    Update,
    /// List installed toolchains.
    List,
    /// Remove an installed version.
    Uninstall(UninstallArgs),
}

#[derive(Args, Debug)]
pub(crate) struct InstallArgs {
    #[arg(help = "Version to install, or stable to use the configured channel.")]
    pub(crate) version: String,
}

#[derive(Args, Debug)]
pub(crate) struct UninstallArgs {
    pub(crate) version: String,
}
