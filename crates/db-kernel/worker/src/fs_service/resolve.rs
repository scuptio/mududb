use super::*;
use ::meta::fs_type_catalog::fs_storage_root;
use ::xengine::tx_mgr::TxMgr;

impl FsService {
    /// Resolve the session's active transaction; fs syscalls require one.
    pub(super) fn session_tx(&self, session_id: OID) -> RS<Arc<dyn TxMgr>> {
        self.sessions
            .with_session_tx(session_id, Ok)?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "fs syscalls require an active transaction"
                )
            })
    }

    /// Resolve `(oid, path)` to the visible `_fs_object` row, the fs type and
    /// the on-disk content path.
    pub(super) async fn resolve(
        &self,
        tx_mgr: &Arc<dyn TxMgr>,
        oid: OID,
        path: &str,
    ) -> RS<ResolvedFsObject> {
        let lookup = self
            .object_store
            .read_fs_object(tx_mgr, oid)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::NotFound,
                    format!("fs object {oid:032x} does not exist")
                )
            })?;
        let row = lookup.row;
        let fs_type = self
            .meta_mgr
            .get_fs_type_by_id(row.fs_id)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "fs type id {} referenced by an fs object is not registered",
                        row.fs_id
                    )
                )
            })?;
        let kind = fs_type.kind();
        let entry_rel = normalize_entry_path(path)?;
        if kind != FsTypeKind::Directory && !entry_rel.is_empty() {
            return Err(mudu_error!(
                ErrorCode::NotADirectory,
                "a path inside the object is only valid for DIRECTORY fs objects"
            ));
        }
        let root = fs_storage_root(&self.data_dir, row.fs_id);
        let content_path = object_content_path(&root, oid, row.generation, &entry_rel);
        Ok(ResolvedFsObject {
            partition_id: lookup.partition_id,
            fs_id: row.fs_id,
            kind,
            generation: row.generation,
            state: row.state,
            staged: lookup.staged,
            entry_rel,
            content_path,
        })
    }
}
