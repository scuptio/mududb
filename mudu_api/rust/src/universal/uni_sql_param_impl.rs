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
