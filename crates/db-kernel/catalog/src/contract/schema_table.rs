use crate::contract::app_schema::DEFAULT_SCHEMA;
use crate::contract::field_info::FieldInfo;
use crate::contract::schema_column::SchemaColumn;
#[cfg(any(test, feature = "test", fuzzing))]
use arbitrary::{Arbitrary, Unstructured};
use mudu::common::id::{AttrIndex, DatumIndex, OID};
use mudu::common::result::RS;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc as TupleDesc;
use mudu_utils::oid::gen_oid;
use serde::{Deserialize, Serialize};
#[cfg(any(test, feature = "test", fuzzing))]
use test_utils::_arb_limit;

fn default_schema_on_decode() -> String {
    DEFAULT_SCHEMA.to_string()
}

/// Schema of a table: the columns plus the key/value layout.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SchemaTable {
    oid: OID,
    table_name: String,
    columns: Vec<SchemaColumn>,
    key_indices: Vec<AttrIndex>,
    value_indices: Vec<AttrIndex>,
    // Schema (namespace) the table belongs to. Last field on purpose: the
    // catalog persists this struct positionally, so catalogs written before
    // the field existed decode through the serde default into the default
    // schema (zero migration).
    #[serde(default = "default_schema_on_decode")]
    schema: String,
}

/// Builds a tuple descriptor from a key/value column slice.
///
/// The input [`AttrIndex`] is the original column order in the table schema,
/// while the generated [`FieldInfo`] datum index is the position inside this
/// tuple.
pub fn schema_columns_to_tuple_desc(
    fields: Vec<(AttrIndex, &SchemaColumn)>,
) -> RS<(TupleDesc, Vec<FieldInfo>)> {
    let field_count = fields.len();
    let mut desc = Vec::with_capacity(field_count);
    for (column_index, sc) in fields.into_iter() {
        let ty = sc.type_param().to_data_type()?;
        let mut field_info = FieldInfo::new(
            sc.get_name().clone(),
            sc.get_oid(),
            ty.clone(),
            DatumIndex::MAX, // set an invalid index
            column_index,
            sc.primary_index(),
            sc.nullable(),
        );
        field_info.set_fs_binding(sc.fs_binding());
        desc.push((ty, field_info, sc.nullable()))
    }

    assert_eq!(desc.len(), field_count);
    let (vec_tuple_desc, mut vec_payload) = TupleDesc::normalized_type_desc_vec(
        desc.into_iter()
            .scan(0usize, |nullable_count, (ty, field_info, nullable)| {
                let null_bit_idx = if nullable {
                    let idx = *nullable_count as u16;
                    *nullable_count += 1;
                    Some(idx)
                } else {
                    None
                };
                Some((ty, (field_info, nullable, null_bit_idx)))
            })
            .collect(),
    )?;
    for (i, f) in vec_payload.iter_mut().enumerate() {
        // set its real index
        f.0.set_datum_index(i);
    }
    let typed_fields = vec_tuple_desc
        .into_iter()
        .zip(vec_payload.iter())
        .map(|(ty, (_, nullable, null_bit_idx))| (ty, *nullable, *null_bit_idx))
        .collect();
    let tuple_desc = TupleDesc::from_typed_fields(typed_fields, 1)?;
    Ok((
        tuple_desc,
        vec_payload
            .into_iter()
            .map(|(field_info, _, _)| field_info)
            .collect(),
    ))
}

#[cfg(any(test, feature = "test", fuzzing))]
impl<'a> Arbitrary<'a> for SchemaTable {
    fn arbitrary(u: &mut Unstructured<'a>) -> arbitrary::Result<Self> {
        let name = String::arbitrary(u)?;
        let v1 = u32::arbitrary(u)?;
        let v2 = u32::arbitrary(u)?;
        let n1 = v1 % _arb_limit::_ARB_MAX_TUPLE_KEY_FIELD as u32 + 1;
        let n2 = v2 % _arb_limit::_ARB_MAX_TUPLE_VALUE_FIELD as u32 + 1;
        let mut columns = vec![];
        let mut key_indices = vec![];
        let mut value_indices = vec![];
        for _i in 0..n1 {
            let s = SchemaColumn::arbitrary(u)?;
            key_indices.push(columns.len());
            columns.push(s);
        }
        for _i in 0..n2 {
            let s = SchemaColumn::arbitrary(u)?;
            value_indices.push(columns.len());
            columns.push(s);
        }
        let schema = SchemaTable::new(name, columns, key_indices, value_indices);
        Ok(schema)
    }
}

