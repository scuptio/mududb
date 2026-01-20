use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_inst::AppInst;
use crate::service::runtime::Runtime;
use crate::service::runtime_impl::create_runtime_service;
use crate::service::runtime_opt::RuntimeOpt;
use actix_cors::Cors;
use actix_web::http::StatusCode;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use base64::Engine;
use mudu::common::id::gen_oid;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::JsonValue;
use mudu_binding::procedure::procedure_invoke;
use mudu_contract::procedure::proc_desc::ProcDesc;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_utils::notifier::Notifier;
use mudu_utils::task_id::TaskID;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::HashMap;
use std::env::temp_dir;
use std::fs;
use std::sync::Arc;
use tracing::{debug, error, info};

#[derive(Serialize, Deserialize)]
struct ProcedureList {
    app_name: String,
    procedures: Vec<String>,
}

async fn web_serve(service: Arc<dyn Runtime>, cfg: &MuduDBCfg) -> std::io::Result<()> {
    let payload_limit = 500 * 1024 * 1024;
    let data = web::Data::new(AppContext {
        service,
    });
    info!("web service start");

    // register all service urls
    HttpServer::new(move || {
        // todo configuration for cors
        let cors = Cors::permissive();
        App::new()
            .wrap(cors)
            .app_data(data.clone())
            // Configure JSON payload limits
            .app_data(
                web::JsonConfig::default()
                    .limit(payload_limit) // for JSON payloads
                    .content_type_required(false)
                    .error_handler(|err, req| {
                        error!("JSON payload error: {} for path: {}", err, req.path());
                        actix_web::error::InternalError::new(
                            err,
                            StatusCode::INTERNAL_SERVER_ERROR,
                        )
                            .into()
                    }),
            )
            // Configure general payload limits (required alongside JsonConfig)
            .app_data(
                web::PayloadConfig::default().limit(payload_limit), // overall payload limit
            )
            // Configure form payload limits
            .app_data(
                web::FormConfig::default().limit(payload_limit), // for form data
            )
            .wrap(actix_web::middleware::Logger::default())
            .service(app_list)
            .service(app_proc_list)
            .service(app_proc_detail)
            .service(invoke)
            .service(install)
    })
        .bind(format!("{}:{}", cfg.listen_ip, cfg.http_listen_port))?
        .run()
        .await?;
    info!("backend server terminated");
    Ok(())
}

pub async fn async_serve(cfg: MuduDBCfg, opt_initialized_notifier: Option<Notifier>) -> RS<()> {
    info!("starting backend server");
    info!("{}", cfg);
    let enable_async = cfg.enable_async && cfg.enable_p2;
    let runtime_opt = RuntimeOpt { enable_p2: cfg.enable_p2, enable_async };
    let service = create_runtime_service(&cfg.mpk_path, &cfg.data_path, opt_initialized_notifier, runtime_opt).await?;
    info!("runtime service initialized");
    web_serve(service, &cfg)
        .await
        .map_err(|e| m_error!(EC::IOErr, "backend run error", e))
}

fn to_param(argv: &Map<String, Value>, desc: &[DatumDesc]) -> RS<ProcedureParam> {
    let mut vec = vec![];
    for (_n, datum_desc) in desc.iter().enumerate() {
        let opt_name = argv.get(datum_desc.name());
        let value = match opt_name {
            Some(t) => t.clone(),
            None => {
                return Err(m_error!(
                    EC::NoSuchElement,
                    format!("no parameter {}", datum_desc.name())
                ));
            }
        };
        let id = datum_desc.dat_type_id();
        let dat_value = id.fn_input_json()(&value, datum_desc.dat_type())
            .map_err(|e| m_error!(EC::TypeBaseErr, "convert printable to internal error", e))?;
        vec.push(dat_value)
    }
    let proc_param = ProcedureParam::new(0, 0, vec);
    Ok(proc_param)
}

#[derive(Clone)]
struct AppContext {
    service: Arc<dyn Runtime>,
}

unsafe impl Send for AppContext {}

unsafe impl Sync for AppContext {}

