//! Tests for the `mudud` CLI argument parser.
#![allow(missing_docs)]

use crate::Args;
use clap::Parser;

#[test]
fn args_parse_with_cfg() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::try_parse_from(["mudud", "--cfg", "/tmp/mududb.toml"])?;
    assert_eq!(args.cfg_path, Some("/tmp/mududb.toml".to_string()));
    Ok(())
}

#[test]
fn args_parse_without_cfg() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::try_parse_from(["mudud"])?;
    assert_eq!(args.cfg_path, None);
    Ok(())
}

#[test]
fn args_parse_rejects_unknown_flag() {
    let result = Args::try_parse_from(["mudud", "--unknown"]);
    assert!(result.is_err());
}
