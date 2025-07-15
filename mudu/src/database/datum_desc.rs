use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::type_declare::TypeDeclare;

#[derive(Clone)]
pub struct DatumDesc {
    type_declare: TypeDeclare,
    name:String,
}


impl DatumDesc {
    pub fn new(name:&str, type_declare: TypeDeclare) -> Self {
        Self {
            type_declare,
            name:name.to_string() 
        }
    }
    
    pub fn name(&self) -> &str {
        &self.name
    }
    
    pub fn type_declare(&self) -> &TypeDeclare {
        &self.type_declare
    }
    
    pub fn data_type_id(&self) -> DatTypeID {
        self.type_declare.param().data_type_id()
    }
}