import crates.«db-kernel».storage.src.page.record_slot_core.spec
open Aeneas Std Result record_slot_spec
set_option maxRecDepth 4096
set_option maxHeartbeats 4000000
namespace record_slot_proof

/-- No write occurs when the destination cannot hold a slot. -/
theorem write_short (s : Slot) (out : Aeneas.Std.Slice U8) (h : out.length < 32) :
    record_slot_core.record_slot_core.RecordSlot.encode_bytes s out
      ⦃ ok out' => ok = false ∧ out' = out ⦄ := by
  unfold record_slot_core.record_slot_core.RecordSlot.encode_bytes
  simp [record_slot_core.record_slot_core.RECORD_SLOT_SIZE, h]

/-- Short input is rejected before any field is read. -/
theorem read_short (input : Aeneas.Std.Slice U8) (h : input.length < 32) :
    record_slot_core.record_slot_core.RecordSlot.decode_bytes input ⦃ r => r = none ⦄ := by
  unfold record_slot_core.record_slot_core.RecordSlot.decode_bytes
  simp [record_slot_core.record_slot_core.RECORD_SLOT_SIZE, h]

/-- Extending a written prefix replaces only the next field's bytes. -/
private theorem append_field (original written field : List U8)
    (h : written.length + field.length ≤ original.length) :
    (written ++ original.drop written.length).setSlice! written.length field =
      (written ++ field) ++ original.drop (written.length + field.length) := by
  have hm : min field.length (original.length - written.length) = field.length := by omega
  simp [List.setSlice!, List.drop_append, List.drop_drop, hm,
    List.append_assoc]

