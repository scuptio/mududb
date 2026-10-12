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
    pub const TABLE_NAME: &str = "order_line";

    /// Column name constants.
    pub mod columns {

        pub const OL_O_ID: &str = "ol_o_id";

        pub const OL_D_ID: &str = "ol_d_id";

        pub const OL_W_ID: &str = "ol_w_id";

        pub const OL_NUMBER: &str = "ol_number";

        pub const OL_I_ID: &str = "ol_i_id";

        pub const OL_SUPPLY_W_ID: &str = "ol_supply_w_id";

        pub const OL_DELIVERY_D: &str = "ol_delivery_d";

        pub const OL_QUANTITY: &str = "ol_quantity";

        pub const OL_AMOUNT: &str = "ol_amount";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct OrderLine {
        pub ol_o_id: i32,

        pub ol_d_id: i32,

        pub ol_w_id: i32,

        pub ol_number: i32,

        pub ol_i_id: i32,

        pub ol_supply_w_id: i32,

        pub ol_delivery_d: Option<String>,

        pub ol_quantity: i32,

        pub ol_amount: mududb::mudu::data_type::numeric::Numeric,
    }

    impl TupleDatumMarker for OrderLine {}

    impl SQLParamMarker for OrderLine {}

    impl OrderLine {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT ol_o_id, ol_d_id, ol_w_id, ol_number, ol_i_id, ol_supply_w_id, ol_delivery_d, ol_quantity, ol_amount FROM order_line WHERE ol_o_id = ? AND ol_d_id = ? AND ol_w_id = ? AND ol_number = ?";

        /// Insert one row; bind [`OrderLine::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO order_line (ol_o_id, ol_d_id, ol_w_id, ol_number, ol_i_id, ol_supply_w_id, ol_delivery_d, ol_quantity, ol_amount) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM order_line WHERE ol_o_id = ? AND ol_d_id = ? AND ol_w_id = ? AND ol_number = ?";

        pub fn new(
            ol_o_id: i32,

            ol_d_id: i32,

            ol_w_id: i32,

            ol_number: i32,

            ol_i_id: i32,

            ol_supply_w_id: i32,

            ol_delivery_d: Option<String>,

            ol_quantity: i32,

            ol_amount: mududb::mudu::data_type::numeric::Numeric,
        ) -> Self {
            Self {
                ol_o_id,

                ol_d_id,

                ol_w_id,

                ol_number,

                ol_i_id,

                ol_supply_w_id,

                ol_delivery_d,

                ol_quantity,

                ol_amount,
            }
        }

        /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
        // The explicit full-row tuple is the point of the typed API.
        #[allow(clippy::type_complexity)]
        pub fn insert_params(
            &self,
        ) -> (
            i32,
            i32,
            i32,
            i32,
            i32,
            i32,
            Option<String>,
            i32,
            mududb::mudu::data_type::numeric::Numeric,
        ) {
            (
                self.ol_o_id,
                self.ol_d_id,
                self.ol_w_id,
                self.ol_number,
                self.ol_i_id,
                self.ol_supply_w_id,
                self.ol_delivery_d.clone(),
                self.ol_quantity,
                self.ol_amount.clone(),
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct OrderLineChange {
        pub ol_i_id: FieldChange<i32>,

        pub ol_supply_w_id: FieldChange<i32>,

        pub ol_delivery_d: FieldChange<Option<String>>,

        pub ol_quantity: FieldChange<i32>,

        pub ol_amount: FieldChange<mududb::mudu::data_type::numeric::Numeric>,
    }

    impl OrderLineChange {
        pub fn is_empty(&self) -> bool {
            self.ol_i_id.is_unchanged()
                && self.ol_supply_w_id.is_unchanged()
                && self.ol_delivery_d.is_unchanged()
                && self.ol_quantity.is_unchanged()
                && self.ol_amount.is_unchanged()
        }

        /// Builds the `UPDATE order_line SET ... WHERE ol_o_id = ? AND ol_d_id = ? AND ol_w_id = ? AND ol_number = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(
            &self,

            ol_o_id: i32,

            ol_d_id: i32,

            ol_w_id: i32,

            ol_number: i32,
        ) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.ol_i_id {
                sets.push([columns::OL_I_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.ol_supply_w_id {
                sets.push([columns::OL_SUPPLY_W_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.ol_delivery_d {
                sets.push([columns::OL_DELIVERY_D, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.ol_quantity {
                sets.push([columns::OL_QUANTITY, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.ol_amount {
                sets.push([columns::OL_AMOUNT, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(ol_o_id));

            params.push(Box::new(ol_d_id));

            params.push(Box::new(ol_w_id));

            params.push(Box::new(ol_number));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "ol_o_id = ? AND ol_d_id = ? AND ol_w_id = ? AND ol_number = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for OrderLine {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<OrderLine>)
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

    impl DatumDyn for OrderLine {
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

    impl Entity for OrderLine {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![

                DatumDesc::new_nullable(
                    columns::OL_O_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_D_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_NUMBER.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_I_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_SUPPLY_W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_DELIVERY_D.to_string(),
                    <String as Datum>::data_type(),
                    true,
                ),

                DatumDesc::new_nullable(
                    columns::OL_QUANTITY.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::OL_AMOUNT.to_string(),
                    DataType::from_id_param(
            TypeFamily::Numeric,
            Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(
                mududb::types::data_type_param_numeric::DataTypeParamNumeric::new(8, 2),
            ))),
        ),
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
            entity::expect_field_count(fields.len(), 9)?;
            Ok(Self {
                ol_o_id: entity::field_from_tuple_binary(TABLE_NAME, columns::OL_O_ID, &fields[0])?,

                ol_d_id: entity::field_from_tuple_binary(TABLE_NAME, columns::OL_D_ID, &fields[1])?,

                ol_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::OL_W_ID, &fields[2])?,

                ol_number: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::OL_NUMBER,
                    &fields[3],
                )?,

                ol_i_id: entity::field_from_tuple_binary(TABLE_NAME, columns::OL_I_ID, &fields[4])?,

                ol_supply_w_id: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::OL_SUPPLY_W_ID,
                    &fields[5],
                )?,

                ol_delivery_d: entity::opt_field_from_tuple_binary(&fields[6])?,

                ol_quantity: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::OL_QUANTITY,
                    &fields[7],
                )?,

                ol_amount: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::OL_AMOUNT,
                    &fields[8],
                )?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 9)?;
            Ok(Self {
                ol_o_id: entity::field_from_tuple_value(TABLE_NAME, columns::OL_O_ID, &values[0])?,

                ol_d_id: entity::field_from_tuple_value(TABLE_NAME, columns::OL_D_ID, &values[1])?,

                ol_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::OL_W_ID, &values[2])?,

                ol_number: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::OL_NUMBER,
                    &values[3],
                )?,

                ol_i_id: entity::field_from_tuple_value(TABLE_NAME, columns::OL_I_ID, &values[4])?,

                ol_supply_w_id: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::OL_SUPPLY_W_ID,
                    &values[5],
                )?,

                ol_delivery_d: entity::opt_field_from_tuple_value(&values[6])?,

                ol_quantity: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::OL_QUANTITY,
                    &values[7],
                )?,

                ol_amount: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::OL_AMOUNT,
                    &values[8],
                )?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.ol_o_id)?,
                entity::field_to_tuple_binary(&self.ol_d_id)?,
                entity::field_to_tuple_binary(&self.ol_w_id)?,
                entity::field_to_tuple_binary(&self.ol_number)?,
                entity::field_to_tuple_binary(&self.ol_i_id)?,
                entity::field_to_tuple_binary(&self.ol_supply_w_id)?,
                entity::opt_field_to_tuple_binary(&self.ol_delivery_d)?,
                entity::field_to_tuple_binary(&self.ol_quantity)?,
                entity::field_to_tuple_binary(&self.ol_amount)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.ol_o_id)?,
                entity::field_to_tuple_value(&self.ol_d_id)?,
                entity::field_to_tuple_value(&self.ol_w_id)?,
                entity::field_to_tuple_value(&self.ol_number)?,
                entity::field_to_tuple_value(&self.ol_i_id)?,
                entity::field_to_tuple_value(&self.ol_supply_w_id)?,
                entity::opt_field_to_tuple_value(&self.ol_delivery_d)?,
                entity::field_to_tuple_value(&self.ol_quantity)?,
                entity::field_to_tuple_value(&self.ol_amount)?,
            ]))
        }
    }
}
