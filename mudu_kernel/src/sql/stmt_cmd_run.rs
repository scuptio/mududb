use crate::contract::ssn_ctx::SsnCtx;
use crate::sql::current_tx::get_tx;
use crate::sql::stmt_cmd::StmtCmd;
use mudu::common::result::RS;
use mudu_utils::task_trace;
use tracing::error;

// Run a command statement
// DDL includes: Create/Alter Table
// DML includes: Insert/Update/Delete
pub async fn run_cmd_stmt(stmt: &dyn StmtCmd, ctx: &dyn SsnCtx) -> RS<u64> {
    let trace = task_trace!();
    trace.watch("procedure.run_cmd.stage", "get_tx_start");
    let _xid = get_tx(ctx).await?;
    trace.watch("procedure.run_cmd.stage", "get_tx_done");
    let r = run_cmd_stmt_gut(stmt, ctx).await;
    match r {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("run command error: {}", e);
            //ctx.thd_ctx().abort_tx(_xid).await?;
            ctx.end_tx()?;
            Err(e)
        }
    }
}

async fn run_cmd_stmt_gut(stmt: &dyn StmtCmd, ctx: &dyn SsnCtx) -> RS<u64> {
    let trace = task_trace!();
    trace.watch("procedure.run_cmd_gut.stage", "realize_start");
    stmt.realize(ctx).await?;
    trace.watch("procedure.run_cmd_gut.stage", "realize_done");
    trace.watch("procedure.run_cmd_gut.stage", "build_start");
    let cmd = stmt.build(ctx).await?;
    trace.watch("procedure.run_cmd_gut.stage", "build_done");
    trace.watch("procedure.run_cmd_gut.stage", "prepare_start");
    cmd.prepare().await?;
    trace.watch("procedure.run_cmd_gut.stage", "prepare_done");
    trace.watch("procedure.run_cmd_gut.stage", "run_start");
    cmd.run().await?;
    trace.watch("procedure.run_cmd_gut.stage", "run_done");
    trace.watch("procedure.run_cmd_gut.stage", "affected_rows_start");
    let rows = cmd.affected_rows().await?;
    trace.watch("procedure.run_cmd_gut.stage", "affected_rows_done");
    Ok(rows)
}
