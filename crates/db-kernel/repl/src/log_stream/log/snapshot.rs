//! Full-prefix log images preserve retry identities and exact retry contents.
use super::*;
use std::collections::BTreeSet;

pub(crate) const MAX_SNAPSHOT_BYTES: usize = 64 * 1024 * 1024;

/// Immutable complete external-log prefix. The initial format retains every
/// committed frame; it rejects compacted history rather than losing retry keys.
/// Not a consensus snapshot or permission to admit requests after restoration.
#[derive(Clone, Serialize, Deserialize)]
pub struct StreamLogSnapshot {
    version: u32,
    identity: GroupIdobject,
    descriptor: StreamDescriptor,
    prefix: Option<StreamProgress>,
    frames: Vec<Vec<u8>>,
}

impl StreamLogSnapshot {
    /// Exact inclusive prefix, or None for an empty committed stream.
    pub fn prefix(&self) -> Option<&StreamProgress> {
        self.prefix.as_ref()
    }

    /// Validates all bytes and retry identities against the caller's authority.
    /// No file is changed. Digest checks detect accidental corruption only.
    pub fn validate_for(
        &self,
        identity: GroupIdobject,
        descriptor: &StreamDescriptor,
        prefix: Option<&StreamProgress>,
    ) -> RS<()> {
        if self.version != 1
            || self.identity != identity
            || self.descriptor != *descriptor
            || self.prefix.as_ref() != prefix
            || identity.generation == 0
        {
            return Err(invalid("log snapshot identity, format or cut mismatch"));
        }
        let count = match prefix {
            Some(p) => p
                .end_lsn
                .checked_add(1)
                .ok_or_else(|| invalid("snapshot LSN exhausted"))?,
            None => 0,
        };
        if self.frames.len() as u64 != count {
            return Err(invalid("log snapshot prefix has gaps or extra frames"));
        }
        let mut digest = ZERO_SEG_HASH;
        let mut requests = BTreeSet::new();
        let mut bytes = 0usize;
        for (lsn, frame) in self.frames.iter().enumerate() {
            bytes = bytes
                .checked_add(frame.len())
                .ok_or_else(|| invalid("snapshot size overflow"))?;
            if bytes > MAX_SNAPSHOT_BYTES {
                return Err(invalid("log snapshot exceeds size budget"));
            }
            let body = decode_snapshot_frame(identity, descriptor, frame)?;
            if frame_lsn(frame)?.as_u64() != lsn as u64 || !requests.insert(body.request_id) {
                return Err(invalid(
                    "log snapshot contains out-of-order or duplicate requests",
                ));
            }
            digest = seg_hash_chain(&digest, std::slice::from_ref(frame));
        }
        if let Some(prefix) = prefix {
            if prefix.stream_id != descriptor.stream_id
                || prefix.stream_generation != descriptor.stream_generation
                || prefix.segment_digest != digest
            {
                return Err(invalid("log snapshot digest disagrees with committed cut"));
            }
        }
        Ok(())
    }
}

impl DurableStreamLog {
    /// Captures only the requested prefix, excluding any speculative tail.
    /// The log owner is held through all reads. Full retry history is required;
    /// a reclaimed prefix must use a future compacted retry-index image format.
    pub async fn capture_snapshot(&self, prefix: Option<&StreamProgress>) -> RS<StreamLogSnapshot> {
        let index = self.index.lock().await;
        healthy(&index)?;
        if index.base_lsn != 0 {
            return Err(invalid("cannot snapshot reclaimed retry history"));
        }
        if let Some(prefix) = prefix {
            if usize::try_from(prefix.end_lsn)
                .ok()
                .and_then(|lsn| index.progress.get(lsn))
                != Some(prefix)
            {
                return Err(invalid("snapshot prefix is not retained durable history"));
            }
        }
        let count = prefix.map_or(0, |p| p.end_lsn + 1);
        let mut frames = Vec::new();
        let mut size = 0usize;
        for lsn in 0..count {
            let frame = self.frame_at(&index, lsn).await?;
            size = size
                .checked_add(frame.len())
                .ok_or_else(|| invalid("snapshot size overflow"))?;
            if size > MAX_SNAPSHOT_BYTES {
                return Err(invalid("log snapshot exceeds size budget"));
            }
            frames.push(frame);
        }
        let image = StreamLogSnapshot {
            version: 1,
            identity: self.identity,
            descriptor: self.descriptor.clone(),
            prefix: prefix.cloned(),
            frames,
        };
        image.validate_for(self.identity, &self.descriptor, prefix)?;
        Ok(image)
    }

    /// Restores an offline log, including request-id dedup and digest history.
    /// Accepts only an empty log or an identical partial prefix from an earlier
    /// interrupted restore. Validates the entire image and existing prefix
    /// before the first write. Never truncates a conflicting or newer log.
    /// On I/O failure/cancellation call drain, then reopen and retry; business and Raft
    /// recovery must finish separately before the caller admits requests.
    pub async fn restore_snapshot(&self, image: &StreamLogSnapshot) -> RS<()> {
        image.validate_for(self.identity, &self.descriptor, image.prefix())?;
        let mut index = self.index.lock().await;
        healthy(&index)?;
        if index.base_lsn != 0 || index.progress.len() > image.frames.len() {
            return Err(invalid("snapshot target has reclaimed or newer history"));
        }
        for (lsn, frame) in image.frames.iter().enumerate().take(index.progress.len()) {
            if self.frame_at(&index, lsn as u64).await? != *frame {
                return Err(invalid("snapshot target has conflicting history"));
            }
        }
        for frame in &image.frames[index.progress.len()..] {
            self.persist(&mut index, frame).await?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;

impl StreamLogSnapshot {
    pub(crate) fn visit_records(
        &self,
        mut visit: impl FnMut(&StreamEnvelope, StreamProgress) -> RS<()>,
    ) -> RS<()> {
        let mut digest = ZERO_SEG_HASH;
        for (lsn, frame) in self.frames.iter().enumerate() {
            let body = decode_snapshot_frame(self.identity, &self.descriptor, frame)?;
            digest = seg_hash_chain(&digest, std::slice::from_ref(frame));
            visit(
                &body,
                StreamProgress {
                    stream_id: self.descriptor.stream_id,
                    stream_generation: self.descriptor.stream_generation,
                    end_lsn: lsn as u64,
                    segment_digest: digest,
                },
            )?;
        }
        Ok(())
    }
}
