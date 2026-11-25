//! Startup-only redo of the exact durable installation plan.
use super::*;
use plan::invalid;
use wal::worker_log::WorkerLogLayout;
use worker::worker_registry::WorkerIdobject;

/// Completes interrupted stream installations before any worker or background
/// task opens storage. The caller must exclusively own both directories.
/// Validates every pending intent before modifying any worker. Missing sources,
/// changed ownership, corrupt intents, and advanced GC fail closed.
pub async fn recover_pending_installs(
    data_dir: &str,
    log_dir: &str,
    chunk_size: u64,
    workers: &[WorkerIdobject],
) -> RS<()> {
    let root = Path::new(data_dir).join(SNAPSHOT_DIR_NAME);
    if !mudu_sys::fs::sync::sync_path_exists(&root) {
        return Ok(());
    }
    let mut pending = Vec::new();
    for dir in mudu_sys::fs::sync::read_dir(root)? {
        if !dir
            .file_name()
            .and_then(|n| n.to_str())
            .is_some_and(|n| n.starts_with("worker_"))
        {
            continue;
        }
        let path = dir.join(PENDING);
        if !mudu_sys::fs::sync::sync_path_exists(&path) {
            continue;
        }
        let bytes = mudu_sys::fs::sync::read(&path)?;
        let intent = decode_intent(&bytes)?;
        let worker = workers
            .iter()
            .find(|worker| worker.worker_index == intent.manifest.worker_index as usize)
            .ok_or_else(|| invalid("pending install belongs to a removed worker"))?;
        if worker.worker_id != intent.worker_id
            || dir != snapshot_worker_dir(data_dir, intent.manifest.worker_index)
        {
            return Err(invalid("pending install worker identity mismatch"));
        }
        let mut partitions = worker
            .partition_ids
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();
        if worker.worker_index == 0 {
            partitions.insert(0);
        }
        if partitions != intent.plan.partitions {
            return Err(invalid("pending install partition ownership changed"));
        }
        super::super::base::validate_candidate(data_dir, intent.worker_id, &intent.manifest)?;
        intent.plan.validate_live(data_dir, &intent.manifest)?;
        let layout = WorkerLogLayout::new(log_dir, worker.worker_id, chunk_size)?;
        if layout
            .load_gc_frontier()?
            .is_some_and(|gc| gc.lsn > intent.manifest.resume_lsn())
        {
            return Err(invalid("WAL GC passed pending snapshot"));
        }
        pending.push((
            InstallJournal {
                path,
                bytes,
                data_dir: data_dir.into(),
                worker_id: intent.worker_id,
                manifest: intent.manifest,
                plan: intent.plan,
            },
            layout,
        ));
    }
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    for (journal, layout) in pending {
        journal.install_files().await?;
        layout
            .truncate_chunks_from_lsn(fs.as_ref(), journal.manifest.resume_lsn())
            .await?;
        if fs.path_exists(Path::new(log_dir)).await? {
            fs.sync_dir(Path::new(log_dir)).await?;
        }
        journal.complete().await?;
    }
    ensure_no_pending_installs(data_dir)
}
