use crate::contract::meta_mgr::MetaMgr;
use crate::contract::query_exec::QueryExec;
use crate::executor::project_tuple_desc;
use crate::x_engine::api::{RSCursor, TupleRow, XContract};
use crate::x_engine::x_param::PAccessRange;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc as TupleDesc;
use mudu_sys::sync::futures_mutex::FMutex;
use std::sync::Arc;

pub struct IndexAccessRange {
    tuple_desc: TupleDesc,
    inner: FMutex<_IndexAccessRange>,
}

struct _IndexAccessRange {
    param: PAccessRange,
    cursor: Option<Arc<dyn RSCursor>>,
    x_contract: Arc<dyn XContract>,
}

impl IndexAccessRange {
    pub async fn new(
        param: PAccessRange,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> RS<Self> {
        let table_desc = meta_mgr.get_table_by_id(param.table_id).await?;
        let tuple_desc = project_tuple_desc(&table_desc, &param.select);
        Ok(Self {
            tuple_desc,
            inner: FMutex::new(_IndexAccessRange::new(param, x_contract)),
        })
    }
}

#[async_trait]
impl QueryExec for IndexAccessRange {
    async fn open(&self) -> RS<()> {
        let mut inner = self.inner.lock().await;
        inner.open().await
    }

    async fn next(&self) -> RS<Option<TupleRow>> {
        let mut inner = self.inner.lock().await;
        inner.next().await
    }

    fn tuple_desc(&self) -> RS<TupleDesc> {
        Ok(self.tuple_desc.clone())
    }
}

impl _IndexAccessRange {
    fn new(param: PAccessRange, x_contract: Arc<dyn XContract>) -> Self {
        Self {
            param,
            cursor: None,
            x_contract,
        }
    }

    async fn open(&mut self) -> RS<()> {
        let param = &self.param;
        let cursor = self
            .x_contract
            .read_range(
                param.tx_mgr.clone(),
                param.table_id,
                &param.pred_key,
                &param.pred_non_key,
                &param.select,
                &param.opt_read,
            )
            .await?;
        self.cursor = Some(cursor);
        Ok(())
    }

    async fn next(&mut self) -> RS<Option<TupleRow>> {
        match &self.cursor {
            Some(cursor) => {
                let row = cursor.next().await?;
                if row.is_none() {
                    self.cursor = None;
                }
                Ok(row)
            }
            None => Ok(None),
        }
    }
}

unsafe impl Send for IndexAccessRange {}

unsafe impl Sync for IndexAccessRange {}
