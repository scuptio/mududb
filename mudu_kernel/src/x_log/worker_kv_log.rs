use mudu::common::buf::Buf;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use short_uuid::ShortUuid;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkerKvLog {
    inner: Arc<WorkerKvLogInner>,
}

struct WorkerKvLogInner {
    state: Mutex<ChunkedWorkerKvLog>,
}

#[derive(Clone, Debug)]
pub struct WorkerLogLayout {
    log_dir: PathBuf,
    log_oid: OID,
    chunk_size: u64,
    short_oid: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLogTail {
    pub current_sequence: Option<u64>,
    pub current_size: u64,
    pub next_sequence: u64,
}

struct ChunkedWorkerKvLog {
    layout: WorkerLogLayout,
    current: Option<ActiveChunk>,
    next_sequence: u64,
}

struct ActiveChunk {
    file: std::fs::File,
    size: u64,
}

impl WorkerKvLog {
    pub fn encode_put_record<K: AsRef<[u8]>, V: AsRef<[u8]>>(key: K, value: V) -> Buf {
        encode_put_payload(key.as_ref(), value.as_ref())
    }

    pub fn new(layout: WorkerLogLayout) -> RS<Self> {
        let tail = layout.scan_tail()?;
        Ok(Self {
            inner: Arc::new(WorkerKvLogInner {
                state: Mutex::new(ChunkedWorkerKvLog::new(layout, tail)?),
            }),
        })
    }

    pub fn append_put<K: AsRef<[u8]>, V: AsRef<[u8]>>(&self, key: K, value: V) -> RS<()> {
        let payload = Self::encode_put_record(key, value);
        self.append_raw(&payload)
    }

    pub fn append_raw(&self, payload: &[u8]) -> RS<()> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        guard.append(payload)
    }

    pub fn flush(&self) -> RS<()> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        guard.flush()
    }

    pub fn decode_put_records(payload: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let mut offset = 0usize;
        let mut records = Vec::new();
        while offset < payload.len() {
            if payload.len() - offset < 10 {
                return Err(m_error!(
                    EC::DecodeErr,
                    "worker kv log record header is truncated"
                ));
            }
            let header = &payload[offset..offset + 4];
            if header != b"MKVL" {
                return Err(m_error!(
                    EC::DecodeErr,
                    "invalid worker kv log record magic"
                ));
            }
            let version = payload[offset + 4];
            let kind = payload[offset + 5];
            if version != 1 || kind != 1 {
                return Err(m_error!(
                    EC::DecodeErr,
                    format!(
                        "unsupported worker kv log record version={} kind={}",
                        version, kind
                    )
                ));
            }
            let key_len =
                u32::from_be_bytes(payload[offset + 6..offset + 10].try_into().unwrap()) as usize;
            let key_start = offset + 10;
            let key_end = key_start + key_len;
            if key_end + 4 > payload.len() {
                return Err(m_error!(
                    EC::DecodeErr,
                    "worker kv log key payload is truncated"
                ));
            }
            let value_len =
                u32::from_be_bytes(payload[key_end..key_end + 4].try_into().unwrap()) as usize;
            let value_start = key_end + 4;
            let value_end = value_start + value_len;
            if value_end > payload.len() {
                return Err(m_error!(
                    EC::DecodeErr,
                    "worker kv log value payload is truncated"
                ));
            }
            records.push((
                payload[key_start..key_end].to_vec(),
                payload[value_start..value_end].to_vec(),
            ));
            offset = value_end;
        }
        Ok(records)
    }
}

impl WorkerLogLayout {
    pub fn new<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> RS<Self> {
        if chunk_size == 0 {
            return Err(m_error!(
                EC::ParseErr,
                "worker log chunk size must be greater than zero"
            ));
        }
        Ok(Self {
            log_dir: log_dir.into(),
            log_oid,
            chunk_size,
            short_oid: ShortUuid::from_uuid(&Uuid::from_u128(log_oid)).to_string(),
        })
    }

