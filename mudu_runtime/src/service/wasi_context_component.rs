use mudu_kernel::server_ur::worker_local::WorkerLocalRef;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

// impl Guest trait
pub struct WasiContextComponent {
    ctx: WasiCtx,
    table: ResourceTable,
    worker_local: Option<WorkerLocalRef>,
}

impl WasiView for WasiContextComponent {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView {
            ctx: &mut self.ctx,
            table: &mut self.table,
        }
    }
}

impl WasiContextComponent {
    pub fn new(ctx: WasiCtx, worker_local: Option<WorkerLocalRef>) -> Self {
        Self {
            ctx,
            table: Default::default(),
            worker_local,
        }
    }

    pub fn worker_local(&self) -> Option<&WorkerLocalRef> {
        self.worker_local.as_ref()
    }
}

pub fn build_wasi_component_context(worker_local: Option<WorkerLocalRef>) -> WasiContextComponent {
    let wasi = WasiCtxBuilder::new().inherit_stdio().inherit_args().build();
    let context = WasiContextComponent::new(wasi, worker_local);
    context
}

pub mod sync_host {
    use super::WasiContextComponent;
    use crate::service::kernel_function_p2::{
        host_close, host_command, host_fetch, host_get, host_open, host_put, host_query, host_range,
    };
    use wasmtime::component::bindgen;

    bindgen!("api" in "wit/api.wit");
    impl mududb::api::system::Host for WasiContextComponent {
        fn query(&mut self, query_in: Vec<u8>) -> Vec<u8> {
            host_query(query_in)
        }

        fn fetch(&mut self, result_cursor: Vec<u8>) -> Vec<u8> {
            host_fetch(result_cursor)
        }

        fn command(&mut self, command_in: Vec<u8>) -> Vec<u8> {
            host_command(command_in)
        }

        fn open(&mut self, open_in: Vec<u8>) -> Vec<u8> {
            host_open(open_in, self.worker_local())
        }

        fn close(&mut self, close_in: Vec<u8>) -> Vec<u8> {
            host_close(close_in, self.worker_local())
        }

        fn get(&mut self, get_in: Vec<u8>) -> Vec<u8> {
            host_get(get_in, self.worker_local())
        }

        fn put(&mut self, put_in: Vec<u8>) -> Vec<u8> {
            host_put(put_in, self.worker_local())
        }

        fn range(&mut self, range_in: Vec<u8>) -> Vec<u8> {
            host_range(range_in, self.worker_local())
        }
    }
}

pub mod async_host {
    use super::WasiContextComponent;
    use crate::service::kernel_function_p2_async::{
        async_host_close, async_host_command, async_host_fetch, async_host_get, async_host_open,
        async_host_put, async_host_query, async_host_range,
    };
    use wasmtime::component::bindgen;

    bindgen!({
            world: "async-api",
            path: "wit/async-api.wit",
            imports: {
                "mududb:async-api/system":async,
            }
    });

    impl mududb::async_api::system::Host for WasiContextComponent {
        async fn query(&mut self, query_in: Vec<u8>) -> Vec<u8> {
            async_host_query(query_in).await
        }

        async fn fetch(&mut self, result_cursor: Vec<u8>) -> Vec<u8> {
            async_host_fetch(result_cursor).await
        }

        async fn command(&mut self, command_in: Vec<u8>) -> Vec<u8> {
            async_host_command(command_in).await
        }

        async fn open(&mut self, open_in: Vec<u8>) -> Vec<u8> {
            async_host_open(open_in, self.worker_local()).await
        }

        async fn close(&mut self, close_in: Vec<u8>) -> Vec<u8> {
            async_host_close(close_in, self.worker_local()).await
        }

        async fn get(&mut self, get_in: Vec<u8>) -> Vec<u8> {
            async_host_get(get_in, self.worker_local()).await
        }

        async fn put(&mut self, put_in: Vec<u8>) -> Vec<u8> {
            async_host_put(put_in, self.worker_local()).await
        }

        async fn range(&mut self, range_in: Vec<u8>) -> Vec<u8> {
            async_host_range(range_in, self.worker_local()).await
        }
    }
}