async fn async_invoke_sync_proc(
    mod_name: String,
    proc_name: String,
    argv: Map<String, Value>,
    app: Arc<dyn AppInst>,
    desc: Arc<ProcDesc>,
) -> RS<Value> {
    let ret = sync_invoke_proc(mod_name, proc_name, argv, app, desc).await?;
    ret
}

async fn service_get_app_and_desc(
    service: Arc<dyn Runtime>,
    app_name: &String,
    mod_name: &String,
    proc_name: &String,
) -> RS<(Arc<dyn AppInst>, Arc<ProcDesc>)> {
    let opt_app = service.app(app_name.clone()).await;
    let app = if let Some(app) = opt_app {
        app
    } else {
        return Err(m_error!(EC::NoneErr, format!("no such app {}", app_name)));
    };
    let desc = app.describe(&mod_name, &proc_name)?;
    Ok((app, desc))
}

async fn sync_invoke_proc(
    mod_name: String,
    proc_name: String,
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
    let ret = invoke_proc_inner(task_id, app, mod_name, proc_name, param, desc).await;
    Ok(ret)
}


async fn async_invoke_async_proc(
    mod_name: String,
    proc_name: String,
    argv: Map<String, Value>,
    app: Arc<dyn AppInst>,
    desc: Arc<ProcDesc>,
) -> RS<JsonValue> {
    let task_id = app.task_create().await?;
    let _g = scopeguard::guard(task_id, |task_id| {
        let _r = app.task_end(task_id);
    });
    let param = to_param(&argv, desc.param_desc().fields())?;
    let result = app.invoke_async(
        task_id,
        &mod_name,
        &proc_name,
        param,
    ).await?;
    let r = procedure_invoke::result_to_json(result)?;
    Ok(r)
}

async fn invoke_proc_inner(
    task_id: TaskID,
    service: Arc<dyn AppInst>,
    mod_name: String,
    proc_name: String,
    param: ProcedureParam,
    _: Arc<ProcDesc>,
) -> RS<JsonValue> {
    let result = service.invoke(task_id, &mod_name, &proc_name, param).await?;
    let r = procedure_invoke::result_to_json(result)?;
    Ok(r)
}

