//! Command-line argument definitions for `mcli` (clap structs and enums).

use clap::{Args, Parser, Subcommand};
use std::path::PathBuf;

const CLI_EXAMPLES: &str = "\
Examples:
  mcli --addr 127.0.0.1:9527 command --json '{\"app_name\":\"demo\",\"sql\":\"select 1\"}'
  mcli --addr 127.0.0.1:9527 shell --app demo
  mcli --addr 127.0.0.1:9527 put --json-file put.json
  cat invoke.json | mcli --addr 127.0.0.1:9527 invoke --json-file -
  mcli --http-addr 127.0.0.1:8300 app-install --mpk target/wasm32-wasip2/release/key-value.mpk
  mcli --addr 127.0.0.1:9527 --http-addr 127.0.0.1:8300 app-invoke --app kv --module key_value --proc kv_read --json '{\"user_key\":\"user-1\"}'
  mcli --http-addr 127.0.0.1:8300 app-list
  mcli --http-addr 127.0.0.1:8300 app-detail --app wallet
  mcli --http-addr 127.0.0.1:8300 app-uninstall --app wallet
  mcli --http-addr 127.0.0.1:8300 server-topology
  mcli --http-addr 127.0.0.1:8300 partition-route --rule-name user_rule --key user-100
  mcli --http-addr 127.0.0.1:8300 repl add-node --node-id 4 --host 10.0.0.4 --port 19530 --learner
  mcli --http-addr 127.0.0.1:8300 repl promote-learner --node-id 4
  mcli --http-addr 127.0.0.1:8300 repl remove-node --node-id 4
  mcli --http-addr 127.0.0.1:8301 repl promote --fencing-cmd ./fence-old-primary.sh
  mcli --http-addr 127.0.0.1:8301 repl promote --yes
  mcli --http-addr 127.0.0.1:8301 repl follow --primary 10.0.0.2:19527 --primary-node-id 2
  mcli --http-addr 127.0.0.1:8300 repl status
  mcli --http-addr 127.0.0.1:8300 repl status --table
  mcli --http-addr 127.0.0.1:8300 backup create --label daily-1
  mcli --http-addr 127.0.0.1:8300 backup list
  mcli --http-addr 127.0.0.1:8300 backup verify --label daily-1
  mcli --http-addr 127.0.0.1:8300 backup delete --label daily-1
  mcli restore --label daily-1 --backup-dir /backups --archive-dir /wal-archive --target-dir /restored
  mcli pitr --label daily-1 --backup-dir /backups --archive-dir /wal-archive --target-dir /recovered --time 1700000000000
  mcli --http-addr 127.0.0.1:8300 dump --label daily-1 --dir /dumps
  mcli --http-addr 127.0.0.1:8300 restore-logical --label daily-1 --dir /dumps
  mcli --http-addr 127.0.0.1:8300 restore-logical --label daily-1 --dir /dumps --drop";

/// Top-level command-line arguments for `mcli`.
#[derive(Parser, Debug)]
#[command(name = "mcli")]
#[command(version)]
#[command(about = "TCP protocol client for MuduDB")]
#[command(after_help = CLI_EXAMPLES)]
pub(crate) struct Cli {
    #[arg(long, global = true, default_value = "127.0.0.1:9527")]
    pub(crate) addr: String,
    #[arg(long, global = true, default_value = "127.0.0.1:8300")]
    pub(crate) http_addr: String,
    #[arg(
        long,
        global = true,
        help = "Render SQL query results as an interactive table (ratatui). Auto-enabled on TTY."
    )]
    pub(crate) table: bool,
    #[arg(
        long,
        global = true,
        conflicts_with = "table",
        visible_alias = "no-tui",
        help = "Disable ratatui TUI rendering and always print JSON."
    )]
    pub(crate) no_table: bool,
    #[arg(
        long,
        global = true,
        help = "Print compact JSON instead of pretty JSON."
    )]
    pub(crate) compact: bool,
    #[command(subcommand)]
    pub(crate) command: Commands,
}

