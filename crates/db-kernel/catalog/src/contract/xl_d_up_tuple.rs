/// Fuzz targets for update-delta encode/decode.
#[cfg(any(test, feature = "test", fuzzing))]
pub mod _fuzz {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use mudu::common::_arb_de_en::_fuzz_decode_and_encode;
    use mudu::common::update_delta::UpdateDelta;

    /// Fuzz target: decode-then-encode round-trip of an update delta.
    pub fn _de_en_x_l_up_tuple(data: &[u8]) {
        _fuzz_decode_and_encode::<UpdateDelta>(data);
    }

    /// Fuzz target placeholder for apply/compensate of an update delta (not
    /// yet exercised).
    pub fn _apply_compensate_x_l_up_tuple(_data: &[u8]) {}
}
