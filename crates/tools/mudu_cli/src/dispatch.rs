//! Command dispatch: connector traits and the subcommand dispatcher.

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::client::json_client::JsonClient;
use mudu_client::management::{
    backup_create, backup_delete, backup_list, backup_verify, dump_logical, fetch_app_detail,
    fetch_app_list, fetch_proc_desc, fetch_server_replication, fetch_server_topology,
    install_app_package, repl_follow, repl_membership, repl_promote, restore_logical,
    route_partition, uninstall_app,
};
use mudu_contract::protocol::{ProcedureInvokeRequest, SessionCloseRequest, SessionCreateRequest};
use serde_json::{Value, json};
use std::io;

use crate::cli::{
    AppDetailArgs, AppInstallArgs, AppInvokeArgs, AppUninstallArgs, BackupArgs, BackupCommands,
    Cli, Commands, DumpArgs, JsonRequestArgs, PartitionRouteArgs, PitrArgs, ReplArgs, ReplCommands,
    RestoreArgs, RestoreLogicalArgs, ShellArgs,
};
use crate::io_util::{
    check_fencing, load_json_request, parse_host_port, to_param, with_invoke_session_id, with_oid,
};
use crate::repl_status::render_repl_status_text;
use crate::shell::{ShellOutputOptions, run_shell};

/// Trait for connecting a [`JsonClient`] during command dispatch.
#[async_trait]
pub(crate) trait JsonClientConnect: Send + Sync {
    /// Async client type produced by this connector.
    type Client: AsyncClient;
    /// Connect to `addr` and return a JSON client backed by the async client.
    async fn connect(&self, addr: &str) -> RS<JsonClient<Self::Client>>;
}

/// Trait for connecting a raw async TCP client during command dispatch.
#[async_trait]
pub(crate) trait AsyncClientConnect: Send + Sync {
    /// Async client type produced by this connector.
    type Client: AsyncClient;
    /// Connect to `addr` and return an async client.
    async fn connect(&self, addr: &str) -> RS<Self::Client>;
}

pub(crate) struct RealJsonConnector;

#[async_trait]
impl JsonClientConnect for RealJsonConnector {
    type Client = AsyncClientImpl;

    async fn connect(&self, addr: &str) -> RS<JsonClient<Self::Client>> {
        JsonClient::connect(addr).await
    }
}

pub(crate) struct RealAsyncConnector;

#[async_trait]
impl AsyncClientConnect for RealAsyncConnector {
    type Client = AsyncClientImpl;

    async fn connect(&self, addr: &str) -> RS<Self::Client> {
        AsyncClientImpl::connect(addr).await
    }
}

pub(crate) async fn run_with_connectors<JC, AC>(
    cli: Cli,
    json_connector: &JC,
    async_connector: &AC,
) -> RS<Value>
where
    JC: JsonClientConnect,
    AC: AsyncClientConnect,
{
    let Cli {
        addr,
        http_addr,
        compact,
        table,
        no_table,
        command,
    } = cli;

    let output = match command {
        Commands::Command(args) => handle_command(args, &addr, json_connector).await?,
        Commands::Shell(args) => {
            handle_shell(
                args,
                &addr,
                ShellOutputOptions {
                    compact,
                    table,
                    no_table,
                },
                json_connector,
            )
            .await?;
            return Ok(json!({ "status": "ok" }));
        }
        Commands::Put(args) => handle_put(args, &addr, async_connector).await?,
        Commands::Get(args) => handle_get(args, &addr, async_connector).await?,
        Commands::Range(args) => handle_range(args, &addr, async_connector).await?,
        Commands::Invoke(args) => handle_invoke(args, &addr, async_connector).await?,
        Commands::AppInstall(args) => handle_app_install(args, &http_addr).await?,
        Commands::AppInvoke(args) => {
            handle_app_invoke(args, &addr, &http_addr, async_connector).await?
        }
        Commands::AppList => handle_app_list(&http_addr).await?,
        Commands::AppDetail(args) => handle_app_detail(args, &http_addr).await?,
        Commands::AppUninstall(args) => handle_app_uninstall(args, &http_addr).await?,
        Commands::ServerTopology => handle_server_topology(&http_addr).await?,
        Commands::PartitionRoute(args) => handle_partition_route(args, &http_addr).await?,
        Commands::Repl(args) => handle_repl(args, &http_addr, table, no_table).await?,
        Commands::Backup(args) => handle_backup(args, &http_addr).await?,
        Commands::Restore(args) => handle_restore(args).await?,
        Commands::Pitr(args) => handle_pitr(args).await?,
        Commands::Dump(args) => handle_dump(args, &http_addr).await?,
        Commands::RestoreLogical(args) => handle_restore_logical(args, &http_addr).await?,
    };

    Ok(output)
}

