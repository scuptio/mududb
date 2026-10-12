//! Unit tests for the `rust::users::Users` entity.

use mududb::common::result::RS;
use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::users::object::{Users, UsersChange, columns};

fn sample_users() -> Users {
    Users::new(
        1,
        Some("name_val".to_string()),
        Some("phone_val".to_string()),
        Some("email_val".to_string()),
        Some("password_val".to_string()),
        Some(1),
        Some(1),
    )
}

fn assert_same_users(lhs: &Users, rhs: &Users) {
    assert_eq!(lhs.user_id, rhs.user_id);
    assert_eq!(lhs.name, rhs.name);
    assert_eq!(lhs.phone, rhs.phone);
    assert_eq!(lhs.email, rhs.email);
    assert_eq!(lhs.password, rhs.password);
    assert_eq!(lhs.created_at, rhs.created_at);
    assert_eq!(lhs.updated_at, rhs.updated_at);
}

#[test]
fn users_lifecycle() -> RS<()> {
    let entity = sample_users();
    assert_eq!(entity.user_id, 1);
    assert_eq!(entity.name.as_deref(), Some("name_val"));

    // Entity metadata
    assert_eq!(Users::table_name(), "users");
    assert_eq!(Users::TABLE_NAME, "users");
    assert_eq!(Users::tuple_desc().fields().len(), 7);
    assert_eq!(Users::tuple_desc().fields()[0].name(), columns::USER_ID);
    assert!(Users::SQL_GET_BY_PK.contains(columns::USER_ID));
    assert!(Users::SQL_INSERT.contains("INSERT INTO users"));

    // Datum / DatumDyn metadata
    let data_type = Users::data_type();
    assert_eq!(entity.type_family()?, data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple()?;
    let from_tuple = Users::from_tuple(&tuple)?;
    assert_same_users(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type)?;
    let from_value = Users::from_value(&value)?;
    assert_same_users(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type)?;
    let from_binary = Users::from_binary(binary.as_ref())?;
    assert_same_users(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type)?;
    let from_textual = Users::from_textual(textual.as_str())?;
    assert_same_users(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type)?;
    let from_cloned = Users::from_value(&cloned_value)?;
    assert_same_users(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.1.as_deref(), Some("name_val"));
    assert_eq!(params.6, Some(1));

    // Partial update statement
    let update = UsersChange {
        name: FieldChange::Set(Some("new_name".to_string())),
        updated_at: FieldChange::Set(Some(2)),
        ..UsersChange::default()
    };
    let (sql, update_params) = match update.update_by_pk(1) {
        Some(parts) => parts,
        None => {
            return Err(mududb::mudu_error!(
                mududb::error::ErrorCode::InvalidState,
                "empty update"
            ));
        }
    };
    assert_eq!(
        sql,
        "UPDATE users SET name = ?, updated_at = ? WHERE user_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(UsersChange::default().update_by_pk(1).is_none());

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = UsersChange {
        name: FieldChange::Set(None),
        ..UsersChange::default()
    };
    let (null_sql, null_params) = match null_update.update_by_pk(1) {
        Some(parts) => parts,
        None => {
            return Err(mududb::mudu_error!(
                mududb::error::ErrorCode::InvalidState,
                "empty update"
            ));
        }
    };
    assert_eq!(null_sql, "UPDATE users SET name = ? WHERE user_id = ?");
    assert_eq!(null_params.len(), 2);
    assert!(null_params[0].to_value(&String::data_type())?.is_null());

    Ok(())
}

#[test]
fn users_nullable_fields_roundtrip() -> RS<()> {
    let entity = Users::new(2, None, None, None, None, None, None);
    let tuple = entity.to_tuple()?;
    assert!(tuple.is_null(1));
    assert!(tuple.is_null(6));
    let restored = Users::from_tuple(&tuple)?;
    assert_same_users(&restored, &entity);

    let value = entity.to_tuple_value()?;
    assert!(value.values()[1].is_null());
    let restored = Users::from_tuple_value(&value)?;
    assert_same_users(&restored, &entity);
    Ok(())
}
