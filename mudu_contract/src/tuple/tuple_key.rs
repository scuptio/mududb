//! `tuple::tuple_key` module.
#![allow(missing_docs)]

use crate::tuple::comparator::{tuple_compare, tuple_equal, tuple_hash};
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::buf::Buf;
use scc::{Comparable, Equivalent};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct TupleKey {
    desc: *const TupleBinaryDesc,
    key: Buf,
}

impl Eq for TupleKey {}

impl PartialEq<Self> for TupleKey {
    fn eq(&self, other: &Self) -> bool {
        let r = tuple_equal(self.desc(), self.buf(), other.buf());
        r.unwrap_or(false)
    }
}

impl PartialOrd<Self> for TupleKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Hash for TupleKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        let r = tuple_hash(self.desc(), self.buf(), state);
        if r.is_err() {
            self.buf().hash(state);
        }
    }
}

impl Ord for TupleKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let r = tuple_compare(self.desc(), self.buf(), other.buf());
        r.unwrap_or_else(|_| self.buf().cmp(other.buf()))
    }
}

impl TupleKey {
    pub fn desc(&self) -> &TupleBinaryDesc {
        unsafe { &(*self.desc) }
    }

    pub fn from_buf(desc: *const TupleBinaryDesc, data: Buf) -> Self {
        Self { desc, key: data }
    }

    pub fn into(self) -> Buf {
        self.key
    }

    pub fn buf(&self) -> &Buf {
        &self.key
    }
}

impl Borrow<[u8]> for TupleKey {
    fn borrow(&self) -> &[u8] {
        self.buf().as_slice()
    }
}

impl Borrow<Buf> for TupleKey {
    fn borrow(&self) -> &Buf {
        self.buf()
    }
}

pub struct _KeyRef<'a, K: AsRef<[u8]>> {
    key_ref: &'a K,
}

pub struct _Key<K: AsRef<[u8]>> {
    key: K,
}

impl<'a, K: AsRef<[u8]>> _KeyRef<'a, K> {
    pub fn new(key_ref: &'a K) -> Self {
        Self { key_ref }
    }
}

impl<K: AsRef<[u8]>> Equivalent<TupleKey> for _KeyRef<'_, K> {
    fn equivalent(&self, key: &TupleKey) -> bool {
        let r = tuple_equal(key.desc(), self.key_ref.as_ref(), key.buf());
        r.unwrap_or(false)
    }
}

impl<K: AsRef<[u8]>> Comparable<TupleKey> for _KeyRef<'_, K> {
    fn compare(&self, key: &TupleKey) -> Ordering {
        let r = tuple_compare(key.desc(), self.key_ref.as_ref(), key.buf());
        r.unwrap_or_else(|_| self.key_ref.as_ref().cmp(key.buf().as_slice()))
    }
}

impl<K: AsRef<[u8]>> _Key<K> {
    pub fn new(key: K) -> Self {
        Self { key }
    }
}

impl<K: AsRef<[u8]>> Equivalent<TupleKey> for _Key<K> {
    fn equivalent(&self, key: &TupleKey) -> bool {
        let r = tuple_equal(key.desc(), self.key.as_ref(), key.buf());
        r.unwrap_or(false)
    }
}

impl<K: AsRef<[u8]>> Comparable<TupleKey> for _Key<K> {
    fn compare(&self, key: &TupleKey) -> Ordering {
        let r = tuple_compare(key.desc(), self.key.as_ref(), key.buf());
        r.unwrap_or_else(|_| self.key.as_ref().cmp(key.buf().as_slice()))
    }
}

unsafe impl Send for TupleKey {}
unsafe impl Sync for TupleKey {}
