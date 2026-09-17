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
    pub const TABLE_NAME: &str = "history";

    /// Column name constants.
    pub mod columns {

        pub const H_ID: &str = "h_id";

        pub const H_C_ID: &str = "h_c_id";

        pub const H_C_D_ID: &str = "h_c_d_id";

        pub const H_C_W_ID: &str = "h_c_w_id";

        pub const H_D_ID: &str = "h_d_id";

        pub const H_W_ID: &str = "h_w_id";

        pub const H_AMOUNT: &str = "h_amount";

        pub const H_DATA: &str = "h_data";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct History {
        pub h_id: String,

        pub h_c_id: i32,

        pub h_c_d_id: i32,

        pub h_c_w_id: i32,

        pub h_d_id: i32,

        pub h_w_id: i32,

        pub h_amount: mududb::mudu::data_type::numeric::Numeric,

        pub h_data: String,
    }

    impl TupleDatumMarker for History {}

    impl SQLParamMarker for History {}

    impl History {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT h_id, h_c_id, h_c_d_id, h_c_w_id, h_d_id, h_w_id, h_amount, h_data FROM history WHERE h_id = ?";

        /// Insert one row; bind [`History::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO history (h_id, h_c_id, h_c_d_id, h_c_w_id, h_d_id, h_w_id, h_amount, h_data) VALUES (?, ?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM history WHERE h_id = ?";

        pub fn new(
            h_id: String,

            h_c_id: i32,

            h_c_d_id: i32,

            h_c_w_id: i32,

            h_d_id: i32,

            h_w_id: i32,

            h_amount: mududb::mudu::data_type::numeric::Numeric,

            h_data: String,
        ) -> Self {
            Self {
                h_id,

                h_c_id,

                h_c_d_id,

                h_c_w_id,

                h_d_id,

                h_w_id,

                h_amount,

                h_data,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (
            String,
            i32,
            i32,
            i32,
            i32,
            i32,
            mududb::mudu::data_type::numeric::Numeric,
            String,
        ) {
            (
                self.h_id.clone(),
                self.h_c_id,
                self.h_c_d_id,
                self.h_c_w_id,
                self.h_d_id,
                self.h_w_id,
                self.h_amount.clone(),
                self.h_data.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct HistoryChange {
        pub h_c_id: FieldChange<i32>,

        pub h_c_d_id: FieldChange<i32>,

        pub h_c_w_id: FieldChange<i32>,

        pub h_d_id: FieldChange<i32>,

        pub h_w_id: FieldChange<i32>,

        pub h_amount: FieldChange<mududb::mudu::data_type::numeric::Numeric>,

        pub h_data: FieldChange<String>,
    }

    impl HistoryChange {
        pub fn is_empty(&self) -> bool {
            self.h_c_id.is_unchanged()
                && self.h_c_d_id.is_unchanged()
                && self.h_c_w_id.is_unchanged()
                && self.h_d_id.is_unchanged()
                && self.h_w_id.is_unchanged()
                && self.h_amount.is_unchanged()
                && self.h_data.is_unchanged()
        }

        /// Builds the `UPDATE history SET ... WHERE h_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, h_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.h_c_id {
                sets.push([columns::H_C_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.h_c_d_id {
                sets.push([columns::H_C_D_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.h_c_w_id {
                sets.push([columns::H_C_W_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.h_d_id {
                sets.push([columns::H_D_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.h_w_id {
                sets.push([columns::H_W_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.h_amount {
                sets.push([columns::H_AMOUNT, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.h_data {
                sets.push([columns::H_DATA, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(h_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "h_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for History {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<History>)
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

    impl DatumDyn for History {
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

    impl Entity for History {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![

                DatumDesc::new_nullable(
                    columns::H_ID.to_string(),
                    <String as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_C_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_C_D_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_C_W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_D_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_AMOUNT.to_string(),
                    DataType::from_id_param(
            TypeFamily::Numeric,
            Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(
                mududb::types::data_type_param_numeric::DataTypeParamNumeric::new(6, 2),
            ))),
        ),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::H_DATA.to_string(),
                    <String as Datum>::data_type(),
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
            entity::expect_field_count(fields.len(), 8)?;
            Ok(Self {
                h_id: entity::field_from_tuple_binary(TABLE_NAME, columns::H_ID, &fields[0])?,

                h_c_id: entity::field_from_tuple_binary(TABLE_NAME, columns::H_C_ID, &fields[1])?,

                h_c_d_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::H_C_D_ID,
                    &fields[2],
                )?,

                h_c_w_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::H_C_W_ID,
                    &fields[3],
                )?,

                h_d_id: entity::field_from_tuple_binary(TABLE_NAME, columns::H_D_ID, &fields[4])?,

                h_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::H_W_ID, &fields[5])?,

                h_amount: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::H_AMOUNT,
                    &fields[6],
                )?,

                h_data: entity::field_from_tuple_binary(TABLE_NAME, columns::H_DATA, &fields[7])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 8)?;
            Ok(Self {
                h_id: entity::field_from_tuple_value(TABLE_NAME, columns::H_ID, &values[0])?,

                h_c_id: entity::field_from_tuple_value(TABLE_NAME, columns::H_C_ID, &values[1])?,

                h_c_d_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::H_C_D_ID,
                    &values[2],
                )?,

                h_c_w_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::H_C_W_ID,
                    &values[3],
                )?,

                h_d_id: entity::field_from_tuple_value(TABLE_NAME, columns::H_D_ID, &values[4])?,

                h_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::H_W_ID, &values[5])?,

                h_amount: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::H_AMOUNT,
                    &values[6],
                )?,

                h_data: entity::field_from_tuple_value(TABLE_NAME, columns::H_DATA, &values[7])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.h_id)?,
                entity::field_to_tuple_binary(&self.h_c_id)?,
                entity::field_to_tuple_binary(&self.h_c_d_id)?,
                entity::field_to_tuple_binary(&self.h_c_w_id)?,
                entity::field_to_tuple_binary(&self.h_d_id)?,
                entity::field_to_tuple_binary(&self.h_w_id)?,
                entity::field_to_tuple_binary(&self.h_amount)?,
                entity::field_to_tuple_binary(&self.h_data)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.h_id)?,
                entity::field_to_tuple_value(&self.h_c_id)?,
                entity::field_to_tuple_value(&self.h_c_d_id)?,
                entity::field_to_tuple_value(&self.h_c_w_id)?,
                entity::field_to_tuple_value(&self.h_d_id)?,
                entity::field_to_tuple_value(&self.h_w_id)?,
                entity::field_to_tuple_value(&self.h_amount)?,
                entity::field_to_tuple_value(&self.h_data)?,
            ]))
        }
    }
}
