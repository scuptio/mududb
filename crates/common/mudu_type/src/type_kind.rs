#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Scalar,
    Array,
    Struct,
    Binary,
}
