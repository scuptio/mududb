use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_binary::DatBinary;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

fn datum_vec_to<T, F: Fn(&dyn DatumDyn, &DatumDesc) -> RS<T>>(
    param: &[&dyn DatumDyn],
    desc: &[DatumDesc],
    to: &F,
) -> RS<Vec<T>> {
    if param.len() != desc.len() {
        return Err(m_error!(
            EC::MuduError,
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
        let dat: DatBinary = datum
            .to_binary(datum_desc.dat_type())
            .map_err(|e| m_error!(EC::MuduError, format!("{:?} to binary error", datum), e))?;
        Ok(dat.into() as Vec<u8>)
    };
    datum_vec_to(param, desc, &f)
}

pub fn datum_vec_to_value_vec(param: &[&dyn DatumDyn], desc: &[DatumDesc]) -> RS<Vec<DatValue>> {
    let f = |datum: &dyn DatumDyn, datum_desc: &DatumDesc| {
        let dat: DatValue = datum
            .to_value(datum_desc.dat_type())
            .map_err(|e| m_error!(EC::MuduError, format!("{:?} to binary error", datum), e))?;
        Ok(dat)
    };
    datum_vec_to(param, desc, &f)
}
