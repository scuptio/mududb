//! Unit tests for the `rust::transactions::Transactions` entity.

use mududb::common::result::RS;
use mududb::contract::database::entity::Entity;
use mududb::contract::database::field_change::FieldChange;
use mududb::types::datum::{Datum, DatumDyn};

use crate::rust::transactions::object::{Transactions, TransactionsChange, columns};

fn sample_transactions() -> Transactions {
    Transactions::new(
        "trans_id_val".to_string(),
        Some("deposit".to_string()),
        Some(1),
        Some(2),
        Some(100),
        Some(1700000000),
    )
}

fn assert_same_transactions(lhs: &Transactions, rhs: &Transactions) {
    assert_eq!(lhs.trans_id, rhs.trans_id);
    assert_eq!(lhs.trans_type, rhs.trans_type);
    assert_eq!(lhs.from_user, rhs.from_user);
    assert_eq!(lhs.to_user, rhs.to_user);
    assert_eq!(lhs.amount, rhs.amount);
    assert_eq!(lhs.created_at, rhs.created_at);
}

/// Unwraps the `update_by_pk` result, failing the test on an empty change.
fn expect_update(change: &TransactionsChange, pk: String) -> RS<(String, Vec<Box<dyn DatumDyn>>)> {
    match change.update_by_pk(pk) {
        Some(parts) => Ok(parts),
        None => Err(mududb::mudu_error!(
            mududb::error::ErrorCode::InvalidState,
            "empty update"
        )),
    }
}

#[test]
fn transactions_lifecycle() -> RS<()> {
    let entity = sample_transactions();
    assert_eq!(entity.trans_id, "trans_id_val");
    assert_eq!(entity.amount, Some(100));

    // Entity metadata
    assert_eq!(Transactions::table_name(), "transactions");
    assert_eq!(Transactions::TABLE_NAME, "transactions");
    assert_eq!(Transactions::tuple_desc().fields().len(), 6);
    assert_eq!(
        Transactions::tuple_desc().fields()[0].name(),
        columns::TRANS_ID
    );
    assert!(Transactions::SQL_GET_BY_PK.contains(columns::TRANS_ID));
    assert!(Transactions::SQL_INSERT.contains("INSERT INTO transactions"));

    // Datum / DatumDyn metadata
    let data_type = Transactions::data_type();
    assert_eq!(entity.type_family()?, data_type.type_family());

    // Tuple roundtrip
    let tuple = entity.to_tuple()?;
    let from_tuple = Transactions::from_tuple(&tuple)?;
    assert_same_transactions(&from_tuple, &entity);

    // Value roundtrip
    let value = entity.to_value(&data_type)?;
    let from_value = Transactions::from_value(&value)?;
    assert_same_transactions(&from_value, &entity);

    // Binary roundtrip
    let binary = entity.to_binary(&data_type)?;
    let from_binary = Transactions::from_binary(binary.as_ref())?;
    assert_same_transactions(&from_binary, &entity);

    // Textual roundtrip
    let textual = entity.to_textual(&data_type)?;
    let from_textual = Transactions::from_textual(textual.as_str())?;
    assert_same_transactions(&from_textual, &entity);

    // Clone through DatumDyn
    let cloned: Box<dyn DatumDyn> = entity.clone_boxed();
    let cloned_value = cloned.to_value(&data_type)?;
    let from_cloned = Transactions::from_value(&cloned_value)?;
    assert_same_transactions(&from_cloned, &entity);

    // Insert parameters follow the column declaration order
    let params = entity.insert_params();
    assert_eq!(params.0, "trans_id_val");
    assert_eq!(params.1.as_deref(), Some("deposit"));
    assert_eq!(params.4, Some(100));

    // Partial update statement
    let update = TransactionsChange {
        trans_type: FieldChange::Set(Some("withdraw".to_string())),
        amount: FieldChange::Set(Some(50)),
        ..TransactionsChange::default()
    };
    let (sql, update_params) = expect_update(&update, "trans_id_val".to_string())?;
    assert_eq!(
        sql,
        "UPDATE transactions SET trans_type = ?, amount = ? WHERE trans_id = ?"
    );
    assert_eq!(update_params.len(), 3);
    assert!(TransactionsChange::default().is_empty());
    assert!(
        TransactionsChange::default()
            .update_by_pk("trans_id_val".to_string())
            .is_none()
    );

    // Set(None) on a nullable column emits `<col> = ?` with a NULL parameter
    let null_update = TransactionsChange {
        from_user: FieldChange::Set(None),
        ..TransactionsChange::default()
    };
    let (null_sql, null_params) = expect_update(&null_update, "trans_id_val".to_string())?;
    assert_eq!(
        null_sql,
        "UPDATE transactions SET from_user = ? WHERE trans_id = ?"
    );
    assert_eq!(null_params.len(), 2);
    assert!(null_params[0].to_value(&i32::data_type())?.is_null());

    Ok(())
}

#[test]
fn transactions_nullable_fields_roundtrip() -> RS<()> {
    let entity = Transactions::new("trans_id_val".to_string(), None, None, None, None, None);
    let tuple = entity.to_tuple()?;
    assert!(tuple.is_null(1));
    assert!(tuple.is_null(5));
    let restored = Transactions::from_tuple(&tuple)?;
    assert_same_transactions(&restored, &entity);

    let value = entity.to_tuple_value()?;
    assert!(value.values()[1].is_null());
    let restored = Transactions::from_tuple_value(&value)?;
    assert_same_transactions(&restored, &entity);
    Ok(())
}