/-- Successful writes preserve the suffix after byte 31. -/
@[step] theorem write_success (s : Slot) (out : Aeneas.Std.Slice U8) (h : 32 ≤ out.length) :
    record_slot_core.record_slot_core.RecordSlot.encode_bytes s out
      ⦃ ok out' => ok = true ∧ out'.val = encode s ++ out.val.drop 32 ⦄ := by
  unfold record_slot_core.record_slot_core.RecordSlot.encode_bytes
  simp only [record_slot_core.record_slot_core.RECORD_SLOT_SIZE]
  split
  · scalar_tac
  ·
    step as ⟨part0, back0, hp0, hplen0, hb0⟩
    step as ⟨word0, hw0⟩
    step as ⟨src0, hs0⟩
    have hsrc0 : src0.length = 4 := by simp only [hs0, Array.length_to_slice]; rfl
    step as ⟨copied0, hc0⟩
    have hout0 : (back0 copied0).length = out.length := by
      simp only [hb0, Aeneas.Std.Slice.setSlice!_length]
    have hv0 : (back0 copied0).val = (bytes s.offset) ++ out.val.drop 4 := by
      simp only [hb0, hc0, hs0, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw0]
      have hprefix : ([] : List U8).length = 0 := by simp
      have hfield : (bytes s.offset).length = 4 := by simp [bytes]
      have hh := append_field out.val ([]) (bytes s.offset) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    step as ⟨part1, back1, hp1, hplen1, hb1⟩
    step as ⟨word1, hw1⟩
    step as ⟨src1, hs1⟩
    have hsrc1 : src1.length = 4 := by simp only [hs1, Array.length_to_slice]; rfl
    step as ⟨copied1, hc1⟩
    have hout1 : (back1 copied1).length = out.length := by
      simp only [hb1, Aeneas.Std.Slice.setSlice!_length, hout0]
    have hv1 : (back1 copied1).val = (bytes s.offset ++ bytes s.size) ++ out.val.drop 8 := by
      simp only [hb1, hc1, hs1, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw1, hv0]
      have hprefix : (bytes s.offset : List U8).length = 4 := by simp [bytes]
      have hfield : (bytes s.size).length = 4 := by simp [bytes]
      have hh := append_field out.val (bytes s.offset) (bytes s.size) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    step as ⟨part2, back2, hp2, hplen2, hb2⟩
    step as ⟨word2, hw2⟩
    step as ⟨src2, hs2⟩
    have hsrc2 : src2.length = 8 := by simp only [hs2, Array.length_to_slice]; rfl
    step as ⟨copied2, hc2⟩
    have hout2 : (back2 copied2).length = out.length := by
      simp only [hb2, Aeneas.Std.Slice.setSlice!_length, hout1]
    have hv2 : (back2 copied2).val = (bytes s.offset ++ bytes s.size ++ bytes s.timestamp) ++ out.val.drop 16 := by
      simp only [hb2, hc2, hs2, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw2, hv1]
      have hprefix : (bytes s.offset ++ bytes s.size : List U8).length = 8 := by simp [bytes]
      have hfield : (bytes s.timestamp).length = 8 := by simp [bytes]
      have hh := append_field out.val (bytes s.offset ++ bytes s.size) (bytes s.timestamp) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    step as ⟨part3, back3, hp3, hplen3, hb3⟩
    step as ⟨word3, hw3⟩
    step as ⟨src3, hs3⟩
    have hsrc3 : src3.length = 8 := by simp only [hs3, Array.length_to_slice]; rfl
    step as ⟨copied3, hc3⟩
    have hout3 : (back3 copied3).length = out.length := by
      simp only [hb3, Aeneas.Std.Slice.setSlice!_length, hout2]
    have hv3 : (back3 copied3).val = (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id) ++ out.val.drop 24 := by
      simp only [hb3, hc3, hs3, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw3, hv2]
      have hprefix : (bytes s.offset ++ bytes s.size ++ bytes s.timestamp : List U8).length = 16 := by simp [bytes]
      have hfield : (bytes s.tuple_id).length = 8 := by simp [bytes]
      have hh := append_field out.val (bytes s.offset ++ bytes s.size ++ bytes s.timestamp) (bytes s.tuple_id) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    step as ⟨part4, back4, hp4, hplen4, hb4⟩
    step as ⟨word4, hw4⟩
    step as ⟨src4, hs4⟩
    have hsrc4 : src4.length = 2 := by simp only [hs4, Array.length_to_slice]; rfl
    step as ⟨copied4, hc4⟩
    have hout4 : (back4 copied4).length = out.length := by
      simp only [hb4, Aeneas.Std.Slice.setSlice!_length, hout3]
    have hv4 : (back4 copied4).val = (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum) ++ out.val.drop 26 := by
      simp only [hb4, hc4, hs4, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw4, hv3]
      have hprefix : (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id : List U8).length = 24 := by simp [bytes]
      have hfield : (bytes s.checksum).length = 2 := by simp [bytes]
      have hh := append_field out.val (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id) (bytes s.checksum) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    step as ⟨part5, back5, hp5, hplen5, hb5⟩
    step as ⟨word5, hw5⟩
    step as ⟨src5, hs5⟩
    have hsrc5 : src5.length = 2 := by simp only [hs5, Array.length_to_slice]; rfl
    step as ⟨copied5, hc5⟩
    have hout5 : (back5 copied5).length = out.length := by
      simp only [hb5, Aeneas.Std.Slice.setSlice!_length, hout4]
    have hv5 : (back5 copied5).val = (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum ++ bytes s.flag) ++ out.val.drop 28 := by
      simp only [hb5, hc5, hs5, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw5, hv4]
      have hprefix : (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum : List U8).length = 26 := by simp [bytes]
      have hfield : (bytes s.flag).length = 2 := by simp [bytes]
      have hh := append_field out.val (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum) (bytes s.flag) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    step as ⟨part6, back6, hp6, hplen6, hb6⟩
    step as ⟨word6, hw6⟩
    step as ⟨src6, hs6⟩
    have hsrc6 : src6.length = 4 := by simp only [hs6, Array.length_to_slice]; rfl
    step as ⟨copied6, hc6⟩
    have hout6 : (back6 copied6).length = out.length := by
      simp only [hb6, Aeneas.Std.Slice.setSlice!_length, hout5]
    have hv6 : (back6 copied6).val = (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum ++ bytes s.flag ++ bytes s.reserved) ++ out.val.drop 32 := by
      simp only [hb6, hc6, hs6, Aeneas.Std.Slice.setSlice!, Array.val_to_slice, hw6, hv5]
      have hprefix : (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum ++ bytes s.flag : List U8).length = 28 := by simp [bytes]
      have hfield : (bytes s.reserved).length = 4 := by simp [bytes]
      have hh := append_field out.val (bytes s.offset ++ bytes s.size ++ bytes s.timestamp ++ bytes s.tuple_id ++ bytes s.checksum ++ bytes s.flag) (bytes s.reserved) (by simp only [hprefix, hfield]; exact Nat.le_trans (by decide) h)
      simpa [bytes, Aeneas.Std.Slice.from_val, List.append_assoc] using hh
    simp [hv6, encode, List.append_assoc]

