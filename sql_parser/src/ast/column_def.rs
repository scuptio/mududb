//! Column definition AST node.

use mudu::common::id::AttrIndex;
use mudu_binding::universal::uni_dat_type::UniDatType;
use mudu_binding::universal::uni_dat_value::UniDatValue;

/// Column definition inside a `CREATE TABLE` statement.
#[derive(Clone, Debug)]
pub struct ColumnDef {
    column_name: String,
    data_type_def: UniDatType,
    data_type_param: Option<Vec<UniDatValue>>,
    opt_primary_key_index: Option<AttrIndex>,
    nullable: bool,
    index: AttrIndex,
}

impl ColumnDef {
    /// Create a new column definition.
    pub fn new(
        column_name: String,
        data_type_def: UniDatType,
        data_type_param: Option<Vec<UniDatValue>>,
    ) -> Self {
        Self {
            column_name,
            data_type_def,
            data_type_param,
            opt_primary_key_index: None,
            nullable: true,
            index: AttrIndex::MAX,
        }
    }

    /// Return the column data type.
    pub fn data_type(&self) -> &UniDatType {
        &self.data_type_def
    }

    /// Return optional data type parameters (e.g., precision/scale for `NUMERIC`).
    pub fn data_type_param(&self) -> &Option<Vec<UniDatValue>> {
        &self.data_type_param
    }

    /// Return `true` if this column is part of the primary key.
    pub fn is_primary_key(&self) -> bool {
        self.opt_primary_key_index.is_some()
    }

    /// Return the column name.
    pub fn column_name(&self) -> &String {
        &self.column_name
    }

    /// Return the primary key index, if this column is a primary key column.
    pub fn primary_key_index(&self) -> Option<AttrIndex> {
        self.opt_primary_key_index
    }

    /// Return the primary key index or [`AttrIndex::MAX`] if not a primary key column.
    pub fn expect_primary_key_index(&self) -> AttrIndex {
        self.opt_primary_key_index.unwrap_or(AttrIndex::MAX)
    }

    /// Mark this column as a primary key column with the given key index.
    pub fn set_primary_key_index(&mut self, index: Option<AttrIndex>) {
        self.opt_primary_key_index = index;
        if index.is_some() {
            self.nullable = false;
        }
    }

    /// Return whether the column is nullable.
    pub fn nullable(&self) -> bool {
        self.nullable
    }

    /// Set whether the column is nullable.
    pub fn set_nullable(&mut self, nullable: bool) {
        self.nullable = nullable;
    }

    /// Set the table-level column index.
    pub fn set_index(&mut self, index: AttrIndex) {
        self.index = index;
    }

    /// Return the table-level column index.
    pub fn column_index(&self) -> AttrIndex {
        self.index
    }
}
