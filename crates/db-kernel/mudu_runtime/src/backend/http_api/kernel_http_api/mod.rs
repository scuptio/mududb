mod apps;
mod backup;
mod repl;
mod topology;

use super::{
    AsyncKernelInvokeClientFactory, HttpApi, KernelInvokeClientFactory, PartitionRouteRequest,
    PartitionRouteResponse, ReplFollowRequest, ReplMembershipRequest, ReplMembershipResponse,
    ReplPromoteRequest, ReplRoleResponse, ServerReplicationResponse, ServerTopology,
};
use crate::backend::app_mgr::AppMgr;
use crate::backend::mudud_cfg::MuduDBCfg;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::conn::mudu_conn_async::{set_default_remote_addr, set_default_remote_worker_id};
use ::meta::meta_mgr_factory::MetaMgrFactory;
use ::worker::worker_registry::WorkerRegistry;
use ::xengine::partition_router::{DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID, PartitionRouter};
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde_json::Value;
use std::sync::Arc;

/// [`HttpApi`] implementation fronting the mududb kernel server: procedure
/// invocations are relayed to the kernel over TCP, while topology, partition
/// routing, replication admin, and backup endpoints are served from the
/// worker registry, meta manager, and replication status.
pub struct KernelHttpApi {
    app_mgr: Arc<dyn AppMgr>,
    tcp_addr: String,
    tcp_multi_port: bool,
    tcp_base_listen_port: u16,
    worker_registry: Arc<WorkerRegistry>,
    meta_mgr: Arc<dyn MetaMgr>,
    partition_router: PartitionRouter,
    client_factory: Arc<dyn AsyncKernelInvokeClientFactory>,
    raft_node_id: Option<u64>,
    http_listen_port: u16,
    server_instance_id: Option<mudu::common::id::OID>,
    backup_dir: Option<String>,
}

impl KernelHttpApi {
    /// Creates the API from the server configuration, opening the meta
    /// manager on the configured database path.
    pub async fn new(
        app_mgr: Arc<dyn AppMgr>,
        cfg: &MuduDBCfg,
        worker_registry: Arc<WorkerRegistry>,
        server_instance_id: mudu::common::id::OID,
    ) -> RS<Self> {
        let meta_mgr = MetaMgrFactory::create(cfg.db_path.clone())
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "create http meta manager failed", e))?;
        let raft_node_id = cfg
            .replication
            .as_ref()
            .map(|replication| replication.node_id);
        let backup_dir = cfg
            .backup
            .as_ref()
            .and_then(|backup| backup.backup_dir.clone());
        Ok(Self::with_client_factory(
            app_mgr,
            format!("{}:{}", cfg.listen_ip, cfg.tcp_listen_port),
            cfg.tcp_multi_port,
            cfg.tcp_listen_port,
            worker_registry,
            meta_mgr,
            Arc::new(KernelInvokeClientFactory),
        )
        .with_raft_node_id(raft_node_id)
        .with_http_listen_port(cfg.http_listen_port)
        .with_server_instance_id(Some(server_instance_id))
        .with_backup_dir(backup_dir))
    }

    /// Creates the API with an explicit invoke-client factory, installing
    /// `tcp_addr` and the registry's default worker as the process-wide
    /// default remote endpoint.
    pub fn with_client_factory(
        app_mgr: Arc<dyn AppMgr>,
        tcp_addr: String,
        tcp_multi_port: bool,
        tcp_base_listen_port: u16,
        worker_registry: Arc<WorkerRegistry>,
        meta_mgr: Arc<dyn MetaMgr>,
        client_factory: Arc<dyn AsyncKernelInvokeClientFactory>,
    ) -> Self {
        set_default_remote_addr(Some(tcp_addr.clone()));
        set_default_remote_worker_id(worker_registry.default_global_worker_id());
        Self {
            app_mgr,
            tcp_addr,
            tcp_multi_port,
            tcp_base_listen_port,
            worker_registry,
            partition_router: PartitionRouter::new(meta_mgr.clone()),
            meta_mgr,
            client_factory,
            raft_node_id: None,
            http_listen_port: 8300,
            server_instance_id: None,
            backup_dir: None,
        }
    }

    /// Attaches the cluster node id used to resolve this server's raft
    /// share for the topology endpoint.
    pub fn with_raft_node_id(mut self, raft_node_id: Option<u64>) -> Self {
        self.raft_node_id = raft_node_id;
        self
    }

    /// Attaches this server's management HTTP port (leader-hint redirect
    /// target of the membership endpoint, symmetric deployment).
    pub fn with_http_listen_port(mut self, http_listen_port: u16) -> Self {
        self.http_listen_port = http_listen_port;
        self
    }

    /// Attaches the kernel server instance id used to resolve the live
    /// workers for the base-backup endpoint.
    pub fn with_server_instance_id(
        mut self,
        server_instance_id: Option<mudu::common::id::OID>,
    ) -> Self {
        self.server_instance_id = server_instance_id;
        self
    }

    /// Attaches the base-backup output directory from `[backup] backup_dir`.
    pub fn with_backup_dir(mut self, backup_dir: Option<String>) -> Self {
        self.backup_dir = backup_dir;
        self
    }

    async fn resolve_partition_worker(
        &self,
        partition_id: mudu::common::id::OID,
    ) -> RS<mudu::common::id::OID> {
        if let Some(worker_id) = self.meta_mgr.get_partition_worker(partition_id).await? {
            return Ok(worker_id);
        }
        if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
            return self
                .worker_registry
                .default_global_worker_id()
                .ok_or_else(|| {
                    mudu::mudu_error!(
                        mudu::error::ErrorCode::EntityNotFound,
                        "worker registry has no default global worker"
                    )
                });
        }
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("no worker placement for partition {}", partition_id)
        ))
    }
}

