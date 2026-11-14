use super::*;
use crate::backend::app_mgr::AppMgr;
use crate::backend::mudu_app_mgr::ListOption;
use crate::backend::mudud_cfg::MuduDBCfg;
use ::catalog::contract::partition_rule::{PartitionBound, PartitionRuleDesc, RangePartitionDef};
use ::catalog::contract::partition_rule_binding::PartitionPlacement;
use ::meta::meta_mgr_factory::MetaMgrFactory;
use ::worker::async_func_runtime::AsyncFuncInvoker;
use actix_web::{App, http::StatusCode, test, web};
use mudu::common::app_info::AppInfo;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_sys::sync::SMutex;
use mudu_type::type_family::TypeFamily;
use mudu_utils::oid::gen_oid;
use std::sync::Arc;

struct MockHttpApi;

#[async_trait(?Send)]
impl HttpApi for MockHttpApi {
    async fn list_apps(&self) -> RS<Vec<String>> {
        Ok(vec!["app1".to_string()])
    }

    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>> {
        Ok(vec![format!("{}/proc1", app_name)])
    }

    async fn procedure_detail(
        &self,
        _app_name: &str,
        _mod_name: &str,
        _proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)> {
        let desc = ProcDesc::new(
            "mod1".to_string(),
            "proc1".to_string(),
            <(i32,)>::tuple_desc_static(&["value".to_string()]),
            <(i32,)>::tuple_desc_static(&["value".to_string()]),
            false,
        );
        Ok((
            desc,
            serde_json::json!({"value": 0}),
            serde_json::json!({"value": 0}),
        ))
    }

    async fn install_mpk(&self, _mpk_binary: Vec<u8>) -> RS<()> {
        Ok(())
    }

    async fn invoke_json(
        &self,
        _app_name: &str,
        _mod_name: &str,
        _proc_name: &str,
        _body: String,
    ) -> RS<Value> {
        Ok(serde_json::json!({"ok": true}))
    }

    async fn server_queues(&self) -> RS<ServerQueuesResponse> {
        Ok(ServerQueuesResponse {
            mode: "ok".to_string(),
            queues: vec![ServerQueueStatus {
                schema: "mududb".to_string(),
                name: "jobs".to_string(),
                queue_id: 42,
                worker_index: 0,
                depth: 3,
                bytes: 128,
                overflow_count: 2,
                next_seq: 10,
                acked_frontier: 7,
                dlq_depth: Some(1),
                waiters: 1,
                shards: vec![],
                consumers: vec![ServerQueueConsumer {
                    name: "cg1".to_string(),
                    shard: 0,
                    frontier: 6,
                    lag: 4,
                }],
                activation: Some(ServerQueueActivation {
                    queue: "jobs".to_string(),
                    procedure: "app/mod/handle".to_string(),
                    enabled: true,
                    consecutive_failures: 0,
                    max_retry: 5,
                    batch: 32,
                }),
            }],
        })
    }

    async fn queue_activation_register(&self, request: QueueActivationRequest) -> RS<()> {
        if request.queue == "missing" {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::EntityNotFound,
                "no such queue missing"
            ));
        }
        Ok(())
    }

    async fn queue_activation_unregister(&self, queue: String) -> RS<()> {
        if queue == "missing" {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::EntityNotFound,
                "queue missing has no registered activation"
            ));
        }
        Ok(())
    }

    async fn server_queue_activations(&self) -> RS<ServerQueueActivationsResponse> {
        Ok(ServerQueueActivationsResponse {
            mode: "ok".to_string(),
            activations: vec![ServerQueueActivation {
                queue: "jobs".to_string(),
                procedure: "app/mod/handle".to_string(),
                enabled: true,
                consecutive_failures: 2,
                max_retry: 5,
                batch: 32,
            }],
        })
    }
}

