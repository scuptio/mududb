use super::handlers::{
    app_list, app_proc_detail, app_proc_list, backup, backup_delete, backup_list, backup_verify,
    dump_logical, install, invoke, partition_merge, partition_move, partition_move_abort,
    partition_route, partition_split, queue_activation_register, queue_activation_unregister,
    repl_follow, repl_membership, repl_promote, restore_logical, server_queue_activations,
    server_queues, server_replication, server_topology, uninstall,
};
use super::{HttpApi, HttpApiCapabilities, HttpApiContext};
use crate::backend::mudud_cfg::MuduDBCfg;
use actix_cors::Cors;
use actix_web::http::StatusCode;
use actix_web::{App, HttpServer, web};
use mudu::common::result::RS;
use mudu_sys::net::sync::StdTcpListener;
use mudu_utils::notifier::Waiter;
use mudu_utils::scoped_task_trace;
use serde_json::Value;
use std::sync::Arc;
use tracing::error;
use ws_gateway::{WsBackend, WsGateway};

/// Adapts the [`HttpApi`] surface to the WebSocket gateway's narrower
/// [`WsBackend`] view (procedure invocation + queue receive). The adapter
/// keeps the gateway free of a `mudu_runtime` dependency, which would
/// otherwise be circular.
struct HttpApiWsAdapter(Arc<dyn HttpApi>);

#[async_trait::async_trait(?Send)]
impl WsBackend for HttpApiWsAdapter {
    async fn invoke_json(&self, app: &str, module: &str, proc: &str, body: String) -> RS<Value> {
        HttpApi::invoke_json(self.0.as_ref(), app, module, proc, body).await
    }

    async fn receive_queue(
        &self,
        queue: &str,
        consumer: Option<&str>,
        wait_forever: bool,
    ) -> RS<Vec<Value>> {
        HttpApi::receive_queue(self.0.as_ref(), queue, consumer, wait_forever).await
    }
}

/// Binds the management HTTP API on the configured listen address and serves
/// it.
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

/// Serves the management HTTP API on an already-bound listener.
pub async fn serve_http_api_on_listener(
    api: Arc<dyn HttpApi>,
    listener: StdTcpListener,
    capabilities: HttpApiCapabilities,
    worker_threads: usize,
) -> std::io::Result<()> {
    serve_http_api_on_listener_with_stop(api, listener, capabilities, worker_threads, None).await
}

/// Serves the management HTTP API on an already-bound listener, stopping
/// gracefully when `stop` fires.
pub async fn serve_http_api_on_listener_with_stop(
    api: Arc<dyn HttpApi>,
    listener: StdTcpListener,
    capabilities: HttpApiCapabilities,
    worker_threads: usize,
    stop: Option<Waiter>,
) -> std::io::Result<()> {
    scoped_task_trace!();
    let payload_limit = 500 * 1024 * 1024;
    let data = web::Data::new(HttpApiContext { api: api.clone() });
    let ws_gateway = WsGateway::new(Arc::new(HttpApiWsAdapter(api)));

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
            // The WebSocket gateway invokes procedures, so it follows the
            // invoke capability: deployments with `enable_invoke = false`
            // must not expose it.
            .configure(|cfg| {
                if capabilities.enable_invoke {
                    ws_gateway.configure(cfg);
                }
            })
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
        .service(server_replication)
        .service(server_queues)
        .service(queue_activation_register)
        .service(queue_activation_unregister)
        .service(server_queue_activations)
        .service(partition_route)
        .service(partition_move)
        .service(partition_move_abort)
        .service(partition_split)
        .service(partition_merge)
        .service(install)
        .service(backup)
        .service(backup_list)
        .service(backup_verify)
        .service(backup_delete)
        .service(dump_logical)
        .service(restore_logical);
    if capabilities.enable_invoke {
        cfg.service(invoke);
    }
    if capabilities.enable_uninstall {
        cfg.service(uninstall);
    }
    if capabilities.enable_repl_admin {
        cfg.service(repl_membership)
            .service(repl_promote)
            .service(repl_follow);
    }
}
