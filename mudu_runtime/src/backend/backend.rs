use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_inst::AppInst;
use crate::service::service::Service;
use crate::service::service_impl::create_runtime_service;
use actix_web::{get, post, web, App, HttpRequest, HttpResponse, HttpServer, Responder};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::tuple::dat_printable::DatPrintable;
use mudu::tuple::datum_desc::DatumDesc;
use mudu_utils::notifier::Notifier;
use mudu_utils::task::spawn_local_task;
use mudu_utils::task_id::TaskID;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::{fs, thread};
use std::env::temp_dir;
use actix_web::http::StatusCode;
use base64::Engine;
use tokio::task::LocalSet;
use tracing::{debug, error, info};
use mudu::common::id::gen_oid;

pub struct Backend {}

#[derive(Serialize, Deserialize)]
struct ProcedureList {
    app_name:String,
    procedures:Vec<String>,
}

impl Backend {
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let ls = LocalSet::new();
        let notifier = Notifier::new();
        let mut builder = tokio::runtime::Builder::new_current_thread();
        builder
            .enable_all()
            .build()
            .map_err(|e| m_error!(EC::IOErr, "build runtime error", e))?
            .block_on(async {
                ls.spawn_local(async move {
                    spawn_local_task(notifier, "", async move {
                        let r = Backend::serve(cfg).await;
                        match r {
                            Ok(_) => {}
                            Err(e) => {
                                error!("backend serve error: {}", e);
                            }
                        }
                    })
                    .unwrap();
                });
                ls.await;
                Ok(())
            })
    }

    pub async fn serve(cfg: MuduDBCfg) -> RS<()> {
        info!("starting backend server");
        info!("{}", cfg);
        let service = create_runtime_service(&cfg.mpk_path, &cfg.data_path)?;
        info!("runtime service initialized");
        Backend::web_serve(service, &cfg)
            .await
            .map_err(|e| m_error!(EC::IOErr, "backend run error", e))
    }

    async fn web_serve(service: Arc<dyn Service>, cfg: &MuduDBCfg) -> std::io::Result<()> {
        let payload_limit = 500 * 1024 * 1024;
        let data = web::Data::new(AppContext {
            conn_str: format!("db={} ddl={} db_type=LibSQL", cfg.data_path, cfg.data_path),
            service,
        });
        info!("web service start");
        // register all service urls
        HttpServer::new(move || {
            App::new()
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
                            ).into()
                        })
                )
                // Configure general payload limits (required alongside JsonConfig)
                .app_data(
                    web::PayloadConfig::default()
                        .limit(payload_limit) // overall payload limit
                )
                // Configure form payload limits
                .app_data(
                    web::FormConfig::default()
                        .limit(payload_limit) // for form data
                )
                .wrap(actix_web::middleware::Logger::default())
                .service(app_list)
                .service(app_proc_list)
                .service(app_proc_detail)
                .service(invoke)
                .service(install)
        })
        .bind(format!("{}:{}", cfg.listen_ip, cfg.listen_port))?
        .run()
        .await?;
        info!("backend server terminated");
        Ok(())
    }
}

fn to_param(argv: &HashMap<String, String>, desc: &[DatumDesc]) -> RS<ProcParam> {
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
        let internal = id.fn_input()(&DatPrintable::from(value), datum_desc.param_obj())
            .map_err(|e| m_error!(EC::TypeBaseErr, "", e))?;
        let dat = id.fn_send()(&internal, datum_desc.param_obj())
            .map_err(|e| m_error!(EC::TypeBaseErr, "", e))?;
        vec.push(dat.into())
    }
    Ok(ProcParam::new(0, vec))
}

#[derive(Clone)]
struct AppContext {
    conn_str: String,
    service: Arc<dyn Service>,
}

unsafe impl Send for AppContext {}

unsafe impl Sync for AppContext {}

async fn async_invoke_proc(
    conn_str: String,
    app_name: String,
    mod_name: String,
    proc_name: String,
    argv: HashMap<String, String>,
    service: Arc<dyn Service>,
) -> RS<RS<Vec<String>>> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    // create a thread
    // to avoid to start a runtime from within a runtime
    // FIXME, change to asynchronous call
    thread::spawn(move || {
        let ret = sync_invoke_proc(conn_str, app_name, mod_name, proc_name, argv, service);
        sender.send(ret).map_err(|e| {
            m_error!(
                EC::IOErr,
                format!("async_invoke_proc_inner send error {:?}", e)
            )
        })
    });
    let ret = receiver.await.map_err(|e| {
        m_error!(
            EC::IOErr,
            format!("async_invoke_proc_inner recv error {:?}", e)
        )
    })?;
    ret
}

