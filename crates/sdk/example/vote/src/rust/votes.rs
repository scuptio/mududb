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
    pub const TABLE_NAME: &str = "votes";

    /// Column name constants.
    pub mod columns {

        pub const VOTE_ID: &str = "vote_id";

        pub const CREATOR_ID: &str = "creator_id";

        pub const TOPIC: &str = "topic";

        pub const VOTE_TYPE: &str = "vote_type";

        pub const MAX_CHOICES: &str = "max_choices";

        pub const END_TIME: &str = "end_time";

        pub const VISIBILITY_RULE: &str = "visibility_rule";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Votes {
        pub vote_id: String,

        pub creator_id: Option<String>,

        pub topic: String,

        pub vote_type: Option<String>,

        pub max_choices: Option<i32>,

        pub end_time: i32,

        pub visibility_rule: Option<String>,
    }

    impl TupleDatumMarker for Votes {}

    impl SQLParamMarker for Votes {}

    impl Votes {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT vote_id, creator_id, topic, vote_type, max_choices, end_time, visibility_rule FROM votes WHERE vote_id = ?";

        /// Insert one row; bind [`Votes::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO votes (vote_id, creator_id, topic, vote_type, max_choices, end_time, visibility_rule) VALUES (?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM votes WHERE vote_id = ?";

        pub fn new(
            vote_id: String,

            creator_id: Option<String>,

            topic: String,

            vote_type: Option<String>,

            max_choices: Option<i32>,

            end_time: i32,

            visibility_rule: Option<String>,
        ) -> Self {
            Self {
                vote_id,

                creator_id,

                topic,

                vote_type,

                max_choices,

                end_time,

                visibility_rule,
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
            String,
            Option<String>,
            Option<i32>,
            i32,
            Option<String>,
        ) {
            (
                self.vote_id.clone(),
                self.creator_id.clone(),
                self.topic.clone(),
                self.vote_type.clone(),
                self.max_choices,
                self.end_time,
                self.visibility_rule.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct VotesChange {
        pub creator_id: FieldChange<Option<String>>,

        pub topic: FieldChange<String>,

        pub vote_type: FieldChange<Option<String>>,

        pub max_choices: FieldChange<Option<i32>>,

        pub end_time: FieldChange<i32>,

        pub visibility_rule: FieldChange<Option<String>>,
    }

    impl VotesChange {
        pub fn is_empty(&self) -> bool {
            self.creator_id.is_unchanged()
                && self.topic.is_unchanged()
                && self.vote_type.is_unchanged()
                && self.max_choices.is_unchanged()
                && self.end_time.is_unchanged()
                && self.visibility_rule.is_unchanged()
        }

        /// Builds the `UPDATE votes SET ... WHERE vote_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, vote_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.creator_id {
                sets.push([columns::CREATOR_ID, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.topic {
                sets.push([columns::TOPIC, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.vote_type {
                sets.push([columns::VOTE_TYPE, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.max_choices {
                sets.push([columns::MAX_CHOICES, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.end_time {
                sets.push([columns::END_TIME, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.visibility_rule {
                sets.push([columns::VISIBILITY_RULE, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(vote_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "vote_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Votes {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Votes>)
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

    impl DatumDyn for Votes {
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

    impl Entity for Votes {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::VOTE_ID.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::CREATOR_ID.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::TOPIC.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::VOTE_TYPE.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::MAX_CHOICES.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::END_TIME.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::VISIBILITY_RULE.to_string(),
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
            entity::expect_field_count(fields.len(), 7)?;
            Ok(Self {
                vote_id: entity::field_from_tuple_binary(TABLE_NAME, columns::VOTE_ID, &fields[0])?,

                creator_id: entity::opt_field_from_tuple_binary(&fields[1])?,

                topic: entity::field_from_tuple_binary(TABLE_NAME, columns::TOPIC, &fields[2])?,

                vote_type: entity::opt_field_from_tuple_binary(&fields[3])?,

                max_choices: entity::opt_field_from_tuple_binary(&fields[4])?,

                end_time: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::END_TIME,
                    &fields[5],
                )?,

                visibility_rule: entity::opt_field_from_tuple_binary(&fields[6])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 7)?;
            Ok(Self {
                vote_id: entity::field_from_tuple_value(TABLE_NAME, columns::VOTE_ID, &values[0])?,

                creator_id: entity::opt_field_from_tuple_value(&values[1])?,

                topic: entity::field_from_tuple_value(TABLE_NAME, columns::TOPIC, &values[2])?,

                vote_type: entity::opt_field_from_tuple_value(&values[3])?,

                max_choices: entity::opt_field_from_tuple_value(&values[4])?,

                end_time: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::END_TIME,
                    &values[5],
                )?,

                visibility_rule: entity::opt_field_from_tuple_value(&values[6])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.vote_id)?,
                entity::opt_field_to_tuple_binary(&self.creator_id)?,
                entity::field_to_tuple_binary(&self.topic)?,
                entity::opt_field_to_tuple_binary(&self.vote_type)?,
                entity::opt_field_to_tuple_binary(&self.max_choices)?,
                entity::field_to_tuple_binary(&self.end_time)?,
                entity::opt_field_to_tuple_binary(&self.visibility_rule)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.vote_id)?,
                entity::opt_field_to_tuple_value(&self.creator_id)?,
                entity::field_to_tuple_value(&self.topic)?,
                entity::opt_field_to_tuple_value(&self.vote_type)?,
                entity::opt_field_to_tuple_value(&self.max_choices)?,
                entity::field_to_tuple_value(&self.end_time)?,
                entity::opt_field_to_tuple_value(&self.visibility_rule)?,
            ]))
        }
    }
}
