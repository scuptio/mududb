# mudu_raft

Raft consensus for MuduDB replication.

`mudu_raft` is a **vendored fork/port** of
[`scuptio/scupt-raft`](https://github.com/scuptio/scupt-raft) (Apache-2.0), a
TLA+-verified Raft implementation, restructured onto MuduDB's architecture and
conventions:

- the consensus core (`core/`) is a faithful, synchronous port of scupt-raft's
  `state_machine_inner.rs` / `quorum.rs` / `raft_conf.rs` state machine
  semantics (pre-vote, elections counted in ticks, joint-consensus-style
  versioned configuration, log matching, majority commit advancement);
- the core is pure: no I/O, no locks, no sleeping, no wall clock — time is
  injected as ticks, which makes it single-threaded event-loop friendly and
  deterministically testable;
- persistence (`store.rs`) uses the MuduDB log frame format v1
  (`doc/en/contract/log_frame_v1.md`, magic `LGFM`, CRC64-XZ lower 32 bits,
  multi-part frames, chunk rotation, torn-tail tolerant recovery), implemented
  locally so the crate does not depend on `mudu_kernel`;
- threading, channels and file I/O go through `mudu_sys` wrappers;
- errors use `mudu::common::result::RS` / `mudu_error!`.

## Deviation notes

Where upstream scupt-raft code conflicted with itself or with correct Raft
semantics, this port follows the intended/TLA+-modelled semantics; each
deviation is documented at the code site:

1. Pre-vote quorum counts the **pre-vote** grant map (upstream reads the
   regular vote map, leaving `follower_pre_vote_granted` dead and elections
   unable to start via pre-vote).
2. A candidate stepping down at the *same* term keeps its vote
   (`voted_for` is only reset on term change), preventing double voting
   within one term.
3. A follower's commit index is clamped to `min(leader_commit, match_index)`
   before being persisted/emitted (upstream persists the unclamped value).
4. `commit_index` is persisted and restored on recovery (upstream
   reconstructs it from the snapshot index); leaders keep their commit index
   across `become_leader` instead of resetting it.
5. Log compaction is gated behind `RaftTuning::compact_min_entries`
   (default: disabled); upstream compacts on every long tick. Compaction
   additionally happens through the forced/install paths below.
6. `conf_nid_vote` uses the vote node sets (upstream used the log node sets;
   identical when every member is a voter).
7. A non-voter (learner) never campaigns on an election timeout (upstream
   sent `PreVoteReq` without checking the node's own vote membership).
8. Snapshot bytes never ride raft messages. Upstream's `ApplyReq` snapshot
   transfer is a stub here: the kernel ships snapshot files over its own
   replication channel and calls `install_snapshot` locally. A follower
   answers an `ApplyReq` stub with a rejection-style `AppendResp` resume
   hint (`next_index = own frontier + 1`) so the leader's `AppendReq`
   flow resumes right above the follower's snapshot frontier instead of
   walking down through the compacted region.
9. The node-local `ConfValue.node_id` never travels with configuration
   updates: the core keeps an immutable `local_node_id`, leaders stamp
   their own id into `conf_new`, and followers rewrite the adopted
   value's `node_id` to their own (upstream adopts the leader's value
   wholesale — latent there only because a static conf never advances its
   version, so the adoption path never fires; a live re-configuration
   would corrupt every follower's node id).
10. Message-acceptance gates (`can_vote`/`can_log`) use the **union** of
    the committed and new configurations (upstream gated on the committed
    configuration only, which drops messages from members that exist only
    in the new configuration — stalling elections and commit advancement
    during joint consensus whenever committed-conf members are
    unavailable). Quorum *counting* still happens per configuration, and
    leaders initialize `next_index`/`match_index` for members that join
    after the election.

## Snapshot installation and log GC

Second-increment additions on top of the base port (all pure extensions of
the same core/store/event-loop pipeline):

- `RaftCore::step_install_snapshot(frontier, fx)` — installs an externally
  produced snapshot frontier: left-truncates the in-memory log, advances
  `commit_index` to at least `frontier.index` (persisted, not re-emitted as
  committed effects), and emits `WriteOp::InstallSnapshot`. Idempotent; a
  frontier beyond the last log index takes the discard path (the external
  snapshot vouches for entries the node never had — how a fresh node joins
  a compacted group); on a term conflict at the frontier the tail is
  discarded and the commit index is pinned to the frontier.
- `RaftCore::step_force_compact(fx)` — compacts the log to the current
  commit index, bypassing the `compact_min_entries` gate.
- `RaftHandle::install_snapshot(frontier) -> RS<()>` /
  `RaftHandle::force_compact() -> RS<()>` — event-loop front-ends that
  block until the operation is durable (persist precedes visibility).
- Store GC: when the snapshot frontier advances, whole chunks whose
  entries all lie at or below the frontier are physically deleted after
  the batch fsync. If the chunk holding the latest `State` record is
  itself eligible, the state is first replayed into a fresh chunk and
  fsynced, so the retained series always carries a valid state. The
  current tail chunk is never deleted (dead bytes are reclaimed when
  rotation turns it into an old chunk); deletion needs no fsync of its
  own — a crash simply leaves chunks behind for the next GC round.

Orchestration note: the kernel paces raft compaction so a follower's
installable frontier stays at or above the leader's snapshot frontier —
the business snapshot point doubles as the raft install frontier, and a
frontier beyond the local log is accepted through the discard path, so a
wiped node can always install the offered frontier and let appends resume
above it.

## Online membership change (joint consensus) and learners

The joint-consensus state machine of the port is fully wired to the
public API:

- `RaftHandle::update_conf(conf_value, nid_vote, nid_log)` begins a
  re-configuration (leader only). It returns immediately with an error
  when the node is not the leader (leader hint attached), when a previous
  re-configuration is still in progress, or when the membership is
  invalid (`nid_vote` must be a non-empty subset of `nid_log`). On
  acceptance it returns once the new configuration is durable, yielding
  an `UpdateConfWaiter`.
- `UpdateConfWaiter` resolves with the committed conf version once the
  joint consensus commits (driven by ticks/heartbeats), or fails with an
  explicit `UpdateConfError` when leadership is lost mid-reconfiguration
  or the node stops — it never hangs. A new leader keeps driving the
  joint consensus from its own state, but the waiter does not migrate;
  the caller may re-issue or re-await on the new leader.
- `ConfNodeValue::derive_add_voter / derive_add_learner /
  derive_remove_node / derive_promote_voter` build the
  `(conf_value, nid_vote, nid_log)` triple for the four common
  operations, with membership validation.
- `RaftHandle::conf()` reads the current `(committed, new)` conf pair;
  `NodeStatus` carries `conf_version_committed` / `conf_version_new` for
  watcher-based progress tracking.
- A learner (`NodeInfo { can_vote: false }`) replicates the log and never
  votes or campaigns; promote it to voter only after it has caught up.

## Layout

- `src/entry.rs` — `ReplEntry`, the replicated payload type.
- `src/core/` — pure Raft state machine (port of scupt-raft consensus).
- `src/frame.rs` — local LGFM v1 frame codec.
- `src/store.rs` — chunked persistent raft log + hard state.
- `src/node.rs` — single-threaded event loop (`RaftHandle`).
- `src/transport.rs` — `RaftTransport` abstraction + in-memory test network.

See `src/lib.rs` for the public API.
