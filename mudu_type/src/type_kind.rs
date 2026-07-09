#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeKind {
    Scalar,
    Array,
    Record,
    Binary,
}
