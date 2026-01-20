
#[cfg(test)]
mod tests {
    use mudu::common::result::RS;

    #[test]
    fn test_mudul_src_gen() {
        let r = test_src_file_gen();
        if r.is_err() {
            let e = r.as_ref().err().unwrap();
            println!("test error : {}", e);
        }
        assert!(r.is_ok());
    }

    fn test_src_file_gen() -> RS<()> {
        Ok(())
    }
}
