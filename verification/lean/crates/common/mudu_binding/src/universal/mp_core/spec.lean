import crates.common.mudu_binding.src.universal.mp_core.generated
open Aeneas Std Result

namespace mp_core_spec

/-- Big-endian byte list of a bit-vector. -/
def be_bytes_of {w : Nat} (bv : BitVec w) : List U8 :=
  bv.toBEBytes.map (@UScalar.mk .U8)

/-- The low byte of a 64-bit word (the `as u8` truncation). -/
def low_byte (bv : BitVec 64) : U8 := UScalar.mk (bv.setWidth 8)

/-- Canonical encoding of an unsigned integer, minimal-width chain. -/
def enc_uint (v : U64) : List U8 :=
  if v.val ≤ 0x7f then [low_byte v.bv]
  else if v.val ≤ 0xff then [0xcc#u8, low_byte v.bv]
  else if v.val ≤ 0xffff then [0xcd#u8] ++ be_bytes_of (v.bv.setWidth 16)
  else if v.val ≤ 0xffffffff then [0xce#u8] ++ be_bytes_of (v.bv.setWidth 32)
  else [0xcf#u8] ++ be_bytes_of v.bv

/-- Canonical encoding of a signed integer; non-negative values follow the
    unsigned chain, negative values the minimal-width signed chain. -/
def enc_sint (v : I64) : List U8 :=
  if 0 ≤ v.val then enc_uint (UScalar.mk v.bv)
  else if -32 ≤ v.val then [low_byte v.bv]
  else if -128 ≤ v.val then [0xd0#u8, low_byte v.bv]
  else if -32768 ≤ v.val then [0xd1#u8] ++ be_bytes_of (v.bv.setWidth 16)
  else if -2147483648 ≤ v.val then [0xd2#u8] ++ be_bytes_of (v.bv.setWidth 32)
  else [0xd3#u8] ++ be_bytes_of v.bv

/-- Canonical encoding of a boolean. -/
def enc_bool (b : Bool) : List U8 := [if b then 0xc3#u8 else 0xc2#u8]

/-- Canonical encoding of MessagePack nil. -/
def enc_nil : List U8 := [0xc0#u8]

/-- Canonical encoding of an `f32` bit pattern. -/
def enc_f32_bits (bits : U32) : List U8 := [0xca#u8] ++ be_bytes_of bits.bv

/-- Canonical encoding of an `f64` bit pattern. -/
def enc_f64_bits (bits : U64) : List U8 := [0xcb#u8] ++ be_bytes_of bits.bv

/-- Canonical encoding of a string length header. -/
def enc_str_header (len : U32) : List U8 :=
  if len.val ≤ 31 then [UScalar.mk (BitVec.ofNat 8 (0xa0 + len.val))]
  else if len.val ≤ 0xff then [0xd9#u8, UScalar.mk (len.bv.setWidth 8)]
  else if len.val ≤ 0xffff then [0xda#u8] ++ be_bytes_of (len.bv.setWidth 16)
  else [0xdb#u8] ++ be_bytes_of len.bv

/-- Canonical encoding of a byte-string length header. -/
def enc_bin_header (len : U32) : List U8 :=
  if len.val ≤ 0xff then [0xc4#u8, UScalar.mk (len.bv.setWidth 8)]
  else if len.val ≤ 0xffff then [0xc5#u8] ++ be_bytes_of (len.bv.setWidth 16)
  else [0xc6#u8] ++ be_bytes_of len.bv

/-- Canonical encoding of an array length header. -/
def enc_array_header (len : U32) : List U8 :=
  if len.val ≤ 15 then [UScalar.mk (BitVec.ofNat 8 (0x90 + len.val))]
  else if len.val ≤ 0xffff then [0xdc#u8] ++ be_bytes_of (len.bv.setWidth 16)
  else [0xdd#u8] ++ be_bytes_of len.bv

/-- Canonical encoding of a map length header. -/
def enc_map_header (len : U32) : List U8 :=
  if len.val ≤ 15 then [UScalar.mk (BitVec.ofNat 8 (0x80 + len.val))]
  else if len.val ≤ 0xffff then [0xde#u8] ++ be_bytes_of (len.bv.setWidth 16)
  else [0xdf#u8] ++ be_bytes_of len.bv

end mp_core_spec
