mod http_api_capabilities;
pub use http_api_capabilities::HttpApiCapabilities;

mod procedure_list;
use procedure_list::ProcedureList;

mod http_api_context;
use http_api_context::HttpApiContext;

mod legacy_http_api;
pub use legacy_http_api::LegacyHttpApi;

#[cfg(target_os = "linux")]
mod tokio_iouring_invoke_client_factory;
#[cfg(target_os = "linux")]
pub use tokio_iouring_invoke_client_factory::TokioIoUringInvokeClientFactory;

#[cfg(target_os = "linux")]
mod tokio_iouring_invoke_client;

#[cfg(target_os = "linux")]
mod io_uring_http_api;
#[cfg(target_os = "linux")]
pub use io_uring_http_api::IoUringHttpApi;

use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_inst::AppInst;
use crate::service::runtime::Runtime;
use actix_cors::Cors;
use actix_web::http::StatusCode;
use actix_web::{App, HttpResponse, HttpServer, Responder, delete, get, post, web};
use async_trait::async_trait;
use base64::Engine;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::JsonValue;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_utils::notifier::Waiter;
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::Arc;
use tracing::error;

#[cfg(target_os = "linux")]
use crate::backend::app_mgr::AppMgr;
#[cfg(target_os = "linux")]
use crate::backend::mudu_app_mgr::ListOption;
#[cfg(target_os = "linux")]
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

    async fn uninstall_app(&self, app_name: &str) -> RS<()> {
        Err(m_error!(
            EC::NotImplemented,
            format!("uninstall is not supported for {}", app_name)
        ))
    }
}

#[cfg(target_os = "linux")]
#[async_trait(?Send)]
pub trait AsyncIoUringInvokeClient: Send {
    async fn create_session(&mut self, config_json: Option<String>) -> RS<u128>;
    async fn invoke_procedure(
        &mut self,
        session_id: u128,
        procedure_name: String,
        procedure_parameters: Vec<u8>,
    ) -> RS<Vec<u8>>;
    async fn close_session(&mut self, session_id: u128) -> RS<bool>;
}

#[cfg(target_os = "linux")]
#[async_trait(?Send)]
pub trait AsyncIoUringInvokeClientFactory: Send + Sync {
    async fn connect(&self, addr: &str) -> RS<Box<dyn AsyncIoUringInvokeClient>>;
}

pub async fn serve_http_api(
    api: Arc<dyn HttpApi>,
    cfg: &MuduDBCfg,
    capabilities: HttpApiCapabilities,
) -> std::io::Result<()> {
    let listener = TcpListener::bind(format!("{}:{}", cfg.listen_ip, cfg.http_listen_port))?;
    serve_http_api_on_listener_with_stop(api, listener, capabilities, cfg.http_worker_threads, None)
        .await
}

pub async fn serve_http_api_on_listener(
    api: Arc<dyn HttpApi>,
    listener: TcpListener,
    capabilities: HttpApiCapabilities,
    worker_threads: usize,
) -> std::io::Result<()> {
    serve_http_api_on_listener_with_stop(api, listener, capabilities, worker_threads, None).await
}

pub async fn serve_http_api_on_listener_with_stop(
    api: Arc<dyn HttpApi>,
    listener: TcpListener,
    capabilities: HttpApiCapabilities,
    worker_threads: usize,
    stop: Option<Waiter>,
) -> std::io::Result<()> {
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
    .listen(listener)?
    .run();

    if let Some(stop) = stop {
        let handle = server.handle();
        tokio::spawn(async move {
            stop.wait().await;
            handle.stop(true).await;
        });
    }

    server.await
}

fn configure_routes(cfg: &mut web::ServiceConfig, capabilities: HttpApiCapabilities) {
    cfg.service(app_list)
        .service(app_proc_list)
        .service(app_proc_detail)
        .service(install);
    if capabilities.enable_invoke {
        cfg.service(invoke);
    }
    if capabilities.enable_uninstall {
        cfg.service(uninstall);
    }
}

