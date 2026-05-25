pub enum NonScalarType {
    Array(String),
    Option(String),
    Box(String),
    Tuple(Vec<String>),
}