/-- Reading a full slot agrees with the byte-level layout specification. -/
@[step] theorem read_success (input : Aeneas.Std.Slice U8) (h : 32 ≤ input.length) :
    record_slot_core.record_slot_core.RecordSlot.decode_bytes input
      ⦃ r => r = some (decode input.val) ⦄ := by
  unfold record_slot_core.record_slot_core.RecordSlot.decode_bytes
  simp only [record_slot_core.record_slot_core.RECORD_SLOT_SIZE]
  split
  · scalar_tac
  ·
    step as ⟨b0, hb0⟩
    step as ⟨b1, hb1⟩
    step as ⟨b2, hb2⟩
    step as ⟨b3, hb3⟩
    step as ⟨w_offset, hw_offset⟩
    have eq_offset : w_offset = (decode input.val).offset := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U32.bv] at hw_offset
      rw [hw_offset]
      change BitVec.fromLEBytes [b0.bv, b1.bv, b2.bv, b3.bv] =
        BitVec.fromLEBytes [(input.val[0]!).bv, (input.val[1]!).bv, (input.val[2]!).bv, (input.val[3]!).bv]
      have eq0 : b0 = input.val[0]! := by
        simpa only [getElem!_pos input.val 0 (by scalar_tac)] using hb0
      have eq1 : b1 = input.val[1]! := by
        simpa only [getElem!_pos input.val 1 (by scalar_tac)] using hb1
      have eq2 : b2 = input.val[2]! := by
        simpa only [getElem!_pos input.val 2 (by scalar_tac)] using hb2
      have eq3 : b3 = input.val[3]! := by
        simpa only [getElem!_pos input.val 3 (by scalar_tac)] using hb3
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 32)
        (show [b0, b1, b2, b3] = [input.val[0]!, input.val[1]!, input.val[2]!, input.val[3]!] from by simp only [eq0, eq1, eq2, eq3])
    step as ⟨b4, hb4⟩
    step as ⟨b5, hb5⟩
    step as ⟨b6, hb6⟩
    step as ⟨b7, hb7⟩
    step as ⟨w_size, hw_size⟩
    have eq_size : w_size = (decode input.val).size := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U32.bv] at hw_size
      rw [hw_size]
      change BitVec.fromLEBytes [b4.bv, b5.bv, b6.bv, b7.bv] =
        BitVec.fromLEBytes [(input.val[4]!).bv, (input.val[5]!).bv, (input.val[6]!).bv, (input.val[7]!).bv]
      have eq4 : b4 = input.val[4]! := by
        simpa only [getElem!_pos input.val 4 (by scalar_tac)] using hb4
      have eq5 : b5 = input.val[5]! := by
        simpa only [getElem!_pos input.val 5 (by scalar_tac)] using hb5
      have eq6 : b6 = input.val[6]! := by
        simpa only [getElem!_pos input.val 6 (by scalar_tac)] using hb6
      have eq7 : b7 = input.val[7]! := by
        simpa only [getElem!_pos input.val 7 (by scalar_tac)] using hb7
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 32)
        (show [b4, b5, b6, b7] = [input.val[4]!, input.val[5]!, input.val[6]!, input.val[7]!] from by simp only [eq4, eq5, eq6, eq7])
    step as ⟨b8, hb8⟩
    step as ⟨b9, hb9⟩
    step as ⟨b10, hb10⟩
    step as ⟨b11, hb11⟩
    step as ⟨b12, hb12⟩
    step as ⟨b13, hb13⟩
    step as ⟨b14, hb14⟩
    step as ⟨b15, hb15⟩
    step as ⟨w_timestamp, hw_timestamp⟩
    have eq_timestamp : w_timestamp = (decode input.val).timestamp := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U64.bv] at hw_timestamp
      rw [hw_timestamp]
      change BitVec.fromLEBytes [b8.bv, b9.bv, b10.bv, b11.bv, b12.bv, b13.bv, b14.bv, b15.bv] =
        BitVec.fromLEBytes [(input.val[8]!).bv, (input.val[9]!).bv, (input.val[10]!).bv, (input.val[11]!).bv, (input.val[12]!).bv, (input.val[13]!).bv, (input.val[14]!).bv, (input.val[15]!).bv]
      have eq8 : b8 = input.val[8]! := by
        simpa only [getElem!_pos input.val 8 (by scalar_tac)] using hb8
      have eq9 : b9 = input.val[9]! := by
        simpa only [getElem!_pos input.val 9 (by scalar_tac)] using hb9
      have eq10 : b10 = input.val[10]! := by
        simpa only [getElem!_pos input.val 10 (by scalar_tac)] using hb10
      have eq11 : b11 = input.val[11]! := by
        simpa only [getElem!_pos input.val 11 (by scalar_tac)] using hb11
      have eq12 : b12 = input.val[12]! := by
        simpa only [getElem!_pos input.val 12 (by scalar_tac)] using hb12
      have eq13 : b13 = input.val[13]! := by
        simpa only [getElem!_pos input.val 13 (by scalar_tac)] using hb13
      have eq14 : b14 = input.val[14]! := by
        simpa only [getElem!_pos input.val 14 (by scalar_tac)] using hb14
      have eq15 : b15 = input.val[15]! := by
        simpa only [getElem!_pos input.val 15 (by scalar_tac)] using hb15
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 64)
        (show [b8, b9, b10, b11, b12, b13, b14, b15] = [input.val[8]!, input.val[9]!, input.val[10]!, input.val[11]!, input.val[12]!, input.val[13]!, input.val[14]!, input.val[15]!] from by simp only [eq8, eq9, eq10, eq11, eq12, eq13, eq14, eq15])
    step as ⟨b16, hb16⟩
    step as ⟨b17, hb17⟩
    step as ⟨b18, hb18⟩
    step as ⟨b19, hb19⟩
    step as ⟨b20, hb20⟩
    step as ⟨b21, hb21⟩
    step as ⟨b22, hb22⟩
    step as ⟨b23, hb23⟩
    step as ⟨w_tuple_id, hw_tuple_id⟩
    have eq_tuple_id : w_tuple_id = (decode input.val).tuple_id := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U64.bv] at hw_tuple_id
      rw [hw_tuple_id]
      change BitVec.fromLEBytes [b16.bv, b17.bv, b18.bv, b19.bv, b20.bv, b21.bv, b22.bv, b23.bv] =
        BitVec.fromLEBytes [(input.val[16]!).bv, (input.val[17]!).bv, (input.val[18]!).bv, (input.val[19]!).bv, (input.val[20]!).bv, (input.val[21]!).bv, (input.val[22]!).bv, (input.val[23]!).bv]
      have eq16 : b16 = input.val[16]! := by
        simpa only [getElem!_pos input.val 16 (by scalar_tac)] using hb16
      have eq17 : b17 = input.val[17]! := by
        simpa only [getElem!_pos input.val 17 (by scalar_tac)] using hb17
      have eq18 : b18 = input.val[18]! := by
        simpa only [getElem!_pos input.val 18 (by scalar_tac)] using hb18
      have eq19 : b19 = input.val[19]! := by
        simpa only [getElem!_pos input.val 19 (by scalar_tac)] using hb19
      have eq20 : b20 = input.val[20]! := by
        simpa only [getElem!_pos input.val 20 (by scalar_tac)] using hb20
      have eq21 : b21 = input.val[21]! := by
        simpa only [getElem!_pos input.val 21 (by scalar_tac)] using hb21
      have eq22 : b22 = input.val[22]! := by
        simpa only [getElem!_pos input.val 22 (by scalar_tac)] using hb22
      have eq23 : b23 = input.val[23]! := by
        simpa only [getElem!_pos input.val 23 (by scalar_tac)] using hb23
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 64)
        (show [b16, b17, b18, b19, b20, b21, b22, b23] = [input.val[16]!, input.val[17]!, input.val[18]!, input.val[19]!, input.val[20]!, input.val[21]!, input.val[22]!, input.val[23]!] from by simp only [eq16, eq17, eq18, eq19, eq20, eq21, eq22, eq23])
    step as ⟨b24, hb24⟩
    step as ⟨b25, hb25⟩
    step as ⟨w_checksum, hw_checksum⟩
    have eq_checksum : w_checksum = (decode input.val).checksum := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U16.bv] at hw_checksum
      rw [hw_checksum]
      change BitVec.fromLEBytes [b24.bv, b25.bv] =
        BitVec.fromLEBytes [(input.val[24]!).bv, (input.val[25]!).bv]
      have eq24 : b24 = input.val[24]! := by
        simpa only [getElem!_pos input.val 24 (by scalar_tac)] using hb24
      have eq25 : b25 = input.val[25]! := by
        simpa only [getElem!_pos input.val 25 (by scalar_tac)] using hb25
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 16)
        (show [b24, b25] = [input.val[24]!, input.val[25]!] from by simp only [eq24, eq25])
    step as ⟨b26, hb26⟩
    step as ⟨b27, hb27⟩
    step as ⟨w_flag, hw_flag⟩
    have eq_flag : w_flag = (decode input.val).flag := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U16.bv] at hw_flag
      rw [hw_flag]
      change BitVec.fromLEBytes [b26.bv, b27.bv] =
        BitVec.fromLEBytes [(input.val[26]!).bv, (input.val[27]!).bv]
      have eq26 : b26 = input.val[26]! := by
        simpa only [getElem!_pos input.val 26 (by scalar_tac)] using hb26
      have eq27 : b27 = input.val[27]! := by
        simpa only [getElem!_pos input.val 27 (by scalar_tac)] using hb27
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 16)
        (show [b26, b27] = [input.val[26]!, input.val[27]!] from by simp only [eq26, eq27])
    step as ⟨b28, hb28⟩
    step as ⟨b29, hb29⟩
    step as ⟨b30, hb30⟩
    step as ⟨b31, hb31⟩
    step as ⟨w_reserved, hw_reserved⟩
    have eq_reserved : w_reserved = (decode input.val).reserved := by
      apply (UScalar.eq_equiv_bv_eq _ _).mpr
      simp only [decode]
      simp only [U32.bv] at hw_reserved
      rw [hw_reserved]
      change BitVec.fromLEBytes [b28.bv, b29.bv, b30.bv, b31.bv] =
        BitVec.fromLEBytes [(input.val[28]!).bv, (input.val[29]!).bv, (input.val[30]!).bv, (input.val[31]!).bv]
      have eq28 : b28 = input.val[28]! := by
        simpa only [getElem!_pos input.val 28 (by scalar_tac)] using hb28
      have eq29 : b29 = input.val[29]! := by
        simpa only [getElem!_pos input.val 29 (by scalar_tac)] using hb29
      have eq30 : b30 = input.val[30]! := by
        simpa only [getElem!_pos input.val 30 (by scalar_tac)] using hb30
      have eq31 : b31 = input.val[31]! := by
        simpa only [getElem!_pos input.val 31 (by scalar_tac)] using hb31
      exact congrArg (fun xs : List U8 => (BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth 32)
        (show [b28, b29, b30, b31] = [input.val[28]!, input.val[29]!, input.val[30]!, input.val[31]!] from by simp only [eq28, eq29, eq30, eq31])
    simp only [eq_offset, eq_size, eq_timestamp, eq_tuple_id, eq_checksum, eq_flag, eq_reserved]

/-- A fragment update preserves every field except the flag word. -/
theorem fragment_fields (s : Slot) (kind : U16) :
    record_slot_core.record_slot_core.RecordSlot.set_fragment_kind s kind ⦃ out =>
      out.offset = s.offset ∧ out.size = s.size ∧ out.timestamp = s.timestamp ∧
      out.tuple_id = s.tuple_id ∧ out.checksum = s.checksum ∧ out.reserved = s.reserved ∧
      (out.flag.bv &&& 3) = (kind.bv &&& 3) ∧
      (out.flag.bv &&& 65532) = (s.flag.bv &&& 65532) ⦄ := by
  simp [record_slot_core.record_slot_core.RecordSlot.set_fragment_kind,
    record_slot_core.record_slot_core.RECORD_SLOT_FLAG_FRAGMENT_MASK]
  simp [lift, UScalar.bv_or, UScalar.bv_and]
  simp [BitVec.and_or_distrib_right, BitVec.and_assoc]

/-- Fragment reads mask exactly the low two bits, including the reserved value 3. -/
theorem fragment_read (s : Slot) :
    record_slot_core.record_slot_core.RecordSlot.fragment_kind s ⦃ r =>
      r.bv = s.flag.bv &&& 3 ⦄ := by
  simp [record_slot_core.record_slot_core.RecordSlot.fragment_kind,
    record_slot_core.record_slot_core.RECORD_SLOT_FLAG_FRAGMENT_MASK]

/-- The size of the canonical encoding is independent of the field values. -/
@[simp] theorem encode_length (s : Slot) : (encode s).length = 32 := by
  simp [encode, bytes]

private theorem length_eight {xs : List U8} (h : xs.length = 8) :
    ∃ a b c d e f g i, xs = [a,b,c,d,e,f,g,i] := by
  let [a,b,c,d,e,f,g,i] := xs
  exact ⟨a,b,c,d,e,f,g,i,rfl⟩

/-- Integer bytes roundtrip without imposing a restriction on the stored bits. -/
private theorem word_roundtrip {ty : UScalarTy} (v : UScalar ty)
    (h : ty.numBits % 8 = 0) :
    (BitVec.fromLEBytes ((bytes v).map UScalar.bv)).setWidth ty.numBits = v.bv := by
  have hm : (bytes v).map UScalar.bv = v.bv.toLEBytes := by
    simp [bytes, List.map_map]
  rw [hm]
  have hw := congrArg (fun x => x.setWidth ty.numBits) (BitVec.fromLEBytes_toLEBytes h v.bv)
  simpa only [BitVec.setWidth_cast, BitVec.setWidth_eq] using hw

/-- Decoding the canonical layout recovers all fields, including reserved bits. -/
theorem decode_encode (s : Slot) (tail : List U8) : decode (encode s ++ tail) = s := by
  obtain ⟨b0, b1, b2, b3, h_offset⟩ := List.length_eq_four.mp (show (bytes s.offset).length = 4 by simp [bytes])
  obtain ⟨b4, b5, b6, b7, h_size⟩ := List.length_eq_four.mp (show (bytes s.size).length = 4 by simp [bytes])
  obtain ⟨b8, b9, b10, b11, b12, b13, b14, b15, h_timestamp⟩ := length_eight (show (bytes s.timestamp).length = 8 by simp [bytes])
  obtain ⟨b16, b17, b18, b19, b20, b21, b22, b23, h_tuple_id⟩ := length_eight (show (bytes s.tuple_id).length = 8 by simp [bytes])
  obtain ⟨b24, b25, h_checksum⟩ := List.length_eq_two.mp (show (bytes s.checksum).length = 2 by simp [bytes])
  obtain ⟨b26, b27, h_flag⟩ := List.length_eq_two.mp (show (bytes s.flag).length = 2 by simp [bytes])
  obtain ⟨b28, b29, b30, b31, h_reserved⟩ := List.length_eq_four.mp (show (bytes s.reserved).length = 4 by simp [bytes])
  have chunk_offset : [(encode s ++ tail)[0]!, (encode s ++ tail)[1]!, (encode s ++ tail)[2]!, (encode s ++ tail)[3]!] = bytes s.offset := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  have chunk_size : [(encode s ++ tail)[4]!, (encode s ++ tail)[5]!, (encode s ++ tail)[6]!, (encode s ++ tail)[7]!] = bytes s.size := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  have chunk_timestamp : [(encode s ++ tail)[8]!, (encode s ++ tail)[9]!, (encode s ++ tail)[10]!, (encode s ++ tail)[11]!, (encode s ++ tail)[12]!, (encode s ++ tail)[13]!, (encode s ++ tail)[14]!, (encode s ++ tail)[15]!] = bytes s.timestamp := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  have chunk_tuple_id : [(encode s ++ tail)[16]!, (encode s ++ tail)[17]!, (encode s ++ tail)[18]!, (encode s ++ tail)[19]!, (encode s ++ tail)[20]!, (encode s ++ tail)[21]!, (encode s ++ tail)[22]!, (encode s ++ tail)[23]!] = bytes s.tuple_id := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  have chunk_checksum : [(encode s ++ tail)[24]!, (encode s ++ tail)[25]!] = bytes s.checksum := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  have chunk_flag : [(encode s ++ tail)[26]!, (encode s ++ tail)[27]!] = bytes s.flag := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  have chunk_reserved : [(encode s ++ tail)[28]!, (encode s ++ tail)[29]!, (encode s ++ tail)[30]!, (encode s ++ tail)[31]!] = bytes s.reserved := by
    simp [encode, h_offset, h_size, h_timestamp, h_tuple_id, h_checksum, h_flag, h_reserved]
  cases s
  simp only [decode, record_slot_core.record_slot_core.RecordSlot.mk.injEq]
  refine ⟨?_, ?_, ?_, ?_, ?_, ?_, ?_⟩
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[0]!, (encode _ ++ tail)[1]!, (encode _ ++ tail)[2]!, (encode _ ++ tail)[3]!].map UScalar.bv)).setWidth 32 = _
    rw [chunk_offset]
    exact word_roundtrip _ (by decide)
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[4]!, (encode _ ++ tail)[5]!, (encode _ ++ tail)[6]!, (encode _ ++ tail)[7]!].map UScalar.bv)).setWidth 32 = _
    rw [chunk_size]
    exact word_roundtrip _ (by decide)
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[8]!, (encode _ ++ tail)[9]!, (encode _ ++ tail)[10]!, (encode _ ++ tail)[11]!, (encode _ ++ tail)[12]!, (encode _ ++ tail)[13]!, (encode _ ++ tail)[14]!, (encode _ ++ tail)[15]!].map UScalar.bv)).setWidth 64 = _
    rw [chunk_timestamp]
    exact word_roundtrip _ (by decide)
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[16]!, (encode _ ++ tail)[17]!, (encode _ ++ tail)[18]!, (encode _ ++ tail)[19]!, (encode _ ++ tail)[20]!, (encode _ ++ tail)[21]!, (encode _ ++ tail)[22]!, (encode _ ++ tail)[23]!].map UScalar.bv)).setWidth 64 = _
    rw [chunk_tuple_id]
    exact word_roundtrip _ (by decide)
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[24]!, (encode _ ++ tail)[25]!].map UScalar.bv)).setWidth 16 = _
    rw [chunk_checksum]
    exact word_roundtrip _ (by decide)
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[26]!, (encode _ ++ tail)[27]!].map UScalar.bv)).setWidth 16 = _
    rw [chunk_flag]
    exact word_roundtrip _ (by decide)
  · apply (UScalar.eq_equiv_bv_eq _ _).mpr
    change (BitVec.fromLEBytes ([(encode _ ++ tail)[28]!, (encode _ ++ tail)[29]!, (encode _ ++ tail)[30]!, (encode _ ++ tail)[31]!].map UScalar.bv)).setWidth 32 = _
    rw [chunk_reserved]
    exact word_roundtrip _ (by decide)

