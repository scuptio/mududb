#[derive(Clone, Debug)]
pub enum DatTyped {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
    String(String),
    Null,
}
