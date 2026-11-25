use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::oid::gen_oid;
use std::collections::{HashMap, HashSet};

use std::path::{Path, PathBuf};
use std::sync::Arc;

const WORKER_MARKER_SUFFIX: &str = ".wid";
const PARTITION_MARKER_SUFFIX: &str = ".pid";

/// Persistent identity of one worker: its index, generated id, and the
/// partitions it owns.
///
/// Workers are provisioned according to execution resources such as CPU cores,
/// whereas partitions are defined by user-level data partitioning.
/// Partitions are not automatically required to map one-to-one to workers.
/// A worker may own multiple partitions in the future.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerIdobject {
    /// Zero-based worker index within the server.
    pub worker_index: usize,
    /// Generated, non-zero worker id.
    pub worker_id: OID,
    /// Ids of the partitions owned by this worker.
    pub partition_ids: Vec<OID>,
}

/// The worker topology of one server: every worker's identity plus lookup
/// indexes by worker id and partition id. The partition indexes are mutable:
/// a partition move
/// adopts/releases partitions at runtime and persists the markers.
#[derive(Debug, Clone)]
pub struct WorkerRegistry {
    workers: Vec<WorkerIdobject>,
    worker_id_to_index: HashMap<OID, usize>,
    partition_id_to_worker_index: Arc<mudu_sys::sync::SMutex<HashMap<OID, usize>>>,
    partition_id_to_worker_id: Arc<mudu_sys::sync::SMutex<HashMap<OID, OID>>>,
    /// Directory of the worker/partition marker files; `None` for registries
    /// built in memory only (tests), which reject runtime adoption.
    marker_dir: Option<PathBuf>,
}

/// Load the worker registry persisted as marker files in `log_dir`, creating
/// and persisting identities for any worker index in `0..worker_count` that
/// has none. Fails when `worker_count` is zero or the persisted registry
/// does not match `worker_count` exactly (duplicates, zero ids, or missing
/// partitions are also rejected).
pub fn load_or_create_worker_registry<P: AsRef<Path>>(
    log_dir: P,
    worker_count: usize,
) -> RS<Arc<WorkerRegistry>> {
    load_or_create_worker_registry_inner(log_dir, worker_count, None)
}

/// Cluster-mode form of [`load_or_create_worker_registry`]: worker ids are
/// derived deterministically from `(node_id, worker_index)` (see
/// `catalog::contract::derive_oid::derive_worker_oid`) so every node can
/// address any worker without exchanging worker maps. A persisted registry
/// whose ids do not match the derived ids is rejected with a clear error
/// (the data directory was initialized in non-cluster mode or under a
/// different cluster node id).
pub fn load_or_create_worker_registry_cluster<P: AsRef<Path>>(
    log_dir: P,
    worker_count: usize,
    node_id: u64,
) -> RS<Arc<WorkerRegistry>> {
    load_or_create_worker_registry_inner(
        log_dir,
        worker_count,
        Some(Box::new(move |worker_index| {
            catalog::contract::derive_oid::derive_worker_oid(node_id, worker_index)
        })),
    )
}

fn load_or_create_worker_registry_inner<P: AsRef<Path>>(
    log_dir: P,
    worker_count: usize,
    worker_id_source: Option<Box<dyn Fn(usize) -> OID>>,
) -> RS<Arc<WorkerRegistry>> {
    if worker_count == 0 {
        // A dedicated control-plane node runs no business workers (its worker
        // count is derived from the role); every other node is rejected by the
        // serve entry's worker-count guard before serving.
        return Ok(Arc::new(WorkerRegistry::new(Vec::new())?));
    }
    let log_dir = log_dir.as_ref().to_path_buf();
    mudu_sys::fs::sync::create_dir_all(&log_dir).map_err(|e| {
        mudu_error!(
            ErrorCode::Io,
            "create worker registry log directory error",
            e
        )
    })?;

    let mut loaded = scan_worker_identities(&log_dir)?;
    for worker_index in 0..worker_count {
        if !loaded
            .iter()
            .any(|worker| worker.worker_index == worker_index)
        {
            let worker_id = match &worker_id_source {
                Some(source) => source(worker_index),
                None => non_zero_oid(),
            };
            loaded.push(create_worker_identity(&log_dir, worker_index, worker_id)?);
        }
    }
    loaded.sort_by_key(|worker| worker.worker_index);
    if let Some(source) = &worker_id_source {
        for worker in &loaded {
            let expected = source(worker.worker_index);
            if worker.worker_id != expected {
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "persisted worker id of worker index {} does not match the cluster-derived id; \
                         the data directory was initialized in non-cluster mode or under a different cluster node id",
                        worker.worker_index
                    )
                ));
            }
        }
    }
    validate_worker_identities(&loaded, worker_count)?;
    Ok(Arc::new(
        WorkerRegistry::new(loaded)?.with_marker_dir(log_dir),
    ))
}

