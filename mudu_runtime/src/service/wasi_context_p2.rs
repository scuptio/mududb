use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};


// impl Guest trait
pub struct WasiContextP2 {
    ctx: WasiCtx,
    table: ResourceTable,
}

impl WasiView for WasiContextP2 {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}


impl WasiContextP2 {
    pub fn new(ctx: WasiCtx) -> Self {
        Self {
            ctx,
            table: Default::default(),
        }
    }
}


pub fn build_wasi_p2_context() -> WasiContextP2 {
    let wasi = WasiCtxBuilder::new()
        .inherit_stdio()
        .inherit_args()
        .build();
    let context = WasiContextP2::new(wasi);
    context
}

pub mod sync_host {
    use super::WasiContextP2;
    use crate::service::kernel_function_p2::{host_command, host_fetch, host_query};
    use wasmtime::component::bindgen;

    bindgen!("api" in "wit/api.wit");
    impl mududb::api::system::Host for WasiContextP2 {
        fn query(&mut self, query_in: Vec<u8>) -> Vec<u8> {
            host_query(query_in)
        }

        fn fetch(&mut self, result_cursor: Vec<u8>) -> Vec<u8> {
            host_fetch(result_cursor)
        }

        fn command(&mut self, command_in: Vec<u8>) -> Vec<u8> {
            host_command(command_in)
        }
    }
}

pub mod async_host {
    use super::WasiContextP2;
    use crate::service::kernel_function_p2_async::{
        async_host_command, async_host_fetch, async_host_query};
    use wasmtime::component::bindgen;

    bindgen!({
            world: "async-api",
            path: "wit/async-api.wit",
            imports: {
                "mududb:async-api/system":async,
            }
    });

    impl mududb::async_api::system::Host for WasiContextP2 {
        async fn query(&mut self, query_in: Vec<u8>) -> Vec<u8> {
            async_host_query(query_in).await
        }

        async fn fetch(&mut self, result_cursor: Vec<u8>) -> Vec<u8> {
            async_host_fetch(result_cursor).await
        }

        async fn command(&mut self, command_in: Vec<u8>) -> Vec<u8> {
            async_host_command(command_in).await
        }
    }
}