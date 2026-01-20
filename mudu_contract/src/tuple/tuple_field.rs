use crate::tuple::binary_to_json::tuple_binary_to_json;
use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu::utils::json::{JsonMap, JsonValue};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TupleField {
    fields: Vec<Vec<u8>>,
}

impl TupleField {
    pub fn new(fields: Vec<Vec<u8>>) -> TupleField {
        Self { fields }
    }

    pub fn fields(&self) -> &Vec<Vec<u8>> {
        &self.fields
    }

    pub fn into_fields(self) -> Vec<Vec<u8>> {
        self.fields
    }

    pub fn mut_fields(&mut self) -> &mut Vec<Vec<u8>> {
        &mut self.fields
    }

    pub fn get(&self, n: usize) -> Option<Vec<u8>> {
        self.fields.get(n).cloned()
    }

    pub fn to_json_value(&self, desc: &[DatumDesc]) -> RS<JsonValue> {
        if self.fields().len() != desc.len() {
            return Err(m_error!(
                EC::DBInternalError,
                format!(
                    "to json value, expected {} fields but got {}",
                    desc.len(),
                    self.fields().len()
                )
            ));
        }
        let mut map = JsonMap::with_capacity(self.fields().len());
        for (i, field) in self.fields().iter().enumerate() {
            let d = &desc[i];
            let json_value = tuple_binary_to_json(field, d)?;
            map.insert(d.name().to_owned(), json_value);
        }
        Ok(JsonValue::Object(map))
    }
    pub fn to_textual(&self, desc: &[DatumDesc]) -> RS<Vec<String>> {
        if self.fields().len() != desc.len() {
            return Err(m_error!(
                EC::DBInternalError,
                format!(
                    "to data printable, expected {} fields but got {}",
                    desc.len(),
                    self.fields().len()
                )
            ));
        }
        let mut vec_string = Vec::with_capacity(self.fields().len());
        for (i, field) in self.fields().iter().enumerate() {
            let datum_desc = &desc[i];
            let id = datum_desc.dat_type_id();
            let (internal, _) = id.fn_recv()(field, datum_desc.dat_type())
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert binary to internal error", e))?;
            let printable = id.fn_output()(&internal, datum_desc.dat_type())
                .map_err(|e| m_error!(EC::TypeBaseErr, "convert internal to binary error", e))?;
            vec_string.push(printable.into())
        }
        Ok(vec_string)
    }
}

impl AsRef<TupleField> for TupleField {
    fn as_ref(&self) -> &Self {
        self
    }
}
