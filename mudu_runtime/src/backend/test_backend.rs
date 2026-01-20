
#[cfg(test)]
pub mod tests {
    use crate::backend::backend::Backend;
    use crate::backend::mududb_cfg::MuduDBCfg;
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use mudu::common::result::RS;
    use std::env::temp_dir;
    use std::fs;

    fn test_db_path() -> String {
        let tmp = temp_dir().join(format!("test_bakend_{}", uuid::Uuid::new_v4().to_string()));
        if !tmp.as_path().exists() {
            fs::create_dir_all(tmp.as_path()).unwrap();
        }
        tmp.to_str().unwrap().to_string()
    }

    fn _cfg() -> MuduDBCfg {
        let cfg = MuduDBCfg {
            mpk_path: wasm_mod_path(),
            data_path: test_db_path(),
            listen_ip: "0.0.0.0".to_string(),
            http_listen_port: 8000,
            pg_listen_port: 5432,
            enable_p2: true,
            enable_async: false,
        };
        cfg
    }

    pub fn test_backend() -> RS<()> {
        let cfg = _cfg();
        Backend::sync_serve(cfg)?;
        Ok(())
    }

}