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
        host_batch, host_cache_delete, host_cache_get, host_cache_open, host_cache_put,
        host_cache_ttl, host_close, host_command, host_delete, host_fetch, host_fs_close,
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

        fn cache_open(&mut self, cache_open_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("cache_open", &cache_open_in)?;

            let output = host_cache_open(cache_open_in, self.worker_local(), control.app_name());
            control.leave_host(output)
        }

        fn cache_get(&mut self, cache_get_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("cache_get", &cache_get_in)?;

            let output = host_cache_get(cache_get_in, self.worker_local());
            control.leave_host(output)
        }

        fn cache_put(&mut self, cache_put_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("cache_put", &cache_put_in)?;

            let output = host_cache_put(cache_put_in, self.worker_local());
            control.leave_host(output)
        }

        fn cache_delete(&mut self, cache_delete_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("cache_delete", &cache_delete_in)?;

            let output = host_cache_delete(cache_delete_in, self.worker_local());
            control.leave_host(output)
        }

        fn cache_ttl(&mut self, cache_ttl_in: Vec<u8>) -> wasmtime::Result<Vec<u8>> {
            let control = self.invocation();
            control.prepare_host("cache_ttl", &cache_ttl_in)?;

            let output = host_cache_ttl(cache_ttl_in, self.worker_local());
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
    use crate::service::invocation_limits::InvocationContext;
    use crate::service::kernel_function_p2_async::{
        async_host_batch, async_host_close, async_host_command, async_host_delete,
        async_host_fetch, async_host_fs_close, async_host_fs_fstat, async_host_fs_fsync,
        async_host_fs_lseek, async_host_fs_open, async_host_fs_pread, async_host_fs_pwrite,
        async_host_fs_read, async_host_fs_readdir, async_host_fs_stat, async_host_fs_write,
        async_host_get, async_host_open, async_host_put, async_host_range, async_host_relation_get,
        async_host_relation_insert, async_host_relation_update,
    };
    use ::conn::worker_local::WorkerLocalRef;
    use std::sync::Arc;
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

    /// The invocation budget control and worker-local interface of the
    /// component behind `accessor`.
    fn control_and_worker(
        accessor: &Accessor<WasiContextComponent, HasSelf<WasiContextComponent>>,
    ) -> (Arc<InvocationContext>, Option<WorkerLocalRef>) {
        accessor.with(|mut access| {
            let control = access.get().invocation();
            let worker = access.get().worker_local();
            (control, worker)
        })
    }

    /// prepare → call → leave for host calls that open no tracked resource.
    pub(crate) async fn drive_plain<F, Fut>(
        control: &Arc<InvocationContext>,
        kind: &str,
        input: Vec<u8>,
        worker: Option<WorkerLocalRef>,
        call: F,
    ) -> wasmtime::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>, Option<WorkerLocalRef>, Arc<InvocationContext>) -> Fut,
        Fut: std::future::Future<Output = Vec<u8>>,
    {
        control.prepare_host(kind, &input)?;
        let output = call(input, worker, control.clone()).await;
        control.leave_host(output)
    }

    /// prepare → call → record → leave for host calls that open or close a
    /// tracked resource (`open`/`close`/`fs_open`/`fs_close`): the handle is
    /// recorded so a crashed procedure still closes it.
    pub(crate) async fn drive_tracked<F, Fut>(
        control: &Arc<InvocationContext>,
        kind: &str,
        input: Vec<u8>,
        worker: Option<WorkerLocalRef>,
        call: F,
    ) -> wasmtime::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>, Option<WorkerLocalRef>, Arc<InvocationContext>) -> Fut,
        Fut: std::future::Future<Output = Vec<u8>>,
    {
        control.prepare_host(kind, &input)?;
        let output = call(input.clone(), worker.clone(), control.clone()).await;
        control.record_host(kind, &input, &output, worker)?;
        control.leave_host(output)
    }

    /// `drive_plain` reading control and worker from the component behind
    /// `accessor` (the only wasmtime-bound step of an async host call).
    async fn drive_plain_access<F, Fut>(
        accessor: &Accessor<WasiContextComponent, HasSelf<WasiContextComponent>>,
        kind: &str,
        input: Vec<u8>,
        call: F,
    ) -> wasmtime::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>, Option<WorkerLocalRef>, Arc<InvocationContext>) -> Fut,
        Fut: std::future::Future<Output = Vec<u8>>,
    {
        let (control, worker) = control_and_worker(accessor);
        drive_plain(&control, kind, input, worker, call).await
    }

    /// `drive_tracked` reading control and worker from the component behind
    /// `accessor`, for the resource-opening host calls.
    async fn drive_tracked_access<F, Fut>(
        accessor: &Accessor<WasiContextComponent, HasSelf<WasiContextComponent>>,
        kind: &str,
        input: Vec<u8>,
        call: F,
    ) -> wasmtime::Result<Vec<u8>>
    where
        F: FnOnce(Vec<u8>, Option<WorkerLocalRef>, Arc<InvocationContext>) -> Fut,
        Fut: std::future::Future<Output = Vec<u8>>,
    {
        let (control, worker) = control_and_worker(accessor);
        drive_tracked(&control, kind, input, worker, call).await
    }

    /// One async host method: delegate to the plain or tracked drive path
    /// with the handler for this WIT method. The macro keeps each bridge a
    /// single statement; the drive fns carry the budget/resource logic and
    /// are unit-tested directly.
    macro_rules! async_host_method {
        ($name:ident, $input:ident, $kind:literal, $drive:ident, $handler:expr) => {
            async fn $name(
                accessor: &Accessor<WasiContextComponent, Self>,
                $input: Vec<u8>,
            ) -> wasmtime::Result<Vec<u8>> {
                $drive(accessor, $kind, $input, $handler).await
            }
        };
    }

    impl mududb::async_api::system::HostWithStore<WasiContextComponent>
        for HasSelf<WasiContextComponent>
    {
        async_host_method!(
            query,
            query_in,
            "query",
            drive_plain_access,
            |i, _w, c| async move { crate::service::invocation_query::query(i, c).await }
        );

        async_host_method!(
            fetch,
            result_cursor,
            "fetch",
            drive_plain_access,
            |i, _w, _c| async move { async_host_fetch(i).await }
        );

        async_host_method!(
            command,
            command_in,
            "command",
            drive_plain_access,
            |i, _w, _c| async move { async_host_command(i).await }
        );

        async_host_method!(
            batch,
            batch_in,
            "batch",
            drive_plain_access,
            |i, _w, _c| async move { async_host_batch(i).await }
        );

        async_host_method!(
            open,
            open_in,
            "open",
            drive_tracked_access,
            |i, w, _c| async move { async_host_open(i, w).await }
        );

        async_host_method!(
            close,
            close_in,
            "close",
            drive_tracked_access,
            |i, w, _c| async move { async_host_close(i, w).await }
        );

        async_host_method!(
            get,
            get_in,
            "get",
            drive_plain_access,
            |i, w, _c| async move { async_host_get(i, w).await }
        );

        async_host_method!(
            put,
            put_in,
            "put",
            drive_plain_access,
            |i, w, _c| async move { async_host_put(i, w).await }
        );

        async_host_method!(
            delete,
            delete_in,
            "delete",
            drive_plain_access,
            |i, w, _c| async move { async_host_delete(i, w).await }
        );

        async_host_method!(
            range,
            range_in,
            "range",
            drive_plain_access,
            |i, w, _c| async move { async_host_range(i, w).await }
        );

        async fn relation_get(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_get_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            drive_plain_access(
                accessor,
                "relation_get",
                relation_get_in,
                |i, w, _c| async move { async_host_relation_get(i, w).await },
            )
            .await
        }

        async fn relation_update(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_update_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            drive_plain_access(
                accessor,
                "relation_update",
                relation_update_in,
                |i, w, _c| async move { async_host_relation_update(i, w).await },
            )
            .await
        }

        async fn relation_insert(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_insert_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            drive_plain_access(
                accessor,
                "relation_insert",
                relation_insert_in,
                |i, w, _c| async move { async_host_relation_insert(i, w).await },
            )
            .await
        }

        async_host_method!(
            fs_open,
            fs_open_in,
            "fs_open",
            drive_tracked_access,
            |i, w, _c| async move { async_host_fs_open(i, w).await }
        );

        async_host_method!(
            fs_close,
            fs_close_in,
            "fs_close",
            drive_tracked_access,
            |i, w, _c| async move { async_host_fs_close(i, w).await }
        );

        async_host_method!(
            fs_read,
            fs_read_in,
            "fs_read",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_read(i, w).await }
        );

        async_host_method!(
            fs_write,
            fs_write_in,
            "fs_write",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_write(i, w).await }
        );

        async_host_method!(
            fs_pread,
            fs_pread_in,
            "fs_pread",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_pread(i, w).await }
        );

        async_host_method!(
            fs_pwrite,
            fs_pwrite_in,
            "fs_pwrite",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_pwrite(i, w).await }
        );

        async_host_method!(
            fs_lseek,
            fs_lseek_in,
            "fs_lseek",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_lseek(i, w).await }
        );

        async_host_method!(
            fs_fstat,
            fs_fstat_in,
            "fs_fstat",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_fstat(i, w).await }
        );

        async_host_method!(
            fs_stat,
            fs_stat_in,
            "fs_stat",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_stat(i, w).await }
        );

        async_host_method!(
            fs_fsync,
            fs_fsync_in,
            "fs_fsync",
            drive_plain_access,
            |i, w, _c| async move { async_host_fs_fsync(i, w).await }
        );

        async fn fs_readdir(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_readdir_in: Vec<u8>,
        ) -> wasmtime::Result<Vec<u8>> {
            drive_plain_access(
                accessor,
                "fs_readdir",
                fs_readdir_in,
                |i, w, _c| async move { async_host_fs_readdir(i, w).await },
            )
            .await
        }
    }
}
