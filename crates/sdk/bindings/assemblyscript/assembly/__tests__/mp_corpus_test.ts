// Shared-corpus tests for the mgen MessagePack runtime (`../mpack`).
// This file is a test entry only; it is not part of the public `index.ts`
// exports. Unlike `mpack_test.ts` (which pins one fixed rmp_serde stream),
// these exports are driven per-vector by `run_mp_corpus_test.mjs` against
// the cross-language primitive corpus
// `crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin`
// (+ the `mp_primitives_v1.json` sidecar), generated from rmp_serde 1.3.1.

import { MpackReader, MpackWriter } from "../mpack";

const COMBO_BIN: u8[] = [0xde, 0xad, 0xbe, 0xef];

// ---- per-kind encoders (compared byte-for-byte with the corpus segment) ----

export function encodeU64(v: u64): Uint8Array {
  const w = new MpackWriter();
  w.writeU64(v);
  return w.toBytes();
}

export function encodeI64(v: i64): Uint8Array {
  const w = new MpackWriter();
  w.writeI64(v);
  return w.toBytes();
}

export function encodeF32(v: f32): Uint8Array {
  const w = new MpackWriter();
  w.writeF32(v);
  return w.toBytes();
}

export function encodeF64(v: f64): Uint8Array {
  const w = new MpackWriter();
  w.writeF64(v);
  return w.toBytes();
}

export function encodeNil(): Uint8Array {
  const w = new MpackWriter();
  w.writeNil();
  return w.toBytes();
}

export function encodeBool(v: bool): Uint8Array {
  const w = new MpackWriter();
  w.writeBool(v);
  return w.toBytes();
}

export function encodeStr(s: string): Uint8Array {
  const w = new MpackWriter();
  w.writeString(s);
  return w.toBytes();
}

export function encodeBin(b: Uint8Array): Uint8Array {
  const w = new MpackWriter();
  w.writeBin(b);
  return w.toBytes();
}

// Array vector payload: u64 elements 0..len-1 (sidecar `array_fill` rule).
export function encodeArray(len: i32): Uint8Array {
  const w = new MpackWriter();
  w.writeArrayHeader(<u32>len);
  for (let i = 0; i < len; i++) w.writeU64(<u64>i);
  return w.toBytes();
}

// The fixed nested vector [u64 42, str "hi", bin 0xDEADBEEF].
export function encodeCombo(): Uint8Array {
  const w = new MpackWriter();
  w.writeArrayHeader(3);
  w.writeU64(42);
  w.writeString("hi");
  const b = new Uint8Array(COMBO_BIN.length);
  for (let i = 0; i < COMBO_BIN.length; i++) b[i] = COMBO_BIN[i];
  w.writeBin(b);
  return w.toBytes();
}

// ---- per-kind decoders (must read the value and consume the segment) ----

export function verifyU64(bytes: Uint8Array, expected: u64): bool {
  const r = new MpackReader(bytes);
  if (r.readU64() != expected) return false;
  return r.isDone();
}

export function verifyI64(bytes: Uint8Array, expected: i64): bool {
  const r = new MpackReader(bytes);
  if (r.readI64() != expected) return false;
  return r.isDone();
}

export function verifyF32(bytes: Uint8Array, expected: f32): bool {
  const r = new MpackReader(bytes);
  if (r.readF32() != expected) return false;
  return r.isDone();
}

export function verifyF64(bytes: Uint8Array, expected: f64): bool {
  const r = new MpackReader(bytes);
  if (r.readF64() != expected) return false;
  return r.isDone();
}

export function verifyNil(bytes: Uint8Array): bool {
  const r = new MpackReader(bytes);
  if (!r.tryNil()) return false;
  return r.isDone();
}

export function verifyBool(bytes: Uint8Array, expected: bool): bool {
  const r = new MpackReader(bytes);
  if (r.readBool() != expected) return false;
  return r.isDone();
}

export function verifyStr(bytes: Uint8Array, expected: string): bool {
  const r = new MpackReader(bytes);
  if (r.readString() != expected) return false;
  return r.isDone();
}

export function verifyBin(bytes: Uint8Array, expected: Uint8Array): bool {
  const r = new MpackReader(bytes);
  const b = r.readBin();
  if (b.length != expected.length) return false;
  for (let i = 0; i < b.length; i++) {
    if (b[i] != expected[i]) return false;
  }
  return r.isDone();
}

export function verifyArray(bytes: Uint8Array, len: i32): bool {
  const r = new MpackReader(bytes);
  if (r.readArrayHeader() != <u32>len) return false;
  for (let i = 0; i < len; i++) {
    if (r.readU64() != <u64>i) return false;
  }
  return r.isDone();
}

export function verifyCombo(bytes: Uint8Array): bool {
  const r = new MpackReader(bytes);
  if (r.readArrayHeader() != 3) return false;
  if (r.readU64() != 42) return false;
  if (r.readString() != "hi") return false;
  const b = r.readBin();
  if (b.length != COMBO_BIN.length) return false;
  for (let i = 0; i < COMBO_BIN.length; i++) {
    if (b[i] != COMBO_BIN[i]) return false;
  }
  return r.isDone();
}
