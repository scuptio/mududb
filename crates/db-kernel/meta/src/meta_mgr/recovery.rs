//! Prepare an entire catalog view before publishing any recovered metadata.
use super::*;
use catalog::contract::{partition_rule_binding::PartitionPlacement, schema_class::SchemaClass};

pub(super) struct CatalogImage {
    tables: Vec<(SchemaClass, TableInfo)>,
    rules: Vec<PartitionRuleDesc>,
    bindings: Vec<TablePartitionBinding>,
    placements: Vec<PartitionPlacement>,
    fs_types: Vec<FsTypeDesc>,
}

impl CatalogImage {
    pub(super) async fn load(catalog: &CatalogRelation) -> RS<Self> {
        let mut schemas = load_schemas_from_catalog(&catalog.schema_catalog).await?;
        let builtins = [fs_object_schema(), queue_consumer_schema()];
        for schema in &schemas {
            if builtins.iter().any(|b| b.id() == schema.id())
                || (schema.schema() == DEFAULT_SCHEMA
                    && [FS_OBJECT_TABLE_NAME, QUEUE_CONSUMER_TABLE_NAME]
                        .contains(&schema.class_name().as_str()))
            {
                return Err(mudu_error!(
                    ER::CorruptedData,
                    "persisted user schema conflicts with a built-in system table"
                ));
            }
        }
        schemas.extend(builtins);
        let tables = schemas
            .into_iter()
            .map(|schema| {
                let table = TableInfo::new(schema.clone())?;
                Ok((schema, table))
            })
            .collect::<RS<Vec<_>>>()?;
        let image = Self {
            tables,
            rules: load_partition_rules_from_catalog(&catalog.partition_rule_catalog).await?,
            bindings: load_partition_bindings_from_catalog(&catalog.partition_binding_catalog)
                .await?,
            placements: load_partition_placements_from_catalog(
                &catalog.partition_placement_catalog,
            )
            .await?,
            fs_types: load_fs_types_from_catalog(&catalog.fs_type).await?,
        };
        image.validate_references().map_err(|error| {
            mudu_error!(
                ER::CorruptedData,
                "catalog reference validation failed",
                error
            )
        })?;
        Ok(image)
    }

    /// Builds a catalog image from a versioned snapshot: reconstructs table
    /// info from each user schema (built-in system tables are already present
    /// from initialization) and validates references before any publication.
    pub(crate) fn from_snapshot(
        snapshot: catalog::contract::meta_mgr::CatalogSnapshot,
    ) -> RS<Self> {
        let schemas = snapshot.schemas;
        let builtins = [fs_object_schema(), queue_consumer_schema()];
        for schema in &schemas {
            if builtins.iter().any(|b| b.id() == schema.id())
                || (schema.schema() == DEFAULT_SCHEMA
                    && [FS_OBJECT_TABLE_NAME, QUEUE_CONSUMER_TABLE_NAME]
                        .contains(&schema.class_name().as_str()))
            {
                return Err(mudu_error!(
                    ER::CorruptedData,
                    "snapshot schema conflicts with a built-in system table"
                ));
            }
        }
        let tables = schemas
            .into_iter()
            .map(|schema| {
                let table = TableInfo::new(schema.clone())?;
                Ok((schema, table))
            })
            .collect::<RS<Vec<_>>>()?;
        let image = Self {
            tables,
            rules: snapshot.rules,
            bindings: snapshot.bindings,
            placements: snapshot.placements,
            fs_types: snapshot.fs_types,
        };
        image.validate_references()?;
        Ok(image)
    }

    fn validate_references(&self) -> RS<()> {
        let tables: std::collections::BTreeMap<_, _> = self
            .tables
            .iter()
            .map(|(schema, _)| (schema.id(), schema))
            .collect();
        let rules: std::collections::BTreeMap<_, _> =
            self.rules.iter().map(|rule| (rule.oid, rule)).collect();
        let fs_types: std::collections::BTreeMap<_, _> = self
            .fs_types
            .iter()
            .map(|desc| (desc.fs_id(), desc))
            .collect();
        for binding in &self.bindings {
            let rule = rules.get(&binding.rule_id).ok_or_else(|| {
                mudu_error!(
                    ER::InvalidState,
                    "catalog binding references a missing rule"
                )
            })?;
            if let Some(schema) = tables.get(&binding.table_id) {
                binding.validate(schema, rule)?;
            } else {
                // Schema-first deletion may have committed before binding
                // cleanup. Preserve that orphan for authoritative drop replay.
                binding.validate_rule(rule)?;
            }
        }
        for (schema, _) in &self.tables {
            super::references::validate_fs_references(schema, |id| {
                fs_types.get(&id).map(|desc| (*desc).clone())
            })?;
        }
        Ok(())
    }

    // No fallible decoding, file access or await remains after preparation.
    // The caller holds the DDL lock and marks catalog initialized only after
    // this synchronous publication finishes. Callers must await initialize.
    pub(super) fn publish(self, mgr: &MetaMgrImpl) {
        for (schema, table) in self.tables {
            mgr.publish_table_local(&schema, table);
        }
        for rule in self.rules {
            mgr.apply_create_partition_rule_local(&rule);
        }
        for binding in self.bindings {
            mgr.apply_bind_table_partition_local(&binding);
        }
        for placement in self.placements {
            mgr.apply_partition_placement_local(&placement);
        }
        for fs_type in self.fs_types {
            mgr.apply_create_fs_type_local(&fs_type);
        }
    }
}
