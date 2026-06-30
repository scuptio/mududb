#![allow(missing_docs)]

use super::{
    AsyncKernelInvokeClientFactory, HttpApi, KernelInvokeClientFactory, PartitionRouteEntry,
    PartitionRouteRequest, PartitionRouteResponse, ServerTopology, WorkerTopology, find_app,
    parse_json_object_body, to_param,
};
use crate::backend::app_mgr::AppMgr;
use crate::backend::mududb_cfg::MuduDBCfg;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_kernel::contract::meta_mgr::MetaMgr;
use mudu_kernel::meta::meta_mgr_factory::MetaMgrFactory;
use mudu_kernel::mudu_conn::mudu_conn_async::{
    set_default_remote_addr, set_default_remote_worker_id,
};
use mudu_kernel::server::partition_router::{
    DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID, PartitionRouter,
};
use mudu_kernel::server::worker_registry::WorkerRegistry;
use serde_json::Value;
use std::ops::Bound;
use std::sync::Arc;

pub struct KernelHttpApi {
    app_mgr: Arc<dyn AppMgr>,
    tcp_addr: String,
    tcp_multi_port: bool,
    tcp_base_listen_port: u16,
    worker_registry: Arc<WorkerRegistry>,
    meta_mgr: Arc<dyn MetaMgr>,
    partition_router: PartitionRouter,
    client_factory: Arc<dyn AsyncKernelInvokeClientFactory>,
}

impl KernelHttpApi {
    pub async fn new(
        app_mgr: Arc<dyn AppMgr>,
        cfg: &MuduDBCfg,
        worker_registry: Arc<WorkerRegistry>,
    ) -> RS<Self> {
        let meta_mgr = MetaMgrFactory::create(cfg.db_path.clone())
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "create http meta manager failed", e))?;
        Ok(Self::with_client_factory(
            app_mgr,
            format!("{}:{}", cfg.listen_ip, cfg.tcp_listen_port),
            cfg.tcp_multi_port,
            cfg.tcp_listen_port,
            worker_registry,
            meta_mgr,
            Arc::new(KernelInvokeClientFactory),
        ))
    }

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
        }
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

#[async_trait(?Send)]
impl HttpApi for KernelHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        let list = self
            .app_mgr
            .list(&crate::backend::mudu_app_mgr::ListOption::default())
            .await?;
        Ok(list.apps.into_iter().map(|app| app.info.name).collect())
    }

    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>> {
        let app = find_app(self.app_mgr.as_ref(), app_name).await?;
        Ok(app
            .mod_proc_desc
            .modules()
            .iter()
            .flat_map(|(mod_name, procedures)| {
                procedures
                    .iter()
                    .map(move |proc_desc| format!("{}/{}", mod_name, proc_desc.proc_name()))
            })
            .collect())
    }

    async fn procedure_detail(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        let app = find_app(self.app_mgr.as_ref(), app_name).await?;
        let procedure = app
            .mod_proc_desc
            .modules()
            .get(mod_name)
            .and_then(|procedures| {
                procedures
                    .iter()
                    .find(|procedure| procedure.proc_name() == proc_name)
            })
            .cloned()
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::EntityNotFound,
                    format!("no such procedure {}/{}/{}", app_name, mod_name, proc_name)
                )
            })?;
        let param_json = procedure.default_param_json()?;
        let return_json = procedure.default_return_json()?;
        Ok((procedure, param_json, return_json))
    }

    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()> {
        self.app_mgr.install(mpk_binary).await
    }

    async fn server_topology(&self) -> RS<ServerTopology> {
        Ok(ServerTopology {
            worker_count: self.worker_registry.workers().len(),
            tcp_multi_port: self.tcp_multi_port,
            tcp_base_listen_port: self.tcp_base_listen_port,
            workers: self
                .worker_registry
                .workers()
                .iter()
                .map(|worker| WorkerTopology {
                    worker_index: worker.worker_index,
                    tcp_listen_port: if self.tcp_multi_port {
                        self.tcp_base_listen_port
                            .saturating_add(worker.worker_index as u16)
                    } else {
                        self.tcp_base_listen_port
                    },
                    worker_id: worker.worker_id,
                    partitions: worker.partition_ids.clone(),
                })
                .collect(),
        })
    }

    async fn uninstall_app(&self, app_name: &str) -> RS<()> {
        self.app_mgr.uninstall(app_name.as_bytes().to_vec()).await
    }

    async fn route_partition(&self, request: PartitionRouteRequest) -> RS<PartitionRouteResponse> {
        let rule = self
            .meta_mgr
            .get_partition_rule_by_name(&request.rule_name)
            .await?
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::EntityNotFound,
                    format!("no such partition rule {}", request.rule_name)
                )
            })?;

        let partition_ids = if let Some(key) = request.key {
            if request.start.is_some() || request.end.is_some() {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Parse,
                    "partition route request cannot specify both key and range"
                ));
            }
            vec![
                self.partition_router.route_rule_exact_partition(
                    &rule,
                    &key.into_iter()
                        .map(|value| value.into_bytes())
                        .collect::<Vec<_>>(),
                )?,
            ]
        } else {
            let start = match request.start {
                Some(values) => Bound::Included(
                    values
                        .into_iter()
                        .map(|value| value.into_bytes())
                        .collect::<Vec<_>>(),
                ),
                None => Bound::Unbounded,
            };
            let end = match request.end {
                Some(values) => Bound::Excluded(
                    values
                        .into_iter()
                        .map(|value| value.into_bytes())
                        .collect::<Vec<_>>(),
                ),
                None => Bound::Unbounded,
            };
            self.partition_router
                .route_rule_range_partitions(&rule, &start, &end)?
        };

        let mut routes = Vec::with_capacity(partition_ids.len());
        for partition_id in partition_ids {
            let worker_id = self.resolve_partition_worker(partition_id).await?;
            routes.push(PartitionRouteEntry {
                partition_id,
                worker_id,
            });
        }
        Ok(PartitionRouteResponse { routes })
    }

    async fn invoke_json(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
        body: String,
    ) -> RS<Value> {
        let map = parse_json_object_body(&body)?;
        let (desc, _, _) = self.procedure_detail(app_name, mod_name, proc_name).await?;
        let param = to_param(&map, desc.param_desc().fields())?;
        let payload = procedure_invoke::serialize_param(param)?;
        let procedure_name = format!("{}/{}/{}", app_name, mod_name, proc_name);
        let mut client = self.client_factory.connect(&self.tcp_addr).await?;
        let session_id = client.create_session(None).await?;
        let invoke_result = client
            .invoke_procedure(session_id, procedure_name, payload)
            .await;
        let close_result = client.close_session(session_id).await;
        let result_binary = match (invoke_result, close_result) {
            (Ok(binary), Ok(_)) => binary,
            (Err(invoke_err), _) => return Err(invoke_err),
            (Ok(_), Err(close_err)) => return Err(close_err),
        };
        let result = procedure_invoke::deserialize_result(&result_binary)?;
        procedure_invoke::result_to_json(result)
    }
}
