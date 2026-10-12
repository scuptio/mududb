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
    pub const TABLE_NAME: &str = "transactions";

    /// Column name constants.
    pub mod columns {

        pub const TRANS_ID: &str = "trans_id";

        pub const TRANS_TYPE: &str = "trans_type";

        pub const FROM_USER: &str = "from_user";

        pub const TO_USER: &str = "to_user";

        pub const AMOUNT: &str = "amount";

        pub const CREATED_AT: &str = "created_at";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Transactions {
        pub trans_id: String,

        pub trans_type: Option<String>,

        pub from_user: Option<i32>,

        pub to_user: Option<i32>,

        pub amount: Option<i32>,

        pub created_at: Option<i32>,
    }

    impl TupleDatumMarker for Transactions {}

    impl SQLParamMarker for Transactions {}

    impl Transactions {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT trans_id, trans_type, from_user, to_user, amount, created_at FROM transactions WHERE trans_id = ?";

        /// Insert one row; bind [`Transactions::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO transactions (trans_id, trans_type, from_user, to_user, amount, created_at) VALUES (?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM transactions WHERE trans_id = ?";

        pub fn new(
            trans_id: String,

            trans_type: Option<String>,

            from_user: Option<i32>,

            to_user: Option<i32>,

            amount: Option<i32>,

            created_at: Option<i32>,
        ) -> Self {
            Self {
                trans_id,

                trans_type,

                from_user,

                to_user,

                amount,

                created_at,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (
            String,
            Option<String>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
            Option<i32>,
        ) {
            (
                self.trans_id.clone(),
                self.trans_type.clone(),
                self.from_user,
                self.to_user,
                self.amount,
                self.created_at,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct TransactionsChange {
        pub trans_type: FieldChange<Option<String>>,

        pub from_user: FieldChange<Option<i32>>,

        pub to_user: FieldChange<Option<i32>>,

        pub amount: FieldChange<Option<i32>>,

        pub created_at: FieldChange<Option<i32>>,
    }

    impl TransactionsChange {
        pub fn is_empty(&self) -> bool {
            self.trans_type.is_unchanged()
                && self.from_user.is_unchanged()
                && self.to_user.is_unchanged()
                && self.amount.is_unchanged()
                && self.created_at.is_unchanged()
        }

        /// Builds the `UPDATE transactions SET ... WHERE trans_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, trans_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.trans_type {
                sets.push([columns::TRANS_TYPE, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.from_user {
                sets.push([columns::FROM_USER, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.to_user {
                sets.push([columns::TO_USER, " = ?"].concat());
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

            params.push(Box::new(trans_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "trans_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Transactions {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Transactions>)
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

    impl DatumDyn for Transactions {
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

    impl Entity for Transactions {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::TRANS_ID.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::TRANS_TYPE.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::FROM_USER.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::TO_USER.to_string(),
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
            entity::expect_field_count(fields.len(), 6)?;
            Ok(Self {
                trans_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::TRANS_ID,
                    &fields[0],
                )?,

                trans_type: entity::opt_field_from_tuple_binary(&fields[1])?,

                from_user: entity::opt_field_from_tuple_binary(&fields[2])?,

                to_user: entity::opt_field_from_tuple_binary(&fields[3])?,

                amount: entity::opt_field_from_tuple_binary(&fields[4])?,

                created_at: entity::opt_field_from_tuple_binary(&fields[5])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 6)?;
            Ok(Self {
                trans_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::TRANS_ID,
                    &values[0],
                )?,

                trans_type: entity::opt_field_from_tuple_value(&values[1])?,

                from_user: entity::opt_field_from_tuple_value(&values[2])?,

                to_user: entity::opt_field_from_tuple_value(&values[3])?,

                amount: entity::opt_field_from_tuple_value(&values[4])?,

                created_at: entity::opt_field_from_tuple_value(&values[5])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.trans_id)?,
                entity::opt_field_to_tuple_binary(&self.trans_type)?,
                entity::opt_field_to_tuple_binary(&self.from_user)?,
                entity::opt_field_to_tuple_binary(&self.to_user)?,
                entity::opt_field_to_tuple_binary(&self.amount)?,
                entity::opt_field_to_tuple_binary(&self.created_at)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.trans_id)?,
                entity::opt_field_to_tuple_value(&self.trans_type)?,
                entity::opt_field_to_tuple_value(&self.from_user)?,
                entity::opt_field_to_tuple_value(&self.to_user)?,
                entity::opt_field_to_tuple_value(&self.amount)?,
                entity::opt_field_to_tuple_value(&self.created_at)?,
            ]))
        }
    }
}
