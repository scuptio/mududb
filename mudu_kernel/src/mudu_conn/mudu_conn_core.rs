use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_contract::tuple::typed_bin::TypedBin;
use mudu_type::datum::DatumDyn;
use mudu_utils::{scoped_task_trace, task_trace};
use sql_parser::ast::parser::SQLParser;
use sql_parser::ast::stmt_type::StmtType;
use std::sync::Arc;

use crate::contract::meta_mgr::MetaMgr;
use crate::contract::query_exec::QueryExec;
use crate::mudu_conn::mudu_result_set_async::MuduResultSetAsync;
use crate::sql::binder::Binder;
use crate::sql::bound_stmt::{BoundCommand, BoundStmt};
use crate::sql::describer::Describer;
use crate::sql::plan_ctx::PlanCtx;
use crate::sql::planner::Planner;
use crate::x_engine::api::XContract;
use crate::x_engine::tx_mgr::TxMgr;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;

pub struct MuduConnCore {
    meta_mgr: Arc<dyn MetaMgr>,
    parser: Arc<SQLParser>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    is_admin: bool,
}

impl MuduConnCore {
    pub fn new(
        meta_mgr: Arc<dyn MetaMgr>,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
        is_admin: bool,
    ) -> RS<Self> {
        Ok(Self {
            meta_mgr,
            parser: Arc::new(SQLParser::new()?),
            async_runtime,
            is_admin,
        })
    }

    pub fn parse_one(&self, sql: &dyn SQLStmt) -> RS<Arc<StmtType>> {
        self.parse_one_text(&sql.to_sql_string())
    }

