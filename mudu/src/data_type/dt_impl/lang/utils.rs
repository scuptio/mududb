use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use std::collections::HashMap;

pub fn dat_type_id_2_lang_type_name(id_name: &Vec<(DatTypeID, &'static str)>) -> Vec<String> {
    let n = (DatTypeID::max() + 1) as usize;
    let mut id2name: Vec<String> = vec![String::default(); n];
    for (id, s) in id_name {
        id2name[id.to_u32() as usize] = s.to_string();
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

pub fn lang_type_name_2_dat_type_id(
    id_name: &Vec<(DatTypeID, &'static str)>,
) -> HashMap<String, (DatTypeID, Vec<DatTypeID>)> {
    let mut name2id = HashMap::new();
    for (id, s) in id_name {
        if !name2id.contains_key(*s) {
        } else {
            let opt = name2id.get_mut(*s);
            match opt {
                Some((t, vec)) => {
                    insert_sorted(vec, id.clone());
                    *t = vec.pop().unwrap();
                }
                None => {}
            }
        }
        name2id.insert(s.to_string(), (id.clone(), Default::default()));
    }

    name2id
}
