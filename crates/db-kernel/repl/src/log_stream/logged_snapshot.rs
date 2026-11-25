//! Binds business state to the external records needed for durable retries.
use super::{state::invalid, StreamLogSnapshot};
use mudu::common::result::RS;
use mudu_raft::stream::{GroupIdobject, StreamDescriptor, StreamProgress};
use serde::{Deserialize, Serialize};

const MAX_BYTES: usize = 64 * 1024 * 1024;

/// Snapshot envelope emitted by LoggedStreamAdapter. Both business state and
/// external retry history must be restored before admitting client requests.
#[derive(Serialize, Deserialize)]
pub struct LoggedStreamSnapshot {
    version: u32,
    log: StreamLogSnapshot,
    business: Vec<u8>,
}
impl LoggedStreamSnapshot {
    pub(super) fn encode(log: StreamLogSnapshot, business: Vec<u8>) -> RS<Vec<u8>> {
        if business.len() > MAX_BYTES {
            return Err(invalid("business snapshot exceeds size budget"));
        }
        let bytes = super::frontier::encode(&Self {
            version: 1,
            log,
            business,
        })?;
        if bytes.len() > MAX_BYTES {
            return Err(invalid("logged snapshot exceeds size budget"));
        }
        Ok(bytes)
    }

    /// Decodes and verifies the log history against the enclosing manifest.
    /// The business codec must separately validate its own image before restore.
    pub fn decode(
        bytes: &[u8],
        identity: GroupIdobject,
        descriptor: &StreamDescriptor,
        prefix: Option<&StreamProgress>,
    ) -> RS<Self> {
        if bytes.len() > MAX_BYTES {
            return Err(invalid("logged snapshot exceeds size budget"));
        }
        let image: Self = super::frontier::decode(bytes)?;
        if image.version != 1 {
            return Err(invalid("unsupported logged snapshot version"));
        }
        image.log.validate_for(identity, descriptor, prefix)?;
        Ok(image)
    }
    /// Full committed log prefix, including stable request IDs and payloads.
    pub fn log(&self) -> &StreamLogSnapshot {
        &self.log
    }
    /// Opaque business image at exactly the manifest's applied prefix.
    pub fn business(&self) -> &[u8] {
        &self.business
    }
}
