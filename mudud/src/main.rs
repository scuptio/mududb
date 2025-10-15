use tracing::error;
use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::load_mududb_cfg;
use mudu_utils::log::log_setup_ex;

fn main() {
    log_setup_ex("info", "mudu=info,mudu_runtime=info", false);
    let r = serve();
    match r {
        Ok(_) => {}
        Err(e) => {
            error!("mududb serve run error: {}", e);
        }
    }
}


fn serve() -> RS<()> {
    let cfg = load_mududb_cfg(None)?;
    Backend::sync_serve(cfg)?;
    Ok(())
}