//! DDL and COPY statement binding methods of the `Binder` impl.
use super::*;

impl Binder {
    pub(super) async fn bind_create_table(
        &self,
        mut stmt: StmtCreateTable,
    ) -> RS<BoundCreateTable> {
        stmt.assign_index_for_columns();
        let mut columns = Vec::new();
        for column in stmt.primary_columns() {
            columns.push(self.schema_column_from_ast(column).await?);
        }
        let value_offset = columns.len();
        let mut value_columns = Vec::new();
        for column in stmt.non_primary_columns() {
            value_columns.push(self.schema_column_from_ast(column).await?);
        }
        let key_indices = (0..columns.len()).collect();
        let value_indices = (0..value_columns.len())
            .map(|index| index + value_offset)
            .collect();
        columns.append(&mut value_columns);
        let mut schema = SchemaTable::new(
            stmt.table_name().clone(),
            columns,
            key_indices,
            value_indices,
        );
        // Unqualified CREATE TABLE always lands in the current schema (no
        // search-path fallback); qualified CREATE is rejected at the grammar
        // level, which only accepts a bare identifier here.
        schema.set_schema(self.current_schema.clone());
        let partition_binding = match stmt.partition() {
            Some(partition) => Some(self.build_partition_binding(&schema, partition).await?.0),
            None => None,
        };
        Ok(BoundCreateTable {
            schema,
            partition_binding,
        })
    }

