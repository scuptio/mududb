use crate::backend::app_mgr::AppMgr;
use crate::backend::mudu_app_mgr::MuduAppMgr;
use crate::backend::mududb_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use crate::service::runtime_opt::ComponentTarget;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_cli::client::client::SyncClient;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_kernel::server_ur::procedure_runtime::ProcInvokerPtr;
use mudu_kernel::server_ur::routing::RoutingMode as KernelRoutingMode;
use mudu_kernel::server_ur::server::{IoUringTcpBackend, IoUringTcpServerConfig};
use mudu_utils::notifier::notify_wait;
use std::env::temp_dir;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

fn reserve_port() -> Option<u16> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Some(listener.local_addr().ok()?.port()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => None,
        Err(e) => panic!("reserve local tcp port error: {e}"),
    }
}

fn wait_until_server_ready(port: u16) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while Instant::now() < deadline {
        if std::net::TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(25));
    }
    panic!("io_uring backend did not become ready on port {}", port);
}

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn kv_example_dir() -> PathBuf {
    workspace_root().join("example").join("key-value")
}

fn kv_package_path() -> PathBuf {
    workspace_root()
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join("key-value.mpk")
}

fn ensure_kv_package_built() -> RS<PathBuf> {
    let example_dir = kv_example_dir();
    let package_path = kv_package_path();
    if package_path.is_file() {
        return Ok(package_path);
    }

    let mut command = Command::new("cargo");
    command.arg("make").current_dir(&example_dir);

    let venv_bin = example_dir.join(".venv").join("bin");
    if venv_bin.is_dir() {
        let current_path = std::env::var_os("PATH").unwrap_or_default();
        let mut paths = vec![venv_bin];
        paths.extend(std::env::split_paths(&current_path));
        let joined = std::env::join_paths(paths)
            .map_err(|e| m_error!(EC::IOErr, "join PATH for key-value package build error", e))?;
        command.env("PATH", joined);
    }

    let output = command
        .output()
        .map_err(|e| m_error!(EC::IOErr, "spawn cargo make for key-value package error", e))?;
    if !output.status.success() {
        return Err(m_error!(
            EC::IOErr,
            format!(
                "build key-value package error: status={}, stdout={}, stderr={}",
                output.status,
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            )
        ));
    }
    if !package_path.is_file() {
        return Err(m_error!(
            EC::IOErr,
            format!(
                "key-value package missing after build: {}",
                package_path.display()
            )
        ));
    }
    Ok(package_path)
}

fn temp_dir_with_prefix(prefix: &str) -> PathBuf {
    temp_dir().join(format!("{}_{}", prefix, uuid::Uuid::new_v4()))
}

fn build_cfg(port: u16, mpk_path: &Path, data_path: &Path) -> MuduDBCfg {
    let mut cfg = MuduDBCfg::default();
    cfg.mpk_path = mpk_path.to_string_lossy().into_owned();
    cfg.data_path = data_path.to_string_lossy().into_owned();
    cfg.listen_ip = "127.0.0.1".to_string();
    cfg.server_mode = ServerMode::IOUring;
    cfg.tcp_listen_port = port;
    cfg.io_uring_worker_threads = 2;
    cfg.enable_p2 = true;
    cfg.component_target = Some(ComponentTarget::P2);
    cfg.enable_async = true;
    cfg.routing_mode = RoutingMode::ConnectionId;
    cfg
}

fn install_kv_package(app_mgr: &MuduAppMgr, package_path: &Path) -> RS<()> {
    let pkg_binary = std::fs::read(package_path)
        .map_err(|e| m_error!(EC::IOErr, "read key-value mpk for test install error", e))?;
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            m_error!(
                EC::TokioErr,
                "create runtime for key-value package install error",
                e
            )
        })?;
    runtime.block_on(async { app_mgr.install(pkg_binary).await })
}