impl WorkerRegistry {
    /// Persist a restored topology in a directory without worker markers.
    /// Existing marker files are rejected rather than overwritten.
    pub fn persist_new(&self, log_dir: &Path) -> RS<()> {
        validate_worker_identities(&self.workers, self.workers.len())?;
        if !scan_worker_identities(log_dir)?.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "worker registry already exists"
            ));
        }
        for worker in &self.workers {
            touch_marker(log_dir.join(worker_marker_name(worker.worker_index, worker.worker_id)))?;
            for partition in &worker.partition_ids {
                touch_marker(log_dir.join(partition_marker_name(worker.worker_id, *partition)))?;
            }
        }
        Ok(())
    }

    /// Build a registry from worker identities, rejecting duplicate worker
    /// ids and duplicate partition ids.
    pub fn new(workers: Vec<WorkerIdobject>) -> RS<Self> {
        let mut worker_id_to_index = HashMap::new();
        let mut partition_id_to_worker_index = HashMap::new();
        let mut partition_id_to_worker_id = HashMap::new();
        for worker in &workers {
            if worker_id_to_index
                .insert(worker.worker_id, worker.worker_index)
                .is_some()
            {
                return Err(mudu_error!(
                    ErrorCode::ObjectAlreadyExists,
                    format!("duplicate worker id {}", worker.worker_id)
                ));
            }
            for &partition_id in &worker.partition_ids {
                if partition_id_to_worker_index
                    .insert(partition_id, worker.worker_index)
                    .is_some()
                {
                    return Err(mudu_error!(
                        ErrorCode::ObjectAlreadyExists,
                        format!("duplicate partition id {}", partition_id)
                    ));
                }
                partition_id_to_worker_id.insert(partition_id, worker.worker_id);
            }
        }
        Ok(Self {
            workers,
            worker_id_to_index,
            partition_id_to_worker_index: Arc::new(mudu_sys::sync::SMutex::new(
                partition_id_to_worker_index,
            )),
            partition_id_to_worker_id: Arc::new(mudu_sys::sync::SMutex::new(
                partition_id_to_worker_id,
            )),
            marker_dir: None,
        })
    }

    /// Sets the marker-file directory (runtime adoption persists markers).
    pub fn with_marker_dir(mut self, dir: PathBuf) -> Self {
        self.marker_dir = Some(dir);
        self
    }

    /// All worker identities of the server.
    pub fn workers(&self) -> &[WorkerIdobject] {
        &self.workers
    }

    /// The identity of the worker with `worker_index`, if any.
    pub fn worker(&self, worker_index: usize) -> Option<&WorkerIdobject> {
        self.workers
            .iter()
            .find(|worker| worker.worker_index == worker_index)
    }

    /// The index of the worker with `worker_id`, if any.
    pub fn worker_index_by_worker_id(&self, worker_id: OID) -> Option<usize> {
        self.worker_id_to_index.get(&worker_id).copied()
    }

    /// The id of the worker owning `partition_id`, if any.
    pub fn worker_id_by_partition_id(&self, partition_id: OID) -> Option<OID> {
        self.partition_id_to_worker_id
            .lock()
            .ok()?
            .get(&partition_id)
            .copied()
    }

    /// The index of the worker owning `partition_id`, if any.
    pub fn worker_index_by_partition_id(&self, partition_id: OID) -> Option<usize> {
        self.partition_id_to_worker_index
            .lock()
            .ok()?
            .get(&partition_id)
            .copied()
    }

    /// Runtime adoption (phase 3): `worker_id` gains `partition_id` — the
    /// lookup indexes update and the partition marker file persists the
    /// change. Idempotent; fails when the partition is owned by another
    /// worker (release it first) or the registry has no marker directory.
    pub fn adopt_partition(&self, worker_id: OID, partition_id: OID) -> RS<()> {
        let Some(&worker_index) = self.worker_id_to_index.get(&worker_id) else {
            return Err(mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no worker with id {}", worker_id)
            ));
        };
        {
            let owner = self
                .partition_id_to_worker_id
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "worker registry lock poisoned"))?
                .get(&partition_id)
                .copied();
            if let Some(owner) = owner {
                if owner == worker_id {
                    return Ok(());
                }
                return Err(mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "partition {} is owned by worker {} (release it before adoption)",
                        partition_id, owner
                    )
                ));
            }
        }
        let Some(dir) = &self.marker_dir else {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "worker registry has no marker directory (in-memory only)"
            ));
        };
        touch_marker(dir.join(partition_marker_name(worker_id, partition_id)))?;
        self.partition_id_to_worker_id
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker registry lock poisoned"))?
            .insert(partition_id, worker_id);
        self.partition_id_to_worker_index
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker registry lock poisoned"))?
            .insert(partition_id, worker_index);
        Ok(())
    }

    /// Runtime release (phase 3): drops `partition_id` from the indexes and
    /// removes its marker file. Idempotent.
    pub fn release_partition(&self, partition_id: OID) -> RS<()> {
        let owner = self
            .partition_id_to_worker_id
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker registry lock poisoned"))?
            .remove(&partition_id);
        self.partition_id_to_worker_index
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker registry lock poisoned"))?
            .remove(&partition_id);
        if let (Some(dir), Some(owner)) = (&self.marker_dir, owner) {
            let marker = dir.join(partition_marker_name(owner, partition_id));
            if mudu_sys::fs::sync::sync_path_exists(&marker) {
                mudu_sys::fs::sync::sync_remove_file(&marker)?;
            }
        }
        Ok(())
    }

    /// The id of worker 0, which serves unpartitioned (global) data.
    pub fn default_global_worker_id(&self) -> Option<OID> {
        self.worker(0).map(|worker| worker.worker_id)
    }
}