    /// Resolves a `PARTITION BY GLOBAL RULE ... REFERENCES (...)` clause
    /// against the bound schema: returns the binding (referenced columns
    /// mapped to attribute indices) and the resolved rule.
    async fn build_partition_binding(
        &self,
        schema: &SchemaTable,
        partition: &sql_parser::ast::stmt_table_partition::StmtTablePartition,
    ) -> RS<(TablePartitionBinding, PartitionRuleDesc)> {
        let rule = self
            .meta_mgr
            .get_partition_rule_by_name(partition.rule_name())
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ER::EntityNotFound,
                    format!("no such partition rule {}", partition.rule_name())
                )
            })?;
        let ref_attr_indices = partition
            .reference_columns()
            .iter()
            .map(|column| {
                schema
                    .columns()
                    .iter()
                    .position(|field| field.get_name() == column)
                    .ok_or_else(|| {
                        mudu_error!(
                            ER::EntityNotFound,
                            format!("no such partition reference column {}", column)
                        )
                    })
            })
            .collect::<RS<Vec<_>>>()?;
        if rule.partitions.is_empty() {
            return Err(mudu_error!(
                ER::Parse,
                format!("partition rule {} has no partitions", partition.rule_name())
            ));
        }
        Ok((
            TablePartitionBinding {
                table_id: schema.id(),
                rule_id: rule.oid,
                ref_attr_indices,
            },
            rule,
        ))
    }

    pub(super) fn bind_create_partition_rule(
        &self,
        stmt: StmtCreatePartitionRule,
    ) -> RS<BoundCreatePartitionRule> {
        let partitions = stmt
            .partitions()
            .iter()
            .map(|partition| {
                Ok(RangePartitionDef::new(
                    partition.name().to_string(),
                    Self::bind_partition_bound(partition.start()),
                    Self::bind_partition_bound(partition.end()),
                ))
            })
            .collect::<RS<Vec<_>>>()?;
        Ok(BoundCreatePartitionRule {
            rule: PartitionRuleDesc::new_range(
                stmt.rule_name().to_string(),
                Self::infer_partition_rule_key_types(stmt.partitions())?,
                partitions,
            ),
        })
    }

    fn infer_partition_rule_key_types(
        partitions: &[sql_parser::ast::stmt_create_partition_rule::StmtRangePartition],
    ) -> RS<Vec<TypeFamily>> {
        let mut width = None;
        let mut type_slots: Vec<InferredKeyType> = Vec::new();

        for partition in partitions {
            for bound in [partition.start(), partition.end()] {
                let values = match bound {
                    StmtPartitionBound::Unbounded => continue,
                    StmtPartitionBound::Value(values) => values,
                };
                if let Some(expected) = width {
                    if expected != values.len() {
                        return Err(mudu_error!(
                            ER::Parse,
                            "partition bound width mismatch in CREATE PARTITION RULE"
                        ));
                    }
                } else {
                    width = Some(values.len());
                    type_slots = vec![InferredKeyType::I64; values.len()];
                }

                for (index, raw) in values.iter().enumerate() {
                    let next = infer_textual_value_type(raw)?;
                    type_slots[index] = type_slots[index].merge(next);
                }
            }
        }

        match width {
            Some(_) => Ok(type_slots
                .into_iter()
                .map(|item| item.to_type_family())
                .collect()),
            None => Err(mudu_error!(
                ER::Parse,
                "cannot infer partition key types from unbounded rule"
            )),
        }
    }

    pub(super) async fn bind_create_partition_placement(
        &self,
        stmt: StmtCreatePartitionPlacement,
    ) -> RS<BoundCreatePartitionPlacement> {
        let rule = self
            .meta_mgr
            .get_partition_rule_by_name(stmt.rule_name())
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ER::EntityNotFound,
                    format!("no such partition rule {}", stmt.rule_name())
                )
            })?;
        let mut placements = Vec::with_capacity(stmt.placements().len());
        for placement in stmt.placements() {
            let partition = rule
                .partitions
                .iter()
                .find(|partition| partition.name == placement.partition_name())
                .ok_or_else(|| {
                    mudu_error!(
                        ER::EntityNotFound,
                        format!(
                            "no such partition {} in rule {}",
                            placement.partition_name(),
                            stmt.rule_name()
                        )
                    )
                })?;
            let worker_id = placement.worker_id().parse::<u128>().map_err(|e| {
                mudu_error!(
                    ER::Parse,
                    format!("invalid worker id {}", placement.worker_id()),
                    e
                )
            })?;
            placements.push(PartitionPlacement {
                partition_id: partition.partition_id,
                worker_id,
            });
        }
        Ok(BoundCreatePartitionPlacement { placements })
    }

    fn bind_partition_bound(bound: &StmtPartitionBound) -> PartitionBound {
        match bound {
            StmtPartitionBound::Unbounded => PartitionBound::Unbounded,
            StmtPartitionBound::Value(values) => PartitionBound::Value(values.clone()),
        }
    }

    pub(super) async fn bind_drop_table(&self, stmt: StmtDropTable) -> RS<BoundDropTable> {
        match self.resolve_table_opt(stmt.table_reference()).await? {
            Some(table_desc) => Ok(BoundDropTable {
                oid: Some(table_desc.id()),
            }),
            None if stmt.drop_if_exists() => Ok(BoundDropTable { oid: None }),
            None => Err(mudu_error!(
                ER::EntityNotFound,
                format!("cannot find table {}", stmt.table_name())
            )),
        }
    }

    pub(super) async fn bind_create_queue(&self, stmt: StmtCreateQueue) -> RS<BoundCreateQueue> {
        let payload_columns = match stmt.payload() {
            QueuePayload::Like(table_name) => self.queue_payload_from_like(table_name).await?,
            QueuePayload::Columns(columns) => self.queue_payload_from_columns(columns).await?,
        };
        let mut columns = queue_system_columns();
        columns.extend(payload_columns);
        let key_indices = vec![0];
        let value_indices = (1..columns.len()).collect();
        let mut schema =
            SchemaTable::new(stmt.name().to_string(), columns, key_indices, value_indices);
        // Unqualified CREATE QUEUE lands in the current schema, the same
        // convention as CREATE TABLE.
        schema.set_schema(self.current_schema.clone());
        schema.set_kind(SchemaObjectKind::Queue);
        let partition_binding = match stmt.partition() {
            Some(partition) => Some(self.bind_queue_partition(&schema, partition).await?),
            None => None,
        };
        Ok(BoundCreateQueue {
            schema,
            partition_binding,
        })
    }

    /// Binds the queue sharding clause: one payload column routed through a
    /// range rule whose partition count fits the `__seq` shard bits.
    async fn bind_queue_partition(
        &self,
        schema: &SchemaTable,
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
    async fn queue_payload_from_like(&self, table_name: &str) -> RS<Vec<SchemaColumn>> {
        let source = self
            .meta_mgr
            .get_table_by_name(&self.current_schema, table_name)
            .await?
            .ok_or_else(|| {
                mudu_error!(ER::EntityNotFound, format!("no such table {}", table_name))
            })?;
        let mut columns = Vec::with_capacity(source.fields().len());
        for field in source.fields() {
            Self::check_queue_payload_name(field.name())?;
            let ty = field.type_desc();
            let mut column = SchemaColumn::new(
                field.name().clone(),
                ty.type_family(),
                DataTypeInfo::from_opt_object(ty),
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
                ER::EntityNotFound,
                format!("no such queue {}", stmt.name())
            )),
        }
    }

    pub(super) fn bind_create_fs_type(stmt: &StmtCreateFsType) -> BoundCreateFsType {
        let kind = match stmt.kind() {
            AstFsTypeKind::File => FsTypeKind::File,
            AstFsTypeKind::Directory => FsTypeKind::Directory,
        };
        BoundCreateFsType {
            name: stmt.name().to_string(),
            kind,
        }
    }

    pub(super) fn bind_drop_type(stmt: &StmtDropType) -> BoundDropType {
        BoundDropType {
            name: stmt.name().to_string(),
        }
    }

    pub(super) async fn bind_copy_from(
        &self,
        stmt: sql_parser::ast::stmt_copy_from::StmtCopyFrom,
    ) -> RS<BoundCopyFrom> {
        let table_desc = self.resolve_table(stmt.table_reference()).await?;
        if table_desc.is_queue() {
            // COPY writes raw key/value rows and would skip the queue system
            // column synthesis; enqueue through INSERT instead.
            return Err(mudu_error!(
                ER::NotImplemented,
                format!(
                    "COPY FROM on queue {} is not supported; enqueue through INSERT",
                    table_desc.name()
                )
            ));
        }
        let layout = CopyLayout::new(&table_desc, stmt.table_columns())?;
        Ok(BoundCopyFrom {
            file_path: stmt.copy_from_file_path().clone(),
            table_id: table_desc.id(),
            key_index: layout.key_index().to_vec(),
            value_index: layout.value_index().to_vec(),
        })
    }

    pub(super) async fn bind_copy_to(
        &self,
        stmt: sql_parser::ast::stmt_copy_to::StmtCopyTo,
    ) -> RS<BoundCopyTo> {
        let table_desc = self.resolve_table(stmt.table_reference()).await?;
        let layout = CopyLayout::new(&table_desc, stmt.table_columns())?;
        Ok(BoundCopyTo {
            file_path: stmt.copy_to_file_path().clone(),
            table_id: table_desc.id(),
            key_indexing: layout.key_index().to_vec(),
            value_indexing: layout.value_index().to_vec(),
        })
    }
}
