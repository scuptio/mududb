pub mod _fuzz {
    use mudu::common::_arb_de_en::_fuzz_decode_and_encode;
    use mudu::common::update_delta::UpdateDelta;

    pub fn _de_en_x_l_up_tuple(data: &[u8]) {
        _fuzz_decode_and_encode::<UpdateDelta>(data);
    }

    pub fn _apply_compensate_x_l_up_tuple(_data: &[u8]) {}
}

#[cfg(test)]
mod _test {
    use crate::fuzz::_test_target::_test::_test_target;

    #[test]
    fn test_schema_table() {
        _test_target("_de_en_x_l_up_tuple");
    }
}
