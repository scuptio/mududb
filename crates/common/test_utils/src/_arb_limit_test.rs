#[cfg(test)]
mod tests {
    use crate::_arb_limit::{
        _ARB_MAX_ARRAY_LEN, _ARB_MAX_DATUM_SIZE, _ARB_MAX_NAME_LEN, _ARB_MAX_STRING_LEN,
        _ARB_MAX_TUPLE_KEY_FIELD, _ARB_MAX_TUPLE_VALUE_FIELD,
    };

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn limits_are_non_zero() {
        assert!(_ARB_MAX_TUPLE_KEY_FIELD > 0);
        assert!(_ARB_MAX_TUPLE_VALUE_FIELD > 0);
        assert!(_ARB_MAX_DATUM_SIZE > 0);
        assert!(_ARB_MAX_NAME_LEN > 0);
        assert!(_ARB_MAX_STRING_LEN > 0);
        assert!(_ARB_MAX_ARRAY_LEN > 0);
    }
}
