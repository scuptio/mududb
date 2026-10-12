use super::tests::{CoreCluster, ELECTION_TIMEOUT};
use super::*;

/// Committed conf versions across the named nodes.
fn committed_versions(c: &CoreCluster, ids: &[NID]) -> Vec<ConfVersion> {
    ids.iter()
        .map(|id| {
            c.core(*id)
                .raft_conf()
                .conf_node_value_committed()
                .term_version()
                .clone()
        })
        .collect()
}

#[test]
fn reconf_add_voter_commits_and_preserves_node_ids() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.add_node(4, &[1, 2, 3, 4]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);

    let committed = c.committed_conf(1).clone();
    let (value, vote, log) = committed.derive_add_voter(4).unwrap();
    let outcome = c.reconf(1, value, vote, log);
    let version = match outcome {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    assert_eq!(version.term, c.core(1).term());

    c.drive_reconf_commit(1, &version);
    // the joint configuration committed on the leader and propagated
    assert!(!c.core(1).raft_conf().is_reconfig_ongoing());
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3, 4]);
    c.tick(1, 1);
    let versions = committed_versions(&c, &[1, 2, 3]);
    assert!(versions.iter().all(|v| *v == version));

    // deviation 9 regression: followers keep their own node id after
    // adopting the leader's configuration
    assert_eq!(c.core(2).node_id(), 2);
    assert_eq!(c.core(3).node_id(), 3);
    // the new member (which bootstrapped with the full peer list) also
    // adopted the configuration and keeps its id
    assert_eq!(c.core(4).node_id(), 4);
    assert_eq!(
        c.core(4).raft_conf().conf_node_value_new().term_version(),
        &version
    );
}

#[test]
fn reconf_rejects_second_begin_and_invalid_membership() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);

    // invalid memberships are errors (checked while no reconf runs)
    let mut fx = Effects::default();
    let base_value = c.committed_conf(1).value.clone();
    assert!(c
        .cores
        .get_mut(&1)
        .unwrap()
        .step_re_conf_begin(base_value.clone(), vec![], vec![1, 2, 3], &mut fx)
        .is_err());
    assert!(c
        .cores
        .get_mut(&1)
        .unwrap()
        .step_re_conf_begin(base_value, vec![1, 9], vec![1, 2, 3], &mut fx)
        .is_err());

    let committed = c.committed_conf(1).clone();
    let (value, vote, log) = committed.derive_add_voter(4).unwrap();
    assert!(matches!(
        c.reconf(1, value.clone(), vote.clone(), log.clone()),
        ReConfOutcome::Accepted { .. }
    ));
    // a second begin while the first is in progress is rejected
    assert_eq!(
        c.reconf(1, value, vote.clone(), log.clone()),
        ReConfOutcome::ReconfigInProgress
    );
    // non-leader is rejected with a hint
    assert_eq!(
        c.reconf(
            2,
            c.committed_conf(2).value.clone(),
            vec![1, 2, 3],
            vec![1, 2, 3]
        ),
        ReConfOutcome::NotLeader {
            leader_hint: Some(1)
        }
    );
}

#[test]
fn reconf_add_learner_replicates_without_voting() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.add_node(4, &[1, 2, 3, 4]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);

    let committed = c.committed_conf(1).clone();
    let (value, vote, log) = committed.derive_add_learner(4).unwrap();
    let version = match c.reconf(1, value, vote, log) {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    c.drive_reconf_commit(1, &version);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3]);
    assert_eq!(c.committed_conf(1).node.nid_log, vec![1, 2, 3, 4]);

    // the learner replicates new entries...
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 3);
    let committed4 = c.committed_indices(4);
    assert!(
        committed4.contains(&2),
        "learner did not replicate: {committed4:?}"
    );
    // ...but never campaigns: ticking it past the election timeout
    // emits no pre-vote and keeps it a follower
    let mut fx = Effects::default();
    for _ in 0..(ELECTION_TIMEOUT + 2) {
        c.cores.get_mut(&4).unwrap().step_tick(&mut fx).unwrap();
    }
    assert!(
        fx.messages.is_empty(),
        "learner campaigned: {:?}",
        fx.messages
    );
    assert_eq!(c.core(4).role(), RaftRole::Follower);
    assert_eq!(c.core(4).term(), 1);
}