#[actix_web::test]
async fn server_queues_route_returns_queue_status() {
    if cfg!(miri) {
        // actix-router leaks route capture names via Box::leak; skip under Miri.
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(MockHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::LEGACY)),
    )
    .await;

    let request = test::TestRequest::get()
        .uri("/mudu/server/queues")
        .to_request();
    let response: Value = test::call_and_read_body_json(&app, request).await;
    assert_eq!(response["status"], 0);
    assert_eq!(response["data"]["mode"], "ok");
    let queue = &response["data"]["queues"][0];
    assert_eq!(queue["schema"], "mududb");
    assert_eq!(queue["name"], "jobs");
    assert_eq!(queue["worker_index"], 0);
    assert_eq!(queue["depth"], 3);
    assert_eq!(queue["next_seq"], 10);
    assert_eq!(queue["acked_frontier"], 7);
    assert_eq!(queue["dlq_depth"], 1);
    assert_eq!(queue["waiters"], 1);
    let consumer = &queue["consumers"][0];
    assert_eq!(consumer["name"], "cg1");
    assert_eq!(consumer["frontier"], 6);
    assert_eq!(consumer["lag"], 4);
    let activation = &queue["activation"];
    assert_eq!(activation["procedure"], "app/mod/handle");
    assert_eq!(activation["enabled"], true);
    assert_eq!(activation["max_retry"], 5);
}

