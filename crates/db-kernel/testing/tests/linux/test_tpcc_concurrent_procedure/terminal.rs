use super::support::TpccBenchCfg;
use mudu::common::result::RS;
use mudu::mudu_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use std::time::Duration;
use tracing::debug;

pub(super) async fn run_terminal(
    terminal_id: usize,
    addr: &str,
    app_name: &str,
    cfg: &TpccBenchCfg,
) -> RS<TerminalReport> {
    let (mut client, session_id) = connect_terminal(terminal_id, addr, cfg).await?;
    debug!(terminal_id, session_id = %session_id, "terminal session created");

    let mut report = TerminalReport {
        committed: 0,
        new_order_committed: 0,
        aborted: 0,
        latency_us: Vec::with_capacity(cfg.operations_per_terminal),
    };

    for op_idx in 0..cfg.operations_per_terminal {
        // Interleave terminal sequences by round to avoid deterministic hot-spot contention
        // (e.g. all terminals hitting the same district/customer at op_idx=0).
        let global_idx = op_idx * cfg.terminals + terminal_id;
        let warehouse_id = value_for(global_idx, cfg.warehouses);
        let district_id = value_for(global_idx, cfg.districts_per_warehouse);
        let customer_id = value_for(global_idx, cfg.customers_per_district);

        let op = op_for(global_idx, cfg.new_order_percent, cfg.payment_percent);
        let started = mudu_sys::time::instant_now();
        if cfg.trace_ops {
            debug!(
                terminal_id,
                op_idx,
                global_idx,
                op = op_name(op),
                "invoke begin"
            );
        }

        let args = OpArgs {
            terminal_id,
            op_idx,
            global_idx,
            app_name,
            cfg,
            warehouse_id,
            district_id,
            customer_id,
        };
        let op_result = run_op(&mut client, session_id, op, &args, &mut report).await;
        record_op_outcome(&mut report, op, &args, started, op_result)?;
    }

    let _ = client
        .close_session(mudu_contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await;

    Ok(report)
}

/// Connects the terminal client and opens its session.
async fn connect_terminal(
    terminal_id: usize,
    addr: &str,
    cfg: &TpccBenchCfg,
) -> RS<(AsyncClientImpl, u128)> {
    let mut client = mudu_sys::timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        AsyncClientImpl::connect(addr),
    )
    .await
    .ok_or_else(|| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("terminal connect timeout terminal_id={terminal_id} addr={addr}")
        )
    })?
    .map_err(|e| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("connect terminal client error terminal_id={terminal_id}"),
            e
        )
    })?;
    let session_id = mudu_sys::timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        client.create_session(mudu_contract::protocol::SessionCreateRequest::new(None)),
    )
    .await
    .ok_or_else(|| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("create terminal session timeout terminal_id={terminal_id}")
        )
    })?
    .map_err(|e| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("create terminal session error terminal_id={terminal_id}"),
            e
        )
    })?
    .session_id();
    Ok((client, session_id))
}

/// Per-operation invoke inputs derived from the interleaved global index.
#[derive(Clone, Copy)]
struct OpArgs<'a> {
    terminal_id: usize,
    op_idx: usize,
    global_idx: usize,
    app_name: &'a str,
    cfg: &'a TpccBenchCfg,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
}

