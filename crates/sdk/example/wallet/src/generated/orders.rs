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

        pub const ORDER_ID: &str = "order_id";

        pub const USER_ID: &str = "user_id";

        pub const MERCH_ID: &str = "merch_id";

        pub const AMOUNT: &str = "amount";

        pub const CREATED_AT: &str = "created_at";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Orders {
        pub order_id: i32,

        pub user_id: Option<i32>,

        pub merch_id: Option<i32>,

        pub amount: Option<i32>,

        pub created_at: Option<i32>,
    }

    impl TupleDatumMarker for Orders {}

    impl SQLParamMarker for Orders {}

    impl Orders {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str =
            "SELECT order_id, user_id, merch_id, amount, created_at FROM orders WHERE order_id = ?";

        /// Insert one row; bind [`Orders::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO orders (order_id, user_id, merch_id, amount, created_at) VALUES (?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM orders WHERE order_id = ?";

        pub fn new(
            order_id: i32,

            user_id: Option<i32>,

            merch_id: Option<i32>,

            amount: Option<i32>,

            created_at: Option<i32>,
        ) -> Self {
            Self {
                order_id,

                user_id,

                merch_id,

                amount,

                created_at,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (i32, Option<i32>, Option<i32>, Option<i32>, Option<i32>) {
            (
                self.order_id,
                self.user_id,
                self.merch_id,
                self.amount,
                self.created_at,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct OrdersChange {
        pub user_id: FieldChange<Option<i32>>,

        pub merch_id: FieldChange<Option<i32>>,

        pub amount: FieldChange<Option<i32>>,

        pub created_at: FieldChange<Option<i32>>,
    }

    impl OrdersChange {
        pub fn is_empty(&self) -> bool {
            self.user_id.is_unchanged()
                && self.merch_id.is_unchanged()
                && self.amount.is_unchanged()
                && self.created_at.is_unchanged()
        }

        /// Builds the `UPDATE orders SET ... WHERE order_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, order_id: i32) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.user_id {
                sets.push([columns::USER_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.merch_id {
                sets.push([columns::MERCH_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.amount {
                sets.push([columns::AMOUNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.created_at {
                sets.push([columns::CREATED_AT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(order_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "order_id = ?",
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
                        columns::ORDER_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::USER_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::MERCH_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::AMOUNT.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::CREATED_AT.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                ])
            })
        }

        fn table_name() -> &'static str {
            TABLE_NAME
        }

        fn from_tuple(row: &TupleField) -> RS<Self> {
            let fields = row.fields();
            entity::expect_field_count(fields.len(), 5)?;
            Ok(Self {
                order_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::ORDER_ID,
                    &fields[0],
                )?,

                user_id: entity::opt_field_from_tuple_binary(&fields[1])?,

                merch_id: entity::opt_field_from_tuple_binary(&fields[2])?,

                amount: entity::opt_field_from_tuple_binary(&fields[3])?,

                created_at: entity::opt_field_from_tuple_binary(&fields[4])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 5)?;
            Ok(Self {
                order_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::ORDER_ID,
                    &values[0],
                )?,

                user_id: entity::opt_field_from_tuple_value(&values[1])?,

                merch_id: entity::opt_field_from_tuple_value(&values[2])?,

                amount: entity::opt_field_from_tuple_value(&values[3])?,

                created_at: entity::opt_field_from_tuple_value(&values[4])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.order_id)?,
                entity::opt_field_to_tuple_binary(&self.user_id)?,
                entity::opt_field_to_tuple_binary(&self.merch_id)?,
                entity::opt_field_to_tuple_binary(&self.amount)?,
                entity::opt_field_to_tuple_binary(&self.created_at)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.order_id)?,
                entity::opt_field_to_tuple_value(&self.user_id)?,
                entity::opt_field_to_tuple_value(&self.merch_id)?,
                entity::opt_field_to_tuple_value(&self.amount)?,
                entity::opt_field_to_tuple_value(&self.created_at)?,
            ]))
        }
    }
}
