import Aeneas
open Aeneas Aeneas.Std Result
namespace verification_bit_bytes

/-- Val-level roundtrip of the two-byte big-endian encoding of a 16-bit word:
    the bytes exist and their weighted sum is the word's value. -/
theorem toBEBytes_val16 (b : BitVec 16) :
    ∃ b0 b1 : U8, b.toBEBytes.map (@UScalar.mk .U8) = [b0, b1] ∧
      b0.val * 256 + b1.val = b.toNat := by
  have h16 : (16:ℕ) > 0 := by decide
  have h16' : (16 - 8 : ℕ) > 0 := by decide
  have h0 : ¬ ((16 - 8 - 8 : ℕ) > 0) := by decide
  unfold BitVec.toBEBytes
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h16, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h16', ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h0, ↓reduceIte]
  simp only [List.reverse_cons, List.reverse_nil, List.map_append, List.map_cons, List.map_nil]
  refine ⟨_, _, rfl, ?_⟩
  simp only [UScalar.val, BitVec.toNat_setWidth, BitVec.toNat_ushiftRight, UScalarTy.U8_numBits_eq]
  have hb : b.toNat < 65536 := by omega
  simp only [show (16 - 8 : ℕ) = 8 by decide, show (8 - 8 : ℕ) = 0 by decide] at *
  omega

/-- Val-level roundtrip of the four-byte big-endian encoding of a 32-bit word. -/
theorem toBEBytes_val32 (b : BitVec 32) :
    ∃ b0 b1 b2 b3 : U8, b.toBEBytes.map (@UScalar.mk .U8) = [b0, b1, b2, b3] ∧
      b0.val * 16777216 + b1.val * 65536 + b2.val * 256 + b3.val = b.toNat := by
  have h32 : (32:ℕ) > 0 := by decide
  have h24 : (32 - 8 : ℕ) > 0 := by decide
  have h16 : (32 - 8 - 8 : ℕ) > 0 := by decide
  have h8 : (32 - 8 - 8 - 8 : ℕ) > 0 := by decide
  have h0 : ¬ ((32 - 8 - 8 - 8 - 8 : ℕ) > 0) := by decide
  unfold BitVec.toBEBytes
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h32, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h24, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h16, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h8, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h0, ↓reduceIte]
  simp only [List.reverse_cons, List.reverse_nil, List.map_append, List.map_cons, List.map_nil]
  refine ⟨_, _, _, _, rfl, ?_⟩
  simp only [UScalar.val, BitVec.toNat_setWidth, BitVec.toNat_ushiftRight, UScalarTy.U8_numBits_eq]
  have hb : b.toNat < 4294967296 := by omega
  simp only [show (32 - 8 : ℕ) = 24 by decide, show (24 - 8 : ℕ) = 16 by decide,
    show (16 - 8 : ℕ) = 8 by decide, show (8 - 8 : ℕ) = 0 by decide] at *
  omega

/-- Val-level roundtrip of the eight-byte big-endian encoding of a 64-bit word. -/
theorem toBEBytes_val64 (b : BitVec 64) :
    ∃ b0 b1 b2 b3 b4 b5 b6 b7 : U8, b.toBEBytes.map (@UScalar.mk .U8) = [b0, b1, b2, b3, b4, b5, b6, b7] ∧
      b0.val * 72057594037927936 + b1.val * 281474976710656 + b2.val * 1099511627776 +
        b3.val * 4294967296 + b4.val * 16777216 + b5.val * 65536 + b6.val * 256 + b7.val =
        b.toNat := by
  have h64 : (64:ℕ) > 0 := by decide
  have h56 : (64 - 8 : ℕ) > 0 := by decide
  have h48 : (64 - 8 - 8 : ℕ) > 0 := by decide
  have h40 : (64 - 8 - 8 - 8 : ℕ) > 0 := by decide
  have h32 : (64 - 8 - 8 - 8 - 8 : ℕ) > 0 := by decide
  have h24 : (64 - 8 - 8 - 8 - 8 - 8 : ℕ) > 0 := by decide
  have h16 : (64 - 8 - 8 - 8 - 8 - 8 - 8 : ℕ) > 0 := by decide
  have h8 : (64 - 8 - 8 - 8 - 8 - 8 - 8 - 8 : ℕ) > 0 := by decide
  have h0 : ¬ ((64 - 8 - 8 - 8 - 8 - 8 - 8 - 8 - 8 : ℕ) > 0) := by decide
  unfold BitVec.toBEBytes
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h64, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h56, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h48, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h40, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h32, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h24, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h16, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h8, ↓reduceIte]
  unfold BitVec.toLEBytes
  simp only [gt_iff_lt, h0, ↓reduceIte]
  simp only [List.reverse_cons, List.reverse_nil, List.map_append, List.map_cons, List.map_nil]
  refine ⟨_, _, _, _, _, _, _, _, rfl, ?_⟩
  simp only [UScalar.val, BitVec.toNat_setWidth, BitVec.toNat_ushiftRight, UScalarTy.U8_numBits_eq]
  have hb : b.toNat < 18446744073709551616 := by omega
  simp only [show (64 - 8 : ℕ) = 56 by decide, show (56 - 8 : ℕ) = 48 by decide,
    show (48 - 8 : ℕ) = 40 by decide, show (40 - 8 : ℕ) = 32 by decide,
    show (32 - 8 : ℕ) = 24 by decide, show (24 - 8 : ℕ) = 16 by decide,
    show (16 - 8 : ℕ) = 8 by decide, show (8 - 8 : ℕ) = 0 by decide] at *
  omega

end verification_bit_bytes
