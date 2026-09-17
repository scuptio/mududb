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
    pub const TABLE_NAME: &str = "options";

    /// Column name constants.
    pub mod columns {

        pub const OPTION_ID: &str = "option_id";

        pub const VOTE_ID: &str = "vote_id";

        pub const OPTION_TEXT: &str = "option_text";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Options {
        pub option_id: String,

        pub vote_id: Option<String>,

        pub option_text: String,
    }

    impl TupleDatumMarker for Options {}

    impl SQLParamMarker for Options {}

    impl Options {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str =
            "SELECT option_id, vote_id, option_text FROM options WHERE option_id = ?";

        /// Insert one row; bind [`Options::insert_params`].
        pub const SQL_INSERT: &'static str =
            "INSERT INTO options (option_id, vote_id, option_text) VALUES (?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM options WHERE option_id = ?";

        pub fn new(option_id: String, vote_id: Option<String>, option_text: String) -> Self {
            Self {
                option_id,

                vote_id,

                option_text,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (String, Option<String>, String) {
            (
                self.option_id.clone(),
                self.vote_id.clone(),
                self.option_text.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct OptionsChange {
        pub vote_id: FieldChange<Option<String>>,

        pub option_text: FieldChange<String>,
    }

    impl OptionsChange {
        pub fn is_empty(&self) -> bool {
            self.vote_id.is_unchanged() && self.option_text.is_unchanged()
        }

        /// Builds the `UPDATE options SET ... WHERE option_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(&self, option_id: String) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.vote_id {
                sets.push([columns::VOTE_ID, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.option_text {
                sets.push([columns::OPTION_TEXT, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(option_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "option_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Options {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Options>)
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

    impl DatumDyn for Options {
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

    impl Entity for Options {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::OPTION_ID.to_string(),
                        <String as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::VOTE_ID.to_string(),
                        <String as Datum>::data_type(),
                        true,
                    ),
                    DatumDesc::new_nullable(
                        columns::OPTION_TEXT.to_string(),
                        <String as Datum>::data_type(),
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
            entity::expect_field_count(fields.len(), 3)?;
            Ok(Self {
                option_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::OPTION_ID,
                    &fields[0],
                )?,

                vote_id: entity::opt_field_from_tuple_binary(&fields[1])?,

                option_text: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::OPTION_TEXT,
                    &fields[2],
                )?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 3)?;
            Ok(Self {
                option_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::OPTION_ID,
                    &values[0],
                )?,

                vote_id: entity::opt_field_from_tuple_value(&values[1])?,

                option_text: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::OPTION_TEXT,
                    &values[2],
                )?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.option_id)?,
                entity::opt_field_to_tuple_binary(&self.vote_id)?,
                entity::field_to_tuple_binary(&self.option_text)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.option_id)?,
                entity::opt_field_to_tuple_value(&self.vote_id)?,
                entity::field_to_tuple_value(&self.option_text)?,
            ]))
        }
    }
}
