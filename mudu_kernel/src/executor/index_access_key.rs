use crate::contract::query_exec::QueryExec;
use crate::sync::a_mutex::AMutex;
use crate::x_engine::api::{TupleRow, XContract};
use crate::x_engine::thd_ctx::ThdCtx;
use crate::x_engine::x_param::PAccessKey;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::tuple::tuple_item_desc::TupleItemDesc as TupleDesc;

pub struct IndexAccessKey {
    tuple_desc: TupleDesc,
    inner: AMutex<_IndexAccessKey>,
}

struct _IndexAccessKey {
    param: PAccessKey,
    thd_ctx: ThdCtx,
}

impl IndexAccessKey {
    pub fn new(param: PAccessKey, tuple_desc: TupleDesc, ctx: ThdCtx) -> Self {
        Self {
            tuple_desc,
            inner: AMutex::new(_IndexAccessKey::new(param, ctx)),
        }
    }
}

#[async_trait]
impl QueryExec for IndexAccessKey {
    async fn open(&self) -> RS<()> {
        let inner = self.inner.lock().await;
        (*inner).open().await
    }

    async fn next(&self) -> RS<Option<TupleRow>> {
        let mut inner = self.inner.lock().await;
        (*inner).next().await
    }

    fn tuple_desc(&self) -> RS<TupleDesc> {
        Ok(self.tuple_desc.clone())
    }
}

impl _IndexAccessKey {
    fn new(param: PAccessKey, thd_ctx: ThdCtx) -> Self {
        Self { param, thd_ctx }
    }

    async fn open(&self) -> RS<()> {
        Ok(())
    }

    async fn next(&mut self) -> RS<Option<TupleRow>> {
        let p = &self.param;
        let t = self
            .thd_ctx
            .read_key(p.xid, p.table_id, &p.pred_key, &p.select, &p.opt_read)
            .await?;
        Ok(t.map(|e| TupleRow::new(e)))
    }
}

unsafe impl Send for IndexAccessKey {}

unsafe impl Sync for IndexAccessKey {}
