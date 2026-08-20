use mudu_kernel::server::worker_local::WorkerLocalRef;
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

    pub fn worker_local(&self) -> Option<WorkerLocalRef> {
        self.worker_local.clone()
    }

    /// Rebind the worker context. Pooled instances are shared across workers,
    /// so the worker must be set before every invocation.
    pub fn set_worker_local(&mut self, worker_local: Option<WorkerLocalRef>) {
        self.worker_local = worker_local;
    }
}

pub fn build_wasi_component_context(worker_local: Option<WorkerLocalRef>) -> WasiContextComponent {
    let wasi = WasiCtxBuilder::new().inherit_stdio().inherit_args().build();

    WasiContextComponent::new(wasi, worker_local)
}

pub mod sync_host {
    use super::WasiContextComponent;
    use crate::service::kernel_function_p2::{
        host_batch, host_close, host_command, host_delete, host_fetch, host_fs_close,
        host_fs_fstat, host_fs_fsync, host_fs_lseek, host_fs_open, host_fs_pread, host_fs_pwrite,
        host_fs_read, host_fs_readdir, host_fs_stat, host_fs_write, host_get, host_open, host_put,
        host_query, host_range, host_relation_get, host_relation_insert, host_relation_update,
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

        fn batch(&mut self, batch_in: Vec<u8>) -> Vec<u8> {
            host_batch(batch_in)
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

        fn delete(&mut self, delete_in: Vec<u8>) -> Vec<u8> {
            host_delete(delete_in, self.worker_local())
        }

        fn range(&mut self, range_in: Vec<u8>) -> Vec<u8> {
            host_range(range_in, self.worker_local())
        }

        fn relation_get(&mut self, relation_get_in: Vec<u8>) -> Vec<u8> {
            host_relation_get(relation_get_in, self.worker_local())
        }

        fn relation_update(&mut self, relation_update_in: Vec<u8>) -> Vec<u8> {
            host_relation_update(relation_update_in, self.worker_local())
        }

        fn relation_insert(&mut self, relation_insert_in: Vec<u8>) -> Vec<u8> {
            host_relation_insert(relation_insert_in, self.worker_local())
        }

        fn fs_open(&mut self, fs_open_in: Vec<u8>) -> Vec<u8> {
            host_fs_open(fs_open_in, self.worker_local())
        }

        fn fs_close(&mut self, fs_close_in: Vec<u8>) -> Vec<u8> {
            host_fs_close(fs_close_in, self.worker_local())
        }

        fn fs_read(&mut self, fs_read_in: Vec<u8>) -> Vec<u8> {
            host_fs_read(fs_read_in, self.worker_local())
        }

        fn fs_write(&mut self, fs_write_in: Vec<u8>) -> Vec<u8> {
            host_fs_write(fs_write_in, self.worker_local())
        }

        fn fs_pread(&mut self, fs_pread_in: Vec<u8>) -> Vec<u8> {
            host_fs_pread(fs_pread_in, self.worker_local())
        }

        fn fs_pwrite(&mut self, fs_pwrite_in: Vec<u8>) -> Vec<u8> {
            host_fs_pwrite(fs_pwrite_in, self.worker_local())
        }

        fn fs_lseek(&mut self, fs_lseek_in: Vec<u8>) -> Vec<u8> {
            host_fs_lseek(fs_lseek_in, self.worker_local())
        }

        fn fs_fstat(&mut self, fs_fstat_in: Vec<u8>) -> Vec<u8> {
            host_fs_fstat(fs_fstat_in, self.worker_local())
        }

        fn fs_stat(&mut self, fs_stat_in: Vec<u8>) -> Vec<u8> {
            host_fs_stat(fs_stat_in, self.worker_local())
        }

        fn fs_fsync(&mut self, fs_fsync_in: Vec<u8>) -> Vec<u8> {
            host_fs_fsync(fs_fsync_in, self.worker_local())
        }

        fn fs_readdir(&mut self, fs_readdir_in: Vec<u8>) -> Vec<u8> {
            host_fs_readdir(fs_readdir_in, self.worker_local())
        }
    }
}

pub mod async_host {
    use super::WasiContextComponent;
    use crate::service::kernel_function_p2_async::{
        async_host_batch, async_host_close, async_host_command, async_host_delete,
        async_host_fetch, async_host_fs_close, async_host_fs_fstat, async_host_fs_fsync,
        async_host_fs_lseek, async_host_fs_open, async_host_fs_pread, async_host_fs_pwrite,
        async_host_fs_read, async_host_fs_readdir, async_host_fs_stat, async_host_fs_write,
        async_host_get, async_host_open, async_host_put, async_host_query, async_host_range,
        async_host_relation_get, async_host_relation_insert, async_host_relation_update,
    };
    use wasmtime::component::{Accessor, HasData, HasSelf, bindgen};

