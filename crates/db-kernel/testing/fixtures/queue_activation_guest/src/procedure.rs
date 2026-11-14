//! Source for the activation fixture, transpiled with mtp.
use mududb::common::{id::OID, result::RS};
use mududb::contract::{sql_params, sql_stmt};
use mududb::sys_interface::sync_api::mudu_command;

/**mudu-proc**/
/// Records a successful activation through the real guest syscall path.
pub fn activated(xid: OID, queue: String) -> RS<()> {
    mudu_command(xid, sql_stmt!(&"UPDATE activation_receipts SET hits = hits + 1 WHERE queue_name = ?"), sql_params!(&(queue,)))?;
    Ok(())
}
