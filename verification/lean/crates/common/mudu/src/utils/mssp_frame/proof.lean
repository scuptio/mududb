import support.bit_bytes
import support.sequences
import crates.common.mudu.src.utils.mssp_frame.spec
open Aeneas Std Result verification_sequences verification_bit_bytes
open mssp_frame_spec

namespace mssp_frame_proof

/-- Val-level roundtrip of the four-byte big-endian encoding of a 32-bit word. -/
private theorem toBEBytes_val32 (b : BitVec 32) :
    ∃ b0 b1 b2 b3 : U8, b.toBEBytes.map (@UScalar.mk .U8) = [b0, b1, b2, b3] ∧
      b0.val * 16777216 + b1.val * 65536 + b2.val * 256 + b3.val = b.toNat := by exact verification_bit_bytes.toBEBytes_val32 b

/-- Big-endian 32-bit read at an offset, success case. -/
@[step] theorem be_u32_spec (input : Aeneas.Std.Slice U8) (offset : Usize)
    (h : offset.val + 4 ≤ input.length) :
    mssp_frame.mssp_frame.be_u32 input offset ⦃ r =>
      r = read_be32_at input.val offset.val ⦄ := by
  unfold mssp_frame.mssp_frame.be_u32
  step as ⟨b0, hb0⟩
  step as ⟨c0, hc0⟩
  step as ⟨m0, hm0⟩
  step as ⟨o1, ho1⟩
  step as ⟨b1, hb1⟩
  step as ⟨c1, hc1⟩
  step as ⟨m1, hm1⟩
  step as ⟨a1, ha1⟩
  step as ⟨o2, ho2⟩
  step as ⟨b2, hb2⟩
  step as ⟨c2, hc2⟩
  step as ⟨m2, hm2⟩
  step as ⟨a2, ha2⟩
  step as ⟨o3, ho3⟩
  step as ⟨b3, hb3⟩
  step as ⟨c3, hc3⟩
  step as ⟨w, hw⟩
  simp only [read_be32_at]
  have e0 : b0 = input.val[offset.val]! := by
    simpa only [getElem!_pos input.val offset.val (by scalar_tac)] using hb0
  have ho1' : o1.val = offset.val + 1 := by scalar_tac
  have e1 : b1 = input.val[offset.val + 1]! := by
    simpa only [getElem!_pos input.val (offset.val + 1) (by scalar_tac), ho1'] using hb1
  have ho2' : o2.val = offset.val + 2 := by scalar_tac
  have e2 : b2 = input.val[offset.val + 2]! := by
    simpa only [getElem!_pos input.val (offset.val + 2) (by scalar_tac), ho2'] using hb2
  have ho3' : o3.val = offset.val + 3 := by scalar_tac
  have e3 : b3 = input.val[offset.val + 3]! := by
    simpa only [getElem!_pos input.val (offset.val + 3) (by scalar_tac), ho3'] using hb3
  rw [← e0, ← e1, ← e2, ← e3]
  apply UScalar.eq_of_val_eq
  simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U32_numBits_eq]
  rw [Nat.mod_eq_of_lt (by scalar_tac)]
  scalar_tac

/-- The encoder produces exactly the canonical layout. -/
theorem encode_layout (magic version kind : U32) :
    mssp_frame.mssp_frame.encode_header_fields magic version kind ⦃ out =>
      out.val = header_fields magic version kind ⦄ := by
  unfold mssp_frame.mssp_frame.encode_header_fields
  step as ⟨m, hm⟩
  step as ⟨v, hv⟩
  step as ⟨k, hk⟩
  have hml : m.length = 4 := by simp [hm, BitVec.toBEBytes_length]
  have hvl : v.length = 4 := by simp [hv, BitVec.toBEBytes_length]
  have hkl : k.length = 4 := by simp [hk, BitVec.toBEBytes_length]
  step as ⟨m0, hm0⟩
  step as ⟨m1, hm1⟩
  step as ⟨m2, hm2⟩
  step as ⟨m3, hm3⟩
  step as ⟨v0, hv0⟩
  step as ⟨v1, hv1⟩
  step as ⟨v2, hv2⟩
  step as ⟨v3, hv3⟩
  step as ⟨k0, hk0⟩
  step as ⟨k1, hk1⟩
  step as ⟨k2, hk2⟩
  step as ⟨k3, hk3⟩
  obtain ⟨b0, b1, b2, b3, hbm, -⟩ := toBEBytes_val32 magic.bv
  obtain ⟨c0, c1, c2, c3, hbv, -⟩ := toBEBytes_val32 version.bv
  obtain ⟨d0, d1, d2, d3, hbk, -⟩ := toBEBytes_val32 kind.bv
  have hm' : m.val = [b0, b1, b2, b3] := by rw [hm]; exact hbm
  have hv' : v.val = [c0, c1, c2, c3] := by rw [hv]; exact hbv
  have hk' : k.val = [d0, d1, d2, d3] := by rw [hk]; exact hbk
  have hm0' : m0 = b0 := by simp [hm0, hm']
  have hm1' : m1 = b1 := by simp [hm1, hm']
  have hm2' : m2 = b2 := by simp [hm2, hm']
  have hm3' : m3 = b3 := by simp [hm3, hm']
  have hv0' : v0 = c0 := by simp [hv0, hv']
  have hv1' : v1 = c1 := by simp [hv1, hv']
  have hv2' : v2 = c2 := by simp [hv2, hv']
  have hv3' : v3 = c3 := by simp [hv3, hv']
  have hk0' : k0 = d0 := by simp [hk0, hk']
  have hk1' : k1 = d1 := by simp [hk1, hk']
  have hk2' : k2 = d2 := by simp [hk2, hk']
  have hk3' : k3 = d3 := by simp [hk3, hk']
  simp only [Array.make_val]
  rw [hm0', hm1', hm2', hm3', hv0', hv1', hv2', hv3', hk0', hk1', hk2', hk3']
  simp [header_fields, hbm, hbv, hbk]

