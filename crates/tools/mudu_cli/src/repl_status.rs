//! Text rendering for `mcli repl status --table`.

use serde_json::Value;
use std::fmt::Write as _;

/// Renders the `GET /mudu/server/replication` JSON as a sectioned text
/// table (`mcli repl status --table`).
pub(crate) fn render_repl_status_text(status: &Value) -> String {
    let mut out = String::new();
    write_replication_section(&mut out, status);
    write_stream_section(&mut out, status);
    write_workers_section(&mut out, status);
    write_subscribers_section(&mut out, status);
    write_storage_section(&mut out, status);
    write_counters_section(&mut out, status);
    out
}

fn write_replication_section(out: &mut String, status: &Value) {
    let _ = writeln!(out, "== replication ==");
    let node = status["node_id"]
        .as_u64()
        .map_or("-".to_string(), |v| v.to_string());
    let role = status["role"].as_str().unwrap_or("-");
    let _ = writeln!(
        out,
        "mode: {}  node_id: {}  role: {}  serving: {}",
        status["mode"].as_str().unwrap_or("none"),
        node,
        role,
        status["serving"].as_bool().unwrap_or(false)
    );
    if let Some(raft) = status.get("raft").filter(|r| !r.is_null()) {
        write_raft_lines(out, raft);
    }
}

fn write_raft_lines(out: &mut String, raft: &Value) {
    let leader = raft["leader_node_id"]
        .as_u64()
        .map_or("-".to_string(), |v| v.to_string());
    let _ = writeln!(
        out,
        "raft: term={}  leader={}  commit_index={}  voters={:?}  learners={:?}",
        raft["term"].as_u64().unwrap_or(0),
        leader,
        raft["commit_index"].as_u64().unwrap_or(0),
        raft["voters"],
        raft["learners"],
    );
    if let Some(vec) = raft["commit_vec"].as_array() {
        for entry in vec {
            let _ = writeln!(
                out,
                "  commit_vec worker={} lsn={} seg_hash={}",
                entry["worker_index"].as_u64().unwrap_or(0),
                entry["lsn"].as_u64().unwrap_or(0),
                entry["seg_hash"].as_str().unwrap_or("-"),
            );
        }
    }
}

fn write_stream_section(out: &mut String, status: &Value) {
    if let Some(stream) = status.get("stream").filter(|s| !s.is_null()) {
        let primary = stream["primary_node_id"]
            .as_u64()
            .map_or("-".to_string(), |v| v.to_string());
        let _ = writeln!(
            out,
            "stream: epoch={}  primary={}  is_primary={}  sync={}  sync_timeout_ms={}",
            stream["epoch"].as_u64().unwrap_or(0),
            primary,
            stream["is_primary"].as_bool().unwrap_or(false),
            stream["sync"].as_bool().unwrap_or(false),
            stream["sync_timeout_ms"].as_u64().unwrap_or(0),
        );
    }
}

fn write_workers_section(out: &mut String, status: &Value) {
    let _ = writeln!(out, "== workers ==");
    let _ = writeln!(
        out,
        "{:<7} {:<12} {:<10} {:<10} {:<10} {:<10}",
        "worker", "wal_next", "chain_lsn", "staging", "snap_lsn", "snap_bytes"
    );
    if let Some(workers) = status["workers"].as_array() {
        for worker in workers {
            let opt = |v: &Value| v.as_u64().map_or("-".to_string(), |x| x.to_string());
            let _ = writeln!(
                out,
                "{:<7} {:<12} {:<10} {:<10} {:<10} {:<10}",
                worker["worker_index"].as_u64().unwrap_or(0),
                worker["wal_next_lsn"].as_u64().unwrap_or(0),
                opt(&worker["chain_lsn"]),
                opt(&worker["staging_max_lsn"]),
                opt(&worker["snapshot"]["committed_lsn"]),
                opt(&worker["snapshot"]["dir_bytes"]),
            );
        }
    }
}

fn write_subscribers_section(out: &mut String, status: &Value) {
    let _ = writeln!(out, "== subscribers ==");
    let _ = writeln!(
        out,
        "{:<6} {:<7} {:<12} {:<12} {:<8}",
        "node", "worker", "stored_lsn", "source_lsn", "lag_lsn"
    );
    if let Some(subscribers) = status["subscribers"].as_array() {
        if subscribers.is_empty() {
            let _ = writeln!(out, "(none connected)");
        }
        for subscriber in subscribers {
            let _ = writeln!(
                out,
                "{:<6} {:<7} {:<12} {:<12} {:<8}",
                subscriber["node_id"].as_u64().unwrap_or(0),
                subscriber["worker_index"].as_u64().unwrap_or(0),
                subscriber["stored_lsn"].as_u64().unwrap_or(0),
                subscriber["source_lsn"].as_u64().unwrap_or(0),
                subscriber["lag_lsn"].as_u64().unwrap_or(0),
            );
        }
    }
}

fn write_storage_section(out: &mut String, status: &Value) {
    let storage = &status["storage"];
    let _ = writeln!(out, "== storage (bytes) ==");
    let _ = writeln!(
        out,
        "wal: {}  staging: {}  raft_log: {}  snapshot: {}  refreshed_at_ms: {}",
        storage["wal_bytes"].as_u64().unwrap_or(0),
        storage["staging_bytes"].as_u64().unwrap_or(0),
        storage["raft_log_bytes"].as_u64().unwrap_or(0),
        storage["snapshot_bytes"].as_u64().unwrap_or(0),
        storage["refreshed_at_ms"].as_u64().unwrap_or(0),
    );
}

fn write_counters_section(out: &mut String, status: &Value) {
    let counters = &status["counters"];
    let _ = writeln!(out, "== counters ==");
    let _ = writeln!(
        out,
        "sync_write_timeouts: {}  snapshots_taken: {}  wal_chunks_reclaimed: {}  staging_chunks_reclaimed: {}  snapshot_offers: {}  snapshot_restores: {}",
        counters["sync_write_timeouts"].as_u64().unwrap_or(0),
        counters["snapshots_taken"].as_u64().unwrap_or(0),
        counters["wal_chunks_reclaimed"].as_u64().unwrap_or(0),
        counters["staging_chunks_reclaimed"].as_u64().unwrap_or(0),
        counters["snapshot_offers_served"].as_u64().unwrap_or(0),
        counters["snapshot_restores_completed"]
            .as_u64()
            .unwrap_or(0),
    );
}
