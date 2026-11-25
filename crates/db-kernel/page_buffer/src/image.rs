//! Immutable snapshots with format-independent validation and writeback policies.
//! Policies belong to consumers; the buffer never interprets their page layout.
use crate::{invalid, RS};
use std::{
    borrow::Cow,
    fmt::Debug,
    ops::Deref,
    sync::{atomic::AtomicU8, Arc},
};
use std::{cell::Cell, rc::Rc};

/// A static validation-policy identity. Consumers must check context-dependent
/// facts (for example the expected page ID) separately on every access.
/// The nonzero-sized atomic marker gives distinct static instances distinct
/// addresses without global registration, counters, or initialization locks.
#[derive(Debug, Default)]
pub struct ImageValidator {
    _marker: AtomicU8,
}
impl ImageValidator {
    /// Creates a policy token, normally stored in a consumer-owned static.
    pub const fn new() -> Self {
        Self {
            _marker: AtomicU8::new(0),
        }
    }
}

/// Consumer-owned serialization of a resident image into a backing-store image.
/// Implementations must preserve page size and never mutate the source snapshot.
/// Errors leave the page dirty; cancellation/retry may invoke the policy again.
pub trait PageWriteback: Send + Sync + Debug {
    /// Produces a complete persistent image, including any format-specific checksum.
    fn encode(&self, bytes: &[u8]) -> RS<Vec<u8>>;
}

/// Internal ownership of a consumer policy; static policies remain allocation-free.
#[derive(Clone, Debug)]
pub(crate) enum WritebackPolicy {
    Static(&'static dyn PageWriteback),
    Shared(Arc<dyn PageWriteback>),
}
impl WritebackPolicy {
    fn encode(&self, bytes: &[u8]) -> RS<Vec<u8>> {
        match self {
            Self::Static(policy) => policy.encode(bytes),
            Self::Shared(policy) => policy.encode(bytes),
        }
    }
}

/// Immutable resident bytes. A writeback policy may materialize derived fields
/// such as checksums only when the buffer initializes or flushes a backing page.
#[derive(Clone, Debug)]
pub struct PageImage(pub(crate) Rc<ImageBytes>);
#[derive(Debug)]
pub(crate) struct ImageBytes {
    pub bytes: Box<[u8]>,
    pub validator: Cell<*mut ImageValidator>,
    pub writeback: Option<WritebackPolicy>,
}
impl Clone for ImageBytes {
    fn clone(&self) -> Self {
        Self {
            bytes: self.bytes.clone(),
            validator: Cell::new(self.validator.get()),
            writeback: self.writeback.clone(),
        }
    }
}
impl PageImage {
    /// Owns raw bytes with no validation certificate or writeback transformation.
    pub fn new(bytes: Vec<u8>) -> Self {
        Self::build(bytes, None)
    }
    /// Owns resident bytes serialized through a static consumer policy on writeback.
    pub fn with_writeback(bytes: Vec<u8>, policy: &'static dyn PageWriteback) -> Self {
        Self::build(bytes, Some(WritebackPolicy::Static(policy)))
    }
    /// Owns a consumer policy that can retain per-owner state across deferred flushes.
    pub fn with_shared_writeback(bytes: Vec<u8>, policy: Arc<dyn PageWriteback>) -> Self {
        Self::build(bytes, Some(WritebackPolicy::Shared(policy)))
    }
    fn build(bytes: Vec<u8>, writeback: Option<WritebackPolicy>) -> Self {
        Self(Rc::new(ImageBytes {
            bytes: bytes.into_boxed_slice(),
            validator: Cell::new(std::ptr::null_mut()),
            writeback,
        }))
    }
    /// Whether these immutable contents have passed this exact validation policy.
    pub fn validated_by(&self, policy: &'static ImageValidator) -> bool {
        std::ptr::eq(self.0.validator.get(), policy)
    }
    /// Structs a successful consumer validation. Only record facts established
    /// for every byte in this image; a page ID alone is not a validation proof.
    pub fn mark_validated(&self, policy: &'static ImageValidator) {
        self.0.validator.set(std::ptr::from_ref(policy).cast_mut());
    }
    /// Materializes persistent bytes without changing this resident snapshot.
    pub fn writeback_bytes(&self) -> RS<Cow<'_, [u8]>> {
        match &self.0.writeback {
            None => Ok(Cow::Borrowed(self)),
            Some(policy) => {
                let bytes = policy.encode(self)?;
                if bytes.len() != self.len() {
                    return Err(invalid("writeback changed page size"));
                }
                Ok(Cow::Owned(bytes))
            }
        }
    }
    /// Returns a weak identity without retaining page payload bytes.
    pub fn identity(&self) -> ImageIdobject {
        ImageIdobject(Rc::downgrade(&self.0))
    }
    /// Hands out the payload bytes, avoiding a copy when this is the only
    /// reference. Writers that rebuild an image from a resident snapshot use
    /// this to keep batch rewrites of one page allocation-free.
    pub fn into_owned(self) -> Vec<u8> {
        match Rc::try_unwrap(self.0) {
            Ok(bytes) => bytes.bytes.into_vec(),
            Err(shared) => shared.bytes.to_vec(),
        }
    }
}
impl Deref for PageImage {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0.bytes
    }
}
/// Weak immutable-image identity; copy-on-write edits dissociate old weak tokens.
#[derive(Clone)]
pub struct ImageIdobject(std::rc::Weak<ImageBytes>);
impl ImageIdobject {
    /// Whether this token identifies exactly this immutable snapshot.
    pub fn matches(&self, image: &PageImage) -> bool {
        self.0.ptr_eq(&Rc::downgrade(&image.0))
    }
}