fn create_procedure_runtimes(app_mgr: &MuduAppMgr, cfg: &MuduDBCfg) -> RS<Vec<ProcInvokerPtr>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            m_error!(
                EC::TokioErr,
                "create runtime for key-value procedure invokers error",
                e
            )
        })?;
    runtime.block_on(async {
        let mut runtimes = Vec::with_capacity(cfg.effective_worker_threads());
        for _ in 0..cfg.effective_worker_threads() {
            runtimes.push(app_mgr.create_invoker(cfg).await?);
        }
        Ok(runtimes)
    })
}

fn serialize_param<T: TupleDatum>(tuple: T) -> RS<Vec<u8>> {
    let desc = T::tuple_desc_static(&[]);
    let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
    procedure_invoke::serialize_param(param)
}

fn invoke_and_decode<T: TupleDatum>(
    client: &mut SyncClient,
    session_id: u128,
    procedure_name: &str,
    param: Vec<u8>,
) -> RS<T> {
    let result_binary = client.invoke_procedure(session_id, procedure_name, param)?;
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    result.to(&T::tuple_desc_static(&[]))
}

#[test]
#[ignore = "requires Linux io_uring, cargo make, wasm32-wasip2 target, and example/key-value package build"]
fn kv_mpk_can_be_used_by_iouring_backend() -> RS<()> {
    let package_path = ensure_kv_package_built()?;
    let mpk_dir = temp_dir_with_prefix("mududb_kv_mpk");
    let data_dir = temp_dir_with_prefix("mududb_kv_data");
    std::fs::create_dir_all(&mpk_dir)
        .map_err(|e| m_error!(EC::IOErr, "create key-value test mpk dir error", e))?;
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| m_error!(EC::IOErr, "create key-value test data dir error", e))?;

    let Some(port) = reserve_port() else {
        eprintln!(
            "skip key-value io_uring test: local tcp bind is not permitted in this environment"
        );
        return Ok(());
    };
    let cfg = build_cfg(port, &mpk_dir, &data_dir);
    let app_mgr = MuduAppMgr::new(cfg.clone());
    install_kv_package(&app_mgr, &package_path)?;
    let procedure_runtimes = create_procedure_runtimes(&app_mgr, &cfg)?;

    let (stop_notifier, server_stop) = notify_wait();
    let server_cfg = IoUringTcpServerConfig::new(
        cfg.effective_worker_threads(),
        cfg.listen_ip.clone(),
        cfg.tcp_listen_port,
        cfg.data_path.clone(),
        KernelRoutingMode::ConnectionId,
        None,
    )?
    .with_log_chunk_size(cfg.io_uring_log_chunk_size)
    .with_worker_procedure_runtimes(procedure_runtimes);

    let server_thread =
        thread::spawn(move || IoUringTcpBackend::sync_serve_with_stop(server_cfg, server_stop));

    wait_until_server_ready(port);

    let test_result = (|| -> RS<()> {
        let mut client = SyncClient::connect(("127.0.0.1", port))?;
        let session_id = client.create_session(None)?;

        let _: () = invoke_and_decode(
            &mut client,
            session_id,
            "kv/key_value/kv_insert",
            serialize_param(("user0001".to_string(), "value-1".to_string()))?,
        )?;

        let read_back: String = invoke_and_decode(
            &mut client,
            session_id,
            "kv/key_value/kv_read",
            serialize_param(("user0001".to_string(),))?,
        )?;
        assert_eq!(read_back, "value-1");

        let updated: String = invoke_and_decode(
            &mut client,
            session_id,
            "kv/key_value/kv_read_modify_write",
            serialize_param(("user0001".to_string(), "-tail".to_string()))?,
        )?;
        assert_eq!(updated, "value-1-tail");

        let rows: Vec<String> = invoke_and_decode(
            &mut client,
            session_id,
            "kv/key_value/kv_scan",
            serialize_param(("user0000".to_string(), "user9999".to_string()))?,
        )?;
        assert_eq!(rows, vec!["user/user0001=value-1-tail".to_string()]);

        assert!(client.close_session(session_id)?);
        Ok(())
    })();

    stop_notifier.notify_all();
    let join_result = server_thread
        .join()
        .map_err(|_| m_error!(EC::ThreadErr, "join key-value io_uring test server error"))?;

    test_result?;
    join_result?;
    Ok(())
}
