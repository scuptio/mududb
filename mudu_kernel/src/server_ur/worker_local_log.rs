use crate::x_log::worker_kv_log::{WorkerKvLog, WorkerLogLayout};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::collections::{HashMap, VecDeque};
use std::ffi::CString;
use std::fs::File;
use std::os::fd::{AsRawFd, FromRawFd, IntoRawFd, RawFd};
use std::os::unix::ffi::OsStrExt;
use std::sync::Arc;

pub(in crate::server_ur) struct WorkerLocalLog {
    layout: WorkerLogLayout,
    current_sequence: Option<u64>,
    next_sequence: u64,
    chunks: HashMap<u64, LogChunkState>,
    pending: VecDeque<PendingLogWrite>,
    next_open_request_id: u64,
    next_close_request_id: u64,
    pending_close: VecDeque<CloseFileRequest>,
    inflight_open: Option<InflightLogOpen>,
    inflight_close: Option<u64>,
    inflight_write: Option<InflightLogWrite>,
}

#[derive(Clone)]
pub(in crate::server_ur) struct OpenFileRequest {
    request_id: u64,
    path: CString,
    flags: i32,
    mode: u32,
}

#[derive(Clone)]
pub(in crate::server_ur) struct CloseFileRequest {
    request_id: u64,
    fd: RawFd,
}

pub(in crate::server_ur) struct InflightLogWrite {
    chunk_sequence: u64,
    file: Arc<File>,
    offset: u64,
    payload: Vec<u8>,
}

struct PendingLogWrite {
    chunk_sequence: u64,
    offset: u64,
    payload: Vec<u8>,
}

struct InflightLogOpen {
    request_id: u64,
    chunk_sequence: u64,
}

struct LogChunkState {
    file: Option<Arc<File>>,
    opening: bool,
    size: u64,
}

impl WorkerLocalLog {
    pub(in crate::server_ur) fn open(layout: WorkerLogLayout) -> RS<Self> {
        let tail = layout.scan_tail()?;
        let mut chunks = HashMap::new();
        if let Some(sequence) = tail.current_sequence {
            chunks.insert(
                sequence,
                LogChunkState {
                    file: None,
                    opening: false,
                    size: tail.current_size,
                },
            );
        }
        Ok(Self {
            layout,
            current_sequence: tail.current_sequence,
            next_sequence: tail.next_sequence,
            chunks,
            pending: VecDeque::new(),
            next_open_request_id: 1,
            next_close_request_id: 1,
            pending_close: VecDeque::new(),
            inflight_open: None,
            inflight_close: None,
            inflight_write: None,
        })
    }

    pub(in crate::server_ur) fn enqueue_put(&mut self, key: &[u8], value: &[u8]) -> RS<()> {
        self.enqueue_payload(WorkerKvLog::encode_put_record(key, value))
    }

    pub(in crate::server_ur) fn take_pending_open_file(&mut self) -> RS<Option<OpenFileRequest>> {
        if self.inflight_open.is_some() {
            return Ok(None);
        }
        let Some(write) = self.pending.front() else {
            return Ok(None);
        };
        let Some(chunk) = self.chunks.get_mut(&write.chunk_sequence) else {
            return Err(m_error!(
                EC::InternalErr,
                format!("missing log chunk state {}", write.chunk_sequence)
            ));
        };
        if chunk.file.is_some() || chunk.opening {
            return Ok(None);
        }
        let path = self.layout.chunk_path(write.chunk_sequence);
        let path = CString::new(path.as_os_str().as_bytes())
            .map_err(|_| m_error!(EC::ParseErr, "worker log chunk path contains NUL byte"))?;
        let request_id = self.next_open_request_id;
        self.next_open_request_id += 1;
        chunk.opening = true;
        self.inflight_open = Some(InflightLogOpen {
            request_id,
            chunk_sequence: write.chunk_sequence,
        });
        Ok(Some(OpenFileRequest::new(
            request_id,
            path,
            libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC,
            0o644,
        )))
    }

