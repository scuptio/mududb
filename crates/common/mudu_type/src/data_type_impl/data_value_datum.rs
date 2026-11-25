//! Implements the [`DatumDyn`] object-safe view for [`DataValue`], keeping the
//! concrete value container free of any dependency on the datum traits.

use crate::data_binary::DataBinary;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_value::DataValue;
use crate::datum::DatumDyn;
use crate::type_family::TypeFamily;
use crate::type_family_ext::TypeFamilyExt;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

impl DatumDyn for DataValue {
    fn type_family(&self) -> RS<TypeFamily> {
        Ok(self.inner.get_type_family())
    }

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
        if self.is_null() {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                "NULL has no binary payload"
            ));
        }
        let id = self.inner.get_type_family();
        id.fn_send()(self, data_type)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "", e))
    }

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
        if self.is_null() {
            return Ok(DataTextual::from("NULL".to_string()));
        }
        let id = self.inner.get_type_family();
        id.fn_output()(self, data_type)
            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "", e))
    }

    fn to_value(&self, _: &DataType) -> RS<DataValue> {
        Ok(self.clone())
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}