#[actix_web::test]
async fn queue_activation_routes_register_list_unregister() {
    if cfg!(miri) {
        // actix-router leaks route capture names via Box::leak; skip under Miri.
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(MockHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::LEGACY)),
    )
    .await;

    let register_req = test::TestRequest::post()
        .uri("/mudu/server/queue_activation")
        .set_payload(r#"{"queue":"jobs","procedure":"app/mod/handle","max_retry":3}"#)
        .to_request();
    let register_resp: Value = test::call_and_read_body_json(&app, register_req).await;
    assert_eq!(register_resp["status"], 0);

    // An unknown queue surfaces the error envelope with its error code.
    let register_req = test::TestRequest::post()
        .uri("/mudu/server/queue_activation")
        .set_payload(r#"{"queue":"missing","procedure":"app/mod/handle"}"#)
        .to_request();
    let register_resp: Value = test::call_and_read_body_json(&app, register_req).await;
    assert_eq!(register_resp["status"], 50009);
    assert_eq!(register_resp["ok"], false);

    let list_req = test::TestRequest::get()
        .uri("/mudu/server/queue_activations")
        .to_request();
    let list_resp: Value = test::call_and_read_body_json(&app, list_req).await;
    assert_eq!(list_resp["status"], 0);
    let activation = &list_resp["data"]["activations"][0];
    assert_eq!(activation["queue"], "jobs");
    assert_eq!(activation["procedure"], "app/mod/handle");
    assert_eq!(activation["enabled"], true);
    assert_eq!(activation["consecutive_failures"], 2);
    assert_eq!(activation["max_retry"], 5);
    assert_eq!(activation["batch"], 32);

    let delete_req = test::TestRequest::delete()
        .uri("/mudu/server/queue_activation/jobs")
        .to_request();
    let delete_resp: Value = test::call_and_read_body_json(&app, delete_req).await;
    assert_eq!(delete_resp["status"], 0);

    let delete_req = test::TestRequest::delete()
        .uri("/mudu/server/queue_activation/missing")
        .to_request();
    let delete_resp: Value = test::call_and_read_body_json(&app, delete_req).await;
    assert_eq!(delete_resp["status"], 50009);
}

#[actix_web::test]
async fn shared_routes_respect_capabilities() {
    if cfg!(miri) {
        // actix-router leaks route capture names via Box::leak; skip under Miri.
        return;
    }
    let app = test::init_service(
        App::new()
            .app_data(web::Data::new(HttpApiContext {
                api: Arc::new(MockHttpApi),
            }))
            .configure(|cfg| configure_routes(cfg, HttpApiCapabilities::LEGACY)),
    )
    .await;

    let invoke_req = test::TestRequest::post()
        .uri("/mudu/app/invoke/app1/mod1/proc1")
        .set_payload(r#"{"v_0":1}"#)
        .to_request();
    let invoke_resp: Value = test::call_and_read_body_json(&app, invoke_req).await;
    assert_eq!(invoke_resp["status"], 0);

    let uninstall_req = test::TestRequest::delete()
        .uri("/mudu/app/uninstall/app1")
        .to_request();
    let uninstall_resp = test::call_service(&app, uninstall_req).await;
    assert_eq!(uninstall_resp.status(), StatusCode::NOT_FOUND);
}

struct MockClient {
    session_id: u128,
    closed: bool,
    requests: Arc<SMutex<Vec<String>>>,
}

#[async_trait(?Send)]
impl AsyncKernelInvokeClient for MockClient {
    async fn create_session(&mut self, _config_json: Option<String>) -> RS<u128> {
        Ok(self.session_id)
    }

    async fn invoke_procedure(
        &mut self,
        _session_id: u128,
        procedure_name: String,
        procedure_parameters: Vec<u8>,
    ) -> RS<Vec<u8>> {
        self.requests.lock().unwrap().push(procedure_name);
        let param = procedure_invoke::deserialize_param(&procedure_parameters)?;
        assert_eq!(param.param_list().len(), 1);
        procedure_invoke::serialize_result(ProcedureResult::from(
            Ok((42i32,)),
            &<(i32,)>::tuple_desc_static(&["value".to_string()]),
        ))
    }

    async fn close_session(&mut self, _session_id: u128) -> RS<bool> {
        if self.closed {
            return Err(mudu_error!(ErrorCode::InvalidState, "close session failed"));
        }
        self.closed = true;
        Ok(true)
    }
}

struct MockClientFactory {
    requests: Arc<SMutex<Vec<String>>>,
    fail_close: bool,
}

#[async_trait(?Send)]
impl AsyncKernelInvokeClientFactory for MockClientFactory {
    async fn connect(&self, _addr: &str) -> RS<Box<dyn AsyncKernelInvokeClient>> {
        Ok(Box::new(MockClient {
            session_id: 9,
            closed: self.fail_close,
            requests: self.requests.clone(),
        }))
    }
}

struct MockAppMgr;

#[async_trait(?Send)]
impl AppMgr for MockAppMgr {
    async fn install(&self, _mpk_binary: Vec<u8>) -> RS<()> {
        Ok(())
    }

    async fn uninstall(&self, _app_name: Vec<u8>) -> RS<()> {
        Ok(())
    }

    async fn list(&self, _option: &ListOption) -> RS<runtime_wasm::service::app_list::AppList> {
        let desc = ProcDesc::new(
            "mod1".to_string(),
            "proc1".to_string(),
            <(i32,)>::tuple_desc_static(&["value".to_string()]),
            <(i32,)>::tuple_desc_static(&["value".to_string()]),
            false,
        );
        let mut mod_desc = ModProcDesc::new_empty();
        mod_desc.add(desc);
        Ok(runtime_wasm::service::app_list::AppList {
            apps: vec![runtime_wasm::service::app_list::AppListItem {
                info: AppInfo {
                    name: "app1".to_string(),
                    lang: "rust".to_string(),
                    version: "0.1.0".to_string(),
                    use_async: false,
                },
                ddl: String::new(),
                mod_proc_desc: mod_desc,
            }],
        })
    }

    async fn create_invoker(&self, _cfg: &MuduDBCfg) -> RS<Arc<dyn AsyncFuncInvoker>> {
        Err(mudu_error!(ErrorCode::NotImplemented, "unused in test"))
    }
}

#[actix_web::test]
async fn kernel_http_api_invokes_over_bridge() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_test_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 4).unwrap();
    let requests = Arc::new(SMutex::new(Vec::new()));
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        MetaMgrFactory::create(
            mudu_sys::env_var::temp_dir()
                .join(format!("http_api_meta_test_{}", gen_oid()))
                .to_string_lossy()
                .to_string(),
        )
        .await
        .unwrap(),
        Arc::new(MockClientFactory {
            requests: requests.clone(),
            fail_close: false,
        }),
    );

    let response = api
        .invoke_json("app1", "mod1", "proc1", r#"{"value": 7}"#.to_string())
        .await
        .unwrap();

    assert!(response.is_object());
    assert!(response.get("return_list").is_some());
    assert_eq!(
        requests.lock().unwrap().as_slice(),
        &["app1/mod1/proc1".to_string()]
    );
}

