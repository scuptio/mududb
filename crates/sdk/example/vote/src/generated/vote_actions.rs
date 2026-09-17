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
    pub const TABLE_NAME: &str = "vote_actions";

    /// Column name constants.
    pub mod columns {

        pub const ACTION_ID: &str = "action_id";

        pub const USER_ID: &str = "user_id";

        pub const VOTE_ID: &str = "vote_id";

        pub const ACTION_TIME: &str = "action_time";

        pub const IS_WITHDRAWN: &str = "is_withdrawn";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct VoteActions {
        pub action_id: String,

        pub user_id: Option<String>,

        pub vote_id: Option<String>,

        pub action_time: i32,

        pub is_withdrawn: Option<i32>,
    }

    impl TupleDatumMarker for VoteActions {}

    impl SQLParamMarker for VoteActions {}

    impl VoteActions {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT action_id, user_id, vote_id, action_time, is_withdrawn FROM vote_actions WHERE action_id = ?";

        /// Insert one row; bind [`VoteActions::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO vote_actions (action_id, user_id, vote_id, action_time, is_withdrawn) VALUES (?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM vote_actions WHERE action_id = ?";

        pub fn new(
            action_id: String,

            user_id: Option<String>,

            vote_id: Option<String>,

            action_time: i32,

            is_withdrawn: Option<i32>,
        ) -> Self {
            Self {
                action_id,

                user_id,

                vote_id,

                action_time,

                is_withdrawn,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (String, Option<String>, Option<String>, i32, Option<i32>) {
            (
                self.action_id.clone(),
                self.user_id.clone(),
                self.vote_id.clone(),
                self.action_time,
                self.is_withdrawn,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct VoteActionsChange {
        pub user_id: FieldChange<Option<String>>,

        pub vote_id: FieldChange<Option<String>>,

        pub action_time: FieldChange<i32>,

        pub is_withdrawn: FieldChange<Option<i32>>,
    }

    impl VoteActionsChange {
        pub fn is_empty(&self) -> bool {
            self.user_id.is_unchanged()
                && self.vote_id.is_unchanged()
                && self.action_time.is_unchanged()
                && self.is_withdrawn.is_unchanged()
        }

        /// Builds the `UPDATE vote_actions SET ... WHERE action_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, action_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.user_id {
                sets.push([columns::USER_ID, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.vote_id {
                sets.push([columns::VOTE_ID, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.action_time {
                sets.push([columns::ACTION_TIME, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.is_withdrawn {
                sets.push([columns::IS_WITHDRAWN, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(action_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "action_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for VoteActions {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<VoteActions>)
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

    impl DatumDyn for VoteActions {
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

    impl Entity for VoteActions {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::ACTION_ID.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::USER_ID.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::VOTE_ID.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::ACTION_TIME.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::IS_WITHDRAWN.to_string(),
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
                action_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::ACTION_ID,
                    &fields[0],
                )?,

                user_id: entity::opt_field_from_tuple_binary(&fields[1])?,

                vote_id: entity::opt_field_from_tuple_binary(&fields[2])?,

                action_time: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::ACTION_TIME,
                    &fields[3],
                )?,

                is_withdrawn: entity::opt_field_from_tuple_binary(&fields[4])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 5)?;
            Ok(Self {
                action_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::ACTION_ID,
                    &values[0],
                )?,

                user_id: entity::opt_field_from_tuple_value(&values[1])?,

                vote_id: entity::opt_field_from_tuple_value(&values[2])?,

                action_time: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::ACTION_TIME,
                    &values[3],
                )?,

                is_withdrawn: entity::opt_field_from_tuple_value(&values[4])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.action_id)?,
                entity::opt_field_to_tuple_binary(&self.user_id)?,
                entity::opt_field_to_tuple_binary(&self.vote_id)?,
                entity::field_to_tuple_binary(&self.action_time)?,
                entity::opt_field_to_tuple_binary(&self.is_withdrawn)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.action_id)?,
                entity::opt_field_to_tuple_value(&self.user_id)?,
                entity::opt_field_to_tuple_value(&self.vote_id)?,
                entity::field_to_tuple_value(&self.action_time)?,
                entity::opt_field_to_tuple_value(&self.is_withdrawn)?,
            ]))
        }
    }
}