async fn handle_command<JC>(args: JsonRequestArgs, addr: &str, json_connector: &JC) -> RS<Value>
where
    JC: JsonClientConnect,
{
    let request = load_json_request(args)?;
    let mut client = json_connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;
    client
        .command(request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, format!("command request failed: {}", e)))
}

async fn handle_shell<JC>(
    args: ShellArgs,
    addr: &str,
    options: ShellOutputOptions,
    json_connector: &JC,
) -> RS<()>
where
    JC: JsonClientConnect,
{
    let mut rl = rustyline::DefaultEditor::new()
        .map_err(|e| mudu_error!(ErrorCode::Io, format!("init readline failed: {e}")))?;
    let mut stdout = io::stdout();
    run_shell(addr, options, args, json_connector, &mut rl, &mut stdout).await
}

async fn handle_put<AC>(args: JsonRequestArgs, addr: &str, async_connector: &AC) -> RS<Value>
where
    AC: AsyncClientConnect,
{
    let request = load_json_request(args)?;
    let mut client = async_connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;
    let session_id = client
        .create_session(SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("session-create for put failed: {}", e)
            )
        })?
        .session_id();
    let request = with_oid(request, session_id)?;
    let mut client = JsonClient::new(client);
    let response = client
        .put(request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, format!("put request failed: {}", e)))?;
    let _ = client
        .into_inner()
        .close_session(SessionCloseRequest::new(session_id))
        .await;
    Ok(response)
}

async fn handle_get<AC>(args: JsonRequestArgs, addr: &str, async_connector: &AC) -> RS<Value>
where
    AC: AsyncClientConnect,
{
    let request = load_json_request(args)?;
    let mut client = async_connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;
    let session_id = client
        .create_session(SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("session-create for get failed: {}", e)
            )
        })?
        .session_id();
    let request = with_oid(request, session_id)?;
    let mut client = JsonClient::new(client);
    let response = client
        .get(request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, format!("get request failed: {}", e)))?;
    let _ = client
        .into_inner()
        .close_session(SessionCloseRequest::new(session_id))
        .await;
    Ok(response)
}

async fn handle_range<AC>(args: JsonRequestArgs, addr: &str, async_connector: &AC) -> RS<Value>
where
    AC: AsyncClientConnect,
{
    let request = load_json_request(args)?;
    let mut client = async_connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;
    let session_id = client
        .create_session(SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("session-create for range failed: {}", e)
            )
        })?
        .session_id();
    let request = with_oid(request, session_id)?;
    let mut client = JsonClient::new(client);
    let response = client
        .range(request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, format!("range request failed: {}", e)))?;
    let _ = client
        .into_inner()
        .close_session(SessionCloseRequest::new(session_id))
        .await;
    Ok(response)
}

async fn handle_invoke<AC>(args: JsonRequestArgs, addr: &str, async_connector: &AC) -> RS<Value>
where
    AC: AsyncClientConnect,
{
    let request = load_json_request(args)?;
    let mut client = async_connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;
    let session_id = client
        .create_session(SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("session-create for invoke failed: {}", e)
            )
        })?
        .session_id();
    let request = with_invoke_session_id(request, session_id)?;
    let mut client = JsonClient::new(client);
    let response = client
        .invoke(request)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, format!("invoke request failed: {}", e)))?;
    let _ = client
        .into_inner()
        .close_session(SessionCloseRequest::new(session_id))
        .await;
    Ok(response)
}

