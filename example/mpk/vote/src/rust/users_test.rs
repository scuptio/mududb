//! Unit tests for the `rust::users::Users` entity.

use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::users::object::{Users, UsersChange, columns};

fn sample_users() -> Users {
    Users::new("user_id_val".to_string(), Some("phone_val".to_string()))
}

fn assert_same_users(lhs: &Users, rhs: &Users) {
    assert_eq!(lhs.user_id, rhs.user_id);
    assert_eq!(lhs.phone, rhs.phone);
}

#[test]
fn users_lifecycle() {
    let entity = sample_users();
    assert_eq!(entity.user_id, "user_id_val");
    assert_eq!(entity.phone.as_deref(), Some("phone_val"));

    // Entity metadata
    assert_eq!(Users::table_name(), "users");
    assert_eq!(Users::TABLE_NAME, "users");
    assert_eq!(Users::tuple_desc().fields().len(), 2);
    assert_eq!(Users::tuple_desc().fields()[0].name(), columns::USER_ID);
    assert!(Users::SQL_GET_BY_PK.contains(columns::USER_ID));
    assert!(Users::SQL_INSERT.contains("INSERT INTO users"));

    // Datum / DatumDyn metadata
    let data_type = Users::data_type();
    assert_eq!(entity.type_family().unwrap(), data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple().unwrap();
    let from_tuple = Users::from_tuple(&tuple).unwrap();
    assert_same_users(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type).unwrap();
    let from_value = Users::from_value(&value).unwrap();
    assert_same_users(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type).unwrap();
    let from_binary = Users::from_binary(binary.as_ref()).unwrap();
    assert_same_users(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type).unwrap();
    let from_textual = Users::from_textual(textual.as_str()).unwrap();
    assert_same_users(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type).unwrap();
    let from_cloned = Users::from_value(&cloned_value).unwrap();
    assert_same_users(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, "user_id_val");
    assert_eq!(params.1.as_deref(), Some("phone_val"));

    // Partial update statement
    let update = UsersChange {
        phone: FieldChange::Set(Some("new_phone".to_string())),
    };
    let (sql, update_params) = update.update_by_pk("user_id_val".to_string()).unwrap();
    assert_eq!(sql, "UPDATE users SET phone = ? WHERE user_id = ?");
    assert_eq!(update_params.len(), 2);
    assert!(UsersChange::default().is_empty());
    assert!(
        UsersChange::default()
            .update_by_pk("user_id_val".to_string())
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = UsersChange {
        phone: FieldChange::Set(None),
    };
    let (null_sql, null_params) = null_update.update_by_pk("user_id_val".to_string()).unwrap();
    assert_eq!(null_sql, "UPDATE users SET phone = ? WHERE user_id = ?");
    assert_eq!(null_params.len(), 2);
    assert!(
        null_params[0]
            .to_value(&String::data_type())
            .unwrap()
            .is_null()
    );
}

#[test]
fn users_nullable_fields_roundtrip() {
    let entity = Users::new("user_id_val".to_string(), None);
    let tuple = entity.to_tuple().unwrap();
    assert!(tuple.is_null(1));
    let restored = Users::from_tuple(&tuple).unwrap();
    assert_same_users(&restored, &entity);

    let value = entity.to_tuple_value().unwrap();
    assert!(value.values()[1].is_null());
    let restored = Users::from_tuple_value(&value).unwrap();
    assert_same_users(&restored, &entity);
}
