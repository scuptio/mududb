//! Reconstructed transaction state of one durable prefix.

use catalog::contract::tx_decision::{CoordinatorState, TxDecisionError};
use mudu::common::id::OID;
use mudu_raft::stream::StreamProgress;
use std::collections::BTreeMap;

#[derive(Clone, Default, serde::Serialize, serde::Deserialize)]
pub(super) struct View {
    pub(super) applied: Option<StreamProgress>,
    pub(super) transactions: BTreeMap<OID, CoordinatorState>,
    pub(super) results: BTreeMap<u64, Result<(), TxDecisionError>>,
}
