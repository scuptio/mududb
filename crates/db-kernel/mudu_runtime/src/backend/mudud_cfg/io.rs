use super::{DEFAULT_CFG_TEMPLATE, MUDUDB_CFG_FILE_NAME, MUDUDB_CONFIG_DIR, MuduDBCfg};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::path::{Path, PathBuf};

/// File name of the persisted node id inside the database directory.
pub const NODE_ID_FILE_NAME: &str = "node_id";

/// Loads the persistent node id from `<db_path>/node_id`, creating it from the
/// configured value on first boot. A mismatch between the configured node id
/// and the persisted one is a startup error.
pub fn load_or_create_node_id<P: AsRef<Path>>(db_path: P, configured: u64) -> RS<u64> {
    let path = db_path.as_ref().join(NODE_ID_FILE_NAME);
    if mudu_sys::fs::sync::sync_path_exists(&path) {
        let text = mudu_sys::fs::sync::read_to_string(&path)?;
        let persisted = text.trim().parse::<u64>().map_err(|e| {
            mudu_error!(
                ErrorCode::Decode,
                format!("decode node id file {} error", path.display()),
                e
            )
        })?;
        if persisted != configured {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "configured node_id {} does not match persisted node_id {} in {}",
                    configured,
                    persisted,
                    path.display()
                )
            ));
        }
        Ok(persisted)
    } else {
        if let Some(parent) = path.parent()
            && !mudu_sys::fs::sync::sync_path_exists(parent)
        {
            mudu_sys::fs::sync::create_dir_all(parent)?;
        }
        mudu_sys::fs::sync::write(&path, configured.to_string().as_bytes())?;
        Ok(configured)
    }
}

/// Returns the default configuration file path in the current working directory.
///
/// `load_mudud_cfg(None)` checks this path first, then continues the ordered
/// search at `~/.mududb/mudud.cfg`.
pub fn default_cfg_path() -> PathBuf {
    PathBuf::from(MUDUDB_CFG_FILE_NAME)
}

/// Returns the global configuration file path under the user's home directory.
fn global_cfg_path(home_dir: Option<PathBuf>) -> Option<PathBuf> {
    home_dir.map(|home| home.join(MUDUDB_CONFIG_DIR).join(MUDUDB_CFG_FILE_NAME))
}

/// Load a MuduDB configuration from the given path or the default locations.
///
/// When `opt_cfg_path` is `None`, the configuration is searched in order:
///
/// 1. `./mudud.cfg` in the current working directory.
/// 2. `~/.mududb/mudud.cfg` in the user's home directory.
///
/// If neither file exists, an error is returned.
///
/// When `opt_cfg_path` is `Some`, only the specified path is used.
pub fn load_mudud_cfg(opt_cfg_path: Option<String>) -> RS<MuduDBCfg> {
    let search_global_config = opt_cfg_path.is_none();
    let local_cfg_path = opt_cfg_path
        .map(PathBuf::from)
        .unwrap_or_else(default_cfg_path);
    load_mudud_cfg_with_local(
        local_cfg_path,
        mudu_sys::env_var::home_dir(),
        search_global_config,
    )
}

pub(crate) fn load_mudud_cfg_with_local(
    local_cfg_path: PathBuf,
    home_dir: Option<PathBuf>,
    search_global_config: bool,
) -> RS<MuduDBCfg> {
    if mudu_sys::fs::sync::sync_path_exists(&local_cfg_path) {
        return read_mudud_cfg(local_cfg_path);
    }

    if search_global_config
        && let Some(global_path) =
            global_cfg_path(home_dir).filter(|p| mudu_sys::fs::sync::sync_path_exists(p))
    {
        return read_mudud_cfg(global_path);
    }

    Err(mudu_error!(
        ErrorCode::NotFound,
        format!(
            "MuduDB configuration file not found: looked at ./{} and ~/.mududb/{}",
            MUDUDB_CFG_FILE_NAME, MUDUDB_CFG_FILE_NAME
        )
    ))
}

/// Write a default configuration file to the current working directory.
///
/// The file is named `mudud.cfg`. If a file with that name already exists, it
/// is overwritten.
pub fn init_mudud_cfg() -> RS<()> {
    let path = PathBuf::from(MUDUDB_CFG_FILE_NAME);
    write_default_mudud_cfg(&path)
}

fn write_default_mudud_cfg<P: AsRef<Path>>(path: P) -> RS<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !mudu_sys::fs::sync::sync_path_exists(parent)
    {
        mudu_sys::fs::sync::create_dir_all(parent)?;
    }
    mudu_sys::fs::sync::write(path, DEFAULT_CFG_TEMPLATE.as_bytes())?;
    Ok(())
}

pub(super) fn read_mudud_cfg<P: AsRef<Path>>(path: P) -> RS<MuduDBCfg> {
    let s = mudu_sys::fs::sync::read_to_string(path.as_ref())?;
    let r = toml::from_str::<MuduDBCfg>(s.as_str());
    let cfg = r.map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            "deserialization MuduDB configuration file error",
            e
        )
    })?;
    Ok(cfg)
}

#[allow(dead_code)]
pub(super) fn write_mudud_cfg<P: AsRef<Path>>(path: P, cfg: &MuduDBCfg) -> RS<()> {
    let path = path.as_ref();
    if let Some(parent) = path.parent()
        && !mudu_sys::fs::sync::sync_path_exists(parent)
    {
        mudu_sys::fs::sync::create_dir_all(parent)?;
    }
    let r = toml::to_string(cfg);
    let s = r.map_err(|e| mudu_error!(ErrorCode::Encode, "serialize configuration error", e))?;

    mudu_sys::fs::sync::write(path, s.as_bytes())?;
    Ok(())
}
