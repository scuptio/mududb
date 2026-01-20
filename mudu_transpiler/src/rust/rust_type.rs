use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dtp_array::DTPArray;
use mudu_binding::universal::uni_type_desc::UniTypeDesc;

#[derive(Debug, Clone)]
pub enum RustType {
    Primitive(String),
    Tuple(Vec<RustType>),
    Custom(String),
    Generic(String, Vec<RustType>),
}

impl RustType {
    pub fn as_ret_type(&self) -> Vec<RustType> {
        match self {
            RustType::Generic(_, vec) => {
                if vec.len() != 1 {
                    panic!("RustType::ret_type_str_inner, return type must be RS<(...)>");
                }
                vec[0].as_ret_type_inner()
            }
            _ => {
                panic!("RustType::ret_type_str_inner, return type must be RS<(...)>");
            }
        }
    }

    pub fn to_type_str(&self) -> String {
        match self {
            RustType::Primitive(s) => s.clone(),
            RustType::Tuple(vec) => {
                let mut s = "(".to_string();
                for t in vec.iter() {
                    s.push_str(t.to_type_str().as_str());
                    s.push_str(", ");
                }
                s.push_str(")");
                s
            }
            RustType::Custom(s) => s.clone(),
            RustType::Generic(s, vec) => {
                let mut s = format!("{}<", s);
                for t in vec.iter() {
                    s.push_str(t.to_type_str().as_str());
                    s.push_str(", ");
                }
                s.push_str(">");
                s
            }
        }
    }

    pub fn to_ret_type_str(&self) -> Vec<String> {
        match self {
            RustType::Generic(_, vec) => {
                if vec.len() != 1 {
                    panic!("RustType::ret_type_str_inner, return type must be RS<(...)>");
                }
                vec[0].to_ret_type_str_inner()
            }
            _ => {
                panic!("RustType::ret_type_str_inner, return type must be RS<(...)>");
            }
        }
    }

    fn to_ret_type_str_inner(&self) -> Vec<String> {
        match self {
            RustType::Primitive(s) => {
                vec![s.clone()]
            }
            RustType::Tuple(vec) => vec.iter().map(|t| t.to_type_str().clone()).collect(),
            _ => {
                vec![self.to_type_str()]
            }
        }
    }

    fn as_ret_type_inner(&self) -> Vec<RustType> {
        match &self {
            RustType::Primitive(_) => {
                vec![self.clone()]
            }
            RustType::Tuple(vec) => {
                (*vec).clone()
            }
            _ => {
                vec![self.clone()]
            }
        }
    }

    pub fn to_dat_type(&self, custom_types: &UniTypeDesc) -> RS<DatType> {
        let dat_type = match self {
            RustType::Primitive(s) => {
                match s.as_str() {
                    "i32" => { DatType::default_for(DatTypeID::I32) }
                    "i64" => { DatType::default_for(DatTypeID::I64) }
                    "f32" => { DatType::default_for(DatTypeID::F32) }
                    "f64" => { DatType::default_for(DatTypeID::F64) }
                    _ => {
                        return Err(m_error!(EC::TypeErr, format!("not support type {}", s)))
                    }
                }
            }
            RustType::Custom(s) => {
                match s.as_str() {
                    "String" => { DatType::default_for(DatTypeID::String) }
                    _ => {
                        let ty = custom_types.types.get(s).map_or_else(
                            ||{ Err(m_error!(EC::NoneErr, format!("no such type name:{}", s)))},
                            |t|{ Ok(t) }
                        )?;
                        ty.clone().uni_to()?
                    }
                }
            }
            RustType::Generic(ident, vec) => {
                if ident == "Vec" && vec.len() == 1 {
                    let array = DTPArray::new(vec[0].to_dat_type(custom_types)?);
                    DatType::from_array(array)
                } else {
                    return Err(m_error!(EC::TypeErr, format!("not support type {:?}", self)))
                }
            }
            _ => {
                return Err(m_error!(EC::TypeErr, format!("not support type {:?}", self)))
            }
        };
        Ok(dat_type)
    }
}