#[get("/mudu/app/list")]
async fn app_list(context: web::Data<HttpApiContext>) -> impl Responder {
    match context.api.list_apps().await {
        Ok(list) => HttpResponse::Ok().json(serde_json::json!({
            "status": 0,
            "message": "ok",
            "data": list,
        })),
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": "fail to get app list",
            "data": e,
        })),
    }
}

#[get("/mudu/app/list/{app_name}")]
async fn app_proc_list(
    path: web::Path<String>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let app_name = path.into_inner();
    match context.api.list_procedures(&app_name).await {
        Ok(procedures) => HttpResponse::Ok().json(serde_json::json!({
            "status": 0,
            "message": "ok",
            "data": ProcedureList { app_name, procedures },
        })),
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": format!("fail to get procedure list of app {}", app_name),
            "data": e,
        })),
    }
}

#[get("/mudu/app/list/{app_name}/{mod_name}/{proc_name}")]
async fn app_proc_detail(
    path: web::Path<(String, String, String)>,
    context: web::Data<HttpApiContext>,
) -> impl Responder {
    let (app_name, mod_name, proc_name) = path.into_inner();
    match context
        .api
        .procedure_detail(&app_name, &mod_name, &proc_name)
        .await
    {
        Ok((desc, param_json_default, return_json_default)) => HttpResponse::Ok().json(
            serde_json::json!({
                "status": 0,
                "message": "ok",
                "data": {
                    "proc_desc": desc,
                    "param_default": param_json_default,
                    "return_default": return_json_default,
                },
            }),
        ),
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": format!("fail to get procedure {}/{}/{} detail ", app_name, mod_name, proc_name),
            "data": e,
        })),
    }
}

#[post("/mudu/app/install")]
async fn install(body: web::Bytes, context: web::Data<HttpApiContext>) -> impl Responder {
    let body_str = String::from_utf8_lossy(&body).to_string();
    match decode_install_request(&body_str) {
        Ok(binary) => match context.api.install_mpk(binary).await {
            Ok(()) => HttpResponse::Ok().json(serde_json::json!({
                "status": 0,
                "message": "ok",
                "data": JsonValue::Null,
            })),
            Err(e) => HttpResponse::Ok().json(serde_json::json!({
                "status": 1001,
                "message": format!("fail to install package {:?}", body_str),
                "data": e,
            })),
        },
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": format!("fail to install package {:?}", body_str),
            "data": e,
        })),
    }
}

#[delete("/mudu/app/uninstall/{app_name}")]
async fn uninstall(path: web::Path<String>, context: web::Data<HttpApiContext>) -> impl Responder {
    let app_name = path.into_inner();
    match context.api.uninstall_app(&app_name).await {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "status": 0,
            "message": "ok",
            "data": JsonValue::Null,
        })),
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": format!("fail to uninstall app {}", app_name),
            "data": e,
        })),
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
        Ok(value) => HttpResponse::Ok().json(serde_json::json!({
            "status": 0,
            "message": "ok",
            "data": value,
        })),
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": format!("fail to invoke procedure {}", proc),
            "data": e,
        })),
    }
}

fn decode_install_request(body_str: &str) -> RS<Vec<u8>> {
    let map = serde_json::from_str::<HashMap<String, String>>(body_str)
        .map_err(|e| m_error!(EC::DecodeErr, "deserialize body error: {}", e))?;
    let mpk_base64 = map
        .get("mpk_base64")
        .ok_or_else(|| m_error!(EC::NoneErr, "mpk_base64 missing for install request"))?;
    base64::engine::general_purpose::STANDARD
        .decode(mpk_base64)
        .map_err(|e| m_error!(EC::DecodeErr, "decode error", e))
}

fn to_param(argv: &Map<String, Value>, desc: &[DatumDesc]) -> RS<ProcedureParam> {
    let mut vec = vec![];
    for datum_desc in desc.iter() {
        let value = argv
            .get(datum_desc.name())
            .ok_or_else(|| {
                m_error!(
                    EC::NoSuchElement,
                    format!("no parameter {}", datum_desc.name())
                )
            })?
            .clone();
        let id = datum_desc.dat_type_id();
        let dat_value = id.fn_input_json()(&value, datum_desc.dat_type())
            .map_err(|e| m_error!(EC::TypeBaseErr, "convert printable to internal error", e))?;
        vec.push(dat_value)
    }
    Ok(ProcedureParam::new(0, 0, vec))
}

