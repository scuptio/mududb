#[cfg(test)]
mod tests {
    use mudu::common::result::RS;
    use mudu::common::xid::XID;
    use mudu_contract::procedure::procedure_param::ProcedureParam;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use mudu_macro::mudu_proc;


    #[mudu_proc]
    pub fn example(xid: XID, a: i32, b: i64, c: String) -> RS<(i64, String)> {
        Ok((a as i64 + b + 1, format!("c={}, {} function invoked", c, xid)))
    }

    #[mudu_proc]
    pub fn test_proc2(xid: XID, a: i32) -> RS<String> {
        Ok(format!("a {},  {} function invoked", a,  xid))
    }

    #[mudu_proc]
    pub fn example_vec(xid: XID, a: i32, b: Vec<i64>) -> RS<(Vec<i64>, String)> {
        Ok((vec![a as i64, (a + 1) as i64], format!("a {}, b:{:?}, {} function invoked", a, b, xid)))
    }

    #[test]
    fn test_mudu_macro2() {
        let param = ProcedureParam::from_tuple(
            1,
            (1i32, 3i64, "string".to_string()),
            &<(i32, i64, String)>::tuple_desc_static(
                &["p1".to_string(), "p2".to_string(), "p3".to_string()]
            )).unwrap();
        let result = mudu_inner_example(&param);
        println!("result {:?}", result)
    }


    #[test]
    fn test_mudu_macro_macro_mp2() {
        let proc_desc = mudu_proc_desc_example();
        let argv = ProcedureParam::from_datum_vec(0, &[&32i32, &64i64, &"s".to_string()], proc_desc.param_desc()).unwrap();
        let param = mudu_binding::procedure::procedure_invoke::serialize_param(argv).unwrap();
        let result_binary = mp2_example(param);
        let result = mudu_binding::procedure::procedure_invoke::deserialize_result(&result_binary);
        assert!(result.is_ok());
        println!("Test passed!, result {:?}", result);
    }
}