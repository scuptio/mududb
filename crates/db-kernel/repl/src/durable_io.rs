//! Shared async durable directory creation and atomic file publication.
use mudu::common::result::RS;
use mudu_sys::contract::{async_fs::AsyncFs, file_options::FileOptions};
use std::path::Path;

/// Creates a directory and durably publishes every newly created parent entry.
pub async fn ensure_dir(fs: &dyn AsyncFs, dir: &Path) -> RS<()> {
    let mut missing = Vec::new();
    let mut cursor = dir;
    while !fs.path_exists(cursor).await? {
        missing.push(cursor.to_path_buf());
        cursor = cursor
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
    }
    fs.create_dir_all(dir).await?;
    for path in missing.iter().rev() {
        fs.sync_dir(path).await?;
        fs.sync_dir(
            path.parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new(".")),
        )
        .await?;
    }
    Ok(())
}

/// Publishes bytes by syncing a temporary file, renaming it and syncing its parent.
/// The caller must create the parent first and serialize competing publications.
pub async fn publish(fs: &dyn AsyncFs, path: &Path, bytes: &[u8]) -> RS<()> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let temporary = parent.join(format!(".publish-{}.tmp", mudu_sys::random::uuid_v4()));
    let file = fs
        .open(
            &temporary,
            FileOptions {
                read: true,
                write: true,
                create_new: true,
                ..FileOptions::default()
            },
        )
        .await?;
    file.write_all_at(0, bytes).await?;
    file.fsync().await?;
    file.close().await?;
    fs.rename(&temporary, path).await?;
    fs.sync_dir(parent).await
}

pub(crate) async fn truncate(fs: &dyn AsyncFs, path: &Path, len: u64) -> RS<()> {
    let file = fs
        .open(
            path,
            FileOptions {
                read: true,
                write: true,
                ..FileOptions::default()
            },
        )
        .await?;
    file.set_len(len).await?;
    file.fsync().await
}
