use mudu::tuple::datum_desc::DatumDesc;

pub trait ResolvedCommand {
    fn placeholder(&self) -> &Vec<DatumDesc>;
}