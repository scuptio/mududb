//! Concurrency spike for the pure-async-task execution model: proves that two
//! async-world component instances can be suspended **mid-hostcall** on ONE
//! OS thread, interleaved, and then resumed to completion with correct
//! results.
//!
//! Construction: each instance's SQL `Context` is backed by a test-only
//! [`DBConnAsync`] decorator ([`ObservedAsyncConn`]) that logs host-call
//! entry/exit (with the OS thread id) and, for instance A, parks the `query`
//! future on a manual [`Gate`] (a tokio `watch` channel). Instance A is
//! therefore *provably* parked inside its WASM activation — the host `query`
//! call has been entered but has not returned — while instance B runs its
//! whole WASM activation (instantiate, call, host calls, return) to
//! completion on the same thread. wasmtime's per-Store event loop detaches
//! A's suspended call state and returns `Poll::Pending` to the embedder; B's
//! ability to enter and leave WASM while A is parked is the property under
//! test.
//!
//! The fixture is `wallet.mpk` (`crates/db-kernel/testing/mpk`), whose
//! `deposit` procedure performs `query` + 2 x `command` host calls through
//! async-lowered imports. (`app1.mpk`'s SQL procs make no host calls, and its
//! async-lifted `proc_sys_call_mtp` fails before its first host call even
//! through the full `invoke_procedure_async` path — a pre-existing fixture
//! issue, not exercised here.)
//!
//! libsql performs real SQLite file IO outside `mudu_sys`; the
//! deterministic-simulation backend keeps fs writes in memory, so this module
//! is compiled for the native backend only (`not(feature = "ds")`).

#![allow(clippy::unwrap_used)]

use crate::db_connector::DBConnector;
use crate::procedure::procedure::Procedure;
use crate::service::app_package::AppPackage;
use crate::service::procedure_invoke_component::{ProcOpt, ProcedureInvokeComponent};
use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
use crate::service::test_wasm_mod_path::wasm_mod_path;
use crate::service::wt_runtime_component::WTRuntimeComponent;
use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::db_conn::DBConnAsync;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql::{Context, DBConn};
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::procedure::procedure_result::ProcedureResult;
use mudu_sys::sync::SMutex;
use mudu_type::data_value::DataValue;
use mudu_utils::oid::new_xid;
use std::future::{Future, poll_fn};
use std::path::PathBuf;
use std::pin::Pin;
use std::sync::Arc;
use std::task::Poll;

/// A manually opened pend point for host calls: `wait` parks until `open` is
/// called. Backed by a tokio `watch` channel, so the parked future is driven
/// by a genuine tokio task wakeup — the same mechanism real async I/O uses.
struct Gate {
    sender: tokio::sync::watch::Sender<bool>,
    receiver: tokio::sync::watch::Receiver<bool>,
}

impl Gate {
    fn new() -> Arc<Self> {
        let (sender, receiver) = tokio::sync::watch::channel(false);
        Arc::new(Self { sender, receiver })
    }

    /// A gate that is already open: `wait` never parks.
    fn opened() -> Arc<Self> {
        let gate = Self::new();
        gate.open();
        gate
    }

    async fn wait(&self) {
        let mut receiver = self.receiver.clone();
        receiver
            .wait_for(|open| *open)
            .await
            .expect("gate sender alive for the whole test");
    }

    fn open(&self) {
        self.sender.send(true).expect("gate receiver alive");
    }
}

/// Test-only `DBConnAsync` decorator: records host-call entry/exit (with the
/// OS thread id) into a shared ordered log and parks the `query` call on a
/// [`Gate`] before delegating to a real libsql async connection. Delegation
/// keeps results genuinely correct; only the timing is controlled.
struct ObservedAsyncConn {
    label: &'static str,
    inner: Arc<dyn DBConnAsync>,
    gate: Arc<Gate>,
    log: Arc<SMutex<Vec<String>>>,
}

