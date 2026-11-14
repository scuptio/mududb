import support.bit_bytes
import support.sequences
import crates.common.mudu_binding.src.universal.mp_core.spec
open Aeneas Std Result verification_sequences verification_bit_bytes
open mp_core_spec

namespace mp_core_proof

/-- The encoder appends the canonical minimal-width encoding. -/
@[step] theorem uint_encode (out : alloc.vec.Vec U8) (v : U64)
    (h : out.val.length + 9 ≤ Usize.max) :
    mp_core.mp_core.write_uint out v ⦃ out' => out'.val = out.val ++ enc_uint v ⦄ := by
  unfold mp_core.mp_core.write_uint
  split
  · step as ⟨i, hi⟩
    step as ⟨out', ho⟩
    have hle : v.val ≤ 127 := by scalar_tac
    have hi' : i = low_byte v.bv := by
      simp only [hi, low_byte, UScalar.cast, BitVec.truncate_eq_setWidth,
        UScalarTy.U8_numBits_eq]
    simp [enc_uint, show v.val ≤ 0x7f by omega, ho, hi']
  · step as ⟨m8, hm8⟩
    have hm8' : m8.val = 0xff := by simp [hm8, U8.rMax]
    split
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨i, hi⟩
      step as ⟨out', ho⟩
      have hlo : ¬ v.val ≤ 0x7f := by scalar_tac
      have hle : v.val ≤ 0xff := by scalar_tac
      have hi' : i = low_byte v.bv := by
        simp only [hi, low_byte, UScalar.cast, BitVec.truncate_eq_setWidth,
          UScalarTy.U8_numBits_eq]
      simp [enc_uint, hlo, hle, ho, ho1, hi', mp_core.mp_core.MARKER_U8]
    · step as ⟨m16, hm16⟩
      have hm16' : m16.val = 0xffff := by simp [hm16, U16.rMax]
      split
      · step as ⟨out1, ho1⟩
        have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
        step as ⟨i2, hi2⟩
        step as ⟨a, ha⟩
        step as ⟨s, hs⟩
        have hlens : s.val.length = 2 := by simp [hs, ha, Array.val_to_slice]
        step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
        have hlo : ¬ v.val ≤ 0xff := by scalar_tac
        have hle : v.val ≤ 0xffff := by scalar_tac
        have hi2' : i2.bv = v.bv.setWidth 16 := by
          simp only [hi2, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U16_numBits_eq]
        simp [enc_uint, show ¬ v.val ≤ 0x7f by omega, hlo, hle, ho, ho1, hi2', ha, hs,
          be_bytes_of, Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_U16]
      · step as ⟨m32, hm32⟩
        have hm32' : m32.val = 0xffffffff := by simp [hm32, U32.rMax]
        split
        · step as ⟨out1, ho1⟩
          have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
          step as ⟨i3, hi3⟩
          step as ⟨a, ha⟩
          step as ⟨s, hs⟩
          have hlens : s.val.length = 4 := by simp [hs, ha, Array.val_to_slice]
          step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
          have hlo : ¬ v.val ≤ 0xffff := by scalar_tac
          have hle : v.val ≤ 0xffffffff := by scalar_tac
          have hi3' : i3.bv = v.bv.setWidth 32 := by
            simp only [hi3, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U32_numBits_eq]
          simp [enc_uint, show ¬ v.val ≤ 0x7f by omega, show ¬ v.val ≤ 0xff by omega, hlo, hle,
            ho, ho1, hi3', ha, hs, be_bytes_of, Array.val_to_slice, List.append_assoc,
            mp_core.mp_core.MARKER_U32]
        · step as ⟨out1, ho1⟩
          have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
          step as ⟨a, ha⟩
          step as ⟨s, hs⟩
          have hlens : s.val.length = 8 := by simp [hs, ha, Array.val_to_slice]
          step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
          have hlo : ¬ v.val ≤ 0xffffffff := by scalar_tac
          simp [enc_uint, show ¬ v.val ≤ 0x7f by omega, show ¬ v.val ≤ 0xff by omega,
            show ¬ v.val ≤ 0xffff by omega, hlo, ho, ho1, ha, hs, be_bytes_of,
            Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_U64]

/-- Val-level roundtrip of the two-byte big-endian encoding. -/
private theorem be16_roundtrip (b : BitVec 16) :
    ∃ b0 b1 : U8, be_bytes_of b = [b0, b1] ∧ b0.val * 256 + b1.val = b.toNat := by unfold be_bytes_of; exact toBEBytes_val16 b

/-- Val-level roundtrip of the four-byte big-endian encoding. -/
private theorem be32_roundtrip (b : BitVec 32) :
    ∃ b0 b1 b2 b3 : U8, be_bytes_of b = [b0, b1, b2, b3] ∧
      b0.val * 16777216 + b1.val * 65536 + b2.val * 256 + b3.val = b.toNat := by unfold be_bytes_of; exact toBEBytes_val32 b

/-- Val-level roundtrip of the eight-byte big-endian encoding. -/
private theorem be64_roundtrip (b : BitVec 64) :
    ∃ b0 b1 b2 b3 b4 b5 b6 b7 : U8, be_bytes_of b = [b0, b1, b2, b3, b4, b5, b6, b7] ∧
      b0.val * 72057594037927936 + b1.val * 281474976710656 + b2.val * 1099511627776 +
        b3.val * 4294967296 + b4.val * 16777216 + b5.val * 65536 + b6.val * 256 + b7.val =
        b.toNat := by unfold be_bytes_of; exact toBEBytes_val64 b

/-- One-byte big-endian payload reader, success case. -/
@[step] theorem read_be1_ok (input : Aeneas.Std.Slice U8) (h : 2 ≤ input.length) :
    mp_core.mp_core.read_be1 input ⦃ r => r = .Ok input.val[1]! ⦄ := by
  unfold mp_core.mp_core.read_be1
  dsimp only
  split
  · scalar_tac
  · step as ⟨b1, hb1⟩
    rw [getElem!_pos input.val 1 (by scalar_tac)]
    simp [hb1]

/-- One-byte big-endian payload reader, short-input case. -/
theorem read_be1_short (input : Aeneas.Std.Slice U8) (h : input.length < 2) :
    mp_core.mp_core.read_be1 input ⦃ r => r = .Err .UnexpectedEnd ⦄ := by
  unfold mp_core.mp_core.read_be1
  dsimp only
  split
  · simp
  · scalar_tac

/-- Two-byte big-endian payload reader, success case. -/
@[step] theorem read_be2_ok (input : Aeneas.Std.Slice U8) (h : 3 ≤ input.length) :
    mp_core.mp_core.read_be2 input ⦃ r =>
      ∃ w : U16, r = .Ok w ∧ w.val = input.val[1]!.val * 256 + input.val[2]!.val ⦄ := by
  unfold mp_core.mp_core.read_be2
  dsimp only
  split
  · scalar_tac
  · step as ⟨b1, hb1⟩
    step as ⟨c1, hc1⟩
    step as ⟨m, hm⟩
    step as ⟨b2, hb2⟩
    step as ⟨c2, hc2⟩
    step as ⟨w, hw⟩
    refine ⟨w, rfl, ?_⟩
    have hb1' : b1 = input.val[1]! := by
      simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
    have hb2' : b2 = input.val[2]! := by
      simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
    subst hb1' hb2'
    scalar_tac

/-- Two-byte big-endian payload reader, short-input case. -/
theorem read_be2_short (input : Aeneas.Std.Slice U8) (h : input.length < 3) :
    mp_core.mp_core.read_be2 input ⦃ r => r = .Err .UnexpectedEnd ⦄ := by
  unfold mp_core.mp_core.read_be2
  dsimp only
  split
  · simp
  · scalar_tac

/-- Four-byte big-endian payload reader, success case. -/
@[step] theorem read_be4_ok (input : Aeneas.Std.Slice U8) (h : 5 ≤ input.length) :
    mp_core.mp_core.read_be4 input ⦃ r =>
      ∃ w : U32, r = .Ok w ∧
        w.val = input.val[1]!.val * 16777216 + input.val[2]!.val * 65536 +
          input.val[3]!.val * 256 + input.val[4]!.val ⦄ := by
  unfold mp_core.mp_core.read_be4
  dsimp only
  split
  · scalar_tac
  · step as ⟨b1, hb1⟩
    step as ⟨c1, hc1⟩
    step as ⟨m1, hm1⟩
    step as ⟨b2, hb2⟩
    step as ⟨c2, hc2⟩
    step as ⟨m2, hm2⟩
    step as ⟨a1, ha1⟩
    step as ⟨b3, hb3⟩
    step as ⟨c3, hc3⟩
    step as ⟨m3, hm3⟩
    step as ⟨a2, ha2⟩
    step as ⟨b4, hb4⟩
    step as ⟨c4, hc4⟩
    step as ⟨w, hw⟩
    refine ⟨w, rfl, ?_⟩
    have hb1' : b1 = input.val[1]! := by
      simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
    have hb2' : b2 = input.val[2]! := by
      simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
    have hb3' : b3 = input.val[3]! := by
      simpa only [getElem!_pos input.val 3 (by scalar_tac)] using hb3
    have hb4' : b4 = input.val[4]! := by
      simpa only [getElem!_pos input.val 4 (by scalar_tac)] using hb4
    subst hb1' hb2' hb3' hb4'
    scalar_tac

/-- Four-byte big-endian payload reader, short-input case. -/
theorem read_be4_short (input : Aeneas.Std.Slice U8) (h : input.length < 5) :
    mp_core.mp_core.read_be4 input ⦃ r => r = .Err .UnexpectedEnd ⦄ := by
  unfold mp_core.mp_core.read_be4
  dsimp only
  split
  · simp
  · scalar_tac

/-- Eight-byte big-endian payload reader, success case. -/
@[step] theorem read_be8_ok (input : Aeneas.Std.Slice U8) (h : 9 ≤ input.length) :
    mp_core.mp_core.read_be8 input ⦃ r =>
      ∃ w : U64, r = .Ok w ∧
        w.val = input.val[1]!.val * 72057594037927936 + input.val[2]!.val * 281474976710656 +
          input.val[3]!.val * 1099511627776 + input.val[4]!.val * 4294967296 +
          input.val[5]!.val * 16777216 + input.val[6]!.val * 65536 +
          input.val[7]!.val * 256 + input.val[8]!.val ⦄ := by
  unfold mp_core.mp_core.read_be8
  dsimp only
  split
  · scalar_tac
  · step as ⟨b1, hb1⟩
    step as ⟨c1, hc1⟩
    step as ⟨m1, hm1⟩
    step as ⟨b2, hb2⟩
    step as ⟨c2, hc2⟩
    step as ⟨m2, hm2⟩
    step as ⟨a1, ha1⟩
    step as ⟨b3, hb3⟩
    step as ⟨c3, hc3⟩
    step as ⟨m3, hm3⟩
    step as ⟨a2, ha2⟩
    step as ⟨b4, hb4⟩
    step as ⟨c4, hc4⟩
    step as ⟨m4, hm4⟩
    step as ⟨a3, ha3⟩
    step as ⟨b5, hb5⟩
    step as ⟨c5, hc5⟩
    step as ⟨m5, hm5⟩
    step as ⟨a4, ha4⟩
    step as ⟨b6, hb6⟩
    step as ⟨c6, hc6⟩
    step as ⟨m6, hm6⟩
    step as ⟨a5, ha5⟩
    step as ⟨b7, hb7⟩
    step as ⟨c7, hc7⟩
    step as ⟨m7, hm7⟩
    step as ⟨a6, ha6⟩
    step as ⟨b8, hb8⟩
    step as ⟨c8, hc8⟩
    step as ⟨w, hw⟩
    refine ⟨w, rfl, ?_⟩
    have hb1' : b1 = input.val[1]! := by
      simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
    have hb2' : b2 = input.val[2]! := by
      simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
    have hb3' : b3 = input.val[3]! := by
      simpa only [getElem!_pos input.val 3 (by scalar_tac)] using hb3
    have hb4' : b4 = input.val[4]! := by
      simpa only [getElem!_pos input.val 4 (by scalar_tac)] using hb4
    have hb5' : b5 = input.val[5]! := by
      simpa only [getElem!_pos input.val 5 (by scalar_tac)] using hb5
    have hb6' : b6 = input.val[6]! := by
      simpa only [getElem!_pos input.val 6 (by scalar_tac)] using hb6
    have hb7' : b7 = input.val[7]! := by
      simpa only [getElem!_pos input.val 7 (by scalar_tac)] using hb7
    have hb8' : b8 = input.val[8]! := by
      simpa only [getElem!_pos input.val 8 (by scalar_tac)] using hb8
    subst hb1' hb2' hb3' hb4' hb5' hb6' hb7' hb8'
    scalar_tac

/-- Eight-byte big-endian payload reader, short-input case. -/
theorem read_be8_short (input : Aeneas.Std.Slice U8) (h : input.length < 9) :
    mp_core.mp_core.read_be8 input ⦃ r => r = .Err .UnexpectedEnd ⦄ := by
  unfold mp_core.mp_core.read_be8
  dsimp only
  split
  · simp
  · scalar_tac

/-- The decoder reads back the canonical encoding and consumes exactly its
    bytes, leaving any suffix untouched. -/
theorem uint_decode (v : U64) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_uint v ++ rest) :
    mp_core.mp_core.read_uint input ⦃ r =>
      ∃ n : Usize, r = .Ok (v, n) ∧ n.val = (enc_uint v).length ⦄ := by
  by_cases h0 : v.val ≤ 0x7f
  · have he : enc_uint v = [low_byte v.bv] := by simp [enc_uint, h0]
    have hin : input.val = low_byte v.bv :: rest := by simp [h, he]
    unfold mp_core.mp_core.read_uint
    step as ⟨b, hb⟩
    have hb' : b = false := by
      have hlen : input.length = rest.length + 1 := by simp [Slice.length, hin]
      rw [hlen] at hb
      cases b <;> simp_all
    simp only [hb', Bool.false_eq_true, ↓reduceIte]
    step as ⟨marker, hm⟩
    have hmk : marker = low_byte v.bv := by simp [hm, hin]
    have hmkv : (low_byte v.bv).val = v.val := by
      have hvv : v.bv.toNat = v.val := rfl
      simp only [low_byte, UScalar.val, UScalar.bv_mk_apply, BitVec.toNat_setWidth,
        UScalarTy.U8_numBits_eq, UScalarTy.U64_numBits_eq]
      exact Nat.mod_eq_of_lt (by omega)
    have hmle : marker ≤ 127#u8 := by rw [hmk]; scalar_tac
    simp only [hmle, ↓reduceIte]
    step as ⟨i, hi⟩
    have hi' : i = v := by
      apply UScalar.eq_of_val_eq
      rw [hi, hmk]
      scalar_tac
    refine ⟨1#usize, by rw [hi'], by simp [he]⟩
  · by_cases h1 : v.val ≤ 0xff
    · have he : enc_uint v = [0xcc#u8, low_byte v.bv] := by simp [enc_uint, h0, h1]
      have hin : input.val = 0xcc#u8 :: low_byte v.bv :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_uint
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 2 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xcc#u8 := by simp [hm, hin]
      have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
      simp only [hle, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_U8 := by
        simp only [hmk, mp_core.mp_core.MARKER_U8]
      simp only [hmk', ↓reduceIte]
      have hlen2 : 2 ≤ input.length := by
        rw [show input.length = rest.length + 2 from by simp [Slice.length, hin]]; omega
      step as ⟨r, hr⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      step as ⟨i, hi⟩
      have hb1 : input.val[1]! = low_byte v.bv := by simp [hin]
      have hmkv : (low_byte v.bv).val = v.val := by
        have hvv : v.bv.toNat = v.val := rfl
        simp only [low_byte, UScalar.val, UScalar.bv_mk_apply, BitVec.toNat_setWidth,
          UScalarTy.U8_numBits_eq, UScalarTy.U64_numBits_eq]
        exact Nat.mod_eq_of_lt (by omega)
      have hi' : i = v := by
        apply UScalar.eq_of_val_eq
        rw [hi, hb1]
        scalar_tac
      refine ⟨2#usize, by rw [hi'], by simp [he]⟩
    · by_cases h2 : v.val ≤ 0xffff
      · obtain ⟨b0, b1, hbb, hvals⟩ := be16_roundtrip (v.bv.setWidth 16)
        have he : enc_uint v = [0xcd#u8, b0, b1] := by simp [enc_uint, h0, h1, h2, hbb]
        have hin : input.val = 0xcd#u8 :: b0 :: b1 :: rest := by simp [h, he]
        unfold mp_core.mp_core.read_uint
        step as ⟨b, hb⟩
        have hb' : b = false := by
          have hlen : input.length = rest.length + 3 := by simp [Slice.length, hin]
          rw [hlen] at hb
          cases b <;> simp_all
        simp only [hb', Bool.false_eq_true, ↓reduceIte]
        step as ⟨marker, hm⟩
        have hmk : marker = 0xcd#u8 := by simp [hm, hin]
        have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
        simp only [hle, ↓reduceIte]
        have hne8 : ¬ (marker = mp_core.mp_core.MARKER_U8) := by
          rw [hmk]; simp only [mp_core.mp_core.MARKER_U8]; scalar_tac
        simp only [hne8, ↓reduceIte]
        have hmk' : marker = mp_core.mp_core.MARKER_U16 := by
          simp only [hmk, mp_core.mp_core.MARKER_U16]
        simp only [hmk', ↓reduceIte]
        have hlen3 : 3 ≤ input.length := by
          rw [show input.length = rest.length + 3 from by simp [Slice.length, hin]]; omega
        step as ⟨r, w, hr, hw⟩
        rw [hr]
        simp only [core.result.Result.Insts.CoreOpsTry.branch]
        step as ⟨i, hi⟩
        have hb0 : input.val[1]! = b0 := by simp [hin]
        have hb1 : input.val[2]! = b1 := by simp [hin]
        rw [hb0, hb1] at hw
        have hvv : v.bv.toNat = v.val := rfl
        have hv16 : (v.bv.setWidth 16).toNat = v.val := by
          simp only [BitVec.toNat_setWidth]; omega
        have hi' : i = v := by
          apply UScalar.eq_of_val_eq
          rw [hi]
          scalar_tac
        refine ⟨3#usize, by rw [hi'], by simp [he]⟩
      · by_cases h3 : v.val ≤ 0xffffffff
        · obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip (v.bv.setWidth 32)
          have he : enc_uint v = [0xce#u8, b0, b1, b2, b3] := by simp [enc_uint, h0, h1, h2, h3, hbb]
          have hin : input.val = 0xce#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
          unfold mp_core.mp_core.read_uint
          step as ⟨b, hb⟩
          have hb' : b = false := by
            have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
            rw [hlen] at hb
            cases b <;> simp_all
          simp only [hb', Bool.false_eq_true, ↓reduceIte]
          step as ⟨marker, hm⟩
          have hmk : marker = 0xce#u8 := by simp [hm, hin]
          have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
          simp only [hle, ↓reduceIte]
          have hne8 : ¬ (marker = mp_core.mp_core.MARKER_U8) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_U8]; scalar_tac
          simp only [hne8, ↓reduceIte]
          have hne16 : ¬ (marker = mp_core.mp_core.MARKER_U16) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_U16]; scalar_tac
          simp only [hne16, ↓reduceIte]
          have hmk' : marker = mp_core.mp_core.MARKER_U32 := by
            simp only [hmk, mp_core.mp_core.MARKER_U32]
          simp only [hmk', ↓reduceIte]
          have hlen5 : 5 ≤ input.length := by
            rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
          step as ⟨r, w, hr, hw⟩
          rw [hr]
          simp only [core.result.Result.Insts.CoreOpsTry.branch]
          step as ⟨i, hi⟩
          have hb0 : input.val[1]! = b0 := by simp [hin]
          have hb1 : input.val[2]! = b1 := by simp [hin]
          have hb2 : input.val[3]! = b2 := by simp [hin]
          have hb3 : input.val[4]! = b3 := by simp [hin]
          rw [hb0, hb1, hb2, hb3] at hw
          have hvv : v.bv.toNat = v.val := rfl
          have hv32 : (v.bv.setWidth 32).toNat = v.val := by
            simp only [BitVec.toNat_setWidth]; omega
          have hi' : i = v := by
            apply UScalar.eq_of_val_eq
            rw [hi]
            scalar_tac
          refine ⟨5#usize, by rw [hi'], by simp [he]⟩
        · obtain ⟨b0, b1, b2, b3, b4, b5, b6, b7, hbb, hvals⟩ := be64_roundtrip v.bv
          have he : enc_uint v = [0xcf#u8, b0, b1, b2, b3, b4, b5, b6, b7] := by
            simp [enc_uint, h0, h1, h2, h3, hbb]
          have hin : input.val = 0xcf#u8 :: b0 :: b1 :: b2 :: b3 :: b4 :: b5 :: b6 :: b7 :: rest := by
            simp [h, he]
          unfold mp_core.mp_core.read_uint
          step as ⟨b, hb⟩
          have hb' : b = false := by
            have hlen : input.length = rest.length + 9 := by simp [Slice.length, hin]
            rw [hlen] at hb
            cases b <;> simp_all
          simp only [hb', Bool.false_eq_true, ↓reduceIte]
          step as ⟨marker, hm⟩
          have hmk : marker = 0xcf#u8 := by simp [hm, hin]
          have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
          simp only [hle, ↓reduceIte]
          have hne8 : ¬ (marker = mp_core.mp_core.MARKER_U8) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_U8]; scalar_tac
          simp only [hne8, ↓reduceIte]
          have hne16 : ¬ (marker = mp_core.mp_core.MARKER_U16) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_U16]; scalar_tac
          simp only [hne16, ↓reduceIte]
          have hne32 : ¬ (marker = mp_core.mp_core.MARKER_U32) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_U32]; scalar_tac
          simp only [hne32, ↓reduceIte]
          have hmk' : marker = mp_core.mp_core.MARKER_U64 := by
            simp only [hmk, mp_core.mp_core.MARKER_U64]
          simp only [hmk', ↓reduceIte]
          have hlen9 : 9 ≤ input.length := by
            rw [show input.length = rest.length + 9 from by simp [Slice.length, hin]]; omega
          step as ⟨r, w, hr, hw⟩
          rw [hr]
          simp only [core.result.Result.Insts.CoreOpsTry.branch]
          have hb0 : input.val[1]! = b0 := by simp [hin]
          have hb1 : input.val[2]! = b1 := by simp [hin]
          have hb2 : input.val[3]! = b2 := by simp [hin]
          have hb3 : input.val[4]! = b3 := by simp [hin]
          have hb4 : input.val[5]! = b4 := by simp [hin]
          have hb5 : input.val[6]! = b5 := by simp [hin]
          have hb6 : input.val[7]! = b6 := by simp [hin]
          have hb7 : input.val[8]! = b7 := by simp [hin]
          rw [hb0, hb1, hb2, hb3, hb4, hb5, hb6, hb7] at hw
          have hvv : v.bv.toNat = v.val := rfl
          have hw' : w = v := by
            apply UScalar.eq_of_val_eq
            scalar_tac
          apply WP.spec_bind (Pₘ := fun cf =>
            cf = core.ops.control_flow.ControlFlow.Continue w)
          · exact (WP.spec_ok _).mpr rfl
          · intro cf hcf
            subst hcf
            dsimp only
            rw [WP.spec_ok]
            refine ⟨9#usize, by rw [hw'], by simp [he]⟩

/-- The signed encoding of a non-negative value is exactly its unsigned
    encoding: the two marker chains coincide there. -/
theorem sint_nonneg_unsigned_chain (v : I64) (hv : 0 ≤ v.val) :
    enc_sint v = enc_uint (UScalar.mk v.bv : U64) := by
  simp [enc_sint, hv]

/-- Signed integers encode via the minimal-width chain; non-negative values
    share the unsigned chain. -/
theorem sint_encode (out : alloc.vec.Vec U8) (v : I64)
    (h : out.val.length + 9 ≤ Usize.max) :
    mp_core.mp_core.write_sint out v ⦃ out' => out'.val = out.val ++ enc_sint v ⦄ := by
  unfold mp_core.mp_core.write_sint
  split
  · step as ⟨i, hi⟩
    step as ⟨out', ho⟩
    have hge : 0 ≤ v.val := by scalar_tac
    have hi' : i = (UScalar.mk v.bv : U64) := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [hi, IScalar.hcast_bv_eq, UScalarTy.U64_numBits_eq]
      rw [BitVec.signExtend_eq_setWidth_of_le v.bv (Nat.le_refl 64)]
      exact BitVec.setWidth_eq _
    simp [enc_sint, hge, ho, hi']
  · split
    · step as ⟨i, hi⟩
      step as ⟨out', ho⟩
      have hge : ¬ 0 ≤ v.val := by scalar_tac
      have hge32 : -32 ≤ v.val := by scalar_tac
      have hi' : i = low_byte v.bv := by
        apply (UScalar.eq_equiv_bv_eq _ _).mpr
        simp only [hi, IScalar.hcast_bv_eq, low_byte, UScalarTy.U8_numBits_eq]
        rw [BitVec.signExtend_eq_setWidth_of_le v.bv (by decide)]
      simp [enc_sint, show ¬ 0 ≤ v.val by omega, hge32, ho, hi']
    · step as ⟨m8, hm8⟩
      have hm8' : m8.val = -128 := by simp [hm8, I8.rMin]
      split
      · step as ⟨out1, ho1⟩
        have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
        step as ⟨i, hi⟩
        step as ⟨out', ho⟩
        have hge : ¬ 0 ≤ v.val := by scalar_tac
        have hge32 : ¬ -32 ≤ v.val := by scalar_tac
        have hge128 : -128 ≤ v.val := by scalar_tac
        have hi' : i = low_byte v.bv := by
          apply (UScalar.eq_equiv_bv_eq _ _).mpr
          simp only [hi, IScalar.hcast_bv_eq, low_byte, UScalarTy.U8_numBits_eq]
          rw [BitVec.signExtend_eq_setWidth_of_le v.bv (by decide)]
        simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega, hge128, ho, ho1,
          hi', mp_core.mp_core.MARKER_I8]
      · step as ⟨m16, hm16⟩
        have hm16' : m16.val = -32768 := by simp [hm16, I16.rMin]
        split
        · step as ⟨out1, ho1⟩
          have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
          step as ⟨i2, hi2⟩
          step as ⟨a, ha⟩
          step as ⟨s2, hs2⟩
          have hlens : s2.val.length = 2 := by simp [hs2, ha, Array.val_to_slice]
          step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
          have hge : ¬ 0 ≤ v.val := by scalar_tac
          have hge32 : ¬ -32 ≤ v.val := by scalar_tac
          have hge128 : ¬ -128 ≤ v.val := by scalar_tac
          have hge32768 : -32768 ≤ v.val := by scalar_tac
          have hi2' : i2.bv = v.bv.setWidth 16 := by
            simp only [hi2, IScalar.cast_bv_eq, IScalarTy.I16_numBits_eq]
            rw [BitVec.signExtend_eq_setWidth_of_le v.bv (by decide)]
          simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega,
            show ¬ -128 ≤ v.val by omega, hge32768, ho, ho1, hi2', ha, hs2, be_bytes_of,
            Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_I16]
        · step as ⟨m32, hm32⟩
          have hm32' : m32.val = -2147483648 := by simp [hm32, I32.rMin]
          split
          · step as ⟨out1, ho1⟩
            have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
            step as ⟨i3, hi3⟩
            step as ⟨a, ha⟩
            step as ⟨s2, hs2⟩
            have hlens : s2.val.length = 4 := by simp [hs2, ha, Array.val_to_slice]
            step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
            have hge : ¬ 0 ≤ v.val := by scalar_tac
            have hge32 : ¬ -32 ≤ v.val := by scalar_tac
            have hge128 : ¬ -128 ≤ v.val := by scalar_tac
            have hge32768 : ¬ -32768 ≤ v.val := by scalar_tac
            have hi3' : i3.bv = v.bv.setWidth 32 := by
              simp only [hi3, IScalar.cast_bv_eq, IScalarTy.I32_numBits_eq]
              rw [BitVec.signExtend_eq_setWidth_of_le v.bv (by decide)]
            simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega,
              show ¬ -128 ≤ v.val by omega, show ¬ -32768 ≤ v.val by omega,
              show -2147483648 ≤ v.val by scalar_tac, ho, ho1, hi3', ha,
              hs2, be_bytes_of, Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_I32]
          · step as ⟨out1, ho1⟩
            have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
            step as ⟨a, ha⟩
            step as ⟨s2, hs2⟩
            have hlens : s2.val.length = 8 := by simp [hs2, ha, Array.val_to_slice]
            step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
            simp [enc_sint, show ¬ 0 ≤ v.val by scalar_tac, show ¬ -32 ≤ v.val by scalar_tac,
              show ¬ -128 ≤ v.val by scalar_tac, show ¬ -32768 ≤ v.val by scalar_tac,
              show ¬ -2147483648 ≤ v.val by scalar_tac, ho, ho1, ha, hs2, be_bytes_of,
              Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_I64]

/-- The boolean encoder appends its canonical single byte. -/
theorem bool_encode (out : alloc.vec.Vec U8) (v : Bool)
    (h : out.val.length + 1 ≤ Usize.max) :
    mp_core.mp_core.write_bool out v ⦃ out' => out'.val = out.val ++ enc_bool v ⦄ := by
  unfold mp_core.mp_core.write_bool
  cases v
  · simp only [show ((false : Bool) = true) = False by decide, ↓reduceIte]
    step as ⟨out', ho⟩
    simp [enc_bool, ho, mp_core.mp_core.MARKER_FALSE]
  · simp only [↓reduceIte]
    step as ⟨out', ho⟩
    simp [enc_bool, ho, mp_core.mp_core.MARKER_TRUE]

/-- The decoder reads back a canonical boolean. -/
theorem bool_decode (v : Bool) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_bool v ++ rest) :
    mp_core.mp_core.read_bool input ⦃ r =>
      ∃ n : Usize, r = .Ok (v, n) ∧ n.val = (enc_bool v).length ⦄ := by
  unfold mp_core.mp_core.read_bool
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = rest.length + 1 := by simp [Slice.length, h, enc_bool]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  cases v
  · have hmk : marker = mp_core.mp_core.MARKER_FALSE := by
      simp [hm, h, enc_bool, mp_core.mp_core.MARKER_FALSE]
    simp only [hmk, ↓reduceIte]
    rw [WP.spec_ok]
    refine ⟨1#usize, rfl, by simp [enc_bool]⟩
  · have hmk : marker = mp_core.mp_core.MARKER_TRUE := by
      simp [hm, h, enc_bool, mp_core.mp_core.MARKER_TRUE]
    have hne : ¬ (marker = mp_core.mp_core.MARKER_FALSE) := by
      rw [hmk]; simp only [mp_core.mp_core.MARKER_FALSE, mp_core.mp_core.MARKER_TRUE]; scalar_tac
    simp only [hne, ↓reduceIte]
    simp only [hmk, ↓reduceIte]
    rw [WP.spec_ok]
    refine ⟨1#usize, rfl, by simp [enc_bool]⟩

/-- The decoder accepts canonical nil. -/
theorem nil_decode (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_nil ++ rest) :
    mp_core.mp_core.read_nil input ⦃ r => r = .Ok 1#usize ⦄ := by
  unfold mp_core.mp_core.read_nil
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = rest.length + 1 := by simp [Slice.length, h, enc_nil]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = mp_core.mp_core.MARKER_NIL := by
    simp [hm, h, enc_nil, mp_core.mp_core.MARKER_NIL]
  simp only [hmk, ↓reduceIte]
  rw [WP.spec_ok]

/-- The f32 encoder appends the marker and the big-endian bit pattern. -/
theorem f32_encode (out : alloc.vec.Vec U8) (bits : U32)
    (h : out.val.length + 5 ≤ Usize.max) :
    mp_core.mp_core.write_f32_bits out bits ⦃ out' =>
      out'.val = out.val ++ enc_f32_bits bits ⦄ := by
  unfold mp_core.mp_core.write_f32_bits
  step as ⟨out1, ho1⟩
  have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
  step as ⟨a, ha⟩
  step as ⟨s2, hs2⟩
  have hlens : s2.val.length = 4 := by simp [hs2, ha, Array.val_to_slice]
  step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
  simp [enc_f32_bits, ho, ho1, ha, hs2, be_bytes_of, Array.val_to_slice,
    mp_core.mp_core.MARKER_F32]

/-- The f64 encoder appends the marker and the big-endian bit pattern. -/
theorem f64_encode (out : alloc.vec.Vec U8) (bits : U64)
    (h : out.val.length + 9 ≤ Usize.max) :
    mp_core.mp_core.write_f64_bits out bits ⦃ out' =>
      out'.val = out.val ++ enc_f64_bits bits ⦄ := by
  unfold mp_core.mp_core.write_f64_bits
  step as ⟨out1, ho1⟩
  have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
  step as ⟨a, ha⟩
  step as ⟨s2, hs2⟩
  have hlens : s2.val.length = 8 := by simp [hs2, ha, Array.val_to_slice]
  step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
  simp [enc_f64_bits, ho, ho1, ha, hs2, be_bytes_of, Array.val_to_slice,
    mp_core.mp_core.MARKER_F64]

/-- The decoder reads back the f32 bit pattern, consuming five bytes. -/
theorem f32_decode (bits : U32) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_f32_bits bits ++ rest) :
    mp_core.mp_core.read_f32_bits input ⦃ r =>
      ∃ n : Usize, r = .Ok (bits, n) ∧ n.val = (enc_f32_bits bits).length ⦄ := by
  obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip bits.bv
  have he : enc_f32_bits bits = [0xca#u8, b0, b1, b2, b3] := by simp [enc_f32_bits, hbb]
  have hin : input.val = 0xca#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
  unfold mp_core.mp_core.read_f32_bits
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xca#u8 := by simp [hm, hin]
  have hmk' : (marker != mp_core.mp_core.MARKER_F32) = false := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_F32]; decide
  simp only [hmk', Bool.false_eq_true, ↓reduceIte]
  have hlen5 : 5 ≤ input.length := by
    rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
  step as ⟨r, w, hr, hw⟩
  rw [hr]
  simp only [core.result.Result.Insts.CoreOpsTry.branch]
  have hb0 : input.val[1]! = b0 := by simp [hin]
  have hb1 : input.val[2]! = b1 := by simp [hin]
  have hb2 : input.val[3]! = b2 := by simp [hin]
  have hb3 : input.val[4]! = b3 := by simp [hin]
  rw [hb0, hb1, hb2, hb3] at hw
  have hwv : w = bits := by
    apply UScalar.eq_of_val_eq
    scalar_tac
  apply WP.spec_bind (Pₘ := fun cf =>
    cf = core.ops.control_flow.ControlFlow.Continue w)
  · exact (WP.spec_ok _).mpr rfl
  · intro cf hcf
    subst hcf
    dsimp only
    rw [WP.spec_ok]
    refine ⟨5#usize, by rw [hwv], by simp [he]⟩

/-- The decoder reads back the f64 bit pattern, consuming nine bytes. -/
theorem f64_decode (bits : U64) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_f64_bits bits ++ rest) :
    mp_core.mp_core.read_f64_bits input ⦃ r =>
      ∃ n : Usize, r = .Ok (bits, n) ∧ n.val = (enc_f64_bits bits).length ⦄ := by
  obtain ⟨b0, b1, b2, b3, b4, b5, b6, b7, hbb, hvals⟩ := be64_roundtrip bits.bv
  have he : enc_f64_bits bits = [0xcb#u8, b0, b1, b2, b3, b4, b5, b6, b7] := by
    simp [enc_f64_bits, hbb]
  have hin : input.val = 0xcb#u8 :: b0 :: b1 :: b2 :: b3 :: b4 :: b5 :: b6 :: b7 :: rest := by
    simp [h, he]
  unfold mp_core.mp_core.read_f64_bits
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = rest.length + 9 := by simp [Slice.length, hin]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xcb#u8 := by simp [hm, hin]
  have hmk' : (marker != mp_core.mp_core.MARKER_F64) = false := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_F64]; decide
  simp only [hmk', Bool.false_eq_true, ↓reduceIte]
  have hlen9 : 9 ≤ input.length := by
    rw [show input.length = rest.length + 9 from by simp [Slice.length, hin]]; omega
  step as ⟨r, w, hr, hw⟩
  rw [hr]
  simp only [core.result.Result.Insts.CoreOpsTry.branch]
  have hb0 : input.val[1]! = b0 := by simp [hin]
  have hb1 : input.val[2]! = b1 := by simp [hin]
  have hb2 : input.val[3]! = b2 := by simp [hin]
  have hb3 : input.val[4]! = b3 := by simp [hin]
  have hb4 : input.val[5]! = b4 := by simp [hin]
  have hb5 : input.val[6]! = b5 := by simp [hin]
  have hb6 : input.val[7]! = b6 := by simp [hin]
  have hb7 : input.val[8]! = b7 := by simp [hin]
  rw [hb0, hb1, hb2, hb3, hb4, hb5, hb6, hb7] at hw
  have hwv : w = bits := by
    apply UScalar.eq_of_val_eq
    scalar_tac
  apply WP.spec_bind (Pₘ := fun cf =>
    cf = core.ops.control_flow.ControlFlow.Continue w)
  · exact (WP.spec_ok _).mpr rfl
  · intro cf hcf
    subst hcf
    dsimp only
    rw [WP.spec_ok]
    refine ⟨9#usize, by rw [hwv], by simp [he]⟩

/-- A negative `i64`'s bit pattern, read as an unsigned natural, is offset by
    2^64. -/
private theorem i64_bv_toNat_neg (v : I64) (h1 : v.val < 0) :
    (v.bv.toNat : ℤ) = v.val + 2 ^ 64 := by
  have hdef : v.val = Int.bmod (↑v.bv.toNat) (2 ^ 64) := BitVec.toInt_eq_toNat_bmod v.bv
  have hb : v.bv.toNat < 2 ^ 64 := BitVec.isLt v.bv
  rw [Int.bmod] at hdef
  split at hdef
  · omega
  · omega

/-- The value below the sign bit stays put under balanced modulo. -/
private theorem bmod_pow2_self_of_low (n : ℕ) (x : Int) (hn : n ≠ 0)
    (h0 : -2 ^ (n - 1) ≤ x) (h1 : x < 2 ^ (n - 1)) :
    Int.bmod x (2 ^ n) = x :=
  Aeneas.Arith.Int.bmod_pow2_eq_of_inBounds' n x hn h0 h1

/-- The value at or above the sign bit wraps down by the modulus. -/
private theorem bmod_pow2_sub_of_high (n : ℕ) (x : Int) (hn : n ≠ 0)
    (h0 : 2 ^ (n - 1) ≤ x) (h1 : x < 2 ^ n) :
    Int.bmod x (2 ^ n) = x - 2 ^ n := by
  rw [Int.bmod]
  have hp : (0 : Int) < 2 ^ (n - 1) := by positivity
  have hpow : (2 : Int) ^ n = 2 * 2 ^ (n - 1) := by
    conv_lhs => rw [show n = n - 1 + 1 from by omega]
    rw [Int.pow_succ']
  have hx : x % (2 ^ n : Int) = x := Int.emod_eq_of_lt (by omega) (by omega)
  have hth : ((2 : Int) ^ n + 1) / 2 = 2 ^ (n - 1) := by omega
  simp [hx, hth]
  omega

/-- The low byte of a negative `i64`'s bit pattern, as an integer offset. -/
private theorem i64_low_byte_int (v : I64) (h0 : -256 ≤ v.val) (h1 : v.val < 0) :
    ((low_byte v.bv).val : ℤ) = v.val + 256 := by
  have hm := i64_bv_toNat_neg v h1
  simp only [low_byte, UScalar.val, UScalar.bv_mk_apply, BitVec.toNat_setWidth,
    UScalarTy.U8_numBits_eq]
  omega

/-- The low 16 bits of a negative `i64`'s bit pattern, as an integer offset. -/
private theorem i64_setWidth16_int (v : I64) (h0 : -32768 ≤ v.val) (h1 : v.val < 0) :
    ((v.bv.setWidth 16).toNat : ℤ) = v.val + 65536 := by
  have hm := i64_bv_toNat_neg v h1
  simp only [BitVec.toNat_setWidth]
  omega

/-- The low 32 bits of a negative `i64`'s bit pattern, as an integer offset. -/
private theorem i64_setWidth32_int (v : I64) (h0 : -2147483648 ≤ v.val) (h1 : v.val < 0) :
    ((v.bv.setWidth 32).toNat : ℤ) = v.val + 4294967296 := by
  have hm := i64_bv_toNat_neg v h1
  simp only [BitVec.toNat_setWidth]
  omega

/-- The decoder reads back a negative signed integer's canonical encoding and
    consumes exactly its bytes. (Non-negative signed values encode through the
    unsigned chain — see `sint_nonneg_unsigned_chain` — and are read back by
    `read_uint`.) -/
theorem sint_decode (v : I64) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (hv : v.val < 0) (h : input.val = enc_sint v ++ rest) :
    mp_core.mp_core.read_sint input ⦃ r =>
      ∃ n : Usize, r = .Ok (v, n) ∧ n.val = (enc_sint v).length ⦄ := by
  by_cases h0 : -32 ≤ v.val
  · have he : enc_sint v = [low_byte v.bv] := by
      simp [enc_sint, show ¬ 0 ≤ v.val by omega, h0]
    have hin : input.val = low_byte v.bv :: rest := by simp [h, he]
    unfold mp_core.mp_core.read_sint
    step as ⟨b, hb⟩
    have hb' : b = false := by
      have hlen : input.length = rest.length + 1 := by simp [Slice.length, hin]
      rw [hlen] at hb
      cases b <;> simp_all
    simp only [hb', Bool.false_eq_true, ↓reduceIte]
    step as ⟨marker, hm⟩
    have hmk : marker = low_byte v.bv := by simp [hm, hin]
    have hlb : ((low_byte v.bv).val : ℤ) = v.val + 256 := i64_low_byte_int v (by omega) hv
    have hge : marker ≥ 224#u8 := by rw [hmk]; scalar_tac
    simp only [hge, ↓reduceIte]
    step as ⟨i, hi⟩
    step as ⟨i1, hi1⟩
    have hhi : (UScalar.hcast .I8 (low_byte v.bv)).val = v.val := by
      rw [UScalar.hcast_val_eq, IScalarTy.I8_numBits_eq, hlb]
      rw [Int.bmod]
      simp [Int.emod_eq_of_lt (by omega : (0:ℤ) ≤ v.val + 256)
        (by omega : v.val + 256 < 256)]
      omega
    have hi1' : i1 = v := by
      apply IScalar.eq_of_val_eq
      rw [hi1, hi, hmk]
      simp only [IScalar.cast_val_eq, IScalarTy.I64_numBits_eq, IScalarTy.I8_numBits_eq]
      rw [hhi]
      show Int.bmod v.val (2 ^ 8) = v.val
      rw [Aeneas.Arith.Int.bmod_pow2_eq_of_inBounds' 8 v.val (by decide) (by omega) (by omega)]
    refine ⟨1#usize, by rw [hi1'], by simp [he]⟩
  · by_cases h1 : -128 ≤ v.val
    · have he : enc_sint v = [0xd0#u8, low_byte v.bv] := by
        simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega, h1]
      have hin : input.val = 0xd0#u8 :: low_byte v.bv :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_sint
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 2 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xd0#u8 := by simp [hm, hin]
      have hge : ¬ (marker ≥ 224#u8) := by rw [hmk]; scalar_tac
      simp only [hge, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_I8 := by
        simp only [hmk, mp_core.mp_core.MARKER_I8]
      simp only [hmk', ↓reduceIte]
      have hlen2 : 2 ≤ input.length := by
        rw [show input.length = rest.length + 2 from by simp [Slice.length, hin]]; omega
      step as ⟨r, hr⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      step as ⟨i, hi⟩
      step as ⟨i1, hi1⟩
      have hb1 : input.val[1]! = low_byte v.bv := by simp [hin]
      have hlb : ((low_byte v.bv).val : ℤ) = v.val + 256 := i64_low_byte_int v (by omega) hv
      have hhi : (UScalar.hcast .I8 (low_byte v.bv)).val = v.val := by
        rw [UScalar.hcast_val_eq, IScalarTy.I8_numBits_eq, hlb]
        rw [bmod_pow2_sub_of_high 8 (v.val + 256) (by decide) (by omega) (by omega)]
        omega
      have hi1' : i1 = v := by
        apply IScalar.eq_of_val_eq
        rw [hi1, hi, hb1]
        simp only [IScalar.cast_val_eq, IScalarTy.I64_numBits_eq, IScalarTy.I8_numBits_eq]
        rw [hhi]
        show Int.bmod v.val (2 ^ 8) = v.val
        rw [bmod_pow2_self_of_low 8 v.val (by decide) (by omega) (by omega)]
      refine ⟨2#usize, by rw [hi1'], by simp [he]⟩
    · by_cases h2 : -32768 ≤ v.val
      · obtain ⟨b0, b1, hbb, hvals⟩ := be16_roundtrip (v.bv.setWidth 16)
        have he : enc_sint v = [0xd1#u8, b0, b1] := by
          simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega,
            show ¬ -128 ≤ v.val by omega, h2, hbb]
        have hin : input.val = 0xd1#u8 :: b0 :: b1 :: rest := by simp [h, he]
        unfold mp_core.mp_core.read_sint
        step as ⟨b, hb⟩
        have hb' : b = false := by
          have hlen : input.length = rest.length + 3 := by simp [Slice.length, hin]
          rw [hlen] at hb
          cases b <;> simp_all
        simp only [hb', Bool.false_eq_true, ↓reduceIte]
        step as ⟨marker, hm⟩
        have hmk : marker = 0xd1#u8 := by simp [hm, hin]
        have hge : ¬ (marker ≥ 224#u8) := by rw [hmk]; scalar_tac
        simp only [hge, ↓reduceIte]
        have hne8 : ¬ (marker = mp_core.mp_core.MARKER_I8) := by
          rw [hmk]; simp only [mp_core.mp_core.MARKER_I8]; scalar_tac
        simp only [hne8, ↓reduceIte]
        have hmk' : marker = mp_core.mp_core.MARKER_I16 := by
          simp only [hmk, mp_core.mp_core.MARKER_I16]
        simp only [hmk', ↓reduceIte]
        have hlen3 : 3 ≤ input.length := by
          rw [show input.length = rest.length + 3 from by simp [Slice.length, hin]]; omega
        step as ⟨r, w, hr, hw⟩
        rw [hr]
        simp only [core.result.Result.Insts.CoreOpsTry.branch]
        step as ⟨i, hi⟩
        step as ⟨i1, hi1⟩
        have hb0 : input.val[1]! = b0 := by simp [hin]
        have hb1 : input.val[2]! = b1 := by simp [hin]
        rw [hb0, hb1] at hw
        have hwv : (w.val : ℤ) = v.val + 65536 := by
          have hv16 := i64_setWidth16_int v (by omega) hv
          omega
        have hhi : (UScalar.hcast .I16 w).val = v.val := by
          rw [UScalar.hcast_val_eq, IScalarTy.I16_numBits_eq, hwv]
          rw [bmod_pow2_sub_of_high 16 (v.val + 65536) (by decide) (by omega) (by omega)]
          omega
        have hi1' : i1 = v := by
          apply IScalar.eq_of_val_eq
          rw [hi1, hi]
          simp only [IScalar.cast_val_eq, IScalarTy.I64_numBits_eq, IScalarTy.I16_numBits_eq]
          rw [hhi]
          show Int.bmod v.val (2 ^ 16) = v.val
          rw [bmod_pow2_self_of_low 16 v.val (by decide) (by omega) (by omega)]
        refine ⟨3#usize, by rw [hi1'], by simp [he]⟩
      · by_cases h3 : -2147483648 ≤ v.val
        · obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip (v.bv.setWidth 32)
          have he : enc_sint v = [0xd2#u8, b0, b1, b2, b3] := by
            simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega,
              show ¬ -128 ≤ v.val by omega, show ¬ -32768 ≤ v.val by omega, h3, hbb]
          have hin : input.val = 0xd2#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
          unfold mp_core.mp_core.read_sint
          step as ⟨b, hb⟩
          have hb' : b = false := by
            have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
            rw [hlen] at hb
            cases b <;> simp_all
          simp only [hb', Bool.false_eq_true, ↓reduceIte]
          step as ⟨marker, hm⟩
          have hmk : marker = 0xd2#u8 := by simp [hm, hin]
          have hge : ¬ (marker ≥ 224#u8) := by rw [hmk]; scalar_tac
          simp only [hge, ↓reduceIte]
          have hne8 : ¬ (marker = mp_core.mp_core.MARKER_I8) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_I8]; scalar_tac
          simp only [hne8, ↓reduceIte]
          have hne16 : ¬ (marker = mp_core.mp_core.MARKER_I16) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_I16]; scalar_tac
          simp only [hne16, ↓reduceIte]
          have hmk' : marker = mp_core.mp_core.MARKER_I32 := by
            simp only [hmk, mp_core.mp_core.MARKER_I32]
          simp only [hmk', ↓reduceIte]
          have hlen5 : 5 ≤ input.length := by
            rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
          step as ⟨r, w, hr, hw⟩
          rw [hr]
          simp only [core.result.Result.Insts.CoreOpsTry.branch]
          step as ⟨i, hi⟩
          step as ⟨i1, hi1⟩
          have hb0 : input.val[1]! = b0 := by simp [hin]
          have hb1 : input.val[2]! = b1 := by simp [hin]
          have hb2 : input.val[3]! = b2 := by simp [hin]
          have hb3 : input.val[4]! = b3 := by simp [hin]
          rw [hb0, hb1, hb2, hb3] at hw
          have hwv : (w.val : ℤ) = v.val + 4294967296 := by
            have hv32 := i64_setWidth32_int v (by omega) hv
            omega
          have hhi : (UScalar.hcast .I32 w).val = v.val := by
            rw [UScalar.hcast_val_eq, IScalarTy.I32_numBits_eq, hwv]
            rw [bmod_pow2_sub_of_high 32 (v.val + 4294967296) (by decide) (by omega) (by omega)]
            omega
          have hi1' : i1 = v := by
            apply IScalar.eq_of_val_eq
            rw [hi1, hi]
            simp only [IScalar.cast_val_eq, IScalarTy.I64_numBits_eq, IScalarTy.I32_numBits_eq]
            rw [hhi]
            show Int.bmod v.val (2 ^ 32) = v.val
            rw [bmod_pow2_self_of_low 32 v.val (by decide) (by omega) (by omega)]
          refine ⟨5#usize, by rw [hi1'], by simp [he]⟩
        · obtain ⟨b0, b1, b2, b3, b4, b5, b6, b7, hbb, hvals⟩ := be64_roundtrip v.bv
          have he : enc_sint v = [0xd3#u8, b0, b1, b2, b3, b4, b5, b6, b7] := by
            simp [enc_sint, show ¬ 0 ≤ v.val by omega, show ¬ -32 ≤ v.val by omega,
              show ¬ -128 ≤ v.val by omega, show ¬ -32768 ≤ v.val by omega,
              show ¬ -2147483648 ≤ v.val by omega, hbb]
          have hin : input.val = 0xd3#u8 :: b0 :: b1 :: b2 :: b3 :: b4 :: b5 :: b6 :: b7 :: rest := by
            simp [h, he]
          unfold mp_core.mp_core.read_sint
          step as ⟨b, hb⟩
          have hb' : b = false := by
            have hlen : input.length = rest.length + 9 := by simp [Slice.length, hin]
            rw [hlen] at hb
            cases b <;> simp_all
          simp only [hb', Bool.false_eq_true, ↓reduceIte]
          step as ⟨marker, hm⟩
          have hmk : marker = 0xd3#u8 := by simp [hm, hin]
          have hge : ¬ (marker ≥ 224#u8) := by rw [hmk]; scalar_tac
          simp only [hge, ↓reduceIte]
          have hne8 : ¬ (marker = mp_core.mp_core.MARKER_I8) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_I8]; scalar_tac
          simp only [hne8, ↓reduceIte]
          have hne16 : ¬ (marker = mp_core.mp_core.MARKER_I16) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_I16]; scalar_tac
          simp only [hne16, ↓reduceIte]
          have hne32 : ¬ (marker = mp_core.mp_core.MARKER_I32) := by
            rw [hmk]; simp only [mp_core.mp_core.MARKER_I32]; scalar_tac
          simp only [hne32, ↓reduceIte]
          have hmk' : marker = mp_core.mp_core.MARKER_I64 := by
            simp only [hmk, mp_core.mp_core.MARKER_I64]
          simp only [hmk', ↓reduceIte]
          have hlen9 : 9 ≤ input.length := by
            rw [show input.length = rest.length + 9 from by simp [Slice.length, hin]]; omega
          step as ⟨r, w, hr, hw⟩
          rw [hr]
          simp only [core.result.Result.Insts.CoreOpsTry.branch]
          step as ⟨i, hi⟩
          have hb0 : input.val[1]! = b0 := by simp [hin]
          have hb1 : input.val[2]! = b1 := by simp [hin]
          have hb2 : input.val[3]! = b2 := by simp [hin]
          have hb3 : input.val[4]! = b3 := by simp [hin]
          have hb4 : input.val[5]! = b4 := by simp [hin]
          have hb5 : input.val[6]! = b5 := by simp [hin]
          have hb6 : input.val[7]! = b6 := by simp [hin]
          have hb7 : input.val[8]! = b7 := by simp [hin]
          rw [hb0, hb1, hb2, hb3, hb4, hb5, hb6, hb7] at hw
          have hsum : w.val = v.bv.toNat := hw.trans hvals
          have hwv : (w.val : ℤ) = v.val + 2 ^ 64 := by
            have hm := i64_bv_toNat_neg v hv
            rw [hsum]
            exact hm
          have hi' : i = v := by
            apply IScalar.eq_of_val_eq
            rw [hi, UScalar.hcast_val_eq, IScalarTy.I64_numBits_eq, hwv]
            have hvb : -2 ^ 63 ≤ v.val := by scalar_tac
            rw [bmod_pow2_sub_of_high 64 (v.val + 2 ^ 64) (by decide) (by omega) (by omega)]
            omega
          refine ⟨9#usize, by rw [hi'], by simp [he]⟩

/-- The string-header encoder appends the canonical minimal-width header. -/
theorem str_header_encode (out : alloc.vec.Vec U8) (len : U32)
    (h : out.val.length + 5 ≤ Usize.max) :
    mp_core.mp_core.write_str_header out len ⦃ out' =>
      out'.val = out.val ++ enc_str_header len ⦄ := by
  unfold mp_core.mp_core.write_str_header
  split
  · have hle : len.val ≤ 31 := by
      simp only [mp_core.mp_core.FIXSTR_MAX_LEN] at *
      scalar_tac
    step as ⟨i, hi⟩
    have hiv : i.val = len.val := by scalar_tac
    have hbase : mp_core.mp_core.FIXSTR_BASE.val = 160 := by simp [mp_core.mp_core.FIXSTR_BASE]
    step as ⟨i1, hi1⟩
    step as ⟨out', ho⟩
    have hi1' : i1 = (UScalar.mk (BitVec.ofNat 8 (0xa0 + len.val)) : U8) := by
      apply UScalar.eq_of_val_eq
      rw [hi1, hbase, hiv]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      rw [hlv]
      have hb : 160 + len.val < 2 ^ 8 := by omega
      rw [Nat.mod_eq_of_lt hb]
    simp [enc_str_header, hle, ho, hi1']
  · step as ⟨m8, hm8⟩
    have hm8' : m8.val = 0xff := by simp [hm8, U8.rMax]
    split
    · step as ⟨out1, ho1⟩
      have hlen1' : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨i, hi⟩
      step as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 31 := by
        simp only [mp_core.mp_core.FIXSTR_MAX_LEN] at *
        scalar_tac
      have hle : len.val ≤ 0xff := by scalar_tac
      have hi' : i = (UScalar.mk (len.bv.setWidth 8) : U8) := by
        apply (UScalar.eq_equiv_bv_eq _ _).mpr
        simp only [hi, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U8_numBits_eq]
      simp [enc_str_header, hlo, hle, ho, ho1, hi', mp_core.mp_core.MARKER_STR8]
    · step as ⟨m16, hm16⟩
      have hm16' : m16.val = 0xffff := by simp [hm16, U16.rMax]
      split
      · step as ⟨out1, ho1⟩
        have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
        step as ⟨i2, hi2⟩
        step as ⟨a, ha⟩
        step as ⟨s2, hs2⟩
        have hlens : s2.val.length = 2 := by simp [hs2, ha, Array.val_to_slice]
        step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
        have hlo : ¬ len.val ≤ 0xff := by scalar_tac
        have hle : len.val ≤ 0xffff := by scalar_tac
        have hi2' : i2.bv = len.bv.setWidth 16 := by
          simp only [hi2, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U16_numBits_eq]
        simp [enc_str_header, show ¬ len.val ≤ 31 by omega, hlo, hle, ho, ho1, hi2', ha, hs2,
          be_bytes_of, Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_STR16]
      · step as ⟨out1, ho1⟩
        have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
        step as ⟨a, ha⟩
        step as ⟨s2, hs2⟩
        have hlens : s2.val.length = 4 := by simp [hs2, ha, Array.val_to_slice]
        step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
        have hlo : ¬ len.val ≤ 0xffff := by scalar_tac
        simp [enc_str_header, show ¬ len.val ≤ 31 by omega, show ¬ len.val ≤ 0xff by omega,
          hlo, ho, ho1, ha, hs2, be_bytes_of, Array.val_to_slice, List.append_assoc,
          mp_core.mp_core.MARKER_STR32]

/-- The bin-header encoder appends the canonical minimal-width header. -/
theorem bin_header_encode (out : alloc.vec.Vec U8) (len : U32)
    (h : out.val.length + 5 ≤ Usize.max) :
    mp_core.mp_core.write_bin_header out len ⦃ out' =>
      out'.val = out.val ++ enc_bin_header len ⦄ := by
  unfold mp_core.mp_core.write_bin_header
  step as ⟨m8, hm8⟩
  have hm8' : m8.val = 0xff := by simp [hm8, U8.rMax]
  split
  · step as ⟨out1, ho1⟩
    step as ⟨i, hi⟩
    step as ⟨out', ho⟩
    all_goals try scalar_tac
    have hle : len.val ≤ 0xff := by scalar_tac
    have hi' : i = (UScalar.mk (len.bv.setWidth 8) : U8) := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [hi, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U8_numBits_eq]
    simp [enc_bin_header, hle, ho, ho1, hi', mp_core.mp_core.MARKER_BIN8]
  · step as ⟨m16, hm16⟩
    have hm16' : m16.val = 0xffff := by simp [hm16, U16.rMax]
    split
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨i2, hi2⟩
      step as ⟨a, ha⟩
      step as ⟨s2, hs2⟩
      have hlens : s2.val.length = 2 := by simp [hs2, ha, Array.val_to_slice]
      step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 0xff := by scalar_tac
      have hle : len.val ≤ 0xffff := by scalar_tac
      have hi2' : i2.bv = len.bv.setWidth 16 := by
        simp only [hi2, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U16_numBits_eq]
      simp [enc_bin_header, hlo, hle, ho, ho1, hi2', ha, hs2, be_bytes_of, Array.val_to_slice,
        List.append_assoc, mp_core.mp_core.MARKER_BIN16]
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨a, ha⟩
      step as ⟨s2, hs2⟩
      have hlens : s2.val.length = 4 := by simp [hs2, ha, Array.val_to_slice]
      step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 0xffff := by scalar_tac
      simp [enc_bin_header, show ¬ len.val ≤ 0xff by omega, hlo, ho, ho1, ha, hs2, be_bytes_of,
        Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_BIN32]

/-- The array-header encoder appends the canonical minimal-width header. -/
theorem array_header_encode (out : alloc.vec.Vec U8) (len : U32)
    (h : out.val.length + 5 ≤ Usize.max) :
    mp_core.mp_core.write_array_header out len ⦃ out' =>
      out'.val = out.val ++ enc_array_header len ⦄ := by
  unfold mp_core.mp_core.write_array_header
  split
  · have hle : len.val ≤ 15 := by
      simp only [mp_core.mp_core.FIXCONTAINER_MAX_LEN] at *
      scalar_tac
    step as ⟨i, hi⟩
    have hiv : i.val = len.val := by scalar_tac
    have hbase : mp_core.mp_core.FIXARRAY_BASE.val = 144 := by simp [mp_core.mp_core.FIXARRAY_BASE]
    step as ⟨i1, hi1⟩
    step as ⟨out', ho⟩
    have hi1' : i1 = (UScalar.mk (BitVec.ofNat 8 (0x90 + len.val)) : U8) := by
      apply UScalar.eq_of_val_eq
      rw [hi1, hbase, hiv]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      rw [hlv]
      have hb : 144 + len.val < 2 ^ 8 := by omega
      rw [Nat.mod_eq_of_lt hb]
    simp [enc_array_header, hle, ho, hi1']
  · step as ⟨m16, hm16⟩
    have hm16' : m16.val = 0xffff := by simp [hm16, U16.rMax]
    split
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨i2, hi2⟩
      step as ⟨a, ha⟩
      step as ⟨s2, hs2⟩
      have hlens : s2.val.length = 2 := by simp [hs2, ha, Array.val_to_slice]
      step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 15 := by
        simp only [mp_core.mp_core.FIXCONTAINER_MAX_LEN] at *
        scalar_tac
      have hle : len.val ≤ 0xffff := by scalar_tac
      have hi2' : i2.bv = len.bv.setWidth 16 := by
        simp only [hi2, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U16_numBits_eq]
      simp [enc_array_header, hlo, hle, ho, ho1, hi2', ha, hs2, be_bytes_of, Array.val_to_slice,
        List.append_assoc, mp_core.mp_core.MARKER_ARRAY16]
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨a, ha⟩
      step as ⟨s2, hs2⟩
      have hlens : s2.val.length = 4 := by simp [hs2, ha, Array.val_to_slice]
      step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 0xffff := by scalar_tac
      simp [enc_array_header, show ¬ len.val ≤ 15 by omega, hlo, ho, ho1, ha, hs2, be_bytes_of,
        Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_ARRAY32]

/-- The map-header encoder appends the canonical minimal-width header. -/
theorem map_header_encode (out : alloc.vec.Vec U8) (len : U32)
    (h : out.val.length + 5 ≤ Usize.max) :
    mp_core.mp_core.write_map_header out len ⦃ out' =>
      out'.val = out.val ++ enc_map_header len ⦄ := by
  unfold mp_core.mp_core.write_map_header
  split
  · have hle : len.val ≤ 15 := by
      simp only [mp_core.mp_core.FIXCONTAINER_MAX_LEN] at *
      scalar_tac
    step as ⟨i, hi⟩
    have hiv : i.val = len.val := by scalar_tac
    have hbase : mp_core.mp_core.FIXMAP_BASE.val = 128 := by simp [mp_core.mp_core.FIXMAP_BASE]
    step as ⟨i1, hi1⟩
    step as ⟨out', ho⟩
    have hi1' : i1 = (UScalar.mk (BitVec.ofNat 8 (0x80 + len.val)) : U8) := by
      apply UScalar.eq_of_val_eq
      rw [hi1, hbase, hiv]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      rw [hlv]
      have hb : 128 + len.val < 2 ^ 8 := by omega
      rw [Nat.mod_eq_of_lt hb]
    simp [enc_map_header, hle, ho, hi1']
  · step as ⟨m16, hm16⟩
    have hm16' : m16.val = 0xffff := by simp [hm16, U16.rMax]
    split
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨i2, hi2⟩
      step as ⟨a, ha⟩
      step as ⟨s2, hs2⟩
      have hlens : s2.val.length = 2 := by simp [hs2, ha, Array.val_to_slice]
      step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 15 := by
        simp only [mp_core.mp_core.FIXCONTAINER_MAX_LEN] at *
        scalar_tac
      have hle : len.val ≤ 0xffff := by scalar_tac
      have hi2' : i2.bv = len.bv.setWidth 16 := by
        simp only [hi2, UScalar.cast, BitVec.truncate_eq_setWidth, UScalarTy.U16_numBits_eq]
      simp [enc_map_header, hlo, hle, ho, ho1, hi2', ha, hs2, be_bytes_of, Array.val_to_slice,
        List.append_assoc, mp_core.mp_core.MARKER_MAP16]
    · step as ⟨out1, ho1⟩
      have hlen1 : out1.val.length = out.val.length + 1 := by simp [ho1]
      step as ⟨a, ha⟩
      step as ⟨s2, hs2⟩
      have hlens : s2.val.length = 4 := by simp [hs2, ha, Array.val_to_slice]
      step with extend_values (c := core.clone.CloneU8) (hc := by intro x; rfl) as ⟨out', ho⟩
      have hlo : ¬ len.val ≤ 0xffff := by scalar_tac
      simp [enc_map_header, show ¬ len.val ≤ 15 by omega, hlo, ho, ho1, ha, hs2, be_bytes_of,
        Array.val_to_slice, List.append_assoc, mp_core.mp_core.MARKER_MAP32]

/-- A truncated u16 payload is rejected. -/
theorem uint_short (b0 : U8) (input : Aeneas.Std.Slice U8) (h : input.val = [0xcd#u8, b0]) :
    mp_core.mp_core.read_uint input ⦃ r => r = .Err .UnexpectedEnd ⦄ := by
  unfold mp_core.mp_core.read_uint
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = 2 := by simp [Slice.length, h]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xcd#u8 := by simp [hm, h]
  have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
  simp only [hle, ↓reduceIte]
  have hne8 : ¬ (marker = mp_core.mp_core.MARKER_U8) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U8]; scalar_tac
  simp only [hne8, ↓reduceIte]
  have hmk' : marker = mp_core.mp_core.MARKER_U16 := by
    simp only [hmk, mp_core.mp_core.MARKER_U16]
  simp only [hmk', ↓reduceIte]
  have hlen2 : input.length < 3 := by simp [Slice.length, h]
  step with read_be2_short (h := hlen2) as ⟨r, hr⟩
  rw [hr]
  simp [core.result.Result.Insts.CoreOpsTry.branch,
    core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual]

/-- A signed-marker input is rejected by the unsigned decoder. -/
theorem uint_foreign_marker (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = 0xd0#u8 :: rest) :
    mp_core.mp_core.read_uint input ⦃ r => r = .Err (.UnsupportedMarker 0xd0#u8) ⦄ := by
  unfold mp_core.mp_core.read_uint
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = rest.length + 1 := by simp [Slice.length, h]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xd0#u8 := by simp [hm, h]
  have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
  simp only [hle, ↓reduceIte]
  have hne8 : ¬ (marker = mp_core.mp_core.MARKER_U8) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U8]; scalar_tac
  simp only [hne8, ↓reduceIte]
  have hne16 : ¬ (marker = mp_core.mp_core.MARKER_U16) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U16]; scalar_tac
  simp only [hne16, ↓reduceIte]
  have hne32 : ¬ (marker = mp_core.mp_core.MARKER_U32) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U32]; scalar_tac
  simp only [hne32, ↓reduceIte]
  have hne64 : ¬ (marker = mp_core.mp_core.MARKER_U64) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U64]; scalar_tac
  simp only [hne64, ↓reduceIte]
  rw [WP.spec_ok]
  rw [hmk]

/-- A truncated i8 payload is rejected. -/
theorem sint_short (input : Aeneas.Std.Slice U8) (h : input.val = [0xd0#u8]) :
    mp_core.mp_core.read_sint input ⦃ r => r = .Err .UnexpectedEnd ⦄ := by
  unfold mp_core.mp_core.read_sint
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = 1 := by simp [Slice.length, h]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xd0#u8 := by simp [hm, h]
  have hge : ¬ (marker ≥ 224#u8) := by rw [hmk]; scalar_tac
  simp only [hge, ↓reduceIte]
  have hmk' : marker = mp_core.mp_core.MARKER_I8 := by
    simp only [hmk, mp_core.mp_core.MARKER_I8]
  simp only [hmk', ↓reduceIte]
  have hlen1 : input.length < 2 := by simp [Slice.length, h]
  step with read_be1_short (h := hlen1) as ⟨r, hr⟩
  rw [hr]
  simp [core.result.Result.Insts.CoreOpsTry.branch,
    core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual]

/-- An unsigned-marker input is rejected by the signed decoder. -/
theorem sint_foreign_marker (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = 0xcc#u8 :: rest) :
    mp_core.mp_core.read_sint input ⦃ r => r = .Err (.UnsupportedMarker 0xcc#u8) ⦄ := by
  unfold mp_core.mp_core.read_sint
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = rest.length + 1 := by simp [Slice.length, h]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xcc#u8 := by simp [hm, h]
  have hge : ¬ (marker ≥ 224#u8) := by rw [hmk]; scalar_tac
  simp only [hge, ↓reduceIte]
  have hne8 : ¬ (marker = mp_core.mp_core.MARKER_I8) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_I8]; scalar_tac
  simp only [hne8, ↓reduceIte]
  have hne16 : ¬ (marker = mp_core.mp_core.MARKER_I16) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_I16]; scalar_tac
  simp only [hne16, ↓reduceIte]
  have hne32 : ¬ (marker = mp_core.mp_core.MARKER_I32) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_I32]; scalar_tac
  simp only [hne32, ↓reduceIte]
  have hne64 : ¬ (marker = mp_core.mp_core.MARKER_I64) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_I64]; scalar_tac
  simp only [hne64, ↓reduceIte]
  rw [WP.spec_ok]
  rw [hmk]

/-- The reserved marker 0xC1 is rejected. -/
theorem reserved_marker_rejected (input : Aeneas.Std.Slice U8) (h : input.val = [0xc1#u8]) :
    mp_core.mp_core.read_uint input ⦃ r => r = .Err (.UnsupportedMarker 0xc1#u8) ⦄ := by
  unfold mp_core.mp_core.read_uint
  step as ⟨b, hb⟩
  have hb' : b = false := by
    have hlen : input.length = 1 := by simp [Slice.length, h]
    rw [hlen] at hb
    cases b <;> simp_all
  simp only [hb', Bool.false_eq_true, ↓reduceIte]
  step as ⟨marker, hm⟩
  have hmk : marker = 0xc1#u8 := by simp [hm, h]
  have hle : ¬ (marker ≤ 127#u8) := by rw [hmk]; scalar_tac
  simp only [hle, ↓reduceIte]
  have hne8 : ¬ (marker = mp_core.mp_core.MARKER_U8) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U8]; scalar_tac
  simp only [hne8, ↓reduceIte]
  have hne16 : ¬ (marker = mp_core.mp_core.MARKER_U16) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U16]; scalar_tac
  simp only [hne16, ↓reduceIte]
  have hne32 : ¬ (marker = mp_core.mp_core.MARKER_U32) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U32]; scalar_tac
  simp only [hne32, ↓reduceIte]
  have hne64 : ¬ (marker = mp_core.mp_core.MARKER_U64) := by
    rw [hmk]; simp only [mp_core.mp_core.MARKER_U64]; scalar_tac
  simp only [hne64, ↓reduceIte]
  rw [WP.spec_ok]
  rw [hmk]

/-- The string-header decoder reads back the canonical header. -/
theorem str_header_decode (len : U32) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_str_header len ++ rest) :
    mp_core.mp_core.read_str_header input ⦃ r =>
      ∃ n : Usize, r = .Ok (len, n) ∧ n.val = (enc_str_header len).length ⦄ := by
  by_cases h0 : len.val ≤ 31
  · have he : enc_str_header len = [(UScalar.mk (BitVec.ofNat 8 (0xa0 + len.val)) : U8)] := by
      simp [enc_str_header, h0]
    have hin : input.val = (UScalar.mk (BitVec.ofNat 8 (0xa0 + len.val)) : U8) :: rest := by
      simp [h, he]
    unfold mp_core.mp_core.read_str_header
    step as ⟨b, hb⟩
    have hb' : b = false := by
      have hlen : input.length = rest.length + 1 := by simp [Slice.length, hin]
      rw [hlen] at hb
      cases b <;> simp_all
    simp only [hb', Bool.false_eq_true, ↓reduceIte]
    step as ⟨marker, hm⟩
    have hmk : marker = (UScalar.mk (BitVec.ofNat 8 (0xa0 + len.val)) : U8) := by simp [hm, hin]
    have hmv : marker.val = 160 + len.val := by
      rw [hmk]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      show (160 + len.bv.toNat) % 256 = 160 + len.bv.toNat
      exact Nat.mod_eq_of_lt (by omega)
    step as ⟨sh, hsh⟩
    have hsh5 : sh = 5#u8 := by
      apply UScalar.eq_of_val_eq
      scalar_tac
    simp only [hsh5, ↓reduceIte]
    step as ⟨m, hmod⟩
    step as ⟨i2, hi2⟩
    have hi2' : i2 = len := by
      apply UScalar.eq_of_val_eq
      scalar_tac
    refine ⟨1#usize, by rw [hi2'], by simp [he]⟩
  · by_cases h1 : len.val ≤ 0xff
    · have he : enc_str_header len = [0xd9#u8, (UScalar.mk (len.bv.setWidth 8) : U8)] := by
        simp [enc_str_header, h0, h1]
      have hin : input.val = 0xd9#u8 :: (UScalar.mk (len.bv.setWidth 8) : U8) :: rest := by
        simp [h, he]
      unfold mp_core.mp_core.read_str_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 2 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xd9#u8 := by simp [hm, hin]
      step as ⟨sh, hsh⟩
      have hshv : sh.val = 6 := by rw [hsh, hmk]; scalar_tac
      have hband : ¬ (sh = 5#u8) := by scalar_tac
      simp only [hband, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_STR8 := by
        simp only [hmk, mp_core.mp_core.MARKER_STR8]
      simp only [hmk', ↓reduceIte]
      have hlen2 : 2 ≤ input.length := by
        rw [show input.length = rest.length + 2 from by simp [Slice.length, hin]]; omega
      step as ⟨r, hr⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      step as ⟨i, hi⟩
      have hb1 : input.val[1]! = (UScalar.mk (len.bv.setWidth 8) : U8) := by simp [hin]
      have hv8 : (UScalar.mk (len.bv.setWidth 8) : U8).val = len.val := by
        simp only [UScalar.val, BitVec.toNat_setWidth, UScalarTy.U8_numBits_eq]
        have hlv : len.bv.toNat = len.val := rfl
        show len.bv.toNat % 256 = len.bv.toNat
        exact Nat.mod_eq_of_lt (by omega)
      have hi' : i = len := by
        apply UScalar.eq_of_val_eq
        rw [hi, hb1]
        simp only [UScalar.cast_val_eq, UScalarTy.U32_numBits_eq]
        rw [hv8]
        show len.val % 2 ^ 32 = len.val
        exact Nat.mod_eq_of_lt (by scalar_tac)
      refine ⟨2#usize, by rw [hi'], by simp [he]⟩
    · by_cases h2 : len.val ≤ 0xffff
      · obtain ⟨b0, b1, hbb, hvals⟩ := be16_roundtrip (len.bv.setWidth 16)
        have he : enc_str_header len = [0xda#u8, b0, b1] := by simp [enc_str_header, h0, h1, h2, hbb]
        have hin : input.val = 0xda#u8 :: b0 :: b1 :: rest := by simp [h, he]
        unfold mp_core.mp_core.read_str_header
        step as ⟨b, hb⟩
        have hb' : b = false := by
          have hlen : input.length = rest.length + 3 := by simp [Slice.length, hin]
          rw [hlen] at hb
          cases b <;> simp_all
        simp only [hb', Bool.false_eq_true, ↓reduceIte]
        step as ⟨marker, hm⟩
        have hmk : marker = 0xda#u8 := by simp [hm, hin]
        step as ⟨sh, hsh⟩
        have hshv : sh.val = 6 := by rw [hsh, hmk]; scalar_tac
        have hband : ¬ (sh = 5#u8) := by scalar_tac
        simp only [hband, ↓reduceIte]
        have hne8 : ¬ (marker = mp_core.mp_core.MARKER_STR8) := by
          rw [hmk]; simp only [mp_core.mp_core.MARKER_STR8]; scalar_tac
        simp only [hne8, ↓reduceIte]
        have hmk' : marker = mp_core.mp_core.MARKER_STR16 := by
          simp only [hmk, mp_core.mp_core.MARKER_STR16]
        simp only [hmk', ↓reduceIte]
        have hlen3 : 3 ≤ input.length := by
          rw [show input.length = rest.length + 3 from by simp [Slice.length, hin]]; omega
        step as ⟨r, w, hr, hw⟩
        rw [hr]
        simp only [core.result.Result.Insts.CoreOpsTry.branch]
        step as ⟨i, hi⟩
        have hb0 : input.val[1]! = b0 := by simp [hin]
        have hb1 : input.val[2]! = b1 := by simp [hin]
        rw [hb0, hb1] at hw
        have hv16 : (len.bv.setWidth 16).toNat = len.val := by
          simp only [BitVec.toNat_setWidth]
          have hlv : len.bv.toNat = len.val := rfl
          exact Nat.mod_eq_of_lt (by omega)
        have hi' : i = len := by
          apply UScalar.eq_of_val_eq
          rw [hi]
          scalar_tac
        refine ⟨3#usize, by rw [hi'], by simp [he]⟩
      · obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip len.bv
        have he : enc_str_header len = [0xdb#u8, b0, b1, b2, b3] := by
          simp [enc_str_header, h0, h1, h2, hbb]
        have hin : input.val = 0xdb#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
        unfold mp_core.mp_core.read_str_header
        step as ⟨b, hb⟩
        have hb' : b = false := by
          have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
          rw [hlen] at hb
          cases b <;> simp_all
        simp only [hb', Bool.false_eq_true, ↓reduceIte]
        step as ⟨marker, hm⟩
        have hmk : marker = 0xdb#u8 := by simp [hm, hin]
        step as ⟨sh, hsh⟩
        have hshv : sh.val = 6 := by rw [hsh, hmk]; scalar_tac
        have hband : ¬ (sh = 5#u8) := by scalar_tac
        simp only [hband, ↓reduceIte]
        have hne8 : ¬ (marker = mp_core.mp_core.MARKER_STR8) := by
          rw [hmk]; simp only [mp_core.mp_core.MARKER_STR8]; scalar_tac
        simp only [hne8, ↓reduceIte]
        have hne16 : ¬ (marker = mp_core.mp_core.MARKER_STR16) := by
          rw [hmk]; simp only [mp_core.mp_core.MARKER_STR16]; scalar_tac
        simp only [hne16, ↓reduceIte]
        have hmk' : marker = mp_core.mp_core.MARKER_STR32 := by
          simp only [hmk, mp_core.mp_core.MARKER_STR32]
        simp only [hmk', ↓reduceIte]
        have hlen5 : 5 ≤ input.length := by
          rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
        step as ⟨r, w, hr, hw⟩
        rw [hr]
        simp only [core.result.Result.Insts.CoreOpsTry.branch]
        have hb0 : input.val[1]! = b0 := by simp [hin]
        have hb1 : input.val[2]! = b1 := by simp [hin]
        have hb2 : input.val[3]! = b2 := by simp [hin]
        have hb3 : input.val[4]! = b3 := by simp [hin]
        rw [hb0, hb1, hb2, hb3] at hw
        have hw' : w = len := by
          apply UScalar.eq_of_val_eq
          rw [hw, hvals]
          rfl
        apply WP.spec_bind (Pₘ := fun cf =>
          cf = core.ops.control_flow.ControlFlow.Continue w)
        · exact (WP.spec_ok _).mpr rfl
        · intro cf hcf
          subst hcf
          dsimp only
          rw [WP.spec_ok]
          refine ⟨5#usize, by rw [hw'], by simp [he]⟩

/-- The bin-header decoder reads back the canonical header. -/
theorem bin_header_decode (len : U32) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_bin_header len ++ rest) :
    mp_core.mp_core.read_bin_header input ⦃ r =>
      ∃ n : Usize, r = .Ok (len, n) ∧ n.val = (enc_bin_header len).length ⦄ := by
  by_cases h0 : len.val ≤ 0xff
  · have he : enc_bin_header len = [0xc4#u8, (UScalar.mk (len.bv.setWidth 8) : U8)] := by
      simp [enc_bin_header, h0]
    have hin : input.val = 0xc4#u8 :: (UScalar.mk (len.bv.setWidth 8) : U8) :: rest := by
      simp [h, he]
    unfold mp_core.mp_core.read_bin_header
    step as ⟨b, hb⟩
    have hb' : b = false := by
      have hlen : input.length = rest.length + 2 := by simp [Slice.length, hin]
      rw [hlen] at hb
      cases b <;> simp_all
    simp only [hb', Bool.false_eq_true, ↓reduceIte]
    step as ⟨marker, hm⟩
    have hmk' : marker = mp_core.mp_core.MARKER_BIN8 := by
      simp only [hm, hin, mp_core.mp_core.MARKER_BIN8]
      simp
    simp only [hmk', ↓reduceIte]
    have hlen2 : 2 ≤ input.length := by
      rw [show input.length = rest.length + 2 from by simp [Slice.length, hin]]; omega
    step as ⟨r, hr⟩
    rw [hr]
    simp only [core.result.Result.Insts.CoreOpsTry.branch]
    step as ⟨i, hi⟩
    have hb1 : input.val[1]! = (UScalar.mk (len.bv.setWidth 8) : U8) := by simp [hin]
    have hv8 : (UScalar.mk (len.bv.setWidth 8) : U8).val = len.val := by
      simp only [UScalar.val, BitVec.toNat_setWidth, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      show len.bv.toNat % 256 = len.bv.toNat
      exact Nat.mod_eq_of_lt (by omega)
    have hi' : i = len := by
      apply UScalar.eq_of_val_eq
      rw [hi, hb1]
      simp only [UScalar.cast_val_eq, UScalarTy.U32_numBits_eq]
      rw [hv8]
      show len.val % 2 ^ 32 = len.val
      exact Nat.mod_eq_of_lt (by scalar_tac)
    refine ⟨2#usize, by rw [hi'], by simp [he]⟩
  · by_cases h1 : len.val ≤ 0xffff
    · obtain ⟨b0, b1, hbb, hvals⟩ := be16_roundtrip (len.bv.setWidth 16)
      have he : enc_bin_header len = [0xc5#u8, b0, b1] := by simp [enc_bin_header, h0, h1, hbb]
      have hin : input.val = 0xc5#u8 :: b0 :: b1 :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_bin_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 3 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xc5#u8 := by simp [hm, hin]
      have hne8 : ¬ (marker = mp_core.mp_core.MARKER_BIN8) := by
        rw [hmk]; simp only [mp_core.mp_core.MARKER_BIN8]; scalar_tac
      simp only [hne8, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_BIN16 := by
        simp only [hmk, mp_core.mp_core.MARKER_BIN16]
      simp only [hmk', ↓reduceIte]
      have hlen3 : 3 ≤ input.length := by
        rw [show input.length = rest.length + 3 from by simp [Slice.length, hin]]; omega
      step as ⟨r, w, hr, hw⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      step as ⟨i, hi⟩
      have hb0 : input.val[1]! = b0 := by simp [hin]
      have hb1 : input.val[2]! = b1 := by simp [hin]
      rw [hb0, hb1] at hw
      have hv16 : (len.bv.setWidth 16).toNat = len.val := by
        simp only [BitVec.toNat_setWidth]
        have hlv : len.bv.toNat = len.val := rfl
        exact Nat.mod_eq_of_lt (by omega)
      have hi' : i = len := by
        apply UScalar.eq_of_val_eq
        rw [hi]
        scalar_tac
      refine ⟨3#usize, by rw [hi'], by simp [he]⟩
    · obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip len.bv
      have he : enc_bin_header len = [0xc6#u8, b0, b1, b2, b3] := by
        simp [enc_bin_header, h0, h1, hbb]
      have hin : input.val = 0xc6#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_bin_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xc6#u8 := by simp [hm, hin]
      have hne8 : ¬ (marker = mp_core.mp_core.MARKER_BIN8) := by
        rw [hmk]; simp only [mp_core.mp_core.MARKER_BIN8]; scalar_tac
      simp only [hne8, ↓reduceIte]
      have hne16 : ¬ (marker = mp_core.mp_core.MARKER_BIN16) := by
        rw [hmk]; simp only [mp_core.mp_core.MARKER_BIN16]; scalar_tac
      simp only [hne16, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_BIN32 := by
        simp only [hmk, mp_core.mp_core.MARKER_BIN32]
      simp only [hmk', ↓reduceIte]
      have hlen5 : 5 ≤ input.length := by
        rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
      step as ⟨r, w, hr, hw⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      have hb0 : input.val[1]! = b0 := by simp [hin]
      have hb1 : input.val[2]! = b1 := by simp [hin]
      have hb2 : input.val[3]! = b2 := by simp [hin]
      have hb3 : input.val[4]! = b3 := by simp [hin]
      rw [hb0, hb1, hb2, hb3] at hw
      have hw' : w = len := by
        apply UScalar.eq_of_val_eq
        rw [hw, hvals]
        rfl
      apply WP.spec_bind (Pₘ := fun cf =>
        cf = core.ops.control_flow.ControlFlow.Continue w)
      · exact (WP.spec_ok _).mpr rfl
      · intro cf hcf
        subst hcf
        dsimp only
        rw [WP.spec_ok]
        refine ⟨5#usize, by rw [hw'], by simp [he]⟩

/-- The array-header decoder reads back the canonical header. -/
theorem array_header_decode (len : U32) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_array_header len ++ rest) :
    mp_core.mp_core.read_array_header input ⦃ r =>
      ∃ n : Usize, r = .Ok (len, n) ∧ n.val = (enc_array_header len).length ⦄ := by
  by_cases h0 : len.val ≤ 15
  · have he : enc_array_header len = [(UScalar.mk (BitVec.ofNat 8 (0x90 + len.val)) : U8)] := by
      simp [enc_array_header, h0]
    have hin : input.val = (UScalar.mk (BitVec.ofNat 8 (0x90 + len.val)) : U8) :: rest := by
      simp [h, he]
    unfold mp_core.mp_core.read_array_header
    step as ⟨b, hb⟩
    have hb' : b = false := by
      have hlen : input.length = rest.length + 1 := by simp [Slice.length, hin]
      rw [hlen] at hb
      cases b <;> simp_all
    simp only [hb', Bool.false_eq_true, ↓reduceIte]
    step as ⟨marker, hm⟩
    have hmk : marker = (UScalar.mk (BitVec.ofNat 8 (0x90 + len.val)) : U8) := by simp [hm, hin]
    have hmv : marker.val = 144 + len.val := by
      rw [hmk]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      show (144 + len.bv.toNat) % 256 = 144 + len.bv.toNat
      exact Nat.mod_eq_of_lt (by omega)
    step as ⟨sh, hsh⟩
    have hshv : sh = 9#u8 := by
      apply UScalar.eq_of_val_eq
      scalar_tac
    simp only [hshv, ↓reduceIte]
    step as ⟨m, hmod⟩
    step as ⟨i2, hi2⟩
    have hi2' : i2 = len := by
      apply UScalar.eq_of_val_eq
      scalar_tac
    refine ⟨1#usize, by rw [hi2'], by simp [he]⟩
  · by_cases h1 : len.val ≤ 0xffff
    · obtain ⟨b0, b1, hbb, hvals⟩ := be16_roundtrip (len.bv.setWidth 16)
      have he : enc_array_header len = [0xdc#u8, b0, b1] := by simp [enc_array_header, h0, h1, hbb]
      have hin : input.val = 0xdc#u8 :: b0 :: b1 :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_array_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 3 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xdc#u8 := by simp [hm, hin]
      step as ⟨sh, hsh⟩
      have hshv : sh.val = 13 := by rw [hsh, hmk]; scalar_tac
      have hband : ¬ (sh = 9#u8) := by scalar_tac
      simp only [hband, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_ARRAY16 := by
        simp only [hmk, mp_core.mp_core.MARKER_ARRAY16]
      simp only [hmk', ↓reduceIte]
      have hlen3 : 3 ≤ input.length := by
        rw [show input.length = rest.length + 3 from by simp [Slice.length, hin]]; omega
      step as ⟨r, w, hr, hw⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      step as ⟨i, hi⟩
      have hb0 : input.val[1]! = b0 := by simp [hin]
      have hb1 : input.val[2]! = b1 := by simp [hin]
      rw [hb0, hb1] at hw
      have hv16 : (len.bv.setWidth 16).toNat = len.val := by
        simp only [BitVec.toNat_setWidth]
        have hlv : len.bv.toNat = len.val := rfl
        exact Nat.mod_eq_of_lt (by omega)
      have hi' : i = len := by
        apply UScalar.eq_of_val_eq
        rw [hi]
        scalar_tac
      refine ⟨3#usize, by rw [hi'], by simp [he]⟩
    · obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip len.bv
      have he : enc_array_header len = [0xdd#u8, b0, b1, b2, b3] := by
        simp [enc_array_header, h0, h1, hbb]
      have hin : input.val = 0xdd#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_array_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xdd#u8 := by simp [hm, hin]
      step as ⟨sh, hsh⟩
      have hshv : sh.val = 13 := by rw [hsh, hmk]; scalar_tac
      have hband : ¬ (sh = 9#u8) := by scalar_tac
      simp only [hband, ↓reduceIte]
      have hne16 : ¬ (marker = mp_core.mp_core.MARKER_ARRAY16) := by
        rw [hmk]; simp only [mp_core.mp_core.MARKER_ARRAY16]; scalar_tac
      simp only [hne16, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_ARRAY32 := by
        simp only [hmk, mp_core.mp_core.MARKER_ARRAY32]
      simp only [hmk', ↓reduceIte]
      have hlen5 : 5 ≤ input.length := by
        rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
      step as ⟨r, w, hr, hw⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      have hb0 : input.val[1]! = b0 := by simp [hin]
      have hb1 : input.val[2]! = b1 := by simp [hin]
      have hb2 : input.val[3]! = b2 := by simp [hin]
      have hb3 : input.val[4]! = b3 := by simp [hin]
      rw [hb0, hb1, hb2, hb3] at hw
      have hw' : w = len := by
        apply UScalar.eq_of_val_eq
        rw [hw, hvals]
        rfl
      apply WP.spec_bind (Pₘ := fun cf =>
        cf = core.ops.control_flow.ControlFlow.Continue w)
      · exact (WP.spec_ok _).mpr rfl
      · intro cf hcf
        subst hcf
        dsimp only
        rw [WP.spec_ok]
        refine ⟨5#usize, by rw [hw'], by simp [he]⟩

/-- The map-header decoder reads back the canonical header. -/
theorem map_header_decode (len : U32) (rest : List U8) (input : Aeneas.Std.Slice U8)
    (h : input.val = enc_map_header len ++ rest) :
    mp_core.mp_core.read_map_header input ⦃ r =>
      ∃ n : Usize, r = .Ok (len, n) ∧ n.val = (enc_map_header len).length ⦄ := by
  by_cases h0 : len.val ≤ 15
  · have he : enc_map_header len = [(UScalar.mk (BitVec.ofNat 8 (0x80 + len.val)) : U8)] := by
      simp [enc_map_header, h0]
    have hin : input.val = (UScalar.mk (BitVec.ofNat 8 (0x80 + len.val)) : U8) :: rest := by
      simp [h, he]
    unfold mp_core.mp_core.read_map_header
    step as ⟨b, hb⟩
    have hb' : b = false := by
      have hlen : input.length = rest.length + 1 := by simp [Slice.length, hin]
      rw [hlen] at hb
      cases b <;> simp_all
    simp only [hb', Bool.false_eq_true, ↓reduceIte]
    step as ⟨marker, hm⟩
    have hmk : marker = (UScalar.mk (BitVec.ofNat 8 (0x80 + len.val)) : U8) := by simp [hm, hin]
    have hmv : marker.val = 128 + len.val := by
      rw [hmk]
      simp only [UScalar.val, BitVec.toNat_ofNat, UScalarTy.U8_numBits_eq]
      have hlv : len.bv.toNat = len.val := rfl
      show (128 + len.bv.toNat) % 256 = 128 + len.bv.toNat
      exact Nat.mod_eq_of_lt (by omega)
    step as ⟨sh, hsh⟩
    have hshv : sh = 8#u8 := by
      apply UScalar.eq_of_val_eq
      scalar_tac
    simp only [hshv, ↓reduceIte]
    step as ⟨m, hmod⟩
    step as ⟨i2, hi2⟩
    have hi2' : i2 = len := by
      apply UScalar.eq_of_val_eq
      scalar_tac
    refine ⟨1#usize, by rw [hi2'], by simp [he]⟩
  · by_cases h1 : len.val ≤ 0xffff
    · obtain ⟨b0, b1, hbb, hvals⟩ := be16_roundtrip (len.bv.setWidth 16)
      have he : enc_map_header len = [0xde#u8, b0, b1] := by simp [enc_map_header, h0, h1, hbb]
      have hin : input.val = 0xde#u8 :: b0 :: b1 :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_map_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 3 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xde#u8 := by simp [hm, hin]
      step as ⟨sh, hsh⟩
      have hshv : sh.val = 13 := by rw [hsh, hmk]; scalar_tac
      have hband : ¬ (sh = 8#u8) := by scalar_tac
      simp only [hband, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_MAP16 := by
        simp only [hmk, mp_core.mp_core.MARKER_MAP16]
      simp only [hmk', ↓reduceIte]
      have hlen3 : 3 ≤ input.length := by
        rw [show input.length = rest.length + 3 from by simp [Slice.length, hin]]; omega
      step as ⟨r, w, hr, hw⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      step as ⟨i, hi⟩
      have hb0 : input.val[1]! = b0 := by simp [hin]
      have hb1 : input.val[2]! = b1 := by simp [hin]
      rw [hb0, hb1] at hw
      have hv16 : (len.bv.setWidth 16).toNat = len.val := by
        simp only [BitVec.toNat_setWidth]
        have hlv : len.bv.toNat = len.val := rfl
        exact Nat.mod_eq_of_lt (by omega)
      have hi' : i = len := by
        apply UScalar.eq_of_val_eq
        rw [hi]
        scalar_tac
      refine ⟨3#usize, by rw [hi'], by simp [he]⟩
    · obtain ⟨b0, b1, b2, b3, hbb, hvals⟩ := be32_roundtrip len.bv
      have he : enc_map_header len = [0xdf#u8, b0, b1, b2, b3] := by
        simp [enc_map_header, h0, h1, hbb]
      have hin : input.val = 0xdf#u8 :: b0 :: b1 :: b2 :: b3 :: rest := by simp [h, he]
      unfold mp_core.mp_core.read_map_header
      step as ⟨b, hb⟩
      have hb' : b = false := by
        have hlen : input.length = rest.length + 5 := by simp [Slice.length, hin]
        rw [hlen] at hb
        cases b <;> simp_all
      simp only [hb', Bool.false_eq_true, ↓reduceIte]
      step as ⟨marker, hm⟩
      have hmk : marker = 0xdf#u8 := by simp [hm, hin]
      step as ⟨sh, hsh⟩
      have hshv : sh.val = 13 := by rw [hsh, hmk]; scalar_tac
      have hband : ¬ (sh = 8#u8) := by scalar_tac
      simp only [hband, ↓reduceIte]
      have hne16 : ¬ (marker = mp_core.mp_core.MARKER_MAP16) := by
        rw [hmk]; simp only [mp_core.mp_core.MARKER_MAP16]; scalar_tac
      simp only [hne16, ↓reduceIte]
      have hmk' : marker = mp_core.mp_core.MARKER_MAP32 := by
        simp only [hmk, mp_core.mp_core.MARKER_MAP32]
      simp only [hmk', ↓reduceIte]
      have hlen5 : 5 ≤ input.length := by
        rw [show input.length = rest.length + 5 from by simp [Slice.length, hin]]; omega
      step as ⟨r, w, hr, hw⟩
      rw [hr]
      simp only [core.result.Result.Insts.CoreOpsTry.branch]
      have hb0 : input.val[1]! = b0 := by simp [hin]
      have hb1 : input.val[2]! = b1 := by simp [hin]
      have hb2 : input.val[3]! = b2 := by simp [hin]
      have hb3 : input.val[4]! = b3 := by simp [hin]
      rw [hb0, hb1, hb2, hb3] at hw
      have hw' : w = len := by
        apply UScalar.eq_of_val_eq
        rw [hw, hvals]
        rfl
      apply WP.spec_bind (Pₘ := fun cf =>
        cf = core.ops.control_flow.ControlFlow.Continue w)
      · exact (WP.spec_ok _).mpr rfl
      · intro cf hcf
        subst hcf
        dsimp only
        rw [WP.spec_ok]
        refine ⟨5#usize, by rw [hw'], by simp [he]⟩

/-- Sanity: the spec is satisfiable — 128 round-trips through its canonical
    two-byte encoding `[0xcc, 0x80]`. -/
theorem sanity_uint_roundtrip :
    mp_core.mp_core.read_uint (Slice.from [0xcc#u8, 0x80#u8] (by scalar_tac))
      ⦃ r => r = .Ok (128#u64, 2#usize) ⦄ := by
  have hinput : (Slice.from [0xcc#u8, 0x80#u8] (by scalar_tac) : Aeneas.Std.Slice U8).val =
      enc_uint 128#u64 ++ ([] : List U8) := by
    simp only [Aeneas.Std.Slice.from_val]
    have hv : (128#u64 : U64).val = 128 := rfl
    simp [enc_uint, hv, low_byte]
    decide
  have h := uint_decode 128#u64 [] (Slice.from [0xcc#u8, 0x80#u8] (by scalar_tac)) hinput
  apply WP.spec_mono h
  intro r hr
  obtain ⟨n, hr', hn⟩ := hr
  have hlen : (enc_uint 128#u64).length = 2 := by decide
  have hn' : n = 2#usize := by
    apply UScalar.eq_of_val_eq
    rw [hn, hlen]
    scalar_tac
  rw [hn'] at hr'
  exact hr'

/-- Sanity: the spec is satisfiable — the string header for length 32
    round-trips through `[0xd9, 0x20]`. -/
theorem sanity_str_header_roundtrip :
    mp_core.mp_core.read_str_header (Slice.from [0xd9#u8, 0x20#u8] (by scalar_tac))
      ⦃ r => r = .Ok (32#u32, 2#usize) ⦄ := by
  have hinput : (Slice.from [0xd9#u8, 0x20#u8] (by scalar_tac) : Aeneas.Std.Slice U8).val =
      enc_str_header 32#u32 ++ ([] : List U8) := by
    simp only [Aeneas.Std.Slice.from_val]
    have hv : (32#u32 : U32).val = 32 := rfl
    simp [enc_str_header, hv]
    decide
  have h := str_header_decode 32#u32 [] (Slice.from [0xd9#u8, 0x20#u8] (by scalar_tac)) hinput
  apply WP.spec_mono h
  intro r hr
  obtain ⟨n, hr', hn⟩ := hr
  have hlen : (enc_str_header 32#u32).length = 2 := by decide
  have hn' : n = 2#usize := by
    apply UScalar.eq_of_val_eq
    rw [hn, hlen]
    scalar_tac
  rw [hn'] at hr'
  exact hr'

end mp_core_proof
