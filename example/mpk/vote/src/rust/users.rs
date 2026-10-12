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
    pub const TABLE_NAME: &str = "users";

    /// Column name constants.
    pub mod columns {

        pub const USER_ID: &str = "user_id";

        pub const PHONE: &str = "phone";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Users {
        pub user_id: String,

        pub phone: Option<String>,
    }

    impl TupleDatumMarker for Users {}

    impl SQLParamMarker for Users {}

    impl Users {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str =
            "SELECT user_id, phone FROM users WHERE user_id = ?";

        /// Insert one row; bind [`Users::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO users (user_id, phone) VALUES (?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM users WHERE user_id = ?";

        pub fn new(user_id: String, phone: Option<String>) -> Self {
            Self { user_id, phone }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (String, Option<String>) {
            (self.user_id.clone(), self.phone.clone())
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct UsersChange {
        pub phone: FieldChange<Option<String>>,
    }

    impl UsersChange {
        pub fn is_empty(&self) -> bool {
            self.phone.is_unchanged()
        }

        /// Builds the `UPDATE users SET ... WHERE user_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, user_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.phone {
                sets.push([columns::PHONE, " = ?"].concat());
                params.push(Box::new(value.clone()));
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

    impl Datum for Users {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Users>)
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

    impl DatumDyn for Users {
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

    impl Entity for Users {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::USER_ID.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::PHONE.to_string(),
                        <String as Datum>::data_type(),
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
            entity::expect_field_count(fields.len(), 2)?;
            Ok(Self {
                user_id: entity::field_from_tuple_binary(TABLE_NAME, columns::USER_ID, &fields[0])?,

                phone: entity::opt_field_from_tuple_binary(&fields[1])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 2)?;
            Ok(Self {
                user_id: entity::field_from_tuple_value(TABLE_NAME, columns::USER_ID, &values[0])?,

                phone: entity::opt_field_from_tuple_value(&values[1])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.user_id)?,
                entity::opt_field_to_tuple_binary(&self.phone)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.user_id)?,
                entity::opt_field_to_tuple_value(&self.phone)?,
            ]))
        }
    }
}
