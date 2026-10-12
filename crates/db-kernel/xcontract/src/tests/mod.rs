#[cfg(test)]
use crate::*;
#[cfg(test)]
use ::catalog::contract::schema_column::SchemaColumn;
#[cfg(test)]
use ::catalog::contract::table_info::TableInfo;
#[cfg(test)]
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
#[cfg(test)]
use mudu_type::data_type_fn_param::DataType;
#[cfg(test)]
use mudu_type::data_type_info::DataTypeInfo;
#[cfg(test)]
use mudu_type::type_family::TypeFamily;
#[cfg(test)]
use std::future::Future;

#[cfg(test)]
mod ops;
#[cfg(test)]
mod tuple_update;
#[cfg(test)]
mod wal_replay;

#[cfg(test)]
fn block_on<F>(fut: F) -> F::Output
where
    F: Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

#[cfg(test)]
fn test_schema() -> SchemaTable {
    SchemaTable::new(
        "t".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

#[cfg(test)]
fn datum(v: i32) -> Vec<u8> {
    v.to_be_bytes().to_vec()
}

#[cfg(test)]
fn key_row(v: i32) -> VecDatum {
    VecDatum::new(vec![(0, datum(v))])
}

#[cfg(test)]
fn value_row(v: i32) -> VecDatum {
    VecDatum::new(vec![(1, datum(v))])
}

#[cfg(test)]
fn datum_string(v: &str) -> Vec<u8> {
    mudu_type::data_type_function::send_binary(
        &mudu_type::data_value::DataValue::from_string(v.to_string()),
        &mudu_type::data_type::DataType::default_for(mudu_type::type_family::TypeFamily::String),
    )
    .unwrap()
}

#[cfg(test)]
fn wallet_users_schema() -> SchemaTable {
    use ::catalog::contract::schema_column::SchemaColumn;
    use mudu_type::data_type_info::DataTypeInfo;

    SchemaTable::new(
        "users".to_string(),
        vec![
            SchemaColumn::new(
                "user_id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "name".to_string(),
                TypeFamily::String,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
            ),
            SchemaColumn::new(
                "phone".to_string(),
                TypeFamily::String,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
            ),
            SchemaColumn::new(
                "email".to_string(),
                TypeFamily::String,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
            ),
            SchemaColumn::new(
                "password".to_string(),
                TypeFamily::String,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::String)),
            ),
            SchemaColumn::new(
                "created_at".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
            SchemaColumn::new(
                "updated_at".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_opt_object(&DataType::default_for(TypeFamily::I32)),
            ),
        ],
        vec![0],
        vec![1, 2, 3, 4, 5, 6],
    )
}

#[cfg(test)]
async fn read_i32_value(
    contract: &WorkerXContract,
    table_id: OID,
    key: i32,
) -> RS<Option<Vec<u8>>> {
    let tx = contract.begin_tx().await?;
    let row = contract
        .read_key(
            tx.clone(),
            table_id,
            &key_row(key),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await?;
    contract.abort_tx(tx).await?;
    Ok(row.and_then(|mut row| row.pop().flatten()))
}

#[cfg(test)]
fn meta_table(schema: &SchemaTable) -> RS<Arc<TableDesc>> {
    TableInfo::new(schema.clone())?.table_desc()
}

#[cfg(test)]
async fn make_contract_with_table(schema: &SchemaTable) -> RS<(Arc<WorkerXContract>, OID)> {
    let meta_mgr = Arc::new(TestMetaMgr::new());
    let table_id = schema.id();
    let contract = WorkerXContract::with_log(meta_mgr, None)?;
    let ddl_tx = contract.begin_tx().await?;
    contract.create_table(ddl_tx.clone(), schema).await?;
    contract.commit_tx(ddl_tx).await?;
    Ok((Arc::new(contract), table_id))
}
