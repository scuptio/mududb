use super::*;
use mudu_sys::contract::file_options::FileOptions;

use crate::fs_fd_table::FdEntryParams;
use ::meta::fs_object::{encode_fs_object_row, FS_OBJECT_STATE_PENDING, FS_OBJECT_STATE_SEALED};
use ::meta::fs_type_catalog::fs_storage_root;
use ::xengine::tx_mgr::PhysicalRelationId;

impl FsService {
    /// Open an fs object (or an entry of a DIRECTORY object) and return a
    /// session-local fd.
    ///
    /// `flags` uses libc `O_*` values: the access mode selects read
    /// (`O_RDONLY`), write (`O_WRONLY`) or read-write (`O_RDWR`); `O_CREAT`,
    /// `O_EXCL`, `O_TRUNC` and `O_APPEND` are rejected. Write opens require a
    /// PENDING object created by the current transaction and anchor to a new
    /// private generation; a DIRECTORY object already sealed by the current
    /// transaction accepts further entries into that same generation; read
    /// opens anchor to the visible SEALED generation.
    pub async fn fs_open(&self, session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
        let tx_mgr = self.session_tx(session_id)?;
        let access = flags & 3;
        if access == 3 || (flags & UNSUPPORTED_OPEN_FLAGS) != 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("unsupported fs open flags {flags:#o}")
            ));
        }
        let resolved = self.resolve(&tx_mgr, oid, path).await?;
        if resolved.kind == FsTypeKind::Directory && resolved.entry_rel.is_empty() {
            return Err(mudu_error!(
                ErrorCode::IsADirectory,
                "an fs object directory root cannot be opened as a file"
            ));
        }
        let entry = if access == ACCESS_READ {
            self.open_read(&resolved, oid).await?
        } else {
            self.open_write(&resolved, oid, access != 1).await?
        };
        self.fds.insert(session_id, Arc::new(entry))
    }

    /// Close an fd. Closing a write fd fsyncs the content file and stages the
    /// SEALED `_fs_object` row (new generation and final length) into the
    /// current transaction.
    pub async fn fs_close(&self, session_id: OID, fd: u32) -> RS<()> {
        let tx_mgr = self.session_tx(session_id)?;
        let entry = self.fds.remove(session_id, fd)?;
        if entry.write {
            entry.file.fsync().await?;
            let length = entry.file.file_len().await?;
            let key = encode_fs_object_key(entry.oid)?;
            let value = encode_fs_object_row(&FsObjectRow {
                fs_id: entry.fs_id,
                kind: entry.kind,
                generation: entry.generation,
                length,
                state: FS_OBJECT_STATE_SEALED,
            })?;
            tx_mgr.put_relation(
                PhysicalRelationId {
                    table_id: FS_OBJECT_TABLE_ID,
                    partition_id: entry.partition_id,
                },
                key,
                value,
            );
        }
        entry.file.close().await?;
        Ok(())
    }

    /// Open the anchored SEALED generation for reading.
    async fn open_read(&self, resolved: &ResolvedFsObject, oid: OID) -> RS<FdEntry> {
        if resolved.state != FS_OBJECT_STATE_SEALED {
            return Err(mudu_error!(
                ErrorCode::NotFound,
                format!("fs object {oid:032x} has no sealed generation")
            ));
        }
        if self.is_directory(&resolved.content_path).await {
            return Err(entry_is_directory(&resolved.entry_rel));
        }
        let file = self
            .fs
            .open(&resolved.content_path, FileOptions::read_only())
            .await?;
        // The fd binds to the entry file, so the immutable on-disk length is
        // the clamp anchor: for FILE objects it equals the cataloged row
        // length, for DIRECTORY entries the row length is not per-entry.
        let length = file.file_len().await?;
        Ok(FdEntry::new(FdEntryParams {
            oid,
            fs_id: resolved.fs_id,
            kind: resolved.kind.as_u32(),
            partition_id: resolved.partition_id,
            generation: resolved.generation,
            entry_rel: resolved.entry_rel.clone(),
            file,
            length,
            read: true,
            write: false,
        }))
    }

    /// Open a new private generation (`resolved.generation + 1`) for writing.
    ///
    /// A DIRECTORY row already sealed by the current transaction is the
    /// exception: every entry is a host file of its own, so sealing one
    /// entry must not lock the remaining entries out — further write opens
    /// anchor to that same private generation instead of the next one.
    async fn open_write(&self, resolved: &ResolvedFsObject, oid: OID, read: bool) -> RS<FdEntry> {
        if !resolved.staged {
            return Err(mudu_error!(
                ErrorCode::PermissionDenied,
                format!("fs object {oid:032x} is not created by the current transaction")
            ));
        }
        let generation = match resolved.state {
            FS_OBJECT_STATE_PENDING => resolved.generation + 1,
            FS_OBJECT_STATE_SEALED if resolved.kind == FsTypeKind::Directory => resolved.generation,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::PermissionDenied,
                    format!("fs object {oid:032x} generation is sealed")
                ));
            }
        };
        let root = fs_storage_root(&self.data_dir, resolved.fs_id);
        let content_path = object_content_path(&root, oid, generation, &resolved.entry_rel);
        if self.is_directory(&content_path).await {
            return Err(entry_is_directory(&resolved.entry_rel));
        }
        if let Some(parent) = content_path.parent() {
            self.fs.create_dir_all(parent).await?;
        }
        let options = if read {
            FileOptions::read_write_create()
        } else {
            FileOptions {
                write: true,
                create: true,
                ..Default::default()
            }
        };
        let file = self.fs.open(&content_path, options).await?;
        Ok(FdEntry::new(FdEntryParams {
            oid,
            fs_id: resolved.fs_id,
            kind: resolved.kind.as_u32(),
            partition_id: resolved.partition_id,
            generation,
            entry_rel: resolved.entry_rel.clone(),
            file,
            length: 0,
            read,
            write: true,
        }))
    }
}
