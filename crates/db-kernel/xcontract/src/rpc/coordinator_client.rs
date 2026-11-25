//! Installed data-group authority used by coordinator and recovery paths.
use crate::*;
use catalog::contract::tx_decision::TxDecisionClient;

/// Installs this worker's data-group coordinator API. The service supplies
/// a weak client so holding a worker cannot keep a stopped group alive.
pub fn set_tx_decision_client(
    tx_decision_client: &SMutex<Option<Arc<dyn TxDecisionClient>>>,
    client: Arc<dyn TxDecisionClient>,
) -> RS<()> {
    *tx_decision_client.lock()? = Some(client);
    Ok(())
}

/// Returns the installed data-group decision API; no control-plane fallback.
pub fn tx_decision_client(
    tx_decision_client: &SMutex<Option<Arc<dyn TxDecisionClient>>>,
) -> RS<Arc<dyn TxDecisionClient>> {
    tx_decision_client.lock()?.clone().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidState,
            "coordinator data-group decision API is not installed"
        )
    })
}