/// Invokes one TPC-C operation; the NewOrder arm additionally counts
/// committed orders in the report.
async fn run_op(
    client: &mut AsyncClientImpl,
    session_id: u128,
    op: TpccOp,
    args: &OpArgs<'_>,
    report: &mut TerminalReport,
) -> RS<()> {
    let &OpArgs {
        terminal_id,
        op_idx,
        global_idx,
        app_name,
        cfg,
        warehouse_id,
        district_id,
        customer_id,
    } = args;
    let ids = (terminal_id, op_idx, global_idx);
    match op {
        TpccOp::NewOrder => {
            let (item_ids, supplier_warehouse_ids, quantities) =
                new_order_lines(global_idx, warehouse_id, cfg.warehouses, cfg.items);
            let invoke_result = invoke_op(
                client,
                session_id,
                &proc_name(app_name, "tpcc_new_order"),
                (
                    warehouse_id,
                    district_id,
                    customer_id,
                    item_ids,
                    supplier_warehouse_ids,
                    quantities,
                ),
                "new_order",
                ids,
                cfg.invoke_timeout_ms,
            )
            .await;
            if invoke_result.is_ok() {
                report.new_order_committed += 1;
            }
            invoke_result.map(|_: String| ())
        }
        TpccOp::Payment => invoke_op(
            client,
            session_id,
            &proc_name(app_name, "tpcc_payment"),
            (warehouse_id, district_id, customer_id, 3_i32),
            "payment",
            ids,
            cfg.invoke_timeout_ms,
        )
        .await
        .map(|_: i32| ()),
        TpccOp::OrderStatus => invoke_op(
            client,
            session_id,
            &proc_name(app_name, "tpcc_order_status"),
            (warehouse_id, district_id, customer_id),
            "order_status",
            ids,
            cfg.invoke_timeout_ms,
        )
        .await
        .map(|_: String| ()),
        TpccOp::Delivery => invoke_op(
            client,
            session_id,
            &proc_name(app_name, "tpcc_delivery"),
            (warehouse_id, district_id, 1_i32),
            "delivery",
            ids,
            cfg.invoke_timeout_ms,
        )
        .await
        .map(|_: String| ()),
        TpccOp::StockLevel => invoke_op(
            client,
            session_id,
            &proc_name(app_name, "tpcc_stock_level"),
            (warehouse_id, district_id, 95_i32),
            "stock_level",
            ids,
            cfg.invoke_timeout_ms,
        )
        .await
        .map(|_: i32| ()),
    }
}

/// Wraps one typed procedure invoke in the configured timeout; a timeout is
/// reported as a Network error naming the operation.
async fn invoke_op<T: TupleDatum, R: TupleDatum>(
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
    op: &str,
    ids: (usize, usize, usize),
    timeout_ms: u64,
) -> RS<R> {
    let (terminal_id, op_idx, global_idx) = ids;
    mudu_sys::timeout(
        Duration::from_millis(timeout_ms),
        invoke_typed(client, session_id, procedure_name, tuple),
    )
    .await
    .ok_or_else(|| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            format!(
                "invoke timeout terminal_id={} op={} op_idx={} global_idx={}",
                terminal_id, op, op_idx, global_idx
            )
        )
    })?
}

/// Records one invoke outcome: retryable aborts are counted and skipped,
/// commits update the latency stats; any other error ends the terminal.
fn record_op_outcome(
    report: &mut TerminalReport,
    op: TpccOp,
    args: &OpArgs<'_>,
    started: mudu_sys::time::Instant,
    op_result: RS<()>,
) -> RS<()> {
    let &OpArgs {
        terminal_id,
        op_idx,
        global_idx,
        cfg,
        ..
    } = args;
    if let Err(err) = op_result {
        if is_retryable_abort(&err, cfg.treat_tx_errors_as_aborts) {
            report.aborted += 1;
            if cfg.trace_ops {
                debug!(
                    terminal_id,
                    op_idx,
                    global_idx,
                    op = op_name(op),
                    err = %err,
                    "invoke aborted"
                );
            }
            return Ok(());
        }
        return Err(err);
    }

    report.committed += 1;
    report
        .latency_us
        .push(started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
    if cfg.trace_ops {
        debug!(
            terminal_id,
            op_idx,
            global_idx,
            op = op_name(op),
            elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
            "invoke done"
        );
    }
    Ok(())
}

pub(super) async fn invoke_void<T: TupleDatum>(
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
) -> RS<()> {
    let payload = serialize_param(tuple)?;
    let result_binary = client
        .invoke_procedure(mudu_contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            mudu_error!(
                mudu::error::ErrorCode::Network,
                "invoke void procedure error",
                e
            )
        })?
        .into_result();
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    let _: () = result.to(&<() as TupleDatum>::tuple_desc_static(&[]))?;
    Ok(())
}