/-- The decoder parses a canonical header back, flags included. -/
theorem header_roundtrip (magic version kind : U32) (rest : List U8)
    (input : Aeneas.Std.Slice U8)
    (h : input.val = header_fields magic version kind ++ rest) :
    mssp_frame.mssp_frame.decode_header_fields input ⦃ r =>
      r = some (magic, version, 0#u32, kind) ⦄ := by
  obtain ⟨m0, m1, m2, m3, hbm, hvm⟩ := toBEBytes_val32 magic.bv
  obtain ⟨v0, v1, v2, v3, hbv, hvv⟩ := toBEBytes_val32 version.bv
  obtain ⟨k0, k1, k2, k3, hbk, hkv⟩ := toBEBytes_val32 kind.bv
  have hfields : header_fields magic version kind =
      [m0, m1, m2, m3, v0, v1, v2, v3, 0#u8, 0#u8, 0#u8, 0#u8, k0, k1, k2, k3] := by
    simp [header_fields, hbm, hbv, hbk]
  have hin : input.val = m0 :: m1 :: m2 :: m3 :: v0 :: v1 :: v2 :: v3 ::
      0#u8 :: 0#u8 :: 0#u8 :: 0#u8 :: k0 :: k1 :: k2 :: k3 :: rest := by
    simp [h, hfields]
  have hlen : input.length = rest.length + 16 := by simp [Slice.length, hin]
  unfold mssp_frame.mssp_frame.decode_header_fields
  dsimp only
  split
  · rename_i hn
    simp only [mssp_frame.mssp_frame.HEADER_LEN] at hn
    scalar_tac
  · step as ⟨w0, hw0⟩
    step as ⟨w1, hw1⟩
    step as ⟨w2, hw2⟩
    step as ⟨w3, hw3⟩
    -- w0 = magic, w1 = version, w2 = 0, w3 = kind
    have hb0 : input.val[0]! = m0 := by simp [hin]
    have hb1 : input.val[1]! = m1 := by simp [hin]
    have hb2 : input.val[2]! = m2 := by simp [hin]
    have hb3 : input.val[3]! = m3 := by simp [hin]
    have hw0' : w0 = magic := by
      apply UScalar.eq_of_val_eq
      simp only [read_be32_at] at hw0
      rw [hb0, hb1, hb2, hb3] at hw0
      rw [hw0]
      rw [hvm]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U32_numBits_eq]
      rw [Nat.mod_eq_of_lt (by omega)]
    have hbv0 : input.val[4]! = v0 := by simp [hin]
    have hbv1 : input.val[5]! = v1 := by simp [hin]
    have hbv2 : input.val[6]! = v2 := by simp [hin]
    have hbv3 : input.val[7]! = v3 := by simp [hin]
    have hw1' : w1 = version := by
      apply UScalar.eq_of_val_eq
      simp only [read_be32_at] at hw1
      rw [hbv0, hbv1, hbv2, hbv3] at hw1
      rw [hw1]
      rw [hvv]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U32_numBits_eq]
      rw [Nat.mod_eq_of_lt (by omega)]
    have hbf0 : input.val[8]! = 0#u8 := by simp [hin]
    have hbf1 : input.val[9]! = 0#u8 := by simp [hin]
    have hbf2 : input.val[10]! = 0#u8 := by simp [hin]
    have hbf3 : input.val[11]! = 0#u8 := by simp [hin]
    have hw2' : w2 = 0#u32 := by
      apply UScalar.eq_of_val_eq
      simp only [read_be32_at] at hw2
      rw [hbf0, hbf1, hbf2, hbf3] at hw2
      rw [hw2]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U32_numBits_eq]
      decide
    have hbk0 : input.val[12]! = k0 := by simp [hin]
    have hbk1 : input.val[13]! = k1 := by simp [hin]
    have hbk2 : input.val[14]! = k2 := by simp [hin]
    have hbk3 : input.val[15]! = k3 := by simp [hin]
    have hw3' : w3 = kind := by
      apply UScalar.eq_of_val_eq
      simp only [read_be32_at] at hw3
      rw [hbk0, hbk1, hbk2, hbk3] at hw3
      rw [hw3]
      rw [hkv]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U32_numBits_eq]
      rw [Nat.mod_eq_of_lt (by omega)]
    rw [hw0', hw1', hw2', hw3']


