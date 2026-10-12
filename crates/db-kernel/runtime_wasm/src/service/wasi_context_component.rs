use super::invocation_limits::InvocationContext;
use super::invocation_memory::InvocationMemory;
use ::conn::worker_local::WorkerLocalRef;
use std::sync::Arc;
use wasmtime::component::ResourceTable;
use wasmtime_wasi::{WasiCtx, WasiCtxBuilder, WasiCtxView, WasiView};

// impl Guest trait
/// WASI store context for guest components: the WASI state and resource
/// table plus the worker-local binding the host calls run against.
pub struct WasiContextComponent {
    pub(crate) memory: InvocationMemory,
    control: Arc<InvocationContext>,
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
    /// Creates a context from a WASI context and an optional worker-local
    /// binding.
    pub fn new(ctx: WasiCtx, worker_local: Option<WorkerLocalRef>) -> Self {
        Self {
            memory: InvocationMemory::new(&Default::default()),
            control: InvocationContext::new(Default::default())
                .expect("valid default invocation limits"),
            ctx,
            table: Default::default(),
            worker_local,
        }
    }

    pub(crate) fn set_invocation(&mut self, control: Arc<InvocationContext>) {
        self.memory = InvocationMemory::new(&control.limits);
        self.control = control;
    }

    pub(crate) fn invocation(&self) -> Arc<InvocationContext> {
        self.control.clone()
    }

    /// Returns the current worker-local binding, if any.
    pub fn worker_local(&self) -> Option<WorkerLocalRef> {
        self.worker_local.clone()
    }

    /// Binds the worker used by this isolated invocation.
    pub fn set_worker_local(&mut self, worker_local: Option<WorkerLocalRef>) {
        self.worker_local = worker_local;
    }
}

/// Builds a WASI store context that inherits stdio and args, bound to the
/// given worker-local interface.
pub fn build_wasi_component_context(worker_local: Option<WorkerLocalRef>) -> WasiContextComponent {
    let wasi = WasiCtxBuilder::new().inherit_stdio().inherit_args().build();

    WasiContextComponent::new(wasi, worker_local)
}

/// Synchronous `mududb:api/system` host bindings for sync-world components.
pub mod sync_host {
    use super::WasiContextComponent;
    use crate::service::kernel_function_p2::{
        host_batch, host_close, host_command, host_delete, host_fetch, host_fs_close,
        host_fs_fstat, host_fs_fsync, host_fs_lseek, host_fs_open, host_fs_pread, host_fs_pwrite,
        host_fs_read, host_fs_readdir, host_fs_stat, host_fs_write, host_get, host_open, host_put,
        host_range, host_relation_get, host_relation_insert, host_relation_update,
    };
    use wasmtime::component::bindgen;

    bindgen!({ world: "api", path: "wit/api.wit", imports: { default: trappable } });
    impl mududb::api::system::Host for WasiContextComponent {
        fn query(&mut self, query_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("ctx", &query_in)?;

            let output = crate::service::invocation_query::query_sync(query_in, control.clone());
            control.leave_host(output)
        }

        fn fetch(&mut self, result_cursor: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fetch", &result_cursor)?;

            let output = host_fetch(result_cursor);
            control.leave_host(output)
        }

        fn command(&mut self, command_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("command", &command_in)?;

            let output = host_command(command_in);
            control.leave_host(output)
        }

        fn batch(&mut self, batch_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("batch", &batch_in)?;

            let output = host_batch(batch_in);
            control.leave_host(output)
        }

        fn open(&mut self, open_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("open", &open_in)?;
            let resource_input = open_in.clone();

            let output = host_open(open_in, self.worker_local());
            control.record_host("open", &resource_input, &output, self.worker_local())?;
            control.leave_host(output)
        }

        fn close(&mut self, close_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("close", &close_in)?;
            let resource_input = close_in.clone();

            let output = host_close(close_in, self.worker_local());
            control.record_host("close", &resource_input, &output, self.worker_local())?;
            control.leave_host(output)
        }

        fn get(&mut self, get_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("get", &get_in)?;

            let output = host_get(get_in, self.worker_local());
            control.leave_host(output)
        }

        fn put(&mut self, put_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("put", &put_in)?;

            let output = host_put(put_in, self.worker_local());
            control.leave_host(output)
        }

