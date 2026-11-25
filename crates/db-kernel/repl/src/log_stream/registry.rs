//! Registry mutation validation, deduplication and recovery checks.

use super::state::{
    invalid, AdapterCapabilities, GroupState, RegisteredStream, RegistryReceipt, StreamLifecycle,
};
use mudu::common::result::RS;
use mudu_raft::stream::{
    RegistryChange, RegistryOp, StreamDescriptor, StreamId, REGISTRY_FORMAT_VERSION,
};
use std::collections::BTreeSet;

const MAX_STREAMS: usize = 4096;

impl GroupState {
    pub(super) fn apply_registry(
        &mut self,
        change: &RegistryChange,
        adapters: &AdapterCapabilities,
    ) -> RS<()> {
        if change.format_version != REGISTRY_FORMAT_VERSION
            || change.identity != self.identity
            || change.request_id == 0
        {
            return Err(invalid(
                "invalid registry format, group generation or request id",
            ));
        }
        if self
            .receipt(change.request_id, &change.operation)?
            .is_some()
        {
            return Ok(());
        }
        if change.previous_version != self.registry_version {
            return Err(invalid("stale registry mutation"));
        }
        let version = self
            .registry_version
            .checked_add(1)
            .ok_or_else(|| invalid("registry version exhausted"))?;
        match &change.operation {
            RegistryOp::Register(descriptor) => {
                validate_descriptor(descriptor, adapters)?;
                if self.streams.len() >= MAX_STREAMS
                    || self.streams.contains_key(&descriptor.stream_id)
                {
                    return Err(invalid(
                        "stream registry is full or stream id was already used",
                    ));
                }
                for dependency in &descriptor.requires {
                    if self
                        .streams
                        .get(dependency)
                        .is_none_or(|s| s.lifecycle != StreamLifecycle::Active)
                    {
                        return Err(invalid("stream dependency is not active"));
                    }
                }
                self.streams.insert(
                    descriptor.stream_id,
                    RegisteredStream {
                        descriptor: descriptor.clone(),
                        lifecycle: StreamLifecycle::Active,
                    },
                );
            }
            RegistryOp::Deactivate {
                stream_id,
                stream_generation,
            } => {
                let stream = self.stream_for_change(*stream_id, *stream_generation)?;
                if stream.lifecycle != StreamLifecycle::Active {
                    return Err(invalid("stream is not active"));
                }
                self.streams
                    .get_mut(stream_id)
                    .ok_or_else(|| invalid("unknown stream"))?
                    .lifecycle = StreamLifecycle::Draining;
            }
            RegistryOp::Remove {
                stream_id,
                stream_generation,
            } => {
                let stream = self.stream_for_change(*stream_id, *stream_generation)?;
                if stream.lifecycle != StreamLifecycle::Draining
                    || self.committed.contains_key(stream_id)
                {
                    return Err(invalid("stream must be drained and empty; nonempty retirement needs data/GC evidence"));
                }
                if self.streams.values().any(|s| {
                    s.lifecycle != StreamLifecycle::Tombstoned
                        && s.descriptor.requires.contains(stream_id)
                }) {
                    return Err(invalid("stream is still referenced by another stream"));
                }
                self.streams
                    .get_mut(stream_id)
                    .ok_or_else(|| invalid("unknown stream"))?
                    .lifecycle = StreamLifecycle::Tombstoned;
            }
        }
        self.registry_version = version;
        self.receipts.insert(
            change.request_id,
            RegistryReceipt {
                operation: change.operation.clone(),
                version,
            },
        );
        Ok(())
    }

    fn stream_for_change(&self, id: StreamId, generation: u64) -> RS<&RegisteredStream> {
        let stream = self
            .streams
            .get(&id)
            .ok_or_else(|| invalid("unknown stream"))?;
        if stream.descriptor.stream_generation != generation {
            return Err(invalid("stale stream generation"));
        }
        Ok(stream)
    }

    /// Returns the original registry version for an idempotent retry, rejecting
    /// request-id reuse for a different operation. Receipts survive checkpoints.
    pub fn receipt(&self, request_id: u128, operation: &RegistryOp) -> RS<Option<u64>> {
        match self.receipts.get(&request_id) {
            Some(receipt) if receipt.operation == *operation => Ok(Some(receipt.version)),
            Some(_) => Err(invalid(
                "registry request id reused for a different operation",
            )),
            None => Ok(None),
        }
    }

