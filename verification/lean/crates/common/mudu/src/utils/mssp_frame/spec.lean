import crates.common.mudu.src.utils.mssp_frame.generated
open Aeneas Std Result

namespace mssp_frame_spec

/-- The canonical 16-byte header layout: magic, version, zero flags, kind,
    all big-endian. -/
def header_fields (magic version kind : U32) : List U8 :=
  magic.bv.toBEBytes.map (@UScalar.mk .U8) ++
  version.bv.toBEBytes.map (@UScalar.mk .U8) ++
  [0#u8, 0#u8, 0#u8, 0#u8] ++
  kind.bv.toBEBytes.map (@UScalar.mk .U8)

/-- Big-endian 32-bit read of four bytes at a natural offset. -/
def read_be32_at (bytes : List U8) (offset : Nat) : U32 :=
  UScalar.mk (BitVec.ofNat 32
    (bytes[offset]!.val * 16777216 + bytes[offset+1]!.val * 65536 + bytes[offset+2]!.val * 256 +
      bytes[offset+3]!.val))

end mssp_frame_spec
