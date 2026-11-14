//! Transactional fixed-size byte edits. Preparation validates every range and
//! performs any required COW allocation; publication cannot fail or suspend.
use crate::{image::WritebackPolicy, invalid, ImageValidator, PageWriteGuard, PageWriteback, RS};
use std::rc::Rc;
use std::{ops::Range, sync::Arc};

enum Change {
    Small {
        at: usize,
        bytes: [u8; 16],
        len: usize,
    },
    Bytes {
        at: usize,
        bytes: Vec<u8>,
    },
    Copy {
        source: Range<usize>,
        destination: usize,
    },
    Fill {
        range: Range<usize>,
        byte: u8,
    },
}
/// A format-independent edit program. Four small changes require no heap storage;
/// larger records/programs allocate during preparation, never during publication.
#[derive(Default)]
pub struct PageEdit {
    first: [Option<Change>; 4],
    rest: Vec<Change>,
    count: usize,
}
impl PageEdit {
    /// Creates an empty edit program.
    pub fn new() -> Self {
        Self::default()
    }
    fn push(&mut self, change: Change) {
        if self.count < self.first.len() {
            self.first[self.count] = Some(change);
        } else {
            self.rest.push(change);
        }
        self.count += 1;
    }
    /// Writes bytes at an absolute offset. Small fixed-width fields stay inline.
    pub fn write(&mut self, at: usize, bytes: &[u8]) {
        if bytes.len() <= 16 {
            let mut small = [0; 16];
            small[..bytes.len()].copy_from_slice(bytes);
            self.push(Change::Small {
                at,
                bytes: small,
                len: bytes.len(),
            });
        } else {
            self.push(Change::Bytes {
                at,
                bytes: bytes.to_vec(),
            });
        }
    }
    /// Transfers a prepared payload without copying it into a second allocation.
    pub fn write_owned(&mut self, at: usize, bytes: Vec<u8>) {
        if bytes.len() <= 16 {
            self.write(at, &bytes);
        } else {
            self.push(Change::Bytes { at, bytes });
        }
    }
    /// Moves a possibly overlapping range, like slice::copy_within.
    pub fn copy_within(&mut self, source: Range<usize>, destination: usize) {
        self.push(Change::Copy {
            source,
            destination,
        });
    }
    /// Fills a range with one byte.
    pub fn fill(&mut self, range: Range<usize>, byte: u8) {
        self.push(Change::Fill { range, byte });
    }
    /// Total copy-within traffic described by this program (excluding writes/fills).
    pub fn moved_bytes(&self) -> usize {
        self.changes()
            .map(|change| match change {
                Change::Copy { source, .. } => source.end.saturating_sub(source.start),
                _ => 0,
            })
            .sum()
    }
    fn changes(&self) -> impl Iterator<Item = &Change> {
        self.first.iter().flatten().chain(self.rest.iter())
    }
    fn validate(&self, size: usize) -> RS<()> {
        let fits = |at: usize, len: usize| at.checked_add(len).is_some_and(|end| end <= size);
        for change in self.changes() {
            let valid = match change {
                Change::Small { at, len, .. } => fits(*at, *len),
                Change::Bytes { at, bytes } => fits(*at, bytes.len()),
                Change::Copy {
                    source,
                    destination,
                } => {
                    source.start <= source.end
                        && source.end <= size
                        && fits(*destination, source.end - source.start)
                }
                Change::Fill { range, .. } => range.start <= range.end && range.end <= size,
            };
            if !valid {
                return Err(invalid("page edit outside fixed-size image"));
            }
        }
        Ok(())
    }
    /// Materializes the program into a private copy, for consumers that prepare
    /// complete replacement images instead of publishing through a page latch.
    pub fn apply_to_copy(&self, bytes: &[u8]) -> RS<Vec<u8>> {
        self.validate(bytes.len())?;
        let mut copy = bytes.to_vec();
        self.apply(&mut copy);
        Ok(copy)
    }
    fn apply(&self, bytes: &mut [u8]) {
        for change in self.changes() {
            match change {
                Change::Small {
                    at,
                    bytes: data,
                    len,
                } => bytes[*at..*at + *len].copy_from_slice(&data[..*len]),
                Change::Bytes { at, bytes: data } => {
                    bytes[*at..*at + data.len()].copy_from_slice(data)
                }
                Change::Copy {
                    source,
                    destination,
                } => bytes.copy_within(source.clone(), *destination),
                Change::Fill { range, byte } => bytes[range.clone()].fill(*byte),
            }
        }
    }
}
// Borrow the desired policy through preparation. Publication clones a shared
// handle only when replacing a different policy, not on every resident update.
enum EditWriteback<'a> {
    Static(Option<&'static dyn PageWriteback>),
    Shared(&'a Arc<dyn PageWriteback>),
}
impl EditWriteback<'_> {
    fn publish(&self, current: &mut Option<WritebackPolicy>) {
        match self {
            Self::Static(policy) => *current = policy.map(WritebackPolicy::Static),
            Self::Shared(policy) => {
                if !matches!(current, Some(WritebackPolicy::Shared(old)) if Arc::ptr_eq(old, policy))
                {
                    *current = Some(WritebackPolicy::Shared(Arc::clone(policy)));
                }
            }
        }
    }
}
impl<'a> PageWriteGuard<'a> {
    /// Validates the complete edit and makes the image uniquely owned before any
    /// byte changes. Existing snapshots trigger COW and remain immutable. A failed
    /// or dropped preparation leaves visible bytes and dirty epochs unchanged.
    /// The consumer may supply a certificate for the resulting format and a
    /// writeback policy; raw edits pass None and invalidate prior validation.
    pub fn prepare_edit(
        self,
        edit: PageEdit,
        validator: Option<&'static ImageValidator>,
        writeback: Option<&'static dyn PageWriteback>,
    ) -> RS<PreparedEdit<'a>> {
        self.prepare_edit_policy(edit, validator, EditWriteback::Static(writeback))
    }

    /// Uses a shared consumer policy without cloning its handle on each update.
    pub fn prepare_edit_with_shared_writeback(
        self,
        edit: PageEdit,
        validator: Option<&'static ImageValidator>,
        writeback: &'a Arc<dyn PageWriteback>,
    ) -> RS<PreparedEdit<'a>> {
        self.prepare_edit_policy(edit, validator, EditWriteback::Shared(writeback))
    }

    fn prepare_edit_policy(
        mut self,
        edit: PageEdit,
        validator: Option<&'static ImageValidator>,
        writeback: EditWriteback<'a>,
    ) -> RS<PreparedEdit<'a>> {
        edit.validate(self.guard.len())?;
        let previous = self.guard.as_ptr();
        Rc::make_mut(&mut self.guard.0);
        // Pointer comparison observes the actual COW result even if another
        // snapshot drops concurrently; weak-token dissociation moves no bytes.
        let copied = self.guard.as_ptr() != previous;
        Ok(PreparedEdit {
            guard: self,
            edit,
            validator,
            writeback,
            copied,
            published: false,
        })
    }
}
/// Prepared single-page publication. Retains the exclusive latch after publish.
pub struct PreparedEdit<'a> {
    guard: PageWriteGuard<'a>,
    edit: PageEdit,
    validator: Option<&'static ImageValidator>,
    writeback: EditWriteback<'a>,
    copied: bool,
    published: bool,
}
impl PreparedEdit<'_> {
    /// Whether preparation copied page bytes to preserve an outstanding snapshot.
    pub fn copied(&self) -> bool {
        self.copied
    }
    /// Byte payload copied during preparation; weak-only identity changes copy zero.
    pub fn copied_bytes(&self) -> usize {
        if self.copied {
            self.guard.len()
        } else {
            0
        }
    }
    /// Copy-within traffic that publication will perform.
    pub fn moved_bytes(&self) -> usize {
        self.edit.moved_bytes()
    }
    /// Applies the prevalidated program once, then marks the page dirty. No await,
    /// I/O, allocation, or consumer callback occurs during publication.
    pub fn publish(&mut self) {
        if self.published {
            return;
        }
        // Preparation made this allocation unique; the held write latch and
        // private guard prevent any new snapshot/weak reference until release.
        let image = Rc::make_mut(&mut self.guard.guard.0);
        self.edit.apply(&mut image.bytes);
        image.validator.set(
            self.validator
                .map_or(std::ptr::null_mut(), |v| std::ptr::from_ref(v).cast_mut()),
        );
        self.writeback.publish(&mut image.writeback);
        self.guard.frame.changed();
        self.published = true;
    }
}