    /// Validates a recovered checkpoint against the locally available adapters.
    /// No unknown stream is silently skipped, including retained tombstones.
    pub fn validate(&self, adapters: &AdapterCapabilities) -> RS<()> {
        if self.identity.generation == 0
            || self.streams.len() > MAX_STREAMS
            || self.receipts.len() > MAX_STREAMS * 3
            || self.cursor.index < self.registry_version
            || (self.cursor.index == 0) != (self.cursor.term == 0)
            || self.receipts.len() as u64 != self.registry_version
        {
            return Err(invalid("invalid registry checkpoint bounds"));
        }
        for (id, stream) in &self.streams {
            if *id != stream.descriptor.stream_id {
                return Err(invalid("checkpoint stream identity mismatch"));
            }
            validate_descriptor(&stream.descriptor, adapters)?;
            for dependency in &stream.descriptor.requires {
                let target = self
                    .streams
                    .get(dependency)
                    .ok_or_else(|| invalid("checkpoint dependency is missing"))?;
                if stream.lifecycle != StreamLifecycle::Tombstoned
                    && target.lifecycle == StreamLifecycle::Tombstoned
                {
                    return Err(invalid("live stream depends on a tombstone"));
                }
            }
        }
        validate_dependency_graph(self)?;
        let versions: BTreeSet<_> = self
            .receipts
            .values()
            .map(|receipt| receipt.version)
            .collect();
        if versions.len() != self.receipts.len()
            || versions.iter().copied().ne(1..=self.registry_version)
        {
            return Err(invalid("registry receipt history has gaps or duplicates"));
        }
        self.validate_registry_history(adapters)?;
        for (id, progress) in &self.committed {
            if *id != progress.stream_id {
                return Err(invalid("checkpoint progress identity mismatch"));
            }
            self.validate_progress(progress)?;
        }
        Ok(())
    }
    fn validate_registry_history(&self, adapters: &AdapterCapabilities) -> RS<()> {
        let mut ordered: Vec<_> = self.receipts.iter().collect();
        ordered.sort_by_key(|(_, receipt)| receipt.version);
        let mut rebuilt = GroupState::new(self.identity)?;
        for (request_id, receipt) in ordered {
            rebuilt.apply_registry(
                &RegistryChange {
                    format_version: REGISTRY_FORMAT_VERSION,
                    identity: self.identity,
                    request_id: *request_id,
                    previous_version: rebuilt.registry_version,
                    operation: receipt.operation.clone(),
                },
                adapters,
            )?;
        }
        if rebuilt.streams != self.streams {
            return Err(invalid(
                "checkpoint registry differs from its durable receipt history",
            ));
        }
        Ok(())
    }
}

fn validate_descriptor(descriptor: &StreamDescriptor, adapters: &AdapterCapabilities) -> RS<()> {
    let unique: BTreeSet<_> = descriptor.requires.iter().copied().collect();
    if descriptor.stream_generation == 0
        || descriptor.kind.trim().is_empty()
        || descriptor.kind.len() > 128
        || descriptor.codec_version == 0
        || descriptor.digest_version == 0
        || descriptor.budget.max_record_bytes == 0
        || descriptor.budget.max_inflight_bytes < descriptor.budget.max_record_bytes as u64
        || descriptor.requires.len() > 64
        || unique.len() != descriptor.requires.len()
        || unique.contains(&descriptor.stream_id)
    {
        return Err(invalid("invalid stream descriptor or resource budget"));
    }
    if !adapters.supports(descriptor) {
        return Err(invalid("unsupported stream kind, codec or digest version"));
    }
    Ok(())
}

fn validate_dependency_graph(state: &GroupState) -> RS<()> {
    let mut complete = BTreeSet::new();
    let mut visiting = BTreeSet::new();
    for id in state.streams.keys() {
        let mut stack = vec![(*id, false)];
        while let Some((current, exit)) = stack.pop() {
            if exit {
                visiting.remove(&current);
                complete.insert(current);
                continue;
            }
            if complete.contains(&current) {
                continue;
            }
            if !visiting.insert(current) {
                return Err(invalid("cyclic stream dependencies"));
            }
            stack.push((current, true));
            let stream = state
                .streams
                .get(&current)
                .ok_or_else(|| invalid("missing stream dependency"))?;
            for dependency in &stream.descriptor.requires {
                stack.push((*dependency, false));
            }
        }
    }
    Ok(())
}
