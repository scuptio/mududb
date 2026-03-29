use crate::backend::http_api::{
    HttpApiCapabilities, LegacyHttpApi, serve_http_api_on_listener_with_stop,
};
use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::runtime_impl::create_runtime_service;
use crate::service::runtime_opt::RuntimeOpt;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::notifier::{Notifier, Waiter};
use std::net::TcpListener;
use std::sync::Arc;
use tracing::{error, info};

pub async fn async_serve(
    cfg: MuduDBCfg,
    stop: Waiter,
    opt_initialized_notifier: Option<Notifier>,
) -> RS<()> {
    let runtime_target = cfg.runtime_target();
    let enable_async = cfg.enable_async && runtime_target.uses_component_model();
    let runtime_opt = RuntimeOpt {
        target: runtime_target,
        enable_async,
    };
    let service = create_runtime_service(
        &cfg.mpk_path,
        &cfg.data_path,
        opt_initialized_notifier,
        runtime_opt,
    )
    .await
    .inspect_err(|e| {
        error!(
            listen_ip = %cfg.listen_ip,
            http_listen_port = cfg.http_listen_port,
            data_path = %cfg.data_path,
            mpk_path = %cfg.mpk_path,
            runtime_target = ?runtime_target,
            enable_async = enable_async,
            "initialize legacy runtime before starting management http service failed: {}",
            e
        );
    })?;
    let listener = TcpListener::bind(format!("{}:{}", cfg.listen_ip, cfg.http_listen_port))
        .map_err(|e| m_error!(EC::IOErr, "bind backend http listener error", e))?;
    info!(
        listen_ip = %cfg.listen_ip,
        http_listen_port = cfg.http_listen_port,
        http_worker_threads = cfg.http_worker_threads,
        runtime_target = ?runtime_target,
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
    .map_err(|e| m_error!(EC::IOErr, "backend run error", e))
}
