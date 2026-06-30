use crate::x_engine::tx_mgr::PhysicalRelationId;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;

pub struct XLockMgr {
    lock: SMutex<HashMap<PhysicalRelationId, HashMap<Vec<u8>, OID>>>,
}

impl XLockMgr {
    pub fn new() -> Self {
        Self {
            lock: SMutex::new(HashMap::new()),
        }
    }

    pub fn try_lock_some(
        &self,
        oid: OID,
        table_keys: &[(PhysicalRelationId, Vec<u8>)],
    ) -> RS<bool> {
        mudu_utils::scoped_task_trace!();
        let mut lock = self.lock.lock()?;
        let mut acquired: Vec<(PhysicalRelationId, Vec<u8>)> = Vec::new();
        for (relation_id, key) in table_keys.iter() {
            let map = lock.entry(*relation_id).or_default();
            if let Some(owner) = map.get(key) {
                if *owner != oid {
                    // Roll back locks already acquired in this call to avoid
                    // leaking partial locks on failure.
                    for (acquired_relation, acquired_key) in acquired.iter() {
                        if let Some(acquired_map) = lock.get_mut(acquired_relation) {
                            if acquired_map.get(acquired_key) == Some(&oid) {
                                acquired_map.remove(acquired_key);
                            }
                        }
                    }
                    return Ok(false);
                }
            } else {
                map.insert(key.clone(), oid);
                acquired.push((*relation_id, key.clone()));
            }
        }
        Ok(true)
    }

    pub fn release(&self, oid: OID, table_keys: &[(PhysicalRelationId, Vec<u8>)]) -> RS<()> {
        let mut lock = self.lock.lock()?;
        for (relation_id, key) in table_keys.iter() {
            let map = lock.entry(*relation_id).or_default();
            if let Some(tx) = map.get(key) {
                if *tx == oid {
                    map.remove(key);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::XLockMgr;
    use crate::x_engine::tx_mgr::PhysicalRelationId;

    #[test]
    fn try_lock_some_rolls_back_partial_acquire_on_conflict() {
        let mgr = XLockMgr::new();
        let r = PhysicalRelationId {
            table_id: 1,
            partition_id: 0,
        };

        let owner_a_keys = vec![(r, b"k2".to_vec())];
        assert!(mgr.try_lock_some(100, &owner_a_keys).unwrap());

        let owner_b_keys = vec![(r, b"k1".to_vec()), (r, b"k2".to_vec())];
        assert!(!mgr.try_lock_some(200, &owner_b_keys).unwrap());

        // If partial lock rollback works, k1 should not be leaked and owner C
        // can lock it.
        let owner_c_keys = vec![(r, b"k1".to_vec())];
        assert!(mgr.try_lock_some(300, &owner_c_keys).unwrap());
    }

    #[test]
    fn try_lock_some_allows_reentrant_same_owner_key() {
        let mgr = XLockMgr::new();
        let r = PhysicalRelationId {
            table_id: 2,
            partition_id: 0,
        };
        let keys = vec![(r, b"k1".to_vec()), (r, b"k1".to_vec())];
        assert!(mgr.try_lock_some(42, &keys).unwrap());
    }
}
