//! Wire conversions between the log-stream mechanism's round/ack identities
//! and the `catalog::contract::control_plane` wire types (the mechanism sits
//! in `repl`; the wire types are pure contracts in `catalog`).

use catalog::contract::control_plane::{StreamAckWire, StreamRoundWire};
use mudu_raft::stream::{GroupId, GroupIdobject, MembershipVersion, StreamId, StreamProgress};
use mudu_raft::ConfVersion;

use super::{AckRound, DurableAck};

/// Converts a mechanism round identity into its wire form.
pub fn round_to_wire(round: &AckRound) -> StreamRoundWire {
    StreamRoundWire {
        cluster_id: round.identity.cluster_id,
        group_id: round.identity.group_id.0,
        group_generation: round.identity.generation,
        registry_version: round.registry_version,
        leader_term: round.leader_term,
        committed_conf: conf_to_wire(&round.membership_version.committed),
        incoming_conf: conf_to_wire(&round.membership_version.incoming),
        codec_version: round.codec_version,
        digest_version: round.digest_version,
        stream_id: round.progress.stream_id.0,
        stream_generation: round.progress.stream_generation,
        end_lsn: round.progress.end_lsn,
        segment_digest: round.progress.segment_digest,
    }
}

/// Converts a wire round identity back into the mechanism form.
pub fn round_from_wire(wire: &StreamRoundWire) -> AckRound {
    AckRound {
        identity: GroupIdobject {
            cluster_id: wire.cluster_id,
            group_id: GroupId(wire.group_id),
            generation: wire.group_generation,
        },
        registry_version: wire.registry_version,
        leader_term: wire.leader_term,
        membership_version: MembershipVersion {
            committed: conf_from_wire(wire.committed_conf),
            incoming: conf_from_wire(wire.incoming_conf),
        },
        codec_version: wire.codec_version,
        digest_version: wire.digest_version,
        progress: StreamProgress {
            stream_id: StreamId(wire.stream_id),
            stream_generation: wire.stream_generation,
            end_lsn: wire.end_lsn,
            segment_digest: wire.segment_digest,
        },
    }
}

/// Converts a mechanism durable ack into its wire form.
pub fn ack_to_wire(ack: &DurableAck) -> StreamAckWire {
    StreamAckWire {
        node_id: ack.node_id,
        session_id: ack.session_id,
        round: round_to_wire(&ack.round),
    }
}

/// Converts a wire durable ack back into the mechanism form.
pub fn ack_from_wire(wire: &StreamAckWire) -> DurableAck {
    DurableAck {
        node_id: wire.node_id,
        session_id: wire.session_id,
        round: round_from_wire(&wire.round),
    }
}

fn conf_to_wire(conf: &ConfVersion) -> (u64, u64, u64) {
    (conf.term, conf.version, conf.index)
}

fn conf_from_wire(wire: (u64, u64, u64)) -> ConfVersion {
    ConfVersion {
        term: wire.0,
        version: wire.1,
        index: wire.2,
    }
}
