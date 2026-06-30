use crate::contract::table_desc::TableDesc;
use crate::x_engine::api::VecSelTerm;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

pub mod index_access_key;
pub mod index_access_range;

pub(crate) fn project_tuple_desc(table_desc: &TableDesc, select: &VecSelTerm) -> TupleFieldDesc {
    let fields = select
        .vec()
        .iter()
        .map(|attr| {
            let field = table_desc.get_attr(*attr);
            DatumDesc::new_nullable(
                field.name().clone(),
                field.type_desc().clone(),
                field.nullable(),
            )
        })
        .collect();
    TupleFieldDesc::new(fields)
}
