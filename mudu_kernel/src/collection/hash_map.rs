use mudu::common::result::RS;
use scc::HashMap;
use std::hash::Hash;

pub async fn hash_map_async_get_or_create<K, V, C, T, R>(
    scc_hash_map: &HashMap<K, V>,
    key: K,
    create: C,
    call: T,
) -> RS<R>
where
    K: Hash + Eq + Copy + 'static,
    V: Clone + 'static,
    C: Fn() -> V + 'static,
    T: for<'r> Fn(&'r V) -> Option<R> + 'static,
{
    let mut key = key;
    loop {
        let opt = scc_hash_map.get_async(&key).await;
        let value = match opt {
            Some(e) => e.get().clone(),
            None => {
                let v_created = create();
                let opt = scc_hash_map.insert_async(key, v_created.clone()).await;
                match opt {
                    Ok(_) => v_created,
                    Err((k, v)) => {
                        key = k;
                        v
                    }
                }
            }
        };
        let r = call(&value);
        match r {
            Some(r) => return Ok(r),
            None => {}
        }
    }
}

pub fn hash_map_get_or_create<K, V, C, T, R>(
    scc_hash_map: &HashMap<K, V>,
    key: K,
    creater: C,
    call: T,
) -> RS<R>
where
    K: Hash + Eq + Clone + 'static,
    V: Clone + 'static,
    C: Fn() -> V + 'static,
    T: for<'r> Fn(&'r V) -> Option<R> + 'static,
{
    let mut key = key;
    loop {
        let opt = scc_hash_map.get(&key);
        let v = match opt {
            Some(e) => e.get().clone(),
            None => {
                let v = creater();
                let _opt = scc_hash_map.insert(key.clone(), v.clone());
                match _opt {
                    Ok(_) => v,
                    Err((_k, _v)) => {
                        key = _k;
                        v
                    }
                }
            }
        };
        let r = call(&v);
        match r {
            Some(r) => return Ok(r),
            None => {}
        }
    }
}
