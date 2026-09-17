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
    pub const TABLE_NAME: &str = "customer";

    /// Column name constants.
    pub mod columns {

        pub const C_ID: &str = "c_id";

        pub const C_D_ID: &str = "c_d_id";

        pub const C_W_ID: &str = "c_w_id";

        pub const C_FIRST: &str = "c_first";

        pub const C_LAST: &str = "c_last";

        pub const C_DISCOUNT: &str = "c_discount";

        pub const C_CREDIT: &str = "c_credit";

        pub const C_BALANCE: &str = "c_balance";

        pub const C_YTD_PAYMENT: &str = "c_ytd_payment";

        pub const C_PAYMENT_CNT: &str = "c_payment_cnt";

        pub const C_DELIVERY_CNT: &str = "c_delivery_cnt";

        pub const C_LAST_ORDER_ID: &str = "c_last_order_id";
    }

    // entity struct definition
    #[derive(Debug, Clone, Default)]
    pub struct Customer {
        pub c_id: i32,

        pub c_d_id: i32,

        pub c_w_id: i32,

        pub c_first: String,

        pub c_last: String,

        pub c_discount: i32,

        pub c_credit: String,

        pub c_balance: mududb::mudu::data_type::numeric::Numeric,

        pub c_ytd_payment: mududb::mudu::data_type::numeric::Numeric,

        pub c_payment_cnt: i32,

        pub c_delivery_cnt: i32,

        pub c_last_order_id: Option<i32>,
    }

    impl TupleDatumMarker for Customer {}

    impl SQLParamMarker for Customer {}

    impl Customer {
        /// Table name.
        pub const TABLE_NAME: &'static str = TABLE_NAME;

        /// Select one row by primary key.
        pub const SQL_GET_BY_PK: &'static str = "SELECT c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id FROM customer WHERE c_id = ? AND c_d_id = ? AND c_w_id = ?";

        /// Insert one row; bind [`Customer::insert_params`].
        pub const SQL_INSERT: &'static str = "INSERT INTO customer (c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)";

        /// Delete one row by primary key.
        pub const SQL_DELETE_BY_PK: &'static str =
            "DELETE FROM customer WHERE c_id = ? AND c_d_id = ? AND c_w_id = ?";

        pub fn new(
            c_id: i32,

            c_d_id: i32,

            c_w_id: i32,

            c_first: String,

            c_last: String,

            c_discount: i32,

            c_credit: String,

            c_balance: mududb::mudu::data_type::numeric::Numeric,

            c_ytd_payment: mududb::mudu::data_type::numeric::Numeric,

            c_payment_cnt: i32,

            c_delivery_cnt: i32,

            c_last_order_id: Option<i32>,
        ) -> Self {
            Self {
                c_id,

                c_d_id,

                c_w_id,

                c_first,

                c_last,

                c_discount,

                c_credit,

                c_balance,

                c_ytd_payment,

                c_payment_cnt,

                c_delivery_cnt,

                c_last_order_id,
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
            String,
            String,
            i32,
            String,
            mududb::mudu::data_type::numeric::Numeric,
            mududb::mudu::data_type::numeric::Numeric,
            i32,
            i32,
            Option<i32>,
        ) {
            (
                self.c_id,
                self.c_d_id,
                self.c_w_id,
                self.c_first.clone(),
                self.c_last.clone(),
                self.c_discount,
                self.c_credit.clone(),
                self.c_balance.clone(),
                self.c_ytd_payment.clone(),
                self.c_payment_cnt,
                self.c_delivery_cnt,
                self.c_last_order_id,
            )
        }
    }

    /// Partial-update changeset: each non-key column is a [`FieldChange`];
    /// `Set` marks the column for update, `Unchanged` leaves it untouched.
    /// For a nullable column, `Set(None)` writes SQL NULL.
    #[derive(Debug, Clone, Default)]
    pub struct CustomerChange {
        pub c_first: FieldChange<String>,

        pub c_last: FieldChange<String>,

        pub c_discount: FieldChange<i32>,

        pub c_credit: FieldChange<String>,

        pub c_balance: FieldChange<mududb::mudu::data_type::numeric::Numeric>,

        pub c_ytd_payment: FieldChange<mududb::mudu::data_type::numeric::Numeric>,

        pub c_payment_cnt: FieldChange<i32>,

        pub c_delivery_cnt: FieldChange<i32>,

        pub c_last_order_id: FieldChange<Option<i32>>,
    }

    impl CustomerChange {
        pub fn is_empty(&self) -> bool {
            self.c_first.is_unchanged()
                && self.c_last.is_unchanged()
                && self.c_discount.is_unchanged()
                && self.c_credit.is_unchanged()
                && self.c_balance.is_unchanged()
                && self.c_ytd_payment.is_unchanged()
                && self.c_payment_cnt.is_unchanged()
                && self.c_delivery_cnt.is_unchanged()
                && self.c_last_order_id.is_unchanged()
        }

        /// Builds the `UPDATE customer SET ... WHERE c_id = ? AND c_d_id = ? AND c_w_id = ?`
        /// statement and its bind parameters (primary key last). Returns `None`
        /// when no column is marked for update.
        pub fn update_by_pk(
            &self,

            c_id: i32,

            c_d_id: i32,

            c_w_id: i32,
        ) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
            let mut sets = Vec::new();
            let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

            if let FieldChange::Set(value) = &self.c_first {
                sets.push([columns::C_FIRST, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.c_last {
                sets.push([columns::C_LAST, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.c_discount {
                sets.push([columns::C_DISCOUNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.c_credit {
                sets.push([columns::C_CREDIT, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.c_balance {
                sets.push([columns::C_BALANCE, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.c_ytd_payment {
                sets.push([columns::C_YTD_PAYMENT, " = ?"].concat());
                params.push(Box::new(value.clone()));
            }

            if let FieldChange::Set(value) = &self.c_payment_cnt {
                sets.push([columns::C_PAYMENT_CNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.c_delivery_cnt {
                sets.push([columns::C_DELIVERY_CNT, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if let FieldChange::Set(value) = &self.c_last_order_id {
                sets.push([columns::C_LAST_ORDER_ID, " = ?"].concat());
                params.push(Box::new(*value));
            }

            if sets.is_empty() {
                return None;
            }

            params.push(Box::new(c_id));

            params.push(Box::new(c_d_id));

            params.push(Box::new(c_w_id));

            let sql = [
                "UPDATE ",
                TABLE_NAME,
                " SET ",
                &sets.join(", "),
                " WHERE ",
                "c_id = ? AND c_d_id = ? AND c_w_id = ?",
            ]
            .concat();
            Some((sql, params))
        }
    }

    impl Datum for Customer {
        fn data_type() -> DataType {
            static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
            ONCE_LOCK
                .get_or_init(entity::entity_data_type::<Customer>)
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

    impl DatumDyn for Customer {
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

    impl Entity for Customer {
        fn tuple_desc() -> &'static TupleFieldDesc {
            static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
            ONCE_LOCK.get_or_init(|| {
                TupleFieldDesc::new(vec![

                DatumDesc::new_nullable(
                    columns::C_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_D_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_W_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_FIRST.to_string(),
                    <String as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_LAST.to_string(),
                    <String as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_DISCOUNT.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_CREDIT.to_string(),
                    <String as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_BALANCE.to_string(),
                    DataType::from_id_param(
            TypeFamily::Numeric,
            Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(
                mududb::types::data_type_param_numeric::DataTypeParamNumeric::new(12, 2),
            ))),
        ),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_YTD_PAYMENT.to_string(),
                    DataType::from_id_param(
            TypeFamily::Numeric,
            Some(mududb::types::data_type_param_kind::DataTypeParamKind::Numeric(Box::new(
                mududb::types::data_type_param_numeric::DataTypeParamNumeric::new(12, 2),
            ))),
        ),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_PAYMENT_CNT.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_DELIVERY_CNT.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::C_LAST_ORDER_ID.to_string(),
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
            entity::expect_field_count(fields.len(), 12)?;
            Ok(Self {
                c_id: entity::field_from_tuple_binary(TABLE_NAME, columns::C_ID, &fields[0])?,

                c_d_id: entity::field_from_tuple_binary(TABLE_NAME, columns::C_D_ID, &fields[1])?,

                c_w_id: entity::field_from_tuple_binary(TABLE_NAME, columns::C_W_ID, &fields[2])?,

                c_first: entity::field_from_tuple_binary(TABLE_NAME, columns::C_FIRST, &fields[3])?,

                c_last: entity::field_from_tuple_binary(TABLE_NAME, columns::C_LAST, &fields[4])?,

                c_discount: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::C_DISCOUNT,
                    &fields[5],
                )?,

                c_credit: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::C_CREDIT,
                    &fields[6],
                )?,

                c_balance: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::C_BALANCE,
                    &fields[7],
                )?,

                c_ytd_payment: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::C_YTD_PAYMENT,
                    &fields[8],
                )?,

                c_payment_cnt: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::C_PAYMENT_CNT,
                    &fields[9],
                )?,

                c_delivery_cnt: entity::field_from_tuple_binary(
                    TABLE_NAME,
                    columns::C_DELIVERY_CNT,
                    &fields[10],
                )?,

                c_last_order_id: entity::opt_field_from_tuple_binary(&fields[11])?,
            })
        }

        fn from_tuple_value(row: &TupleValue) -> RS<Self> {
            let values = row.values();
            entity::expect_field_count(values.len(), 12)?;
            Ok(Self {
                c_id: entity::field_from_tuple_value(TABLE_NAME, columns::C_ID, &values[0])?,

                c_d_id: entity::field_from_tuple_value(TABLE_NAME, columns::C_D_ID, &values[1])?,

                c_w_id: entity::field_from_tuple_value(TABLE_NAME, columns::C_W_ID, &values[2])?,

                c_first: entity::field_from_tuple_value(TABLE_NAME, columns::C_FIRST, &values[3])?,

                c_last: entity::field_from_tuple_value(TABLE_NAME, columns::C_LAST, &values[4])?,

                c_discount: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::C_DISCOUNT,
                    &values[5],
                )?,

                c_credit: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::C_CREDIT,
                    &values[6],
                )?,

                c_balance: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::C_BALANCE,
                    &values[7],
                )?,

                c_ytd_payment: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::C_YTD_PAYMENT,
                    &values[8],
                )?,

                c_payment_cnt: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::C_PAYMENT_CNT,
                    &values[9],
                )?,

                c_delivery_cnt: entity::field_from_tuple_value(
                    TABLE_NAME,
                    columns::C_DELIVERY_CNT,
                    &values[10],
                )?,

                c_last_order_id: entity::opt_field_from_tuple_value(&values[11])?,
            })
        }

        fn to_tuple(&self) -> RS<TupleField> {
            Ok(TupleField::new_nullable(vec![
                entity::field_to_tuple_binary(&self.c_id)?,
                entity::field_to_tuple_binary(&self.c_d_id)?,
                entity::field_to_tuple_binary(&self.c_w_id)?,
                entity::field_to_tuple_binary(&self.c_first)?,
                entity::field_to_tuple_binary(&self.c_last)?,
                entity::field_to_tuple_binary(&self.c_discount)?,
                entity::field_to_tuple_binary(&self.c_credit)?,
                entity::field_to_tuple_binary(&self.c_balance)?,
                entity::field_to_tuple_binary(&self.c_ytd_payment)?,
                entity::field_to_tuple_binary(&self.c_payment_cnt)?,
                entity::field_to_tuple_binary(&self.c_delivery_cnt)?,
                entity::opt_field_to_tuple_binary(&self.c_last_order_id)?,
            ]))
        }

        fn to_tuple_value(&self) -> RS<TupleValue> {
            Ok(TupleValue::from(vec![
                entity::field_to_tuple_value(&self.c_id)?,
                entity::field_to_tuple_value(&self.c_d_id)?,
                entity::field_to_tuple_value(&self.c_w_id)?,
                entity::field_to_tuple_value(&self.c_first)?,
                entity::field_to_tuple_value(&self.c_last)?,
                entity::field_to_tuple_value(&self.c_discount)?,
                entity::field_to_tuple_value(&self.c_credit)?,
                entity::field_to_tuple_value(&self.c_balance)?,
                entity::field_to_tuple_value(&self.c_ytd_payment)?,
                entity::field_to_tuple_value(&self.c_payment_cnt)?,
                entity::field_to_tuple_value(&self.c_delivery_cnt)?,
                entity::opt_field_to_tuple_value(&self.c_last_order_id)?,
            ]))
        }
    }
}
