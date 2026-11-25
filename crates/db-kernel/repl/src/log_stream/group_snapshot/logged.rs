//! Preflight all logged adapters and their dependency histories before restore.
use super::*;
use crate::log_stream::LoggedStreamSnapshot;

impl GroupSnapshotBundle {
    /// Decodes every LoggedStreamAdapter envelope before any target log is
    /// modified, and proves each historical cross-stream dependency from the
    /// included frames. Raw custom adapters require their own restore codec.
    /// Business codecs must still validate their images; this is not an install
    /// transaction or authorization to open service/compact consensus history.
    pub fn decode_logged_images(
        &self,
        adapters: &AdapterCapabilities,
    ) -> RS<BTreeMap<StreamId, LoggedStreamSnapshot>> {
        self.manifest
            .validate(self.manifest.state.identity(), adapters)?;
        self.verify_images()?;
        let mut images = BTreeMap::new();
        let mut histories = BTreeMap::<StreamId, Vec<StreamProgress>>::new();
        for (id, reference) in &self.manifest.images {
            let descriptor = &self.manifest.state.streams()[id].descriptor;
            let image = LoggedStreamSnapshot::decode(
                &self.images[id],
                self.manifest.state.identity(),
                descriptor,
                reference.prefix.as_ref(),
            )?;
            let mut history = Vec::new();
            image.log().visit_records(|_, prefix| {
                history.push(prefix);
                Ok(())
            })?;
            histories.insert(*id, history);
            images.insert(*id, image);
        }
        for image in images.values() {
            image.log().visit_records(|record, _| {
                for dependency in &record.dependencies {
                    let actual = histories.get(&dependency.stream_id).and_then(|history| {
                        usize::try_from(dependency.end_lsn)
                            .ok()
                            .and_then(|i| history.get(i))
                    });
                    if actual != Some(dependency) {
                        return Err(invalid(
                            "snapshot cross-stream dependency is not included at its exact digest",
                        ));
                    }
                }
                Ok(())
            })?;
        }
        Ok(images)
    }
}
