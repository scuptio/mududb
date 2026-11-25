//! Check the exact durable record before permitting catalog side effects.
use super::*;

impl MetaStreamMachine {
    // Called while holding the apply owner. False means an exact durable retry
    // of an already applied request, requiring no additional business mutation.
    pub(super) async fn validate_record(
        &self,
        request_id: u128,
        record: &StreamStruct,
    ) -> RS<bool> {
        if record.identity != self.log.identity()
            || record.progress.stream_id != self.log.descriptor().stream_id
            || record.progress.stream_generation != self.log.descriptor().stream_generation
        {
            return Err(invalid("metadata apply identity mismatch"));
        }
        let (durable_id, durable) = self.log.record(record.progress.end_lsn).await?;
        if durable_id != request_id
            || durable.progress != record.progress
            || durable.identity != record.identity
            || durable.dependencies != record.dependencies
            || durable.payload != record.payload
        {
            return Err(invalid("metadata apply differs from its durable record"));
        }
        let previous = self.applied.lock()?.clone();
        if let Some(prefix) = &previous {
            if !self.log.contains(prefix).await? {
                return Err(invalid(
                    "metadata applied history no longer matches durable log",
                ));
            }
        }
        if previous
            .as_ref()
            .is_some_and(|p| record.progress.end_lsn <= p.end_lsn)
        {
            if self.requests.lock()?.get(&request_id) != Some(&record.progress.end_lsn) {
                return Err(invalid(
                    "metadata retry is missing its applied request identity",
                ));
            }
            return Ok(false);
        }
        let next = match previous {
            Some(prefix) => prefix
                .end_lsn
                .checked_add(1)
                .ok_or_else(|| invalid("metadata LSN exhausted"))?,
            None => 0,
        };
        if record.progress.end_lsn != next {
            return Err(invalid("metadata apply is not contiguous"));
        }
        if self.requests.lock()?.contains_key(&request_id) {
            return Err(invalid(
                "metadata request identity was already applied at another LSN",
            ));
        }
        Ok(true)
    }
}
