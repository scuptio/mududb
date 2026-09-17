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
    pub const TABLE_NAME: &str = "district";

    /// Column name constants.
    pub mod columns {

        pub const D_ID: &str = "d_id";

        pub const D_W_ID: &str = "d_w_id";

        pub const D_NAME: &str = "d_name";

        pub const D_TAX: &str = "d_tax";

        pub const D_YTD: &str = "d_ytd";

        pub const D_NEXT_O_ID: &str = "d_next_o_id";

        pub const D_LAST_DELIVERY_O_ID: &str = "d_last_delivery_o_id";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct District {
        pub d_id: i32,

        pub d_w_id: i32,

        pub d_name: String,

        pub d_tax: i32,

        pub d_ytd: mududb::mudu::data_type::numeric::Numeric,

        pub d_next_o_id: i32,

        pub d_last_delivery_o_id: i32,
    }

    impl TupleDatumMarker for District {}

    impl SQLParamMarker for District {}

    impl District {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id FROM district WHERE d_id = ? AND d_w_id = ?";

        /// Insert one row; bind [`District::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO district (d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id) VALUES (?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str =
            "DELETE FROM district WHERE d_id = ? AND d_w_id = ?";

        pub fn new(
            d_id: i32,

            d_w_id: i32,

            d_name: String,

            d_tax: i32,

            d_ytd: mududb::mudu::data_type::numeric::Numeric,

            d_next_o_id: i32,

            d_last_delivery_o_id: i32,
        ) -> Self {
            Self {
                d_id,

                d_w_id,

                d_name,

                d_tax,

                d_ytd,

                d_next_o_id,

                d_last_delivery_o_id,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (
            i32,
            i32,
            String,
            i32,
            mududb::mudu::data_type::numeric::Numeric,
            i32,
            i32,
        ) {
            (
                self.d_id,
                self.d_w_id,
                self.d_name.clone(),
                self.d_tax,
                self.d_ytd.clone(),
                self.d_next_o_id,
                self.d_last_delivery_o_id,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct DistrictChange {
        pub d_name: FieldChange<String>,

        pub d_tax: FieldChange<i32>,

        pub d_ytd: FieldChange<mududb::mudu::data_type::numeric::Numeric>,

        pub d_next_o_id: FieldChange<i32>,

        pub d_last_delivery_o_id: FieldChange<i32>,
    }

    impl DistrictChange {
        pub fn is_empty(&self) -> bool {
            self.d_name.is_unchanged()
                && self.d_tax.is_unchanged()
                && self.d_ytd.is_unchanged()
                && self.d_next_o_id.is_unchanged()
                && self.d_last_delivery_o_id.is_unchanged()
        }

        /// Builds the `UPDATE district SET ... WHERE d_id = ? AND d_w_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(
            &self,

            d_id: i32,

            d_w_id: i32,
        ) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.d_name {
                sets.push([columns::D_NAME, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.d_tax {
                sets.push([columns::D_TAX, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.d_ytd {
                sets.push([columns::D_YTD, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.d_next_o_id {
                sets.push([columns::D_NEXT_O_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.d_last_delivery_o_id {
                sets.push([columns::D_LAST_DELIVERY_O_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(d_id));

            params.push(Box::new(d_w_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "d_id = ? AND d_w_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for District {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<District>)
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

    impl DatumDyn for District {
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

    impl Entity for District {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![

                DatumDesc::new_nullable(
                    columns::D_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::D_W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::D_NAME.to_string(),
                    <String as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::D_TAX.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::D_YTD.to_string(),
                    DataType::from_id_param(
            TypeFamily::Numeric,
            Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(
                mududb::types::data_type_param_numeric::DataTypeParamNumeric::new(12, 2),
            ))),
        ),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::D_NEXT_O_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::D_LAST_DELIVERY_O_ID.to_string(),
                    <i32 as Datum>::data_type(),
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
            entity::expect_field_count(fields.len(), 7)?;
            Ok(Self {
                d_id: entity::field_from_tuple_binary(TABLE_NAME, columns::D_ID, &fields[0])?,

                d_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::D_W_ID, &fields[1])?,

                d_name: entity::field_from_tuple_binary(TABLE_NAME, columns::D_NAME, &fields[2])?,

                d_tax: entity::field_from_tuple_binary(TABLE_NAME, columns::D_TAX, &fields[3])?,

                d_ytd: entity::field_from_tuple_binary(TABLE_NAME, columns::D_YTD, &fields[4])?,

                d_next_o_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::D_NEXT_O_ID,
                    &fields[5],
                )?,

                d_last_delivery_o_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::D_LAST_DELIVERY_O_ID,
                    &fields[6],
                )?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 7)?;
            Ok(Self {
                d_id: entity::field_from_tuple_value(TABLE_NAME, columns::D_ID, &values[0])?,

                d_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::D_W_ID, &values[1])?,

                d_name: entity::field_from_tuple_value(TABLE_NAME, columns::D_NAME, &values[2])?,

                d_tax: entity::field_from_tuple_value(TABLE_NAME, columns::D_TAX, &values[3])?,

                d_ytd: entity::field_from_tuple_value(TABLE_NAME, columns::D_YTD, &values[4])?,

                d_next_o_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::D_NEXT_O_ID,
                    &values[5],
                )?,

                d_last_delivery_o_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::D_LAST_DELIVERY_O_ID,
                    &values[6],
                )?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.d_id)?,
                entity::field_to_tuple_binary(&self.d_w_id)?,
                entity::field_to_tuple_binary(&self.d_name)?,
                entity::field_to_tuple_binary(&self.d_tax)?,
                entity::field_to_tuple_binary(&self.d_ytd)?,
                entity::field_to_tuple_binary(&self.d_next_o_id)?,
                entity::field_to_tuple_binary(&self.d_last_delivery_o_id)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.d_id)?,
                entity::field_to_tuple_value(&self.d_w_id)?,
                entity::field_to_tuple_value(&self.d_name)?,
                entity::field_to_tuple_value(&self.d_tax)?,
                entity::field_to_tuple_value(&self.d_ytd)?,
                entity::field_to_tuple_value(&self.d_next_o_id)?,
                entity::field_to_tuple_value(&self.d_last_delivery_o_id)?,
            ]))
        }
    }
}
