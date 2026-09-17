use crate::type_family::TypeFamily;
use std::collections::HashMap;

pub fn type_family_2_lang_type_name(
    id_name: &Vec<(TypeFamily, &'static str)>,
) -> HashMap<TypeFamily, String> {
    let mut id2name = HashMap::new();
    for (id, s) in id_name {
        id2name.insert(*id, s.to_string());
    }
    id2name
}

fn insert_sorted<T: Ord>(vec: &mut Vec<T>, item: T) {
    match vec.binary_search(&item) {
        Ok(pos) | Err(pos) => {
            vec.insert(pos, item);
        }
    }
}

pub fn lang_type_name_2_type_family(
    id_name: &Vec<(TypeFamily, &'static str)>,
) -> HashMap<String, (TypeFamily, Vec<TypeFamily>)> {
    let mut name2id = HashMap::new();
    for (id, s) in id_name {
        if !name2id.contains_key(*s) {
        } else {
            let opt = name2id.get_mut(*s);
            if let Some((t, vec)) = opt {
                insert_sorted(vec, *id);
                *t = vec.pop().unwrap();
            }
        }
        name2id.insert(s.to_string(), (*id, Default::default()));
    }

    name2id
}
