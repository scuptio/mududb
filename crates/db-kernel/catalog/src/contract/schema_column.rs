#[cfg(any(test, feature = "test", fuzzing))]
use arbitrary::{Arbitrary, Unstructured};
use mudu::common::id::{AttrIndex, OID};
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily as TypeID;
use mudu_utils::oid::gen_oid;
use serde::{Deserialize, Serialize};

use crate::contract::fs_type::FsColumnBinding;

/// Column descriptor of a table schema.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaColumn {
    oid: OID,
    name: String,
    type_id: TypeID,
    type_param: DataTypeInfo,
    index: AttrIndex,
    is_primary: Option<AttrIndex>,
    #[serde(default = "default_nullable")]
    nullable: bool,
    #[serde(default)]
    fs_binding: Option<FsColumnBinding>,
}

impl SchemaColumn {
    /// Creates a non-key nullable column with a fresh oid.
    pub fn new(name: String, data_type: TypeID, type_param: DataTypeInfo) -> Self {
        Self {
            oid: gen_oid(),
            name,
            type_id: data_type,
            type_param: type_param.clone(),

            index: 0,
            is_primary: None,
            nullable: true,
            fs_binding: None,
        }
    }

    /// Creates a non-key nullable column with the given oid.
    pub fn new_with_oid(
        oid: OID,
        name: String,
        data_type: TypeID,
        type_param: DataTypeInfo,
    ) -> Self {
        Self {
            oid,
            name,
            type_id: data_type,
            type_param: type_param.clone(),

            index: 0,
            is_primary: None,
            nullable: true,
            fs_binding: None,
        }
    }

    /// The column oid.
    pub fn get_oid(&self) -> OID {
        self.oid
    }

    /// The column name.
    pub fn get_name(&self) -> &String {
        &self.name
    }

    /// Whether the column belongs to the primary key.
    pub fn is_primary(&self) -> bool {
        self.is_primary.is_some()
    }

    /// Position within the primary key when the column is a key column.
    pub fn primary_index(&self) -> Option<AttrIndex> {
        self.is_primary
    }

    /// Marks the column as the `index`-th primary key column, or clears key
    /// membership when `None`; key columns are forced non-nullable.
    pub fn set_primary_index(&mut self, index: Option<AttrIndex>) {
        self.is_primary = index;
        if index.is_some() {
            self.nullable = false;
        }
    }

    /// Whether the column accepts NULL.
    pub fn nullable(&self) -> bool {
        self.nullable
    }

    /// Sets whether the column accepts NULL.
    pub fn set_nullable(&mut self, nullable: bool) {
        self.nullable = nullable;
    }

    /// Position of the column within its key or value tuple (normalized at
    /// schema construction).
    pub fn get_index(&self) -> AttrIndex {
        self.index
    }

    /// Sets the column position within its key or value tuple.
    pub fn set_index(&mut self, index: AttrIndex) {
        self.index = index;
    }

    /// The column's type family.
    pub fn type_id(&self) -> TypeID {
        self.type_id
    }

    /// Whether the column's type has a fixed encoded length.
    pub fn is_fixed_length(&self) -> bool {
        self.type_id().is_fixed_len()
    }

    /// The column's type parameters.
    pub fn type_param(&self) -> &DataTypeInfo {
        &self.type_param
    }

    /// The filesystem binding of the column, when stored in an external
    /// filesystem.
    pub fn fs_binding(&self) -> Option<FsColumnBinding> {
        self.fs_binding
    }

    /// Sets or clears the filesystem binding of the column.
    pub fn set_fs_binding(&mut self, fs_binding: Option<FsColumnBinding>) {
        self.fs_binding = fs_binding;
    }
}

fn default_nullable() -> bool {
    true
}

#[cfg(any(test, feature = "test", fuzzing))]
impl<'a> Arbitrary<'a> for SchemaColumn {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let name = String::arbitrary(u)?;
        let data_type = TypeID::arbitrary(u)?;
        let fn_arbitrary = data_type.fn_arb_param();
        let param = fn_arbitrary(u)?;
        let schema = Self::new(name, data_type, DataTypeInfo::from_opt_object(&param));
        Ok(schema)
    }
}
