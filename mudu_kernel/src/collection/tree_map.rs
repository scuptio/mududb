use scc::ebr::Guard;
use scc::TreeIndex;

pub async fn tree_map_async_get_or_create<K, V, C, T>(
    scc_tree_map: &TreeIndex<K, V>,
    key: K,
    create: C,
    is_valid: T,
) -> Option<V>
where
    K: Ord + Eq + Copy + 'static,
    V: Clone + 'static,
    C: Fn() -> V + 'static,
    T: for<'r> Fn(&'r V) -> bool + 'static,
{
    let mut key = key;
    let mut value = None;
    loop {
        let guard = Guard::new();
        let opt = scc_tree_map.peek(&key, &guard);
        match opt {
            Some(e) => {
                if is_valid(e) {
                    return Some(e.clone());
                }
            }
            None => {
                let v = match value {
                    Some(v) => v,
                    None => create(),
                };
                let opt = scc_tree_map.insert_async(key, v.clone()).await;
                match opt {
                    Ok(_) => {
                        return Some(v);
                    }
                    Err((k, v)) => {
                        key = k;
                        value = Some(v)
                    }
                }
            }
        }
    }
}

pub fn tree_map_get_or_create<K, V, C, T>(
    scc_tree_map: &TreeIndex<K, V>,
    key: K,
    creater: C,
    is_valid: T,
) -> Option<V>
where
    K: Ord + Eq + Copy + 'static,
    V: Clone + 'static,
    C: Fn() -> V + 'static,
    T: for<'r> Fn(&'r V) -> bool + 'static,
{
    let mut key = key;
    let mut value = None;
    loop {
        let guard = Guard::new();
        let opt = scc_tree_map.peek(&key, &guard);
        match opt {
            Some(e) => {
                if is_valid(e) {
                    return Some(e.clone());
                }
            }
            None => {
                let v = match value {
                    Some(v) => v,
                    None => creater(),
                };
                let opt = scc_tree_map.insert(key, v.clone());
                match opt {
                    Ok(_) => {
                        return Some(v);
                    }
                    Err((k, v)) => {
                        key = k;
                        value = Some(v)
                    }
                }
            }
        }
    }
}
