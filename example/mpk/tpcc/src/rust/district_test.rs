//! Unit tests for the `rust::district::District` object.

use mududb::contract::database::field_change::FieldChange;
use mududb::contract::database::object::Object;
use mududb::mudu::data_type::numeric::Numeric;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::district::object::{District, DistrictChange, columns};

fn sample_district() -> District {
    District::new(
        1,
        2,
        "d_name_val".to_string(),
        10,
        Numeric::from(1000i32),
        3001,
        3000,
    )
}

fn assert_same_district(lhs: &District, rhs: &District) {
    assert_eq!(lhs.d_id, rhs.d_id);
    assert_eq!(lhs.d_w_id, rhs.d_w_id);
    assert_eq!(lhs.d_name, rhs.d_name);
    assert_eq!(lhs.d_tax, rhs.d_tax);
    assert_eq!(lhs.d_ytd, rhs.d_ytd);
    assert_eq!(lhs.d_next_o_id, rhs.d_next_o_id);
    assert_eq!(lhs.d_last_delivery_o_id, rhs.d_last_delivery_o_id);
}

#[test]
fn district_lifecycle() {
    let object = sample_district();
    assert_eq!(object.d_id, 1);
    assert_eq!(object.d_next_o_id, 3001);

    // Object metadata
    assert_eq!(District::class_name(), "district");
    assert_eq!(District::TABLE_NAME, "district");
    assert_eq!(District::tuple_desc().fields().len(), 7);
    assert_eq!(District::tuple_desc().fields()[0].name(), columns::D_ID);
    assert!(District::SQL_GET_BY_PK.contains(columns::D_ID));
    assert!(District::SQL_INSERT.contains("INSERT INTO district"));

    // Datum / DatumDyn metadata
    let data_type = District::data_type();
    assert_eq!(object.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = object.to_tuple().unwrap();
    let from_tuple = District::from_tuple(&tuple).unwrap();
    assert_same_district(&from_tuple, &object);

    // Value roundtrip
    let value = object.to_value(&data_type).unwrap();
    let from_value = District::from_value(&value).unwrap();
    assert_same_district(&from_value, &object);

    // Binary roundtrip
    let binary = object.to_binary(&data_type).unwrap();
    let from_binary = District::from_binary(binary.as_ref()).unwrap();
    assert_same_district(&from_binary, &object);

    // Textual roundtrip
    let textual = object.to_textual(&data_type).unwrap();
    let from_textual = District::from_textual(textual.as_str()).unwrap();
    assert_same_district(&from_textual, &object);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = object.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = District::from_value(&cloned_value).unwrap();
    assert_same_district(&from_cloned, &object);

    // Insert parameters follow the column declaration order
    let params = object.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.2, "d_name_val");
    assert_eq!(params.5, 3001);

    // Partial update statement over the composite primary key
    let update = DistrictChange {
        d_ytd: FieldChange::Set(Numeric::from(2000i32)),
        d_next_o_id: FieldChange::Set(3002),
        ..DistrictChange::default()
    };
    let (sql, update_params) = update.update_by_pk(1, 2).unwrap();
    assert_eq!(
        sql,
        "UPDATE district SET d_ytd = ?, d_next_o_id = ? WHERE d_id = ? AND d_w_id = ?"
    );
    assert_eq!(update_params.len(), 4);
    assert!(DistrictChange::default().is_empty());
    assert!(DistrictChange::default().update_by_pk(1, 2).is_none());
}