/// [`HttpApi`] entry points. Every method delegates to a same-named inherent
/// method hosted by the domain continuation `impl` blocks in the sibling
/// modules (`apps`, `topology`, `backup`, `repl`); the associated-function
/// form `KernelHttpApi::m(self, ...)` names the inherent method directly, so
/// these delegations fail to compile rather than recurse if a domain module
/// drops a method.
#[async_trait(?Send)]
impl HttpApi for KernelHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        KernelHttpApi::list_apps(self).await
    }

    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>> {
        KernelHttpApi::list_procedures(self, app_name).await
    }

    async fn procedure_detail(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        KernelHttpApi::procedure_detail(self, app_name, mod_name, proc_name).await
    }

    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()> {
        KernelHttpApi::install_mpk(self, mpk_binary).await
    }

    async fn invoke_json(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
        body: String,
    ) -> RS<Value> {
        KernelHttpApi::invoke_json(self, app_name, mod_name, proc_name, body).await
    }

    async fn route_partition(&self, request: PartitionRouteRequest) -> RS<PartitionRouteResponse> {
        KernelHttpApi::route_partition(self, request).await
    }

    async fn server_topology(&self) -> RS<ServerTopology> {
        KernelHttpApi::server_topology(self).await
    }

    async fn server_replication(&self) -> RS<ServerReplicationResponse> {
        KernelHttpApi::server_replication(self).await
    }

    async fn uninstall_app(&self, app_name: &str) -> RS<()> {
        KernelHttpApi::uninstall_app(self, app_name).await
    }

    async fn repl_membership(
        &self,
        request: ReplMembershipRequest,
    ) -> Result<ReplMembershipResponse, ::repl::raft_service::ReplMembershipError> {
        KernelHttpApi::repl_membership(self, request).await
    }

    async fn repl_promote(
        &self,
        request: ReplPromoteRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        KernelHttpApi::repl_promote(self, request).await
    }

    async fn repl_follow(
        &self,
        request: ReplFollowRequest,
    ) -> Result<ReplRoleResponse, ::repl::stream_admin::StreamAdminError> {
        KernelHttpApi::repl_follow(self, request).await
    }

    async fn backup(&self, label: &str) -> RS<Value> {
        KernelHttpApi::backup(self, label).await
    }

    async fn backup_list(&self) -> RS<Vec<String>> {
        KernelHttpApi::backup_list(self).await
    }

    async fn backup_verify(&self, label: &str) -> RS<Value> {
        KernelHttpApi::backup_verify(self, label).await
    }

    async fn backup_delete(&self, label: &str) -> RS<bool> {
        KernelHttpApi::backup_delete(self, label).await
    }

    async fn dump_logical(&self, label: &str, dir: &str) -> RS<Value> {
        KernelHttpApi::dump_logical(self, label, dir).await
    }

    async fn restore_logical(&self, label: &str, dir: &str, drop_existing: bool) -> RS<Value> {
        KernelHttpApi::restore_logical(self, label, dir, drop_existing).await
    }
}
