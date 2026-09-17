use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_sql_param::UniSqlParam;
use mudu::common::result::RS;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_contract::database::sql_params::SQLParams;

impl UniSqlParam {
    pub fn uni_to(self) -> RS<SQLParamValue> {
        let mut vec = Vec::with_capacity(self.params.len());
        for v in self.params {
            let value = v.uni_to()?;
            vec.push(value);
        }
        Ok(SQLParamValue::from_vec(vec).with_param_names(self.param_names))
    }

    pub fn uni_from(p: SQLParamValue) -> RS<UniSqlParam> {
        let param_names = p.param_names().map(|names| names.to_vec());
        let mut params = Vec::with_capacity(p.params().len());
        for v in p.into() {
            let mu_value = UniDataValue::uni_from(v)?;
            params.push(mu_value);
        }
        Ok(UniSqlParam {
            params,
            param_names,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::UniSqlParam;
    use crate::universal::uni_data_value::UniDataValue;
    use crate::universal::uni_scalar_value::UniScalarValue;
    use mudu_contract::database::sql_params::SQLParams;

    #[test]
    fn uni_to_and_uni_from_roundtrip() {
        let original = UniSqlParam {
            params: vec![
                UniDataValue::Scalar(UniScalarValue::from_i32(1)),
                UniDataValue::Scalar(UniScalarValue::from_i64(2)),
                UniDataValue::Scalar(UniScalarValue::from_string("three".to_string())),
            ],
            param_names: None,
        };
        let sql_value = original.uni_to().unwrap();
        assert_eq!(sql_value.params().len(), 3);
        assert!(sql_value.param_names().is_none());
        let roundtrip = UniSqlParam::uni_from(sql_value).unwrap();
        assert_eq!(roundtrip.params.len(), 3);
        assert!(roundtrip.param_names.is_none());
        assert_eq!(*roundtrip.params[0].as_scalar().unwrap().expect_i32(), 1);
        assert_eq!(
            roundtrip.params[2].as_scalar().unwrap().expect_string(),
            "three"
        );
    }

    #[test]
    fn uni_to_and_uni_from_roundtrip_with_names() {
        let original = UniSqlParam {
            params: vec![UniDataValue::Scalar(UniScalarValue::from_i32(1))],
            param_names: Some(vec!["user_id".to_string()]),
        };
        let sql_value = original.uni_to().unwrap();
        assert_eq!(sql_value.param_names(), Some(&["user_id".to_string()][..]));
        let roundtrip = UniSqlParam::uni_from(sql_value).unwrap();
        assert_eq!(roundtrip.param_names, Some(vec!["user_id".to_string()]));
    }

    #[test]
    fn uni_to_empty_params() {
        let param = UniSqlParam {
            params: vec![],
            param_names: None,
        };
        let sql_value = param.uni_to().unwrap();
        assert!(sql_value.params().is_empty());
        let roundtrip = UniSqlParam::uni_from(sql_value).unwrap();
        assert!(roundtrip.params.is_empty());
    }
}
