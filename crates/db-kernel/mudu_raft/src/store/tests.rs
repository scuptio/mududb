use super::*;
use crate::core::conf::NodeInfo;
use crate::entry::ReplEntry;
use mudu_sys::tokio;

fn test_conf_value(node_id: u64) -> ConfValue {
    ConfValue::new(
        node_id,
        10,
        vec![NodeInfo {
            node_id,
            can_vote: true,
        }],
    )
}

fn test_entry(term: u64, index: u64) -> LogEntry {
    LogEntry {
        term,
        index,
        value: ReplEntry::Barrier { term },
    }
}

fn state_ops() -> Vec<WriteOp> {
    vec![
        WriteOp::UpConfCommitted {
            value: test_conf_value(1),
            version: ConfVersion {
                term: 0,
                version: 1,
                index: 0,
            },
        },
        WriteOp::UpConfNew {
            value: test_conf_value(1),
            version: ConfVersion {
                term: 0,
                version: 1,
                index: 0,
            },
        },
    ]
}

fn small_cfg() -> StoreCfg {
    StoreCfg {
        chunk_size: 4096,
        max_part_size: 1024,
        fsync: true,
    }
}

fn open_temp() -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let dir = raft_log_dir(tmp.path());
    (tmp, dir)
}

#[tokio::test]
async fn empty_store_recovers_none() {
    let (_tmp, dir) = open_temp();
    let (store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    assert!(recovered.is_none());
    drop(store);
}

#[tokio::test]
async fn state_and_entries_roundtrip() {
    let (_tmp, dir) = open_temp();
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    store
        .apply_batch_async(&[
            WriteOp::UpTermVotedFor {
                term: 3,
                voted_for: Some(1),
            },
            WriteOp::WriteLog {
                prev_index: 0,
                entries: vec![test_entry(3, 1), test_entry(3, 2)],
                opt: crate::core::message::WriteOpt::default(),
            },
            WriteOp::UpCommitIndex(2),
        ])
        .await
        .unwrap();
    drop(store);

    let (_store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(recovered.term, 3);
    assert_eq!(recovered.voted_for, Some(1));
    assert_eq!(recovered.commit_index, 2);
    assert_eq!(recovered.log.len(), 2);
    assert_eq!(recovered.log[0], test_entry(3, 1));
    assert_eq!(recovered.log[1], test_entry(3, 2));
    assert_eq!(recovered.snapshot_index_term.index, 0);
    assert_eq!(recovered.conf_committed.node.nid_vote, vec![1]);
}

#[tokio::test]
async fn truncate_right_marker_drops_tail() {
    let (_tmp, dir) = open_temp();
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    store
        .apply_batch_async(&[WriteOp::WriteLog {
            prev_index: 0,
            entries: vec![
                test_entry(1, 1),
                test_entry(1, 2),
                test_entry(1, 3),
                test_entry(1, 4),
            ],
            opt: crate::core::message::WriteOpt::default(),
        }])
        .await
        .unwrap();
    // conflict at index 3: truncate and rewrite
    store
        .apply_batch_async(&[WriteOp::WriteLog {
            prev_index: 2,
            entries: vec![test_entry(2, 3)],
            opt: crate::core::message::WriteOpt::default(),
        }])
        .await
        .unwrap();
    drop(store);

    let (_store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    let recovered = recovered.unwrap();
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, vec![1, 2, 3]);
    assert_eq!(recovered.log[2].term, 2);
}

#[tokio::test]
async fn chunk_rotation_keeps_all_records() {
    let (_tmp, dir) = open_temp();
    let cfg = StoreCfg {
        chunk_size: 256,
        max_part_size: 128,
        fsync: true,
    };
    let mut store = RaftStore::open_async(&dir, cfg).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    for i in 1..=20u64 {
        store
            .apply_batch_async(&[WriteOp::WriteLog {
                prev_index: i - 1,
                entries: vec![test_entry(1, i)],
                opt: crate::core::message::WriteOpt::default(),
            }])
            .await
            .unwrap();
    }
    drop(store);

    let chunks = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    assert!(chunks.len() > 1, "expected rotation, got {:?}", chunks);
    let (_store, recovered) = RaftStore::open_async(&dir, cfg).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(recovered.log.len(), 20);
    for (i, e) in recovered.log.iter().enumerate() {
        assert_eq!(e.index, i as u64 + 1);
    }
}

#[tokio::test]
async fn torn_tail_is_repaired() {
    let (_tmp, dir) = open_temp();
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    store
        .apply_batch_async(&[WriteOp::WriteLog {
            prev_index: 0,
            entries: vec![test_entry(1, 1), test_entry(1, 2)],
            opt: crate::core::message::WriteOpt::default(),
        }])
        .await
        .unwrap();
    drop(store);

    // corrupt the tail of the only chunk: append garbage
    let chunks = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    assert_eq!(chunks.len(), 1);
    let path = &chunks[0].1;
    let mut bytes = fs_sync::read(path).unwrap();
    let orig_len = bytes.len();
    bytes.extend_from_slice(&[0xde, 0xad, 0xbe, 0xef]);
    bytes[orig_len - 3] ^= 0xff; // corrupt the last frame's tailer
    fs_sync::write(path, &bytes).unwrap();

    let (_store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    let recovered = recovered.unwrap();
    // the corrupt frame (entry 2) is dropped, entry 1 survives;
    // the state record (written in the first batch) survives
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, vec![1]);
}

#[tokio::test]
async fn compact_frontier_is_persisted() {
    let (_tmp, dir) = open_temp();
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    store
        .apply_batch_async(&[WriteOp::WriteLog {
            prev_index: 0,
            entries: vec![test_entry(1, 1), test_entry(1, 2), test_entry(1, 3)],
            opt: crate::core::message::WriteOpt::default(),
        }])
        .await
        .unwrap();
    store
        .apply_batch_async(&[
            WriteOp::UpCommitIndex(2),
            WriteOp::CompactLog(vec![test_entry(1, 1), test_entry(1, 2)]),
        ])
        .await
        .unwrap();
    drop(store);

    let (_store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(recovered.snapshot_index_term.index, 2);
    assert_eq!(recovered.snapshot_index_term.term, 1);
    // compacted entries are not read back
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, vec![3]);
}

#[tokio::test]
async fn append_after_recovery_continues_the_same_log() {
    let (_tmp, dir) = open_temp();
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    store
        .apply_batch_async(&[WriteOp::WriteLog {
            prev_index: 0,
            entries: vec![test_entry(1, 1), test_entry(1, 2)],
            opt: crate::core::message::WriteOpt::default(),
        }])
        .await
        .unwrap();
    drop(store);

    // reopen and keep appending where the recovered tail left off
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store
        .apply_batch_async(&[
            WriteOp::WriteLog {
                prev_index: 2,
                entries: vec![test_entry(2, 3)],
                opt: crate::core::message::WriteOpt::default(),
            },
            WriteOp::UpTermVotedFor {
                term: 2,
                voted_for: None,
            },
        ])
        .await
        .unwrap();
    drop(store);

    let (_store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    let recovered = recovered.unwrap();
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, vec![1, 2, 3]);
    assert_eq!(recovered.term, 2);
    assert_eq!(recovered.log[2].term, 2);
}

#[tokio::test]
async fn multi_part_oversize_entry_roundtrip() {
    let (_tmp, dir) = open_temp();
    let cfg = StoreCfg {
        chunk_size: 1 << 20,
        max_part_size: 256,
        fsync: true,
    };
    let mut store = RaftStore::open_async(&dir, cfg).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    let big = ReplEntry::CommitVec {
        term: 1,
        per_worker: (0..100u32)
            .map(|w| (w, w as u64 * 7, [w as u8; 32]))
            .collect(),
    };
    let entry = LogEntry {
        term: 1,
        index: 1,
        value: big.clone(),
    };
    store
        .apply_batch_async(&[WriteOp::WriteLog {
            prev_index: 0,
            entries: vec![entry],
            opt: crate::core::message::WriteOpt::default(),
        }])
        .await
        .unwrap();
    drop(store);

    let (_store, recovered) = RaftStore::open_async(&dir, cfg).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(recovered.log.len(), 1);
    assert_eq!(recovered.log[0].value, big);
}

async fn write_entries(store: &mut RaftStore, from: u64, to: u64) {
    for i in from..=to {
        store
            .apply_batch_async(&[WriteOp::WriteLog {
                prev_index: i - 1,
                entries: vec![test_entry(1, i)],
                opt: crate::core::message::WriteOpt::default(),
            }])
            .await
            .unwrap();
    }
}

fn gc_cfg() -> StoreCfg {
    StoreCfg {
        chunk_size: 512,
        max_part_size: 256,
        fsync: true,
    }
}

#[tokio::test]
async fn install_snapshot_op_persists_frontier() {
    let (_tmp, dir) = open_temp();
    let mut store = RaftStore::open_async(&dir, small_cfg()).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    write_entries(&mut store, 1, 10).await;
    store
        .apply_batch_async(&[WriteOp::InstallSnapshot(TermIndex { term: 1, index: 7 })])
        .await
        .unwrap();
    assert_eq!(store.snapshot_frontier(), (7, 1));
    drop(store);

    let (_store, recovered) = RaftStore::open_async(&dir, small_cfg()).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(
        recovered.snapshot_index_term,
        TermIndex { term: 1, index: 7 }
    );
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, vec![8, 9, 10]);
}

#[tokio::test]
async fn gc_deletes_whole_chunks_below_frontier() {
    let (_tmp, dir) = open_temp();
    let cfg = gc_cfg();
    let mut store = RaftStore::open_async(&dir, cfg).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    write_entries(&mut store, 1, 30).await;
    let before = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    assert!(before.len() >= 4, "expected several chunks: {:?}", before);

    // compact entries 1..=20: chunks fully below the frontier go away
    let compacted: Vec<LogEntry> = (1..=20).map(|i| test_entry(1, i)).collect();
    store
        .apply_batch_async(&[WriteOp::CompactLog(compacted)])
        .await
        .unwrap();
    let after = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    assert!(
        after.len() < before.len(),
        "expected GC to delete chunks: before {:?}, after {:?}",
        before,
        after
    );
    // at least the tail (and the state chunk) survive
    assert!(!after.is_empty());
    assert_eq!(store.snapshot_frontier(), (20, 1));
    drop(store);

    // recovery starts from the first retained chunk: the log must be
    // contiguous right above the frontier
    let (_store, recovered) = RaftStore::open_async(&dir, cfg).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(
        recovered.snapshot_index_term,
        TermIndex { term: 1, index: 20 }
    );
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, (21..=30).collect::<Vec<u64>>());

    // appends continue seamlessly after the GC'd series
    let mut store = RaftStore::open_async(&dir, cfg).await.unwrap().0;
    write_entries(&mut store, 31, 35).await;
    drop(store);
    let (_store, recovered) = RaftStore::open_async(&dir, cfg).await.unwrap();
    let recovered = recovered.unwrap();
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, (21..=35).collect::<Vec<u64>>());
}

