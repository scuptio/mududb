use crate::contract::query_exec::QueryExec;
use crate::sync::a_mutex::AMutex;
use crate::x_engine::api::{RSCursor, TupleRow, XContract};
use crate::x_engine::thd_ctx::ThdCtx;
use crate::x_engine::x_param::PAccessRange;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::tuple::tuple_item_desc::TupleItemDesc as TupleDesc;
use std::sync::Arc;

pub struct IndexAccessRange {
    inner: AMutex<_IndexAccessRange>,
}

struct _IndexAccessRange {
    param: Option<PAccessRange>,
    cursor: Option<Arc<dyn RSCursor>>,
    thd_ctx: ThdCtx,
}

impl IndexAccessRange {
    fn new(param: PAccessRange, thd_ctx: ThdCtx) -> Self {
        Self {
            inner: AMutex::new(_IndexAccessRange::new(param, thd_ctx)),
        }
    }
}

#[async_trait]
impl QueryExec for IndexAccessRange {
    async fn open(&self) -> RS<()> {
        let mut inner = self.inner.lock().await;
        (*inner).open().await
    }

    async fn next(&self) -> RS<Option<TupleRow>> {
        let mut inner = self.inner.lock().await;
        inner.next().await
    }

    fn tuple_desc(&self) -> RS<TupleDesc> {
        todo!()
    }
}

impl _IndexAccessRange {
    fn new(param: PAccessRange, thd_ctx: ThdCtx) -> Self {
        Self {
            param: Some(param),
            cursor: None,
            thd_ctx,
        }
    }

    async fn open(&mut self) -> RS<()> {
        if self.param.is_some() {
            return Ok(());
        }
        let mut param = None;
        std::mem::swap(&mut self.param, &mut param);
        let p = param.unwrap();
        let t = self
            .thd_ctx
            .read_range(
                p.xid,
                p.table_id,
                &p.pred_key,
                &p.pred_non_key,
                &p.select,
                &p.opt_read,
            )
            .await?;
        self.cursor = Some(t);

        Ok(())
    }

    async fn next(&mut self) -> RS<Option<TupleRow>> {
        match &self.cursor {
            Some(c) => {
                let opt = c.next().await?;
                if opt.is_none() {
                    self.cursor = None;
                }
                Ok(opt)
            }
            None => Ok(None),
        }
    }
}

unsafe impl Send for IndexAccessRange {}

unsafe impl Sync for IndexAccessRange {}
