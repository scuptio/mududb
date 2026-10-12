use ::catalog::contract::table_desc::TableDesc;
use ::xengine::api::VecSelTerm;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

pub mod aggregate;
pub mod filter;
/// Key-equality point-read executor.
pub mod index_access_key;
/// Key-range scan executor.
pub mod index_access_range;
pub mod value_compare;

/// Builds the output tuple descriptor of the projection `select` over
/// `table_desc`.
pub fn project_tuple_desc(table_desc: &TableDesc, select: &VecSelTerm) -> TupleFieldDesc {
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
