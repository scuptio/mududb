#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{
    block_on, datum, datum_string, key_row, make_contract_with_table, meta_table, test_schema,
    value_row, wallet_users_schema,
};
use crate::utils::build_value_tuple;
use crate::*;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;

#[test]
fn xcontract_insert_and_read_nullable_value_column() {
    block_on(async move {
        let meta_mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let table_id = schema.id();
        let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

        let ddl = contract.begin_tx().await?;
        contract.create_table(ddl.clone(), &schema).await?;
        contract.commit_tx(ddl).await?;

        let tx = contract.begin_tx().await?;
        contract
            .insert(
                tx.clone(),
                table_id,
                &key_row(7),
                &VecDatum::new(Vec::new()),
                &OptInsert::default(),
            )
            .await?;
        contract.commit_tx(tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(7),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(row, Some(vec![None]));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn build_value_tuple_supports_partial_insert_with_mixed_types() {
    let schema = wallet_users_schema();
    let desc = meta_table(&schema).unwrap();
    let input = VecDatum::new(vec![
        (1, datum_string("Alice")),
        (2, datum_string("12345678")),
        (3, datum_string("alice@xxx.com")),
        (4, datum_string("aaa")),
        (5, datum(0)),
    ]);
    let tuple = build_value_tuple(&input, &desc).unwrap();
    assert!(!tuple.is_empty());
}

#[test]
fn xcontract_update_multiple_varchar_columns_preserves_tuple() {
    block_on(async move {
        let schema = wallet_users_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        let insert_tx = contract.begin_tx().await?;
        contract
            .insert(
                insert_tx.clone(),
                table_id,
                &key_row(7),
                &VecDatum::new(vec![
                    (1, datum_string("Al")),
                    (2, datum_string("1")),
                    (3, datum_string("a@b")),
                    (4, datum_string("pw")),
                    (5, datum(100)),
                    (6, datum(200)),
                ]),
                &OptInsert::default(),
            )
            .await?;
        contract.commit_tx(insert_tx).await?;

        // One update mixing longer, same-length, and shorter VARCHAR
        // assignments plus a fixed-length column.
        let update_tx = contract.begin_tx().await?;
        let updated = contract
            .update(
                update_tx.clone(),
                table_id,
                &key_row(7),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![
                    (1, datum_string("Alice")),
                    (2, datum_string("2")),
                    (3, datum_string("ab")),
                    (4, datum_string("password-123")),
                    (6, datum(201)),
                ]),
                &OptUpdate::default(),
            )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(7),
                &VecSelTerm::new(vec![1, 2, 3, 4, 5, 6]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(
            row,
            Some(vec![
                Some(datum_string("Alice")),
                Some(datum_string("2")),
                Some(datum_string("ab")),
                Some(datum_string("password-123")),
                Some(datum(100)),
                Some(datum(201)),
            ])
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn xcontract_update_null_varchar_column_rebuilds_tuple() {
    block_on(async move {
        let schema = wallet_users_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        // Partial-column insert leaves phone/email/password NULL.
        let insert_tx = contract.begin_tx().await?;
        contract
            .insert(
                insert_tx.clone(),
                table_id,
                &key_row(9),
                &VecDatum::new(vec![
                    (1, datum_string("Bob")),
                    (5, datum(10)),
                    (6, datum(20)),
                ]),
                &OptInsert::default(),
            )
            .await?;
        contract.commit_tx(insert_tx).await?;

        // Writing a currently-NULL VARCHAR column must materialize its
        // payload and clear the null bit without disturbing siblings.
        let update_tx = contract.begin_tx().await?;
        let updated = contract
            .update(
                update_tx.clone(),
                table_id,
                &key_row(9),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![
                    (1, datum_string("Bobby")),
                    (3, datum_string("bob@example.com")),
                ]),
                &OptUpdate::default(),
            )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(9),
                &VecSelTerm::new(vec![1, 2, 3, 4, 5, 6]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(
            row,
            Some(vec![
                Some(datum_string("Bobby")),
                None,
                Some(datum_string("bob@example.com")),
                None,
                Some(datum(10)),
                Some(datum(20)),
            ])
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn xcontract_update_set_to_null_and_back_rebuilds_tuple() {
    block_on(async move {
        let schema = wallet_users_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        let insert_tx = contract.begin_tx().await?;
        contract
            .insert(
                insert_tx.clone(),
                table_id,
                &key_row(3),
                &VecDatum::new(vec![
                    (1, datum_string("Carol")),
                    (5, datum(100)),
                    (6, datum(200)),
                ]),
                &OptInsert::default(),
            )
            .await?;
        contract.commit_tx(insert_tx).await?;

        // `SET name = NULL, created_at = NULL`: the NULL assignments must
        // set the null bitmap bits through the whole-tuple rebuild path.
        let update_tx = contract.begin_tx().await?;
        let updated = contract
            .update(
                update_tx.clone(),
                table_id,
                &key_row(3),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &OptUpdate {
                    delta_assignments: Vec::new(),
                    null_assignments: vec![1, 5],
                },
            )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(3),
                &VecSelTerm::new(vec![1, 5, 6]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(row, Some(vec![None, None, Some(datum(200))]));

        // Setting the columns back to values (the other direction through
        // the rebuild path) clears the null bits again.
        let update_tx = contract.begin_tx().await?;
        let updated = contract
            .update(
                update_tx.clone(),
                table_id,
                &key_row(3),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![(1, datum_string("Caroline")), (5, datum(101))]),
                &OptUpdate::default(),
            )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(3),
                &VecSelTerm::new(vec![1, 5, 6]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(
            row,
            Some(vec![
                Some(datum_string("Caroline")),
                Some(datum(101)),
                Some(datum(200)),
            ])
        );
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn xcontract_delta_over_null_column_evaluates_to_null() {
    block_on(async move {
        let schema = wallet_users_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        let insert_tx = contract.begin_tx().await?;
        contract
            .insert(
                insert_tx.clone(),
                table_id,
                &key_row(4),
                &VecDatum::new(vec![(5, datum(100)), (6, datum(200))]),
                &OptInsert::default(),
            )
            .await?;
        contract.commit_tx(insert_tx).await?;

        // NULL out the INT column first.
        let update_tx = contract.begin_tx().await?;
        contract
            .update(
                update_tx.clone(),
                table_id,
                &key_row(4),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &OptUpdate {
                    delta_assignments: Vec::new(),
                    null_assignments: vec![5],
                },
            )
            .await?;
        contract.commit_tx(update_tx).await?;

        // `SET created_at = created_at + 1` over the NULL column: per SQL
        // semantics the result stays NULL (and the update lands on the
        // rebuild path instead of failing).
        let update_tx = contract.begin_tx().await?;
        let updated = contract
            .update(
                update_tx.clone(),
                table_id,
                &key_row(4),
                &Predicate::CNF(vec![]),
                &VecDatum::new(vec![]),
                &OptUpdate {
                    delta_assignments: vec![DeltaAssign {
                        attr: 5,
                        op: DeltaOp::Add,
                        literal: datum(1),
                    }],
                    null_assignments: Vec::new(),
                },
            )
            .await?;
        assert_eq!(updated, 1);
        contract.commit_tx(update_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(4),
                &VecSelTerm::new(vec![5, 6]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(row, Some(vec![None, Some(datum(200))]));
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn iouring_xcontract_update_maps_table_attr_to_value_tuple_index() {
    block_on(async move {
        let r = _iouring_xcontract_update_maps_table_attr_to_value_tuple_index().await;
        assert!(r.is_ok())
    })
}

async fn _iouring_xcontract_update_maps_table_attr_to_value_tuple_index() -> RS<()> {
    let meta_mgr = Arc::new(TestMetaMgr::new());
    let schema = test_schema();
    let table_id = schema.id();
    let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

    let ddl_tx = contract.begin_tx().await?;
    contract.create_table(ddl_tx.clone(), &schema).await?;
    contract.commit_tx(ddl_tx).await?;

    let insert_tx = contract.begin_tx().await?;
    let insert_key = key_row(1);
    let insert_value = value_row(10);
    let opt_insert = OptInsert::default();
    contract
        .insert(
            insert_tx.clone(),
            table_id,
            &insert_key,
            &insert_value,
            &opt_insert,
        )
        .await?;
    contract.commit_tx(insert_tx).await?;

    let update_tx = contract.begin_tx().await?;
    let update_key = key_row(1);
    let pred_non_key = Predicate::CNF(vec![]);
    let update_value = value_row(20);
    let updated = contract
        .update(
            update_tx.clone(),
            table_id,
            &update_key,
            &pred_non_key,
            &update_value,
            &OptUpdate::default(),
        )
        .await?;
    assert_eq!(updated, 1);
    contract.commit_tx(update_tx).await?;

    let read_tx = contract.begin_tx().await?;
    let read_key = key_row(1);
    let select = VecSelTerm::new(vec![1]);
    let opt_read = OptRead::default();
    let relation = contract
        .read_key(read_tx, table_id, &read_key, &select, &opt_read)
        .await?;
    assert_eq!(relation, Some(vec![Some(datum(20))]));
    Ok(())
}

fn plus_one_update() -> OptUpdate {
    OptUpdate {
        delta_assignments: vec![DeltaAssign {
            attr: 1,
            op: DeltaOp::Add,
            literal: datum(1),
        }],
        null_assignments: Vec::new(),
    }
}

/// Build a contract with table `t` created and key 1 seeded with v = 10.
async fn contract_with_seeded_row() -> RS<(WorkerXContract, OID)> {
    let meta_mgr = Arc::new(TestMetaMgr::new());
    let schema = test_schema();
    let table_id = schema.id();
    let contract = WorkerXContract::with_log(meta_mgr, None).unwrap();

    let ddl_tx = contract.begin_tx().await?;
    contract.create_table(ddl_tx.clone(), &schema).await?;
    contract.commit_tx(ddl_tx).await?;

    let insert_tx = contract.begin_tx().await?;
    contract
        .insert(
            insert_tx.clone(),
            table_id,
            &key_row(1),
            &value_row(10),
            &OptInsert::default(),
        )
        .await?;
    contract.commit_tx(insert_tx).await?;
    Ok((contract, table_id))
}

/// Point-read column `v` of key 1 under `tx`.
async fn read_v_row(
    contract: &WorkerXContract,
    table_id: OID,
    tx: &Arc<dyn TxMgr>,
) -> RS<Option<Vec<Option<DataBin>>>> {
    contract
        .read_key(
            tx.clone(),
            table_id,
            &key_row(1),
            &VecSelTerm::new(vec![1]),
            &OptRead::default(),
        )
        .await
}

/// Apply a pure delta update to key 1 (no absolute values) under `tx`.
async fn apply_delta_update(
    contract: &WorkerXContract,
    table_id: OID,
    tx: &Arc<dyn TxMgr>,
    opt_update: &OptUpdate,
) -> RS<usize> {
    contract
        .update(
            tx.clone(),
            table_id,
            &key_row(1),
            &Predicate::CNF(vec![]),
            &VecDatum::new(vec![]),
            opt_update,
        )
        .await
}

#[test]
fn iouring_xcontract_delta_update_evaluates_on_latest_committed_value() {
    block_on(async move {
        let r = _iouring_xcontract_delta_update_evaluates_on_latest_committed_value().await;
        assert!(r.is_ok())
    })
}

async fn _iouring_xcontract_delta_update_evaluates_on_latest_committed_value() -> RS<()> {
    let (contract, table_id) = contract_with_seeded_row().await?;

    // A plain read observes v = 10 (no lock held).
    let stale_tx = contract.begin_tx().await?;
    let row = read_v_row(&contract, table_id, &stale_tx).await?;
    assert_eq!(row, Some(vec![Some(datum(10))]));

    // Another transaction commits `v = v + 1` in the meantime.
    let tx2 = contract.begin_tx().await?;
    let updated = apply_delta_update(&contract, table_id, &tx2, &plus_one_update()).await?;
    assert_eq!(updated, 1);
    contract.commit_tx(tx2).await?;

    // The stale reader's `v = v + 1` is evaluated on the latest committed
    // value (11) under the statement lock, yielding 12. An absolute
    // update computed from the stale read would have written 11 and lost
    // the concurrent increment.
    let updated = apply_delta_update(&contract, table_id, &stale_tx, &plus_one_update()).await?;
    assert_eq!(updated, 1);
    // Read-your-writes: the same transaction observes its staged delta.
    let row = read_v_row(&contract, table_id, &stale_tx).await?;
    assert_eq!(row, Some(vec![Some(datum(12))]));
    contract.commit_tx(stale_tx).await?;

    // Subtraction form: `v = v - 2` on the committed 12 yields 10.
    let tx3 = contract.begin_tx().await?;
    let minus_two = OptUpdate {
        delta_assignments: vec![DeltaAssign {
            attr: 1,
            op: DeltaOp::Sub,
            literal: datum(2),
        }],
        null_assignments: Vec::new(),
    };
    let updated = apply_delta_update(&contract, table_id, &tx3, &minus_two).await?;
    assert_eq!(updated, 1);
    contract.commit_tx(tx3).await?;

    let read_tx = contract.begin_tx().await?;
    let row = read_v_row(&contract, table_id, &read_tx).await?;
    assert_eq!(row, Some(vec![Some(datum(10))]));
    Ok(())
}
