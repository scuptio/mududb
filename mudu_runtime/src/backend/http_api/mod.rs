#![allow(missing_docs)]

mod http_api_capabilities;
pub use http_api_capabilities::HttpApiCapabilities;

mod procedure_list;
use procedure_list::ProcedureList;

mod http_api_context;
pub(crate) use http_api_context::HttpApiContext;

mod legacy_http_api;
pub use legacy_http_api::LegacyHttpApi;

mod kernel_http_api;
mod kernel_invoke_client;
mod kernel_invoke_client_factory;
pub use kernel_http_api::KernelHttpApi;
pub use kernel_invoke_client_factory::KernelInvokeClientFactory;

use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_inst::AppInst;
use crate::service::runtime::Runtime;
use actix_cors::Cors;
use actix_web::http::StatusCode;
use actix_web::{App, HttpResponse, HttpServer, Responder, delete, get, post, web};
use async_trait::async_trait;
use base64::Engine;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu::utils::json::JsonValue;
use mudu_binding::procedure::procedure_invoke;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_sys::net::sync::StdTcpListener;
use mudu_utils::notifier::Waiter;
use mudu_utils::scoped_task_trace;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::Arc;
use tracing::error;

fn serialize_oid_as_unioid<S>(oid: &OID, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    UniOid::from(*oid).serialize(serializer)
}

fn deserialize_oid_from_unioid<'de, D>(deserializer: D) -> Result<OID, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(UniOid::deserialize(deserializer)?.to_oid())
}

fn serialize_oid_vec_as_unioid<S>(oids: &[OID], serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let uni_oids: Vec<UniOid> = oids.iter().copied().map(UniOid::from).collect();
    uni_oids.serialize(serializer)
}

fn deserialize_oid_vec_from_unioid<'de, D>(deserializer: D) -> Result<Vec<OID>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Vec::<UniOid>::deserialize(deserializer)?
        .into_iter()
        .map(|oid| oid.to_oid())
        .collect())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct WorkerTopology {
    pub worker_index: usize,
    pub tcp_listen_port: u16,
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub worker_id: OID,
    #[serde(
        serialize_with = "serialize_oid_vec_as_unioid",
        deserialize_with = "deserialize_oid_vec_from_unioid"
    )]
    pub partitions: Vec<OID>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ServerTopology {
    pub worker_count: usize,
    pub tcp_multi_port: bool,
    pub tcp_base_listen_port: u16,
    pub workers: Vec<WorkerTopology>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteRequest {
    pub rule_name: String,
    #[serde(default)]
    pub key: Option<Vec<String>>,
    #[serde(default)]
    pub start: Option<Vec<String>>,
    #[serde(default)]
    pub end: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteEntry {
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub partition_id: mudu::common::id::OID,
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub worker_id: mudu::common::id::OID,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteResponse {
    pub routes: Vec<PartitionRouteEntry>,
}

use crate::backend::app_mgr::AppMgr;
use crate::backend::mudu_app_mgr::ListOption;
use crate::service::app_list::AppListItem;

#[async_trait(?Send)]
pub trait HttpApi: Send + Sync {
    async fn list_apps(&self) -> RS<Vec<String>>;
    async fn list_procedures(&self, app_name: &str) -> RS<Vec<String>>;
    async fn procedure_detail(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
    ) -> RS<(ProcDesc, JsonValue, JsonValue)>;
    async fn install_mpk(&self, mpk_binary: Vec<u8>) -> RS<()>;
    async fn invoke_json(
        &self,
        app_name: &str,
        mod_name: &str,
        proc_name: &str,
        body: String,
    ) -> RS<Value>;

    async fn route_partition(&self, _request: PartitionRouteRequest) -> RS<PartitionRouteResponse> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "partition route is not supported"
        ))
    }

    async fn server_topology(&self) -> RS<ServerTopology> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "server topology is not supported"
        ))
    }

    async fn uninstall_app(&self, app_name: &str) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!("uninstall is not supported for {}", app_name)
        ))
    }
}

