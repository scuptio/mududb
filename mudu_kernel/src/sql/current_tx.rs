use crate::contract::ssn_ctx::SsnCtx;
use mudu::common::result::RS;
use mudu::common::xid::XID;


pub async fn get_tx(ctx: &dyn SsnCtx) -> RS<XID> {
    let opt_tx = ctx.current_tx();
    let xid = match opt_tx {
        Some(id) => id,
        None => {
            let id = todo!();
            ctx.begin_tx(id)?;
            id
        }
    };
    Ok(xid)
}
