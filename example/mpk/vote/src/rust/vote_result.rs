pub mod object {
    use mududb::common::result::RS;
    use mududb::contract::database::entity::{self, Entity};

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
    pub const TABLE_NAME: &str = "vote_result";

    /// Column name constants.
    pub mod columns {

        pub const VOTE_ID: &str = "vote_id";

        pub const TOPIC: &str = "topic";

        pub const VOTE_ENDED: &str = "vote_ended";

        pub const TOTAL_VOTES: &str = "total_votes";

        pub const OPTIONS: &str = "options";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct VoteResult {
        pub vote_id: Option<String>,

        pub topic: Option<String>,

        pub vote_ended: Option<i32>,

        pub total_votes: Option<i32>,

        pub options: Option<String>,
    }

    impl TupleDatumMarker for VoteResult {}

    impl SQLParamMarker for VoteResult {}

    impl VoteResult {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Insert one row; bind [`VoteResult::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO vote_result (vote_id, topic, vote_ended, total_votes, options) VALUES (?, ?, ?, ?, ?)";

        pub fn new(
            vote_id: Option<String>,

            topic: Option<String>,

            vote_ended: Option<i32>,

            total_votes: Option<i32>,

            options: Option<String>,
        ) -> Self {
            Self {
                vote_id,

                topic,

                vote_ended,

                total_votes,

                options,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (
            Option<String>,
            Option<String>,
            Option<i32>,
            Option<i32>,
            Option<String>,
        ) {
            (
                self.vote_id.clone(),
                self.topic.clone(),
                self.vote_ended,
                self.total_votes,
                self.options.clone(),
            )
        }
    }

    impl Datum for VoteResult {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<VoteResult>)
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

    impl DatumDyn for VoteResult {
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

    impl Entity for VoteResult {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::VOTE_ID.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::TOPIC.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::VOTE_ENDED.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::TOTAL_VOTES.to_string(),
                        <i32 as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::OPTIONS.to_string(),
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
            entity::expect_field_count(fields.len(), 5)?;
            Ok(Self {
                vote_id: entity::opt_field_from_tuple_binary(&fields[0])?,

                topic: entity::opt_field_from_tuple_binary(&fields[1])?,

                vote_ended: entity::opt_field_from_tuple_binary(&fields[2])?,

                total_votes: entity::opt_field_from_tuple_binary(&fields[3])?,

                options: entity::opt_field_from_tuple_binary(&fields[4])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 5)?;
            Ok(Self {
                vote_id: entity::opt_field_from_tuple_value(&values[0])?,

                topic: entity::opt_field_from_tuple_value(&values[1])?,

                vote_ended: entity::opt_field_from_tuple_value(&values[2])?,

                total_votes: entity::opt_field_from_tuple_value(&values[3])?,

                options: entity::opt_field_from_tuple_value(&values[4])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::opt_field_to_tuple_binary(&self.vote_id)?,
                entity::opt_field_to_tuple_binary(&self.topic)?,
                entity::opt_field_to_tuple_binary(&self.vote_ended)?,
                entity::opt_field_to_tuple_binary(&self.total_votes)?,
                entity::opt_field_to_tuple_binary(&self.options)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::opt_field_to_tuple_value(&self.vote_id)?,
                entity::opt_field_to_tuple_value(&self.topic)?,
                entity::opt_field_to_tuple_value(&self.vote_ended)?,
                entity::opt_field_to_tuple_value(&self.total_votes)?,
                entity::opt_field_to_tuple_value(&self.options)?,
            ]))
        }
    }
}
