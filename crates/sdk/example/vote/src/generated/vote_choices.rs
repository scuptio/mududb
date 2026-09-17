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
    pub const TABLE_NAME: &str = "vote_choices";

    /// Column name constants.
    pub mod columns {

        pub const CHOICE_ID: &str = "choice_id";

        pub const ACTION_ID: &str = "action_id";

        pub const OPTION_ID: &str = "option_id";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct VoteChoices {
        pub choice_id: String,

        pub action_id: Option<String>,

        pub option_id: Option<String>,
    }

    impl TupleDatumMarker for VoteChoices {}

    impl SQLParamMarker for VoteChoices {}

    impl VoteChoices {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str =
            "SELECT choice_id, action_id, option_id FROM vote_choices WHERE choice_id = ?";

        /// Insert one row; bind [`VoteChoices::insert_params`].
        pub const SQL_INSERT: &'static str =
            "INSERT INTO vote_choices (choice_id, action_id, option_id) VALUES (?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM vote_choices WHERE choice_id = ?";

        pub fn new(
            choice_id: String,

            action_id: Option<String>,

            option_id: Option<String>,
        ) -> Self {
            Self {
                choice_id,

                action_id,

                option_id,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (String, Option<String>, Option<String>) {
            (
                self.choice_id.clone(),
                self.action_id.clone(),
                self.option_id.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct VoteChoicesChange {
        pub action_id: FieldChange<Option<String>>,

        pub option_id: FieldChange<Option<String>>,
    }

    impl VoteChoicesChange {
        pub fn is_empty(&self) -> bool {
            self.action_id.is_unchanged() && self.option_id.is_unchanged()
        }

        /// Builds the `UPDATE vote_choices SET ... WHERE choice_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, choice_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.action_id {
                sets.push([columns::ACTION_ID, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.option_id {
                sets.push([columns::OPTION_ID, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(choice_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "choice_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for VoteChoices {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<VoteChoices>)
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

    impl DatumDyn for VoteChoices {
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

    impl Entity for VoteChoices {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::CHOICE_ID.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::ACTION_ID.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::OPTION_ID.to_string(),
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
            entity::expect_field_count(fields.len(), 3)?;
            Ok(Self {
                choice_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::CHOICE_ID,
                    &fields[0],
                )?,

                action_id: entity::opt_field_from_tuple_binary(&fields[1])?,

                option_id: entity::opt_field_from_tuple_binary(&fields[2])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 3)?;
            Ok(Self {
                choice_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::CHOICE_ID,
                    &values[0],
                )?,

                action_id: entity::opt_field_from_tuple_value(&values[1])?,

                option_id: entity::opt_field_from_tuple_value(&values[2])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.choice_id)?,
                entity::opt_field_to_tuple_binary(&self.action_id)?,
                entity::opt_field_to_tuple_binary(&self.option_id)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.choice_id)?,
                entity::opt_field_to_tuple_value(&self.action_id)?,
                entity::opt_field_to_tuple_value(&self.option_id)?,
            ]))
        }
    }
}