        fn delete(&mut self, delete_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("delete", &delete_in)?;

            let output = host_delete(delete_in, self.worker_local());
            control.leave_host(output)
        }

        fn range(&mut self, range_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("range", &range_in)?;

            let output = host_range(range_in, self.worker_local());
            control.leave_host(output)
        }

        fn relation_get(&mut self, relation_get_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("relation_get", &relation_get_in)?;

            let output = host_relation_get(relation_get_in, self.worker_local());
            control.leave_host(output)
        }

        fn relation_update(&mut self, relation_update_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("relation_update", &relation_update_in)?;

            let output = host_relation_update(relation_update_in, self.worker_local());
            control.leave_host(output)
        }

        fn relation_insert(&mut self, relation_insert_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("relation_insert", &relation_insert_in)?;

            let output = host_relation_insert(relation_insert_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_open(&mut self, fs_open_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_open", &fs_open_in)?;
            let resource_input = fs_open_in.clone();

            let output = host_fs_open(fs_open_in, self.worker_local());
            control.record_host("fs_open", &resource_input, &output, self.worker_local())?;
            control.leave_host(output)
        }

        fn fs_close(&mut self, fs_close_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_close", &fs_close_in)?;
            let resource_input = fs_close_in.clone();

            let output = host_fs_close(fs_close_in, self.worker_local());
            control.record_host("fs_close", &resource_input, &output, self.worker_local())?;
            control.leave_host(output)
        }

        fn fs_read(&mut self, fs_read_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_read", &fs_read_in)?;

            let output = host_fs_read(fs_read_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_write(&mut self, fs_write_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_write", &fs_write_in)?;

            let output = host_fs_write(fs_write_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_pread(&mut self, fs_pread_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_pread", &fs_pread_in)?;

            let output = host_fs_pread(fs_pread_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_pwrite(&mut self, fs_pwrite_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_pwrite", &fs_pwrite_in)?;

            let output = host_fs_pwrite(fs_pwrite_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_lseek(&mut self, fs_lseek_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_lseek", &fs_lseek_in)?;

            let output = host_fs_lseek(fs_lseek_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_fstat(&mut self, fs_fstat_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_fstat", &fs_fstat_in)?;

            let output = host_fs_fstat(fs_fstat_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_stat(&mut self, fs_stat_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_stat", &fs_stat_in)?;

            let output = host_fs_stat(fs_stat_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_fsync(&mut self, fs_fsync_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_fsync", &fs_fsync_in)?;

            let output = host_fs_fsync(fs_fsync_in, self.worker_local());
            control.leave_host(output)
        }

        fn fs_readdir(&mut self, fs_readdir_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("fs_readdir", &fs_readdir_in)?;

            let output = host_fs_readdir(fs_readdir_in, self.worker_local());
            control.leave_host(output)
        }
    }
}

/// Asynchronous `mududb:async-api/system` host bindings for components
/// built against the async host API.
pub mod async_host {
    use super::WasiContextComponent;
    use crate::service::kernel_function_p2_async::{
        async_host_batch, async_host_close, async_host_command, async_host_delete,
        async_host_fetch, async_host_fs_close, async_host_fs_fstat, async_host_fs_fsync,
        async_host_fs_lseek, async_host_fs_open, async_host_fs_pread, async_host_fs_pwrite,
        async_host_fs_read, async_host_fs_readdir, async_host_fs_stat, async_host_fs_write,
        async_host_get, async_host_open, async_host_put, async_host_range, async_host_relation_get,
        async_host_relation_insert, async_host_relation_update,
    };
    use wasmtime::component::{Accessor, HasData, HasSelf, bindgen};

    bindgen!({
            world: "async-api",
            path: "wit/async-api.wit",
            imports: {
                "mududb:async-api/system": async | trappable
            },

    });
    impl HasData for WasiContextComponent {
        type Data<'a> = &'a mut WasiContextComponent;
    }

    impl mududb::async_api::system::Host for WasiContextComponent {}

    impl mududb::async_api::system::HostWithStore<WasiContextComponent>
        for HasSelf<WasiContextComponent>
    {
        async fn query(
            accessor: &Accessor<WasiContextComponent, Self>,
            query_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("query", &query_in)?;

            let output = crate::service::invocation_query::query(query_in, control.clone()).await;
            control.leave_host(output)
        }

        async fn fetch(
            accessor: &Accessor<WasiContextComponent, Self>,
            result_cursor: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fetch", &result_cursor)?;

            let output = async_host_fetch(result_cursor).await;
            control.leave_host(output)
        }

        async fn command(
            accessor: &Accessor<WasiContextComponent, Self>,
            command_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("command", &command_in)?;

            let output = async_host_command(command_in).await;
            control.leave_host(output)
        }

        async fn batch(
            accessor: &Accessor<WasiContextComponent, Self>,
            batch_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("batch", &batch_in)?;

            let output = async_host_batch(batch_in).await;
            control.leave_host(output)
        }

        async fn open(
            accessor: &Accessor<WasiContextComponent, Self>,
            open_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("open", &open_in)?;
            let resource_input = open_in.clone();
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_open(open_in, worker).await;
            control.record_host(
                "open",
                &resource_input,
                &output,
                accessor.with(|mut access| access.get().worker_local()),
            )?;
            control.leave_host(output)
        }

        async fn close(
            accessor: &Accessor<WasiContextComponent, Self>,
            close_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("close", &close_in)?;
            let resource_input = close_in.clone();
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_close(close_in, worker).await;
            control.record_host(
                "close",
                &resource_input,
                &output,
                accessor.with(|mut access| access.get().worker_local()),
            )?;
            control.leave_host(output)
        }

        async fn get(
            accessor: &Accessor<WasiContextComponent, Self>,
            get_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("get", &get_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_get(get_in, worker).await;
            control.leave_host(output)
        }

        async fn put(
            accessor: &Accessor<WasiContextComponent, Self>,
            put_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("put", &put_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_put(put_in, worker).await;
            control.leave_host(output)
        }

        async fn delete(
            accessor: &Accessor<WasiContextComponent, Self>,
            delete_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("delete", &delete_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_delete(delete_in, worker).await;
            control.leave_host(output)
        }

        async fn range(
            accessor: &Accessor<WasiContextComponent, Self>,
            range_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("range", &range_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_range(range_in, worker).await;
            control.leave_host(output)
        }

        async fn relation_get(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_get_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("relation_get", &relation_get_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_relation_get(relation_get_in, worker).await;
            control.leave_host(output)
        }

        async fn relation_update(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_update_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("relation_update", &relation_update_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_relation_update(relation_update_in, worker).await;
            control.leave_host(output)
        }

        async fn relation_insert(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_insert_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("relation_insert", &relation_insert_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_relation_insert(relation_insert_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_open(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_open_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_open", &fs_open_in)?;
            let resource_input = fs_open_in.clone();
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_open(fs_open_in, worker).await;
            control.record_host(
                "fs_open",
                &resource_input,
                &output,
                accessor.with(|mut access| access.get().worker_local()),
            )?;
            control.leave_host(output)
        }

        async fn fs_close(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_close_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_close", &fs_close_in)?;
            let resource_input = fs_close_in.clone();
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_close(fs_close_in, worker).await;
            control.record_host(
                "fs_close",
                &resource_input,
                &output,
                accessor.with(|mut access| access.get().worker_local()),
            )?;
            control.leave_host(output)
        }

        async fn fs_read(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_read_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_read", &fs_read_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_read(fs_read_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_write(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_write_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_write", &fs_write_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_write(fs_write_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_pread(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_pread_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_pread", &fs_pread_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_pread(fs_pread_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_pwrite(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_pwrite_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_pwrite", &fs_pwrite_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_pwrite(fs_pwrite_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_lseek(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_lseek_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_lseek", &fs_lseek_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_lseek(fs_lseek_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_fstat(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_fstat_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_fstat", &fs_fstat_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_fstat(fs_fstat_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_stat(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_stat_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_stat", &fs_stat_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_stat(fs_stat_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_fsync(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_fsync_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_fsync", &fs_fsync_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_fsync(fs_fsync_in, worker).await;
            control.leave_host(output)
        }

        async fn fs_readdir(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_readdir_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            let control = accessor.with(|mut access| access.get().invocation());
            control.prepare_host("fs_readdir", &fs_readdir_in)?;
            let worker = accessor.with(|mut access| access.get().worker_local());

            let output = async_host_fs_readdir(fs_readdir_in, worker).await;
            control.leave_host(output)
        }
    }
}