fn sync_invoke_proc(
    _conn_str: String,
    app_name: String,
    mod_name: String,
    proc_name: String,
    argv: HashMap<String, String>,
    service: Arc<dyn Service>,
) -> RS<RS<Vec<String>>> {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| m_error!(EC::IOErr, "runtime build error", e))?;
    let ret = runtime.block_on(async move {
        let opt_app = service
            .app(&app_name);
        let app = if let Some(app) = opt_app {
            app
        } else {
            return Err(m_error!(EC::NoneErr, format!("no such app {}", &app_name)))
        };
        let task_id = app.task_create()?;
        let _app = app.clone();
        let _g = scopeguard::guard(task_id, |task_id| {
            let _r = _app.task_end(task_id);
        });

        let desc = app.describe(&mod_name, &proc_name)?;
        let param = to_param(&argv, desc.param_desc().fields())?;
        let thread = thread::spawn(move || {
            let ret = invoke_proc_inner(task_id, app, mod_name, proc_name, param, desc);
            ret
        });
        let ret = thread
            .join()
            .map_err(|_e| m_error!(EC::IOErr, "invoke_proc_inner thread error"))?;
        ret
    });
    Ok(ret)
}

fn invoke_proc_inner(
    task_id: TaskID,
    service: Arc<dyn AppInst>,
    mod_name: String,
    proc_name: String,
    param: ProcParam,
    desc: Arc<ProcDesc>,
) -> RS<Vec<String>> {
    let result = service.invoke(task_id, &mod_name, &proc_name, param)?;
    let ret = result.to_string(desc.return_desc())?;
    ret
}