#[async_trait(?Send)]
pub trait AsyncKernelInvokeClient: Send {
    async fn create_session(&mut self, config_json: Option<String>) -> RS<u128>;
    async fn invoke_procedure(
        &mut self,
        session_id: u128,
        procedure_name: String,
        procedure_parameters: Vec<u8>,
    ) -> RS<Vec<u8>>;
    async fn close_session(&mut self, session_id: u128) -> RS<bool>;
}

#[async_trait(?Send)]
pub trait AsyncKernelInvokeClientFactory: Send + Sync {
    async fn connect(&self, addr: &str) -> RS<Box<dyn AsyncKernelInvokeClient>>;
}

pub async fn serve_http_api(
    api: Arc<dyn HttpApi>,
    cfg: &MuduDBCfg,
    capabilities: HttpApiCapabilities,
) -> std::io::Result<()> {
    let addr: std::net::SocketAddr = format!("{}:{}", cfg.listen_ip, cfg.http_listen_port)
        .parse()
        .map_err(std::io::Error::other)?;
    let listener = StdTcpListener::bind(addr).map_err(std::io::Error::other)?;
    serve_http_api_on_listener_with_stop(api, listener, capabilities, cfg.http_worker_threads, None)
        .await
}

pub async fn serve_http_api_on_listener(
    api: Arc<dyn HttpApi>,
    listener: StdTcpListener,
    capabilities: HttpApiCapabilities,
    worker_threads: usize,
) -> std::io::Result<()> {
    serve_http_api_on_listener_with_stop(api, listener, capabilities, worker_threads, None).await
}

pub async fn serve_http_api_on_listener_with_stop(
    api: Arc<dyn HttpApi>,
    listener: StdTcpListener,
    capabilities: HttpApiCapabilities,
    worker_threads: usize,
    stop: Option<Waiter>,
) -> std::io::Result<()> {
    scoped_task_trace!();
    let payload_limit = 500 * 1024 * 1024;
    let data = web::Data::new(HttpApiContext { api });

    let server = HttpServer::new(move || {
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(data.clone())
            .app_data(
                web::JsonConfig::default()
                    .limit(payload_limit)
                    .content_type_required(false)
                    .error_handler(|err, req| {
                        error!("JSON payload error: {} for path: {}", err, req.path());
                        actix_web::error::InternalError::new(err, StatusCode::INTERNAL_SERVER_ERROR)
                            .into()
                    }),
            )
            .app_data(web::PayloadConfig::default().limit(payload_limit))
            .app_data(web::FormConfig::default().limit(payload_limit))
            .wrap(actix_web::middleware::Logger::default())
            .configure(|cfg| configure_routes(cfg, capabilities))
    })
    .workers(worker_threads)
    .listen(listener.into_inner())?
    .run();

    if let Some(stop) = stop {
        let handle = server.handle();
        let _ =
            mudu_sys::task::async_::spawn_task_detached("http-server-stop-handler", async move {
                scoped_task_trace!();
                stop.wait().await;
                handle.stop(true).await;
            });
    }

    server.await
}

pub(crate) fn configure_routes(cfg: &mut web::ServiceConfig, capabilities: HttpApiCapabilities) {
    cfg.service(app_list)
        .service(app_proc_list)
        .service(app_proc_detail)
        .service(server_topology)
        .service(partition_route)
        .service(install);
    if capabilities.enable_invoke {
        cfg.service(invoke);
    }
    if capabilities.enable_uninstall {
        cfg.service(uninstall);
    }
}

fn http_ok(data: Value) -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "ok": true,
        "data": data,
        "error": Value::Null,
        "status": 0,
        "message": "ok"
    }))
}