    pub fn log_oid(&self) -> OID {
        self.log_oid
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn chunk_path(&self, sequence: u64) -> PathBuf {
        self.log_dir
            .join(format!("{}.{}.xl", self.short_oid, sequence))
    }

    pub fn scan_tail(&self) -> RS<WorkerLogTail> {
        std::fs::create_dir_all(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "create worker kv log directory error", e))?;
        let mut max_sequence: Option<u64> = None;
        for entry in std::fs::read_dir(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "scan worker kv log directory error", e))?
        {
            let entry =
                entry.map_err(|e| m_error!(EC::IOErr, "read worker kv log directory entry", e))?;
            if let Some(sequence) = self.parse_chunk_sequence(entry.path().as_path()) {
                max_sequence = Some(max_sequence.map_or(sequence, |current| current.max(sequence)));
            }
        }
        let Some(sequence) = max_sequence else {
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: 0,
            });
        };
        let size = std::fs::metadata(self.chunk_path(sequence))
            .map_err(|e| m_error!(EC::IOErr, "read worker kv chunk metadata error", e))?
            .len();
        if size < self.chunk_size {
            Ok(WorkerLogTail {
                current_sequence: Some(sequence),
                current_size: size,
                next_sequence: sequence + 1,
            })
        } else {
            Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: sequence + 1,
            })
        }
    }

    pub fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        std::fs::create_dir_all(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "create worker kv log directory error", e))?;
        let mut entries = Vec::<(u64, PathBuf)>::new();
        for entry in std::fs::read_dir(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "scan worker kv log directory error", e))?
        {
            let entry =
                entry.map_err(|e| m_error!(EC::IOErr, "read worker kv log directory entry", e))?;
            let path = entry.path();
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                entries.push((sequence, path));
            }
        }
        entries.sort_by_key(|(sequence, _)| *sequence);
        Ok(entries.into_iter().map(|(_, path)| path).collect())
    }

    fn parse_chunk_sequence(&self, path: &Path) -> Option<u64> {
        let file_name = path.file_name()?.to_str()?;
        let prefix = format!("{}.", self.short_oid);
        let suffix = ".xl";
        if !file_name.starts_with(&prefix) || !file_name.ends_with(suffix) {
            return None;
        }
        let sequence = &file_name[prefix.len()..file_name.len() - suffix.len()];
        sequence.parse::<u64>().ok()
    }
}

impl ChunkedWorkerKvLog {
    fn new(layout: WorkerLogLayout, tail: WorkerLogTail) -> RS<Self> {
        let current = match tail.current_sequence {
            Some(sequence) => Some(Self::open_existing_chunk(
                &layout,
                sequence,
                tail.current_size,
            )?),
            None => None,
        };
        Ok(Self {
            layout,
            current,
            next_sequence: tail.next_sequence,
        })
    }

    fn append(&mut self, payload: &[u8]) -> RS<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let payload_len = payload.len() as u64;
        if payload_len > self.layout.chunk_size() {
            self.current = None;
            let mut dedicated = self.open_fresh_chunk()?;
            Self::write_payload(&mut dedicated, payload)?;
            Self::flush_chunk(&mut dedicated)?;
            return Ok(());
        }

        let needs_rotate = self
            .current
            .as_ref()
            .map(|chunk| chunk.size + payload_len > self.layout.chunk_size())
            .unwrap_or(true);
        if needs_rotate {
            self.current = Some(self.open_fresh_chunk()?);
        }

