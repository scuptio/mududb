//! Unit tests for the `rust::wallets::Wallets` entity.

use mududb::common::result::RS;
use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::wallets::object::{Wallets, WalletsChange, columns};

fn sample_wallets() -> Wallets {
    Wallets::new(1, Some(100), Some(1700000000))
}

fn assert_same_wallets(lhs: &Wallets, rhs: &Wallets) {
    assert_eq!(lhs.user_id, rhs.user_id);
    assert_eq!(lhs.balance, rhs.balance);
    assert_eq!(lhs.updated_at, rhs.updated_at);
}

/// Unwraps the `update_by_pk` result, failing the test on an empty change.
fn expect_update(change: &WalletsChange, pk: i32) -> RS<(String, Vec<Box<dyn DatumDyn>>)> {
    match change.update_by_pk(pk) {
        Some(parts) => Ok(parts),
        None => Err(mududb::mudu_error!(
            mududb::error::ErrorCode::InvalidState,
            "empty update"
        )),
    }
}

#[test]
fn wallets_lifecycle() -> RS<()> {
    let entity = sample_wallets();
    assert_eq!(entity.user_id, 1);
    assert_eq!(entity.balance, Some(100));

    // Entity metadata
    assert_eq!(Wallets::table_name(), "wallets");
    assert_eq!(Wallets::TABLE_NAME, "wallets");
    assert_eq!(Wallets::tuple_desc().fields().len(), 3);
    assert_eq!(Wallets::tuple_desc().fields()[0].name(), columns::USER_ID);
    assert!(Wallets::SQL_GET_BY_PK.contains(columns::USER_ID));
    assert!(Wallets::SQL_INSERT.contains("INSERT INTO wallets"));

    // Datum / DatumDyn metadata
    let data_type = Wallets::data_type();
    assert_eq!(entity.type_family()?, data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple()?;
    let from_tuple = Wallets::from_tuple(&tuple)?;
    assert_same_wallets(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type)?;
    let from_value = Wallets::from_value(&value)?;
    assert_same_wallets(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type)?;
    let from_binary = Wallets::from_binary(binary.as_ref())?;
    assert_same_wallets(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type)?;
    let from_textual = Wallets::from_textual(textual.as_str())?;
    assert_same_wallets(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type)?;
    let from_cloned = Wallets::from_value(&cloned_value)?;
    assert_same_wallets(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, 1);
    assert_eq!(params.1, Some(100));
    assert_eq!(params.2, Some(1700000000));

    // Partial update statement
    let update = WalletsChange {
        balance: FieldChange::Set(Some(200)),
        updated_at: FieldChange::Set(Some(1700000001)),
    };
    let (sql, update_params) = expect_update(&update, 1)?;
    assert_eq!(
        sql,
        "UPDATE wallets SET balance = ?, updated_at = ? WHERE user_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(WalletsChange::default().is_empty());
    assert!(WalletsChange::default().update_by_pk(1).is_none());

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = WalletsChange {
        balance: FieldChange::Set(None),
        ..WalletsChange::default()
    };
    let (null_sql, null_params) = expect_update(&null_update, 1)?;
    assert_eq!(null_sql, "UPDATE wallets SET balance = ? WHERE user_id = ?");
    assert_eq!(null_params.len(), 2);
    assert!(null_params[0].to_value(&i32::data_type())?.is_null());

    Ok(())
}

#[test]
fn wallets_nullable_fields_roundtrip() -> RS<()> {
    let entity = Wallets::new(2, None, None);
    let tuple = entity.to_tuple()?;
    assert!(tuple.is_null(1));
    assert!(tuple.is_null(2));
    let restored = Wallets::from_tuple(&tuple)?;
    assert_same_wallets(&restored, &entity);

    let value = entity.to_tuple_value()?;
    assert!(value.values()[1].is_null());
    let restored = Wallets::from_tuple_value(&value)?;
    assert_same_wallets(&restored, &entity);
    Ok(())
}
