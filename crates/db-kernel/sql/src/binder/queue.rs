//! Queue statement binding methods of the `Binder` impl (CREATE/DROP QUEUE,
//! RECEIVE/ACK and the named consumer cursors).
use super::*;

impl Binder {
    /// Bind a queue `RECEIVE` statement: resolve the queue through the search
    /// path and build the envelope descriptor (`__seq`, `__lsn`, then the
    /// payload columns).
    pub(super) async fn bind_receive(&self, stmt: &StmtReceive) -> RS<BoundReceive> {
        let desc = self.resolve_queue(stmt.name()).await?;
        let count = stmt.count().unwrap_or(1);
        if count == 0 {
            return Err(mudu_error!(
                ER::InvalidArgument,
                "RECEIVE count must be at least 1"
            ));
        }
        Ok(BoundReceive {
            table_id: desc.id(),
            count,
            wait: stmt.wait(),
            consumer: stmt.consumer().map(str::to_string),
            tuple_desc: queue_envelope_tuple_desc(desc.as_ref()),
        })
    }

    /// Bind a queue `ACK` statement: resolve the queue through the search
    /// path.
    pub(super) async fn bind_ack(&self, stmt: &StmtAck) -> RS<BoundAck> {
        let desc = self.resolve_queue(stmt.name()).await?;
        Ok(BoundAck {
            table_id: desc.id(),
            seq: stmt.seq(),
            consumer: stmt.consumer().map(str::to_string),
        })
    }

    /// Bind a `CREATE CONSUMER` statement: resolve the queue the consumer
    /// cursor is registered on through the search path.
    pub(super) async fn bind_create_consumer(
        &self,
        stmt: &StmtCreateConsumer,
    ) -> RS<BoundCreateConsumer> {
        let desc = self.resolve_queue(stmt.queue()).await?;
        Ok(BoundCreateConsumer {
            table_id: desc.id(),
            consumer: stmt.consumer().to_string(),
        })
    }

    /// Bind a `DROP CONSUMER` statement: resolve the queue the consumer
    /// cursor belongs to through the search path.
    pub(super) async fn bind_drop_consumer(
        &self,
        stmt: &StmtDropConsumer,
    ) -> RS<BoundDropConsumer> {
        let desc = self.resolve_queue(stmt.queue()).await?;
        Ok(BoundDropConsumer {
            table_id: desc.id(),
            consumer: stmt.consumer().to_string(),
        })
    }

    pub(super) async fn bind_create_queue(&self, stmt: StmtCreateQueue) -> RS<BoundCreateQueue> {
        let payload_columns = match stmt.payload() {
            QueuePayload::Like(class_name) => self.queue_payload_from_like(class_name).await?,
            QueuePayload::Columns(columns) => self.queue_payload_from_columns(columns).await?,
        };
        let mut columns = queue_system_columns();
        columns.extend(payload_columns);
        let key_indices = vec![0];
        let value_indices = (1..columns.len()).collect();
        // Queues can be partition-routed, so their oid derives from the
        // qualified name exactly like tables (see `bind_create_table`).
        let mut schema = SchemaClass::new_with_oid(
            ::catalog::contract::derive_oid::derive_table_oid(&self.current_schema, stmt.name()),
            stmt.name().to_string(),
            columns,
            key_indices,
            value_indices,
        );
        // Unqualified CREATE QUEUE lands in the current schema, the same
        // convention as CREATE TABLE.
        schema.set_schema(self.current_schema.clone());
        schema.set_kind(SchemaClassKind::Queue);
        let partition_binding = match stmt.partition() {
            Some(partition) => Some(self.bind_queue_partition(&schema, partition).await?),
            None => None,
        };
        Ok(BoundCreateQueue {
            schema,
            partition_binding,
        })
    }

    pub(super) async fn bind_drop_queue(&self, stmt: StmtDropQueue) -> RS<BoundDropQueue> {
        // `get_queue` resolves through the same search path as a table
        // reference and reports `None` when the name resolves to a non-queue
        // object, so `DROP QUEUE` aimed at a plain table fails here.
        match self
            .meta_mgr
            .get_queue(&self.current_schema, stmt.name())
            .await?
        {
            Some(queue_desc) => Ok(BoundDropQueue {
                oid: queue_desc.id(),
            }),
            None => Err(mudu_error!(
                ER::ObjectNotFound,
                format!("no such queue {}", stmt.name())
            )),
        }
    }