/// Available `mcli` subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum Commands {
    /// Send a SQL query or execute request encoded as JSON.
    Command(JsonRequestArgs),
    /// Interactive SQL shell (like mysql/psql).
    Shell(ShellArgs),
    /// Put a key-value item using a JSON request body.
    Put(JsonRequestArgs),
    /// Get a key using a JSON request body.
    Get(JsonRequestArgs),
    /// Scan a key range using a JSON request body.
    Range(JsonRequestArgs),
    /// Invoke a procedure using a JSON request body.
    Invoke(JsonRequestArgs),
    /// Install a .mpk package through the HTTP management API.
    AppInstall(AppInstallArgs),
    /// Invoke an installed procedure through the TCP protocol.
    AppInvoke(AppInvokeArgs),
    /// List installed apps via HTTP management API.
    AppList,
    /// Show app procedures or one procedure detail via HTTP management API.
    AppDetail(AppDetailArgs),
    /// Uninstall an app via HTTP management API.
    AppUninstall(AppUninstallArgs),
    /// Get worker topology via HTTP management API.
    ServerTopology,
    /// Route a partition key/range via HTTP management API.
    PartitionRoute(PartitionRouteArgs),
    /// Replication cluster administration (raft mode).
    Repl(ReplArgs),
    /// Base backup administration.
    Backup(BackupArgs),
    /// Restore a base backup + archived WAL into a target directory (offline).
    Restore(RestoreArgs),
    /// Point-in-time recovery into a target directory (offline).
    Pitr(PitrArgs),
    /// Take a full-database logical dump (schema + CSV) into a directory.
    Dump(DumpArgs),
    /// Restore a full-database logical dump from a directory.
    RestoreLogical(RestoreLogicalArgs),
}

/// Arguments for the `repl` subcommand family.
#[derive(Args, Debug)]
pub(crate) struct ReplArgs {
    #[command(subcommand)]
    pub(crate) command: ReplCommands,
}

/// Available `mcli repl` subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum ReplCommands {
    /// Add a node to the raft group (voter by default).
    AddNode(ReplAddNodeArgs),
    /// Remove a node from the raft group.
    RemoveNode(ReplNodeArgs),
    /// Promote a learner to voter after it has caught up.
    PromoteLearner(ReplNodeArgs),
    /// Promote this node to stream primary (fencing required).
    Promote(ReplPromoteArgs),
    /// Point this node at a new stream primary.
    Follow(ReplFollowArgs),
    /// Show the replication status of a node (JSON, or a sectioned table
    /// with --table).
    Status,
}

/// Arguments for `repl promote`.
#[derive(Args, Debug)]
pub(crate) struct ReplPromoteArgs {
    /// Fencing command run locally first; promote aborts when it fails.
    #[arg(long = "fencing-cmd", conflicts_with = "yes")]
    pub(crate) fencing_cmd: Option<String>,
    /// Confirm explicitly that the old primary is fenced (required when no
    /// fencing command is given).
    #[arg(long)]
    pub(crate) yes: bool,
}

/// Arguments for `repl follow`.
#[derive(Args, Debug)]
pub(crate) struct ReplFollowArgs {
    /// Replication address of the new primary (host:port).
    #[arg(long)]
    pub(crate) primary: String,
    /// Node id of the new primary.
    #[arg(long = "primary-node-id")]
    pub(crate) primary_node_id: u64,
}

/// Arguments for `repl add-node`.
#[derive(Args, Debug)]
pub(crate) struct ReplAddNodeArgs {
    #[arg(long = "node-id")]
    pub(crate) node_id: u64,
    #[arg(long)]
    pub(crate) host: String,
    #[arg(long)]
    pub(crate) port: u16,
    /// Join as a learner (replicates but never votes); promote later.
    #[arg(long)]
    pub(crate) learner: bool,
}

/// Arguments for `repl remove-node` and `repl promote-learner`.
#[derive(Args, Debug)]
pub(crate) struct ReplNodeArgs {
    #[arg(long = "node-id")]
    pub(crate) node_id: u64,
}

/// Arguments for the `backup` subcommand family.
#[derive(Args, Debug)]
pub(crate) struct BackupArgs {
    #[command(subcommand)]
    pub(crate) command: BackupCommands,
}

/// Available `mcli backup` subcommands.
#[derive(Subcommand, Debug)]
pub(crate) enum BackupCommands {
    /// Take a base backup of the server (requires `[backup] backup_dir`).
    Create(BackupCreateArgs),
    /// List the labels of every valid base backup.
    List,
    /// Verify a base backup's snapshot files against their manifests.
    Verify(BackupLabelArgs),
    /// Delete a base backup.
    Delete(BackupLabelArgs),
}

/// Arguments for `backup create`.
#[derive(Args, Debug)]
pub(crate) struct BackupCreateArgs {
    /// Label of the new backup (a directory name under `backup_dir`).
    #[arg(long)]
    pub(crate) label: String,
}

/// Arguments for `backup verify` and `backup delete`.
#[derive(Args, Debug)]
pub(crate) struct BackupLabelArgs {
    /// Label of the backup to operate on.
    #[arg(long)]
    pub(crate) label: String,
}

