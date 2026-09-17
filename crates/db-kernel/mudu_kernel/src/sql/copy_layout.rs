use crate::contract::table_desc::TableDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use std::collections::HashMap;

pub(crate) struct CopyLayout {
    key_index: Vec<usize>,
    value_index: Vec<usize>,
}

impl CopyLayout {
    pub(crate) fn new(table_desc: &TableDesc, columns: &[String]) -> RS<Self> {
        let columns = if columns.is_empty() {
            Self::ordered_columns(table_desc)
        } else if columns.len() == table_desc.oid2col().len() {
            columns.to_vec()
        } else {
            return Err(mudu_error!(
                ER::InvalidArgument,
                format!(
                    "the columns of table {} is not equal to the size specified {}",
                    table_desc.name(),
                    columns.len()
                )
            ));
        };

        let mut name_to_position = HashMap::new();
        for (index, name) in columns.iter().enumerate() {
            name_to_position.insert(name.clone(), index);
        }

        let mut key_index = vec![];
        let mut value_index = vec![];
        for (target, oids) in [
            (&mut key_index, table_desc.key_field_oid()),
            (&mut value_index, table_desc.value_field_oid()),
        ] {
            for oid in oids {
                let info = table_desc.oid2col().get(oid).ok_or_else(|| {
                    mudu_error!(
                        ER::EntityNotFound,
                        format!("cannot find column oid {}", oid)
                    )
                })?;
                let position = name_to_position.get(info.name()).ok_or_else(|| {
                    mudu_error!(
                        ER::EntityNotFound,
                        format!("cannot find column name {}", info.name())
                    )
                })?;
                target.push(*position);
            }
        }

        Ok(Self {
            key_index,
            value_index,
        })
    }

    pub(crate) fn key_index(&self) -> &[usize] {
        &self.key_index
    }

    pub(crate) fn value_index(&self) -> &[usize] {
        &self.value_index
    }

    fn ordered_columns(table_desc: &TableDesc) -> Vec<String> {
        let mut columns: Vec<_> = table_desc
            .oid2col()
            .values()
            .map(|field| (field.column_index(), field.name().clone()))
            .collect();
        columns.sort_by_key(|left| left.0);
        columns.into_iter().map(|(_, name)| name).collect()
    }
}
