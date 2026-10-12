pub mod object {
    use mududb::common::result::RS;
    use mududb::contract::database::entity::{self, Entity};

    use mududb::contract::database::field_change::FieldChange;

    use mududb::contract::database::sql_params::SQLParamMarker;
    use mududb::contract::tuple::datum_desc::DatumDesc;
    use mududb::contract::tuple::tuple_datum::TupleDatumMarker;
    use mududb::contract::tuple::tuple_field::TupleField;
    use mududb::contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mududb::contract::tuple::tuple_value::TupleValue;
    use mududb::types::data_binary::DataBinary;
    use mududb::types::data_textual::DataTextual;
    use mududb::types::data_type::DataType;
    use mududb::types::data_value::DataValue;
    use mududb::types::datum::{Datum, DatumDyn};
    use mududb::types::type_family::TypeFamily;

    /// Table name.
    pub const TABLE_NAME: &str = "warehouse";

    /// Column name constants.
    pub mod columns {

        pub const W_ID: &str = "w_id";

        pub const W_NAME: &str = "w_name";

        pub const W_TAX: &str = "w_tax";

        pub const W_YTD: &str = "w_ytd";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Warehouse {
        pub w_id: i32,

        pub w_name: String,

        pub w_tax: i32,

        pub w_ytd: mududb::mudu::data_type::numeric::Numeric,
    }

    impl TupleDatumMarker for Warehouse {}

    impl SQLParamMarker for Warehouse {}

    impl Warehouse {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str =
            "SELECT w_id, w_name, w_tax, w_ytd FROM warehouse WHERE w_id = ?";

        /// Insert one row; bind [`Warehouse::insert_params`].
        pub const SQL_INSERT: &'static str =
            "INSERT INTO warehouse (w_id, w_name, w_tax, w_ytd) VALUES (?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM warehouse WHERE w_id = ?";

        pub fn new(
            w_id: i32,

            w_name: String,

            w_tax: i32,

            w_ytd: mududb::mudu::data_type::numeric::Numeric,
        ) -> Self {
            Self {
                w_id,

                w_name,

                w_tax,

                w_ytd,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (i32, String, i32, mududb::mudu::data_type::numeric::Numeric) {
            (
                self.w_id,
                self.w_name.clone(),
                self.w_tax,
                self.w_ytd.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct WarehouseChange {
        pub w_name: FieldChange<String>,

        pub w_tax: FieldChange<i32>,

        pub w_ytd: FieldChange<mududb::mudu::data_type::numeric::Numeric>,
    }

    impl WarehouseChange {
        pub fn is_empty(&self) -> bool {
            self.w_name.is_unchanged() && self.w_tax.is_unchanged() && self.w_ytd.is_unchanged()
        }

        /// Builds the `UPDATE warehouse SET ... WHERE w_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, w_id: i32) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.w_name {
                sets.push([columns::W_NAME, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.w_tax {
                sets.push([columns::W_TAX, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.w_ytd {
                sets.push([columns::W_YTD, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(w_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "w_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Warehouse {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Warehouse>)
                .clone()
        }

        fn from_binary(binary: &[u8]) -> RS<Self> {
            entity::entity_from_binary(binary)
        }

        fn from_value(value: &DataValue) -> RS<Self> {
            entity::entity_from_value(value)
        }

        fn from_textual(textual: &str) -> RS<Self> {
            entity::entity_from_textual(textual)
        }
    }

    impl DatumDyn for Warehouse {
        fn type_family(&self) -> RS<TypeFamily> {
            entity::entity_type_family()
        }

        fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
            entity::entity_to_binary(self, data_type)
        }

        fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
            entity::entity_to_textual(self, data_type)
        }

        fn to_value(&self, data_type: &DataType) -> RS<DataValue> {
            entity::entity_to_value(self, data_type)
        }

        fn clone_boxed(&self) -> Box<dyn DatumDyn> {
            entity::entity_clone_boxed(self)
        }
    }

    impl Entity for Warehouse {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![

                DatumDesc::new_nullable(
                    columns::W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::W_NAME.to_string(),
                    <String as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::W_TAX.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::W_YTD.to_string(),
                    DataType::from_id_param(
            TypeFamily::Numeric,
            Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(
                mududb::types::data_type_param_numeric::DataTypeParamNumeric::new(12, 2),
            ))),
        ),
                    false,
                ),

            ])
            })
        }

        fn table_name() -> &'static str {
            TABLE_NAME
        }

        fn from_tuple(row: &TupleField) -> RS<Self> {
            let fields = row.fields();
            entity::expect_field_count(fields.len(), 4)?;
            Ok(Self {
                w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::W_ID, &fields[0])?,

                w_name: entity::field_from_tuple_binary(TABLE_NAME, columns::W_NAME, &fields[1])?,

                w_tax: entity::field_from_tuple_binary(TABLE_NAME, columns::W_TAX, &fields[2])?,

                w_ytd: entity::field_from_tuple_binary(TABLE_NAME, columns::W_YTD, &fields[3])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 4)?;
            Ok(Self {
                w_id: entity::field_from_tuple_value(TABLE_NAME, columns::W_ID, &values[0])?,

                w_name: entity::field_from_tuple_value(TABLE_NAME, columns::W_NAME, &values[1])?,

                w_tax: entity::field_from_tuple_value(TABLE_NAME, columns::W_TAX, &values[2])?,

                w_ytd: entity::field_from_tuple_value(TABLE_NAME, columns::W_YTD, &values[3])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.w_id)?,
                entity::field_to_tuple_binary(&self.w_name)?,
                entity::field_to_tuple_binary(&self.w_tax)?,
                entity::field_to_tuple_binary(&self.w_ytd)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.w_id)?,
                entity::field_to_tuple_value(&self.w_name)?,
                entity::field_to_tuple_value(&self.w_tax)?,
                entity::field_to_tuple_value(&self.w_ytd)?,
            ]))
        }
    }
}