impl ObservedAsyncConn {
    fn record(&self, event: &str) {
        self.log.lock().unwrap().push(format!(
            "{}:{}@{:?}",
            self.label,
            event,
            std::thread::current().id()
        ));
    }
}

#[async_trait]
impl DBConnAsync for ObservedAsyncConn {
    async fn prepare(&self, stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
        self.inner.prepare(stmt).await
    }

    async fn exec_silent(&self, sql_text: String) -> RS<()> {
        self.inner.exec_silent(sql_text).await
    }

    async fn begin_tx(&self) -> RS<OID> {
        self.inner.begin_tx().await
    }

    async fn rollback_tx(&self) -> RS<()> {
        self.inner.rollback_tx().await
    }

    async fn commit_tx(&self) -> RS<()> {
        self.inner.commit_tx().await
    }

    async fn query(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        self.record("query-enter");
        self.gate.wait().await;
        let result = self.inner.query(sql, param).await;
        self.record("query-exit");
        result
    }

    async fn execute(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        self.record("execute-enter");
        let result = self.inner.execute(sql, param).await;
        self.record("execute-exit");
        result
    }

    async fn batch(&self, sql: Box<dyn SQLStmt>, param: Box<dyn SQLParams>) -> RS<u64> {
        self.record("batch-enter");
        let result = self.inner.batch(sql, param).await;
        self.record("batch-exit");
        result
    }
}

fn wallet_package() -> AppPackage {
    let path = PathBuf::from(wasm_mod_path())
        .join("../../../testing/mpk/wallet.mpk")
        .canonicalize()
        .unwrap();
    AppPackage::load(path).unwrap()
}

fn wallet_procedure(proc_name: &str) -> Procedure {
    let package = wallet_package();
    let mut runtime = WTRuntimeComponent::build(&RuntimeOpt {
        component_target: ComponentTarget::P2,
        enable_async: true,
        sever_mode: Default::default(),
        async_runtime: None,
        defer_initdb: false,
    })
    .unwrap();
    runtime.instantiate().unwrap();
    runtime
        .compile_modules(&package)
        .unwrap()
        .into_iter()
        .find(|(name, _)| name == "wallet")
        .unwrap()
        .1
        .procedure(proc_name)
        .unwrap()
}

fn temp_db_dir(label: &str) -> String {
    let dir = mudu_sys::env_var::temp_dir().join(format!(
        "mudu-concurrent-wasm-{}-{}",
        label,
        mudu_sys::random::next_uuid_v4_string()
    ));
    mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
    dir.to_str().unwrap().to_string()
}

/// Open a real libsql async connection against `db_dir` and initialize it
/// with the wallet DDL and seed data (users 1 and 2, wallet balance 10000
/// each). Must be called exactly once per `db_dir`.
async fn init_wallet_db(db_dir: &str) {
    let package = wallet_package();
    let conn_str = format!(
        "db={} app={} db_type=LibSQLAsync",
        db_dir, package.package_cfg.name
    );
    let conn = DBConnector::connect(&conn_str).await.unwrap();
    let sql = package.ddl_sql.clone() + package.initdb_sql.as_str();
    conn.execute_silent(sql).await.unwrap();
}

/// Open a real libsql async connection against an initialized `db_dir`.
async fn real_async_conn(db_dir: &str) -> Arc<dyn DBConnAsync> {
    let package = wallet_package();
    let conn_str = format!(
        "db={} app={} db_type=LibSQLAsync",
        db_dir, package.package_cfg.name
    );
    let conn = DBConnector::connect(&conn_str).await.unwrap();
    match conn {
        DBConn::Async(async_conn) => async_conn,
        DBConn::Sync(_) => panic!("LibSQLAsync connection must be async"),
    }
}

