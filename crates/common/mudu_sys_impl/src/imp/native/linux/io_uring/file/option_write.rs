/// Options controlling a submitted write.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptionWrite {
    /// When true, the write completes optimistically with the full length
    /// without waiting for the CQE result.
    pub blind_write: bool,
}
