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
