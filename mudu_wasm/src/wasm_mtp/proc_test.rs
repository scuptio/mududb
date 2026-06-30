#[cfg(test)]
mod tests {
    use crate::wasm_mtp::proc::proc_mtp;
    use mududb::common::result::RS;

    #[test]
    fn proc_mtp_computes_result() -> RS<()> {
        let result = proc_mtp(42, 1, 2, "x".to_string())?;
        assert_eq!(result.0, 3);
        assert_eq!(result.1, "xid:42, a=1, b=2, c=x");
        Ok(())
    }
}