#[actix_web::test]
async fn kernel_http_api_routes_point_and_range_by_rule_name() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_route_test_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 4).unwrap();
    let meta_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_route_meta_{}", gen_oid()));
    let meta_mgr = MetaMgrFactory::create(meta_dir.to_string_lossy().to_string())
        .await
        .unwrap();
    meta_mgr.initialize().await.unwrap();

    let rule = PartitionRuleDesc::new_range(
        "global_rule".to_string(),
        vec![TypeFamily::I32],
        vec![
            RangePartitionDef::new(
                "p0".to_string(),
                PartitionBound::Unbounded,
                PartitionBound::Value(vec![b"100".to_vec()]),
            ),
            RangePartitionDef::new(
                "p1".to_string(),
                PartitionBound::Value(vec![b"100".to_vec()]),
                PartitionBound::Unbounded,
            ),
        ],
    );
    let p0 = rule.partitions[0].partition_id;
    let p1 = rule.partitions[1].partition_id;
    let w0 = registry.workers()[0].worker_id;
    let w1 = registry.workers()[1].worker_id;
    meta_mgr.create_partition_rule(&rule).await.unwrap();
    meta_mgr
        .upsert_partition_placements(&[
            PartitionPlacement {
                partition_id: p0,
                worker_id: w0,
            },
            PartitionPlacement {
                partition_id: p1,
                worker_id: w1,
            },
        ])
        .await
        .unwrap();

    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        meta_mgr,
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    );

    let point = api
        .route_partition(PartitionRouteRequest {
            rule_name: "global_rule".to_string(),
            key: Some(vec!["12".to_string()]),
            start: None,
            end: None,
        })
        .await
        .unwrap();
    assert_eq!(point.routes.len(), 1);
    assert_eq!(point.routes[0].partition_id, p0);
    assert_eq!(point.routes[0].worker_id, w0);

    let range = api
        .route_partition(PartitionRouteRequest {
            rule_name: "global_rule".to_string(),
            key: None,
            start: Some(vec!["50".to_string()]),
            end: Some(vec!["150".to_string()]),
        })
        .await
        .unwrap();
    assert_eq!(range.routes.len(), 2);
    assert_eq!(range.routes[0].partition_id, p0);
    assert_eq!(range.routes[0].worker_id, w0);
    assert_eq!(range.routes[1].partition_id, p1);
    assert_eq!(range.routes[1].worker_id, w1);
}

#[actix_web::test]
async fn kernel_http_api_lists_metadata_and_topology() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_meta_list_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 3).unwrap();
    let meta_mgr = MetaMgrFactory::create(
        mudu_sys::env_var::temp_dir()
            .join(format!("http_api_meta_mgr_{}", gen_oid()))
            .to_string_lossy()
            .to_string(),
    )
    .await
    .unwrap();
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry.clone(),
        meta_mgr,
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    );

    assert_eq!(api.list_apps().await.unwrap(), vec!["app1".to_string()]);
    assert_eq!(
        api.list_procedures("app1").await.unwrap(),
        vec!["mod1/proc1".to_string()]
    );
    let (desc, param_json, return_json) =
        api.procedure_detail("app1", "mod1", "proc1").await.unwrap();
    assert_eq!(desc.proc_name(), "proc1");
    assert_eq!(param_json["value"], 0);
    assert_eq!(return_json["value"], 0);

    let topology = api.server_topology().await.unwrap();
    assert_eq!(topology.worker_count, registry.workers().len());
    assert!(!topology.tcp_multi_port);
    assert_eq!(topology.tcp_base_listen_port, 9527);
    assert_eq!(topology.workers.len(), registry.workers().len());
}

