pub mod _fuzz {
    use arbitrary::{Arbitrary, Unstructured};
    use mudu::common::buf::Buf;
    use mudu::common::update_delta::UpdateDelta;
    use test_utils::_arbitrary::{_arbitrary_data, _arbitrary_vec_n};

    struct TupleAndDelta {
        tuple: Buf,
        delta: Vec<UpdateDelta>,
    }

    const MIN_TUPLE_LEN: usize = 8;
    const MAX_TUPLE_LEN: usize = 100;
    const MAX_DELTA: usize = 16;

    impl Arbitrary<'_> for TupleAndDelta {
        fn arbitrary(u: &mut Unstructured) -> arbitrary::Result<Self> {
            let mut tuple_len = (u32::arbitrary(u)? as usize) % MAX_TUPLE_LEN;
            if tuple_len < MIN_TUPLE_LEN {
                tuple_len = MIN_TUPLE_LEN;
            }
            let tuple: Buf = _arbitrary_vec_n(u, tuple_len)?;
            assert_eq!(tuple.len(), tuple_len);

            let num_delta = (u8::arbitrary(u)? as usize) % MAX_DELTA;
            let mut delta = vec![];
            for _i in 0..num_delta {
                UpdateDelta::arb_set_tuple_max_len(tuple_len);
                let d = UpdateDelta::arbitrary(u)?;
                assert!(tuple_len >= (d.offset() + d.to_up_size()) as usize);
                tuple_len = tuple_len + d.delta().len() - d.to_up_size() as usize;
                delta.push(d)
            }

            Ok(Self { tuple, delta })
        }
    }

    pub fn _fuzz_delta_apply(d: &[u8]) {
        let tuple_and_delta = _arbitrary_data::<TupleAndDelta>(d);
        for t in tuple_and_delta {
            test_apply(t.tuple, t.delta)
        }
    }

    fn test_apply(tuple: Buf, delta: Vec<UpdateDelta>) {
        let mut undo = vec![];
        let mut tuple_buf = tuple.clone();
        for up in delta.iter() {
            let undo_up = up.apply_to(&mut tuple_buf);
            undo.push(undo_up);
        }
        for up in undo.iter().rev() {
            let _ = up.apply_to(&mut tuple_buf);
        }
        assert_eq!(tuple, tuple_buf);
    }
}

#[cfg(test)]
mod __test {
    use crate::fuzz::_test_target::_test::_test_target;
    #[test]
    fn test_schema_table() {
        _test_target("_delta_apply");
    }
}
