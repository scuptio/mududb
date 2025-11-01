use mudu::data_type::dt_impl::dat_typed::DatTyped;

pub enum ItemValue {
    Literal(DatTyped),
    Placeholder,
}
