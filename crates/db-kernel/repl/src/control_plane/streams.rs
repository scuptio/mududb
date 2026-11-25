//! Stream descriptors of the unified control plane's two log streams.
//!
//! The descriptors are deterministic constants: every replica of the control
//! group opens the identical streams, so bootstrap registration is a fixed,
//! idempotent pair of registry records.

use catalog::contract::control_plane::{
    CONTROL_STREAM_CODEC_VERSION, CONTROL_STREAM_DIGEST_VERSION, CONTROL_STREAM_EPOCH,
    CONTROL_STREAM_EPOCH_KIND, CONTROL_STREAM_META, CONTROL_STREAM_META_KIND,
};
use mudu_raft::stream::{StreamBudget, StreamDescriptor, StreamId};

/// Largest catalog control record (DDL ops carry full schemas).
const META_MAX_RECORD_BYTES: u32 = 4 * 1024 * 1024;
/// Outstanding payload byte budget of the catalog stream.
const META_MAX_INFLIGHT_BYTES: u64 = 64 * 1024 * 1024;
/// Largest epoch control record (an advance is a handful of integers).
const EPOCH_MAX_RECORD_BYTES: u32 = 4096;
/// Outstanding payload byte budget of the epoch stream.
const EPOCH_MAX_INFLIGHT_BYTES: u64 = 4 * 1024 * 1024;

/// The catalog stream descriptor registered at bootstrap.
pub fn meta_stream_descriptor() -> StreamDescriptor {
    StreamDescriptor {
        stream_id: StreamId(CONTROL_STREAM_META),
        stream_generation: 1,
        kind: CONTROL_STREAM_META_KIND.to_string(),
        codec_version: CONTROL_STREAM_CODEC_VERSION,
        digest_version: CONTROL_STREAM_DIGEST_VERSION,
        budget: StreamBudget {
            max_record_bytes: META_MAX_RECORD_BYTES,
            max_inflight_bytes: META_MAX_INFLIGHT_BYTES,
            // DDL must not starve behind bulk epoch traffic.
            priority: 2,
        },
        requires: Vec::new(),
    }
}

/// The epoch stream descriptor registered at bootstrap.
pub fn epoch_stream_descriptor() -> StreamDescriptor {
    StreamDescriptor {
        stream_id: StreamId(CONTROL_STREAM_EPOCH),
        stream_generation: 1,
        kind: CONTROL_STREAM_EPOCH_KIND.to_string(),
        codec_version: CONTROL_STREAM_CODEC_VERSION,
        digest_version: CONTROL_STREAM_DIGEST_VERSION,
        budget: StreamBudget {
            max_record_bytes: EPOCH_MAX_RECORD_BYTES,
            max_inflight_bytes: EPOCH_MAX_INFLIGHT_BYTES,
            priority: 1,
        },
        requires: Vec::new(),
    }
}

/// Adapter capabilities covering exactly the two control streams.
pub fn control_adapter_capabilities() -> crate::log_stream::AdapterCapabilities {
    let mut capabilities = crate::log_stream::AdapterCapabilities::default();
    for descriptor in [meta_stream_descriptor(), epoch_stream_descriptor()] {
        capabilities.add(
            descriptor.kind.clone(),
            descriptor.codec_version,
            descriptor.digest_version,
        );
    }
    capabilities
}