#[get("/mudu/app/list")]
async fn app_list(context: web::Data<AppContext>) -> impl Responder {
    let result = handle_app_list(context.service.as_ref());
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
    let result = handle_procedure_list(&app_name, context.service.as_ref());

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
        },
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
        handle_procedure_detail(&app_name, &mod_name, &proc_name, context.service.as_ref());
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
    let result = handle_install(body_str.clone(), context.service.as_ref());
    match result {
        Ok(()) => HttpResponse::Ok().json(serde_json::json!({
            "status": 0,
            "message": "ok",
            "data": serde_json::Value::Null,
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
        context.conn_str.clone(),
        app_name,
        mod_name,
        proc_name,
        body_str,
        context.service.clone(),
    ).await;
    match result {
        Ok(vec) => HttpResponse::Ok().json(serde_json::json!({
            "status": 0,
            "message": "ok",
            "data": vec,
        })),
        Err(e) => HttpResponse::Ok().json(serde_json::json!({
            "status": 1001,
            "message": format!("fail to invoke procedure {}", proc),
            "data": e,
        })),
    }
}



fn handle_app_list(service: &dyn Service) -> RS<Vec<String>> {
    let list = service.list();
    Ok(list)
}

fn handle_procedure_list(app_name: &String, service: &dyn Service) -> RS<Vec<String>> {
    let procedure_list = if let Some(app) = service.app(app_name) {
        app.procedure()?
    } else {
        Vec::new()
    };
    Ok(procedure_list
        .iter()
        .map(|e| format!("{}/{}", e.0, e.1))
        .collect())
}

fn handle_procedure_detail(
    app_name: &String,
    mod_name: &String,
    proc_name: &String,
    service: &dyn Service,
) -> RS<(ProcDesc, serde_json::Value, serde_json::Value)> {
    if let Some(app) = service.app(app_name) {
        let desc = app.describe(mod_name, proc_name)?;
        let proc_desc = desc.as_ref().clone();
        let param_json = desc.as_ref().default_param_json()?;
        let param_json_val = serde_json::from_str(&param_json)
            .map_err(|e| m_error!(EC::DecodeErr, "deserialize error", e))?;
        let return_json = desc.as_ref().default_return_json()?;
        let return_json_val = serde_json::from_str(&return_json)
            .map_err(|e| m_error!(EC::DecodeErr, "deserialize error", e))?;
        Ok((proc_desc, param_json_val, return_json_val))
    } else {
        Err(m_error!(
            EC::NoneErr,
            format!("procedure detail error, no such app {}", app_name)
        ))
    }
}

fn handle_install(body_str: String, service: &dyn Service) -> RS<()> {
    let map = serde_json::from_str::<HashMap<String, String>>(&body_str)
        .map_err(|e| { m_error!(EC::DecodeErr, "deserialize body error: {}", e) })?;
    let mpk_base64 = map
        .get("mpk_base64")
        .ok_or_else(|| m_error!(EC::NoneErr, "mpk_base64 missing for install request"))?;
    let binary = base64::engine::general_purpose::STANDARD.decode(mpk_base64)
        .map_err(|e| { m_error!(EC::DecodeErr, "decode error", e) })?;
    let temp_mpk_file = temp_dir().join(format!("{:x}.mpk", gen_oid()));
    fs::write(&temp_mpk_file, &binary)
        .map_err(|e| { m_error!(EC::NoneErr, "write error", e) })?;
    let file_path = temp_mpk_file.as_path().to_str()
        .ok_or_else(|| {m_error!(EC::NoneErr, "cannot get string of PathBuf")})?.to_string();
    service.install(&file_path)
}

async fn handle_invoke_proc(
    conn_string:String,
    app_name:String,
    mod_name:String,
    proc_name:String,
    body:String,
    context: Arc<dyn Service>,
) -> RS<Vec<String>> {
    let map = serde_json::from_str::<HashMap<String, String>>(&body)
        .map_err(|e| m_error!(EC::DecodeErr, "deserialize error", e))?;
    let name = format!("{}/{}/{}", app_name, mod_name, proc_name);
    debug!("invoke procedure: {} <{:?}>", name, map);
    async_invoke_proc(
        conn_string,
        app_name,
        mod_name,
        proc_name,
        map,
        context,
    )
    .await?
}
#[cfg(test)]
mod test {
    use crate::backend::backend::Backend;
    use crate::backend::mududb_cfg::MuduDBCfg;
    use crate::service::test_wasm_mod_path::wasm_mod_path;
    use mudu::common::result::RS;
    use mudu::error::ec::EC;
    use mudu::m_error;
    use mudu_utils::debug::async_debug_serve;
    use mudu_utils::log::log_setup_ex;
    use mudu_utils::notifier::Notifier;
    use mudu_utils::task::spawn_local_task;
    use reqwest;
    use std::collections::HashMap;
    use std::net::{SocketAddr, TcpStream};
    use std::str::FromStr;
    use std::time::Duration;
    use tokio::task::LocalSet;
    use tracing::{error, info};

    #[test]
    fn test() {
        log_setup_ex("info", "mudu_runtime=debug", false);
        let _ = run_test();
    }

    fn _cfg() -> MuduDBCfg {
        let cfg = MuduDBCfg {
            mpk_path: wasm_mod_path(),
            data_path: wasm_mod_path(),
            listen_ip: "0.0.0.0".to_string(),
            listen_port: 8000,
        };
        cfg
    }
    async fn run_backend() -> RS<()> {
        let cfg = _cfg();
        Backend::serve(cfg).await
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
        wait_service_start(localhost, cfg.listen_port).await?;
        for i in 0..5 {
            let mut param = HashMap::new();
            param.insert("a".to_string(), i.to_string());
            param.insert("b".to_string(), i.to_string());
            param.insert("c".to_string(), format!("\"{}\"", i));
            fe_request(localhost, cfg.listen_port, "app1/mod_0/proc/", &param).await?;
        }
        Ok(())
    }

    fn url_prefix(ip: &str, port: u16) -> String {
        format!("http://{}:{}/mudu/app/invoke", ip, port)
    }

    async fn fe_request(
        ip: &str,
        port: u16,
        fn_proc: &str,
        param: &HashMap<String, String>,
    ) -> RS<()> {
        let client = reqwest::Client::new();
        let response = client
            .post(format!("{}/{}", url_prefix(ip, port), fn_proc))
            .json(param)
            .send()
            .await
            .map_err(|e| m_error!(EC::IOErr, "fe request run error", e))?;

        if response.status().is_success() {
            let map = response
                .json::<HashMap<String, String>>()
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
        let notifier = Notifier::default();
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

        ls.spawn_local(async move {
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
