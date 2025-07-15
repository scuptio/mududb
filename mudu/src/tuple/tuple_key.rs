use crate::common::buf::Buf;
use crate::tuple::comparator::{tuple_compare, tuple_equal, tuple_hash};
use crate::tuple::tuple_desc::TupleDesc;
use scc::{Comparable, Equivalent};
use std::borrow::Borrow;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct TupleKey {
    desc: *const TupleDesc,
    key: Buf,
}

impl Eq for TupleKey {}

impl PartialEq<Self> for TupleKey {
    fn eq(&self, other: &Self) -> bool {
        let r = tuple_equal(self.desc(), self.buf(), other.buf());
        r.unwrap_or_else(|e| panic!("error {}", e))
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
        r.unwrap_or_else(|e| panic!("TupleKey Hash::hash error {}", e))
    }
}

impl Ord for TupleKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let r = tuple_compare(self.desc(), self.buf(), other.buf());
        r.unwrap_or_else(|e| panic!("TupleKey Ord::cmp error {}", e))
    }
}

impl TupleKey {
    pub fn desc(&self) -> &TupleDesc {
        unsafe { &(*self.desc) }
    }

    pub fn from_buf(desc: *const TupleDesc, data: Buf) -> Self {
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
        r.unwrap_or_else(|e| panic!("error {}", e))
    }
}

impl<K: AsRef<[u8]>> Comparable<TupleKey> for _KeyRef<'_, K> {
    fn compare(&self, key: &TupleKey) -> Ordering {
        let r = tuple_compare(key.desc(), self.key_ref.as_ref(), key.buf());
        r.unwrap_or_else(|e| panic!("error {}", e))
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
        r.unwrap_or_else(|e| panic!("error {}", e))
    }
}

impl<K: AsRef<[u8]>> Comparable<TupleKey> for _Key<K> {
    fn compare(&self, key: &TupleKey) -> Ordering {
        let r = tuple_compare(key.desc(), self.key.as_ref(), key.buf());
        r.unwrap_or_else(|e| panic!("error {}", e))
    }
}

unsafe impl Send for TupleKey {}
unsafe impl Sync for TupleKey {}
