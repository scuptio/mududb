//! Fail closed when a stream-bearing Raft group lacks its business owner.
use mudu::common::result::RS;
use mudu::{error::ErrorCode, mudu_error};
use mudu_raft::{ConfValue, MembershipFence, RaftHandle, ReplEntry, UpdateConfWaiter};

pub(super) async fn protocol_only_change(
    raft: &RaftHandle,
    expected: mudu_raft::stream::MembershipVersion,
    conf: ConfValue,
    voters: Vec<u64>,
    members: Vec<u64>,
) -> RS<UpdateConfWaiter> {
    let term = raft.term();
    raft.propose(ReplEntry::Barrier { term })?.wait().await?;
    let mut cursor = 0;
    let frontier = loop {
        let batch = raft.read_committed_async(cursor, 256).await?;
        if batch.snapshot.index != 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "snapshot requires a replication-group membership owner"
            ));
        }
        for (index, entry) in &batch.entries {
            if !matches!(
                entry,
                ReplEntry::Barrier { .. } | ReplEntry::XidAlloc { .. }
            ) {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    "external streams require a replication-group membership owner"
                ));
            }
            cursor = index.index;
        }
        if cursor >= batch.committed_index {
            break cursor;
        }
    };
    let (old, new) = raft.conf_async().await?;
    if !expected.matches(&old, &new) {
        return Err(mudu_error!(
            ErrorCode::ResourceBusy,
            "requested membership was derived from a stale configuration"
        ));
    }
    let proof = MembershipFence {
        term,
        membership: mudu_raft::stream::MembershipVersion::from_conf(&old, &new),
        committed_index: frontier,
        guards: vec![],
    };
    raft.update_conf_fenced_async(conf, voters, members, Some(proof))
        .await
        .map_err(Into::into)
}
