//! Tests for the `mudud` CLI argument parser.
#![allow(missing_docs)]

use crate::{Args, Command, ServeArgs};
use clap::Parser;

#[test]
fn args_parse_serve_with_cfg() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::try_parse_from(["mudud", "serve", "--cfg", "/tmp/mududb.toml"])?;
    match args.command {
        Some(Command::Serve(serve_args)) => {
            assert_eq!(serve_args.cfg_path, Some("/tmp/mududb.toml".to_string()));
        }
        other => assert!(
            matches!(other, Some(Command::Serve(_))),
            "expected serve subcommand"
        ),
    }
    Ok(())
}

#[test]
fn args_parse_serve_with_short_cfg() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::try_parse_from(["mudud", "serve", "-c", "/tmp/mududb.toml"])?;
    match args.command {
        Some(Command::Serve(serve_args)) => {
            assert_eq!(serve_args.cfg_path, Some("/tmp/mududb.toml".to_string()));
        }
        other => assert!(
            matches!(other, Some(Command::Serve(_))),
            "expected serve subcommand"
        ),
    }
    Ok(())
}

#[test]
fn args_parse_without_subcommand_defaults_to_serve() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::try_parse_from(["mudud"])?;
    assert!(args.command.is_none());
    Ok(())
}

#[test]
fn args_parse_init_cfg() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::try_parse_from(["mudud", "init-cfg"])?;
    assert!(matches!(args.command, Some(Command::InitCfg)));
    Ok(())
}

#[test]
fn args_parse_rejects_unknown_flag() {
    let result = Args::try_parse_from(["mudud", "--unknown"]);
    assert!(result.is_err());
}

#[test]
fn serve_args_default_has_no_cfg_path() {
    let args = ServeArgs::default();
    assert_eq!(args.cfg_path, None);
}