#[actix_web::test]
async fn kernel_http_api_surfaces_close_session_failure() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_close_err_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 2).unwrap();
    let meta_mgr = MetaMgrFactory::create(
        mudu_sys::env_var::temp_dir()
            .join(format!("http_api_close_meta_{}", gen_oid()))
            .to_string_lossy()
            .to_string(),
    )
    .await
    .unwrap();
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        meta_mgr,
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: true,
        }),
    );

    let err = api
        .invoke_json("app1", "mod1", "proc1", r#"{"value": 7}"#.to_string())
        .await
        .unwrap_err();
    assert!(err.to_string().contains("close session failed"));
}

#[actix_web::test]
async fn kernel_http_api_rejects_mixed_route_request_shapes() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_route_shape_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 2).unwrap();
    let meta_mgr = MetaMgrFactory::create(
        mudu_sys::env_var::temp_dir()
            .join(format!("http_api_route_shape_meta_{}", gen_oid()))
            .to_string_lossy()
            .to_string(),
    )
    .await
    .unwrap();
    meta_mgr.initialize().await.unwrap();
    let rule = PartitionRuleDesc::new_range(
        "shape_rule".to_string(),
        vec![TypeFamily::I32],
        vec![RangePartitionDef::new(
            "p0".to_string(),
            PartitionBound::Unbounded,
            PartitionBound::Unbounded,
        )],
    );
    meta_mgr.create_partition_rule(&rule).await.unwrap();

    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        meta_mgr,
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    );

    let err = api
        .route_partition(PartitionRouteRequest {
            rule_name: "shape_rule".to_string(),
            key: Some(vec!["1".to_string()]),
            start: Some(vec!["0".to_string()]),
            end: None,
        })
        .await
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("cannot specify both key and range")
    );
}

#[actix_web::test]
async fn kernel_http_api_backup_endpoints_require_configuration() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_backup_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 1).unwrap();
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        MetaMgrFactory::create(
            mudu_sys::env_var::temp_dir()
                .join(format!("http_api_backup_meta_{}", gen_oid()))
                .to_string_lossy()
                .to_string(),
        )
        .await
        .unwrap(),
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    );

    // Without [backup] backup_dir every backup endpoint rejects early.
    let err = api.backup("l1").await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);
    let err = api.backup_list().await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);
    let err = api.backup_verify("l1").await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);
    let err = api.backup_delete("l1").await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);

    // Without a server instance id the dump/restore endpoints reject early.
    let err = api.dump_logical("l1", "/tmp/x").await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);
    let err = api
        .restore_logical("l1", "/tmp/x", false)
        .await
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);
}