#[get("/mudu/app/list")]
async fn app_list(context: web::Data<AppContext>) -> impl Responder {
    let result = handle_app_list(context.service.as_ref()).await;
    match result {
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
async fn app_proc_list(path: web::Path<String>, context: web::Data<AppContext>) -> impl Responder {
    let app_name = path.into_inner();
    let result = handle_procedure_list(&app_name, context.service.as_ref()).await;

    match result {
        Ok(procedures) => {
            let procedure_list = ProcedureList {
                app_name,
                procedures,
            };
            HttpResponse::Ok().json(serde_json::json!({
                "status": 0,
                "message": "ok",
                "data": procedure_list,
            }))
        }
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
    context: web::Data<AppContext>,
) -> impl Responder {
    let (app_name, mod_name, proc_name) = path.into_inner();
    let result =
        handle_procedure_detail(&app_name, &mod_name, &proc_name, context.service.as_ref()).await;
    match result {
        Ok((desc, param_json_default, return_json_default)) => {
            HttpResponse::Ok()
                .json(serde_json::json!({
                    "status": 0,
                    "message": "ok",
                    "data": {
                        "proc_desc":desc,
                        "param_default":param_json_default,
                        "return_default":return_json_default
                    },
                }))
        }
        Err(e) => {
            HttpResponse::Ok()
                .json(serde_json::json!({
                "status": 1001,
                "message": format!("fail to get procedure {}/{}/{} detail ", app_name, mod_name, proc_name),
                "data": e,
            }))
        }
    }
}

#[post("/mudu/app/install")]
async fn install(
    _req: HttpRequest,
    body: web::Bytes,
    context: web::Data<AppContext>,
) -> impl Responder {
    let body_str = String::from_utf8_lossy(&body).to_string();
    let result = handle_install(body_str.clone(), context.service.as_ref()).await;
    match result {
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
    }
}

#[post("/mudu/app/invoke/{app_name}/{mod_name}/{proc_name}")]
async fn invoke(
    req: HttpRequest,
    body: web::Bytes,
    context: web::Data<AppContext>,
) -> impl Responder {
    let app_name = req.match_info().get("app_name").unwrap().to_string();
    let mod_name = req.match_info().get("mod_name").unwrap().to_string();
    let proc_name = req.match_info().get("proc_name").unwrap().to_string();
    let body_str = String::from_utf8_lossy(&body).to_string();
    let proc = format!("{}/{}/{}", app_name, mod_name, proc_name);
    let result = handle_invoke_proc(
        app_name,
        mod_name,
        proc_name,
        body_str,
        context.service.clone(),
    ).await;
    match result {
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

async fn handle_app_list(service: &dyn Runtime) -> RS<Vec<String>> {
    let list = service.list().await;
    Ok(list)
}

async fn handle_procedure_list(app_name: &String, service: &dyn Runtime) -> RS<Vec<String>> {
    let procedure_list = if let Some(app) = service.app(app_name.clone()).await {
        app.procedure()?
    } else {
        Vec::new()
    };
    Ok(procedure_list
        .iter()
        .map(|e| format!("{}/{}", e.0, e.1))
        .collect())
}

async fn handle_procedure_detail(
    app_name: &String,
    mod_name: &String,
    proc_name: &String,
    service: &dyn Runtime,
) -> RS<(ProcDesc, JsonValue, JsonValue)> {
    if let Some(app) = service.app(app_name.clone()).await {
        let desc = app.describe(mod_name, proc_name)?;
        let proc_desc = desc.as_ref().clone();
        let param_json = desc.as_ref().default_param_json()?;
        let return_json = desc.as_ref().default_return_json()?;
        Ok((proc_desc, param_json, return_json))
    } else {
        Err(m_error!(
            EC::NoneErr,
            format!("procedure detail error, no such app {}", app_name)
        ))
    }
}

async fn handle_install(body_str: String, service: &dyn Runtime) -> RS<()> {
    let map = serde_json::from_str::<HashMap<String, String>>(&body_str)
        .map_err(|e| m_error!(EC::DecodeErr, "deserialize body error: {}", e))?;
    let mpk_base64 = map
        .get("mpk_base64")
        .ok_or_else(|| m_error!(EC::NoneErr, "mpk_base64 missing for install request"))?;
    let binary = base64::engine::general_purpose::STANDARD
        .decode(mpk_base64)
        .map_err(|e| m_error!(EC::DecodeErr, "decode error", e))?;
    let temp_mpk_file = temp_dir().join(format!("{:x}.mpk", gen_oid()));
    fs::write(&temp_mpk_file, &binary).map_err(|e| m_error!(EC::NoneErr, "write error", e))?;
    let file_path = temp_mpk_file
        .as_path()
        .to_str()
        .ok_or_else(|| m_error!(EC::NoneErr, "cannot get string of PathBuf"))?
        .to_string();
    service.install(file_path).await
}

async fn handle_invoke_proc(

    app_name: String,
    mod_name: String,
    proc_name: String,
    body: String,
    context: Arc<dyn Runtime>,
) -> RS<Value> {
    let object: Value = serde_json::from_str(&body)
        .map_err(|e| m_error!(EC::DecodeErr, "deserialize error", e))?;
    let map = match object {
        Value::Object(obj_map) => { obj_map }
        _ => {
            return Err(m_error!(EC::DecodeErr, "request json body must be an object"));
        }
    };
    let name = format!("{}/{}/{}", app_name, mod_name, proc_name);
    debug!("invoke procedure: {} <{:?}>", name, map);
    let (app, desc) = service_get_app_and_desc(context, &app_name, &mod_name, &proc_name).await?;
    if app.cfg().use_async {
        async_invoke_async_proc(mod_name, proc_name, map, app, desc).await
    } else {
        async_invoke_sync_proc(mod_name, proc_name, map, app, desc).await
    }
}

#[allow(unused)]
#[cfg(test)]
mod test {
    use crate::backend::mududb_cfg::MuduDBCfg;
    use crate::backend::web_serve::async_serve;
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::{json_value, m_error};
    use mudu_utils::debug::async_debug_serve;
    use mudu_utils::log::log_setup_ex;
    use mudu_utils::notifier::NotifyWait;
    use mudu_utils::task::spawn_local_task;
    use reqwest;

    use std::env::temp_dir;
    use std::fs;
    use std::net::{SocketAddr, TcpStream};
    use std::str::FromStr;
    use std::time::Duration;

    use mudu::utils::json::JsonValue;
    use tokio::task::LocalSet;
    use tracing::{error, info};

    fn test_db_path() -> String {
        let tmp = temp_dir().join(format!("test_db_{}", uuid::Uuid::new_v4().to_string()));
        if !tmp.as_path().exists() {
            fs::create_dir_all(tmp.as_path()).unwrap();
        }
        tmp.to_str().unwrap().to_string()
    }

    //#[test]
    fn test() {
        log_setup_ex("info", "mudu_runtime=debug", false);
        let _ = run_test();
    }

    fn _cfg() -> MuduDBCfg {
        let cfg = MuduDBCfg {
            mpk_path: wasm_mod_path(),
            data_path: test_db_path(),
            listen_ip: "0.0.0.0".to_string(),
            http_listen_port: 8000,
            pg_listen_port: 5432,
            enable_p2: true,
            enable_async: true,
        };
        cfg
    }
    async fn run_backend() -> RS<()> {
        let cfg = _cfg();
        async_serve(cfg, None).await
    }

    async fn wait_service_start(ip: &str, port: u16) -> RS<()> {
        let addr = SocketAddr::from_str(&format!("{}:{}", ip, port))
            .map_err(|e| m_error!(EC::ParseErr, "parse ip error", e))?;
        loop {
            match TcpStream::connect_timeout(&addr, Duration::from_secs(5)) {
                Ok(_) => return Ok(()),
                Err(_) => {
                    continue;
                }
            }
        }
    }

    async fn run_frontend() -> RS<()> {
        let cfg = _cfg();
        let localhost = "127.0.0.1";
        wait_service_start(localhost, cfg.http_listen_port).await?;
        for i in 0..5 {
            let param = json_value!({
                "a":i,
                "b":i,
                "c":format!("\"{}\"", i)
            });
            fe_request(localhost, cfg.http_listen_port, "app1", "mod_0", "proc", &param).await?;
        }
        Ok(())
    }

    fn url_prefix(ip: &str, port: u16) -> String {
        format!("http://{}:{}/mudu/app/invoke", ip, port)
    }

    async fn fe_request(
        ip: &str,
        port: u16,
        app_name: &str,
        mod_name: &str,
        fn_proc: &str,
        param: &JsonValue,
    ) -> RS<()> {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/{}/{}/{}", url_prefix(ip, port), app_name, mod_name, fn_proc))
            .json(param)
            .send()
            .await
            .map_err(|e| m_error!(EC::IOErr, "fe request run error", e))?;
        assert!(response.status().is_success());
        if response.status().is_success() {
            let map = response
                .json::<JsonValue>()
                .await
                .map_err(|e| m_error!(EC::DecodeErr, "fe request decode response error", e))?;
            info!("{map:#?}");
        } else {
            error!("fe request failed, response status: {}", response.status());
        }

        Ok(())
    }

    fn run_test() -> RS<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let ls = LocalSet::new();
        let notifier = NotifyWait::default();
        let n1 = notifier.clone();
        let n2 = notifier.clone();
        let nd = notifier.clone();

        ls.spawn_local(async move {
            spawn_local_task(nd, "debug", async move {
                async_debug_serve(([0, 0, 0, 0], 3300).into()).await
            })
        });
        ls.spawn_local(async move {
            let res = spawn_local_task(n1, "backend", async move {
                let ret = run_backend().await;
                match &ret {
                    Ok(()) => {}
                    Err(e) => {
                        error!("backend run error: {}", e);
                    }
                }
            });
            match res {
                Ok(j) => {
                    let _r = j.await;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        });

        let _ = ls.spawn_local(async move {
            let res = spawn_local_task(n2, "frontend", async move {
                let ret = run_frontend().await;
                match &ret {
                    Ok(()) => {}
                    Err(e) => {
                        error!("frontend run error: {}", e);
                    }
                }
                notifier.notify_all(); // end of this program
                ret
            });
            assert!(res.is_ok());
            match res {
                Ok(j) => {
                    let _r = j.await;
                    Ok(())
                }
                Err(e) => Err(e),
            }
        });
        runtime.block_on(ls);
        Ok(())
    }
}
