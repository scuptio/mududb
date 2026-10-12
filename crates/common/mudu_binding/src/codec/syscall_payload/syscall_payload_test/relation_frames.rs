#![allow(clippy::unwrap_used)]

use super::*;

#[test]
fn relation_get_frames_roundtrip() {
    let frame = encode_relation_get_request(
        sample_oid(),
        "district",
        &[(1, &b"w1"[..]), (0, &b"d1"[..])],
        &[0, 1, 5],
    );
    check_request(
        MessageKind::RelationGet,
        &frame,
        decode_relation_get_request,
        |(oid, table, key, select)| {
            let key_refs = key
                .iter()
                .map(|(attr, datum)| (*attr, datum.as_slice()))
                .collect::<Vec<_>>();
            encode_relation_get_request(oid, &table, &key_refs, &select)
        },
    );
    let (oid, table, key, select) = decode_relation_get_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(table, "district");
    assert_eq!(key, vec![(1, b"w1".to_vec()), (0, b"d1".to_vec())]);
    assert_eq!(select, vec![0, 1, 5]);

    let row = vec![Some(b"d1".to_vec()), None, Some(b"42".to_vec())];
    let ok_frame = encode_relation_get_result(&Ok(Some(row.clone())));
    let err_frame = encode_relation_get_result(&Err(not_found_error()));
    check_result(
        MessageKind::RelationGet,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_relation_get_result,
        |v| encode_relation_get_result(&Ok(v)),
    );
    assert_eq!(decode_relation_get_result(&ok_frame).unwrap(), Some(row));
    let none_frame = encode_relation_get_result(&Ok(None));
    assert_eq!(decode_relation_get_result(&none_frame).unwrap(), None);
}

#[test]
fn relation_update_frames_roundtrip() {
    let frame = encode_relation_update_request(
        sample_oid(),
        "stock",
        &[(1, &b"w1"[..]), (0, &b"i2"[..])],
        &[(2, &b"q"[..])],
        &[(3, 0, &b"7"[..]), (4, 1, &b"1"[..])],
    );
    let (oid, table, key, values, deltas) = decode_relation_update_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(table, "stock");
    assert_eq!(key, vec![(1, b"w1".to_vec()), (0, b"i2".to_vec())]);
    assert_eq!(values, vec![(2, b"q".to_vec())]);
    assert_eq!(deltas, vec![(3, 0, b"7".to_vec()), (4, 1, b"1".to_vec())]);

    let ok_frame = encode_relation_update_result(&Ok(1));
    let err_frame = encode_relation_update_result(&Err(not_found_error()));
    check_result(
        MessageKind::RelationUpdate,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_relation_update_result,
        |v| encode_relation_update_result(&Ok(v)),
    );
    assert_eq!(decode_relation_update_result(&ok_frame).unwrap(), 1);
}

#[test]
fn relation_insert_frames_roundtrip() {
    let frame = encode_relation_insert_request(
        sample_oid(),
        "new_order",
        &[(2, &b"w1"[..]), (1, &b"d1"[..]), (0, &b"o1"[..])],
        &[],
    );
    let (oid, table, key, values) = decode_relation_insert_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(table, "new_order");
    assert_eq!(key.len(), 3);
    assert!(values.is_empty());
    check_unit_result(
        MessageKind::RelationInsert,
        encode_relation_insert_result,
        decode_relation_insert_result,
        ErrorCode::EntityAlreadyExists,
    );
}
