#[cfg(test)]
mod tests {
    use crate::common::_arb_de_en::_fuzz_decode_and_encode;
    use crate::common::update_delta::UpdateDelta;

    #[test]
    fn fuzz_decode_and_encode_round_trips_update_delta() {
        _fuzz_decode_and_encode::<UpdateDelta>(&[0u8; 32]);
    }

    #[test]
    fn fuzz_decode_and_encode_handles_short_input() {
        // Empty input causes T::arbitrary to fail and the loop breaks immediately.
        _fuzz_decode_and_encode::<UpdateDelta>(&[]);
    }
}
