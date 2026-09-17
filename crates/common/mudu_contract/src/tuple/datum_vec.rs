//! `tuple::datum_vec` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_binary::DataBinary;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;

fn datum_vec_to<T, F: Fn(&dyn DatumDyn, &DatumDesc) -> RS<T>>(
    param: &[&dyn DatumDyn],
    desc: &[DatumDesc],
    to: &F,
) -> RS<Vec<T>> {
    if param.len() != desc.len() {
        return Err(mudu_error!(
            ErrorCode::TypeConversionFailed,
            format!(
                "Incorrect number of parameters provided: {} != {}",
                param.len(),
                desc.len()
            )
        ));
    }
    let mut vec = Vec::with_capacity(desc.len());
    for (i, datum) in param.iter().enumerate() {
        let datum_desc = &desc[i];
        let t: T = to(*datum, datum_desc)?;
        vec.push(t);
    }
    Ok(vec)
}

pub fn datum_vec_to_bin_vec(param: &[&dyn DatumDyn], desc: &[DatumDesc]) -> RS<Vec<Vec<u8>>> {
    let f = |datum: &dyn DatumDyn, datum_desc: &DatumDesc| {
        let dat: DataBinary = datum.to_binary(datum_desc.data_type()).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                format!("{:?} to binary error", datum),
                e
            )
        })?;
        Ok(dat.into() as Vec<u8>)
    };
    datum_vec_to(param, desc, &f)
}

pub fn datum_vec_to_value_vec(param: &[&dyn DatumDyn], desc: &[DatumDesc]) -> RS<Vec<DataValue>> {
    let f = |datum: &dyn DatumDyn, datum_desc: &DatumDesc| {
        let dat: DataValue = datum.to_value(datum_desc.data_type()).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                format!("{:?} to binary error", datum),
                e
            )
        })?;
        Ok(dat)
    };
    datum_vec_to(param, desc, &f)
}
