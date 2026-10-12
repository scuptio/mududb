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
        let partition_binding = if let Some(partition) = stmt.partition() {
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
            Some(TablePartitionBinding {
                table_id: schema.id(),
                rule_id: rule.oid,
                ref_attr_indices,
            })
        } else {
            None
        };
        Ok(BoundCreateTable {
            schema,
            partition_binding,
        })
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
