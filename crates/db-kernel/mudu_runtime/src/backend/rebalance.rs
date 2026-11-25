//! The rebalance scheduler: runs on the MetaGroup leader,
//! places never-placed partitions, and moves partitions from the fullest
//! node group to the emptiest — one move per round, and never while a
//! migration is in flight (the conservative default policy; the mechanism
//! is the `MigrationDriver`).

use std::sync::Arc;
use std::time::Duration;

use catalog::contract::cluster_router::ClusterRouter;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tracing::{debug, warn};

use crate::backend::http_api::PartitionMoveRequest;
use crate::backend::migration_driver::MigrationDriver;

/// Spawns the rebalance scheduler thread (cluster mode only).
pub(crate) fn spawn_rebalance_scheduler(
    server_instance_id: OID,
    db_path: String,
    interval: Duration,
    stop: mudu_utils::notifier::NotifyWait,
) -> RS<()> {
    mudu_sys::task::sync::spawn_thread_named("rebalance-scheduler".to_string(), move || {
        rebalance_loop(server_instance_id, db_path, interval, stop)
    })?;
    Ok(())
}

/// The scheduler loop: one round per interval, on the MetaGroup leader
/// only.
fn rebalance_loop(
    server_instance_id: OID,
    db_path: String,
    interval: Duration,
    stop: mudu_utils::notifier::NotifyWait,
) -> RS<()> {
    let runtime = mudu_utils::task_async::CurrentThreadTaskRuntime::new()
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "build rebalance runtime error", e))?;
    let mut meta_mgr: Option<Arc<dyn catalog::contract::meta_mgr::MetaMgr>> = None;
    while !stop.is_notified() {
        // Interruptible sleep in small increments.
        let mut waited = Duration::ZERO;
        while waited < interval && !stop.is_notified() {
            let step = Duration::from_millis(100).min(interval - waited);
            mudu_sys::task::sync::sleep_blocking(step);
            waited += step;
        }
        if stop.is_notified() {
            break;
        }
        if ::server::server::cluster_directory::is_meta_leader(server_instance_id) != Some(true) {
            continue;
        }
        if meta_mgr.is_none() {
            let db_path = db_path.clone();
            let created = runtime.block_on(async move {
                let mgr = ::meta::meta_mgr_factory::MetaMgrFactory::create(db_path).await?;
                mgr.initialize().await?;
                Ok::<_, mudu::error::MuduError>(mgr)
            });
            match created {
                Ok(mgr) => meta_mgr = Some(mgr),
                Err(err) => {
                    warn!("rebalance: meta manager create failed: {err}");
                    continue;
                }
            }
        }
        let Some(mgr) = meta_mgr.clone() else {
            continue;
        };
        let round = async move {
            let driver = match MigrationDriver::for_server(server_instance_id, mgr) {
                Ok(Some(driver)) => driver,
                Ok(None) => return Ok(()),
                Err(err) => {
                    warn!("rebalance: driver init failed: {err}");
                    return Ok(());
                }
            };
            rebalance_round(&driver).await
        };
        if let Err(err) = runtime.block_on(round) {
            warn!("rebalance round failed: {err}");
        }
    }
    Ok(())
}

/// One rebalance round: place never-placed partitions first; then, when a
/// group's partition count exceeds the emptiest group's by two or more,
/// move one partition over. One action per round (conservative default).
async fn rebalance_round(driver: &MigrationDriver) -> RS<()> {
    let directory = driver.directory().clone();
    if directory.any_migrating() {
        return Ok(());
    }
    let topology = directory.topology().clone();
    for rule in driver.meta_mgr().list_partition_rules().await? {
        // Effective placement of every partition of the rule.
        let mut counts: std::collections::BTreeMap<u64, Vec<String>> = topology
            .group
            .iter()
            .map(|group| (group.group_id, Vec::new()))
            .collect();
        let mut unplaced = Vec::new();
        for def in &rule.partitions {
            match directory.placement(&rule.name, &def.name) {
                Some((group_id, _)) => {
                    counts.entry(group_id).or_default().push(def.name.clone());
                }
                None => unplaced.push(def.name.clone()),
            }
        }
        // Never-placed partitions (created at runtime): place them
        // round-robin, starting at the emptiest group.
        for name in unplaced {
            let Some((target_group, _)) =
                counts.iter().min_by_key(|(_, partitions)| partitions.len())
            else {
                continue;
            };
            let target_group = *target_group;
            let Some(group) = topology.group(target_group) else {
                continue;
            };
            let worker_index = counts[&target_group].len() as u32 % group.worker_count;
            debug!(
                rule = %rule.name,
                partition = %name,
                target_group,
                worker_index,
                "rebalance: placing a never-placed partition"
            );
            driver
                .place_partition(rule.name.clone(), name, target_group, worker_index)
                .await?;
            return Ok(());
        }
        // Imbalance: move one partition from the fullest group to the
        // emptiest.
        let Some((fullest_group, fullest)) = counts.iter().max_by_key(|(_, p)| p.len()) else {
            continue;
        };
        let Some((emptiest_group, emptiest)) = counts.iter().min_by_key(|(_, p)| p.len()) else {
            continue;
        };
        if fullest.len() < emptiest.len() + 2 {
            continue;
        }
        let (fullest_group, fullest) = (*fullest_group, fullest.clone());
        let (emptiest_group, emptiest) = (*emptiest_group, emptiest.clone());
        let Some(partition) = fullest.first().cloned() else {
            continue;
        };
        let Some(group) = topology.group(emptiest_group) else {
            continue;
        };
        let worker_index = emptiest.len() as u32 % group.worker_count;
        debug!(
            rule = %rule.name,
            partition = %partition,
            from = fullest_group,
            to = emptiest_group,
            "rebalance: moving a partition"
        );
        driver
            .move_partition(PartitionMoveRequest {
                rule: rule.name.clone(),
                partition,
                group_id: emptiest_group,
                worker_index: Some(worker_index),
            })
            .await?;
        return Ok(());
    }
    Ok(())
}