#[actix_web::test]
async fn kernel_http_api_backup_endpoints_drive_the_thread_bridge() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_backup2_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 1).unwrap();
    let backup_dir = mudu_sys::env_var::temp_dir()
        .join(format!("http_api_backup_dir_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    // A dedicated server instance id: no workers registered under it.
    let server_instance_id = 9_876_543;
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        MetaMgrFactory::create(
            mudu_sys::env_var::temp_dir()
                .join(format!("http_api_backup_meta2_{}", gen_oid()))
                .to_string_lossy()
                .to_string(),
        )
        .await
        .unwrap(),
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    )
    .with_server_instance_id(Some(server_instance_id))
    .with_backup_dir(Some(backup_dir.clone()));

    // Base backup with no live workers fails from the worker registry.
    let err = api.backup("l1").await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);

    // Listing an empty/unwritten backup directory succeeds with no labels.
    assert_eq!(api.backup_list().await.unwrap(), Vec::<String>::new());

    // Verifying a missing backup is a corruption-class error.
    let err = api.backup_verify("absent").await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);

    // Deleting a missing backup is a clean `false`.
    assert!(!api.backup_delete("absent").await.unwrap());

    // Logical dump/restore run through the same bridge; with no live
    // workers they fail inside the dump/restore routine.
    let dump_dir = mudu_sys::env_var::temp_dir()
        .join(format!("http_api_dump_{}", gen_oid()))
        .to_string_lossy()
        .to_string();
    assert!(api.dump_logical("l1", &dump_dir).await.is_err());
    assert!(api.restore_logical("l1", &dump_dir, false).await.is_err());

    let _ = mudu_sys::fs::sync::remove_dir_all(&backup_dir);
    let _ = mudu_sys::fs::sync::remove_dir_all(&dump_dir);
}

#[actix_web::test]
async fn http_api_default_methods_report_not_implemented() {
    // MockHttpApi overrides only the queue endpoints; every other default
    // body must report NotImplemented (capability gating relies on it).
    let api = MockHttpApi;
    let route_req = PartitionRouteRequest {
        rule_name: "r".to_string(),
        key: Some(vec!["1".to_string()]),
        start: None,
        end: None,
    };
    assert_eq!(
        api.route_partition(route_req).await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.server_topology().await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.server_replication().await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.uninstall_app("app1").await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.backup("l".to_string().as_str()).await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.backup_list().await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.backup_verify("l").await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.backup_delete("l").await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.dump_logical("l", "/tmp/x").await.unwrap_err().ec(),
        ErrorCode::NotImplemented
    );
    assert_eq!(
        api.restore_logical("l", "/tmp/x", false)
            .await
            .unwrap_err()
            .ec(),
        ErrorCode::NotImplemented
    );

    let membership_err = api
        .repl_membership(ReplMembershipRequest {
            action: "remove".to_string(),
            node_id: 9,
            host: None,
            port: None,
        })
        .await
        .unwrap_err();
    assert_eq!(membership_err.error.ec(), ErrorCode::NotImplemented);
    assert!(membership_err.leader_hint.is_none());

    let promote_err = api
        .repl_promote(ReplPromoteRequest { fencing_ok: true })
        .await
        .unwrap_err();
    assert_eq!(promote_err.error.ec(), ErrorCode::NotImplemented);

    let follow_err = api
        .repl_follow(ReplFollowRequest {
            primary_node_id: 1,
            primary_host: "127.0.0.1".to_string(),
            primary_port: 9700,
        })
        .await
        .unwrap_err();
    assert_eq!(follow_err.error.ec(), ErrorCode::NotImplemented);
}

#[actix_web::test]
async fn mock_fixtures_cover_their_trait_surfaces() {
    // Keep the mocks honest: each stub body runs so a signature drift in
    // the trait breaks this test instead of a distant caller.
    let api = MockHttpApi;
    assert_eq!(api.list_apps().await.unwrap(), vec!["app1".to_string()]);
    assert_eq!(
        api.list_procedures("app1").await.unwrap(),
        vec!["app1/proc1".to_string()]
    );
    let (_desc, param, ret) = api.procedure_detail("app1", "mod1", "proc1").await.unwrap();
    assert_eq!(param, serde_json::json!({"value": 0}));
    assert_eq!(ret, serde_json::json!({"value": 0}));
    api.install_mpk(vec![1, 2, 3]).await.unwrap();
    let _ = api
        .invoke_json("app1", "mod1", "proc1", "{}".to_string())
        .await
        .unwrap();

    let factory = MockClientFactory {
        requests: Arc::new(SMutex::new(Vec::new())),
        fail_close: false,
    };
    let _client = factory.connect("127.0.0.1:1").await.unwrap();

    let mgr = MockAppMgr;
    mgr.install(vec![0]).await.unwrap();
    mgr.uninstall(b"app1".to_vec()).await.unwrap();
    let listed = mgr.list(&ListOption { names: vec![] }).await.unwrap();
    assert_eq!(listed.apps.len(), 1);
    match mgr.create_invoker(&MuduDBCfg::default()).await {
        Err(err) => assert_eq!(err.ec(), ErrorCode::NotImplemented),
        Ok(_) => panic!("create_invoker must stay unimplemented in the mock"),
    }
}

