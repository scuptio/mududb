use crate::index::index_key::compare_context::CompareContext;
use mudu::common::result::RS;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct KeyTuple {
    tuple: Vec<u8>,
}

impl KeyTuple {
    pub fn new(tuple: Vec<u8>) -> Self {
        Self { tuple }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.tuple
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.tuple
    }
}

impl From<Vec<u8>> for KeyTuple {
    fn from(value: Vec<u8>) -> Self {
        Self::new(value)
    }
}

impl Eq for KeyTuple {}

impl PartialEq<Self> for KeyTuple {
    fn eq(&self, other: &Self) -> bool {
        let r = CompareContext::with_context_mut(|c: &mut CompareContext| {
            if c.result.is_err() {
                return None;
            }
            let r: RS<bool> = (c.comparator.equal)(&self.tuple, &other.tuple, &c.desc);
            match r {
                Ok(e) => Some(e),
                Err(e) => {
                    c.result = Err(e);
                    None
                }
            }
        });
        r.unwrap_or(true)
    }
}

impl PartialOrd<Self> for KeyTuple {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for KeyTuple {
    fn cmp(&self, other: &Self) -> Ordering {
        let r = CompareContext::with_context_mut(|c: &mut CompareContext| {
            if c.result.is_err() {
                return None;
            }
            let r: RS<Ordering> =
                (c.comparator.compare)(&self.tuple, &other.tuple, &c.desc);
            match r {
                Ok(o) => Some(o),
                Err(e) => {
                    c.result = Err(e);
                    None
                }
            }
        });
        r.unwrap_or(Ordering::Equal)
    }
}

impl Hash for KeyTuple {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let _ = CompareContext::with_context_mut(|c: &mut CompareContext| {
            if c.result.is_err() {
                return None;
            }
            let r: RS<()> =
                (c.comparator.hash_cal_one)(&self.tuple, &c.desc, state as &mut dyn Hasher);
            match r {
                Ok(()) => Some(()),
                Err(e) => {
                    c.result = Err(e);
                    None
                }
            }
        });
    }
}
