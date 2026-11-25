//! Reference checks shared by live DDL and catalog recovery.
use super::*;
use catalog::contract::schema_class::SchemaClass;

pub(super) fn validate_fs_references(
    schema: &SchemaClass,
    lookup: impl Fn(u64) -> Option<FsTypeDesc>,
) -> RS<()> {
    for column in schema.columns() {
        if let Some(binding) = column.fs_binding() {
            if !lookup(binding.fs_id()).is_some_and(|desc| {
                desc.fs_id() == binding.fs_id() && desc.kind() == binding.kind()
            }) {
                return Err(mudu_error!(
                    ER::InvalidArgument,
                    "column filesystem binding refers to a missing or incompatible type"
                ));
            }
        }
    }
    Ok(())
}

impl MetaMgrImpl {
    pub(super) fn validate_rule_bindings(&self, rule: &PartitionRuleDesc) -> RS<()> {
        let mut result = Ok(());
        self.binding_by_table_id.iter_sync(|_, binding| {
            if binding.rule_id != rule.oid {
                return true;
            }
            result = self.validate_binding_for_rule(binding, rule);
            result.is_ok()
        });
        result
    }

    fn validate_binding_for_rule(
        &self,
        binding: &TablePartitionBinding,
        rule: &PartitionRuleDesc,
    ) -> RS<()> {
        match self.lookup_table_info_by_id(binding.table_id) {
            Some(table) => binding.validate(table.schema()?.as_ref(), rule),
            None => binding.validate_rule(rule),
        }
    }
}
