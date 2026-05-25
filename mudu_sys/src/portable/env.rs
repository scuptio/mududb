use crate::api::env::SysEnv;
use crate::api::fs::SysFs;
use crate::api::net::SysNet;
use crate::api::random::SysRandom;
use crate::api::sync::SysSync;
use crate::api::task_async::SysTaskAsync;
use crate::api::task_sync::SysTaskSync;
use crate::api::time::SysTime;
use crate::portable::task_async::PortableTaskAsync;
use crate::portable::task_sync::PortableTaskSync;
use chrono::{DateTime, Utc};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::time::{Instant, SystemTime};
use uuid::Uuid;

pub struct PortableSysEnv {
    time: PortableTime,
    random: PortableRandom,
    fs: PortableFs,
    net: UnsupportedNet,
    task_async: PortableTaskAsync,
    task_sync: PortableTaskSync,
    sync: UnsupportedSync,
}

impl PortableSysEnv {
    pub fn new() -> Self {
        Self {
            time: PortableTime,
            random: PortableRandom,
            fs: PortableFs,
            net: UnsupportedNet,
            task_async: PortableTaskAsync,
            task_sync: PortableTaskSync,
            sync: UnsupportedSync,
        }
    }
}

impl SysEnv for PortableSysEnv {
    fn time(&self) -> &dyn SysTime {
        &self.time
    }

    fn random(&self) -> &dyn SysRandom {
        &self.random
    }

    fn fs(&self) -> &dyn SysFs {
        &self.fs
    }

    fn net(&self) -> &dyn SysNet {
        &self.net
    }

    fn task_async(&self) -> &dyn SysTaskAsync {
        &self.task_async
    }

    fn task_sync(&self) -> &dyn SysTaskSync {
        &self.task_sync
    }

    fn sync(&self) -> &dyn SysSync {
        &self.sync
    }
}

struct PortableTime;

impl SysTime for PortableTime {
    fn instant_now(&self) -> Instant {
        Instant::now()
    }

    fn system_time_now(&self) -> SystemTime {
        SystemTime::now()
    }

    fn utc_now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

struct PortableRandom;

impl SysRandom for PortableRandom {
    fn uuid_v4(&self) -> Uuid {
        Uuid::new_v4()
    }
}

struct PortableFs;

#[cfg(not(target_arch = "wasm32"))]
fn flag_rdwr(flags: i32) -> i32 {
    flags & libc::O_RDWR
}

#[cfg(target_arch = "wasm32")]
fn flag_rdwr(flags: i32) -> i32 {
    // POSIX-compatible fallback values used by wasi-style open flags.
    flags & 0x0002
}

#[cfg(not(target_arch = "wasm32"))]
fn flag_wronly(flags: i32) -> i32 {
    flags & libc::O_WRONLY
}

#[cfg(target_arch = "wasm32")]
fn flag_wronly(flags: i32) -> i32 {
    flags & 0x0001
}

#[cfg(not(target_arch = "wasm32"))]
fn flag_creat(flags: i32) -> i32 {
    flags & libc::O_CREAT
}

#[cfg(target_arch = "wasm32")]
fn flag_creat(flags: i32) -> i32 {
    flags & 0x0040
}

#[cfg(not(target_arch = "wasm32"))]
fn flag_trunc(flags: i32) -> i32 {
    flags & libc::O_TRUNC
}

#[cfg(target_arch = "wasm32")]
fn flag_trunc(flags: i32) -> i32 {
    flags & 0x0200
}

#[cfg(not(target_arch = "wasm32"))]
fn flag_append(flags: i32) -> i32 {
    flags & libc::O_APPEND
}

#[cfg(target_arch = "wasm32")]
fn flag_append(flags: i32) -> i32 {
    flags & 0x0400
}

impl SysFs for PortableFs {
    fn open(&self, path: &Path, flags: i32, _mode: u32) -> RS<File> {
        let mut options = std::fs::OpenOptions::new();
        let read = flag_rdwr(flags) != 0 || flag_wronly(flags) == 0;
        let write = flag_rdwr(flags) != 0 || flag_wronly(flags) != 0;
        options.read(read);
        options.write(write);
        options.create(flag_creat(flags) != 0);
        options.truncate(flag_trunc(flags) != 0);
        options.append(flag_append(flags) != 0);
        options
            .open(path)
            .map_err(|e| m_error!(EC::IOErr, "open file error", e))
    }

