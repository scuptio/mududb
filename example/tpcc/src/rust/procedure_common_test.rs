//! Unit tests for shared TPC-C procedure helpers.

use crate::rust::procedure_common::{
    customer_name, district_name, item_name, order_status_text, require_positive,
    validate_order_lines, warehouse_name,
};

#[test]
fn require_positive_accepts_positive() {
    assert!(require_positive("count", 1).is_ok());
}

#[test]
fn require_positive_rejects_zero_and_negative() {
    assert!(
        require_positive("count", 0)
            .unwrap_err()
            .message()
            .contains("positive")
    );
    assert!(
        require_positive("count", -1)
            .unwrap_err()
            .message()
            .contains("positive")
    );
}

#[test]
fn customer_name_format() {
    let (first, last) = customer_name(1, 2, 3);
    assert_eq!(first, "Customer1_2_3");
    assert_eq!(last, "Last3");
}

#[test]
fn district_warehouse_and_item_name_formats() {
    assert_eq!(district_name(4, 5), "District4_5");
    assert_eq!(warehouse_name(6), "Warehouse6");
    assert_eq!(item_name(7), "Item7");
}

#[test]
fn validate_order_lines_checks_lengths_and_positivity() {
    assert!(
        validate_order_lines(&[], &[], &[])
            .unwrap_err()
            .message()
            .contains("at least one item")
    );

    assert!(
        validate_order_lines(&[1, 2], &[1], &[1])
            .unwrap_err()
            .message()
            .contains("length mismatch")
    );

    assert!(
        validate_order_lines(&[1], &[1], &[1, 2])
            .unwrap_err()
            .message()
            .contains("length mismatch")
    );

    assert!(
        validate_order_lines(&[0], &[1], &[1])
            .unwrap_err()
            .message()
            .contains("positive")
    );

    assert!(validate_order_lines(&[1], &[1], &[1]).is_ok());
}

#[test]
fn order_status_text_format() {
    let text = order_status_text(10, 2, 5, 100, true, "OK");
    assert!(text.contains("order=10"));
    assert!(text.contains("lines=2"));
    assert!(text.contains("qty=5"));
    assert!(text.contains("amount=100"));
    assert!(text.contains("all_local=true"));
    assert!(text.contains("status=OK"));
}