    pub(in crate::server_ur) fn rollback_pending_open_file(&mut self, request_id: u64) -> RS<()> {
        let inflight = self.inflight_open.take().ok_or_else(|| {
            m_error!(
                EC::InternalErr,
                "rollback worker log open without inflight open"
            )
        })?;
        if inflight.request_id != request_id {
            return Err(m_error!(
                EC::InternalErr,
                format!(
                    "rollback worker log open request id mismatch: expected {}, got {}",
                    inflight.request_id, request_id
                )
            ));
        }
        let chunk = self
            .chunks
            .get_mut(&inflight.chunk_sequence)
            .ok_or_else(|| {
                m_error!(
                    EC::InternalErr,
                    "worker log chunk disappeared during rollback"
                )
            })?;
        chunk.opening = false;
        Ok(())
    }

    pub(in crate::server_ur) fn finish_pending_open_file(
        &mut self,
        request_id: u64,
        fd: RawFd,
    ) -> RS<()> {
        let inflight = self.inflight_open.take().ok_or_else(|| {
            m_error!(
                EC::InternalErr,
                "worker log open completion without inflight open"
            )
        })?;
        if inflight.request_id != request_id {
            return Err(m_error!(
                EC::InternalErr,
                format!(
                    "worker log open completion request id mismatch: expected {}, got {}",
                    inflight.request_id, request_id
                )
            ));
        }
        let chunk = self
            .chunks
            .get_mut(&inflight.chunk_sequence)
            .ok_or_else(|| m_error!(EC::InternalErr, "worker log chunk disappeared during open"))?;
        chunk.opening = false;
        let file = unsafe { File::from_raw_fd(fd) };
        chunk.file = Some(Arc::new(file));
        Ok(())
    }

    pub(in crate::server_ur) fn take_pending_close_file(&mut self) -> Option<CloseFileRequest> {
        if self.inflight_close.is_some() {
            return None;
        }
        let request = self.pending_close.pop_front()?;
        self.inflight_close = Some(request.request_id());
        Some(request)
    }

    pub(in crate::server_ur) fn rollback_pending_close_file(
        &mut self,
        request: CloseFileRequest,
    ) -> RS<()> {
        if self.inflight_close.take() != Some(request.request_id()) {
            return Err(m_error!(
                EC::InternalErr,
                format!(
                    "rollback worker log close request id mismatch for {}",
                    request.request_id()
                )
            ));
        }
        self.pending_close.push_front(request);
        Ok(())
    }

    pub(in crate::server_ur) fn finish_pending_close_file(&mut self, request_id: u64) -> RS<()> {
        if self.inflight_close.take() != Some(request_id) {
            return Err(m_error!(
                EC::InternalErr,
                format!(
                    "worker log close completion request id mismatch for {}",
                    request_id
                )
            ));
        }
        Ok(())
    }

    pub(in crate::server_ur) fn take_pending_write(&mut self) -> Option<InflightLogWrite> {
        if self.inflight_write.is_some() {
            return None;
        }
        let write = self.pending.front()?;
        let chunk = self.chunks.get(&write.chunk_sequence)?;
        let file = chunk.file.clone()?;
        let write = self.pending.pop_front()?;
        Some(InflightLogWrite::new(
            write.chunk_sequence,
            file,
            write.offset,
            write.payload,
        ))
    }

    pub(in crate::server_ur) fn inflight_open(&self) -> bool {
        self.inflight_open.is_some()
    }

    pub(in crate::server_ur) fn inflight_close(&self) -> bool {
        self.inflight_close.is_some()
    }

    pub(in crate::server_ur) fn inflight_write(&self) -> Option<&InflightLogWrite> {
        self.inflight_write.as_ref()
    }

    pub(in crate::server_ur) fn take_inflight_write(&mut self) -> Option<InflightLogWrite> {
        self.inflight_write.take()
    }

    pub(in crate::server_ur) fn set_inflight_write(&mut self, inflight: Option<InflightLogWrite>) {
        self.inflight_write = inflight;
    }

    pub(in crate::server_ur) fn cleanup_chunk_if_unused(&mut self, chunk_sequence: u64) -> RS<()> {
        let still_referenced = self.current_sequence == Some(chunk_sequence)
            || self
                .pending
                .iter()
                .any(|write| write.chunk_sequence == chunk_sequence)
            || self
                .inflight_open
                .as_ref()
                .map(|open| open.chunk_sequence == chunk_sequence)
                .unwrap_or(false)
            || self
                .inflight_write
                .as_ref()
                .map(|write| write.chunk_sequence == chunk_sequence)
                .unwrap_or(false);
        if !still_referenced {
            let Some(chunk) = self.chunks.remove(&chunk_sequence) else {
                return Ok(());
            };
            if let Some(file) = chunk.file {
                match Arc::try_unwrap(file) {
                    Ok(file) => {
                        let request_id = self.next_close_request_id;
                        self.next_close_request_id += 1;
                        self.pending_close
                            .push_back(CloseFileRequest::new(request_id, file.into_raw_fd()));
                    }
                    Err(_shared) => {}
                }
            }
        }
        Ok(())
    }

