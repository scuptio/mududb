use std::env::{home_dir, temp_dir};
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

#[derive(Serialize, Deserialize, Eq, PartialEq, Debug, Clone)]
pub struct MuduDBCfg {
    pub bytecode_path: String,
    pub ddl_path: String,
    pub listen_ip: String,
    pub listen_port: u16,
}

impl Display for MuduDBCfg {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        write!(f, "MuduDB Cfg Setting:\n")?;
        write!(f, "-------------------\n")?;
        write!(f, "  -> Byte code path: {}\n", self.bytecode_path)?;
        write!(f, "  -> DDL sql path: {}\n", self.ddl_path)?;
        write!(f, "  -> Listen IP address: {}\n", self.listen_ip)?;
        write!(f, "  -> Listening port: {}\n", self.listen_port)?;
        write!(f, "-------------------\n")?;
        Ok(())
    }
}

impl Default for MuduDBCfg {
    fn default() -> Self {
        Self {
            bytecode_path: temp_dir().to_str().unwrap().to_string(),
            ddl_path: temp_dir().to_str().unwrap().to_string(),
            listen_ip: temp_dir().to_str().unwrap().to_string(),
            listen_port: 8300,
        }
    }
}

const MUDUDB_CFG_TOML_PATH: &str = ".mudu/mududb_cfg.toml";

pub fn load_mududb_cfg(opt_cfg_path: Option<String>) -> RS<MuduDBCfg> {
    let cfg_path = match opt_cfg_path {
        Some(cfg_path) => PathBuf::from(cfg_path),
        None => {
            let opt_home = home_dir();
            let home_path = match opt_home {
                Some(p) => p,
                None => return Err(m_error!(EC::IOErr, "no home path env setting")),
            };
            home_path.join(MUDUDB_CFG_TOML_PATH)
        }
    };

    if cfg_path.exists() {
        let cfg = read_mududb_cfg(cfg_path)?;
        Ok(cfg)
    } else {
        let cfg = MuduDBCfg::default();
        write_mududb_cfg(cfg_path, &cfg)?;
        Ok(cfg)
    }
}

fn read_mududb_cfg<P: AsRef<Path>>(path: P) -> RS<MuduDBCfg> {
    let r = fs::read_to_string(path);
    let s = r.map_err(|e|
        m_error!(EC::IOErr, "read MuduDB configuration error", e)
    )?;
    let r = toml::from_str::<MuduDBCfg>(s.as_str());
    let cfg =
        r.map_err(|e|
            m_error!(EC::IOErr, "deserialization MuduDB configuration file error", e)
        )?;
    Ok(cfg)
}

fn write_mududb_cfg<P: AsRef<Path>>(path: P, cfg: &MuduDBCfg) -> RS<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent)
                .map_err(|e| {
                    m_error!(EC::IOErr, "create directory error", e)
                })?;
        }
    }
    let r = toml::to_string(cfg);
    let s = r.map_err(|e| {
        m_error!(EC::EncodeErr, "serialize configuration error", e)
    })?;

    let r = fs::write(path, s);
    r.map_err(|e| m_error!(EC::IOErr, "write configuration file error", e))?;
    Ok(())
}

#[cfg(test)]
mod _test {
    use std::env::temp_dir;
    use crate::backend::mududb_cfg::{read_mududb_cfg, write_mududb_cfg, MuduDBCfg};
    #[test]
    fn test_conf() {
        let cfg = MuduDBCfg::default();
        let path = temp_dir().join("mudu/mududb_cfg.toml");
        let r = write_mududb_cfg(path.clone(), &cfg);
        assert!(r.is_ok());
        let r = read_mududb_cfg(path.clone());
        assert!(r.is_ok());
        let conf1 = r.unwrap();
        assert_eq!(conf1, cfg);
    }
}
