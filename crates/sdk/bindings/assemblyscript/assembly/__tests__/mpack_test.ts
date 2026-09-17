// Boundary round-trip tests for the mgen MessagePack runtime (`../mpack`).
// This file is a test entry only; it is not part of the public `index.ts`
// exports. The exact expected bytes are produced by an rmp_serde 1.3.1
// scratch program and compared by `run_mpack_test.mjs`.

import { MpackReader, MpackWriter } from "../mpack";

const BIN_PATTERN_MOD: i32 = 251;

function repeatChar(c: string, n: i32): string {
  let s = "";
  for (let i = 0; i < n; i++) s += c;
  return s;
}

function patternBin(n: i32): Uint8Array {
  const b = new Uint8Array(n);
  for (let i = 0; i < n; i++) b[i] = <u8>(i % BIN_PATTERN_MOD);
  return b;
}

// Encodes the full boundary table into one stream, mirroring the rmp_serde
// scratch program byte for byte.
export function encodeBoundary(): Uint8Array {
  const w = new MpackWriter();
  w.writeNil();
  w.writeBool(false);
  w.writeBool(true);

  const u64s: u64[] = [0, 1, 127, 128, 255, 256, 65535, 65536, 4294967295, 4294967296, u64.MAX_VALUE];
  for (let i = 0; i < u64s.length; i++) w.writeU64(u64s[i]);

  const i64s: i64[] = [2, 127, 128, -1, -32, -33, -128, -129, -32768, -32769, -2147483648, -2147483649, i64.MIN_VALUE];
  for (let i = 0; i < i64s.length; i++) w.writeI64(i64s[i]);

  w.writeF32(1.5);
  w.writeF32(0.1);
  w.writeF64(1.5);
  w.writeF64(-3.141592653589793);

  w.writeString("");
  w.writeString(repeatChar("a", 31));
  w.writeString(repeatChar("b", 32));
  w.writeString(repeatChar("c", 255));
  w.writeString(repeatChar("d", 256));
  w.writeString("héllo世界");

  w.writeBin(patternBin(0));
  w.writeBin(patternBin(255));
  w.writeBin(patternBin(256));

  w.writeArrayHeader(0);
  w.writeArrayHeader(15);
  w.writeArrayHeader(16);
  w.writeArrayHeader(65536);

  // list<u8>-as-array quirk sample: header + one writeU64 per byte.
  w.writeArrayHeader(3);
  w.writeU64(1);
  w.writeU64(2);
  w.writeU64(3);

  return w.toBytes();
}

// Decodes the boundary stream (as produced by rmp_serde) and checks every
// value plus the trailing-byte check.
export function verifyDecode(bytes: Uint8Array): bool {
  const r = new MpackReader(bytes);

  if (!r.tryNil()) return false;
  if (r.readBool()) return false;
  if (!r.readBool()) return false;

  const u64s: u64[] = [0, 1, 127, 128, 255, 256, 65535, 65536, 4294967295, 4294967296, u64.MAX_VALUE];
  for (let i = 0; i < u64s.length; i++) {
    if (r.readU64() != u64s[i]) return false;
  }

  const i64s: i64[] = [2, 127, 128, -1, -32, -33, -128, -129, -32768, -32769, -2147483648, -2147483649, i64.MIN_VALUE];
  for (let i = 0; i < i64s.length; i++) {
    if (r.readI64() != i64s[i]) return false;
  }

  if (r.readF32() != <f32>1.5) return false;
  if (r.readF32() != <f32>0.1) return false;
  if (r.readF64() != 1.5) return false;
  if (r.readF64() != -3.141592653589793) return false;

  if (r.readString() != "") return false;
  if (r.readString() != repeatChar("a", 31)) return false;
  if (r.readString() != repeatChar("b", 32)) return false;
  if (r.readString() != repeatChar("c", 255)) return false;
  if (r.readString() != repeatChar("d", 256)) return false;
  if (r.readString() != "héllo世界") return false;

  const binLens: i32[] = [0, 255, 256];
  for (let i = 0; i < binLens.length; i++) {
    const b = r.readBin();
    if (b.length != binLens[i]) return false;
    for (let j = 0; j < b.length; j++) {
      if (b[j] != <u8>(j % BIN_PATTERN_MOD)) return false;
    }
  }

  if (r.readArrayHeader() != 0) return false;
  if (r.readArrayHeader() != 15) return false;
  if (r.readArrayHeader() != 16) return false;
  if (r.readArrayHeader() != 65536) return false;

  if (r.readArrayHeader() != 3) return false;
  if (r.readU64() != 1) return false;
  if (r.readU64() != 2) return false;
  if (r.readU64() != 3) return false;

  return r.isDone();
}

export function roundtrip(): bool {
  return verifyDecode(encodeBoundary());
}

// tryNil must consume a real nil and must NOT consume a non-nil marker.
export function tryNilProbe(): bool {
  const w = new MpackWriter();
  w.writeNil();
  w.writeU64(1);
  const r = new MpackReader(w.toBytes());
  if (!r.tryNil()) return false;
  if (r.tryNil()) return false; // next byte is 0x01, must not be consumed
  if (r.readU64() != 1) return false;
  return r.isDone();
}

// Cross-width decode leniency, mirroring rmp_serde::from_slice: f32 readable
// from a 0xCB f64 marker and vice versa.
export function floatLeniency(): bool {
  const wf = new MpackWriter();
  wf.writeF64(1.5);
  if (new MpackReader(wf.toBytes()).readF32() != <f32>1.5) return false;
  const wc = new MpackWriter();
  wc.writeF32(1.5);
  if (new MpackReader(wc.toBytes()).readF64() != 1.5) return false;
  return true;
}

// Integer decode leniency: readI64 accepts any in-range int encoding
// (a 0xCC u8 encoding here), mirroring rmp_serde::from_slice::<i64>.
export function intLeniency(): bool {
  const w = new MpackWriter();
  w.writeU64(200); // 0xCC encoding
  return new MpackReader(w.toBytes()).readI64() == 200;
}

// readU64 on a negative encoding must throw (AS has no try/catch; the node
// runner asserts that calling this aborts), matching rmp_serde's
// "expected u64" error for from_slice::<u64>(negfixint -1).
export function readU64FromNegfixint(): u64 {
  const n = new MpackWriter();
  n.writeI64(-1); // negfixint 0xFF
  return new MpackReader(n.toBytes()).readU64();
}
