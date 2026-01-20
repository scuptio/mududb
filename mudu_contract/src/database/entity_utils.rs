use crate::database::entity::Entity;
use crate::tuple::tuple_field::TupleField;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use crate::tuple::tuple_value::TupleValue;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type as data_type;
use mudu_type::dat_binary::DatBinary;
use mudu_type::dat_textual::DatTextual;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

fn _entity_from_tuple<R: Entity, T: AsRef<TupleField>, D: AsRef<TupleFieldDesc>>(
    row: T,
    desc: D,
) -> RS<R> {
    let mut s = R::new_empty();
    if row.as_ref().fields().len() != desc.as_ref().fields().len() {
        panic!("Entity from_tuple wrong length tuple fields:{}, description fields:{}",
               row.as_ref().fields().len(),
               desc.as_ref().fields().len());
    }
    for (i, dat) in row.as_ref().fields().iter().enumerate() {
        let dd = &desc.as_ref().fields()[i];
        s.set_field_binary(dd.name(), dat)?;
    }
    Ok(s)
}

fn _entity_from_tuple_value<R: Entity, T: AsRef<TupleValue>, D: AsRef<TupleFieldDesc>>(
    row: T,
    desc: D,
) -> RS<R> {
    let mut s = R::new_empty();
    if row.as_ref().values().len() != desc.as_ref().fields().len() {
        panic!("Entity from tuple value, wrong length tuple fields:{}, description fields:{}",
               row.as_ref().values().len(),
               desc.as_ref().fields().len());
    }
    for (i, dat) in row.as_ref().values().iter().enumerate() {
        let dd = &desc.as_ref().fields()[i];
        s.set_field_value(dd.name(), dat)?;
    }
    Ok(s)
}

fn _entity_to_tuple<R: Entity, D: AsRef<TupleFieldDesc>>(record: &R, desc: D) -> RS<TupleField> {
    let mut tuple = vec![];
    for d in desc.as_ref().fields() {
        let opt_datum = record.get_field_binary(d.name())?;
        if let Some(datum) = opt_datum {
            tuple.push(datum);
        } else {
            panic!("Field {} return None", d.name());
        }
    }
    Ok(TupleField::new(tuple))
}


fn _entity_from_value<R: Entity, V: AsRef<DatValue>, D: AsRef<TupleFieldDesc>>(
    value: V,
    desc: D,
) -> RS<R> {
    let opt_object = value.as_ref().as_record();
    let object = if let Some(object) = opt_object {
        object
    } else {
        return Err(m_error!(EC::TypeErr, "expected a object type"));
    };

    let mut record = R::new_empty();
    if desc.as_ref().fields().len() != object.len() {
        return Err(m_error!(EC::TypeErr, "wrong field length expected"));
    }
    for (i, filed_data) in object.iter().enumerate() {
        let field_name = desc.as_ref().fields()[i].name();
        record.set_field_value(field_name, filed_data)?;
    }
    Ok(record)
}

fn _entity_to_value<R: Entity>(record: &R, ty: &DatType) -> RS<DatValue> {
    let mut value = vec![];
    let object_param = match ty.dat_type_id() {
        DatTypeID::Record => { ty.expect_record_param() }
        _ => { return Err(m_error!(EC::TypeErr, "convert object to other not support")) }
    };
    for (f_name, _ty) in object_param.fields() {
        let opt_value = record.get_field_value(f_name)?;
        if let Some(datum) = opt_value {
            value.push(datum);
        } else {
            panic!("Field {} return None", f_name);
        }
    }
    Ok(DatValue::from_record(value))
}


pub fn entity_from_tuple_field<E:Entity, T: AsRef<TupleField>>(
    tuple_row: T
) -> RS<E> {
    _entity_from_tuple(tuple_row, E::tuple_desc())
}

pub fn entity_from_tuple_value<E:Entity, T: AsRef<TupleValue>>(
    tuple_row: T
) -> RS<E> {
    _entity_from_tuple_value(tuple_row, E::tuple_desc())
}

pub fn entity_from_value<E:Entity, V: AsRef<DatValue>>(
    value: V
) -> RS<E> {
    _entity_from_value(value, E::tuple_desc())
}

pub fn entity_from_textual<E: Entity>(
    textual: &str
) -> RS<E> {
    let ty = E::dat_type();
    let value = ty.dat_type_id().fn_input()(textual, ty)
        .map_err(|e| { m_error!(EC::TypeErr, "input from string error", e) })?;
    entity_from_value(&value)
}

pub fn entity_dat_type_id() -> RS<DatTypeID> {
    Ok(DatTypeID::Record)
}

pub fn entity_from_binary<E:Entity>(binary:&[u8]) -> RS<E> {
    let ty = E::dat_type();
    let (value, _) = ty.dat_type_id().fn_recv()(binary, ty)
        .map_err(|e|{ m_error!(EC::TypeErr, "convert binary to entity error", e)})?;
    let entity = entity_from_value(&value)?;
    Ok(entity)
}

pub fn entity_dat_type<E:Entity>() -> DatType {
    let object_name = E::object_name().to_string();
    let field_desc = E::tuple_desc();
    let mut vec = Vec::new();
    for field in field_desc.fields() {
        let dat_type = field.dat_type();
        vec.push((field.name().to_string(), dat_type.clone()));
    }
    data_type::record::new_record_type(object_name, vec)
}

pub fn entity_to_tuple<E:Entity>(entity:&E) -> RS<TupleField> {
    _entity_to_tuple(entity, E::tuple_desc())
}

pub fn entity_to_binary<E:Entity>(entity:&E, ty:&DatType) -> RS<DatBinary> {
    let value = entity_to_value(entity, ty)?;
    let id = ty.dat_type_id();
    let binary = id.fn_send()(&value, ty)
        .map_err(|e| m_error!(EC::CompareErr, "convert to binary error", e))?;
    Ok(binary)
}

pub fn entity_to_textual<E:Entity>(entity:&E,  ty:&DatType) -> RS<DatTextual> {
    let value = entity_to_value(entity, ty)?;
    let id = ty.dat_type_id();
    let textual = id.fn_output()(&value, ty)
        .map_err(|e| m_error!(EC::CompareErr, "convert to textual error", e))?;
    Ok(textual)
}

pub fn entity_to_value<E:Entity>(entity:&E, ty:&DatType) -> RS<DatValue> {
    _entity_to_value(entity, ty)
}

pub fn entity_clone_boxed<E:Entity>(entity:&E) -> Box<dyn DatumDyn> {
    Box::new(entity.clone())
}