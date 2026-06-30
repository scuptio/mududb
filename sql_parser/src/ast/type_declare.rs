use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dt_info::DTInfo;

/// SQL data type declaration extracted from a column definition.
#[derive(Clone, Debug)]
pub struct TypeDeclare {
    id: DatTypeID,
    param: DatType,
}

impl TypeDeclare {
    /// Create a new type declaration from a concrete data type.
    pub fn new(param: DatType) -> Self {
        Self {
            id: param.dat_type_id(),
            param,
        }
    }

    /// Return the data type identifier.
    pub fn id(&self) -> DatTypeID {
        self.id
    }

    /// Return the underlying data type.
    pub fn param(&self) -> &DatType {
        &self.param
    }

    /// Return the data type metadata info.
    pub fn param_info(&self) -> DTInfo {
        self.param.to_info()
    }
}

#[cfg(test)]
mod tests {
    use super::TypeDeclare;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    #[test]
    fn type_declare_exposes_param_metadata() {
        let ty = DatType::new_no_param(DatTypeID::I64);
        let declare = TypeDeclare::new(ty.clone());

        assert_eq!(declare.id(), DatTypeID::I64);
        assert_eq!(declare.param().dat_type_id(), DatTypeID::I64);
        assert_eq!(declare.param_info().id, ty.to_info().id);
        assert_eq!(declare.param_info().param, ty.to_info().param);
    }
}
