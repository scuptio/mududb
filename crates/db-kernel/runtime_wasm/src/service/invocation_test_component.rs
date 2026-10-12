//! Small component fixtures independent of external SDKs.

use super::wasi_context_component::WasiContextComponent;
use super::wt_instance_pre::WTInstancePre;
use wasmtime::component::{Component, Linker};
use wasmtime::{Config, Engine};

pub(crate) fn component(body: &str) -> WTInstancePre {
    let source = format!(
        r#"(component
      (core module $m
        (memory (export "memory") 1)
        (global $counter (mut i32) (i32.const 0))
        (func (export "realloc") (param i32 i32 i32 i32) (result i32) i32.const 1024)
        (func (export "run") (param i32 i32) (result i32)
          {body}
          i32.const 0 i32.const 16 i32.store
          i32.const 4 i32.const 1 i32.store
          i32.const 0))
      (core instance $i (instantiate $m))
      (func (export "mp2-test") (param "input" (list u8)) (result (list u8))
        (canon lift (core func $i "run") (memory (core memory $i "memory")) (realloc (core func $i "realloc")))))"#
    );
    let mut config = Config::new();
    config
        .wasm_component_model(true)
        .consume_fuel(true)
        .epoch_interruption(true);
    let engine = Engine::new(&config).unwrap();
    let component = Component::new(&engine, source).unwrap();
    let linker = Linker::<WasiContextComponent>::new(&engine);
    WTInstancePre::from_component(linker.instantiate_pre(&component).unwrap(), false).unwrap()
}
