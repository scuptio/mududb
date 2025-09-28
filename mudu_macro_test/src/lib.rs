#[cfg(test)]
mod tests {
    use mudu::common::result::RS;
    use mudu::common::serde_utils::{deserialize_sized_from, serialize_sized_to_vec};
    use mudu::common::xid::XID;
    use mudu::procedure::proc_param::ProcParam;
    use mudu::procedure::proc_result::ProcResult;
    use mudu_macro::mudu_proc;

    #[mudu_proc]
    pub fn example(xid: XID, a: i32, b: i64, c: String) -> RS<(i64, String)> {
        Ok((a as i64 + b + 1, format!("c={}, {} function invoked", c, xid)))
    }

    #[test]
    fn test_mudu_macro_macro() {
        let argv_desc = mudu_proc_argv_desc_example();
        let result_desc = mudu_proc_result_desc_example();
        let argv = ProcParam::from_datum_vec(0, &[&32i32, &64i64, &"s".to_string()], argv_desc).unwrap();
        let mut output_buf = vec![0u8; 10240usize];
        let input_buf = serialize_sized_to_vec(&argv).unwrap();
        let r = mudu_proc_example(input_buf.as_ptr(), input_buf.len(), output_buf.as_mut_ptr(), output_buf.len());
        if r == 0 {
            let (result, _n): (ProcResult, _) = deserialize_sized_from(&output_buf).unwrap();
            let r = result.to::<(i64, String)>(result_desc).unwrap();
            println!("result {:?}", r);
        }

        println!("Test passed!");
    }
}

