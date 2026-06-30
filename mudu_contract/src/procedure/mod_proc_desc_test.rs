#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::procedure::mod_proc_desc::ModProcDesc;
    use crate::procedure::proc_desc::ProcDesc;
    use crate::tuple::tuple_datum::TupleDatum;
    use mudu::common::result::RS;
    use mudu_sys::random::next_uuid_v4_string;
    use mudu_utils::json::{read_json, write_json};
    use std::collections::HashMap;

    fn sample_proc(module: &str, name: &str) -> ProcDesc {
        let param_desc = <(i32, i32, i64)>::tuple_desc_static(&[]);
        let return_desc = <(i32, String)>::tuple_desc_static(&[]);
        ProcDesc::new(
            module.to_string(),
            name.to_string(),
            param_desc,
            return_desc,
            false,
        )
    }

    #[test]
    fn test_app_proc_desc() {
        _test_app_proc_desc().unwrap()
    }

    fn _test_app_proc_desc() -> RS<()> {
        let mut map = HashMap::new();
        for j in 0..2 {
            let mod_name = format!("mod_{}", j);
            let mut vec = vec![];
            for i in 0..3 {
                let proc_desc = sample_proc(&mod_name, &format!("proc_{}", i));
                vec.push(proc_desc);
            }
            map.insert(mod_name, vec);
        }
        let app_proc_desc = ModProcDesc::new(map);
        let id = next_uuid_v4_string();
        let path = format!(
            "{}/proc_desc_{}.toml",
            mudu_sys::env_var::temp_dir().to_str().unwrap(),
            id
        );

        println!("{}", path);
        write_json(&app_proc_desc, &path)?;

        let app_proc_desc1: ModProcDesc = read_json(&path)?;
        println!("{}", app_proc_desc1);
        Ok(())
    }

    #[test]
    fn new_empty() {
        let desc = ModProcDesc::new_empty();
        assert!(desc.modules().is_empty());
    }

    #[test]
    fn new_with_modules() {
        let mut map = HashMap::new();
        map.insert("mod".to_string(), vec![sample_proc("mod", "p")]);
        let desc = ModProcDesc::new(map);
        assert_eq!(desc.modules().len(), 1);
        assert_eq!(desc.modules()["mod"].len(), 1);
    }

    #[test]
    fn add_to_existing_module() {
        let mut desc = ModProcDesc::new_empty();
        desc.add(sample_proc("mod", "p1"));
        desc.add(sample_proc("mod", "p2"));
        assert_eq!(desc.modules().get("mod").unwrap().len(), 2);
    }

    #[test]
    fn add_to_new_module() {
        let mut desc = ModProcDesc::new_empty();
        desc.add(sample_proc("mod1", "p1"));
        desc.add(sample_proc("mod2", "p2"));
        assert_eq!(desc.modules().len(), 2);
    }

    #[test]
    fn merge_existing_module() {
        let mut a = ModProcDesc::new_empty();
        a.add(sample_proc("mod", "p1"));
        let mut b = ModProcDesc::new_empty();
        b.add(sample_proc("mod", "p2"));
        a.merge(&mut b);
        assert_eq!(a.modules().get("mod").unwrap().len(), 2);
        assert!(b.modules().is_empty());
    }

    #[test]
    fn merge_new_module() {
        let mut a = ModProcDesc::new_empty();
        a.add(sample_proc("mod1", "p1"));
        let mut b = ModProcDesc::new_empty();
        b.add(sample_proc("mod2", "p2"));
        a.merge(&mut b);
        assert_eq!(a.modules().len(), 2);
        assert!(b.modules().is_empty());
    }

    #[test]
    fn into_modules() {
        let mut desc = ModProcDesc::new_empty();
        desc.add(sample_proc("mod", "p"));
        let map = desc.into_modules();
        assert_eq!(map.len(), 1);
        assert_eq!(map["mod"].len(), 1);
    }

    #[test]
    fn display_and_debug() {
        let mut desc = ModProcDesc::new_empty();
        desc.add(sample_proc("mod", "p"));
        let s = format!("{}", desc);
        assert!(s.contains("mod"));
        let d = format!("{:?}", desc);
        assert!(d.contains("mod"));
    }
}
