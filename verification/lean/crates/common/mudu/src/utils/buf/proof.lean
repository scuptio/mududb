import crates.common.mudu.src.utils.buf.spec
open Aeneas Std Result
open sized_buf_spec

namespace sized_buf_proof

@[step]
theorem length_correct (n : Usize) :
    buf.buf.sized_buf_len n ⦃ r => length_post n.val r ⦄ := by
  unfold buf.buf.sized_buf_len
  split
  · simp [length_post, max_payload]
    scalar_tac
  · step
    step
    simp [length_post, max_payload]
    scalar_tac

@[step]
theorem length_success (n : Usize) (h : n.val ≤ max_payload) :
    buf.buf.sized_buf_len n ⦃ r => ∃ total, r = .Ok total ∧ total.val = n.val + 4 ⦄ := by
  have hs := length_correct n
  simpa [length_post, show ¬n.val > max_payload by omega] using hs

@[step]
theorem length_overflow (n : Usize) (h : n.val > max_payload) :
    buf.buf.sized_buf_len n ⦃ r => r = .Err .LengthOverflow ⦄ := by
  simpa [length_post, h] using length_correct n

/-- Successful writes encode the payload and preserve the untouched suffix. -/
theorem write_success (dest src : Aeneas.Std.Slice U8)
    (hlen : src.length ≤ max_payload) (hcap : src.length + 4 ≤ dest.length) :
    buf.buf.write_sized_buf dest src ⦃ r out =>
      ∃ total n, r = .Ok total ∧ total.val = src.length + 4 ∧ n.val = src.length ∧
        out.val = header n ++ src.val ++ dest.val.drop (src.length + 4) ⦄ := by
  unfold buf.buf.write_sized_buf
  step with length_success as ⟨ r, total, hr, htotal ⟩
  subst r
  simp only [core.result.Result.Insts.CoreOpsTry.branch]
  step
  split
  · scalar_tac
  · step
    step
    step
    step
    step
    step
    step
    step
    all_goals try scalar_tac
    refine ⟨total, i4, rfl, ?_, ?_, ?_⟩
    · simpa using htotal
    · scalar_tac
    · have hb : (sized_buf_spec.header i4).length = 4 := by simp [sized_buf_spec.header]
      have hm : min src.val.length (dest.val.length - 4) = src.val.length := by
        simp only [Aeneas.Std.Slice.length] at hcap
        omega
      simp_all [sized_buf_spec.header, List.setSlice!, Aeneas.Std.Slice.length]
      simp_all [List.drop_append, List.drop_drop, Nat.add_comm]

/-- An unrepresentable length is rejected before touching output. -/
theorem write_overflow (dest src : Aeneas.Std.Slice U8) (h : src.length > max_payload) :
    buf.buf.write_sized_buf dest src ⦃ r out => r = .Err .LengthOverflow ∧ out = dest ⦄ := by
  unfold buf.buf.write_sized_buf
  step with length_overflow as ⟨r, hr⟩
  subst r
  simp [core.result.Result.Insts.CoreOpsTry.branch,
    core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual]

/-- Insufficient output space reports the total required length without writing. -/
theorem write_short (dest src : Aeneas.Std.Slice U8)
    (hlen : src.length ≤ max_payload) (hcap : dest.length < src.length + 4) :
    buf.buf.write_sized_buf dest src ⦃ r out =>
      ∃ total, r = .Err (.BufferTooSmall total dest.len) ∧
        total.val = src.length + 4 ∧ out = dest ⦄ := by
  unfold buf.buf.write_sized_buf
  step with length_success as ⟨r, total, hr, ht⟩
  subst r
  simp only [core.result.Result.Insts.CoreOpsTry.branch]
  step
  split
  · simp only [WP.spec_ok, Std.WP.uncurry']
    exact ⟨total, rfl, by simpa using ht, trivial⟩
  · scalar_tac

/-- A missing header is distinguished from a truncated payload. -/
theorem read_short_header (input : Aeneas.Std.Slice U8) (h : input.length < 4) :
    buf.buf.read_sized_buf input ⦃ r => r = .Err (.HeaderTooShort input.len) ⦄ := by
  unfold buf.buf.read_sized_buf
  dsimp only
  split
  · simp
  · scalar_tac

/-- A valid header and enough payload bytes yield the exact borrowed payload. -/
theorem read_success (input : Aeneas.Std.Slice U8)
    (hh : 4 ≤ input.length)
    (hlen : (decode_header input.val).val ≤ max_payload)
    (hcap : (decode_header input.val).val + 4 ≤ input.length) :
    buf.buf.read_sized_buf input ⦃ r =>
      ∃ total payload, r = .Ok (total, payload) ∧
        total.val = (decode_header input.val).val + 4 ∧
        payload.val = (input.val.drop 4).take (decode_header input.val).val ⦄ := by
  unfold buf.buf.read_sized_buf
  dsimp only
  split
  · scalar_tac
  · step as ⟨b0, hb0⟩
    step as ⟨b1, hb1⟩
    step as ⟨b2, hb2⟩
    step as ⟨b3, hb3⟩
    step as ⟨word, hword⟩
    have hn : word = decode_header input.val := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode_header]
      simp only [U32.bv] at hword
      rw [hword]
      change BitVec.fromBEBytes [b0.bv, b1.bv, b2.bv, b3.bv] =
        BitVec.fromBEBytes [(input.val[0]!).bv, (input.val[1]!).bv,
          (input.val[2]!).bv, (input.val[3]!).bv]
      congr 5
      · simpa only [getElem!_pos input.val 0 (by scalar_tac)] using hb0
      · simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
      · simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
      · simpa only [getElem!_pos input.val 3 (by scalar_tac)] using congrArg U8.bv hb3
    step
    step with length_success as ⟨r, total, hr, ht⟩
    subst r
    simp only [core.result.Result.Insts.CoreOpsTry.branch]
    step
    split
    · scalar_tac
    · step
      step
      simp_all [List.slice]

