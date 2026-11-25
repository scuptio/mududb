use super::*;
use mudu::{error::ErrorCode, mudu_error};

#[derive(Serialize, Deserialize)]
struct Envelope {
    bytes: Vec<u8>,
    crc64: u64,
}
const MAX_MANIFEST_BYTES: u64 = 64 * 1024 * 1024;

impl GroupSnapshotBundle {
    /// Publishes images first and the checksummed manifest last, with fsync of
    /// files and directory entries. Caller must exclusively own this version
    /// directory. A sealed bundle is immutable; retry only accepts identical data.
    pub async fn publish(&self, dir: &Path, adapters: &AdapterCapabilities) -> RS<()> {
        self.manifest
            .validate(self.manifest.state.identity(), adapters)?;
        self.verify_images()?;
        let fs = mudu_sys::fs::tokio_fs();
        let bytes = encode(&self.manifest)?;
        if fs.path_exists(&dir.join(MANIFEST)).await? {
            let existing = Self::read(dir, self.manifest.state.identity(), adapters).await?;
            if encode(&existing.manifest)? != bytes || existing.images != self.images {
                return Err(invalid("refusing to replace a sealed group snapshot"));
            }
            return Ok(());
        }
        crate::durable_io::ensure_dir(fs.as_ref(), dir).await?;
        for (id, image) in &self.manifest.images {
            crate::durable_io::publish(fs.as_ref(), &dir.join(&image.file), &self.images[id])
                .await?;
        }
        crate::durable_io::publish(fs.as_ref(), &dir.join(MANIFEST), &bytes).await
    }

    /// Loads and verifies the complete bundle before exposing any image. Unknown
    /// adapters, changed generations, missing images and checksum failures reject
    /// recovery; no individual stream is silently skipped.
    pub async fn read(
        dir: &Path,
        identity: GroupIdobject,
        adapters: &AdapterCapabilities,
    ) -> RS<Self> {
        let fs = mudu_sys::fs::tokio_fs();
        let path = dir.join(MANIFEST);
        if fs.metadata_len(&path).await? > MAX_MANIFEST_BYTES {
            return Err(invalid("snapshot manifest exceeds budget"));
        }
        let envelope: Envelope =
            rmp_serde::from_slice(&fs.read_all(&path).await?).map_err(|e| {
                mudu_error!(
                    ErrorCode::CorruptedData,
                    "decode group snapshot envelope",
                    e
                )
            })?;
        if crc64(&envelope.bytes) != envelope.crc64 {
            return Err(invalid("group snapshot manifest checksum mismatch"));
        }
        let manifest: GroupSnapshotManifest =
            rmp_serde::from_slice(&envelope.bytes).map_err(|e| {
                mudu_error!(
                    ErrorCode::CorruptedData,
                    "decode group snapshot manifest",
                    e
                )
            })?;
        manifest.validate(identity, adapters)?;
        let mut images = BTreeMap::new();
        for (id, image) in &manifest.images {
            let path = dir.join(&image.file);
            if fs.metadata_len(&path).await? != image.len {
                return Err(invalid("group snapshot image size mismatch"));
            }
            images.insert(*id, fs.read_all(&path).await?);
        }
        let bundle = Self { manifest, images };
        bundle.verify_images()?;
        Ok(bundle)
    }

    pub(super) fn verify_images(&self) -> RS<()> {
        if self.images.len() != self.manifest.images.len() {
            return Err(invalid("snapshot image coverage mismatch"));
        }
        for (id, image) in &self.manifest.images {
            let bytes = self
                .images
                .get(id)
                .ok_or_else(|| invalid("missing snapshot image"))?;
            if bytes.len() as u64 != image.len || crc64(bytes) != image.crc64 {
                return Err(invalid("group snapshot image checksum mismatch"));
            }
        }
        Ok(())
    }
}

fn encode(manifest: &GroupSnapshotManifest) -> RS<Vec<u8>> {
    let bytes = rmp_serde::to_vec(manifest)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode group manifest", e))?;
    let crc64 = crc64(&bytes);
    let bytes = rmp_serde::to_vec(&Envelope { bytes, crc64 })
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode group manifest envelope", e))?;
    if bytes.len() as u64 > MAX_MANIFEST_BYTES {
        return Err(invalid("snapshot manifest exceeds budget"));
    }
    Ok(bytes)
}

#[derive(Serialize, Deserialize)]
struct BundleBlob {
    version: u32,
    manifest: GroupSnapshotManifest,
    images: BTreeMap<StreamId, Vec<u8>>,
}

const MAX_BLOB_BYTES: u64 = MAX_BUNDLE_BYTES + MAX_MANIFEST_BYTES;

impl GroupSnapshotBundle {
    /// Serializes the complete bundle (manifest plus every image) as one
    /// checksummed transport blob. Shipped to a RaftFollower whose stream prefix
    /// fell below the retained frontier; not a consensus snapshot.
    pub fn encode(&self) -> RS<Vec<u8>> {
        self.verify_images()?;
        let payload = rmp_serde::to_vec(&BundleBlob {
            version: 1,
            manifest: self.manifest.clone(),
            images: self.images.clone(),
        })
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode group snapshot bundle", e))?;
        let checksum = crc64(&payload);
        let bytes = rmp_serde::to_vec(&Envelope {
            bytes: payload,
            crc64: checksum,
        })
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode group snapshot envelope", e))?;
        if bytes.len() as u64 > MAX_BLOB_BYTES {
            return Err(invalid(
                "group snapshot bundle exceeds the transport budget",
            ));
        }
        Ok(bytes)
    }

    /// Decodes and validates a transport blob before any image is exposed. A
    /// checksum mismatch, foreign identity, changed adapter codec or a broken
    /// image rejects recovery; no individual stream is silently skipped.
    pub fn decode(
        bytes: &[u8],
        identity: GroupIdobject,
        adapters: &AdapterCapabilities,
    ) -> RS<Self> {
        if bytes.len() as u64 > MAX_BLOB_BYTES {
            return Err(invalid(
                "group snapshot bundle exceeds the transport budget",
            ));
        }
        let envelope: Envelope = rmp_serde::from_slice(bytes).map_err(|e| {
            mudu_error!(
                ErrorCode::CorruptedData,
                "decode group snapshot envelope",
                e
            )
        })?;
        if crc64(&envelope.bytes) != envelope.crc64 {
            return Err(invalid("group snapshot bundle checksum mismatch"));
        }
        let blob: BundleBlob = rmp_serde::from_slice(&envelope.bytes).map_err(|e| {
            mudu_error!(ErrorCode::CorruptedData, "decode group snapshot bundle", e)
        })?;
        if blob.version != 1 {
            return Err(invalid("unsupported group snapshot bundle version"));
        }
        let bundle = Self {
            manifest: blob.manifest,
            images: blob.images,
        };
        bundle.manifest.validate(identity, adapters)?;
        bundle.verify_images()?;
        Ok(bundle)
    }
}