    /// Binds the queue sharding clause: one payload column routed through a
    /// range rule whose partition count fits the `__seq` shard bits.
    async fn bind_queue_partition(
        &self,
        schema: &SchemaClass,
        partition: &sql_parser::ast::stmt_table_partition::StmtTablePartition,
    ) -> RS<TablePartitionBinding> {
        if partition.reference_columns().len() != 1 {
            return Err(mudu_error!(
                ER::InvalidArgument,
                "queue partition clause must reference exactly one payload column"
            ));
        }
        let (binding, rule) = self.build_partition_binding(schema, partition).await?;
        let attr = binding.ref_attr_indices[0];
        if attr < QUEUE_PAYLOAD_ATTR_START {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!(
                    "queue partition column {} must be a payload column, not a system column",
                    partition.reference_columns()[0]
                )
            ));
        }
        let column = &schema.columns()[attr];
        if rule.key_types != [column.type_id()] {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!(
                    "queue partition column {} type {:?} does not match rule {} key types {:?}",
                    partition.reference_columns()[0],
                    column.type_id(),
                    rule.name,
                    rule.key_types
                )
            ));
        }
        if rule.partitions.len() as u64 > QUEUE_SEQ_MAX_SHARD + 1 {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!(
                    "partition rule {} has {} partitions, a queue supports at most {}",
                    rule.name,
                    rule.partitions.len(),
                    QUEUE_SEQ_MAX_SHARD + 1
                )
            ));
        }
        Ok(binding)
    }

    // `LIKE <table>` payload: copy every source column (key and value alike)
    // as a plain payload column — name, type and nullability are inherited,
    // while primary-key/index marks and fs bindings are not (the queue primary
    // key is always `__seq`).
    async fn queue_payload_from_like(&self, class_name: &str) -> RS<Vec<SchemaColumn>> {
        let source = self
            .meta_mgr
            .get_table_by_name(&self.current_schema, class_name)
            .await?
            .ok_or_else(|| {
                mudu_error!(ER::ObjectNotFound, format!("no such table {}", class_name))
            })?;
        let mut columns = Vec::with_capacity(source.fields().len());
        for field in source.fields() {
            Self::check_queue_payload_name(field.name())?;
            let ty = field.type_desc();
            let mut column = SchemaColumn::new(
                field.name().clone(),
                ty.type_family(),
                mudu_type::data_type_codec::from_opt_object(ty),
            );
            column.set_nullable(field.nullable());
            columns.push(column);
        }
        Ok(columns)
    }

    async fn queue_payload_from_columns(
        &self,
        columns: &[sql_parser::ast::column_def::ColumnDef],
    ) -> RS<Vec<SchemaColumn>> {
        let mut result = Vec::with_capacity(columns.len());
        for column in columns {
            if column.is_primary_key() {
                return Err(mudu_error!(
                    ER::InvalidArgument,
                    format!(
                        "queue payload column {} cannot be a primary key: the queue primary key is always {}",
                        column.column_name(),
                        QUEUE_SEQ_COLUMN
                    )
                ));
            }
            Self::check_queue_payload_name(column.column_name())?;
            let schema_column = self.schema_column_from_ast(column).await?;
            if schema_column.fs_binding().is_some() {
                // The enqueue path synthesizes the system columns and skips
                // the fs DML hooks, so an fs-bound payload column would never
                // get its object binding.
                return Err(mudu_error!(
                    ER::NotImplemented,
                    format!(
                        "queue payload column {} with a filesystem type is not supported yet",
                        column.column_name()
                    )
                ));
            }
            result.push(schema_column);
        }
        Ok(result)
    }

    fn check_queue_payload_name(name: &str) -> RS<()> {
        if is_queue_system_column(name) {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!("queue payload column name {} is reserved", name)
            ));
        }
        Ok(())
    }

    async fn resolve_queue(&self, name: &str) -> RS<Arc<TableDesc>> {
        self.meta_mgr
            .get_queue(&self.current_schema, name)
            .await?
            .ok_or_else(|| mudu_error!(ER::ObjectNotFound, format!("no such queue {}", name)))
    }
}

/// Whether `stmt` is a queue `RECEIVE` carrying a `WAIT`/`WAIT FOREVER`
/// clause. The worker SQL layer rejects such statements inside an explicit
/// transaction: WAIT parks the statement between short claim transactions,
/// which only autocommit mode can provide.
pub fn is_receive_wait_stmt(stmt: &StmtType) -> bool {
    matches!(
        stmt,
        StmtType::Command(StmtCommand::Receive(receive)) if receive.wait().is_some()
    )
}
