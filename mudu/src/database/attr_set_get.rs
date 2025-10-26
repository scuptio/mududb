use crate::common::result::RS;
use crate::database::attr_value::AttrValue;
use crate::tuple::datum::Datum;

pub fn attr_get_binary<R: Datum, A: AttrValue<R>>(attribute: &Option<A>) -> RS<Option<Vec<u8>>> {
    let opt_datum = match attribute {
        Some(value) => Some(value.get_binary()?),
        None => None,
    };
    Ok(opt_datum)
}

pub fn attr_set_binary<R: Datum, A: AttrValue<R>, D: AsRef<[u8]>>(
    attribute: &mut Option<A>,
    binary: D,
) -> RS<()> {
    match attribute {
        Some(value) => {
            value.set_binary(binary.as_ref())?;
        }
        None => {
            *attribute = Some(A::from_binary(binary.as_ref())?);
        }
    }
    Ok(())
}