    fn read_exact_at(&self, file: &File, len: usize, offset: u64) -> RS<Vec<u8>> {
        let mut cloned = file
            .try_clone()
            .map_err(|e| m_error!(EC::IOErr, "clone file for read_exact_at error", e))?;
        cloned
            .seek(SeekFrom::Start(offset))
            .map_err(|e| m_error!(EC::IOErr, "seek for read_exact_at error", e))?;
        let mut buf = vec![0u8; len];
        cloned
            .read_exact(&mut buf)
            .map_err(|e| m_error!(EC::IOErr, "read_exact_at error", e))?;
        Ok(buf)
    }

    fn write_all_at(&self, file: &File, payload: &[u8], offset: u64) -> RS<()> {
        let mut cloned = file
            .try_clone()
            .map_err(|e| m_error!(EC::IOErr, "clone file for write_all_at error", e))?;
        cloned
            .seek(SeekFrom::Start(offset))
            .map_err(|e| m_error!(EC::IOErr, "seek for write_all_at error", e))?;
        cloned
            .write_all(payload)
            .map_err(|e| m_error!(EC::IOErr, "write_all_at error", e))?;
        Ok(())
    }

    fn fsync(&self, file: &File) -> RS<()> {
        file.sync_all()
            .map_err(|e| m_error!(EC::IOErr, "fsync error", e))
    }

    fn close(&self, file: File) -> RS<()> {
        drop(file);
        Ok(())
    }

    fn create_dir_all(&self, path: &Path) -> RS<()> {
        std::fs::create_dir_all(path).map_err(|e| {
            m_error!(
                EC::IOErr,
                format!("create_dir_all {} error", path.display()),
                e
            )
        })
    }

    fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        let mut entries = Vec::new();
        for entry in std::fs::read_dir(path)
            .map_err(|e| m_error!(EC::IOErr, format!("read_dir {} error", path.display()), e))?
        {
            let entry = entry.map_err(|e| m_error!(EC::IOErr, "read_dir entry error", e))?;
            entries.push(entry.path());
        }
        Ok(entries)
    }

    fn metadata_len(&self, path: &Path) -> RS<u64> {
        Ok(std::fs::metadata(path)
            .map_err(|e| m_error!(EC::IOErr, format!("metadata {} error", path.display()), e))?
            .len())
    }

    fn read_all(&self, path: &Path) -> RS<Vec<u8>> {
        std::fs::read(path)
            .map_err(|e| m_error!(EC::IOErr, format!("read_all {} error", path.display()), e))
    }

    fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(err) => Err(m_error!(
                EC::IOErr,
                format!("remove_file_if_exists {} error", path.display()),
                err
            )),
        }
    }
}

struct UnsupportedNet;

impl SysNet for UnsupportedNet {
    fn create_tcp_listener_fd(&self, _listen_addr: std::net::SocketAddr, _backlog: i32) -> RS<i32> {
        Err(m_error!(
            EC::NotImplemented,
            "network operations are not supported on this target"
        ))
    }

    fn set_tcp_nodelay(&self, _fd: i32) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "network operations are not supported on this target"
        ))
    }
}

struct UnsupportedSync;

impl SysSync for UnsupportedSync {
    fn eventfd(&self) -> RS<i32> {
        Err(m_error!(
            EC::NotImplemented,
            "eventfd is not supported on this target"
        ))
    }

    fn notify_eventfd(&self, _fd: i32) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "eventfd is not supported on this target"
        ))
    }

    fn read_eventfd(&self, _fd: i32) -> RS<u64> {
        Err(m_error!(
            EC::NotImplemented,
            "eventfd is not supported on this target"
        ))
    }

    fn close_fd(&self, _fd: i32) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            "eventfd is not supported on this target"
        ))
    }
}
