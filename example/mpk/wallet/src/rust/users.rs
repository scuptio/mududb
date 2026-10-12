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

        pub const NAME: &str = "name";

        pub const PHONE: &str = "phone";

        pub const EMAIL: &str = "email";

        pub const PASSWORD: &str = "password";

        pub const CREATED_AT: &str = "created_at";

        pub const UPDATED_AT: &str = "updated_at";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Users {
        pub user_id: i32,

        pub name: Option<String>,

        pub phone: Option<String>,

        pub email: Option<String>,

        pub password: Option<String>,

        pub created_at: Option<i32>,

        pub updated_at: Option<i32>,
    }

    impl TupleDatumMarker for Users {}

    impl SQLParamMarker for Users {}

    impl Users {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT user_id, name, phone, email, password, created_at, updated_at FROM users WHERE user_id = ?";

        /// Insert one row; bind [`Users::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO users (user_id, name, phone, email, password, created_at, updated_at) VALUES (?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM users WHERE user_id = ?";

        pub fn new(
            user_id: i32,

            name: Option<String>,

            phone: Option<String>,

            email: Option<String>,

            password: Option<String>,

            created_at: Option<i32>,

            updated_at: Option<i32>,
        ) -> Self {
            Self {
                user_id,

                name,

                phone,

                email,

                password,

                created_at,

                updated_at,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (
            i32,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<String>,
            Option<i32>,
            Option<i32>,
        ) {
            (
                self.user_id,
                self.name.clone(),
                self.phone.clone(),
                self.email.clone(),
                self.password.clone(),
                self.created_at,
                self.updated_at,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct UsersChange {
        pub name: FieldChange<Option<String>>,

        pub phone: FieldChange<Option<String>>,

        pub email: FieldChange<Option<String>>,

        pub password: FieldChange<Option<String>>,

        pub created_at: FieldChange<Option<i32>>,

        pub updated_at: FieldChange<Option<i32>>,
    }

    impl UsersChange {
        pub fn is_empty(&self) -> bool {
            self.name.is_unchanged()
                && self.phone.is_unchanged()
                && self.email.is_unchanged()
                && self.password.is_unchanged()
                && self.created_at.is_unchanged()
                && self.updated_at.is_unchanged()
        }

        /// Builds the `UPDATE users SET ... WHERE user_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, user_id: i32) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.name {
                sets.push([columns::NAME, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.phone {
                sets.push([columns::PHONE, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.email {
                sets.push([columns::EMAIL, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.password {
                sets.push([columns::PASSWORD, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.created_at {
                sets.push([columns::CREATED_AT, " = ?"].concat());
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
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::NAME.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::PHONE.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::EMAIL.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::PASSWORD.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::CREATED_AT.to_string(),
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
            entity::expect_field_count(fields.len(), 7)?;
            Ok(Self {
                user_id: entity::field_from_tuple_binary(TABLE_NAME, columns::USER_ID, &fields[0])?,

                name: entity::opt_field_from_tuple_binary(&fields[1])?,

                phone: entity::opt_field_from_tuple_binary(&fields[2])?,

                email: entity::opt_field_from_tuple_binary(&fields[3])?,

                password: entity::opt_field_from_tuple_binary(&fields[4])?,

                created_at: entity::opt_field_from_tuple_binary(&fields[5])?,

                updated_at: entity::opt_field_from_tuple_binary(&fields[6])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 7)?;
            Ok(Self {
                user_id: entity::field_from_tuple_value(TABLE_NAME, columns::USER_ID, &values[0])?,

                name: entity::opt_field_from_tuple_value(&values[1])?,

                phone: entity::opt_field_from_tuple_value(&values[2])?,

                email: entity::opt_field_from_tuple_value(&values[3])?,

                password: entity::opt_field_from_tuple_value(&values[4])?,

                created_at: entity::opt_field_from_tuple_value(&values[5])?,

                updated_at: entity::opt_field_from_tuple_value(&values[6])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.user_id)?,
                entity::opt_field_to_tuple_binary(&self.name)?,
                entity::opt_field_to_tuple_binary(&self.phone)?,
                entity::opt_field_to_tuple_binary(&self.email)?,
                entity::opt_field_to_tuple_binary(&self.password)?,
                entity::opt_field_to_tuple_binary(&self.created_at)?,
                entity::opt_field_to_tuple_binary(&self.updated_at)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.user_id)?,
                entity::opt_field_to_tuple_value(&self.name)?,
                entity::opt_field_to_tuple_value(&self.phone)?,
                entity::opt_field_to_tuple_value(&self.email)?,
                entity::opt_field_to_tuple_value(&self.password)?,
                entity::opt_field_to_tuple_value(&self.created_at)?,
                entity::opt_field_to_tuple_value(&self.updated_at)?,
            ]))
        }
    }
}
