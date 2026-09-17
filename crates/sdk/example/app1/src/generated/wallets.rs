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
    pub const TABLE_NAME: &str = "wallets";

    /// Column name constants.
    pub mod columns {

        pub const USER_ID: &str = "user_id";

        pub const BALANCE: &str = "balance";

        pub const UPDATED_AT: &str = "updated_at";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Wallets {
        pub user_id: i32,

        pub balance: Option<i32>,

        pub updated_at: Option<i32>,
    }

    impl TupleDatumMarker for Wallets {}

    impl SQLParamMarker for Wallets {}

    impl Wallets {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str =
            "SELECT user_id, balance, updated_at FROM wallets WHERE user_id = ?";

        /// Insert one row; bind [`Wallets::insert_params`].
        pub const SQL_INSERT: &'static str =
            "INSERT INTO wallets (user_id, balance, updated_at) VALUES (?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM wallets WHERE user_id = ?";

        pub fn new(user_id: i32, balance: Option<i32>, updated_at: Option<i32>) -> Self {
            Self {
                user_id,

                balance,

                updated_at,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (i32, Option<i32>, Option<i32>) {
            (self.user_id, self.balance, self.updated_at)
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct WalletsChange {
        pub balance: FieldChange<Option<i32>>,

        pub updated_at: FieldChange<Option<i32>>,
    }

    impl WalletsChange {
        pub fn is_empty(&self) -> bool {
            self.balance.is_unchanged() && self.updated_at.is_unchanged()
        }

        /// Builds the `UPDATE wallets SET ... WHERE user_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, user_id: i32) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.balance {
                sets.push([columns::BALANCE, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.updated_at {
                sets.push([columns::UPDATED_AT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(user_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "user_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Wallets {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Wallets>)
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

    impl DatumDyn for Wallets {
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

    impl Entity for Wallets {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::USER_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::BALANCE.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::UPDATED_AT.to_string(),
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
            entity::expect_field_count(fields.len(), 3)?;
            Ok(Self {
                user_id: entity::field_from_tuple_binary(TABLE_NAME, columns::USER_ID, &fields[0])?,

                balance: entity::opt_field_from_tuple_binary(&fields[1])?,

                updated_at: entity::opt_field_from_tuple_binary(&fields[2])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 3)?;
            Ok(Self {
                user_id: entity::field_from_tuple_value(TABLE_NAME, columns::USER_ID, &values[0])?,

                balance: entity::opt_field_from_tuple_value(&values[1])?,

                updated_at: entity::opt_field_from_tuple_value(&values[2])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.user_id)?,
                entity::opt_field_to_tuple_binary(&self.balance)?,
                entity::opt_field_to_tuple_binary(&self.updated_at)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.user_id)?,
                entity::opt_field_to_tuple_value(&self.balance)?,
                entity::opt_field_to_tuple_value(&self.updated_at)?,
            ]))
        }
    }
}
