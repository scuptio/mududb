//! Uncontended edits complete synchronously. Only a contended pin enters the
//! async continuation, which retains its page and rechecks the current image.
use super::{
    images,
    node::{Key, Link},
    patch,
    view::NodeView,
    PagedBTree, RS,
};
use page_buffer::{BufferAccess, PagePin, PageReadGuard, PageWriteGuard};
use std::cmp::Ordering;

/// Carries a contended page to the async continuation without restarting descent.
pub(super) enum ChangeAttempt<'a> {
    Done(Option<Option<u128>>),
    Read(PagePin<'a>),
    Write(PagePin<'a>),
}

// A level-one parent supplies a leaf hint, never permission to skip validation.
enum Route {
    Leaf,
    Next(Link, bool),
    Fallback,
}

/// A validated leaf position plus whether the write evidences an ordered
/// stream boundary (arming the write memo).
struct LeafTarget {
    at: usize,
    equal: bool,
    boundary: bool,
}

impl<V: Send> PagedBTree<V> {
    pub(super) fn try_change_resident<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        key: &[u8],
        replacement: Option<u128>,
    ) -> RS<ChangeAttempt<'a>> {
        if let Some(done) = self.change_via_memo(access, key, replacement)? {
            return Ok(done);
        }
        self.change_from(access, key, replacement, self.root_link(), false)
    }

    /// Single-leaf memo: append/delete streams hit one leaf for a whole burst,
    /// so validate the memoized page under its write latch and skip the
    /// descent. The memo is only a hint: any pin/latch/validation failure
    /// falls back to the full descent.
    fn change_via_memo<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        key: &[u8],
        replacement: Option<u128>,
    ) -> RS<Option<ChangeAttempt<'a>>> {
        let pid = self.memo.get();
        if pid == 0 {
            return Ok(None);
        }
        let Some(page) = self.buffer.try_pin(access, &self.buffer.page_ref(pid))? else {
            self.memo.set(0);
            return Ok(None);
        };
        let Some(guard) = page.try_write() else {
            return Ok(None);
        };
        let changed = self.change_latched(&page, guard, key, replacement, true)?;
        if changed.is_none() {
            // A fallback means the stream moved on; drop the hint so random
            // phases never pay more than one wasted probe.
            self.memo.set(0);
        }
        Ok(changed.map(|old| ChangeAttempt::Done(Some(old))))
    }

    /// The usual path constructs no async descent/latch future and owns no snapshot.
    ///
    /// Interior levels route over the decoded-node cache first: a token-valid
    /// hit needs neither the page read latch nor the slot-directory machinery.
    /// A miss or an undecidable comparison (overflow key) falls back to the
    /// byte view for that level, so the worst case is the previous path plus
    /// one cache probe.
    fn change_from<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        key: &[u8],
        replacement: Option<u128>,
        mut link: Link,
        mut leaf_hint: bool,
    ) -> RS<ChangeAttempt<'a>> {
        loop {
            let Some(page) = self
                .buffer
                .try_pin(access, &self.buffer.page_ref(link.pid))?
            else {
                return Ok(ChangeAttempt::Done(None));
            };
            if !leaf_hint {
                if let Some(node) = self.nodes.borrow().get(link.pid, page.cache_token()) {
                    match self.change_route_cached(&node, key)? {
                        Route::Leaf => leaf_hint = true,
                        Route::Next(next, leaf) => {
                            link = next;
                            leaf_hint = leaf;
                            continue;
                        }
                        Route::Fallback => leaf_hint = false,
                    }
                }
                if !leaf_hint {
                    let Some(guard) = page.try_read() else {
                        return Ok(ChangeAttempt::Read(page));
                    };
                    let route = self.change_route(page.id(), &guard, key)?;
                    drop(guard);
                    match route {
                        Route::Leaf => {}
                        Route::Next(next, leaf) => {
                            link = next;
                            leaf_hint = leaf;
                            continue;
                        }
                        Route::Fallback => return Ok(ChangeAttempt::Done(None)),
                    }
                }
            }
            if let Some(guard) = page.try_write() {
                return self
                    .change_latched(&page, guard, key, replacement, false)
                    .map(ChangeAttempt::Done);
            }
            return Ok(ChangeAttempt::Write(page));
        }
    }

    /// Cache-guided routing: mirrors `change_route` exactly, returning
    /// `Route::Fallback` only for comparisons the cache cannot decide inline
    /// (overflow keys), in which case the caller retries on the byte view.
    fn change_route_cached(&self, node: &super::node_cache::CachedNode, key: &[u8]) -> RS<Route> {
        if let Some(high) = &node.high {
            let Some(order) = self.compare_cached_inline(node, *high, key)? else {
                return Ok(Route::Fallback);
            };
            if order.reverse() != Ordering::Less {
                return Ok(if node.right == 0 {
                    Route::Fallback
                } else {
                    Route::Next(Link::new(node.right), node.leaf())
                });
            }
        }
        if node.leaf() {
            return Ok(Route::Leaf);
        }
        let Some((at, equal)) = self.position_cached_sync(node, key)? else {
            return Ok(Route::Fallback);
        };
        let child = if equal {
            node.entries[at].target
        } else if at == 0 {
            node.left
        } else {
            node.entries[at - 1].target
        };
        Ok(Route::Next(Link::new(child), node.level == 1))
    }

    /// Resume only the contended portion. All guards are dropped before waiting
    /// for another guard; cancellation releases the retained pin automatically.
    pub(super) async fn finish_change_resident<'a>(
        &self,
        access: &'a BufferAccess<'_>,
        key: &[u8],
        replacement: Option<u128>,
        mut attempt: ChangeAttempt<'a>,
    ) -> RS<Option<Option<u128>>> {
        loop {
            attempt = match attempt {
                ChangeAttempt::Done(result) => return Ok(result),
                ChangeAttempt::Write(page) => {
                    let guard = self.resident_write(&page).await;
                    return self.change_latched(&page, guard, key, replacement, false);
                }
                ChangeAttempt::Read(page) => {
                    let guard = self.contended_read(&page).await;
                    let route = self.change_route(page.id(), &guard, key)?;
                    drop(guard);
                    match route {
                        Route::Leaf => {
                            if let Some(guard) = page.try_write() {
                                return self.change_latched(&page, guard, key, replacement, false);
                            }
                            ChangeAttempt::Write(page)
                        }
                        Route::Next(link, leaf) => {
                            self.change_from(access, key, replacement, link, leaf)?
                        }
                        Route::Fallback => return Ok(None),
                    }
                }
            };
        }
    }

    /// Decide the next page from a validated, shared-latched image.
    fn change_route(&self, pid: u64, guard: &PageReadGuard<'_>, key: &[u8]) -> RS<Route> {
        self.validate_page(guard.image(), pid)?;
        let node = NodeView::new(guard)?;
        if let Some(high) = node.high()? {
            let Some(order) = self.compare_inline_order(high, key, true)? else {
                return Ok(Route::Fallback);
            };
            if order != Ordering::Less {
                return Ok(if node.right() == 0 {
                    Route::Fallback
                } else {
                    Route::Next(Link::new(node.right()), node.leaf())
                });
            }
        }
        if node.leaf() {
            return Ok(Route::Leaf);
        }
        let Some((at, equal)) = self.position_inline(&node, key)? else {
            return Ok(Route::Fallback);
        };
        let child = if equal {
            node.value(at)? as u64
        } else if at == 0 {
            node.left()
        } else {
            node.value(at - 1)? as u64
        };
        Ok(Route::Next(Link::new(child), node.level() == 1))
    }

    /// Recheck type/fence after exclusive acquisition: a split may move the key
    /// while the writer waits. Unsupported layouts retain the structural fallback.
    /// Fence and position probes consult the decoded-node cache first (the
    /// token is read under the write latch, so a hit matches the latched bytes
    /// exactly); undecidable comparisons fall back to the byte view.
    /// `memoized` marks a leaf reached through the write memo instead of a
    /// descent: a key below all entries then cannot prove membership (it may
    /// belong to an earlier leaf), so the caller falls back to a full descent.
    fn change_latched(
        &self,
        page: &PagePin<'_>,
        guard: PageWriteGuard<'_>,
        key: &[u8],
        replacement: Option<u128>,
        memoized: bool,
    ) -> RS<Option<Option<u128>>> {
        self.validate_page(guard.image(), page.id())?;
        let node = NodeView::new(&guard)?;
        if !node.leaf() {
            return Ok(None);
        }
        let cached = self.nodes.borrow().get(page.id(), page.cache_token());
        if let Some(high) = &node.high()? {
            let order = match cached.as_deref() {
                Some(cached) if cached.high.is_some() => self
                    .compare_cached_inline(cached, cached.high.unwrap(), key)?
                    .map(|order| order.reverse()),
                _ => None,
            };
            let order = match order {
                Some(order) => Some(order),
                None => self.compare_inline_order(*high, key, true)?,
            };
            if order != Some(Ordering::Less) {
                return Ok(None);
            }
        }
        let positioned = match cached.as_deref() {
            Some(cached) => self.position_cached_sync(cached, key)?,
            None => None,
        };
        let Some((at, equal)) = match positioned {
            Some(found) => Ok(Some(found)),
            None => self.position_inline(&node, key),
        }?
        else {
            return Ok(None);
        };
        if memoized && at == 0 && !equal {
            // A memo hit cannot prove the key does not belong to an earlier
            // leaf; only a descent establishes the lower bound.
            return Ok(None);
        }
        // Only boundary writes mark the memo: an append at the end or a
        // removal at the front evidences an ordered stream. Random
        // mid-leaf replacements must not arm the probe, or mixed/random
        // workloads would pay a wasted memo probe per write.
        let boundary = (replacement.is_some() && !equal && at == node.len())
            || (replacement.is_none() && equal && at == 0);
        self.change_apply(
            page,
            guard,
            key,
            replacement,
            LeafTarget {
                at,
                equal,
                boundary,
            },
        )
    }

    /// Builds and publishes the byte patch for a validated leaf position,
    /// mirrors the edit into the cached node, and updates counters and the
    /// write memo. `None` means the page no longer accepts the edit
    /// (overlong key, or a full page needing a split).
    fn change_apply(
        &self,
        page: &PagePin<'_>,
        guard: PageWriteGuard<'_>,
        key: &[u8],
        replacement: Option<u128>,
        target: LeafTarget,
    ) -> RS<Option<Option<u128>>> {
        let LeafTarget {
            at,
            equal,
            boundary,
        } = target;
        let node = NodeView::new(&guard)?;
        let old = if equal { Some(node.value(at)?) } else { None };
        // Mirror the edit into the cached decoded node at publication, so
        // readers never re-decode after a write.
        let cache_edit = match (replacement, equal) {
            (Some(value), true) => Some(super::node_cache::CachedEdit::Replace {
                at,
                target: value as u64,
                upper: (value >> 64) as u64,
            }),
            (None, true) => Some(super::node_cache::CachedEdit::Remove { at }),
            (Some(value), false) => Some(super::node_cache::CachedEdit::Insert {
                at,
                key_head: key.to_vec(),
                overflow: 0,
                len: key.len() as u64,
                target: value as u64,
                upper: (value >> 64) as u64,
            }),
            (None, false) => None,
        };
        let edit = match (replacement, equal) {
            (Some(value), true) => patch::replace(&node, at, value)?,
            (None, true) => patch::remove(&node, at)?,
            (None, false) => return Ok(Some(None)),
            (Some(value), false) => {
                if key.len() > self.inline_key_len(key.len()) {
                    return Ok(None);
                }
                let Some(edit) = patch::insert(&node, at, &Key::inline(key), value)? else {
                    return Ok(None);
                };
                edit
            }
        };
        let before = page.cache_token();
        let mut write = guard.prepare_edit_with_shared_writeback(
            edit,
            Some(&images::VALIDATOR),
            &self.writeback_policy,
        )?;
        self.record_edit(&write);
        write.publish();
        if let Some(edit) = cache_edit {
            self.cache_apply_edit(page, before, edit);
        }
        if replacement.is_none() {
            self.count.set(self.count.get() - 1);
        } else if !equal {
            self.count.set(self.count.get() + 1);
        }
        if boundary {
            self.memo.set(page.id());
        }
        Ok(Some(old))
    }
}
