use crate::data_type::type_desc::TypeDesc;
use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TupleItemDesc {
    vec: Vec<DatumDesc>,
}

impl TupleItemDesc {
    pub fn new(vec: Vec<DatumDesc>) -> Self {
        Self { vec }
    }

    pub fn vec_datum_desc(&self) -> &Vec<DatumDesc> {
        &self.vec
    }

    pub fn to_tuple_desc(&self) -> (TupleBinaryDesc, Vec<usize>) {
        let vec_type_desc: Vec<(TypeDesc, usize)> = self.vec.iter()
            .enumerate()
            .map(|(i, e)|
                { (TypeDesc::new(e.dat_type_id(), e.type_declare().param_info()), i) })
            .collect();
        let (vec_normalized_desc, index_mapping) = TupleBinaryDesc::normalized_type_desc_vec::<usize>(vec_type_desc);
        let desc = TupleBinaryDesc::from(vec_normalized_desc);
        (desc, index_mapping)
    }
}


impl AsRef<TupleItemDesc> for TupleItemDesc {
    fn as_ref(&self) -> &TupleItemDesc {
        self
    }
}


#[cfg(test)]
mod tests {
    use crate::common::serde_utils::{deserialize_from_json, serialize_to_json};
    use crate::data_type::dat_type::DatType;
    use crate::data_type::dt_impl::dat_type_id::DatTypeID;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_item_desc::TupleItemDesc;

    #[test]
    fn test_row_desc() {
        let vec = vec![
            DatumDesc::new("c1".to_string(), DatType::new_with_no_param(DatTypeID::I32)),
            DatumDesc::new("c2".to_string(), DatType::new_with_no_param(DatTypeID::I64)),
            DatumDesc::new("c3".to_string(), DatType::new_with_no_param(DatTypeID::I32)),
        ];
        let desc = TupleItemDesc::new(vec);
        let json = serialize_to_json(&desc).unwrap();
        println!("{}", json);
        let _desc: TupleItemDesc = deserialize_from_json(&json).unwrap();
    }
}