    /// Parses exactly one statement from `text`, using the process-wide parse
    /// cache. Callers that already rendered the SQL text (e.g. the worker's
    /// plan-cache path) avoid a second `to_sql_string` this way.
    pub fn parse_one_text(&self, text: &str) -> RS<Arc<StmtType>> {
        crate::mudu_conn::stmt_parse_cache::parse_one_cached(text, |text| {
            let stmt_list = self.parser.parse(text)?;
            let mut stmts = stmt_list.into_stmts();
            if stmts.len() != 1 {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    "expected exactly one statement"
                ));
            }
            Ok(stmts.remove(0))
        })
    }

    pub fn parse_many(&self, sql: &dyn SQLStmt) -> RS<Vec<StmtType>> {
        Ok(self.parser.parse(&sql.to_sql_string())?.into_stmts())
    }

    pub async fn describe_stmt(&self, stmt: &StmtType) -> RS<Arc<TupleFieldDesc>> {
        let desc = Describer::describe(self.meta_mgr.as_ref(), stmt).await?;
        Ok(Arc::new(desc))
    }

    pub async fn query(
        &self,
        stmt: &StmtType,
        params: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<Arc<dyn mudu_contract::database::result_set::ResultSetAsync>> {
        let (rows, desc) = self.query_rows(stmt, params, tx_mgr, x_contract).await?;
        Ok(Arc::new(MuduResultSetAsync::from_rows(rows, desc)))
    }

    pub async fn query_rows(
        &self,
        stmt: &StmtType,
        params: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<(Vec<TupleValue>, TupleFieldDesc)> {
        self.query_inner(stmt, params, tx_mgr, x_contract).await
    }

    pub async fn execute(
        &self,
        stmt: &StmtType,
        params: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<u64> {
        scoped_task_trace!();
        self.execute_inner(stmt, params, tx_mgr, x_contract).await
    }

    async fn query_inner(
        &self,
        stmt: &StmtType,
        params: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<(Vec<TupleValue>, TupleFieldDesc)> {
        let trace = task_trace!();
        trace.watch("query.stage", "bind");
        let bound = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlBind,
            );
            Binder::new(self.meta_mgr.clone())
                .bind_ref(stmt, params.as_ref())
                .await?
        };
        let BoundStmt::Query(bound_query) = bound else {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                "statement is not a query"
            ));
        };
        let planner = Planner::new(PlanCtx {
            tx_mgr,
            meta_mgr: self.meta_mgr.clone(),
            x_contract,
            async_runtime: self.async_runtime.clone(),
        });
        trace.watch("query.stage", "plan");
        let exec = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlPlan,
            );
            planner.plan_query(bound_query).await?
        };
        trace.watch("query.stage", "exec_rows");
        let _stage =
            crate::server::stage_stats::StageGuard::new(crate::server::stage_stats::Stage::SqlRun);
        query_exec_to_rows(exec).await
    }

    async fn execute_inner(
        &self,
        stmt: &StmtType,
        params: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
        x_contract: Arc<dyn XContract>,
    ) -> RS<u64> {
        let trace = task_trace!();
        trace.watch("procedure.core_execute.stage", "bind_start");
        let bound = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlBind,
            );
            Binder::new(self.meta_mgr.clone())
                .bind_ref(stmt, params.as_ref())
                .await?
        };
        trace.watch("procedure.core_execute.stage", "bind_done");
        let BoundStmt::Command(bound_command) = bound else {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                "statement is not a command"
            ));
        };
        if matches!(
            bound_command,
            BoundCommand::CreateFsType(_) | BoundCommand::DropType(_)
        ) && !self.is_admin
        {
            return Err(mudu_error!(
                ErrorCode::PermissionDenied,
                "CREATE/DROP TYPE FILESYSTEM requires an admin session"
            ));
        }
        let planner = Planner::new(PlanCtx {
            tx_mgr,
            meta_mgr: self.meta_mgr.clone(),
            x_contract,
            async_runtime: self.async_runtime.clone(),
        });
        trace.watch("procedure.core_execute.stage", "plan_command_start");
        let cmd = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlPlan,
            );
            planner.plan_command(bound_command).await?
        };
        trace.watch("procedure.core_execute.stage", "plan_command_done");
        trace.watch("procedure.core_execute.stage", "prepare_start");
        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlPrepare,
            );
            cmd.prepare().await?;
        }
        trace.watch("procedure.core_execute.stage", "prepare_done");
        trace.watch("procedure.core_execute.stage", "run_start");
        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlRun,
            );
            cmd.run().await?;
        }
        trace.watch("procedure.core_execute.stage", "run_done");
        trace.watch("procedure.core_execute.stage", "affected_rows_start");
        cmd.affected_rows().await
    }
}

pub async fn query_exec_to_rows(exec: Arc<dyn QueryExec>) -> RS<(Vec<TupleValue>, TupleFieldDesc)> {
    let trace = task_trace!();
    trace.watch("query.exec.stage", "open");
    exec.open().await?;
    let desc = exec.tuple_desc()?;
    let mut rows = Vec::new();
    loop {
        trace.watch("query.exec.stage", "next");
        trace.watch("query.exec.row_index", &rows.len().to_string());
        let next = exec.next().await?;
        let Some(row) = next else {
            trace.watch("query.exec.stage", "done");
            break;
        };
        let value = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::ResultDecode,
            );
            tuple_field_to_value(row, &desc)?
        };
        rows.push(value);
    }
    Ok((rows, desc))
}

pub(crate) fn tuple_field_to_value(
    row: mudu_contract::tuple::tuple_field::TupleField,
    desc: &TupleFieldDesc,
) -> RS<TupleValue> {
    let mut values = Vec::with_capacity(row.fields().len());
    // Consume the row: each field's bytes move straight into the decoder
    // instead of being cloned out of the executor's tuple.
    for (index, field) in row.into_fields().into_iter().enumerate() {
        let datum_desc = &desc.fields()[index];
        match field {
            Some(field) => {
                let typed = TypedBin::new(datum_desc.type_family(), field);
                values.push(typed.to_value(datum_desc.data_type())?);
            }
            None => values.push(mudu_type::data_value::DataValue::null()),
        }
    }
    Ok(TupleValue::from(values))
}
