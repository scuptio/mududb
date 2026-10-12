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
    pub const TABLE_NAME: &str = "new_order";

    /// Column name constants.
    pub mod columns {

        pub const NO_O_ID: &str = "no_o_id";

        pub const NO_D_ID: &str = "no_d_id";

        pub const NO_W_ID: &str = "no_w_id";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct NewOrder {
        pub no_o_id: i32,

        pub no_d_id: i32,

        pub no_w_id: i32,
    }

    impl TupleDatumMarker for NewOrder {}

    impl SQLParamMarker for NewOrder {}

    impl NewOrder {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT no_o_id, no_d_id, no_w_id FROM new_order WHERE no_o_id = ? AND no_d_id = ? AND no_w_id = ?";

        /// Insert one row; bind [`NewOrder::insert_params`].
        pub const SQL_INSERT: &'static str =
            "INSERT INTO new_order (no_o_id, no_d_id, no_w_id) VALUES (?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str =
            "DELETE FROM new_order WHERE no_o_id = ? AND no_d_id = ? AND no_w_id = ?";

        pub fn new(no_o_id: i32, no_d_id: i32, no_w_id: i32) -> Self {
            Self {
                no_o_id,

                no_d_id,

                no_w_id,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(&self) -> (i32, i32, i32) {
            (self.no_o_id, self.no_d_id, self.no_w_id)
        }
    }

    impl Datum for NewOrder {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<NewOrder>)
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

    impl DatumDyn for NewOrder {
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

    impl Entity for NewOrder {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![
                    DatumDesc::new_nullable(
                        columns::NO_O_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::NO_D_ID.to_string(),
                        <i32 as Datum>::data_type(),
                        false,
                    ),
                    DatumDesc::new_nullable(
                        columns::NO_W_ID.to_string(),
                        <i32 as Datum>::data_type(),
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
                no_o_id: entity::field_from_tuple_binary(TABLE_NAME, columns::NO_O_ID, &fields[0])?,

                no_d_id: entity::field_from_tuple_binary(TABLE_NAME, columns::NO_D_ID, &fields[1])?,

                no_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::NO_W_ID, &fields[2])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 3)?;
            Ok(Self {
                no_o_id: entity::field_from_tuple_value(TABLE_NAME, columns::NO_O_ID, &values[0])?,

                no_d_id: entity::field_from_tuple_value(TABLE_NAME, columns::NO_D_ID, &values[1])?,

                no_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::NO_W_ID, &values[2])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.no_o_id)?,
                entity::field_to_tuple_binary(&self.no_d_id)?,
                entity::field_to_tuple_binary(&self.no_w_id)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.no_o_id)?,
                entity::field_to_tuple_value(&self.no_d_id)?,
                entity::field_to_tuple_value(&self.no_w_id)?,
            ]))
        }
    }
}