/-- An overflowing header is rejected even when its payload is truncated. -/
theorem read_overflow (input : Aeneas.Std.Slice U8)
    (hh : 4 ≤ input.length) (hlen : (decode_header input.val).val > max_payload) :
    buf.buf.read_sized_buf input ⦃ r => r = .Err .LengthOverflow ⦄ := by
  unfold buf.buf.read_sized_buf
  dsimp only
  split
  · scalar_tac
  · step as ⟨b0, hb0⟩
    step as ⟨b1, hb1⟩
    step as ⟨b2, hb2⟩
    step as ⟨b3, hb3⟩
    step as ⟨word, hword⟩
    have hn : word = decode_header input.val := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode_header]
      simp only [U32.bv] at hword
      rw [hword]
      change BitVec.fromBEBytes [b0.bv, b1.bv, b2.bv, b3.bv] =
        BitVec.fromBEBytes [(input.val[0]!).bv, (input.val[1]!).bv,
          (input.val[2]!).bv, (input.val[3]!).bv]
      congr 5
      · simpa only [getElem!_pos input.val 0 (by scalar_tac)] using hb0
      · simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
      · simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
      · simpa only [getElem!_pos input.val 3 (by scalar_tac)] using congrArg U8.bv hb3
    step
    step with length_overflow as ⟨r, hr⟩
    subst r
    simp [core.result.Result.Insts.CoreOpsTry.branch,
      core.result.Result.Insts.CoreOpsTry_traitFromResidualResult.from_residual]

/-- Truncated payloads report the required total, not merely payload length. -/
theorem read_truncated (input : Aeneas.Std.Slice U8)
    (hh : 4 ≤ input.length) (hlen : (decode_header input.val).val ≤ max_payload)
    (hcap : input.length < (decode_header input.val).val + 4) :
    buf.buf.read_sized_buf input ⦃ r =>
      ∃ total, r = .Err (.BufferTooSmall total input.len) ∧
        total.val = (decode_header input.val).val + 4 ⦄ := by
  unfold buf.buf.read_sized_buf
  dsimp only
  split
  · scalar_tac
  · step as ⟨b0, hb0⟩
    step as ⟨b1, hb1⟩
    step as ⟨b2, hb2⟩
    step as ⟨b3, hb3⟩
    step as ⟨word, hword⟩
    have hn : word = decode_header input.val := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode_header]
      simp only [U32.bv] at hword
      rw [hword]
      change BitVec.fromBEBytes [b0.bv, b1.bv, b2.bv, b3.bv] =
        BitVec.fromBEBytes [(input.val[0]!).bv, (input.val[1]!).bv,
          (input.val[2]!).bv, (input.val[3]!).bv]
      congr 5
      · simpa only [getElem!_pos input.val 0 (by scalar_tac)] using hb0
      · simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
      · simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
      · simpa only [getElem!_pos input.val 3 (by scalar_tac)] using congrArg U8.bv hb3
    step
    step with length_success as ⟨r, total, hr, ht⟩
    subst r
    simp only [core.result.Result.Insts.CoreOpsTry.branch]
    step
    split
    · simp_all
    · scalar_tac

