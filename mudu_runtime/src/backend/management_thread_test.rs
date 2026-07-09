#![allow(clippy::unwrap_used)]

use super::{MuduDBCfg, spawn_management_thread};
use crate::backend::app_mgr::AppMgr;
use crate::backend::mudu_app_mgr::ListOption;
use crate::backend::mudud_cfg::{RoutingMode, ServerMode};
use crate::service::app_list::AppList;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_kernel::server::async_func_runtime::AsyncFuncInvoker;
use mudu_kernel::server::worker_local::WorkerLocalRef;
use mudu_kernel::server::worker_registry::WorkerRegistry;
use mudu_utils::notifier::notify_wait;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

struct MockAppMgr;

#[async_trait(?Send)]
impl AppMgr for MockAppMgr {
    async fn install(&self, _mpk_binary: Vec<u8>) -> RS<()> {
        Ok(())
    }

    async fn uninstall(&self, _app_name: Vec<u8>) -> RS<()> {
        Ok(())
    }

    async fn list(&self, _option: &ListOption) -> RS<AppList> {
        Ok(AppList { apps: vec![] })
    }

    async fn create_invoker(&self, _cfg: &MuduDBCfg) -> RS<Arc<dyn AsyncFuncInvoker>> {
        Ok(Arc::new(MockInvoker))
    }
}

struct MockInvoker;

#[async_trait]
impl AsyncFuncInvoker for MockInvoker {
    async fn invoke(
        &self,
        _session_id: OID,
        _procedure_name: &str,
        _procedure_parameters: Vec<u8>,
        _worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        Ok(Vec::new())
    }
}

fn temp_db_path(label: &str) -> String {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    mudu_sys::env_var::temp_dir()
        .join(format!("mudu-mgmt-{label}-{nanos}"))
        .to_str()
        .unwrap()
        .to_string()
}

fn test_cfg(listen_ip: &str, http_port: u16) -> MuduDBCfg {
    MuduDBCfg {
        listen_ip: listen_ip.to_string(),
        http_listen_port: http_port,
        db_path: temp_db_path("db"),
        server_mode: ServerMode::Legacy,
        routing_mode: RoutingMode::ConnectionId,
        enable_async: false,
        ..Default::default()
    }
}

// Miri requires every spawned thread to be joined, but the management thread is
// intentionally detached in production, so skip these tests under Miri.
#[cfg_attr(miri, ignore)]
#[test]
fn spawn_management_thread_rejects_invalid_address() {
    let cfg = test_cfg("not-a-valid-ip", 0);
    let (stop_tx, stop_rx) = notify_wait();
    let registry = Arc::new(WorkerRegistry::new(vec![]).unwrap());

    let err = spawn_management_thread(cfg, Arc::new(MockAppMgr), registry, stop_rx)
        .err()
        .unwrap();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Parse);

    let _ = stop_tx;
}

#[cfg_attr(miri, ignore)]
#[test]
fn spawn_management_thread_starts_and_stops() {
    let cfg = test_cfg("127.0.0.1", 0);
    let (stop_tx, stop_rx) = notify_wait();
    let registry = Arc::new(WorkerRegistry::new(vec![]).unwrap());

    spawn_management_thread(cfg, Arc::new(MockAppMgr), registry, stop_rx).unwrap();

    stop_tx.notify_all();
}