async fn handle_app_install(args: AppInstallArgs, http_addr: &str) -> RS<Value> {
    let mpk_binary = mudu_sys::fs::sync::sync_read_all(&args.mpk).map_err(|e| {
        mudu_error!(
            ErrorCode::Io,
            format!("read {} failed: {}", args.mpk.display(), e)
        )
    })?;
    install_app_package(http_addr, mpk_binary)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
    let mut response = json!({
        "status": "ok",
    });
    if let Value::Object(ref mut map) = response {
        map.insert(
            "mpk_path".to_string(),
            Value::String(args.mpk.display().to_string()),
        );
    }
    Ok(response)
}

async fn handle_app_invoke<AC>(
    args: AppInvokeArgs,
    addr: &str,
    http_addr: &str,
    async_connector: &AC,
) -> RS<Value>
where
    AC: AsyncClientConnect,
{
    let request = load_json_request(args.request)?;
    let proc_desc = fetch_proc_desc(http_addr, &args.app, &args.module, &args.proc)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
    let request_object = request.as_object().cloned().ok_or_else(|| {
        mudu_error!(
            ErrorCode::InvalidArgument,
            "invoke request JSON must be an object"
        )
    })?;
    let param = to_param(&request_object, proc_desc.param_desc().fields())?;
    let payload = procedure_invoke::serialize_param(param).map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            format!("serialize procedure param failed: {}", e)
        )
    })?;
    let mut client = async_connector.connect(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            format!("connect {} failed: {}", addr, e)
        )
    })?;
    let session_id = client
        .create_session(SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Network,
                format!("session-create for app-invoke failed: {}", e)
            )
        })?
        .session_id();
    let result_binary = client
        .invoke_procedure(ProcedureInvokeRequest::new(
            session_id,
            format!("{}/{}/{}", args.app, args.module, args.proc),
            payload,
        ))
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, format!("tcp invoke failed: {}", e)))?
        .into_result();
    let _ = client
        .close_session(SessionCloseRequest::new(session_id))
        .await;
    let result = procedure_invoke::deserialize_result(&result_binary).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!("deserialize procedure result failed: {}", e)
        )
    })?;
    procedure_invoke::result_to_json(result).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!("convert procedure result to JSON failed: {}", e)
        )
    })
}

async fn handle_app_list(http_addr: &str) -> RS<Value> {
    fetch_app_list(http_addr)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, e))
}

async fn handle_app_detail(args: AppDetailArgs, http_addr: &str) -> RS<Value> {
    if args.proc.is_some() && args.module.is_none() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "--proc requires --module"
        ));
    }
    fetch_app_detail(
        http_addr,
        &args.app,
        args.module.as_deref(),
        args.proc.as_deref(),
    )
    .await
    .map_err(|e| mudu_error!(ErrorCode::Network, e))
}

async fn handle_app_uninstall(args: AppUninstallArgs, http_addr: &str) -> RS<Value> {
    uninstall_app(http_addr, &args.app)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
    Ok(json!({
        "status": "ok",
        "app": args.app,
    }))
}

async fn handle_server_topology(http_addr: &str) -> RS<Value> {
    serde_json::to_value(
        fetch_server_topology(http_addr)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
    )
    .map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            format!("serialize server topology failed: {}", e)
        )
    })
}

async fn handle_partition_route(args: PartitionRouteArgs, http_addr: &str) -> RS<Value> {
    if args.key.is_some() && (args.start.is_some() || args.end.is_some()) {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "use either --key or (--start/--end), not both"
        ));
    }
    if args.key.is_none() && args.start.is_none() && args.end.is_none() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "partition-route requires either --key or at least one of --start/--end"
        ));
    }
    serde_json::to_value(
        route_partition(http_addr, &args.rule_name, args.key, args.start, args.end)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
    )
    .map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            format!("serialize partition route response failed: {}", e)
        )
    })
}

