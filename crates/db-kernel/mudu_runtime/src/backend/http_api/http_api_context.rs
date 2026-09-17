use super::HttpApi;
use std::sync::Arc;

#[derive(Clone)]
pub(crate) struct HttpApiContext {
    pub api: Arc<dyn HttpApi>,
}

unsafe impl Send for HttpApiContext {}
unsafe impl Sync for HttpApiContext {}
