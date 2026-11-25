//! Name-derived deterministic OIDs for cluster-consistent object identity.
//!
//! In a multi-group cluster the same DDL is applied independently on every
//! node group, while the partition RPC layer carries table/partition oids
//! that the remote owner's storage uses verbatim. Those ids must therefore
//! be identical on every group. Deriving them from the qualified object name
//! (instead of a random `gen_oid`) gives that consistency without a global
//! allocator.
//!
//! User table routing derives identities in the distributed DDL path.
//! Filesystem types and derived dead-letter queues use the same deterministic
//! identities in local and distributed creation. Filesystem catalog recovery
//! rejects noncanonical IDs instead of translating older allocated identities.

use mudu::common::id::OID;

/// FNV-1a 128-bit offset basis.
const FNV1A_128_OFFSET: u128 = 0x6c62272e07bb014262b821756295c58d;
/// FNV-1a 128-bit prime.
const FNV1A_128_PRIME: u128 = 0x0000_0000_0100_0000_0000_0000_0000_013B;

/// FNV-1a 128 over length-prefixed segments; the length prefix removes the
/// ambiguity between `("ab", "c")` and `("a", "bc")`.
fn fnv1a_128(segments: &[&str]) -> u128 {
    let mut hash = FNV1A_128_OFFSET;
    for segment in segments {
        let bytes = segment.as_bytes();
        for byte in (bytes.len() as u32).to_be_bytes() {
            hash ^= byte as u128;
            hash = hash.wrapping_mul(FNV1A_128_PRIME);
        }
        for byte in bytes {
            hash ^= *byte as u128;
            hash = hash.wrapping_mul(FNV1A_128_PRIME);
        }
    }
    // 0 is the reserved invalid oid; fold it to 1 (measure-zero event).
    if hash == 0 {
        1
    } else {
        hash
    }
}

/// Derives the oid of a user table or queue from its qualified name.
pub fn derive_table_oid(schema: &str, table: &str) -> OID {
    fnv1a_128(&["table", schema, table])
}

/// Derives a column oid from its fully qualified table and column names.
pub fn derive_column_oid(schema: &str, table: &str, column: &str) -> OID {
    fnv1a_128(&["column", schema, table, column])
}

/// Derives the oid of a partition rule from its name.
pub fn derive_partition_rule_oid(rule: &str) -> OID {
    fnv1a_128(&["partition_rule", rule])
}

/// Derives the oid of one partition of a rule from the qualified names.
pub fn derive_partition_oid(rule: &str, partition: &str) -> OID {
    fnv1a_128(&["partition", rule, partition])
}

/// Derives the worker id of `worker_index` on cluster node `node_id`.
///
/// Cluster mode derives worker ids deterministically so every node can
/// address any worker without exchanging worker maps (the id is the channel
/// endpoint id). Node-local mode keeps random worker ids.
pub fn derive_worker_oid(node_id: u64, worker_index: usize) -> OID {
    fnv1a_128(&["worker", &node_id.to_string(), &worker_index.to_string()])
}

/// Derives the node-level service endpoint id of cluster node `node_id`
/// (used by node-level services such as the control-plane raft handler).
pub fn derive_node_endpoint(node_id: u64) -> OID {
    fnv1a_128(&["node_endpoint", &node_id.to_string()])
}

/// Derives a filesystem-type id from its name (deterministic so every node
/// of the cluster allocates the same nonzero id). Catalog insertion must
/// additionally reject collisions; this hash is not a uniqueness proof.
pub fn derive_fs_id(name: &str) -> u64 {
    (fnv1a_128(&["fs_type", name]) as u64).max(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derived_oids_are_deterministic_and_distinct() {
        assert_eq!(derive_table_oid("s", "t"), derive_table_oid("s", "t"));
        assert_ne!(derive_table_oid("s", "t"), derive_table_oid("s", "u"));
        assert_ne!(derive_table_oid("s", "t"), derive_table_oid("x", "t"));
        assert_ne!(derive_table_oid("s", "t"), derive_partition_rule_oid("t"));
        assert_ne!(
            derive_partition_oid("r", "p1"),
            derive_partition_oid("r", "p2")
        );
        assert_ne!(
            derive_partition_oid("r", "p1"),
            derive_partition_rule_oid("r")
        );
        // Segment-boundary ambiguity: ("ab","c") must differ from ("a","bc").
        assert_ne!(fnv1a_128(&["ab", "c"]), fnv1a_128(&["a", "bc"]));
        for oid in [
            derive_table_oid("mududb", "t"),
            derive_partition_rule_oid("r"),
            derive_partition_oid("r", "p"),
        ] {
            assert_ne!(oid, 0);
        }
    }
}
