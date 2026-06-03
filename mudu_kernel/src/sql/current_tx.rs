use crate::contract::ssn_ctx::SsnCtx;
use mudu::common::result::RS;
use mudu::common::xid::{new_xid, OID};

pub async fn get_tx(ctx: &dyn SsnCtx) -> RS<OID> {
    let opt_tx = ctx.current_tx();
    let xid = match opt_tx {
        Some(id) => id,
        None => {
            let id = new_xid();
            ctx.begin_tx(id)?;
            id
        }
    };
    Ok(xid)
}