async fn invoke_typed<T: TupleDatum, R: TupleDatum>(
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
) -> RS<R> {
    let payload = serialize_param(tuple)?;
    let result_binary = client
        .invoke_procedure(mudu_contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            mudu_error!(
                mudu::error::ErrorCode::Network,
                "invoke typed procedure error",
                e
            )
        })?
        .into_result();
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    result.to(&<R as TupleDatum>::tuple_desc_static(&[]))
}

fn serialize_param<T: TupleDatum>(tuple: T) -> RS<Vec<u8>> {
    let desc = T::tuple_desc_static(&[]);
    let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
    procedure_invoke::serialize_param(param)
}

pub(super) fn proc_name(app: &str, proc: &str) -> String {
    format!("{app}/tpcc/{proc}")
}

#[derive(Clone, Copy)]
enum TpccOp {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
}

fn op_name(op: TpccOp) -> &'static str {
    match op {
        TpccOp::NewOrder => "new_order",
        TpccOp::Payment => "payment",
        TpccOp::OrderStatus => "order_status",
        TpccOp::Delivery => "delivery",
        TpccOp::StockLevel => "stock_level",
    }
}

fn op_for(index: usize, new_order_percent: usize, payment_percent: usize) -> TpccOp {
    let bucket = index % 100;
    if bucket < new_order_percent {
        TpccOp::NewOrder
    } else if bucket < new_order_percent + payment_percent {
        TpccOp::Payment
    } else if bucket < 85 {
        TpccOp::OrderStatus
    } else if bucket < 93 {
        TpccOp::Delivery
    } else {
        TpccOp::StockLevel
    }
}

fn value_for(index: usize, modulo: i32) -> i32 {
    (index as i32 % modulo) + 1
}

fn new_order_lines(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let line_count = (index % 5) + 3;
    let mut item_ids = Vec::with_capacity(line_count);
    let mut supplier_warehouse_ids = Vec::with_capacity(line_count);
    let mut quantities = Vec::with_capacity(line_count);
    for line_idx in 0..line_count {
        item_ids.push(value_for(index * 7 + line_idx * 3 + 1, item_count));
        let supplier_warehouse_id = if warehouse_count > 1 && line_idx % 3 == 2 {
            value_for(index + line_idx + 1, warehouse_count)
        } else {
            warehouse_id
        };
        supplier_warehouse_ids.push(supplier_warehouse_id);
        quantities.push(((index + line_idx) % 10) as i32 + 1);
    }
    (item_ids, supplier_warehouse_ids, quantities)
}

pub(super) fn percentile_ms(latency_us: &mut [u64], percentile: f64) -> f64 {
    if latency_us.is_empty() {
        return 0.0;
    }
    latency_us.sort_unstable();
    let n = latency_us.len();
    let p = percentile.clamp(0.0, 100.0) / 100.0;
    let rank = ((n as f64) * p).ceil().max(1.0) as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    latency_us[idx] as f64 / 1000.0
}

fn is_retryable_abort(err: &mudu::error::MuduError, treat_tx_errors_as_aborts: bool) -> bool {
    if err.ec() == mudu::error::ErrorCode::Transaction
        && (treat_tx_errors_as_aborts || err.message().contains("write-write conflict"))
    {
        return true;
    }
    match err.err_src() {
        mudu::error::ErrorSource::MuduError(src) => {
            is_retryable_abort(&src, treat_tx_errors_as_aborts)
        }
        mudu::error::ErrorSource::Other(msg) => msg.contains("write-write conflict"),
        mudu::error::ErrorSource::None => false,
    }
}

pub(super) struct TerminalReport {
    pub(super) committed: usize,
    pub(super) new_order_committed: usize,
    pub(super) aborted: usize,
    pub(super) latency_us: Vec<u64>,
}
