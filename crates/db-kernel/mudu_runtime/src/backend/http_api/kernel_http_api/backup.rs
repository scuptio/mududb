use super::KernelHttpApi;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use serde_json::Value;

/// Base-backup and logical dump/restore endpoints. The backup routines block
/// on kernel I/O, so each endpoint bridges onto its own thread and relays the
/// outcome back over a oneshot channel.
impl KernelHttpApi {
    pub(super) async fn backup(&self, label: &str) -> RS<Value> {
        let backup_dir = self.backup_dir.clone().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "backup is not configured: [backup] backup_dir is missing"
            )
        })?;
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "server instance id is not available"
            )
        })?;
        let label = label.to_string();
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("base-backup".to_string(), move || {
            let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                ::backup::backup::run_server_base_backup(server_instance_id, &backup_dir, &label)
                    .await
            })
            .and_then(|label| label)
            .and_then(|label| {
                serde_json::to_value(label)
                    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode backup label error", e))
            });
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn base-backup thread error", e))?;
        rx.await
            .map_err(|e| mudu_error!(ErrorCode::ChannelClosed, "base-backup thread dropped", e))?
    }

    pub(super) async fn backup_list(&self) -> RS<Vec<String>> {
        let backup_dir = self.backup_dir.clone().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "backup is not configured: [backup] backup_dir is missing"
            )
        })?;
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("backup-list".to_string(), move || {
            let result = ::backup::backup::list_backup_labels(&backup_dir);
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn backup-list thread error", e))?;
        rx.await
            .map_err(|e| mudu_error!(ErrorCode::ChannelClosed, "backup-list thread dropped", e))?
    }

    pub(super) async fn backup_verify(&self, label: &str) -> RS<Value> {
        let backup_dir = self.backup_dir.clone().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "backup is not configured: [backup] backup_dir is missing"
            )
        })?;
        let label = label.to_string();
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("backup-verify".to_string(), move || {
            let result =
                ::backup::backup::verify_backup(&backup_dir, &label).and_then(|verified| {
                    verified.map_or_else(
                        || {
                            Err(mudu_error!(
                                ErrorCode::CorruptedData,
                                format!("backup {label} is missing or corrupt")
                            ))
                        },
                        |label| {
                            serde_json::to_value(label).map_err(|e| {
                                mudu_error!(ErrorCode::Encode, "encode backup label error", e)
                            })
                        },
                    )
                });
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn backup-verify thread error", e))?;
        rx.await
            .map_err(|e| mudu_error!(ErrorCode::ChannelClosed, "backup-verify thread dropped", e))?
    }

    pub(super) async fn backup_delete(&self, label: &str) -> RS<bool> {
        let backup_dir = self.backup_dir.clone().ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "backup is not configured: [backup] backup_dir is missing"
            )
        })?;
        let label = label.to_string();
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("backup-delete".to_string(), move || {
            let result = ::backup::backup::delete_backup(&backup_dir, &label);
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn backup-delete thread error", e))?;
        rx.await
            .map_err(|e| mudu_error!(ErrorCode::ChannelClosed, "backup-delete thread dropped", e))?
    }

    pub(super) async fn dump_logical(&self, label: &str, dir: &str) -> RS<Value> {
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "server instance id is not available"
            )
        })?;
        let label = label.to_string();
        let dir = dir.to_string();
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("logical-dump".to_string(), move || {
            let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                ::backup::logical_backup::run_logical_dump(server_instance_id, &dir, &label).await
            })
            .and_then(|report| report)
            .and_then(|report| {
                serde_json::to_value(report)
                    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode dump report error", e))
            });
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn logical-dump thread error", e))?;
        rx.await
            .map_err(|e| mudu_error!(ErrorCode::ChannelClosed, "logical-dump thread dropped", e))?
    }

    pub(super) async fn restore_logical(
        &self,
        label: &str,
        dir: &str,
        drop_existing: bool,
    ) -> RS<Value> {
        let server_instance_id = self.server_instance_id.ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidState,
                "server instance id is not available"
            )
        })?;
        let label = label.to_string();
        let dir = dir.to_string();
        let (tx, rx) = mudu_sys::tokio::sync::oneshot::channel();
        mudu_sys::task::sync::spawn_thread_named("logical-restore".to_string(), move || {
            let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                ::backup::logical_backup::run_logical_restore(
                    server_instance_id,
                    &dir,
                    &label,
                    drop_existing,
                )
                .await
            })
            .and_then(|report| report)
            .and_then(|report| {
                serde_json::to_value(report)
                    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode restore report error", e))
            });
            let _ = tx.send(result);
            Ok::<_, mudu::error::MuduError>(())
        })
        .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn logical-restore thread error", e))?;
        rx.await.map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "logical-restore thread dropped",
                e
            )
        })?
    }
}
