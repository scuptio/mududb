//! MBP1 policy lives entirely in the index. Resident edits are certified by their
//! checked builders; persistent snapshots are sealed only on initialization or
//! writeback. Newly read raw images never inherit a resident certificate.
use super::diagnostics::Diagnostics;
use super::{codec, invalid, view, RS};
use page_buffer::{ImageValidator, PageImage, PageWriteback};
use std::sync::{Arc, OnceLock};

pub(super) static VALIDATOR: ImageValidator = ImageValidator::new();
#[cfg(test)]
pub(super) static WRITEBACK: IndexWriteback = IndexWriteback {
    diagnostics: OnceLock::new(),
};
#[derive(Debug, Default)]
pub(super) struct IndexWriteback {
    pub(super) diagnostics: OnceLock<Arc<Diagnostics>>,
}
impl PageWriteback for IndexWriteback {
    fn encode(&self, bytes: &[u8]) -> RS<Vec<u8>> {
        if bytes.len() < codec::HEADER {
            return Err(invalid("truncated writeback image"));
        }
        let diagnostics = self.diagnostics.get().map(Arc::as_ref);
        let mut encoded = bytes.to_vec();
        if let Some(diagnostics) = diagnostics {
            diagnostics.record_writeback_copy(encoded.len());
        }
        codec::seal_observed(&mut encoded, diagnostics);
        Ok(encoded)
    }
}
/// Only checked index encoders/patch builders may certify newly created bytes.
#[cfg(test)]
pub(super) fn resident(bytes: Vec<u8>) -> PageImage {
    let image = PageImage::with_writeback(bytes, &WRITEBACK);
    image.mark_validated(&VALIDATOR);
    image
}
#[cfg(test)]
pub(super) fn validate(image: &PageImage, size: usize, pid: u64) -> RS<()> {
    validate_observed(image, size, pid, None)
}
pub(super) fn validate_observed(
    image: &PageImage,
    size: usize,
    pid: u64,
    diagnostics: Option<&Diagnostics>,
) -> RS<()> {
    // Context-dependent facts are not part of the shared format certificate.
    if image.len() != size || size < codec::HEADER || codec::u64_at(image, 48) != pid {
        return Err(invalid("wrong page identity or size"));
    }
    if !image.validated_by(&VALIDATOR) {
        codec::validate_image_observed(image, size, pid, diagnostics)?;
        view::validate_layout(image)?;
        image.mark_validated(&VALIDATOR);
    }
    Ok(())
}
