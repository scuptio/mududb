#[cfg(test)]
mod tests {
    use crate::service::mudu_package::MuduPackage;
    use crate::service::runtime_opt::{ComponentTarget, RuntimeOpt};
    use crate::service::wt_runtime_component::WTRuntimeComponent;
    use mudu::common::app_info::AppInfo;
    use mudu_contract::procedure::mod_proc_desc::ModProcDesc;
    use mudu_contract::procedure::proc_desc::ProcDesc;
    use mudu_contract::tuple::tuple_datum::TupleDatum;
    use std::collections::HashMap;

    fn test_proc_desc(module_name: &str, proc_name: &str) -> ProcDesc {
        ProcDesc::new(
            module_name.to_string(),
            proc_name.to_string(),
            <()>::tuple_desc_static(&[]),
            <()>::tuple_desc_static(&[]),
            false,
        )
    }

    fn test_package(desc: ModProcDesc, modules: HashMap<String, Vec<u8>>) -> MuduPackage {
        MuduPackage {
            package_cfg: AppInfo {
                name: "app".to_string(),
                lang: "rust".to_string(),
                version: "0.1.0".to_string(),
                use_async: false,
            },
            ddl_sql: "create table t(id int primary key);".to_string(),
            package_desc: desc,
            initdb_sql: String::new(),
            modules,
        }
    }

    // These tests build and instantiate a Wasmtime runtime component, which
    // calls foreign functions that Miri does not support. They are ignored
    // under Miri and run only on native builds.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn instantiate_rejects_p3_target() {
        let mut runtime = WTRuntimeComponent::build(&RuntimeOpt {
            component_target: ComponentTarget::P3,
            enable_async: false,
            sever_mode: Default::default(),
            async_runtime: None,
        })
        .unwrap();

        let err = runtime.instantiate().unwrap_err();
        assert!(err.to_string().contains("not implemented yet"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn compile_modules_requires_declared_module_bytes() {
        let mut desc_map = HashMap::new();
        desc_map.insert("mod_0".to_string(), vec![test_proc_desc("mod_0", "proc")]);
        let package = test_package(ModProcDesc::new(desc_map), HashMap::new());

        let runtime = WTRuntimeComponent::build(&RuntimeOpt::default()).unwrap();
        let err = match runtime.compile_modules(&package) {
            Ok(_) => panic!("expected missing module error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("no such module named mod_0"));
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn compile_modules_rejects_plain_wasm_module_for_component_runtime() {
        let mut desc_map = HashMap::new();
        desc_map.insert("mod_0".to_string(), vec![test_proc_desc("mod_0", "proc")]);
        let mut modules = HashMap::new();
        modules.insert("mod_0".to_string(), b"\0asm\x01\0\0\0".to_vec());
        let package = test_package(ModProcDesc::new(desc_map), modules);

        let mut runtime = WTRuntimeComponent::build(&RuntimeOpt::default()).unwrap();
        runtime.instantiate().unwrap();
        let err = match runtime.compile_modules(&package) {
            Ok(_) => panic!("expected component validation error"),
            Err(err) => err,
        };
        assert!(err.to_string().contains("runtime target is component"));
    }
}