        let chunk = self.current.as_mut().expect("current chunk must exist");
        Self::write_payload(chunk, payload)?;
        if chunk.size >= self.layout.chunk_size() {
            self.current = None;
        }
        Ok(())
    }

    fn flush(&mut self) -> RS<()> {
        if let Some(chunk) = self.current.as_mut() {
            Self::flush_chunk(chunk)?;
        }
        Ok(())
    }

    fn write_payload(chunk: &mut ActiveChunk, payload: &[u8]) -> RS<()> {
        chunk
            .file
            .write_all(payload)
            .map_err(|e| m_error!(EC::IOErr, "append worker kv log payload error", e))?;
        chunk.size += payload.len() as u64;
        Ok(())
    }

    fn flush_chunk(chunk: &mut ActiveChunk) -> RS<()> {
        chunk
            .file
            .flush()
            .map_err(|e| m_error!(EC::IOErr, "flush worker kv log payload error", e))?;
        Ok(())
    }

    fn open_fresh_chunk(&mut self) -> RS<ActiveChunk> {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        Self::open_chunk_file(&self.layout, sequence)
    }

    fn open_existing_chunk(layout: &WorkerLogLayout, sequence: u64, size: u64) -> RS<ActiveChunk> {
        let mut chunk = Self::open_chunk_file(layout, sequence)?;
        chunk.size = size;
        Ok(chunk)
    }

    fn open_chunk_file(layout: &WorkerLogLayout, sequence: u64) -> RS<ActiveChunk> {
        let path = layout.chunk_path(sequence);
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .append(true)
            .open(path)
            .map_err(|e| m_error!(EC::IOErr, "open worker kv log chunk error", e))?;
        let size = file
            .metadata()
            .map_err(|e| m_error!(EC::IOErr, "read worker kv log chunk metadata error", e))?
            .len();
        Ok(ActiveChunk { file, size })
    }
}

fn encode_put_payload(key: &[u8], value: &[u8]) -> Buf {
    let mut payload = Vec::with_capacity(14 + key.len() + value.len());
    // The record starts with a short fixed header so recovery can distinguish
    // the real payload from the zero padding required by O_DIRECT writes.
    payload.extend_from_slice(b"MKVL");
    payload.push(1u8);
    payload.push(1u8);
    payload.extend_from_slice(&(key.len() as u32).to_be_bytes());
    payload.extend_from_slice(key);
    payload.extend_from_slice(&(value.len() as u32).to_be_bytes());
    payload.extend_from_slice(value);
    payload
}

#[cfg(test)]
mod tests {
    use super::*;
    use mudu::common::id::gen_oid;
    use std::env::temp_dir;

    #[test]
    fn worker_log_appends_payload() {
        let dir = temp_dir().join(format!("worker_kv_log_test_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
        let path = layout.chunk_path(0);
        let log = WorkerKvLog::new(layout).unwrap();
        log.append_put(b"k1", b"v1").unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn worker_log_encodes_framed_put_record() {
        let payload = encode_put_payload(b"k1", b"v1");
        assert_eq!(&payload[0..4], b"MKVL");
        assert_eq!(payload[4], 1);
        assert_eq!(payload[5], 1);
        assert_eq!(u32::from_be_bytes(payload[6..10].try_into().unwrap()), 2);
        assert_eq!(&payload[10..12], b"k1");
        assert_eq!(u32::from_be_bytes(payload[12..16].try_into().unwrap()), 2);
        assert_eq!(&payload[16..18], b"v1");
    }

    #[test]
    fn worker_log_rotates_chunks_by_size() {
        let dir = temp_dir().join(format!("worker_kv_log_chunk_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 40).unwrap();
        let prefix = layout.short_oid.clone();
        let log = WorkerKvLog::new(layout).unwrap();
        log.append_raw(&vec![1u8; 20]).unwrap();
        log.append_raw(&vec![2u8; 20]).unwrap();
        log.append_raw(&vec![3u8; 20]).unwrap();
        assert!(dir.join(format!("{}.0.xl", prefix)).exists());
        assert!(dir.join(format!("{}.1.xl", prefix)).exists());
    }

    #[test]
    fn worker_log_places_oversized_entry_in_dedicated_chunk() {
        let dir = temp_dir().join(format!("worker_kv_log_oversized_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 32).unwrap();
        let prefix = layout.short_oid.clone();
        let log = WorkerKvLog::new(layout).unwrap();
        log.append_raw(&vec![1u8; 8]).unwrap();
        log.append_raw(&vec![2u8; 64]).unwrap();
        log.append_raw(&vec![3u8; 8]).unwrap();
        assert_eq!(
            std::fs::metadata(dir.join(format!("{}.0.xl", prefix)))
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            std::fs::metadata(dir.join(format!("{}.1.xl", prefix)))
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            std::fs::metadata(dir.join(format!("{}.2.xl", prefix)))
                .unwrap()
                .len(),
            8
        );
    }
}