async fn handle_repl(args: ReplArgs, http_addr: &str, table: bool, no_table: bool) -> RS<Value> {
    let output = match &args.command {
        ReplCommands::AddNode(add) => {
            let action = if add.learner {
                "add_learner"
            } else {
                "add_voter"
            };
            repl_membership(
                http_addr,
                action,
                add.node_id,
                Some(add.host.clone()),
                Some(add.port),
            )
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?
        }
        ReplCommands::RemoveNode(remove) => {
            repl_membership(http_addr, "remove", remove.node_id, None, None)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?
        }
        ReplCommands::PromoteLearner(promote) => {
            repl_membership(http_addr, "promote_learner", promote.node_id, None, None)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?
        }
        ReplCommands::Promote(promote) => {
            let fencing_ok = check_fencing(promote)?;
            repl_promote(http_addr, fencing_ok)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?
        }
        ReplCommands::Follow(follow) => {
            let (host, port) = parse_host_port(&follow.primary)?;
            repl_follow(http_addr, follow.primary_node_id, &host, port)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?
        }
        ReplCommands::Status => {
            let status = fetch_server_replication(http_addr)
                .await
                .map_err(|e| mudu_error!(ErrorCode::Network, e))?;
            if table && !no_table {
                print!("{}", render_repl_status_text(&status));
                return Ok(json!({"status": "ok"}));
            }
            status
        }
    };
    Ok(output)
}

async fn handle_backup(args: BackupArgs, http_addr: &str) -> RS<Value> {
    let output = match &args.command {
        BackupCommands::Create(create) => backup_create(http_addr, &create.label)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
        BackupCommands::List => backup_list(http_addr)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
        BackupCommands::Verify(verify) => backup_verify(http_addr, &verify.label)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
        BackupCommands::Delete(delete) => backup_delete(http_addr, &delete.label)
            .await
            .map_err(|e| mudu_error!(ErrorCode::Network, e))?,
    };
    Ok(output)
}

async fn handle_restore(args: RestoreArgs) -> RS<Value> {
    use backup::restore::{RestoreTarget, restore_backup};
    let report = restore_backup(
        &args.backup_dir,
        &args.archive_dir,
        &args.label,
        &args.target_dir.to_string_lossy(),
        RestoreTarget::End,
    )
    .await
    .map_err(|e| mudu_error!(ErrorCode::Database, format!("restore failed: {e}")))?;
    serde_json::to_value(report).map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            format!("serialize restore report failed: {e}")
        )
    })
}

async fn handle_pitr(args: PitrArgs) -> RS<Value> {
    use backup::restore::{RestoreTarget, restore_backup};
    let target = if let Some(lsn) = args.lsn {
        RestoreTarget::Lsn(lsn)
    } else if let Some(time) = args.time {
        RestoreTarget::Time(time)
    } else if let Some(xid) = args.xid {
        RestoreTarget::Xid(xid)
    } else {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "pitr requires one of --lsn, --time or --xid"
        ));
    };
    let report = restore_backup(
        &args.backup_dir,
        &args.archive_dir,
        &args.label,
        &args.target_dir.to_string_lossy(),
        target,
    )
    .await
    .map_err(|e| mudu_error!(ErrorCode::Database, format!("pitr failed: {e}")))?;
    serde_json::to_value(report).map_err(|e| {
        mudu_error!(
            ErrorCode::Encode,
            format!("serialize pitr report failed: {e}")
        )
    })
}

async fn handle_dump(args: DumpArgs, http_addr: &str) -> RS<Value> {
    dump_logical(http_addr, &args.label, &args.dir)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, e))
}

async fn handle_restore_logical(args: RestoreLogicalArgs, http_addr: &str) -> RS<Value> {
    restore_logical(http_addr, &args.label, &args.dir, args.drop)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, e))
}
