use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_sql_param::UniSqlParam;
use mudu::common::result::RS;
use mudu_contract::database::sql_param_value::SQLParamValue;

impl UniSqlParam {
    pub fn uni_to(self) -> RS<SQLParamValue> {
        let mut vec = Vec::with_capacity(self.params.len());
        for v in self.params {
            let value = v.uni_to()?;
            vec.push(value);
        }
        Ok(SQLParamValue::from_vec(vec))
    }

    pub fn uni_from(p: SQLParamValue) -> RS<UniSqlParam> {
        let mut params = Vec::with_capacity(p.params().len());
        for v in p.into() {
            let mu_value = UniDatValue::uni_from(v)?;
            params.push(mu_value);
        }
        Ok(UniSqlParam { params })
    }
}

#[cfg(test)]
mod tests {
    use super::UniSqlParam;
    use crate::universal::uni_dat_value::UniDatValue;
    use crate::universal::uni_scalar_value::UniScalarValue;

    #[test]
    fn uni_to_and_uni_from_roundtrip() {
        let original = UniSqlParam {
            params: vec![
                UniDatValue::Scalar(UniScalarValue::from_i32(1)),
                UniDatValue::Scalar(UniScalarValue::from_i64(2)),
                UniDatValue::Scalar(UniScalarValue::from_string("three".to_string())),
            ],
        };
        let sql_value = original.uni_to().unwrap();
        assert_eq!(sql_value.params().len(), 3);
        let roundtrip = UniSqlParam::uni_from(sql_value).unwrap();
        assert_eq!(roundtrip.params.len(), 3);
        assert_eq!(*roundtrip.params[0].as_scalar().unwrap().expect_i32(), 1);
        assert_eq!(
            roundtrip.params[2].as_scalar().unwrap().expect_string(),
            "three"
        );
    }

    #[test]
    fn uni_to_empty_params() {
        let param = UniSqlParam { params: vec![] };
        let sql_value = param.uni_to().unwrap();
        assert!(sql_value.params().is_empty());
        let roundtrip = UniSqlParam::uni_from(sql_value).unwrap();
        assert!(roundtrip.params.is_empty());
    }
}
