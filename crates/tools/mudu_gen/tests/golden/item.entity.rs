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
pub const TABLE_NAME: &str = "item";

/// Column name constants.
pub mod columns {

    pub const I_ID: &str = "i_id";

    pub const I_NAME: &str = "i_name";

    pub const I_PRICE: &str = "i_price";

    pub const I_DATA: &str = "i_data";

    pub const I_IM_ID: &str = "i_im_id";

}

// entity struct definition
#[derive(Debug, Clone, Default)]
pub struct Item {

    pub i_id: i32,

    pub i_name: Option<String>,

    pub i_price: f64,

    pub i_data: Option<String>,

    pub i_im_id: Option<i32>,

}

impl TupleDatumMarker for Item {}

impl SQLParamMarker for Item {}

impl Item {
    /// Table name.
    pub const TABLE_NAME: &'static str = TABLE_NAME;

    /// Select one row by primary key.
    pub const SQL_GET_BY_PK: &'static str = "SELECT i_id, i_name, i_price, i_data, i_im_id FROM item WHERE i_id = ?";

    /// Insert one row; bind [`Item::insert_params`].
    pub const SQL_INSERT: &'static str = "INSERT INTO item (i_id, i_name, i_price, i_data, i_im_id) VALUES (?, ?, ?, ?, ?)";

    /// Delete one row by primary key.
    pub const SQL_DELETE_BY_PK: &'static str = "DELETE FROM item WHERE i_id = ?";


    pub fn new(

        i_id: i32,

        i_name: Option<String>,

        i_price: f64,

        i_data: Option<String>,

        i_im_id: Option<i32>,

    ) -> Self {
        Self {

            i_id,

            i_name,

            i_price,

            i_data,

            i_im_id,

        }
    }

    /// Typed bind parameters for [`SQL_INSERT`], in column declaration order.
    // The explicit full-row tuple is the point of the typed API.
    #[allow(clippy::type_complexity)]
    pub fn insert_params(&self) -> (

        i32,

        Option<String>,

        f64,

        Option<String>,

        Option<i32>,

    ) {
        (

            self.i_id,

            self.i_name.clone(),

            self.i_price,

            self.i_data.clone(),

            self.i_im_id,

        )
    }
}


/// Partial-update changeset: each non-key column is a [`FieldChange`];
/// `Set` marks the column for update, `Unchanged` leaves it untouched.
/// For a nullable column, `Set(None)` writes SQL NULL.
#[derive(Debug, Clone, Default)]
pub struct ItemChange {

    pub i_name: FieldChange<Option<String>>,

    pub i_price: FieldChange<f64>,

    pub i_data: FieldChange<Option<String>>,

    pub i_im_id: FieldChange<Option<i32>>,

}

impl ItemChange {
    pub fn is_empty(&self) -> bool {


        self.i_name.is_unchanged()



            && self.i_price.is_unchanged()



            && self.i_data.is_unchanged()



            && self.i_im_id.is_unchanged()


    }

    /// Builds the `UPDATE item SET ... WHERE i_id = ?`
    /// statement and its bind parameters (primary key last). Returns `None`
    /// when no column is marked for update.
    pub fn update_by_pk(
        &self,

        i_id: i32,

    ) -> Option<(String, Vec<Box<dyn DatumDyn>>)> {
        let mut sets = Vec::new();
        let mut params: Vec<Box<dyn DatumDyn>> = Vec::new();

        if let FieldChange::Set(value) = &self.i_name {
            sets.push([columns::I_NAME, " = ?"].concat());
            params.push(Box::new(value.clone()));
        }

        if let FieldChange::Set(value) = &self.i_price {
            sets.push([columns::I_PRICE, " = ?"].concat());
            params.push(Box::new(*value));
        }

        if let FieldChange::Set(value) = &self.i_data {
            sets.push([columns::I_DATA, " = ?"].concat());
            params.push(Box::new(value.clone()));
        }

        if let FieldChange::Set(value) = &self.i_im_id {
            sets.push([columns::I_IM_ID, " = ?"].concat());
            params.push(Box::new(*value));
        }

        if sets.is_empty() {
            return None;
        }

        params.push(Box::new(i_id));

        let sql = [
            "UPDATE ",
            TABLE_NAME,
            " SET ",
            &sets.join(", "),
            " WHERE ",
            "i_id = ?",
        ]
        .concat();
        Some((sql, params))
    }
}


impl Datum for Item {
    fn data_type() -> DataType {
        static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
        ONCE_LOCK
            .get_or_init(entity::entity_data_type::<Item>)
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

impl DatumDyn for Item {
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

impl Entity for Item {
    fn tuple_desc() -> &'static TupleFieldDesc {
        static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| {
            TupleFieldDesc::new(vec![

                DatumDesc::new_nullable(
                    columns::I_ID.to_string(),
                    <i32 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::I_NAME.to_string(),
                    <String as Datum>::data_type(),
                    true,
                ),

                DatumDesc::new_nullable(
                    columns::I_PRICE.to_string(),
                    <f64 as Datum>::data_type(),
                    false,
                ),

                DatumDesc::new_nullable(
                    columns::I_DATA.to_string(),
                    <String as Datum>::data_type(),
                    true,
                ),

                DatumDesc::new_nullable(
                    columns::I_IM_ID.to_string(),
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


            i_id: entity::field_from_tuple_binary(
                TABLE_NAME,
                columns::I_ID,
                &fields[0],
            )?,



            i_name: entity::opt_field_from_tuple_binary(&fields[1])?,



            i_price: entity::field_from_tuple_binary(
                TABLE_NAME,
                columns::I_PRICE,
                &fields[2],
            )?,



            i_data: entity::opt_field_from_tuple_binary(&fields[3])?,



            i_im_id: entity::opt_field_from_tuple_binary(&fields[4])?,


        })
    }

    fn from_tuple_value(row: &TupleValue) -> RS<Self> {
        let values = row.values();
        entity::expect_field_count(values.len(), 5)?;
        Ok(Self {


            i_id: entity::field_from_tuple_value(
                TABLE_NAME,
                columns::I_ID,
                &values[0],
            )?,



            i_name: entity::opt_field_from_tuple_value(&values[1])?,



            i_price: entity::field_from_tuple_value(
                TABLE_NAME,
                columns::I_PRICE,
                &values[2],
            )?,



            i_data: entity::opt_field_from_tuple_value(&values[3])?,



            i_im_id: entity::opt_field_from_tuple_value(&values[4])?,


        })
    }

    fn to_tuple(&self) -> RS<TupleField> {
        Ok(TupleField::new_nullable(vec![


            entity::field_to_tuple_binary(&self.i_id)?,



            entity::opt_field_to_tuple_binary(&self.i_name)?,



            entity::field_to_tuple_binary(&self.i_price)?,



            entity::opt_field_to_tuple_binary(&self.i_data)?,



            entity::opt_field_to_tuple_binary(&self.i_im_id)?,


        ]))
    }

    fn to_tuple_value(&self) -> RS<TupleValue> {
        Ok(TupleValue::from(vec![


            entity::field_to_tuple_value(&self.i_id)?,



            entity::opt_field_to_tuple_value(&self.i_name)?,



            entity::field_to_tuple_value(&self.i_price)?,



            entity::opt_field_to_tuple_value(&self.i_data)?,



            entity::opt_field_to_tuple_value(&self.i_im_id)?,


        ]))
    }
}

}