fn parse_json_object_body(body: &str) -> RS<Map<String, Value>> {
    let object: Value =
        serde_json::from_str(body).map_err(|e| m_error!(EC::DecodeErr, "deserialize error", e))?;
    match object {
        Value::Object(obj_map) => Ok(obj_map),
        _ => Err(m_error!(
            EC::DecodeErr,
            "request json body must be an object"
        )),
    }
}

async fn runtime_get_app_and_desc(
    service: Arc<dyn Runtime>,
    app_name: &str,
    mod_name: &str,
    proc_name: &str,
) -> RS<(Arc<dyn AppInst>, Arc<ProcDesc>)> {
    let app = service
        .app(app_name.to_string())
        .await
        .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app {}", app_name)))?;
    let desc = app.describe(&mod_name.to_string(), &proc_name.to_string())?;
    Ok((app, desc))
}

async fn legacy_invoke_sync_proc(
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
        .invoke(
            task_id,
            &mod_name.to_string(),
            &proc_name.to_string(),
            param,
            None,
        )
        .await?;
    Ok(procedure_invoke::result_to_json(result))
}

async fn legacy_invoke_async_proc(
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
        .invoke_async(
            task_id,
            &mod_name.to_string(),
            &proc_name.to_string(),
            param,
            None,
        )
        .await?;
    procedure_invoke::result_to_json(result)
}

#[cfg(target_os = "linux")]
async fn find_app(app_mgr: &dyn AppMgr, app_name: &str) -> RS<AppListItem> {
    let listed_apps = app_mgr
        .list(&ListOption {
            names: vec![app_name.to_string()],
        })
        .await?;
    listed_apps
        .apps
        .into_iter()
        .find(|app| app.info.name == app_name)
        .ok_or_else(|| m_error!(EC::NoneErr, format!("no such app {}", app_name)))
}

#[cfg(test)]
mod test {
    use super::*;
    use actix_web::{App, test};
    use mudu::common::app_info::AppInfo;
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use mudu_contract::procedure::procedure_result::ProcedureResult;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_kernel::server_ur::procedure_runtime::ProcInvoker;
    use std::sync::Mutex;

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

    #[cfg(target_os = "linux")]
    struct MockClient {
        session_id: u128,
        closed: bool,
        requests: Arc<Mutex<Vec<String>>>,
    }

    #[cfg(target_os = "linux")]
    #[async_trait(?Send)]
    impl AsyncIoUringInvokeClient for MockClient {
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
            self.closed = true;
            Ok(true)
        }
    }

    #[cfg(target_os = "linux")]
    struct MockClientFactory {
        requests: Arc<Mutex<Vec<String>>>,
    }

    #[cfg(target_os = "linux")]
    #[async_trait(?Send)]
    impl AsyncIoUringInvokeClientFactory for MockClientFactory {
        async fn connect(&self, _addr: &str) -> RS<Box<dyn AsyncIoUringInvokeClient>> {
            Ok(Box::new(MockClient {
                session_id: 9,
                closed: false,
                requests: self.requests.clone(),
            }))
        }
    }

    #[cfg(target_os = "linux")]
    struct MockAppMgr;

    #[cfg(target_os = "linux")]
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

        async fn create_invoker(&self, _cfg: &MuduDBCfg) -> RS<Arc<dyn ProcInvoker>> {
            Err(m_error!(EC::NotImplemented, "unused in test"))
        }
    }

    #[cfg(target_os = "linux")]
    #[actix_web::test]
    async fn iouring_http_api_invokes_over_bridge() {
        let requests = Arc::new(Mutex::new(Vec::new()));
        let api = IoUringHttpApi::with_client_factory(
            Arc::new(MockAppMgr),
            "127.0.0.1:9527".to_string(),
            Arc::new(MockClientFactory {
                requests: requests.clone(),
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
}
