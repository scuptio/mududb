#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use crate::procedure::procedure_param::ProcedureParam;
    use crate::tuple::tuple_datum::TupleDatum;
    use crate::tuple::typed_bin::TypedBin;
    use mudu::common::id::OID;
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dat_value::DatValue;
    use mudu_type::datum::DatumDyn;

    fn sample_oid() -> OID {
        42
    }

    fn sample_values() -> Vec<DatValue> {
        vec![DatValue::from_i32(1), DatValue::from_i64(2)]
    }

    #[test]
    fn new_and_accessors() {
        let p = ProcedureParam::new(sample_oid(), 7, sample_values());
        assert_eq!(p.session_id(), sample_oid());
        assert_eq!(p.procedure_id(), 7);
        assert_eq!(p.param_list().len(), 2);
        assert_eq!(p.param_list()[0].to_i32(), 1);
        assert_eq!(p.param_list()[1].to_i64(), 2);
    }

    #[test]
    fn set_session_id() {
        let mut p = ProcedureParam::new(sample_oid(), 0, sample_values());
        p.set_session_id(100);
        assert_eq!(p.session_id(), 100);
    }

    #[test]
    fn into_returns_inner_values() {
        let p = ProcedureParam::new(sample_oid(), 7, sample_values());
        let (xid, pid, list) = p.into();
        assert_eq!(xid, sample_oid());
        assert_eq!(pid, 7);
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn from_tuple() {
        let tuple = (1i32, 2i64);
        let desc = <(i32, i64) as TupleDatum>::tuple_desc_static(&[]);
        let p = ProcedureParam::from_tuple(sample_oid(), tuple, &desc).unwrap();
        assert_eq!(p.session_id(), sample_oid());
        assert_eq!(p.procedure_id(), 0);
        assert_eq!(p.param_list().len(), 2);
        assert_eq!(p.param_list()[0].to_i32(), 1);
        assert_eq!(p.param_list()[1].to_i64(), 2);
    }

    #[test]
    fn from_datum_vec() {
        let desc = <(i32, i64) as TupleDatum>::tuple_desc_static(&[]);
        let bin_i32 = TypedBin::new(DatTypeID::I32, vec![0, 0, 0, 1]);
        let bin_i64 = TypedBin::new(DatTypeID::I64, vec![0, 0, 0, 0, 0, 0, 0, 2]);
        let argv: Vec<&dyn DatumDyn> = vec![&bin_i32, &bin_i64];
        let p = ProcedureParam::from_datum_vec(sample_oid(), &argv, &desc).unwrap();
        assert_eq!(p.session_id(), sample_oid());
        assert_eq!(p.procedure_id(), 0);
        assert_eq!(p.param_list().len(), 2);
        assert_eq!(p.param_list()[0].to_i32(), 1);
        assert_eq!(p.param_list()[1].to_i64(), 2);
    }

    #[test]
    fn debug_is_available() {
        let p = ProcedureParam::new(sample_oid(), 7, sample_values());
        let s = format!("{:?}", p);
        assert!(!s.is_empty());
    }
}
