//! Unit tests for the `rust::users::Users` entity.

use mududb::common::result::RS;
use mududb::contract::database::entity::Entity;
use mududb::error::{ErrorCode, mudu_error};
use mududb::types::data_value::DataValue;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::users::object::Users;

#[test]
fn users_lifecycle() -> RS<()> {
    let user_id_sample = 1i32;
    let name_sample = "name_val".to_string();
    let phone_sample = "phone_val".to_string();
    let email_sample = "email_val".to_string();
    let password_sample = "password_val".to_string();
    let created_at_sample = 1i32;
    let updated_at_sample = 1i32;
    let mut entity = Users::new(
        Some(user_id_sample),
        Some(name_sample.clone()),
        Some(phone_sample.clone()),
        Some(email_sample.clone()),
        Some(password_sample.clone()),
        Some(created_at_sample),
        Some(updated_at_sample),
    );
    assert_eq!(entity.get_user_id(), &Some(user_id_sample));
    assert_eq!(entity.get_name(), &Some(name_sample.clone()));
    assert_eq!(entity.get_phone(), &Some(phone_sample.clone()));
    assert_eq!(entity.get_email(), &Some(email_sample.clone()));
    assert_eq!(entity.get_password(), &Some(password_sample.clone()));
    assert_eq!(entity.get_created_at(), &Some(created_at_sample));
    assert_eq!(entity.get_updated_at(), &Some(updated_at_sample));

    // Entity metadata
    assert_eq!(Users::object_name(), "users");
    assert_eq!(Users::tuple_desc().fields().len(), 7);

    // Datum / DatumDyn metadata
    let data_type = Users::data_type();
    assert_eq!(entity.type_family()?, data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple()?;
    let from_tuple = Users::from_tuple(&tuple)?;
    assert_eq!(from_tuple.get_user_id(), &Some(user_id_sample));
    assert_eq!(from_tuple.get_name(), &Some(name_sample.clone()));
    assert_eq!(from_tuple.get_phone(), &Some(phone_sample.clone()));
    assert_eq!(from_tuple.get_email(), &Some(email_sample.clone()));
    assert_eq!(from_tuple.get_password(), &Some(password_sample.clone()));
    assert_eq!(from_tuple.get_created_at(), &Some(created_at_sample));
    assert_eq!(from_tuple.get_updated_at(), &Some(updated_at_sample));

    // Value roundtrip
    let value = entity.to_value(&data_type)?;
    let from_value = Users::from_value(&value)?;
    assert_eq!(from_value.get_user_id(), &Some(user_id_sample));
    assert_eq!(from_value.get_name(), &Some(name_sample.clone()));
    assert_eq!(from_value.get_phone(), &Some(phone_sample.clone()));
    assert_eq!(from_value.get_email(), &Some(email_sample.clone()));
    assert_eq!(from_value.get_password(), &Some(password_sample.clone()));
    assert_eq!(from_value.get_created_at(), &Some(created_at_sample));
    assert_eq!(from_value.get_updated_at(), &Some(updated_at_sample));

    // Binary roundtrip
    let binary = entity.to_binary(&data_type)?;
    let from_binary = Users::from_binary(binary.as_ref())?;
    assert_eq!(from_binary.get_user_id(), &Some(user_id_sample));
    assert_eq!(from_binary.get_name(), &Some(name_sample.clone()));
    assert_eq!(from_binary.get_phone(), &Some(phone_sample.clone()));
    assert_eq!(from_binary.get_email(), &Some(email_sample.clone()));
    assert_eq!(from_binary.get_password(), &Some(password_sample.clone()));
    assert_eq!(from_binary.get_created_at(), &Some(created_at_sample));
    assert_eq!(from_binary.get_updated_at(), &Some(updated_at_sample));

    // Textual roundtrip
    let textual = entity.to_textual(&data_type)?;
    let from_textual = Users::from_textual(textual.as_str())?;
    assert_eq!(from_textual.get_user_id(), &Some(user_id_sample));
    assert_eq!(from_textual.get_name(), &Some(name_sample.clone()));
    assert_eq!(from_textual.get_phone(), &Some(phone_sample.clone()));
    assert_eq!(from_textual.get_email(), &Some(email_sample.clone()));
    assert_eq!(from_textual.get_password(), &Some(password_sample.clone()));
    assert_eq!(from_textual.get_created_at(), &Some(created_at_sample));
    assert_eq!(from_textual.get_updated_at(), &Some(updated_at_sample));

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type)?;
    let from_cloned = Users::from_value(&cloned_value)?;
    assert_eq!(from_cloned.get_user_id(), &Some(user_id_sample));
    assert_eq!(from_cloned.get_name(), &Some(name_sample.clone()));
    assert_eq!(from_cloned.get_phone(), &Some(phone_sample.clone()));
    assert_eq!(from_cloned.get_email(), &Some(email_sample.clone()));
    assert_eq!(from_cloned.get_password(), &Some(password_sample.clone()));
    assert_eq!(from_cloned.get_created_at(), &Some(created_at_sample));
    assert_eq!(from_cloned.get_updated_at(), &Some(updated_at_sample));

    // Field-level binary/value accessors
    {
        let bin = entity
            .get_field_binary("user_id")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("user_id", &bin)?;
        let val = entity
            .get_field_value("user_id")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_i32(), Some(&user_id_sample));
        entity.set_field_value("user_id", DataValue::from_i32(user_id_sample))?;
        assert_eq!(
            entity
                .get_field_value("user_id")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_i32(),
            Some(&user_id_sample)
        );
    }
    {
        let bin = entity
            .get_field_binary("name")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("name", &bin)?;
        let val = entity
            .get_field_value("name")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_string(), Some(&name_sample));
        entity.set_field_value("name", DataValue::from_string(name_sample.clone()))?;
        assert_eq!(
            entity
                .get_field_value("name")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_string(),
            Some(&name_sample)
        );
    }
    {
        let bin = entity
            .get_field_binary("phone")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("phone", &bin)?;
        let val = entity
            .get_field_value("phone")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_string(), Some(&phone_sample));
        entity.set_field_value("phone", DataValue::from_string(phone_sample.clone()))?;
        assert_eq!(
            entity
                .get_field_value("phone")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_string(),
            Some(&phone_sample)
        );
    }
    {
        let bin = entity
            .get_field_binary("email")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("email", &bin)?;
        let val = entity
            .get_field_value("email")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_string(), Some(&email_sample));
        entity.set_field_value("email", DataValue::from_string(email_sample.clone()))?;
        assert_eq!(
            entity
                .get_field_value("email")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_string(),
            Some(&email_sample)
        );
    }
    {
        let bin = entity
            .get_field_binary("password")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("password", &bin)?;
        let val = entity
            .get_field_value("password")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_string(), Some(&password_sample));
        entity.set_field_value("password", DataValue::from_string(password_sample.clone()))?;
        assert_eq!(
            entity
                .get_field_value("password")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_string(),
            Some(&password_sample)
        );
    }
    {
        let bin = entity
            .get_field_binary("created_at")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("created_at", &bin)?;
        let val = entity
            .get_field_value("created_at")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_i32(), Some(&created_at_sample));
        entity.set_field_value("created_at", DataValue::from_i32(created_at_sample))?;
        assert_eq!(
            entity
                .get_field_value("created_at")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_i32(),
            Some(&created_at_sample)
        );
    }
    {
        let bin = entity
            .get_field_binary("updated_at")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing binary"))?;
        entity.set_field_binary("updated_at", &bin)?;
        let val = entity
            .get_field_value("updated_at")?
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?;
        assert_eq!(val.as_i32(), Some(&updated_at_sample));
        entity.set_field_value("updated_at", DataValue::from_i32(updated_at_sample))?;
        assert_eq!(
            entity
                .get_field_value("updated_at")?
                .ok_or_else(|| mudu_error!(ErrorCode::InvalidArgument, "missing value"))?
                .as_i32(),
            Some(&updated_at_sample)
        );
    }

    Ok(())
}
