use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_utils::sync::f_mutex::FMutex;
use std::sync::Arc;

use crate::contract::query_exec::QueryExec;

pub struct MuduResultSetAsync {
    desc: Arc<TupleFieldDesc>,
    inner: FMutex<ResultRows>,
}

struct ResultRows {
    rows: Vec<TupleValue>,
    index: usize,
}

impl MuduResultSetAsync {
    pub fn from_rows(rows: Vec<TupleValue>, desc: TupleFieldDesc) -> Self {
        Self {
            desc: Arc::new(desc),
            inner: FMutex::new(ResultRows { rows, index: 0 }),
        }
    }

    pub async fn from_query_exec(exec: Arc<dyn QueryExec>) -> RS<Self> {
        let (rows, desc) = super::mudu_conn_core::query_exec_to_rows(exec).await?;
        Ok(Self::from_rows(rows, desc))
    }
}

#[async_trait]
impl ResultSetAsync for MuduResultSetAsync {
    async fn next(&self) -> RS<Option<TupleValue>> {
        let mut inner = self.inner.lock().await;
        if inner.index >= inner.rows.len() {
            return Ok(None);
        }
        let index = inner.index;
        let row = inner.rows.remove(index);
        Ok(Some(row))
    }

    fn desc(&self) -> &TupleFieldDesc {
        self.desc.as_ref()
    }
}
