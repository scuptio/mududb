import crates.«db-kernel».storage.src.page.record_slot_core.generated
open Aeneas Std Result
namespace record_slot_spec
abbrev Slot := record_slot_core.record_slot_core.RecordSlot

/-- Canonical little-endian bytes of one scalar, including every reserved bit. -/
def bytes {ty : UScalarTy} (v : UScalar ty) : List U8 :=
  v.bv.toLEBytes.map (@UScalar.mk .U8)

/-- The fixed 32-byte disk layout; this specification does not call the codec. -/
def encode (s : Slot) : List U8 :=
  bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum ++ bytes s.flag ++ bytes s.reserved

/-- Decode exactly the seven fields, retaining unknown flags and reserved data. -/
def decode (b : List U8) : Slot := {
  offset := UScalar.mk ((BitVec.fromLEBytes ([b[0]!, b[1]!, b[2]!, b[3]!].map UScalar.bv)).setWidth 32)
  size := UScalar.mk ((BitVec.fromLEBytes ([b[4]!, b[5]!, b[6]!, b[7]!].map UScalar.bv)).setWidth 32)
  timestamp := UScalar.mk ((BitVec.fromLEBytes ([b[8]!, b[9]!, b[10]!, b[11]!, b[12]!, b[13]!, b[14]!, b[15]!].map UScalar.bv)).setWidth 64)
  tuple_id := UScalar.mk ((BitVec.fromLEBytes ([b[16]!, b[17]!, b[18]!, b[19]!, b[20]!, b[21]!, b[22]!, b[23]!].map UScalar.bv)).setWidth 64)
  checksum := UScalar.mk ((BitVec.fromLEBytes ([b[24]!, b[25]!].map UScalar.bv)).setWidth 16)
  flag := UScalar.mk ((BitVec.fromLEBytes ([b[26]!, b[27]!].map UScalar.bv)).setWidth 16)
  reserved := UScalar.mk ((BitVec.fromLEBytes ([b[28]!, b[29]!, b[30]!, b[31]!].map UScalar.bv)).setWidth 32)
}
end record_slot_spec
