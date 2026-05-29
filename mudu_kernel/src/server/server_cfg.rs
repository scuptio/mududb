use crate::server::message_bus_api::ServerInstanceId;
use crate::server::routing::RoutingMode;
use mudu::common::id::gen_oid;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

/// Configuration shared by both execution paths of the `client` backend.
///
/// The same configuration is consumed by both the io_uring worker-ring backend
/// and the Tokio backend so they keep the worker model and protocol surface
/// aligned.
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
}

impl ServerCfg {
    /// Creates a backend configuration.
    ///
    /// The resulting value can be used by both the io_uring and Tokio TCP
    /// backends with the same externally visible behavior.
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
            m_error!(
                EC::ParseErr,
                format!("worker index too large for port mapping: {}", worker_index)
            )
        })?;
        self.listen_port.checked_add(worker_offset).ok_or_else(|| {
            m_error!(
                EC::ParseErr,
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
}
