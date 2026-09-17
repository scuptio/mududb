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
    pub const TABLE_NAME: &str = "orders";

    /// Column name constants.
    pub mod columns {

        pub const O_ID: &str = "o_id";

        pub const O_D_ID: &str = "o_d_id";

        pub const O_W_ID: &str = "o_w_id";

        pub const O_C_ID: &str = "o_c_id";

        pub const O_ENTRY_D: &str = "o_entry_d";

        pub const O_CARRIER_ID: &str = "o_carrier_id";

        pub const O_OL_CNT: &str = "o_ol_cnt";

        pub const O_ALL_LOCAL: &str = "o_all_local";

        pub const O_STATUS: &str = "o_status";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Orders {
        pub o_id: i32,

        pub o_d_id: i32,

        pub o_w_id: i32,

        pub o_c_id: i32,

        pub o_entry_d: String,

        pub o_carrier_id: Option<i32>,

        pub o_ol_cnt: i32,

        pub o_all_local: i32,

        pub o_status: String,
    }

    impl TupleDatumMarker for Orders {}

    impl SQLParamMarker for Orders {}

    impl Orders {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status FROM orders WHERE o_id = ? AND o_d_id = ? AND o_w_id = ?";

        /// Insert one row; bind [`Orders::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO orders (o_id, o_d_id, o_w_id, o_c_id, o_entry_d, o_carrier_id, o_ol_cnt, o_all_local, o_status) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str =
            "DELETE FROM orders WHERE o_id = ? AND o_d_id = ? AND o_w_id = ?";

        pub fn new(
            o_id: i32,

            o_d_id: i32,

            o_w_id: i32,

            o_c_id: i32,

            o_entry_d: String,

            o_carrier_id: Option<i32>,

            o_ol_cnt: i32,

            o_all_local: i32,

            o_status: String,
        ) -> Self {
            Self {
                o_id,

                o_d_id,

                o_w_id,

                o_c_id,

                o_entry_d,

                o_carrier_id,

                o_ol_cnt,

                o_all_local,

                o_status,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (i32, i32, i32, i32, String, Option<i32>, i32, i32, String) {
            (
                self.o_id,
                self.o_d_id,
                self.o_w_id,
                self.o_c_id,
                self.o_entry_d.clone(),
                self.o_carrier_id,
                self.o_ol_cnt,
                self.o_all_local,
                self.o_status.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct OrdersChange {
        pub o_c_id: FieldChange<i32>,

        pub o_entry_d: FieldChange<String>,

        pub o_carrier_id: FieldChange<Option<i32>>,

        pub o_ol_cnt: FieldChange<i32>,

        pub o_all_local: FieldChange<i32>,

        pub o_status: FieldChange<String>,
    }

    impl OrdersChange {
        pub fn is_empty(&self) -> bool {
            self.o_c_id.is_unchanged()
                && self.o_entry_d.is_unchanged()
                && self.o_carrier_id.is_unchanged()
                && self.o_ol_cnt.is_unchanged()
                && self.o_all_local.is_unchanged()
                && self.o_status.is_unchanged()
        }

        /// Builds the `UPDATE orders SET ... WHERE o_id = ? AND o_d_id = ? AND o_w_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(
            &self,

            o_id: i32,

            o_d_id: i32,

            o_w_id: i32,
        ) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.o_c_id {
                sets.push([columns::O_C_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.o_entry_d {
                sets.push([columns::O_ENTRY_D, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.o_carrier_id {
                sets.push([columns::O_CARRIER_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.o_ol_cnt {
                sets.push([columns::O_OL_CNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.o_all_local {
                sets.push([columns::O_ALL_LOCAL, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.o_status {
                sets.push([columns::O_STATUS, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(o_id));

            params.push(Box::new(o_d_id));

            params.push(Box::new(o_w_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "o_id = ? AND o_d_id = ? AND o_w_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Orders {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Orders>)
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

    impl DatumDyn for Orders {
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

    impl Entity for Orders {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::O_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_D_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_W_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_C_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_ENTRY_D.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_CARRIER_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_OL_CNT.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_ALL_LOCAL.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::O_STATUS.to_string(),
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
            entity::expect_field_count(fields.len(), 9)?;
            Ok(Self {
                o_id: entity::field_from_tuple_binary(TABLE_NAME, columns::O_ID, &fields[0])?,

                o_d_id: entity::field_from_tuple_binary(TABLE_NAME, columns::O_D_ID, &fields[1])?,

                o_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::O_W_ID, &fields[2])?,

                o_c_id: entity::field_from_tuple_binary(TABLE_NAME, columns::O_C_ID, &fields[3])?,

                o_entry_d: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::O_ENTRY_D,
                    &fields[4],
                )?,

                o_carrier_id: entity::opt_field_from_tuple_binary(&fields[5])?,

                o_ol_cnt: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::O_OL_CNT,
                    &fields[6],
                )?,

                o_all_local: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::O_ALL_LOCAL,
                    &fields[7],
                )?,

                o_status: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::O_STATUS,
                    &fields[8],
                )?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 9)?;
            Ok(Self {
                o_id: entity::field_from_tuple_value(TABLE_NAME, columns::O_ID, &values[0])?,

                o_d_id: entity::field_from_tuple_value(TABLE_NAME, columns::O_D_ID, &values[1])?,

                o_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::O_W_ID, &values[2])?,

                o_c_id: entity::field_from_tuple_value(TABLE_NAME, columns::O_C_ID, &values[3])?,

                o_entry_d: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::O_ENTRY_D,
                    &values[4],
                )?,

                o_carrier_id: entity::opt_field_from_tuple_value(&values[5])?,

                o_ol_cnt: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::O_OL_CNT,
                    &values[6],
                )?,

                o_all_local: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::O_ALL_LOCAL,
                    &values[7],
                )?,

                o_status: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::O_STATUS,
                    &values[8],
                )?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.o_id)?,
                entity::field_to_tuple_binary(&self.o_d_id)?,
                entity::field_to_tuple_binary(&self.o_w_id)?,
                entity::field_to_tuple_binary(&self.o_c_id)?,
                entity::field_to_tuple_binary(&self.o_entry_d)?,
                entity::opt_field_to_tuple_binary(&self.o_carrier_id)?,
                entity::field_to_tuple_binary(&self.o_ol_cnt)?,
                entity::field_to_tuple_binary(&self.o_all_local)?,
                entity::field_to_tuple_binary(&self.o_status)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.o_id)?,
                entity::field_to_tuple_value(&self.o_d_id)?,
                entity::field_to_tuple_value(&self.o_w_id)?,
                entity::field_to_tuple_value(&self.o_c_id)?,
                entity::field_to_tuple_value(&self.o_entry_d)?,
                entity::opt_field_to_tuple_value(&self.o_carrier_id)?,
                entity::field_to_tuple_value(&self.o_ol_cnt)?,
                entity::field_to_tuple_value(&self.o_all_local)?,
                entity::field_to_tuple_value(&self.o_status)?,
            ]))
        }
    }
}
