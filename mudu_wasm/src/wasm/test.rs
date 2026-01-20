#[cfg(test)]
mod tests {
    use crate::wasm::{proc, proc2};
    use mudu::common::result::RS;
    use mudu::this_file;
    use mudu::utils::toml::write_toml;
    use mudu_contract::procedure::package_desc::PackageDesc;
    use std::path::PathBuf;

    #[test]
    fn test_gen_proc_desc() {
        _test_gen_proc_desc().unwrap();
    }

    fn _test_gen_proc_desc() -> RS<()> {
        let mut app_proc_desc = PackageDesc::new_empty();
        for proc_desc in vec![
            proc::mudu_proc_desc_proc(),
            proc2::mudu_proc_desc_proc_sys_call(),
            proc2::mudu_proc_desc_proc2()
        ] {
            app_proc_desc.add(proc_desc.clone());
        }
        let mut path_buf = PathBuf::from(this_file!());
        path_buf.pop();
        path_buf.pop();
        path_buf.pop();
        let path_buf = path_buf.join("toml").join("package.desc.toml");
        write_toml(&app_proc_desc, &path_buf).unwrap();
        Ok(())
    }
}