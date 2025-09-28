use crate::common::result::RS;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::datum::DatumDyn;
use crate::tuple::datum_desc::DatumDesc;

fn datum_vec_to<T, F: Fn(&dyn DatumDyn, &DatumDesc) -> RS<T>>(
    param: &[&dyn DatumDyn],
    desc: &Vec<DatumDesc>,
    to: &F) -> RS<Vec<T>> {
    if param.len() != desc.len() {
        return Err(m_error!(EC::MuduError, format!("Incorrect number of parameters provided: {} != {}", param.len(), desc.len())));
    }
    let mut vec = Vec::with_capacity(desc.len());
    for (i, datum) in param.iter().enumerate() {
        let datum_desc = &desc[i];
        let t: T = to(*datum, datum_desc)?;
        vec.push(t);
    }
    Ok(vec)
}


pub fn datum_vec_to_bin_vec(
    param: &[&dyn DatumDyn],
    desc: &Vec<DatumDesc>,
) -> RS<Vec<Vec<u8>>> {
    let f = |datum: &dyn DatumDyn, datum_desc: &DatumDesc| {
        let dat: DatBinary = datum
            .to_binary(datum_desc.type_declare().param())
            .map_err(|e| {
                m_error!(EC::MuduError, format!("{:?} to binary error", datum), e)
            })?;
        Ok(dat.into() as Vec<u8>)
    };
    datum_vec_to(param, desc, &f)
}