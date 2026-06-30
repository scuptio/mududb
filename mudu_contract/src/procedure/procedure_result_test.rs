#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::procedure::procedure_result::ProcedureResult;
    use crate::tuple::tuple_datum::TupleDatum;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_type::dat_value::DatValue;

    fn sample_values() -> Vec<DatValue> {
        vec![DatValue::from_i32(1), DatValue::from_i64(2)]
    }

    #[test]
    fn new_and_return_list() {
        let r = ProcedureResult::new(sample_values());
        assert_eq!(r.return_list().len(), 2);
        assert_eq!(r.return_list()[0].to_i32(), 1);
        assert_eq!(r.return_list()[1].to_i64(), 2);
    }

    #[test]
    fn into_returns_inner_vec() {
        let r = ProcedureResult::new(sample_values());
        let list = r.into();
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn from_ok_tuple() {
        let tuple = (1i32, 2i64);
        let desc = <(i32, i64) as TupleDatum>::tuple_desc_static(&[]);
        let r = ProcedureResult::from(Ok(tuple), &desc).unwrap();
        assert_eq!(r.return_list().len(), 2);
        assert_eq!(r.return_list()[0].to_i32(), 1);
        assert_eq!(r.return_list()[1].to_i64(), 2);
    }

    #[test]
    fn from_err_tuple() {
        let desc = <(i32, i64) as TupleDatum>::tuple_desc_static(&[]);
        let err = mudu_error!(ErrorCode::Parse, "test error");
        let result = ProcedureResult::from(Err::<(i32, i64), _>(err), &desc);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().ec(), ErrorCode::Parse);
    }

    #[test]
    fn to_tuple() {
        let desc = <(i32, i64) as TupleDatum>::tuple_desc_static(&[]);
        let tuple = (1i32, 2i64);
        let r = ProcedureResult::from(Ok(tuple), &desc).unwrap();
        let decoded: (i32, i64) = r.to(&desc).unwrap();
        assert_eq!(decoded, tuple);
    }

    #[test]
    fn clone_is_available() {
        let r = ProcedureResult::new(sample_values());
        let cloned = r.clone();
        assert_eq!(cloned.return_list().len(), 2);
    }

    #[test]
    fn debug_is_available() {
        let r = ProcedureResult::new(sample_values());
        let s = format!("{:?}", r);
        assert!(!s.is_empty());
    }
}
