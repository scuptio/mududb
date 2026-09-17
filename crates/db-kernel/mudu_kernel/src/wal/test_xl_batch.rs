#[cfg(test)]
mod _test {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::fuzz::_test_target::_test::_test_target;

    #[test]
    fn _de_en_x_l_batch() {
        _test_target("_de_en_x_l_batch");
    }
}
