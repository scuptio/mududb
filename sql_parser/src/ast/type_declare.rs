use mudu_type::data_type::DataType;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;

/// SQL data type declaration extracted from a column definition.
#[derive(Clone, Debug)]
pub struct TypeDeclare {
    id: TypeFamily,
    param: DataType,
}

impl TypeDeclare {
    /// Create a new type declaration from a concrete data type.
    pub fn new(param: DataType) -> Self {
        Self {
            id: param.type_family(),
            param,
        }
    }

    /// Return the data type identifier.
    pub fn id(&self) -> TypeFamily {
        self.id
    }

    /// Return the underlying data type.
    pub fn param(&self) -> &DataType {
        &self.param
    }

    /// Return the data type metadata info.
    pub fn param_info(&self) -> DataTypeInfo {
        self.param.to_info()
    }
}

#[cfg(test)]
mod tests {
    use super::TypeDeclare;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    #[test]
    fn type_declare_exposes_param_metadata() {
        let ty = DataType::new_no_param(TypeFamily::I64);
        let declare = TypeDeclare::new(ty.clone());

        assert_eq!(declare.id(), TypeFamily::I64);
        assert_eq!(declare.param().type_family(), TypeFamily::I64);
        assert_eq!(declare.param_info().id, ty.to_info().id);
        assert_eq!(declare.param_info().param, ty.to_info().param);
    }
}
