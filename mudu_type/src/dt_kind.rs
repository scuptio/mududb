#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DTKind {
    Scalar,
    Array,
    Record,
    Binary,
}