#[tokio::test]
async fn gc_replays_state_when_state_chunk_is_compactable() {
    let (_tmp, dir) = open_temp();
    let cfg = gc_cfg();
    let mut store = RaftStore::open_async(&dir, cfg).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    write_entries(&mut store, 1, 30).await;

    // Defensive-branch setup (unreachable through apply_batch, where
    // the latest State always lands in the tail): pretend the frontier
    // advanced to 20 while the latest State record still sits in an
    // early chunk that is now fully compactable.
    store.state.snapshot_index = 20;
    store.state.snapshot_term = 1;
    let old_state_chunk = store.state_chunk_seq;
    assert_ne!(old_state_chunk, store.tail_seq);
    assert!(store.chunk_max.get(&old_state_chunk).copied().unwrap_or(0) <= 20);

    store.gc_compact_chunks().await.unwrap();

    // the latest state was replayed into a fresh chunk which survives
    assert_ne!(store.state_chunk_seq, old_state_chunk);
    assert_eq!(store.state_chunk_seq, store.tail_seq);
    let remaining = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    let state_path = chunk_path(&dir, store.state_chunk_seq);
    assert!(remaining.iter().any(|(_, p)| *p == state_path));
    drop(store);

    // reopen: the replayed state recovers everything
    let (_store, recovered) = RaftStore::open_async(&dir, cfg).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(
        recovered.snapshot_index_term,
        TermIndex { term: 1, index: 20 }
    );
    assert_eq!(recovered.term, 0);
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, (21..=30).collect::<Vec<u64>>());
    assert_eq!(recovered.conf_committed.node.nid_vote, vec![1]);
}

#[tokio::test]
async fn gc_keeps_tail_and_mixed_chunks() {
    let (_tmp, dir) = open_temp();
    let cfg = gc_cfg();
    let mut store = RaftStore::open_async(&dir, cfg).await.unwrap().0;
    store.apply_batch_async(&state_ops()).await.unwrap();
    write_entries(&mut store, 1, 6).await;
    let before = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    // compact only the first entry: with 512-byte chunks, most chunks
    // hold entries above the frontier and must be retained
    store
        .apply_batch_async(&[WriteOp::CompactLog(vec![test_entry(1, 1)])])
        .await
        .unwrap();
    let after = list_chunks(mudu_sys::fs::tokio_fs().as_ref(), &dir)
        .await
        .unwrap();
    assert_eq!(before.len(), after.len(), "mixed chunks must survive");
    drop(store);
    let (_store, recovered) = RaftStore::open_async(&dir, cfg).await.unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(
        recovered.snapshot_index_term,
        TermIndex { term: 1, index: 1 }
    );
    let indices: Vec<u64> = recovered.log.iter().map(|e| e.index).collect();
    assert_eq!(indices, vec![2, 3, 4, 5, 6]);
}
