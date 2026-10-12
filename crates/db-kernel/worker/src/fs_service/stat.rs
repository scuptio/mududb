use super::*;
use std::collections::BTreeMap;

use ::meta::fs_type_catalog::fs_storage_root;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_oid::UniOid;

impl FsService {
    /// Stat an fs object or entry without opening an fd. The DIRECTORY object
    /// root is virtual — no host path exists for it — and reports length 0;
    /// an entry stats its `{root}/{oidhex}.{gen}.{entry}` host path.
    pub async fn fs_stat(&self, session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
        let tx_mgr = self.session_tx(session_id)?;
        let resolved = self.resolve(&tx_mgr, oid, path).await?;
        let is_dir = (resolved.kind == FsTypeKind::Directory && resolved.entry_rel.is_empty())
            || self.is_directory(&resolved.content_path).await;
        let length = if is_dir {
            0
        } else {
            self.fs.metadata_len(&resolved.content_path).await?
        };
        Ok(UniFsStat {
            oid: UniOid::from_oid(oid),
            generation: resolved.generation,
            entry: resolved.entry_rel,
            length,
            state: resolved.state,
        })
    }

    /// List the entries of a DIRECTORY fs object directory.
    ///
    /// The object root (`path` empty) is virtual: its entries are the fs root
    /// children matching the generation prefix rule, each reported under the
    /// first path segment of the remainder after the prefix. A sub-path names
    /// the real host directory `{root}/{oidhex}.{gen}.{path}` and is listed
    /// directly.
    pub async fn fs_readdir(&self, session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
        let tx_mgr = self.session_tx(session_id)?;
        let resolved = self.resolve(&tx_mgr, oid, path).await?;
        if resolved.kind != FsTypeKind::Directory {
            return Err(mudu_error!(
                ErrorCode::NotADirectory,
                "fs_readdir requires a DIRECTORY fs object"
            ));
        }
        let mut entries = if resolved.entry_rel.is_empty() {
            self.readdir_object_root(&resolved, oid).await?
        } else {
            self.readdir_host_dir(&resolved.content_path).await?
        };
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    /// Probe whether `path` is a directory by attempting to list it.
    pub(super) async fn is_directory(&self, path: &Path) -> bool {
        self.fs.read_dir(path).await.is_ok()
    }

    /// List the root entries of a DIRECTORY object generation by scanning the
    /// fs storage root for names matching the generation prefix rule.
    async fn readdir_object_root(
        &self,
        resolved: &ResolvedFsObject,
        oid: OID,
    ) -> RS<Vec<UniFsDirent>> {
        let root = fs_storage_root(&self.data_dir, resolved.fs_id);
        // A missing fs root means the generation has no entries yet.
        if !self.fs.path_exists(&root).await? {
            return Ok(Vec::new());
        }
        let dotted_prefix = format!("{}.", object_prefix(oid, resolved.generation));
        let mut entries: BTreeMap<String, UniFsDirent> = BTreeMap::new();
        for path in self.fs.read_dir(&root).await? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // Prefix-match rule: the exact `{oidhex}.{gen}` name carries no
            // entry (it is the FILE-object content path form), so only the
            // dotted prefix is stripped here.
            let Some(remainder) = name.strip_prefix(&dotted_prefix) else {
                continue;
            };
            // Only the first path segment names a root entry; anything deeper
            // belongs to that entry's subtree, reported as a directory.
            let Some(first) = remainder
                .split('/')
                .next()
                .filter(|first| !first.is_empty())
            else {
                continue;
            };
            let entry = entries.entry(first.to_string()).or_insert(UniFsDirent {
                name: first.to_string(),
                is_dir: true,
                length: 0,
            });
            if !remainder.contains('/') {
                *entry = self.dir_ent(first, &path).await?;
            }
        }
        Ok(entries.into_values().collect())
    }

    /// List a real host directory: each child becomes one entry.
    async fn readdir_host_dir(&self, dir: &Path) -> RS<Vec<UniFsDirent>> {
        let paths = self.fs.read_dir(dir).await?;
        let mut entries = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            entries.push(self.dir_ent(name, &path).await?);
        }
        Ok(entries)
    }

    /// Build the [`UniFsDirent`] of a host path: directories report length 0,
    /// files their content length.
    async fn dir_ent(&self, name: &str, path: &Path) -> RS<UniFsDirent> {
        let is_dir = self.is_directory(path).await;
        let length = if is_dir {
            0
        } else {
            self.fs.metadata_len(path).await?
        };
        Ok(UniFsDirent {
            name: name.to_string(),
            is_dir,
            length,
        })
    }
}
