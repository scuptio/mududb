/// external binary format of a raw tuple
///
/// The tuple format:
///     | -- tuple header            -- |  TUPLE_HEADER SIZE
///     | -- offset and length slot  -- |  point to variable length data
///     | -- fixed length data       -- |
///     | -- variable length data    -- |
pub type TupleRaw = [u8];
