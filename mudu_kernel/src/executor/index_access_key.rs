use crate::contract::meta_mgr::MetaMgr;
use crate::contract::query_exec::QueryExec;
use crate::executor::project_tuple_desc;
use crate::x_engine::api::{TupleRow, XContract};
use crate::x_engine::x_param::PAccessKey;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc as TupleDesc;
use mudu_sys::sync::async_::futures_mutex::FMutex;
use std::sync::Arc;

pub struct IndexAccessKey {
    tuple_desc: TupleDesc,
    inner: FMutex<_IndexAccessKey>,
}

struct _IndexAccessKey {
    param: PAccessKey,
    x_contract: Arc<dyn XContract>,
    fetched: bool,
}

impl IndexAccessKey {
    pub async fn new(
        param: PAccessKey,
        x_contract: Arc<dyn XContract>,
        meta_mgr: Arc<dyn MetaMgr>,
    ) -> RS<Self> {
        let table_desc = meta_mgr.get_table_by_id(param.table_id).await?;
        let tuple_desc = project_tuple_desc(&table_desc, &param.select);
        Ok(Self {
            tuple_desc,
            inner: FMutex::new(_IndexAccessKey::new(param, x_contract)),
        })
    }
}

#[async_trait]
impl QueryExec for IndexAccessKey {
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

impl _IndexAccessKey {
    fn new(param: PAccessKey, x_contract: Arc<dyn XContract>) -> Self {
        Self {
            param,
            x_contract,
            fetched: false,
        }
    }

    async fn open(&mut self) -> RS<()> {
        self.fetched = false;
        Ok(())
    }

    async fn next(&mut self) -> RS<Option<TupleRow>> {
        if self.fetched {
            return Ok(None);
        }
        self.fetched = true;

        let p = &self.param;
        let row = self
            .x_contract
            .read_key(
                p.tx_mgr.clone(),
                p.table_id,
                &p.pred_key,
                &p.select,
                &p.opt_read,
            )
            .await?;
        Ok(row.map(TupleRow::new_nullable))
    }
}

unsafe impl Send for IndexAccessKey {}

unsafe impl Sync for IndexAccessKey {}
