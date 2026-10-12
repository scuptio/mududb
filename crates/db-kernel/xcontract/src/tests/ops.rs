#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::{block_on, datum, key_row, make_contract_with_table, test_schema, value_row};
use crate::*;

#[test]
fn xcontract_delete_removes_existing_row() {
    block_on(async move {
        let schema = test_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

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

        let delete_tx = contract.begin_tx().await?;
        let deleted = contract
            .delete(
                delete_tx.clone(),
                table_id,
                &key_row(1),
                &Predicate::CNF(Vec::new()),
                &OptDelete::default(),
            )
            .await?;
        assert_eq!(deleted, 1);
        contract.commit_tx(delete_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let row = contract
            .read_key(
                read_tx,
                table_id,
                &key_row(1),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await?;
        assert_eq!(row, None);
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn xcontract_delete_missing_row_returns_zero() {
    block_on(async move {
        let schema = test_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        let tx = contract.begin_tx().await?;
        let deleted = contract
            .delete(
                tx.clone(),
                table_id,
                &key_row(99),
                &Predicate::CNF(Vec::new()),
                &OptDelete::default(),
            )
            .await?;
        assert_eq!(deleted, 0);
        contract.abort_tx(tx).await?;
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn xcontract_read_range_returns_matching_rows() {
    block_on(async move {
        let schema = test_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        let insert_tx = contract.begin_tx().await?;
        for (k, v) in [(1, 10), (2, 20), (3, 30)] {
            contract
                .insert(
                    insert_tx.clone(),
                    table_id,
                    &key_row(k),
                    &value_row(v),
                    &OptInsert::default(),
                )
                .await?;
        }
        contract.commit_tx(insert_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let cursor = contract
            .read_range(
                read_tx.clone(),
                table_id,
                &RangeData::new(
                    std::ops::Bound::Included(vec![(0, datum(1))]),
                    std::ops::Bound::Included(vec![(0, datum(3))]),
                ),
                &Predicate::CNF(Vec::new()),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await?;

        let mut rows = Vec::new();
        while let Some(row) = cursor.next().await? {
            rows.push(row);
        }
        assert_eq!(rows.len(), 3);
        contract.abort_tx(read_tx).await?;
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}

#[test]
fn xcontract_read_range_with_key_prefix_eq_filters_rows() {
    block_on(async move {
        let schema = test_schema();
        let (contract, table_id) = make_contract_with_table(&schema).await?;

        let insert_tx = contract.begin_tx().await?;
        for (k, v) in [(1, 10), (2, 20), (3, 30)] {
            contract
                .insert(
                    insert_tx.clone(),
                    table_id,
                    &key_row(k),
                    &value_row(v),
                    &OptInsert::default(),
                )
                .await?;
        }
        contract.commit_tx(insert_tx).await?;

        let read_tx = contract.begin_tx().await?;
        let cursor = contract
            .read_range(
                read_tx.clone(),
                table_id,
                &RangeData::new(
                    std::ops::Bound::Included(vec![(0, datum(1))]),
                    std::ops::Bound::Included(vec![(0, datum(3))]),
                ),
                &Predicate::KeyPrefixEq(vec![(0, datum(2))]),
                &VecSelTerm::new(vec![1]),
                &OptRead::default(),
            )
            .await?;

        let mut rows = Vec::new();
        while let Some(row) = cursor.next().await? {
            rows.push(row);
        }
        assert_eq!(rows.len(), 1);
        contract.abort_tx(read_tx).await?;
        Ok::<(), mudu::error::MuduError>(())
    })
    .unwrap();
}
