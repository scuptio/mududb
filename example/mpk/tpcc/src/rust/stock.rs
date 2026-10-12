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
    pub const TABLE_NAME: &str = "stock";

    /// Column name constants.
    pub mod columns {

        pub const S_I_ID: &str = "s_i_id";

        pub const S_W_ID: &str = "s_w_id";

        pub const S_QUANTITY: &str = "s_quantity";

        pub const S_YTD: &str = "s_ytd";

        pub const S_ORDER_CNT: &str = "s_order_cnt";

        pub const S_REMOTE_CNT: &str = "s_remote_cnt";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Stock {
        pub s_i_id: i32,

        pub s_w_id: i32,

        pub s_quantity: i32,

        pub s_ytd: i32,

        pub s_order_cnt: i32,

        pub s_remote_cnt: i32,
    }

    impl TupleDatumMarker for Stock {}

    impl SQLParamMarker for Stock {}

    impl Stock {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt FROM stock WHERE s_i_id = ? AND s_w_id = ?";

        /// Insert one row; bind [`Stock::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO stock (s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt) VALUES (?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str =
            "DELETE FROM stock WHERE s_i_id = ? AND s_w_id = ?";

        pub fn new(
            s_i_id: i32,

            s_w_id: i32,

            s_quantity: i32,

            s_ytd: i32,

            s_order_cnt: i32,

            s_remote_cnt: i32,
        ) -> Self {
            Self {
                s_i_id,

                s_w_id,

                s_quantity,

                s_ytd,

                s_order_cnt,

                s_remote_cnt,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (i32, i32, i32, i32, i32, i32) {
            (
                self.s_i_id,
                self.s_w_id,
                self.s_quantity,
                self.s_ytd,
                self.s_order_cnt,
                self.s_remote_cnt,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct StockChange {
        pub s_quantity: FieldChange<i32>,

        pub s_ytd: FieldChange<i32>,

        pub s_order_cnt: FieldChange<i32>,

        pub s_remote_cnt: FieldChange<i32>,
    }

    impl StockChange {
        pub fn is_empty(&self) -> bool {
            self.s_quantity.is_unchanged()
                && self.s_ytd.is_unchanged()
                && self.s_order_cnt.is_unchanged()
                && self.s_remote_cnt.is_unchanged()
        }

        /// Builds the `UPDATE stock SET ... WHERE s_i_id = ? AND s_w_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(
            &self,

            s_i_id: i32,

            s_w_id: i32,
        ) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.s_quantity {
                sets.push([columns::S_QUANTITY, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.s_ytd {
                sets.push([columns::S_YTD, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.s_order_cnt {
                sets.push([columns::S_ORDER_CNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.s_remote_cnt {
                sets.push([columns::S_REMOTE_CNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(s_i_id));

            params.push(Box::new(s_w_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "s_i_id = ? AND s_w_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Stock {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Stock>)
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

    impl DatumDyn for Stock {
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

    impl Entity for Stock {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::S_I_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::S_W_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::S_QUANTITY.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::S_YTD.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::S_ORDER_CNT.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::S_REMOTE_CNT.to_string(),
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
            entity::expect_field_count(fields.len(), 6)?;
            Ok(Self {
                s_i_id: entity::field_from_tuple_binary(TABLE_NAME, columns::S_I_ID, &fields[0])?,

                s_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::S_W_ID, &fields[1])?,

                s_quantity: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::S_QUANTITY,
                    &fields[2],
                )?,

                s_ytd: entity::field_from_tuple_binary(TABLE_NAME, columns::S_YTD, &fields[3])?,

                s_order_cnt: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::S_ORDER_CNT,
                    &fields[4],
                )?,

                s_remote_cnt: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::S_REMOTE_CNT,
                    &fields[5],
                )?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 6)?;
            Ok(Self {
                s_i_id: entity::field_from_tuple_value(TABLE_NAME, columns::S_I_ID, &values[0])?,

                s_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::S_W_ID, &values[1])?,

                s_quantity: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::S_QUANTITY,
                    &values[2],
                )?,

                s_ytd: entity::field_from_tuple_value(TABLE_NAME, columns::S_YTD, &values[3])?,

                s_order_cnt: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::S_ORDER_CNT,
                    &values[4],
                )?,

                s_remote_cnt: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::S_REMOTE_CNT,
                    &values[5],
                )?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.s_i_id)?,
                entity::field_to_tuple_binary(&self.s_w_id)?,
                entity::field_to_tuple_binary(&self.s_quantity)?,
                entity::field_to_tuple_binary(&self.s_ytd)?,
                entity::field_to_tuple_binary(&self.s_order_cnt)?,
                entity::field_to_tuple_binary(&self.s_remote_cnt)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.s_i_id)?,
                entity::field_to_tuple_value(&self.s_w_id)?,
                entity::field_to_tuple_value(&self.s_quantity)?,
                entity::field_to_tuple_value(&self.s_ytd)?,
                entity::field_to_tuple_value(&self.s_order_cnt)?,
                entity::field_to_tuple_value(&self.s_remote_cnt)?,
            ]))
        }
    }
}