fn scan_worker_identities(log_dir: &Path) -> RS<Vec<WorkerIdobject>> {
    let mut worker_ids = HashMap::<usize, OID>::new();
    let mut partitions = HashMap::<OID, Vec<OID>>::new();
    for entry in mudu_sys::fs::sync::read_dir_entries(log_dir)
        .map_err(|e| mudu_error!(ErrorCode::Io, "scan worker registry directory error", e))?
    {
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some((worker_index, worker_id)) = parse_worker_marker(file_name) {
            if worker_ids.insert(worker_index, worker_id).is_some() {
                return Err(mudu_error!(
                    ErrorCode::ObjectAlreadyExists,
                    format!("duplicate worker marker for worker index {}", worker_index)
                ));
            }
            partitions.entry(worker_id).or_default();
        } else if let Some((worker_id, partition_id)) = parse_partition_marker(file_name) {
            partitions.entry(worker_id).or_default().push(partition_id);
        }
    }

    let mut workers = Vec::with_capacity(worker_ids.len());
    for (worker_index, worker_id) in worker_ids {
        let mut partition_ids = partitions.remove(&worker_id).unwrap_or_default();
        partition_ids.sort_unstable();
        partition_ids.dedup();
        if partition_ids.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                format!("worker {} has no partition markers", worker_id)
            ));
        }
        workers.push(WorkerIdobject {
            worker_index,
            worker_id,
            partition_ids,
        });
    }
    Ok(workers)
}

fn create_worker_identity(
    log_dir: &Path,
    worker_index: usize,
    worker_id: OID,
) -> RS<WorkerIdobject> {
    let partition_id = non_zero_oid();
    touch_marker(log_dir.join(worker_marker_name(worker_index, worker_id)))?;
    touch_marker(log_dir.join(partition_marker_name(worker_id, partition_id)))?;
    Ok(WorkerIdobject {
        worker_index,
        worker_id,
        partition_ids: vec![partition_id],
    })
}