#[actix_web::test]
async fn kernel_http_api_repl_admin_endpoints_validate_configuration() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_repl_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 1).unwrap();
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        MetaMgrFactory::create(
            mudu_sys::env_var::temp_dir()
                .join(format!("http_api_repl_meta_{}", gen_oid()))
                .to_string_lossy()
                .to_string(),
        )
        .await
        .unwrap(),
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    );

    // Unknown action is rejected before any raft interaction.
    let err = api
        .repl_membership(ReplMembershipRequest {
            action: "bogus".to_string(),
            node_id: 2,
            host: None,
            port: None,
        })
        .await
        .unwrap_err();
    assert_eq!(err.error.ec(), ErrorCode::InvalidArgument);

    // No raft node id configured: every admin endpoint rejects.
    let membership_req = ReplMembershipRequest {
        action: "remove".to_string(),
        node_id: 2,
        host: None,
        port: None,
    };
    let err = api.repl_membership(membership_req).await.unwrap_err();
    assert_eq!(err.error.ec(), ErrorCode::InvalidState);
    let err = api
        .repl_follow(ReplFollowRequest {
            primary_node_id: 1,
            primary_host: "127.0.0.1".to_string(),
            primary_port: 9700,
        })
        .await
        .unwrap_err();
    assert_eq!(err.error.ec(), ErrorCode::InvalidState);
    let err = api
        .repl_promote(ReplPromoteRequest { fencing_ok: true })
        .await
        .unwrap_err();
    assert_eq!(err.error.ec(), ErrorCode::InvalidState);
}

#[actix_web::test]
async fn kernel_http_api_repl_membership_drives_the_thread_bridge() {
    if cfg!(miri) {
        return;
    }
    let log_dir = mudu_sys::env_var::temp_dir().join(format!("http_api_repl2_{}", gen_oid()));
    let registry = ::worker::worker_registry::load_or_create_worker_registry(&log_dir, 1).unwrap();
    let api = KernelHttpApi::with_client_factory(
        Arc::new(MockAppMgr),
        "127.0.0.1:9527".to_string(),
        false,
        9527,
        registry,
        MetaMgrFactory::create(
            mudu_sys::env_var::temp_dir()
                .join(format!("http_api_repl_meta2_{}", gen_oid()))
                .to_string_lossy()
                .to_string(),
        )
        .await
        .unwrap(),
        Arc::new(MockClientFactory {
            requests: Arc::new(SMutex::new(Vec::new())),
            fail_close: false,
        }),
    )
    // A raft node id is configured but no raft group runs under it: the
    // membership call crosses the thread bridge and fails in the registry.
    .with_raft_node_id(Some(8_765_431));

    let err = api
        .repl_membership(ReplMembershipRequest {
            action: "add_learner".to_string(),
            node_id: 2,
            host: Some("127.0.0.1".to_string()),
            port: Some(9701),
        })
        .await
        .unwrap_err();
    assert_eq!(err.error.ec(), ErrorCode::InvalidState);

    // Same bridge for stream promote: no stream service under the node id.
    let err = api
        .repl_promote(ReplPromoteRequest { fencing_ok: true })
        .await
        .unwrap_err();
    assert!(err.error.ec() != ErrorCode::NotImplemented);
}