fn error_payload(err: &MuduError) -> Value {
    serde_json::json!({
        "code": err.ec().to_u32(),
        "name": format!("{:?}", err.ec()),
        "message": err.message(),
        "source": err.err_src().to_json_str(),
        "location": err.loc()
    })
}

fn http_err(user_message: impl Into<String>, err: &MuduError) -> HttpResponse {
    let msg = user_message.into();
    let payload = error_payload(err);
    HttpResponse::Ok().json(serde_json::json!({
        "ok": false,
        "data": payload.clone(),
        "error": payload.clone(),
        "status": err.ec().to_u32(),
        "message": msg,
    }))
}

#[post("/mudu/partition/route")]
async fn partition_route(body: String, context: web::Data<HttpApiContext>) -> impl Responder {
    let request = match serde_json::from_str::<PartitionRouteRequest>(&body) {
        Ok(request) => request,
        Err(e) => {
            let err = mudu_error!(
                ErrorCode::Decode,
                "fail to parse partition route request",
                e
            );
            return http_err("fail to parse partition route request", &err);
        }
    };
    match context.api.route_partition(request).await {
        Ok(route) => http_ok(serde_json::to_value(route).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to route partition", &e),
    }
}

#[get("/mudu/server/topology")]
async fn server_topology(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.server_topology().await {
        Ok(topology) => http_ok(serde_json::to_value(topology).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to get server topology", &e),
    }
}

#[get("/mudu/app/list")]
async fn app_list(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.list_apps().await {
        Ok(list) => http_ok(serde_json::to_value(list).unwrap_or(Value::Null)),
        Err(e) => http_err("fail to get app list", &e),
    }
}

#[get("/mudu/app/list/{app_name}")]
async fn app_proc_list(
    path: web::Path<String>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    scoped_task_trace!();
    let app_name = path.into_inner();
    match context.api.list_procedures(&app_name).await {
        Ok(procedures) => http_ok(
            serde_json::to_value(ProcedureList {
                app_name,
                procedures,
            })
            .unwrap_or(Value::Null),
        ),
        Err(e) => http_err(
            format!("fail to get procedure list of app {}", app_name),
            &e,
        ),
    }
}

#[get("/mudu/app/list/{app_name}/{mod_name}/{proc_name}")]
async fn app_proc_detail(
    path: web::Path<(String, String, String)>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    scoped_task_trace!();
    let (app_name, mod_name, proc_name) = path.into_inner();
    match context
        .api
        .procedure_detail(&app_name, &mod_name, &proc_name)
        .await
    {
        Ok((desc, param_json_default, return_json_default)) => http_ok(serde_json::json!({
            "proc_desc": desc,
            "param_default": param_json_default,
            "return_default": return_json_default,
        })),
        Err(e) => http_err(
            format!(
                "fail to get procedure {}/{}/{} detail ",
                app_name, mod_name, proc_name
            ),
            &e,
        ),
    }
}

#[post("/mudu/app/install")]
async fn install(body: web::Bytes, context: web::Data<HttpApiContext>) -> impl Responder {
    let body_str = String::from_utf8_lossy(&body).to_string();
    match decode_install_request(&body_str) {
        Ok(binary) => {
            let package_name = mpk_package_name(&binary).unwrap_or_else(|| "<unknown>".to_string());
            match context.api.install_mpk(binary).await {
                Ok(()) => http_ok(JsonValue::Null),
                Err(e) => http_err(format!("fail to install package {}", package_name), &e),
            }
        }
        Err(e) => http_err("fail to install package <invalid request>", &e),
    }
}

#[delete("/mudu/app/uninstall/{app_name}")]
async fn uninstall(path: web::Path<String>, context: web::Data<HttpApiContext>) -> impl Responder {
    let app_name = path.into_inner();
    match context.api.uninstall_app(&app_name).await {
        Ok(()) => http_ok(JsonValue::Null),
        Err(e) => http_err(format!("fail to uninstall app {}", app_name), &e),
    }
}

#[post("/mudu/app/invoke/{app_name}/{mod_name}/{proc_name}")]
async fn invoke(
    path: web::Path<(String, String, String)>,
    body: web::Bytes,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let (app_name, mod_name, proc_name) = path.into_inner();
    let body_str = String::from_utf8_lossy(&body).to_string();
    let proc = format!("{}/{}/{}", app_name, mod_name, proc_name);
    match context
        .api
        .invoke_json(&app_name, &mod_name, &proc_name, body_str)
        .await
    {
        Ok(value) => http_ok(value),
        Err(e) => http_err(format!("fail to invoke procedure {}", proc), &e),
    }
}

pub(crate) fn decode_install_request(body_str: &str) -> RS<Vec<u8>> {
    let map = serde_json::from_str::<HashMap<String, String>>(body_str)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "deserialize body error: {}", e))?;
    let mpk_base64 = map.get("mpk_base64").ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidArgument,
            "mpk_base64 missing for install request"
        )
    })?;
    base64::engine::general_purpose::STANDARD
        .decode(mpk_base64)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode error", e))
}