/-- Short inputs are rejected. -/
theorem short_rejected (input : Aeneas.Std.Slice U8) (h : input.length < 16) :
    mssp_frame.mssp_frame.decode_header_fields input ⦃ r => r = none ⦄ := by
  unfold mssp_frame.mssp_frame.decode_header_fields
  dsimp only
  split
  · simp
  · rename_i hn
    simp only [mssp_frame.mssp_frame.HEADER_LEN] at hn
    scalar_tac

/-- Sanity: the specification is satisfiable — the canonical MSSP v1 header
    for message kind 1 (query) round-trips byte-for-byte. -/
theorem sanity_header :
    mssp_frame.mssp_frame.decode_header_fields
      (Slice.from [0x4d#u8, 0x53#u8, 0x53#u8, 0x50#u8, 0#u8, 0#u8, 0#u8, 1#u8,
        0#u8, 0#u8, 0#u8, 0#u8, 0#u8, 0#u8, 0#u8, 1#u8] (by scalar_tac))
      ⦃ r => r = some (0x4d535350#u32, 1#u32, 0#u32, 1#u32) ⦄ := by
  have hinput : (Slice.from [0x4d#u8, 0x53#u8, 0x53#u8, 0x50#u8, 0#u8, 0#u8, 0#u8, 1#u8,
        0#u8, 0#u8, 0#u8, 0#u8, 0#u8, 0#u8, 0#u8, 1#u8] (by scalar_tac) : Aeneas.Std.Slice U8).val =
      header_fields 0x4d535350#u32 1#u32 1#u32 ++ ([] : List U8) := by
    simp only [Aeneas.Std.Slice.from_val]
    simp [header_fields, BitVec.toBEBytes, BitVec.toLEBytes]
    decide
  have h := header_roundtrip 0x4d535350#u32 1#u32 1#u32 [] _ hinput
  apply WP.spec_mono h
  intro r hr
  exact hr

/-- Sanity: a one-byte input is rejected as too short. -/
theorem sanity_short :
    mssp_frame.mssp_frame.decode_header_fields (Slice.from [0x4d#u8] (by scalar_tac))
      ⦃ r => r = none ⦄ := by
  have hlen : (Slice.from [0x4d#u8] (by scalar_tac) : Aeneas.Std.Slice U8).length < 16 := by
    simp [Aeneas.Std.Slice.from_val, Slice.length]
  have h := short_rejected (Slice.from [0x4d#u8] (by scalar_tac)) hlen
  apply WP.spec_mono h
  intro r hr
  exact hr

end mssp_frame_proof