impl SchemaTable {
    /// Creates a table schema with a fresh oid.
    ///
    /// `columns` shall preserve the original column order of the table schema.
    /// `key_indices` / `value_indices` shall reference entries in `columns` via AttrIndex.
    /// This constructor shall be used only for new schema creation.
    /// During recovery, the schema shall be loaded from storage and deserialized.
    /// For any given SchemaTable value, TableInfo::new(...).table_desc() deterministically
    /// yields an identical field mapping and identical index semantics.
    /// Each SchemaColumn.index shall be normalized to its position within the
    /// corresponding key or value tuple.
    pub fn new(
        table_name: String,
        columns: Vec<SchemaColumn>,
        key_indices: Vec<AttrIndex>,
        value_indices: Vec<AttrIndex>,
    ) -> Self {
        Self::new_with_oid(gen_oid(), table_name, columns, key_indices, value_indices)
    }

    /// Creates a table schema with the given oid; invariants as in
    /// [`SchemaTable::new`].
    pub fn new_with_oid(
        oid: OID,
        table_name: String,
        columns: Vec<SchemaColumn>,
        key_indices: Vec<AttrIndex>,
        value_indices: Vec<AttrIndex>,
    ) -> Self {
        let mut s = SchemaTable {
            oid,
            table_name,
            columns,
            key_indices,
            value_indices,
            schema: DEFAULT_SCHEMA.to_string(),
        };
        for (i, index) in s.key_indices.iter().copied().enumerate() {
            let sc = &mut s.columns[index];
            sc.set_primary_index(Some(i as AttrIndex));
            sc.set_index(i as AttrIndex);
        }
        for (i, index) in s.value_indices.iter().copied().enumerate() {
            let sc = &mut s.columns[index];
            sc.set_primary_index(None);
            sc.set_index(i as AttrIndex);
        }
        s
    }

    /// The table oid.
    pub fn id(&self) -> OID {
        self.oid
    }

    /// The bare table name.
    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    /// Return the schema (namespace) this table belongs to.
    pub fn schema(&self) -> &String {
        &self.schema
    }

    /// Set the schema (namespace) this table belongs to.
    pub fn set_schema(&mut self, schema: String) {
        self.schema = schema;
    }

    /// All columns in original table order.
    pub fn columns(&self) -> &Vec<SchemaColumn> {
        &self.columns
    }

    /// The column at `index` in original table order.
    pub fn column_by_index(&self, index: AttrIndex) -> &SchemaColumn {
        &self.columns[index]
    }

    /// Original column indices of the key columns.
    pub fn key_indices(&self) -> &Vec<AttrIndex> {
        &self.key_indices
    }

    /// Original column indices of the value columns.
    pub fn value_indices(&self) -> &Vec<AttrIndex> {
        &self.value_indices
    }

    /// The key columns in key order.
    pub fn key_columns(&self) -> Vec<&SchemaColumn> {
        self.key_indices
            .iter()
            .map(|index| &self.columns[*index])
            .collect()
    }

    /// The value columns in value order.
    pub fn value_columns(&self) -> Vec<&SchemaColumn> {
        self.value_indices
            .iter()
            .map(|index| &self.columns[*index])
            .collect()
    }

    /// Tuple descriptor and field mapping of the key tuple.
    pub fn key_tuple_desc(&self) -> RS<(TupleDesc, Vec<FieldInfo>)> {
        schema_columns_to_tuple_desc(
            self.key_indices
                .iter()
                .map(|index| (*index, &self.columns[*index]))
                .collect(),
        )
    }

    /// Tuple descriptor and field mapping of the value tuple.
    pub fn value_tuple_desc(&self) -> RS<(TupleDesc, Vec<FieldInfo>)> {
        schema_columns_to_tuple_desc(
            self.value_indices
                .iter()
                .map(|index| (*index, &self.columns[*index]))
                .collect(),
        )
    }
}
