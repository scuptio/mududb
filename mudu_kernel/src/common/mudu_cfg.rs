use home::home_dir;
use mudu::common::result::RS;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use toml;

const CFG_TOML_PATH: &str = ".mudu/cfg.toml";

const LOG_FILE_EXT: &str = "xl";

/// log file size limit 10MB
const LOG_FILE_LIMIT: u64 = 1024 * 1024 * 10;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct MuduCfg {
    pub server_listen_port: u16,
    pub server_bind_address: String,
    pub db_path: String,
    pub session_threads: u32,
    pub x_log_folder: String,
    pub x_log_ext_name: String,
    pub x_log_channels: u32,
    pub x_log_file_size_limit: u64,
    pub x_log_use_io_uring: bool,
}

impl Default for MuduCfg {
    fn default() -> Self {
        MuduCfg {
            server_listen_port: 5432,
            server_bind_address: "0.0.0.0".to_string(),
            db_path: "/tmp/mudu/".to_string(),
            session_threads: 4,
            x_log_folder: "x_log".to_string(),
            x_log_ext_name: LOG_FILE_EXT.to_string(),
            x_log_channels: 4,
            x_log_file_size_limit: LOG_FILE_LIMIT,
            x_log_use_io_uring: false,
        }
    }
}

pub fn load_mudu_conf(opt_cfg_path: Option<String>) -> RS<MuduCfg> {
    let cfg_path = match opt_cfg_path {
        Some(cfg_path) => PathBuf::from(cfg_path),
        None => {
            let opt_home = home_dir();
            let home_path = match opt_home {
                Some(p) => p,
                None => return Err(m_error!(ER::IOErr, "no home path env setting")),
            };
            home_path.join(CFG_TOML_PATH)
        }
    };

    if cfg_path.exists() {
        let cfg = read_mudu_conf(cfg_path)?;
        Ok(cfg)
    } else {
        let cfg = MuduCfg::default();
        write_mudu_conf(cfg_path, &cfg)?;
        Ok(cfg)
    }
}

fn read_mudu_conf<P: AsRef<Path>>(path: P) -> RS<MuduCfg> {
    let r = fs::read_to_string(path);
    let s = r.map_err(|e|
        m_error!(ER::IOErr, "read Mudu configuration error", e)
    )?;
    let r = toml::from_str::<MuduCfg>(s.as_str());
    let cfg =
        r.map_err(|e|
            m_error!(ER::IOErr, "deserialization configuration file error", e)
        )?;
    Ok(cfg)
}

fn write_mudu_conf<P: AsRef<Path>>(path: P, cfg: &MuduCfg) -> RS<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| {
                    m_error!(ER::IOErr, "create directory error", e)
                })?;
        }
    }
    let r = toml::to_string(cfg);
    let s = r.map_err(|e| {
        m_error!(ER::EncodeErr, "serialize configuration error", e)
    })?;

    let r = fs::write(path, s);
    r.map_err(|e| m_error!(ER::IOErr, "write configuration file error", e))?;
    Ok(())
}

#[cfg(test)]
mod _test {
    use crate::common::mudu_cfg::{read_mudu_conf, write_mudu_conf, MuduCfg};
    #[test]
    fn test_conf() {
        let cfg = MuduCfg::default();
        let path = "/tmp/mudu/cfg.toml".to_string();
        let r = write_mudu_conf(path.clone(), &cfg);
        assert!(r.is_ok());
        let r = read_mudu_conf(path.clone());
        assert!(r.is_ok());
        let conf1 = r.unwrap();
        assert_eq!(conf1, cfg);
    }
}
