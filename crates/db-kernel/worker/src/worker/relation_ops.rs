use super::*;
use ::catalog::contract::app_schema::current_schema;
use ::xengine::api::{
    DeltaAssign, DeltaOp, OptInsert, OptRead, OptUpdate, Predicate, VecDatum, VecSelTerm,
};
use ::xengine::DataBin;
use mudu::common::id::AttrIndex;

impl WorkerRuntime {
    /// Resolve a relation (table) name to its object id, failing with
    /// `EntityNotFound` when the table does not exist. The bare name resolves
    /// against the schema of the requesting application (search path
    /// `[current_schema, mududb]` inside the catalog).
    async fn relation_table_id(&self, app_name: Option<&str>, table: &str) -> RS<OID> {
        let desc = self
            .meta_mgr()
            .get_table_by_name(&current_schema(app_name), table)
            .await?;
        let desc = desc.ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such relation: {table}")
            )
        })?;
        Ok(desc.id())
    }

    /// Point-read one relation row by primary key through `XContract`,
    /// bypassing SQL parsing and result-set serialization. Runs inside the
    /// session transaction when one is active (procedure mode begins it in
    /// `pre_invoke`); otherwise the read executes as an autocommit statement.
    pub(crate) async fn relation_get_in_session(
        &self,
        session_id: OID,
        app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let table_id = self.relation_table_id(app_name, table).await?;
        let pred_key = VecDatum::new(key);
        let select = VecSelTerm::new(select);
        if self.session_manager.has_session_tx(session_id)? {
            let tx_mgr = self.sql_tx_mgr(session_id)?.ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "session transaction is missing")
            })?;
            return self
                .contract
                .read_key(tx_mgr, table_id, &pred_key, &select, &OptRead::default())
                .await;
        }
        let tx_mgr = self.contract.begin_tx().await?;
        let result = self
            .contract
            .read_key(
                tx_mgr.clone(),
                table_id,
                &pred_key,
                &select,
                &OptRead::default(),
            )
            .await;
        if result.is_ok() {
            self.contract.commit_tx(tx_mgr).await?;
        } else {
            self.contract.abort_tx(tx_mgr).await?;
        }
        result
    }

    /// Read-modify-write one relation row by primary key through `XContract`
    /// (statement lock, MVCC, delta assignments evaluated on the locked row).
    /// Session-transaction and autocommit handling mirrors
    /// [`Self::relation_get_in_session`].
    pub(crate) async fn relation_update_in_session(
        &self,
        session_id: OID,
        app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
        deltas: Vec<(AttrIndex, DeltaOp, DataBin)>,
    ) -> RS<u64> {
        let table_id = self.relation_table_id(app_name, table).await?;
        let pred_key = VecDatum::new(key);
        let values = VecDatum::new(values);
        let opt_update = OptUpdate {
            delta_assignments: deltas
                .into_iter()
                .map(|(attr, op, literal)| DeltaAssign { attr, op, literal })
                .collect(),
            // The relation-update payload carries absolute datums and deltas
            // only; it cannot express a NULL assignment.
            null_assignments: Vec::new(),
        };
        if self.session_manager.has_session_tx(session_id)? {
            let tx_mgr = self.sql_tx_mgr(session_id)?.ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "session transaction is missing")
            })?;
            return self
                .contract
                .update(
                    tx_mgr,
                    table_id,
                    &pred_key,
                    &Predicate::CNF(vec![]),
                    &values,
                    &opt_update,
                )
                .await
                .map(|affected| affected as u64);
        }
        let tx_mgr = self.contract.begin_tx().await?;
        let result = self
            .contract
            .update(
                tx_mgr.clone(),
                table_id,
                &pred_key,
                &Predicate::CNF(vec![]),
                &values,
                &opt_update,
            )
            .await;
        if result.is_ok() {
            self.contract.commit_tx(tx_mgr).await?;
        } else {
            self.contract.abort_tx(tx_mgr).await?;
        }
        result.map(|affected| affected as u64)
    }

    /// Insert one relation row through `XContract` (duplicate primary keys
    /// fail with `EntityAlreadyExists`). Session-transaction and autocommit
    /// handling mirrors [`Self::relation_get_in_session`].
    pub(crate) async fn relation_insert_in_session(
        &self,
        session_id: OID,
        app_name: Option<&str>,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<()> {
        let table_id = self.relation_table_id(app_name, table).await?;
        let keys = VecDatum::new(key);
        let values = VecDatum::new(values);
        if self.session_manager.has_session_tx(session_id)? {
            let tx_mgr = self.sql_tx_mgr(session_id)?.ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "session transaction is missing")
            })?;
            return self
                .contract
                .insert(tx_mgr, table_id, &keys, &values, &OptInsert::default())
                .await;
        }
        let tx_mgr = self.contract.begin_tx().await?;
        let result = self
            .contract
            .insert(
                tx_mgr.clone(),
                table_id,
                &keys,
                &values,
                &OptInsert::default(),
            )
            .await;
        if result.is_ok() {
            self.contract.commit_tx(tx_mgr).await?;
        } else {
            self.contract.abort_tx(tx_mgr).await?;
        }
        result
    }
}
