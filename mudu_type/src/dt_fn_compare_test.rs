#[cfg(test)]
mod tests {
    use crate::dt_fn_compare::ErrCompare;
    use std::error::Error;

    #[test]
    fn err_compare_display_and_error() {
        let err = ErrCompare::ErrInternal("oops".to_string());
        assert!(err.to_string().contains("ErrInternal"));
        assert!(err.to_string().contains("oops"));
        assert!(err.source().is_none());
    }
}