/// Register a SQL `Context` whose connection is the observed (logged, gated)
/// decorator over a real libsql connection; returns the session OID the
/// guest passes back in its host calls, plus the real inner connection (used
/// to verify committed database state).
async fn observed_context(
    db_dir: &str,
    label: &'static str,
    gate: Arc<Gate>,
    log: Arc<SMutex<Vec<String>>>,
) -> (OID, Arc<dyn DBConnAsync>) {
    let inner = real_async_conn(db_dir).await;
    let observed = ObservedAsyncConn {
        label,
        inner: inner.clone(),
        gate,
        log,
    };
    let oid = new_xid();
    let _context = Context::create(oid, DBConn::Async(Arc::new(observed))).unwrap();
    (oid, inner)
}

fn deposit_param(session: OID, user_id: i32, amount: i32) -> ProcedureParam {
    ProcedureParam::new(
        session,
        0,
        vec![DataValue::from_i32(user_id), DataValue::from_i32(amount)],
    )
}

fn call_future(
    proc: &Procedure,
    param: ProcedureParam,
) -> Pin<Box<dyn Future<Output = RS<ProcedureResult>> + Send + '_>> {
    Box::pin(ProcedureInvokeComponent::call_async(
        proc,
        ComponentTarget::P2,
        ProcOpt::default(),
        param,
        None,
    ))
}

/// Poll `future` once per round (yielding to the tokio runtime between
/// rounds) until `log` contains `marker`; every poll must come back
/// `Pending` — a `Ready` here means the call finished before reaching its
/// expected suspension point.
async fn poll_until_logged<F: Future + Unpin>(
    future: &mut F,
    log: &Arc<SMutex<Vec<String>>>,
    marker: &str,
) {
    for _ in 0..64 {
        poll_fn(|cx| {
            assert!(
                Pin::new(&mut *future).poll(cx).is_pending(),
                "call completed before reaching the suspension point {marker:?}"
            );
            Poll::Ready(())
        })
        .await;
        if log.lock().unwrap().iter().any(|e| e.contains(marker)) {
            return;
        }
        tokio::task::yield_now().await;
    }
    panic!("call never reached the suspension point {marker:?}");
}

/// The log entries with the `@ThreadId` suffix stripped.
fn stripped_log(log: &Arc<SMutex<Vec<String>>>) -> Vec<String> {
    log.lock()
        .unwrap()
        .iter()
        .map(|e| e.split('@').next().unwrap().to_string())
        .collect()
}

fn assert_single_thread(log: &Arc<SMutex<Vec<String>>>) {
    let entries = log.lock().unwrap();
    let thread_ids: std::collections::BTreeSet<&str> = entries
        .iter()
        .map(|e| e.split('@').nth(1).unwrap())
        .collect();
    assert_eq!(
        thread_ids.len(),
        1,
        "host calls ran on more than one OS thread: {thread_ids:?}"
    );
}

/// The committed balance of `user_id`, read through a separate connection.
async fn wallet_balance(conn: &Arc<dyn DBConnAsync>, user_id: i32) -> i32 {
    let rs = conn
        .query(
            Box::new(format!(
                "SELECT balance FROM wallets WHERE user_id = {user_id}"
            )),
            Box::new(()),
        )
        .await
        .unwrap();
    let batch = ResultBatch::from_result_set_async(0, rs.as_ref())
        .await
        .unwrap();
    let rows = batch.rows();
    assert_eq!(rows.len(), 1, "user {user_id} must have a wallet row");
    rows[0].values()[0].to_i32()
}

/// `deposit` performs exactly `query`, then `command`, then `command`.
const DEPOSIT_SEQUENCE: [&str; 6] = [
    "query-enter",
    "query-exit",
    "execute-enter",
    "execute-exit",
    "execute-enter",
    "execute-exit",
];

fn deposit_sequence_for(label: &str) -> Vec<String> {
    DEPOSIT_SEQUENCE
        .iter()
        .map(|event| format!("{label}:{event}"))
        .collect()
}

