use crate::database::datum_desc::DatumDesc;
use std::collections::HashMap;


#[derive(Clone)]
pub struct RowDesc {
    vec : Vec<DatumDesc>,
    map: HashMap<String, DatumDesc>
}

impl RowDesc {
    pub fn new(vec : Vec<DatumDesc>) -> Self {
        let mut map = HashMap::new();
        for d in vec.iter() {
            let opt = map.insert(d.name().to_string(), d.clone());
            if opt.is_some() {
                panic!("Duplicate key: {}", d.name());
            }
        }
        Self { vec, map }
    }
    
    pub fn desc(&self) -> &Vec<DatumDesc> {
        &self.vec
    }
    
    pub fn find_by_name(&self, name: &str) -> Option<&DatumDesc> {
        self.map.get(name)
    }
}

impl AsRef<RowDesc> for RowDesc {
    fn as_ref(&self) -> &RowDesc {
        self
    }
}