pub(crate) fn mpk_package_name(binary: &[u8]) -> Option<String> {
    let cursor = Cursor::new(binary);
    let mut archive = zip::ZipArchive::new(cursor).ok()?;
    let mut package_cfg = String::new();
    archive
        .by_name("package.cfg.json")
        .ok()?
        .read_to_string(&mut package_cfg)
        .ok()?;
    serde_json::from_str::<Value>(&package_cfg)
        .ok()?
        .get("name")?
        .as_str()
        .map(str::to_string)
}

pub(crate) fn to_param(argv: &Map<String, Value>, desc: &[DatumDesc]) -> RS<ProcedureParam> {
    let mut vec = vec![];
    for datum_desc in desc.iter() {
        let value = argv
            .get(datum_desc.name())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("no parameter {}", datum_desc.name())
                )
            })?
            .clone();
        let id = datum_desc.dat_type_id();
        let dat_value = id.fn_input_json()(&value, datum_desc.dat_type()).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "convert printable to internal error",
                e
            )
        })?;
        vec.push(dat_value)
    }
    Ok(ProcedureParam::new(0, 0, vec))
}

pub(crate) fn parse_json_object_body(body: &str) -> RS<Map<String, Value>> {
    let object: Value = serde_json::from_str(body)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "deserialize error", e))?;
    match object {
        Value::Object(obj_map) => Ok(obj_map),
        _ => Err(mudu_error!(
            ErrorCode::Decode,
            "request json body must be an object"
        )),
    }
}

pub(crate) async fn runtime_get_app_and_desc(
    service: Arc<dyn Runtime>,
    app_name: &str,
    mod_name: &str,
    proc_name: &str,
) -> RS<(Arc<dyn AppInst>, Arc<ProcDesc>)> {
    let app = service.app(app_name.to_string()).await.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("no such app {}", app_name)
        )
    })?;
    let desc = app.describe(mod_name, proc_name)?;
    Ok((app, desc))
}

pub(crate) async fn legacy_invoke_sync_proc(
    mod_name: &str,
    proc_name: &str,
    argv: Map<String, Value>,
    app: Arc<dyn AppInst>,
    desc: Arc<ProcDesc>,
) -> RS<RS<Value>> {
    let task_id = app.task_create().await?;
    let _app = app.clone();
    let _g = scopeguard::guard(task_id, |task_id| {
        let _r = _app.task_end(task_id);
    });

    let param = to_param(&argv, desc.param_desc().fields())?;
    let result = app
        .invoke(task_id, mod_name, proc_name, param, None)
        .await?;
    Ok(procedure_invoke::result_to_json(result))
}