    bindgen!({
            world: "async-api",
            path: "wit/async-api.wit",
            imports: {
                "mududb:async-api/system":async
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
            _accessor: &Accessor<WasiContextComponent, Self>,
            query_in: Vec<u8>,
        ) -> Vec<u8> {
            async_host_query(query_in).await
        }

        async fn fetch(
            _accessor: &Accessor<WasiContextComponent, Self>,
            result_cursor: Vec<u8>,
        ) -> Vec<u8> {
            async_host_fetch(result_cursor).await
        }

        async fn command(
            _accessor: &Accessor<WasiContextComponent, Self>,
            command_in: Vec<u8>,
        ) -> Vec<u8> {
            async_host_command(command_in).await
        }

        async fn batch(
            _accessor: &Accessor<WasiContextComponent, Self>,
            batch_in: Vec<u8>,
        ) -> Vec<u8> {
            async_host_batch(batch_in).await
        }

        async fn open(
            accessor: &Accessor<WasiContextComponent, Self>,
            open_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_open(open_in, worker).await
        }

        async fn close(
            accessor: &Accessor<WasiContextComponent, Self>,
            close_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_close(close_in, worker).await
        }

        async fn get(accessor: &Accessor<WasiContextComponent, Self>, get_in: Vec<u8>) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_get(get_in, worker).await
        }

        async fn put(accessor: &Accessor<WasiContextComponent, Self>, put_in: Vec<u8>) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_put(put_in, worker).await
        }

        async fn delete(
            accessor: &Accessor<WasiContextComponent, Self>,
            delete_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_delete(delete_in, worker).await
        }

        async fn range(
            accessor: &Accessor<WasiContextComponent, Self>,
            range_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_range(range_in, worker).await
        }

        async fn relation_get(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_get_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_relation_get(relation_get_in, worker).await
        }

        async fn relation_update(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_update_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_relation_update(relation_update_in, worker).await
        }

        async fn relation_insert(
            accessor: &Accessor<WasiContextComponent, Self>,
            relation_insert_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_relation_insert(relation_insert_in, worker).await
        }

        async fn fs_open(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_open_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_open(fs_open_in, worker).await
        }

        async fn fs_close(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_close_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_close(fs_close_in, worker).await
        }

        async fn fs_read(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_read_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_read(fs_read_in, worker).await
        }

        async fn fs_write(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_write_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_write(fs_write_in, worker).await
        }

        async fn fs_pread(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_pread_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_pread(fs_pread_in, worker).await
        }

        async fn fs_pwrite(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_pwrite_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_pwrite(fs_pwrite_in, worker).await
        }

        async fn fs_lseek(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_lseek_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_lseek(fs_lseek_in, worker).await
        }

        async fn fs_fstat(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_fstat_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_fstat(fs_fstat_in, worker).await
        }

        async fn fs_stat(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_stat_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_stat(fs_stat_in, worker).await
        }

        async fn fs_fsync(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_fsync_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_fsync(fs_fsync_in, worker).await
        }

        async fn fs_readdir(
            accessor: &Accessor<WasiContextComponent, Self>,
            fs_readdir_in: Vec<u8>,
        ) -> Vec<u8> {
            let worker = accessor.with(|mut access| access.get().worker_local());

            async_host_fs_readdir(fs_readdir_in, worker).await
        }
    }
}
