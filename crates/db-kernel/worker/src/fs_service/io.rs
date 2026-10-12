use super::*;
use ::meta::fs_object::{FS_OBJECT_STATE_PENDING, FS_OBJECT_STATE_SEALED};
use mudu_binding::universal::uni_oid::UniOid;

impl FsService {
    /// Read up to `len` bytes at the fd cursor, advancing the cursor by the
    /// number of bytes read. A short buffer signals EOF.
    pub async fn fs_read(&self, session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
        self.session_tx(session_id)?;
        check_io_len(len)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.read {
            return Err(bad_access(fd, "reading"));
        }
        let cursor = entry.cursor();
        let data = read_clamped(entry.as_ref(), cursor, len).await?;
        entry.set_cursor(cursor + data.len() as u64);
        Ok(data)
    }

    /// Write `data` at the fd cursor, advancing the cursor and growing the
    /// content length. Returns the number of bytes written.
    pub async fn fs_write(&self, session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
        self.session_tx(session_id)?;
        check_io_len(data.len() as u32)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.write {
            return Err(bad_access(fd, "writing"));
        }
        let cursor = entry.cursor();
        entry.file.write_all_at(cursor, data).await?;
        let end = cursor
            .checked_add(data.len() as u64)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "fs write range overflows"))?;
        entry.set_cursor(end);
        entry.grow_length(end);
        Ok(data.len() as u32)
    }

    /// Read up to `len` bytes at `offset` without moving the cursor. An
    /// offset at or past EOF yields an empty buffer.
    pub async fn fs_pread(&self, session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
        self.session_tx(session_id)?;
        check_io_len(len)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.read {
            return Err(bad_access(fd, "reading"));
        }
        read_clamped(entry.as_ref(), offset, len).await
    }

    /// Write `data` at `offset` without moving the cursor. Offsets past EOF
    /// produce a sparse hole.
    pub async fn fs_pwrite(&self, session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
        self.session_tx(session_id)?;
        check_io_len(data.len() as u32)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.write {
            return Err(bad_access(fd, "writing"));
        }
        entry.file.write_all_at(offset, data).await?;
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "fs write range overflows"))?;
        entry.grow_length(end);
        Ok(())
    }

    /// Move the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new
    /// cursor. Pure in-memory operation: no IO is performed.
    pub fn fs_lseek(&self, session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
        self.session_tx(session_id)?;
        let entry = self.fds.get(session_id, fd)?;
        let base = match whence {
            WHENCE_SET => 0,
            WHENCE_CUR => entry.cursor() as i128,
            WHENCE_END => entry.length() as i128,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("invalid fs lseek whence {whence}")
                ));
            }
        };
        let new_cursor = base + offset as i128;
        if new_cursor < 0 || new_cursor > u64::MAX as i128 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("fs lseek to {new_cursor} is out of range")
            ));
        }
        let new_cursor = new_cursor as u64;
        entry.set_cursor(new_cursor);
        Ok(new_cursor)
    }

    /// Return the stat record of an open fd (anchored generation, entry path,
    /// current length, and the state of the anchored row).
    pub fn fs_fstat(&self, session_id: OID, fd: u32) -> RS<UniFsStat> {
        self.session_tx(session_id)?;
        let entry = self.fds.get(session_id, fd)?;
        Ok(UniFsStat {
            oid: UniOid::from_oid(entry.oid),
            generation: entry.generation,
            entry: entry.entry_rel.clone(),
            length: entry.length(),
            state: if entry.write {
                FS_OBJECT_STATE_PENDING
            } else {
                FS_OBJECT_STATE_SEALED
            },
        })
    }

    /// Flush a write fd's private generation to durable storage.
    pub async fn fs_fsync(&self, session_id: OID, fd: u32) -> RS<()> {
        self.session_tx(session_id)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.write {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("fs fd {fd} is not open for writing")
            ));
        }
        entry.file.fsync().await
    }
}