pub(crate) async fn legacy_invoke_async_proc(
    mod_name: &str,
    proc_name: &str,
    argv: Map<String, Value>,
    app: Arc<dyn AppInst>,
    desc: Arc<ProcDesc>,
) -> RS<Value> {
    let task_id = app.task_create().await?;
    let _g = scopeguard::guard(task_id, |task_id| {
        let _r = app.task_end(task_id);
    });
    let param = to_param(&argv, desc.param_desc().fields())?;
    let result = app
        .invoke_async(task_id, mod_name, proc_name, param, None)
        .await?;
    procedure_invoke::result_to_json(result)
}

pub(crate) async fn find_app(app_mgr: &dyn AppMgr, app_name: &str) -> RS<AppListItem> {
    let listed_apps = app_mgr
        .list(&ListOption {
            names: vec![app_name.to_string()],
        })
        .await?;
    listed_apps
        .apps
        .into_iter()
        .find(|app| app.info.name == app_name)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such app {}", app_name)
            )
        })
}

// These tests spin up an actix-web app. actix-router intentionally leaks route
// capture names via `Box::leak`, which Miri reports as reachable memory leaks.
// Each test below returns early when running under Miri.
#[cfg(test)]
mod test {
    use super::*;
    use actix_web::{App, test};
    use mudu::common::app_info::AppInfo;
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use mudu_contract::procedure::procedure_result::ProcedureResult;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_kernel::contract::partition_rule::{
        PartitionBound, PartitionRuleDesc, RangePartitionDef,
    };
    use mudu_kernel::contract::partition_rule_binding::PartitionPlacement;
    use mudu_kernel::meta::meta_mgr_factory::MetaMgrFactory;
    use mudu_kernel::server::async_func_runtime::AsyncFuncInvoker;
    use mudu_sys::sync::SMutex;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_utils::oid::gen_oid;

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

        async fn list(&self, _option: &ListOption) -> RS<crate::service::app_list::AppList> {
            let desc = ProcDesc::new(
                "mod1".to_string(),
                "proc1".to_string(),
                <(i32,)>::tuple_desc_static(&["value".to_string()]),
                <(i32,)>::tuple_desc_static(&["value".to_string()]),
                false,
            );
            let mut mod_desc = ModProcDesc::new_empty();
            mod_desc.add(desc);
            Ok(crate::service::app_list::AppList {
                apps: vec![crate::service::app_list::AppListItem {
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
        let registry =
            mudu_kernel::server::worker_registry::load_or_create_worker_registry(&log_dir, 4)
                .unwrap();
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
        let log_dir =
            mudu_sys::env_var::temp_dir().join(format!("http_api_route_test_{}", gen_oid()));
        let registry =
            mudu_kernel::server::worker_registry::load_or_create_worker_registry(&log_dir, 4)
                .unwrap();
        let meta_dir =
            mudu_sys::env_var::temp_dir().join(format!("http_api_route_meta_{}", gen_oid()));
        let meta_mgr = MetaMgrFactory::create(meta_dir.to_string_lossy().to_string())
            .await
            .unwrap();
        meta_mgr.initialize().await.unwrap();

        let rule = PartitionRuleDesc::new_range(
            "global_rule".to_string(),
            vec![DatTypeID::I32],
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
        let log_dir =
            mudu_sys::env_var::temp_dir().join(format!("http_api_meta_list_{}", gen_oid()));
        let registry =
            mudu_kernel::server::worker_registry::load_or_create_worker_registry(&log_dir, 3)
                .unwrap();
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
        let log_dir =
            mudu_sys::env_var::temp_dir().join(format!("http_api_close_err_{}", gen_oid()));
        let registry =
            mudu_kernel::server::worker_registry::load_or_create_worker_registry(&log_dir, 2)
                .unwrap();
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
        let log_dir =
            mudu_sys::env_var::temp_dir().join(format!("http_api_route_shape_{}", gen_oid()));
        let registry =
            mudu_kernel::server::worker_registry::load_or_create_worker_registry(&log_dir, 2)
                .unwrap();
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
            vec![DatTypeID::I32],
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
}
