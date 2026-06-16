use crate::tuple::tuple_binary::{TupleBinary, TupleSlice};
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use crate::tuple::write_value;
use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

pub fn build_tuple_into(
    vec: &[Buf],
    tuple_desc: &TupleBinaryDesc,
    tuple: &mut TupleSlice,
) -> RS<Result<usize, usize>> {
    if vec.len() != tuple_desc.field_count() {
        return Err(m_error!(
            EC::ParseErr,
            format!(
                "value length {} does not match tuple field count {}",
                vec.len(),
                tuple_desc.field_count()
            )
        ));
    }
    if tuple.len() < tuple_desc.min_tuple_size() {
        return Ok(Err(tuple_desc.min_tuple_size()));
    }
    let mut offset = tuple_desc.meta_size();
    if offset > tuple.len() {
        return Err(m_error!(
            EC::TupleErr,
            format!(
                "tuple meta size {} exceeds tuple len {}",
                offset,
                tuple.len()
            )
        ));
    }
    for (i, v) in vec.iter().enumerate() {
        let field = tuple_desc.get_field_desc(i);
        let r = write_value::write_value_to_tuple(field, offset, v, tuple)?;
        let size = match &r {
            Ok(size) => *size,
            Err(_) => {
                return Ok(r);
            }
        };
        write_value::write_slot_to_tuple(field, offset, size, tuple)?;
        offset += size;
    }
    Ok(Ok(offset))
}

pub fn build_tuple(vec: &[Buf], tuple_desc: &TupleBinaryDesc) -> RS<TupleBinary> {
    let mut tuple = vec![0; tuple_desc.min_tuple_size()];
    tuple.resize(tuple_desc.min_tuple_size(), 0);
    if vec.len() != tuple_desc.field_count() {
        return Err(m_error!(
            EC::ParseErr,
            format!(
                "value length {} does not match tuple field count {}",
                vec.len(),
                tuple_desc.field_count()
            )
        ));
    }
    if tuple.len() < tuple_desc.min_tuple_size() {
        return Err(m_error!(
            EC::InsufficientBufferSpace,
            format!(
                "tuple buffer size {} is less than {}",
                tuple.len(),
                tuple_desc.min_tuple_size()
            )
        ));
    }
    let mut offset = tuple_desc.meta_size();
    if offset > tuple.len() {
        return Err(m_error!(
            EC::TupleErr,
            format!(
                "tuple meta size {} exceeds tuple len {}",
                offset,
                tuple.len()
            )
        ));
    }
    for (i, v) in vec.iter().enumerate() {
        let field = tuple_desc.get_field_desc(i);
        let size = loop {
            let r = write_value::write_value_to_tuple(field, offset, v, &mut tuple)?;
            match &r {
                Ok(size) => break *size,
                Err(_size) => {
                    tuple.resize(tuple.len() * 2, 0);
                }
            };
        };
        write_value::write_slot_to_tuple(field, offset, size, &mut tuple)?;
        offset += size;
    }
    tuple.resize(offset, 0);
    Ok(tuple)
}

#[cfg(test)]
mod tests {
    use super::{build_tuple, build_tuple_into};
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use mudu::error::ec::EC;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    #[test]
    fn zero_field_tuple_is_allowed() {
        let desc = TupleBinaryDesc::from(Vec::new()).unwrap();

        let tuple = build_tuple(&Vec::new(), &desc).unwrap();
        assert!(tuple.is_empty());

        let mut into_buf = Vec::new();
        let result = build_tuple_into(&[], &desc, &mut into_buf).unwrap();
        assert_eq!(result, Ok(0));
    }

    #[test]
    fn build_tuple_rejects_mismatched_field_count() {
        let desc = TupleBinaryDesc::from(vec![DatType::new_no_param(DatTypeID::I32)]).unwrap();
        let err = build_tuple(&Vec::new(), &desc).unwrap_err();
        assert_eq!(err.ec(), EC::ParseErr);
    }
}