/// Core spike: A parks mid-hostcall on a closed gate; B runs its full WASM
/// activation to completion on the same OS thread; then A's gate opens and A
/// resumes mid-hostcall and completes. The ordered host-call log must show a
/// genuine interleave — the whole of B's activation nested inside A's
/// suspended host call — an execution order a strictly serialized model
/// cannot produce. Both instances must commit the
/// database state their parameters imply.
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn two_instances_suspend_mid_hostcall_and_interleave_on_one_thread() {
    let db_dir = temp_db_dir("interleave");
    init_wallet_db(&db_dir).await;
    let log: Arc<SMutex<Vec<String>>> = Arc::new(SMutex::new(Vec::new()));
    let proc = wallet_procedure("deposit");

    let gate_a = Gate::new();
    let (oid_a, conn_a) = observed_context(&db_dir, "A", gate_a.clone(), log.clone()).await;
    let (oid_b, _conn_b) = observed_context(&db_dir, "B", Gate::opened(), log.clone()).await;

    let mut fut_a = call_future(&proc, deposit_param(oid_a, 1, 100));
    let fut_b = call_future(&proc, deposit_param(oid_b, 2, 200));

    // Phase 1: drive A (and only A) until its host `query` call is parked on
    // the closed gate. A is now suspended mid-hostcall inside its WASM
    // activation, its Store's call state detached by wasmtime's event loop.
    poll_until_logged(&mut fut_a, &log, "A:query-enter").await;
    assert_eq!(stripped_log(&log), ["A:query-enter"]);

    // Phase 2: with A parked, drive B to completion on this same thread. If
    // wasmtime could not suspend A's in-WASM call, B could not enter WASM
    // here (the thread-local call-state chain would still be held by A).
    let result_b = fut_b.await.unwrap();
    assert_eq!(result_b.return_list().len(), 0);
    let mut expected = vec!["A:query-enter".to_string()];
    expected.extend(deposit_sequence_for("B"));
    assert_eq!(stripped_log(&log), expected);

    // Phase 3: open A's gate; A resumes mid-hostcall and completes.
    gate_a.open();
    let result_a = fut_a.await.unwrap();
    assert_eq!(result_a.return_list().len(), 0);
    expected.extend(deposit_sequence_for("A").into_iter().skip(1));
    assert_eq!(stripped_log(&log), expected);
    assert_single_thread(&log);

    // Both instances produced their own correct database state.
    assert_eq!(wallet_balance(&conn_a, 1).await, 10100);
    assert_eq!(wallet_balance(&conn_a, 2).await, 10200);

    Context::remove(oid_a);
    Context::remove(oid_b);
}

/// Sequential control: the same two deposits run back-to-back (gates open
/// from the start) must produce the same per-instance host-call sequences
/// and the same committed state as the interleaved run — concurrency changes
/// interleaving, not outcomes.
#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn sequential_control_run_produces_identical_results() {
    let db_dir = temp_db_dir("sequential");
    init_wallet_db(&db_dir).await;
    let log: Arc<SMutex<Vec<String>>> = Arc::new(SMutex::new(Vec::new()));
    let proc = wallet_procedure("deposit");

    let (oid_a, conn_a) = observed_context(&db_dir, "A", Gate::opened(), log.clone()).await;
    let (oid_b, _conn_b) = observed_context(&db_dir, "B", Gate::opened(), log.clone()).await;

    let result_a = call_future(&proc, deposit_param(oid_a, 1, 100))
        .await
        .unwrap();
    let result_b = call_future(&proc, deposit_param(oid_b, 2, 200))
        .await
        .unwrap();
    assert_eq!(result_a.return_list().len(), 0);
    assert_eq!(result_b.return_list().len(), 0);

    let mut expected = deposit_sequence_for("A");
    expected.extend(deposit_sequence_for("B"));
    assert_eq!(stripped_log(&log), expected);
    assert_single_thread(&log);

    assert_eq!(wallet_balance(&conn_a, 1).await, 10100);
    assert_eq!(wallet_balance(&conn_a, 2).await, 10200);

    Context::remove(oid_a);
    Context::remove(oid_b);
}