/-- Byte extraction is also a left inverse: no unknown on-disk bit is lost. -/
private theorem bytes_inverse (xs : List (BitVec 8)) :
    (BitVec.fromLEBytes xs).toLEBytes = xs := by
  apply List.ext_getElem
  · simp; omega
  · intro i hi hi'
    apply BitVec.eq_of_getElem_eq
    intro j hj
    rw [BitVec.getElem_eq_testBit_toNat]
    change ((BitVec.fromLEBytes xs).toLEBytes[i]).testBit j = _
    rw [BitVec.toLEBytes_getElem_testBit _ i j hi ⟨hj, by omega⟩]
    rw [BitVec.fromLEBytes_getElem]
    have hdiv : (8 * i + j) / 8 = i := by omega
    have hmod : (8 * i + j) % 8 = j := by omega
    simp [hdiv, hmod, BitVec.getElem_eq_testBit_toNat]

private theorem rebuild_bytes {ty : UScalarTy} (xs : List U8)
    (h : ty.numBits = 8 * xs.length) :
    bytes (UScalar.mk ((BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth ty.numBits)) = xs := by
  have hb : ((BitVec.fromLEBytes (xs.map UScalar.bv)).setWidth ty.numBits).toLEBytes =
      xs.map UScalar.bv := by
    have hn : ty.numBits = 8 * (xs.map UScalar.bv).length := by simpa using h
    rw [hn, BitVec.setWidth_eq]
    exact bytes_inverse _
  change List.map (@UScalar.mk .U8) _ = xs
  rw [hb]
  have hm : (@UScalar.mk .U8) ∘ UScalar.bv = id := by
    funext x; cases x; rfl
  simp [List.map_map, hm]

/-- Every 32-byte disk image survives decoding and re-encoding, even with unknown flags. -/
theorem encode_decode (input : List U8) (h : input.length = 32) :
    encode (decode input) = input := by
  let [b0, b1, b2, b3, b4, b5, b6, b7, b8, b9, b10, b11, b12, b13, b14, b15, b16, b17, b18, b19, b20, b21, b22, b23, b24, b25, b26, b27, b28, b29, b30, b31] := input
  simp only [encode, decode]
  have h0 := rebuild_bytes (ty := .U32) [b0, b1, b2, b3] (by simp)
  simp only [UScalarTy.U32_numBits_eq] at h0
  have h4 := rebuild_bytes (ty := .U32) [b4, b5, b6, b7] (by simp)
  simp only [UScalarTy.U32_numBits_eq] at h4
  have h8 := rebuild_bytes (ty := .U64) [b8, b9, b10, b11, b12, b13, b14, b15] (by simp)
  simp only [UScalarTy.U64_numBits_eq] at h8
  have h16 := rebuild_bytes (ty := .U64) [b16, b17, b18, b19, b20, b21, b22, b23] (by simp)
  simp only [UScalarTy.U64_numBits_eq] at h16
  have h24 := rebuild_bytes (ty := .U16) [b24, b25] (by simp)
  simp only [UScalarTy.U16_numBits_eq] at h24
  have h26 := rebuild_bytes (ty := .U16) [b26, b27] (by simp)
  simp only [UScalarTy.U16_numBits_eq] at h26
  have h28 := rebuild_bytes (ty := .U32) [b28, b29, b30, b31] (by simp)
  simp only [UScalarTy.U32_numBits_eq] at h28
  change bytes (@UScalar.mk .U32 ((BitVec.fromLEBytes ([b0, b1, b2, b3].map UScalar.bv)).setWidth 32)) ++ bytes (@UScalar.mk .U32 ((BitVec.fromLEBytes ([b4, b5, b6, b7].map UScalar.bv)).setWidth 32)) ++ bytes (@UScalar.mk .U64 ((BitVec.fromLEBytes ([b8, b9, b10, b11, b12, b13, b14, b15].map UScalar.bv)).setWidth 64)) ++ bytes (@UScalar.mk .U64 ((BitVec.fromLEBytes ([b16, b17, b18, b19, b20, b21, b22, b23].map UScalar.bv)).setWidth 64)) ++ bytes (@UScalar.mk .U16 ((BitVec.fromLEBytes ([b24, b25].map UScalar.bv)).setWidth 16)) ++ bytes (@UScalar.mk .U16 ((BitVec.fromLEBytes ([b26, b27].map UScalar.bv)).setWidth 16)) ++ bytes (@UScalar.mk .U32 ((BitVec.fromLEBytes ([b28, b29, b30, b31].map UScalar.bv)).setWidth 32)) = _
  rw [h0, h4, h8, h16, h24, h26, h28]
  rfl

/-- The production writer followed by the production reader is an identity. -/
theorem roundtrip (s : Slot) (out : Aeneas.Std.Slice U8) (h : 32 ≤ out.length) :
    (do
      let (written, encoded) ← record_slot_core.record_slot_core.RecordSlot.encode_bytes s out
      let decoded ← record_slot_core.record_slot_core.RecordSlot.decode_bytes encoded
      ok (written, decoded)) ⦃ written decoded => written = true ∧ decoded = some s ⦄ := by
  step with write_success as ⟨written, encoded, hw, he⟩
  have hn : 32 ≤ encoded.length := by simp [Aeneas.Std.Slice.length, he]
  step with read_success
  simp_all [decode_encode]

/-- The production decoder and encoder preserve an arbitrary complete disk slot. -/
theorem disk_roundtrip (input : Aeneas.Std.Slice U8) (h : input.length = 32) :
    (do
      let decoded ← record_slot_core.record_slot_core.RecordSlot.decode_bytes input
      match decoded with
      | none => ok none
      | some slot =>
        let (_, encoded) ← record_slot_core.record_slot_core.RecordSlot.encode_bytes slot input
        ok (some encoded)) ⦃ r => r = some input ⦄ := by
  step with read_success as ⟨decoded, hd⟩
  simp only [hd]
  step with write_success as ⟨written, encoded, hw, he⟩
  have hbytes : encode (decode input.val) = input.val := encode_decode _ h
  have htail : input.val.drop 32 = [] := List.drop_eq_nil_iff.mpr (by simpa only [Aeneas.Std.Slice.length] using Nat.le_of_eq h)
  have heq : encoded = input := by
    apply (Aeneas.Std.Slice.eq_iff _ _).mpr
    simpa only [hbytes, htail, List.append_nil] using he
  simp [heq]
/-- Concrete slot exercised by the sanity checks. -/
def sanity_slot : Slot :=
  { offset := 1#u32, size := 2#u32, timestamp := 3#u64, tuple_id := 4#u64, checksum := 5#u16, flag := 6#u16, reserved := 7#u32 }

/-- Sanity: a concrete slot survives the production encode/decode pair. -/
theorem sanity_roundtrip :
    (do
      let (written, encoded) ← record_slot_core.record_slot_core.RecordSlot.encode_bytes
        sanity_slot (Aeneas.Std.Slice.from (List.replicate 32 0#u8) (by scalar_tac))
      let decoded ← record_slot_core.record_slot_core.RecordSlot.decode_bytes encoded
      ok (written, decoded)) ⦃ written decoded =>
        written = true ∧ decoded = some sanity_slot ⦄ :=
  roundtrip _ _ (by decide)

/-- Sanity: a concrete short buffer is rejected without modification. -/
theorem sanity_write_short :
    record_slot_core.record_slot_core.RecordSlot.encode_bytes sanity_slot
      (Aeneas.Std.Slice.from [0#u8] (by scalar_tac)) ⦃ ok out' =>
      ok = false ∧ out' = Aeneas.Std.Slice.from [0#u8] (by scalar_tac) ⦄ :=
  write_short _ _ (by decide)

end record_slot_proof
