use crate::server::message_bus_api::ServerInstanceId;
use crate::server::routing::RoutingMode;
use crate::storage::page::page_block_ref::DEFAULT_PAGE_SIZE;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::oid::gen_oid;

/// Configuration shared by both execution paths of the `client` backend.
///
/// The same configuration is consumed by both the io_uring worker-ring backend
/// and the Tokio backend so they keep the worker model and protocol surface
/// aligned.
#[derive(Debug)]
pub struct ServerCfg {
    server_instance_id: ServerInstanceId,
    worker_count: usize,
    listen_ip: String,
    listen_port: u16,
    multi_port: bool,
    data_dir: String,
    log_dir: String,
    log_chunk_size: u64,
    routing_mode: RoutingMode,
    page_size: usize,
}

impl ServerCfg {
    /// Creates a backend configuration.
    ///
    /// The resulting value can be used by both the io_uring and Tokio TCP
    /// backends with the same externally visible behavior.
    ///
    /// The page size defaults to [`DEFAULT_PAGE_SIZE`] and can be changed with
    /// [`Self::with_page_size`] before the database directory is created.
    pub fn new(
        worker_count: usize,
        listen_ip: String,
        listen_port: u16,
        data_dir: String,
        log_dir: String,
        routing_mode: RoutingMode,
    ) -> RS<Self> {
        Ok(Self {
            server_instance_id: gen_oid(),
            worker_count,
            listen_ip,
            listen_port,
            multi_port: false,
            data_dir,
            log_dir,
            log_chunk_size: 64 * 1024 * 1024,
            routing_mode,
            page_size: DEFAULT_PAGE_SIZE,
        })
    }

    pub fn with_log_chunk_size(mut self, log_chunk_size: u64) -> Self {
        self.log_chunk_size = log_chunk_size;
        self
    }

    pub fn with_multi_port(mut self, multi_port: bool) -> Self {
        self.multi_port = multi_port;
        self
    }

    /// Overrides the database page size.
    ///
    /// `page_size` must be a power of two and at least `DEFAULT_PAGE_SIZE`.
    /// This value is a `Persistent` config: changing it for an existing data
    /// directory requires a migration tool.
    pub fn with_page_size(mut self, page_size: usize) -> RS<Self> {
        if page_size < DEFAULT_PAGE_SIZE {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!(
                    "page_size {} is below minimum {}",
                    page_size, DEFAULT_PAGE_SIZE
                )
            ));
        }
        if !page_size.is_power_of_two() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("page_size {} is not a power of two", page_size)
            ));
        }
        self.page_size = page_size;
        Ok(self)
    }

    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn listen_ip(&self) -> &str {
        &self.listen_ip
    }

    pub fn listen_port(&self) -> u16 {
        self.listen_port
    }

    pub fn multi_port(&self) -> bool {
        self.multi_port
    }

    pub fn listen_port_for_worker(&self, worker_index: usize) -> RS<u16> {
        if !self.multi_port {
            return Ok(self.listen_port);
        }
        let worker_offset = u16::try_from(worker_index).map_err(|_| {
            mudu_error!(
                ErrorCode::Parse,
                format!("worker index too large for port mapping: {}", worker_index)
            )
        })?;
        self.listen_port.checked_add(worker_offset).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "worker listen port overflow: base_port={}, worker_index={}",
                    self.listen_port, worker_index
                )
            )
        })
    }

    pub fn log_dir(&self) -> &str {
        &self.log_dir
    }

    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    pub fn log_chunk_size(&self) -> u64 {
        self.log_chunk_size
    }

    pub fn routing_mode(&self) -> RoutingMode {
        self.routing_mode
    }

    pub fn page_size(&self) -> usize {
        self.page_size
    }
}