    fn enqueue_payload(&mut self, payload: Vec<u8>) -> RS<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let payload_len = payload.len() as u64;
        let (chunk_sequence, offset) = if payload_len > self.layout.chunk_size() {
            let chunk_sequence = self.allocate_chunk_sequence();
            self.chunks.insert(
                chunk_sequence,
                LogChunkState {
                    file: None,
                    opening: false,
                    size: payload_len,
                },
            );
            (chunk_sequence, 0)
        } else {
            let chunk_sequence = self.select_current_chunk(payload_len);
            let chunk = self.chunks.get_mut(&chunk_sequence).ok_or_else(|| {
                m_error!(
                    EC::InternalErr,
                    format!("missing current log chunk state {}", chunk_sequence)
                )
            })?;
            let offset = chunk.size;
            chunk.size += payload_len;
            if chunk.size >= self.layout.chunk_size()
                && self.current_sequence == Some(chunk_sequence)
            {
                self.current_sequence = None;
            }
            (chunk_sequence, offset)
        };
        self.pending.push_back(PendingLogWrite {
            chunk_sequence,
            offset,
            payload,
        });
        Ok(())
    }

    fn select_current_chunk(&mut self, payload_len: u64) -> u64 {
        if let Some(sequence) = self.current_sequence {
            if let Some(chunk) = self.chunks.get(&sequence) {
                if chunk.size + payload_len <= self.layout.chunk_size() {
                    return sequence;
                }
            }
        }
        let sequence = self.allocate_chunk_sequence();
        self.chunks.insert(
            sequence,
            LogChunkState {
                file: None,
                opening: false,
                size: 0,
            },
        );
        self.current_sequence = Some(sequence);
        sequence
    }

    fn allocate_chunk_sequence(&mut self) -> u64 {
        let sequence = self.next_sequence;
        self.next_sequence += 1;
        sequence
    }
}

impl InflightLogWrite {
    fn new(chunk_sequence: u64, file: Arc<File>, offset: u64, payload: Vec<u8>) -> Self {
        Self {
            chunk_sequence,
            file,
            offset,
            payload,
        }
    }

    pub(in crate::server_ur) fn chunk_sequence(&self) -> u64 {
        self.chunk_sequence
    }

    pub(in crate::server_ur) fn fd(&self) -> RawFd {
        self.file.as_raw_fd()
    }

    pub(in crate::server_ur) fn offset(&self) -> u64 {
        self.offset
    }

    pub(in crate::server_ur) fn payload(&self) -> &[u8] {
        &self.payload
    }

    pub(in crate::server_ur) fn payload_len(&self) -> usize {
        self.payload.len()
    }

    pub(in crate::server_ur) fn consume_prefix(&mut self, written: usize) {
        self.payload.drain(0..written);
        self.offset += written as u64;
    }
}

impl OpenFileRequest {
    pub(in crate::server_ur) fn new(request_id: u64, path: CString, flags: i32, mode: u32) -> Self {
        Self {
            request_id,
            path,
            flags,
            mode,
        }
    }

    pub(in crate::server_ur) fn request_id(&self) -> u64 {
        self.request_id
    }

    pub(in crate::server_ur) fn path(&self) -> &CString {
        &self.path
    }

    pub(in crate::server_ur) fn flags(&self) -> i32 {
        self.flags
    }

    pub(in crate::server_ur) fn mode(&self) -> u32 {
        self.mode
    }
}

impl CloseFileRequest {
    fn new(request_id: u64, fd: RawFd) -> Self {
        Self { request_id, fd }
    }

    pub(in crate::server_ur) fn request_id(&self) -> u64 {
        self.request_id
    }

    pub(in crate::server_ur) fn fd(&self) -> RawFd {
        self.fd
    }
}