#[test]
fn reconf_promote_learner_then_it_becomes_leader() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.add_node(4, &[1, 2, 3, 4]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);

    // join as learner, then promote
    let (value, vote, log) = c.committed_conf(1).derive_add_learner(4).unwrap();
    let v1 = match c.reconf(1, value, vote, log) {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    c.drive_reconf_commit(1, &v1);
    let (value, vote, log) = c.committed_conf(1).derive_promote_voter(4).unwrap();
    let v2 = match c.reconf(1, value, vote, log) {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    c.drive_reconf_commit(1, &v2);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3, 4]);
    // node 4 adopted the promotion (it is a voter now)
    c.tick(1, 1);
    assert_eq!(
        c.committed_conf(4).node.nid_vote,
        vec![1, 2, 3, 4],
        "node 4 did not adopt the voter conf"
    );

    // kill the old leader; the promoted learner campaigns and wins
    c.isolate(1);
    c.elect(4);
    let outcome = c.propose(4, CoreCluster::barrier(2));
    assert!(matches!(outcome, ProposeOutcome::Accepted { .. }));
    c.tick(4, 1);
    let committed3 = c.committed_indices(3);
    assert!(
        committed3.contains(&2),
        "entry did not commit under the promoted leader: {committed3:?}"
    );
}

#[test]
fn reconf_remove_node_shrinks_group() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.add_node(4, &[1, 2, 3, 4]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);

    // add node 4 as voter, then remove it again
    let (value, vote, log) = c.committed_conf(1).derive_add_voter(4).unwrap();
    let v1 = match c.reconf(1, value, vote, log) {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    c.drive_reconf_commit(1, &v1);
    let (value, vote, log) = c.committed_conf(1).derive_remove_node(4).unwrap();
    let v2 = match c.reconf(1, value, vote, log) {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    c.drive_reconf_commit(1, &v2);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3]);
    assert_eq!(c.committed_conf(1).node.nid_log, vec![1, 2, 3]);

    // the shrunk group keeps committing; the removed node hears nothing
    let before = c.committed_indices(4);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 3);
    assert!(c.committed_indices(2).contains(&2));
    assert_eq!(
        c.committed_indices(4),
        before,
        "removed node received entries"
    );
    // the removed node adopted the excluding conf during the joint
    // phase (it never learns the commit itself — a removed node is
    // cut off by design)
    assert_eq!(
        c.core(4).raft_conf().conf_node_value_new().node.nid_vote,
        vec![1, 2, 3],
        "removed node did not adopt the excluding conf"
    );
    // its messages are ignored by the group, so it cannot disrupt it
    let mut fx = Effects::default();
    for _ in 0..(ELECTION_TIMEOUT + 2) {
        c.cores.get_mut(&4).unwrap().step_tick(&mut fx).unwrap();
    }
    c.apply_fx(4, fx);
    c.route_all();
    assert_eq!(c.core(1).role(), RaftRole::Leader);
    assert_eq!(c.core(1).term(), 1);
}

#[test]
fn reconf_survives_leader_change() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.add_node(4, &[1, 2, 3, 4]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);

    let (value, vote, log) = c.committed_conf(1).derive_add_voter(4).unwrap();
    let version = match c.reconf(1, value, vote, log) {
        ReConfOutcome::Accepted { version } => version,
        other => panic!("unexpected outcome: {other:?}"),
    };
    // propagate conf_new once, then isolate the leader before it can
    // commit the joint configuration
    c.tick(1, 1);
    c.isolate(1);
    c.elect(2);
    // the new leader drives the same joint consensus to completion
    c.drive_reconf_commit(2, &version);
    assert_eq!(c.committed_conf(2).node.nid_vote, vec![1, 2, 3, 4]);
    c.tick(2, 1);
    assert_eq!(
        c.committed_conf(3).term_version(),
        &version,
        "follower did not adopt the committed conf"
    );
}

#[test]
fn derive_helpers_validate_membership() {
    let c = CoreCluster::new(&[1, 2, 3]);
    let committed = c.committed_conf(1);
    assert!(committed.derive_add_voter(1).is_err());
    assert!(committed.derive_add_learner(2).is_err());
    assert!(committed.derive_remove_node(9).is_err());
    assert!(committed.derive_promote_voter(1).is_err());
    assert!(committed.derive_promote_voter(9).is_err());
    // removing all but one voter still works; removing the last voter
    // is rejected through a chain
    let (v2, _, _) = committed.derive_remove_node(3).unwrap();
    let cnv2 = ConfNodeValue::new(v2, 0, 2, 0);
    let (v3, _, _) = cnv2.derive_remove_node(2).unwrap();
    let cnv3 = ConfNodeValue::new(v3, 0, 3, 0);
    assert!(cnv3.derive_remove_node(1).is_err());
    // add-learner then promote works out
    let (v4, vote4, log4) = committed.derive_add_learner(4).unwrap();
    assert_eq!(vote4, vec![1, 2, 3]);
    assert_eq!(log4, vec![1, 2, 3, 4]);
    let cnv4 = ConfNodeValue::new(v4, 0, 2, 0);
    let (_, vote5, log5) = cnv4.derive_promote_voter(4).unwrap();
    assert_eq!(vote5, vec![1, 2, 3, 4]);
    assert_eq!(log5, vec![1, 2, 3, 4]);
}
