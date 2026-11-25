import crates.common.mudu.src.utils.buf.generated
open Aeneas Std Result

namespace sized_buf_spec

/-- Largest payload whose total encoded length fits the API's u32 result. -/
def max_payload : Nat := 4294967291

/-- The length contract uses mathematical integers, not wrapping arithmetic. -/
def length_post (n : Nat) (r : core.result.Result U32 buf.buf.SizedBufError) : Prop :=
  if n > max_payload then r = .Err .LengthOverflow
  else ∃ total, r = .Ok total ∧ total.val = n + 4

/-- Canonical network-order representation of a u32. -/
def header (n : U32) : List U8 := n.bv.toBEBytes.map (@UScalar.mk .U8)

/-- Interpret the first four bytes as a network-order 32-bit word.
Only used when the input has at least four bytes. -/
def decode_header (bytes : List U8) : U32 :=
  UScalar.mk ((BitVec.fromBEBytes
    ([bytes[0]!, bytes[1]!, bytes[2]!, bytes[3]!].map UScalar.bv)).setWidth 32)

end sized_buf_spec
