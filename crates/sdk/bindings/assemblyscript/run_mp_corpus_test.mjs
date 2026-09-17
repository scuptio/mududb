// Runs the shared MessagePack primitive corpus tests: compiles
// assembly/__tests__/mp_corpus_test.ts with the local asc into a temp dir,
// then for every vector of the cross-language corpus
// crates/db-kernel/testing/fixtures/golden/v1/mp_primitives_v1.bin (+ the
// mp_primitives_v1.json sidecar, generated from rmp_serde 1.3.1) checks that
// mpack.ts encodes the value to exactly the golden segment bytes and decodes
// the segment back to the value.
// Usage: node run_mp_corpus_test.mjs   (from crates/sdk/bindings/assemblyscript)
import { readFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const fixtureDir = join(here, "..", "..", "..", "db-kernel", "testing", "fixtures", "golden", "v1");
const corpus = readFileSync(join(fixtureDir, "mp_primitives_v1.bin"));
const sidecar = JSON.parse(readFileSync(join(fixtureDir, "mp_primitives_v1.json"), "utf8"));

function unpackSegments(bytes) {
  const segments = [];
  let offset = 0;
  while (offset < bytes.length) {
    const length = bytes.readUInt32BE(offset);
    offset += 4;
    segments.push(bytes.subarray(offset, offset + length));
    offset += length;
  }
  return segments;
}

// Sidecar `bin_fill` rule: byte i = i mod 251.
function patternBin(length) {
  const b = new Uint8Array(length);
  for (let i = 0; i < length; i++) b[i] = i % 251;
  return b;
}

const out = join(mkdtempSync(join(tmpdir(), "mp-corpus-test-")), "mp_corpus_test.wasm");
execFileSync("npx", ["asc", "assembly/__tests__/mp_corpus_test.ts", "--outFile", out, "--bindings", "esm", "--debug"], {
  cwd: here,
  stdio: "inherit",
});
const mod = await import(pathToFileURL(out.replace(/\.wasm$/, ".js")).href);

const results = [];
function report(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  " + detail : ""}`);
}

function bytesEqual(actual, segment) {
  const actualBuf = Buffer.from(actual.buffer, actual.byteOffset, actual.byteLength);
  return actualBuf.equals(Buffer.from(segment.buffer, segment.byteOffset, segment.byteLength));
}

const segments = unpackSegments(corpus);
const vectors = sidecar.vectors;
if (segments.length !== vectors.length) {
  console.log(`FAIL  segment count ${segments.length} != sidecar vector count ${vectors.length}`);
  process.exit(1);
}

vectors.forEach((vector, index) => {
  const segment = segments[index];
  const label = `#${index} ${vector.kind}` +
    (vector.value !== undefined ? ` ${JSON.stringify(vector.value)}` : "") +
    (vector.len !== undefined ? ` len ${vector.len}` : "");
  let encoded;
  let decoded;
  switch (vector.kind) {
    case "u64": {
      const v = BigInt(vector.value);
      encoded = mod.encodeU64(v);
      decoded = mod.verifyU64(new Uint8Array(segment), v);
      break;
    }
    case "i64": {
      const v = BigInt(vector.value);
      encoded = mod.encodeI64(v);
      decoded = mod.verifyI64(new Uint8Array(segment), v);
      break;
    }
    case "f32": {
      const v = Math.fround(Number(vector.value));
      encoded = mod.encodeF32(v);
      decoded = mod.verifyF32(new Uint8Array(segment), v);
      break;
    }
    case "f64": {
      const v = Number(vector.value);
      encoded = mod.encodeF64(v);
      decoded = mod.verifyF64(new Uint8Array(segment), v);
      break;
    }
    case "nil":
      encoded = mod.encodeNil();
      decoded = mod.verifyNil(new Uint8Array(segment));
      break;
    case "bool":
      encoded = mod.encodeBool(vector.value);
      decoded = mod.verifyBool(new Uint8Array(segment), vector.value);
      break;
    case "str": {
      const v = vector.value !== undefined ? vector.value : "a".repeat(vector.len);
      encoded = mod.encodeStr(v);
      decoded = mod.verifyStr(new Uint8Array(segment), v);
      break;
    }
    case "bin": {
      const v = patternBin(vector.len);
      encoded = mod.encodeBin(v);
      decoded = mod.verifyBin(new Uint8Array(segment), v);
      break;
    }
    case "array":
      encoded = mod.encodeArray(vector.len);
      decoded = mod.verifyArray(new Uint8Array(segment), vector.len);
      break;
    case "combo":
      encoded = mod.encodeCombo();
      decoded = mod.verifyCombo(new Uint8Array(segment));
      break;
    default:
      report(`${label}: unknown kind`, false);
      return;
  }
  report(`${label}: encode byte-exact`, bytesEqual(encoded, segment));
  report(`${label}: decode value`, decoded === true);
});

const failed = results.filter((r) => !r.ok);
console.log(failed.length === 0 ? `\nALL ${results.length} CHECKS PASSED` : `\n${failed.length} CHECK(S) FAILED`);
process.exit(failed.length === 0 ? 0 : 1);
