use crate::contract::ssn_ctx::SsnCtx;
use crate::sql::current_tx::get_tx;
use crate::sql::stmt_cmd::StmtCmd;
use mudu::common::result::RS;
use tracing::error;

// Run a command statement
// DDL includes: Create/Alter Table
// DML includes: Insert/Update/Delete
pub async fn run_cmd_stmt(stmt: &dyn StmtCmd, ctx: &dyn SsnCtx) -> RS<u64> {
    let xid = get_tx(ctx).await?;
    let r = run_cmd_stmt_gut(stmt, ctx).await;
    match r {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("run command error: {}", e);
            //ctx.thd_ctx().abort_tx(xid).await?;
            ctx.end_tx()?;
            Err(e)
        }
    }
}

async fn run_cmd_stmt_gut(stmt: &dyn StmtCmd, ctx: &dyn SsnCtx) -> RS<u64> {
    stmt.realize(ctx).await?;
    let cmd = stmt.build(ctx).await?;
    cmd.prepare().await?;
    cmd.run().await?;
    let rows = cmd.affected_rows().await?;
    Ok(rows)
}