/// Arguments for `restore`.
#[derive(Args, Debug)]
pub(crate) struct RestoreArgs {
    /// Label of the backup to restore.
    #[arg(long)]
    pub(crate) label: String,
    /// Target data directory to materialize the restored server into.
    #[arg(long = "target-dir")]
    pub(crate) target_dir: PathBuf,
    /// Base backup directory (`[backup] backup_dir`).
    #[arg(long = "backup-dir")]
    pub(crate) backup_dir: String,
    /// WAL archive directory (`[backup] archive_dir`).
    #[arg(long = "archive-dir")]
    pub(crate) archive_dir: String,
}

/// Arguments for `pitr`.
#[derive(Args, Debug)]
pub(crate) struct PitrArgs {
    /// Label of the backup to recover from.
    #[arg(long)]
    pub(crate) label: String,
    /// Target data directory to materialize the recovered server into.
    #[arg(long = "target-dir")]
    pub(crate) target_dir: PathBuf,
    /// Base backup directory (`[backup] backup_dir`).
    #[arg(long = "backup-dir")]
    pub(crate) backup_dir: String,
    /// WAL archive directory (`[backup] archive_dir`).
    #[arg(long = "archive-dir")]
    pub(crate) archive_dir: String,
    /// Stop at the given LSN.
    #[arg(long, conflicts_with_all = ["time", "xid"])]
    pub(crate) lsn: Option<u64>,
    /// Stop after the last commit at or before this timestamp (millis).
    #[arg(long, conflicts_with_all = ["lsn", "xid"])]
    pub(crate) time: Option<u64>,
    /// Stop after the commit of this transaction id.
    #[arg(long, conflicts_with_all = ["lsn", "time"])]
    pub(crate) xid: Option<u64>,
}

/// Arguments for `dump`.
#[derive(Args, Debug)]
pub(crate) struct DumpArgs {
    /// Label of the dump (a directory name under `dir`).
    #[arg(long)]
    pub(crate) label: String,
    /// Directory the dump is written to (server-side path).
    #[arg(long)]
    pub(crate) dir: String,
}

/// Arguments for `restore-logical`.
#[derive(Args, Debug)]
pub(crate) struct RestoreLogicalArgs {
    /// Label of the dump to restore.
    #[arg(long)]
    pub(crate) label: String,
    /// Directory the dump was written to (server-side path).
    #[arg(long)]
    pub(crate) dir: String,
    /// Drop and recreate same-named tables instead of skipping them.
    #[arg(long)]
    pub(crate) drop: bool,
}

/// Arguments for subcommands that take an inline JSON body or a JSON file.
#[derive(Args, Debug)]
pub(crate) struct JsonRequestArgs {
    #[arg(long, conflicts_with = "json_file", help = "Inline JSON request body.")]
    pub(crate) json: Option<String>,
    #[arg(
        long = "json-file",
        conflicts_with = "json",
        help = "Read JSON request body from a file. Use '-' to read from stdin."
    )]
    pub(crate) json_file: Option<PathBuf>,
}

/// Arguments for the `app-install` subcommand.
#[derive(Args, Debug)]
pub(crate) struct AppInstallArgs {
    #[arg(long, help = "Path to the .mpk package file to install.")]
    pub(crate) mpk: PathBuf,
}

/// Arguments for the `app-invoke` subcommand.
#[derive(Args, Debug)]
pub(crate) struct AppInvokeArgs {
    #[arg(long)]
    pub(crate) app: String,
    #[arg(long)]
    pub(crate) module: String,
    #[arg(long)]
    pub(crate) proc: String,
    #[command(flatten)]
    pub(crate) request: JsonRequestArgs,
}

/// Arguments for the `shell` subcommand.
#[derive(Args, Debug)]
pub(crate) struct ShellArgs {
    #[arg(
        long,
        default_value = "demo",
        help = "Initial app name to run queries against."
    )]
    pub(crate) app: String,
}

/// Arguments for the `app-detail` subcommand.
#[derive(Args, Debug)]
pub(crate) struct AppDetailArgs {
    #[arg(long)]
    pub(crate) app: String,
    #[arg(long)]
    pub(crate) module: Option<String>,
    #[arg(long)]
    pub(crate) proc: Option<String>,
}

/// Arguments for the `app-uninstall` subcommand.
#[derive(Args, Debug)]
pub(crate) struct AppUninstallArgs {
    #[arg(long)]
    pub(crate) app: String,
}

/// Arguments for the `partition-route` subcommand.
#[derive(Args, Debug)]
pub(crate) struct PartitionRouteArgs {
    #[arg(long = "rule-name")]
    pub(crate) rule_name: String,
    #[arg(long, value_delimiter = ',')]
    pub(crate) key: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    pub(crate) start: Option<Vec<String>>,
    #[arg(long, value_delimiter = ',')]
    pub(crate) end: Option<Vec<String>>,
}
