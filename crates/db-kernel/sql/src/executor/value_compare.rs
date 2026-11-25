//! Comparison of decoded datum values for the executor layer.
//!
//! Used by the residual filter executor and by MIN/MAX aggregation. Both
//! operands are decoded with the same column `DataType`, so only same-kind
//! comparisons are supported.

use mudu::common::result::RS;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_type::data_value::DataValue;
use std::cmp::Ordering;

/// Compare two decoded values of the same type.
///
/// Returns `None` when either side is NULL, following SQL three-valued
/// logic (the comparison result is UNKNOWN).
pub(crate) fn compare_values(left: &DataValue, right: &DataValue) -> RS<Option<Ordering>> {
    if left.is_null() || right.is_null() {
        return Ok(None);
    }
    let ordering = if let (Some(l), Some(r)) = (left.as_i32(), right.as_i32()) {
        l.cmp(r)
    } else if let (Some(l), Some(r)) = (left.as_i64(), right.as_i64()) {
        l.cmp(r)
    } else if let (Some(l), Some(r)) = (left.as_f32(), right.as_f32()) {
        l.partial_cmp(r).unwrap_or(Ordering::Equal)
    } else if let (Some(l), Some(r)) = (left.as_f64(), right.as_f64()) {
        l.partial_cmp(r).unwrap_or(Ordering::Equal)
    } else if let (Some(l), Some(r)) = (left.as_numeric(), right.as_numeric()) {
        l.as_bigdecimal().cmp(r.as_bigdecimal())
    } else if let (Some(l), Some(r)) = (left.as_string(), right.as_string()) {
        l.cmp(r)
    } else if let (Some(l), Some(r)) = (left.as_binary(), right.as_binary()) {
        l.cmp(r)
    } else {
        return Err(mudu_error!(
            ER::NotImplemented,
            "comparison between these datum kinds is not supported"
        ));
    };
    Ok(Some(ordering))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use mudu::data_type::numeric::Numeric;

    #[test]
    fn compare_values_covers_all_kinds_null_and_mismatch() {
        // NULL on either side is UNKNOWN (SQL three-valued logic).
        assert_eq!(
            compare_values(&DataValue::null(), &DataValue::from_i32(1)).unwrap(),
            None
        );
        assert_eq!(
            compare_values(&DataValue::from_i32(1), &DataValue::from_i32(2)).unwrap(),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_values(&DataValue::from_i64(3), &DataValue::from_i64(2)).unwrap(),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_values(&DataValue::from_f32(1.0), &DataValue::from_f32(1.0)).unwrap(),
            Some(Ordering::Equal)
        );
        assert_eq!(
            compare_values(&DataValue::from_f64(2.0), &DataValue::from_f64(1.0)).unwrap(),
            Some(Ordering::Greater)
        );
        assert_eq!(
            compare_values(
                &DataValue::from_numeric(Numeric::parse("1.5").unwrap()),
                &DataValue::from_numeric(Numeric::parse("2.5").unwrap())
            )
            .unwrap(),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_values(
                &DataValue::from_string("a".to_string()),
                &DataValue::from_string("b".to_string())
            )
            .unwrap(),
            Some(Ordering::Less)
        );
        assert_eq!(
            compare_values(
                &DataValue::from_binary(vec![2]),
                &DataValue::from_binary(vec![1])
            )
            .unwrap(),
            Some(Ordering::Greater)
        );
        // Mismatched kinds are rejected.
        assert!(compare_values(
            &DataValue::from_i32(1),
            &DataValue::from_string("x".to_string())
        )
        .is_err());
    }
}