fn validate_worker_identities(workers: &[WorkerIdobject], worker_count: usize) -> RS<()> {
    if workers.len() != worker_count {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "worker registry count {} does not match expected {}",
                workers.len(),
                worker_count
            )
        ));
    }
    let mut worker_ids = HashSet::new();
    let mut partition_ids = HashSet::new();
    for worker in workers {
        if worker.worker_id == 0 {
            return Err(mudu_error!(ErrorCode::Parse, "worker id cannot be zero"));
        }
        if !worker_ids.insert(worker.worker_id) {
            return Err(mudu_error!(
                ErrorCode::ObjectAlreadyExists,
                format!("duplicate worker id {}", worker.worker_id)
            ));
        }
        if worker.partition_ids.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!("worker {} has no partitions", worker.worker_id)
            ));
        }
        for &partition_id in &worker.partition_ids {
            if partition_id == 0 {
                return Err(mudu_error!(ErrorCode::Parse, "partition id cannot be zero"));
            }
            if !partition_ids.insert(partition_id) {
                return Err(mudu_error!(
                    ErrorCode::ObjectAlreadyExists,
                    format!("duplicate partition id {}", partition_id)
                ));
            }
        }
    }
    Ok(())
}

fn touch_marker(path: PathBuf) -> RS<()> {
    mudu_sys::fs::sync::SOpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map(|_| ())
}

fn worker_marker_name(worker_index: usize, worker_id: OID) -> String {
    format!(
        "worker.{}.{}{}",
        worker_index,
        oid_hex(worker_id),
        WORKER_MARKER_SUFFIX
    )
}

fn partition_marker_name(worker_id: OID, partition_id: OID) -> String {
    format!(
        "partition.{}.{}{}",
        oid_hex(worker_id),
        oid_hex(partition_id),
        PARTITION_MARKER_SUFFIX
    )
}

fn parse_worker_marker(file_name: &str) -> Option<(usize, OID)> {
    let base = file_name.strip_suffix(WORKER_MARKER_SUFFIX)?;
    let mut parts = base.split('.');
    if parts.next()? != "worker" {
        return None;
    }
    let worker_index = parts.next()?.parse::<usize>().ok()?;
    let worker_id = u128::from_str_radix(parts.next()?, 16).ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((worker_index, worker_id))
}

fn parse_partition_marker(file_name: &str) -> Option<(OID, OID)> {
    let base = file_name.strip_suffix(PARTITION_MARKER_SUFFIX)?;
    let mut parts = base.split('.');
    if parts.next()? != "partition" {
        return None;
    }
    let worker_id = u128::from_str_radix(parts.next()?, 16).ok()?;
    let partition_id = u128::from_str_radix(parts.next()?, 16).ok()?;
    if parts.next().is_some() {
        return None;
    }
    Some((worker_id, partition_id))
}

fn oid_hex(oid: OID) -> String {
    format!("{:032x}", oid)
}

fn non_zero_oid() -> OID {
    loop {
        let oid = gen_oid();
        if oid != 0 {
            return oid;
        }
    }
}

/// The [`catalog::contract::partition_adoption::PartitionAdoption`]
/// implementation of one worker: updates the registry
/// (durable markers) and the runtime's live partition set (shared with
/// `WorkerRuntime`).
pub struct WorkerPartitionAdoption {
    /// The server's worker registry (marker files + lookup indexes).
    pub registry: Arc<WorkerRegistry>,
    /// The adopting/releasing worker's id.
    pub worker_id: OID,
    /// The runtime's live partition set.
    pub partition_ids: Arc<mudu_sys::sync::SMutex<Vec<OID>>>,
}

impl catalog::contract::partition_adoption::PartitionAdoption for WorkerPartitionAdoption {
    fn adopt(&self, partition_id: OID) -> RS<()> {
        self.registry
            .adopt_partition(self.worker_id, partition_id)?;
        let mut partitions = self
            .partition_ids
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker partition set poisoned"))?;
        if !partitions.contains(&partition_id) {
            partitions.push(partition_id);
        }
        Ok(())
    }

    fn release(&self, partition_id: OID) -> RS<()> {
        self.registry.release_partition(partition_id)?;
        self.partition_ids
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker partition set poisoned"))?
            .retain(|partition| *partition != partition_id);
        Ok(())
    }
}
