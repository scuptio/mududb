#![allow(missing_docs)]

use crate::backend::http_api::{
    HttpApiCapabilities, LegacyHttpApi, serve_http_api_on_listener_with_stop,
};
use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::runtime_impl::create_runtime_service;
use crate::service::runtime_opt::RuntimeOpt;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::net::sync::StdTcpListener;
use mudu_utils::notifier::{Notifier, Waiter};
use std::sync::Arc;
use tracing::{error, info};

pub async fn async_serve(
    cfg: MuduDBCfg,
    stop: Waiter,
    opt_initialized_notifier: Option<Notifier>,
) -> RS<()> {
    let component_target = cfg.component_target();
    let enable_async = cfg.enable_async;
    let runtime_opt = RuntimeOpt {
        component_target,
        enable_async,
        sever_mode: cfg.server_mode,
        async_runtime: RuntimeOpt::build_async_runtime(cfg.server_mode),
    };
    let service = create_runtime_service(
        &cfg.mpk_path,
        &cfg.db_path,
        opt_initialized_notifier,
        runtime_opt,
    )
    .await
    .inspect_err(|e| {
        error!(
            listen_ip = %cfg.listen_ip,
            http_listen_port = cfg.http_listen_port,
            data_path = %cfg.db_path,
            mpk_path = %cfg.mpk_path,
            component_target = ?component_target,
            enable_async = enable_async,
            "initialize legacy runtime before starting management http service failed: {}",
            e
        );
    })?;
    let addr: std::net::SocketAddr = format!("{}:{}", cfg.listen_ip, cfg.http_listen_port)
        .parse()
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Parse,
                "parse backend http listener address error",
                e
            )
        })?;
    let listener = StdTcpListener::bind(addr)?;
    info!(
        listen_ip = %cfg.listen_ip,
        http_listen_port = cfg.http_listen_port,
        http_worker_threads = cfg.http_worker_threads,
        component_target = ?component_target,
        enable_async = enable_async,
        capabilities = ?HttpApiCapabilities::LEGACY,
        "legacy management http service listening"
    );
    serve_http_api_on_listener_with_stop(
        Arc::new(LegacyHttpApi::new(service)),
        listener,
        HttpApiCapabilities::LEGACY,
        cfg.http_worker_threads,
        Some(stop),
    )
    .await
    .map_err(|e| mudu_error!(ErrorCode::from(&e), "backend run error", e))
}
