//! Membership changes are fenced by a complete external-data catch-up proof.
use super::{member_ids, rpc, GenericReplicationGroup};
use crate::log_stream::state::invalid;
use mudu::common::result::RS;
use mudu_raft::stream::MembershipVersion;
use mudu_raft::{ConfValue, MembershipFence, UpdateConfWaiter};

/// Verifies every resulting voter holds the complete committed stream
/// vector, then admits the change against that exact Raft prefix. New voters
/// must first join as learners. Stream owners remain locked until admission;
/// concurrent registry/protocol changes invalidate the owner-side fence.
pub(super) async fn update_membership(
    group: &GenericReplicationGroup,
    expected: MembershipVersion,
    conf_value: ConfValue,
    voters: Vec<u64>,
    members: Vec<u64>,
) -> RS<UpdateConfWaiter> {
    group.ensure_running()?;
    let mut owners = Vec::with_capacity(group.streams.len());
    for slot in group.streams.values() {
        owners.push(slot.owner.lock().await);
    }
    let (term, state) = group.registry.leader_view().await?;
    let (old, new) = group.raft.conf_async().await?;
    let membership = MembershipVersion::from_conf(&old, &new);
    if membership != expected {
        return Err(invalid(
            "membership changed while deriving the requested configuration",
        ));
    }
    if membership.committed != membership.incoming {
        return Err(invalid("membership change already in progress"));
    }
    let known = member_ids(&old, &new);
    if voters.iter().any(|peer| !known.contains(peer)) {
        return Err(invalid("new voters must join as learners before promotion"));
    }
    let mut guards = vec![group.running.clone()];
    for peer in &voters {
        if *peer == group.raft.node_id() {
            for progress in state.committed().values() {
                if !group
                    .slot(progress.stream_id)?
                    .log
                    .contains(progress)
                    .await?
                {
                    return Err(invalid("local voter is missing committed stream data"));
                }
            }
        } else if !state.committed().is_empty() {
            let session = rpc(group.transport.session(*peer)).await?;
            for progress in state.committed().values() {
                let actual = rpc(group.transport.progress_at(
                    *peer,
                    &session,
                    progress.stream_id,
                    progress.end_lsn,
                ))
                .await?;
                if actual.as_ref() != Some(progress) {
                    return Err(invalid("voter has not caught up every committed stream"));
                }
            }
            guards.push(session.valid);
        }
    }
    group.ensure_leader(term)?;
    group
        .raft
        .update_conf_fenced_async(
            conf_value,
            voters,
            members,
            Some(MembershipFence {
                term,
                membership,
                committed_index: state.frontier().index,
                guards,
            }),
        )
        .await
        .map_err(Into::into)
}
