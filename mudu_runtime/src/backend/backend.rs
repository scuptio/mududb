use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::app_inst::AppInst;
use crate::service::service::Service;
use crate::service::service_impl::create_runtime_service;
use actix_web::{post, web, App, HttpResponse, HttpServer, Responder};
use mudu::common::id::gen_oid;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::tuple::dat_printable::DatPrintable;
use mudu::tuple::datum_desc::DatumDesc;
use mudu_utils::notifier::Notifier;
use mudu_utils::task::spawn_local_task;
use std::collections::HashMap;
use std::sync::Arc;
use std::thread;
use tokio::task::LocalSet;
use tracing::{debug, error, info};

pub struct Backend {}

impl Backend {
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let ls = LocalSet::new();
        let notifier = Notifier::new();
        let mut builder = tokio::runtime::Builder::new_current_thread();
        builder
            .enable_all()
            .build()
            .map_err(
                |e| {
                    m_error!(EC::IOErr, "build runtime error", e)
                })?
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
                    }).unwrap();
                });
                ls.await;
                Ok(())
            })
    }

    pub async fn serve(cfg: MuduDBCfg) -> RS<()> {
        info!("starting backend server");
        info!("{}", cfg);
        let service = create_runtime_service(
            &cfg.bytecode_path,
            &cfg.db_path,
        )?;
        info!("runtime service initialized");
        Backend::web_serve(service, &cfg).await.map_err(|e| {
            m_error!(EC::IOErr, "backend run error", e)
        })
    }

    async fn web_serve(service: Arc<dyn Service>, cfg: &MuduDBCfg) -> std::io::Result<()> {
        let data = web::Data::new(AppContext {
            conn_str: format!("db={} ddl={} db_type=LibSQL", cfg.db_path, cfg.db_path),
            service,
        });
        info!("web service start");
        HttpServer::new(move || {
            App::new()
                .app_data(data.clone())
                .service(invoke_proc1)
                .service(invoke_proc2)
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
            Some(t) => { t.clone() }
            None => {
                return Err(m_error!(EC::NoSuchElement, format!("no parameter {}", datum_desc.name())));
            }
        };
        let id = datum_desc.dat_type_id();
        let internal = id.fn_input()
            (&DatPrintable::from(value), datum_desc.param_obj())
            .map_err(|e| { m_error!(EC::ConvertErr, "", e) })?;
        let dat = id.fn_send()(&internal, datum_desc.param_obj())
            .map_err(|e| { m_error!(EC::ConvertErr, "", e) })?;
        vec.push(dat.into())
    }
    Ok(ProcParam::new(
        gen_oid(),
        vec,
    ))
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
            m_error!(EC::IOErr, format!("async_invoke_proc_inner send error {:?}", e))
        })
    });
    let ret = receiver.await
        .map_err(|e| {
            m_error!(EC::IOErr, format!("async_invoke_proc_inner recv error {:?}", e))
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
        .map_err(|e|
            m_error!(EC::IOErr, "runtime build error", e)
        )?;
    let ret = runtime.block_on(async move {
        let app = service.app(&app_name)
            .ok_or(m_error!(EC::NoneErr, format!("no such app {}", &app_name)))?;
        let desc = app.describe(&mod_name, &proc_name)?;
        let param = to_param(
            &argv,
            desc.param_desc().fields(),
        )?;
        let thread = thread::spawn(move || {
            let ret = invoke_proc_inner(app, mod_name, proc_name, param, desc);
            ret
        });
        let ret = thread.join().map_err(|_e| {
            m_error!(EC::IOErr, "invoke_proc_inner thread error")
        })?;
        ret
    });
    Ok(ret)
}

fn invoke_proc_inner(
    service: Arc<dyn AppInst>,
    mod_name: String,
    proc_name: String,
    param: ProcParam,
    desc: Arc<ProcDesc>,
) -> RS<Vec<String>> {
    let result = service.invoke(&mod_name, &proc_name, param)?;
    let ret = result.to_string(desc.return_desc())?;
    ret
}

#[post("/mudu/{app_name}/{mod_name}/{proc_name}")]
async fn invoke_proc1(
    path: web::Path<(String, String, String)>,
    argv: web::Json<HashMap<String, String>>,
    context: web::Data<AppContext>,
) -> impl Responder {
    handle_invoke_proc(path, argv, context).await
}

#[post("/mudu/{app_name}/{mod_name}/{proc_name}/")]
async fn invoke_proc2(
    path: web::Path<(String, String, String)>,
    argv: web::Json<HashMap<String, String>>,
    context: web::Data<AppContext>,
) -> impl Responder {
    handle_invoke_proc(path, argv, context).await
}

async fn handle_invoke_proc(
    path: web::Path<(String, String, String)>,
    argv: web::Json<HashMap<String, String>>,
    context: web::Data<AppContext>,
) -> impl Responder {
    let (app_name, mod_name, proc_name) = path.into_inner();
    let name = format!("{}/{}/{}", app_name, mod_name, proc_name);
    debug!("invoke procedure: {} <{:?}>", name, argv);
    let r = async_invoke_proc(
        context.conn_str.clone(),
        app_name,
        mod_name,
        proc_name,
        argv.to_owned(),
        context.service.clone(),
    ).await;
    HttpResponse::Ok()
        .json(serde_json::json!({
            "status": "success",
            "message": format!("invoke procedure {}, result <{:?}>", name, r),
        }))
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
            bytecode_path: wasm_mod_path(),
            db_path: wasm_mod_path(),
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
            match TcpStream::connect_timeout(
                &addr,
                Duration::from_secs(5)) {
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
            fe_request(
                localhost,
                cfg.listen_port,
                "app1/mod_0/proc/",
                &param,
            ).await?;
        }
        Ok(())
    }

    fn url_prefix(ip: &str, port: u16) -> String {
        format!("http://{}:{}/mudu", ip, port)
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
            .map_err(|e| { m_error!(EC::IOErr, "fe request run error", e) })?;

        if response.status().is_success() {
            let map = response.json::<HashMap<String, String>>()
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
            spawn_local_task(nd, "debug",
                             async move {
                                 async_debug_serve(([0, 0, 0, 0], 3300).into()).await
                             })
        });
        ls.spawn_local(async move {
            let res = spawn_local_task(
                n1, "backend",
                async move {
                    let ret = run_backend().await;
                    match &ret {
                        Ok(()) => {}
                        Err(e) => {
                            error!("backend run error: {}", e);
                        }
                    }
                },
            );
            match res {
                Ok(j) => {
                    let _r = j.await;
                    Ok(())
                }
                Err(e) => { Err(e) }
            }
        });

        ls.spawn_local(async move {
            let res = spawn_local_task(
                n2, "frontend",
                async move {
                    let ret = run_frontend().await;
                    match &ret {
                        Ok(()) => {}
                        Err(e) => {
                            error!("frontend run error: {}", e);
                        }
                    }
                    notifier.notify_all(); // end of this program
                    ret
                },
            );
            match res {
                Ok(j) => {
                    let _r = j.await;
                    Ok(())
                }
                Err(e) => { Err(e) }
            }
        });
        runtime.block_on(ls);
        Ok(())
    }
}