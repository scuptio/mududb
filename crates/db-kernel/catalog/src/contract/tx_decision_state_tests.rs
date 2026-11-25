//! Transaction lifecycle rejection and retry invariants.
#![allow(clippy::unwrap_used)]
use super::tx_decision::{
    CoordinatorRef, TxDecision, TxDecisionError as E, TxDecisionStruct as R, TxParticipant,
};
use super::tx_decision_state::transition;

fn owner() -> CoordinatorRef {
    CoordinatorRef {
        cluster_id: 1,
        group_id: 2,
        group_generation: 1,
        tx_id: 3,
    }
}
fn participant() -> TxParticipant {
    TxParticipant {
        group_id: 4,
        group_generation: 1,
        worker_index: 0,
        partition_id: 5,
    }
}
fn begin(participants: Vec<TxParticipant>, snapshot_epoch: u64) -> R {
    R::Begin {
        owner_term: 1,
        coordinator: owner(),
        participants,
        snapshot_epoch,
    }
}

#[test]
fn begin_is_immutable_and_requires_a_set() {
    assert_eq!(
        transition(None, &begin(vec![], 5)),
        Err(E::InvalidParticipants)
    );
    assert_eq!(
        transition(None, &begin(vec![participant(), participant()], 5)),
        Err(E::InvalidParticipants)
    );
    let record = begin(vec![participant()], 5);
    let state = transition(None, &record).unwrap();
    assert_eq!(transition(Some(&state), &record), Ok(state.clone()));
    assert_eq!(
        transition(Some(&state), &begin(vec![participant()], 6)),
        Err(E::ConflictingBegin)
    );
    let other = TxParticipant {
        partition_id: 6,
        ..participant()
    };
    assert_eq!(
        transition(Some(&state), &begin(vec![other], 5)),
        Err(E::ConflictingBegin)
    );
}

#[test]
fn rejection_never_changes_state_and_decision_never_changes() {
    let state = transition(None, &begin(vec![participant()], 5)).unwrap();
    let commit = |epoch| R::Decide {
        coordinator: owner(),
        decision: TxDecision::Committed {
            commit_epoch: epoch,
        },
    };
    let done = R::ParticipantDone {
        coordinator: owner(),
        participant: participant(),
    };
    let complete = R::Complete {
        coordinator: owner(),
    };
    assert_eq!(transition(None, &commit(7)), Err(E::MissingBegin));
    assert_eq!(
        transition(Some(&state), &commit(4)),
        Err(E::InvalidCommitEpoch)
    );
    assert_eq!(transition(Some(&state), &done), Err(E::MissingDecision));
    assert_eq!(transition(Some(&state), &complete), Err(E::MissingDecision));
    let foreign = R::Decide {
        coordinator: CoordinatorRef {
            group_generation: 2,
            ..owner()
        },
        decision: TxDecision::Aborted,
    };
    assert_eq!(
        transition(Some(&state), &foreign),
        Err(E::ForeignCoordinator)
    );
    assert!(state.decision.is_none());
    let decided = transition(Some(&state), &commit(7)).unwrap();
    assert_eq!(
        transition(Some(&decided), &commit(8)),
        Err(E::ConflictingDecision)
    );
    assert_eq!(
        transition(
            Some(&decided),
            &R::Decide {
                coordinator: owner(),
                decision: TxDecision::Aborted
            }
        ),
        Err(E::ConflictingDecision)
    );
    assert_eq!(transition(Some(&decided), &commit(7)), Ok(decided.clone()));
    assert_eq!(
        transition(Some(&decided), &complete),
        Err(E::ParticipantsPending)
    );
    let stranger = R::ParticipantDone {
        coordinator: owner(),
        participant: TxParticipant {
            group_generation: 2,
            ..participant()
        },
    };
    assert_eq!(
        transition(Some(&decided), &stranger),
        Err(E::UnknownParticipant)
    );
    let finished = transition(Some(&decided), &done).unwrap();
    assert_eq!(transition(Some(&finished), &done), Ok(finished.clone()));
    let completed = transition(Some(&finished), &complete).unwrap();
    assert!(completed.complete);
    for retry in [begin(vec![participant()], 5), commit(7), done, complete] {
        assert_eq!(transition(Some(&completed), &retry), Ok(completed.clone()));
    }
}
