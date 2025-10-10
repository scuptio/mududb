use tracing::error;
use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::load_mududb_cfg;

fn main() {
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