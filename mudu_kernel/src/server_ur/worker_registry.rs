use mudu::common::id::{gen_oid, OID};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::collections::{HashMap, HashSet};
use std::fs::{self, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const WORKER_MARKER_SUFFIX: &str = ".wid";
const PARTITION_MARKER_SUFFIX: &str = ".pid";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerIdentity {
    pub worker_index: usize,
    pub worker_id: OID,
    pub partition_ids: Vec<OID>,
}

#[derive(Debug, Clone)]
pub struct WorkerRegistry {
    workers: Vec<WorkerIdentity>,
    worker_id_to_index: HashMap<OID, usize>,
    partition_id_to_worker_index: HashMap<OID, usize>,
    partition_id_to_worker_id: HashMap<OID, OID>,
}

pub fn load_or_create_worker_registry<P: AsRef<Path>>(
    log_dir: P,
    worker_count: usize,
) -> RS<Arc<WorkerRegistry>> {
    if worker_count == 0 {
        return Err(m_error!(
            EC::ParseErr,
            "worker count must be greater than zero"
        ));
    }
    let log_dir = log_dir.as_ref().to_path_buf();
    fs::create_dir_all(&log_dir)
        .map_err(|e| m_error!(EC::IOErr, "create worker registry log directory error", e))?;

    let mut loaded = scan_worker_identities(&log_dir)?;
    for worker_index in 0..worker_count {
        if !loaded
            .iter()
            .any(|worker| worker.worker_index == worker_index)
        {
            loaded.push(create_worker_identity(&log_dir, worker_index)?);
        }
    }
    loaded.sort_by_key(|worker| worker.worker_index);
    validate_worker_identities(&loaded, worker_count)?;
    Ok(Arc::new(WorkerRegistry::new(loaded)?))
}

impl WorkerRegistry {
    pub fn new(workers: Vec<WorkerIdentity>) -> RS<Self> {
        let mut worker_id_to_index = HashMap::new();
        let mut partition_id_to_worker_index = HashMap::new();
        let mut partition_id_to_worker_id = HashMap::new();
        for worker in &workers {
            if worker_id_to_index
                .insert(worker.worker_id, worker.worker_index)
                .is_some()
            {
                return Err(m_error!(
                    EC::ExistingSuchElement,
                    format!("duplicate worker id {}", worker.worker_id)
                ));
            }
            for &partition_id in &worker.partition_ids {
                if partition_id_to_worker_index
                    .insert(partition_id, worker.worker_index)
                    .is_some()
                {
                    return Err(m_error!(
                        EC::ExistingSuchElement,
                        format!("duplicate partition id {}", partition_id)
                    ));
                }
                partition_id_to_worker_id.insert(partition_id, worker.worker_id);
            }
        }
        Ok(Self {
            workers,
            worker_id_to_index,
            partition_id_to_worker_index,
            partition_id_to_worker_id,
        })
    }

    pub fn workers(&self) -> &[WorkerIdentity] {
        &self.workers
    }

    pub fn worker(&self, worker_index: usize) -> Option<&WorkerIdentity> {
        self.workers
            .iter()
            .find(|worker| worker.worker_index == worker_index)
    }

    pub fn worker_index_by_worker_id(&self, worker_id: OID) -> Option<usize> {
        self.worker_id_to_index.get(&worker_id).copied()
    }

    pub fn worker_id_by_partition_id(&self, partition_id: OID) -> Option<OID> {
        self.partition_id_to_worker_id.get(&partition_id).copied()
    }

    pub fn worker_index_by_partition_id(&self, partition_id: OID) -> Option<usize> {
        self.partition_id_to_worker_index
            .get(&partition_id)
            .copied()
    }
}

fn scan_worker_identities(log_dir: &Path) -> RS<Vec<WorkerIdentity>> {
    let mut worker_ids = HashMap::<usize, OID>::new();
    let mut partitions = HashMap::<OID, Vec<OID>>::new();
    for entry in fs::read_dir(log_dir)
        .map_err(|e| m_error!(EC::IOErr, "scan worker registry directory error", e))?
    {
        let entry = entry
            .map_err(|e| m_error!(EC::IOErr, "read worker registry directory entry error", e))?;
        let path = entry.path();
        let Some(file_name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };
        if let Some((worker_index, worker_id)) = parse_worker_marker(file_name) {
            if worker_ids.insert(worker_index, worker_id).is_some() {
                return Err(m_error!(
                    EC::ExistingSuchElement,
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
            return Err(m_error!(
                EC::NoneErr,
                format!("worker {} has no partition markers", worker_id)
            ));
        }
        workers.push(WorkerIdentity {
            worker_index,
            worker_id,
            partition_ids,
        });
    }
    Ok(workers)
}

fn create_worker_identity(log_dir: &Path, worker_index: usize) -> RS<WorkerIdentity> {
    let worker_id = non_zero_oid();
    let partition_id = non_zero_oid();
    touch_marker(log_dir.join(worker_marker_name(worker_index, worker_id)))?;
    touch_marker(log_dir.join(partition_marker_name(worker_id, partition_id)))?;
    Ok(WorkerIdentity {
        worker_index,
        worker_id,
        partition_ids: vec![partition_id],
    })
}

fn validate_worker_identities(workers: &[WorkerIdentity], worker_count: usize) -> RS<()> {
    if workers.len() != worker_count {
        return Err(m_error!(
            EC::ParseErr,
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
            return Err(m_error!(EC::ParseErr, "worker id cannot be zero"));
        }
        if !worker_ids.insert(worker.worker_id) {
            return Err(m_error!(
                EC::ExistingSuchElement,
                format!("duplicate worker id {}", worker.worker_id)
            ));
        }
        if worker.partition_ids.is_empty() {
            return Err(m_error!(
                EC::ParseErr,
                format!("worker {} has no partitions", worker.worker_id)
            ));
        }
        for &partition_id in &worker.partition_ids {
            if partition_id == 0 {
                return Err(m_error!(EC::ParseErr, "partition id cannot be zero"));
            }
            if !partition_ids.insert(partition_id) {
                return Err(m_error!(
                    EC::ExistingSuchElement,
                    format!("duplicate partition id {}", partition_id)
                ));
            }
        }
    }
    Ok(())
}

fn touch_marker(path: PathBuf) -> RS<()> {
    OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(path)
        .map(|_| ())
        .map_err(|e| m_error!(EC::IOErr, "create worker registry marker error", e))
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