/-- The byte-level format specification roundtrips independently of the Rust code. -/
theorem decode_header_append (n : U32) (tail : List U8) :
    decode_header (header n ++ tail) = n := by
  have hlen : (header n).length = 4 := by simp [header]
  obtain ⟨a, b, c, d, heq⟩ := List.length_eq_four.mp hlen
  have hbytes : [(header n ++ tail)[0]!, (header n ++ tail)[1]!,
      (header n ++ tail)[2]!, (header n ++ tail)[3]!] = header n := by
    simp [heq]
  apply (UScalar.eq_equiv_bv_eq _ _).mpr
  change (BitVec.fromBEBytes
    ([(header n ++ tail)[0]!, (header n ++ tail)[1]!,
      (header n ++ tail)[2]!, (header n ++ tail)[3]!].map UScalar.bv)).setWidth 32 = n.bv
  have h := congrArg (fun xs : List U8 =>
    (BitVec.fromBEBytes (xs.map UScalar.bv)).setWidth 32) hbytes
  rw [h]
  have hm : (header n).map UScalar.bv = n.bv.toBEBytes := by
    simp [header, List.map_map]
  have hm' := congrArg (fun xs : List (BitVec 8) => (BitVec.fromBEBytes xs).setWidth 32) hm
  rw [hm']
  unfold BitVec.fromBEBytes
  simp only [BitVec.setWidth_cast]
  change (BitVec.fromLEBytes n.bv.toBEBytes.reverse).setWidth 32 = n.bv
  have hrev : n.bv.toBEBytes.reverse = n.bv.toLEBytes := by simp [BitVec.toBEBytes]
  rw [congrArg (fun xs : List (BitVec 8) => (BitVec.fromLEBytes xs).setWidth 32) hrev]
  have hround := BitVec.fromLEBytes_toLEBytes (by decide : 32 % 8 = 0) n.bv
  have hround' := congrArg (fun x => x.setWidth 32) hround
  simpa only [BitVec.setWidth_cast, BitVec.setWidth_eq] using hround'

/-- Writing then reading returns the original payload and the same total length. -/
theorem roundtrip (dest src : Aeneas.Std.Slice U8)
    (hlen : src.length ≤ max_payload) (hcap : src.length + 4 ≤ dest.length) :
    (do
      let (written, out) ← buf.buf.write_sized_buf dest src
      let read ← buf.buf.read_sized_buf out
      ok (written, read)) ⦃ written read =>
        ∃ total, written = .Ok total ∧ read = .Ok (total, src) ∧
          total.val = src.length + 4 ⦄ := by
  step with write_success as ⟨written, out, total, n, hw, ht, hn, hout⟩
  have hheader : decode_header out.val = n := by
    rw [hout, List.append_assoc]
    exact decode_header_append n _
  have houtlen : out.length = dest.length := by
    simp [Aeneas.Std.Slice.length, hout, header]
    scalar_tac
  step with read_success as ⟨read, consumed, payload, hr, hc, hp⟩
  have heq : consumed = total := by scalar_tac
  have hpayload : payload = src := by
    apply (Aeneas.Std.Slice.eq_iff _ _).mpr
    simp_all [header, Aeneas.Std.Slice.length]
  simp only [hw, hr, heq, hpayload]
  exact ⟨total, rfl, rfl, ht⟩

/-- Sanity: the roundtrip contract succeeds on a concrete payload and buffer. -/
theorem sanity_roundtrip :
    (do
      let (written, out) ← buf.buf.write_sized_buf
        (Aeneas.Std.Slice.from [0#u8, 0#u8, 0#u8, 0#u8, 0#u8] (by scalar_tac))
        (Aeneas.Std.Slice.from [1#u8] (by scalar_tac))
      let read ← buf.buf.read_sized_buf out
      ok (written, read)) ⦃ written read =>
        ∃ total, written = .Ok total ∧
          read = .Ok (total, Aeneas.Std.Slice.from [1#u8] (by scalar_tac)) ∧ total.val = 5 ⦄ :=
  roundtrip _ _ (by decide) (by decide)

/-- Sanity: a concrete too-small destination reports the required total length. -/
theorem sanity_write_short :
    buf.buf.write_sized_buf (Aeneas.Std.Slice.from [0#u8] (by scalar_tac))
      (Aeneas.Std.Slice.from [1#u8] (by scalar_tac)) ⦃ r out =>
      ∃ total, r = .Err (.BufferTooSmall total (Aeneas.Std.Slice.from [0#u8] (by scalar_tac)).len) ∧
        total.val = 5 ∧ out = Aeneas.Std.Slice.from [0#u8] (by scalar_tac) ⦄ :=
  write_short _ _ (by decide) (by decide)

end sized_buf_proof
