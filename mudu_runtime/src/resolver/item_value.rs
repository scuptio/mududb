use mudu_type::dat_typed::DatTyped;

pub enum ItemValue {
    Literal(DatTyped),
    Placeholder,
}
