use std::cell::RefCell;
use std::collections::HashMap;
use crate::rust::game_object::GameObject;

thread_local! {
    static MAP:RefCell<HashMap<u64, GameObject>> = RefCell::new(HashMap::new());
}


struct Instance {

}


impl Instance {
    pub fn put(id:u64, object:GameObject) {
        MAP.with(|m| {
            m.borrow_mut().insert(id, object);
        })
    }

    pub fn get(id:u64) -> Option<GameObject> {
        MAP.with(|m| {
            m.borrow().get(&id).cloned()
        })
    }
}