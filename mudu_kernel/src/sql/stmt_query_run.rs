use crate::contract::query_exec::QueryExec;
use crate::contract::ssn_ctx::SsnCtx;
use crate::sql::current_tx::get_tx;
use crate::sql::proj_list::ProjList;
use crate::sql::stmt_query::StmtQuery;
use async_std::prelude::Stream;
use futures::stream;
use mudu::common::result::RS;
use mudu::data_type::dt_impl::dat_type_id::DatTypeID as TypeID;
use mudu::data_type::dt_impl::dat_typed::DatTyped;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use pgwire::api::portal::Format;
use pgwire::api::results::{DataRowEncoder, FieldInfo};
use pgwire::api::Type as PGDataType;
use pgwire::error::{PgWireError, PgWireResult};
use pgwire::messages::data::DataRow;
use std::sync::Arc;
use tracing::error;

// Run a query execution statement(Select)
pub async fn run_query_stmt(
    stmt: &dyn StmtQuery,
    ctx: &dyn SsnCtx,
) -> RS<(
    Arc<Vec<FieldInfo>>,
    impl Stream<Item=PgWireResult<DataRow>>,
)> {
    let xid = get_tx(ctx).await?;
    let r = run_query_stmt_gut(stmt, ctx).await;
    match r {
        Ok(r) => Ok(r),
        Err(e) => {
            error!("run query error: {}", e);
            todo!();
            //ctx.thd_ctx().abort_tx(xid).await?;
            ctx.end_tx()?;
            Err(e)
        }
    }
}

pub async fn run_query_stmt_gut(
    stmt: &dyn StmtQuery,
    ctx: &dyn SsnCtx,
) -> RS<(
    Arc<Vec<FieldInfo>>,
    impl Stream<Item=PgWireResult<DataRow>>,
)> {
    let (exec, fields) = build_query_exec(stmt, ctx).await?;
    let stream = encode_pg_wire_row_data(&*exec, &fields).await?;
    Ok((fields, stream))
}

async fn build_query_exec(
    stmt: &dyn StmtQuery,
    ctx: &dyn SsnCtx,
) -> RS<(Arc<dyn QueryExec>, Arc<Vec<FieldInfo>>)> {
    stmt.realize(ctx).await?;
    let desc = stmt.proj_list()?;
    let fields = to_pg_field_info(&desc, &Default::default())?;
    let cmd = stmt.build(ctx).await?;
    cmd.open().await?;
    Ok((cmd, Arc::new(fields)))
}

fn to_pg_field_info(rd: &ProjList, format: &Format) -> RS<Vec<FieldInfo>> {
    rd.fields()
        .iter()
        .enumerate()
        .map(|(index, desc)| {
            Ok(FieldInfo::new(
                desc.name().clone(),
                None,
                None,
                dt_id_to_pg_type(desc.type_desc().data_type_id()),
                format.format_for(index),
            ))
        })
        .collect()
}

fn dt_id_to_pg_type(dt: TypeID) -> PGDataType {
    match dt {
        TypeID::I32 => PGDataType::INT4,
        TypeID::I64 => PGDataType::INT8,
        TypeID::F32 => PGDataType::FLOAT4,
        TypeID::F64 => PGDataType::FLOAT8,
        TypeID::CharVarLen => PGDataType::TEXT,
        TypeID::CharFixedLen => PGDataType::TEXT,
    }
}

async fn encode_pg_wire_row_data(
    rows: &dyn QueryExec,
    fields: &Arc<Vec<FieldInfo>>,
) -> RS<impl Stream<Item=PgWireResult<DataRow>>> {
    let mut results: Vec<PgWireResult<DataRow>> = Vec::new();
    let cols = fields.len();
    let mut has_err = false;
    let tuple_desc = rows.tuple_desc()?;
    while let Ok(Some(row)) = rows.next().await {
        if row.fields().len() != cols || tuple_desc.fields().len() != cols {
            return Err(m_error!(ER::FatalError,
                "fatal error: non consistent column number"
            ));
        }
        let mut encoder = DataRowEncoder::new(fields.clone());
        for idx in 0..cols {
            if let Some(datum) = row.get(idx) {
                let field_desc = &tuple_desc.fields()[idx];
                let dat_type_id = field_desc.dat_type_id();
                let internal = dat_type_id.fn_recv()
                    (&datum, field_desc.param_obj()).map_err(|e| {
                    m_error!(ER::TypeBaseErr, "recv error", e)
                })?;
                let value = dat_type_id.fn_to_typed()(&internal, field_desc.param_obj()).map_err(|e| {
                    m_error!(ER::TypeBaseErr, "to_typed error", e)
                })?;

                let r = match value {
                    DatTyped::I32(v) => encoder.encode_field(&v),
                    DatTyped::I64(v) => encoder.encode_field(&v),
                    DatTyped::F32(v) => encoder.encode_field(&v),
                    DatTyped::F64(v) => encoder.encode_field(&v),
                    DatTyped::String(v) => encoder.encode_field(&v),
                };
                if let Err(e) = r {
                    has_err = true;
                    results.push(Err(e));
                }
            } else {
                has_err = true;
                results.push(Err(PgWireError::ApiError(Box::new(ER::IndexOutOfRange))));
                break;
            }
        }
        if !has_err {
            let e = encoder.finish();
            results.push(e);
        }
    }

    Ok(stream::iter(results.into_iter()))
}
