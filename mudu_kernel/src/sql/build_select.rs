use crate::contract::table_desc::TableDesc;
use crate::sql::proj_field::ProjField;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::data_type::type_desc::TypeDesc;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use sql_parser::ast::select_term::SelectTerm;

pub fn visit_select_term(
    select_term: &Vec<SelectTerm>,
    table_desc: &TableDesc,
) -> RS<(Vec<OID>, Vec<ProjField>, Vec<TypeDesc>)> {
    let mut ids = vec![];
    let mut proj_fields = vec![];
    let mut type_desc_vec = vec![];
    for (i, term) in select_term.iter().enumerate() {
        let oid = table_desc.name2oid().get(term.field().name()).map_or(
            Err(m_error!(ER::NoSuchElement, format!("{}", term.field().name()))),
            |id| Ok(*id),
        )?;
        ids.push(oid);
        let name = if term.alias().is_empty() {
            term.field().name().clone()
        } else {
            term.alias().clone()
        };
        let f = table_desc
            .oid2col()
            .get(&oid)
            .map_or(Err(m_error!(ER::NoSuchElement, format!("{}", oid))), Ok)?;
        let type_desc = f.type_desc().clone();
        proj_fields.push(ProjField::new(i, oid, name, type_desc.clone()));
        type_desc_vec.push(type_desc);
    }
    Ok((ids, proj_fields, type_desc_vec))
}
