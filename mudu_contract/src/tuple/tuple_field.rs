//! `tuple::tuple_field` module.
#![allow(missing_docs)]

use crate::tuple::binary_to_json::tuple_binary_to_json;
use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::json::{JsonMap, JsonValue};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TupleField {
    fields: Vec<Option<Vec<u8>>>,
}

impl TupleField {
    pub fn new(fields: Vec<Vec<u8>>) -> TupleField {
        Self {
            fields: fields.into_iter().map(Some).collect(),
        }
    }

    pub fn new_nullable(fields: Vec<Option<Vec<u8>>>) -> TupleField {
        Self { fields }
    }

    pub fn fields(&self) -> &Vec<Option<Vec<u8>>> {
        &self.fields
    }

    pub fn into_fields(self) -> Vec<Option<Vec<u8>>> {
        self.fields
    }

    pub fn mut_fields(&mut self) -> &mut Vec<Option<Vec<u8>>> {
        &mut self.fields
    }

    pub fn get(&self, n: usize) -> Option<Vec<u8>> {
        self.fields.get(n).cloned().flatten()
    }

    pub fn is_null(&self, n: usize) -> bool {
        matches!(self.fields.get(n), Some(None))
    }

    pub fn to_json_value(&self, desc: &[DatumDesc]) -> RS<JsonValue> {
        if self.fields().len() != desc.len() {
            return Err(mudu_error!(
                ErrorCode::Database,
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
            let json_value = match field {
                Some(field) => tuple_binary_to_json(field, d)?,
                None => JsonValue::Null,
            };
            map.insert(d.name().to_owned(), json_value);
        }
        Ok(JsonValue::Object(map))
    }
    pub fn to_textual(&self, desc: &[DatumDesc]) -> RS<Vec<String>> {
        if self.fields().len() != desc.len() {
            return Err(mudu_error!(
                ErrorCode::Database,
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
            let Some(field) = field else {
                vec_string.push("NULL".to_string());
                continue;
            };
            let id = datum_desc.dat_type_id();
            let printable = id.fn_recv()(field, datum_desc.dat_type())
                .and_then(|(internal, _)| id.fn_output()(&internal, datum_desc.dat_type()))
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::TypeConversionFailed,
                        "convert binary to internal error",
                        e
                    )
                })?;
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
