use crate::common::result::RS;
use crate::data_type::dt_param::ParamObj;
use crate::error::ec::EC;
use crate::m_error;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::datum::Datum;

pub fn datum_from_binary<T: Datum + 'static, B: AsRef<[u8]>>(datum: B) -> RS<T> {
    let desc = T::datum_desc();
    let dat_type_id = desc.dat_type_id();
    let r = dat_type_id.fn_recv()(datum.as_ref(), &ParamObj::default_for(dat_type_id))
        .map_err(|e| m_error!(EC::ConvertErr, "from binary error", e))?;
    let value: T = r.into_to_typed();
    Ok(value)
}

pub fn datum_to_binary<T: Datum + 'static>(datum: &T) -> RS<Vec<u8>> {
    let desc = T::datum_desc();
    let dat_type_id = desc.dat_type_id();
    let internal = DatInternal::from_datum(datum.clone())?;
    let binary = dat_type_id.fn_send()(&internal, desc.param_obj())
        .map_err(|e| m_error!(EC::ConvertErr, "convert datum error", e))?;
    Ok(binary.into())
}
