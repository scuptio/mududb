// Runs the MSSP syscall corpus tests for the mgen-generated AssemblyScript
// codec: compiles assembly/__tests__/syscall_corpus_test.ts with the local
// asc into a temp dir, then drives it against the cross-language corpus
// crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin
// (47 frames: one request + one ok response per message kind 1..23 in
// MessageKind discriminant order, plus a trailing `get` UniError response).
// The sidecar syscall_payload_v1_all.json is the single source of truth for
// the semantic expectations: for every frame the runner checks, through the
// generated AS codec:
//   - the header routes to the expected message kind,
//   - requests encode from the documented inputs byte-exactly,
//   - decodeRequestJson/decodeResponseJson/decodeGetErrJson (decoded in wasm,
//     serialized to the sidecar `expect` shape) deep-equal the sidecar
//     `expect` object,
//   - responses encode from the documented values byte-exactly and
//     decode-then-re-encode byte-identically.
// It also spot-checks generated record/variant codecs (UniError err_details
// array quirk, UniResultSet cursor array quirk, a relation delta, an fs
// dirent) against an independent JS MessagePack encoding.
// Usage: node run_syscall_corpus_test.mjs   (from crates/sdk/bindings/assemblyscript)
import { readFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const fixtureDir = join(here, "..", "..", "..", "db-kernel", "testing", "fixtures", "golden", "v1");
const corpus = readFileSync(join(fixtureDir, "syscall_payload_v1_all.bin"));
const sidecar = JSON.parse(readFileSync(join(fixtureDir, "syscall_payload_v1_all.json"), "utf8"));

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

// ---- independent minimal-form MessagePack encoder (mirrors mpack.ts /
// rmp_serde 1.3.x rules) used only for the record/variant spot checks ----

function mpU64(v) {
  v = BigInt(v);
  if (v <= 0x7fn) return Buffer.from([Number(v)]);
  if (v <= 0xffn) return Buffer.from([0xcc, Number(v)]);
  if (v <= 0xffffn) { const b = Buffer.alloc(3); b[0] = 0xcd; b.writeUInt16BE(Number(v), 1); return b; }
  if (v <= 0xffffffffn) { const b = Buffer.alloc(5); b[0] = 0xce; b.writeUInt32BE(Number(v), 1); return b; }
  const b = Buffer.alloc(9); b[0] = 0xcf; b.writeBigUInt64BE(v, 1); return b;
}

function mpBool(v) {
  return Buffer.from([v ? 0xc3 : 0xc2]);
}

function mpStr(s) {
  const bytes = Buffer.from(s, "utf8");
  const n = bytes.length;
  let header;
  if (n <= 31) header = Buffer.from([0xa0 | n]);
  else if (n <= 255) header = Buffer.from([0xd9, n]);
  else if (n <= 65535) { header = Buffer.alloc(3); header[0] = 0xda; header.writeUInt16BE(n, 1); }
  else { header = Buffer.alloc(5); header[0] = 0xdb; header.writeUInt32BE(n, 1); }
  return Buffer.concat([header, bytes]);
}

function mpBin(bytes) {
  const n = bytes.length;
  let header;
  if (n <= 255) header = Buffer.from([0xc4, n]);
  else if (n <= 65535) { header = Buffer.alloc(3); header[0] = 0xc5; header.writeUInt16BE(n, 1); }
  else { header = Buffer.alloc(5); header[0] = 0xc6; header.writeUInt32BE(n, 1); }
  return Buffer.concat([header, bytes]);
}

function mpArray(items) {
  const n = items.length;
  let header;
  if (n <= 15) header = Buffer.from([0x90 | n]);
  else if (n <= 65535) { header = Buffer.alloc(3); header[0] = 0xdc; header.writeUInt16BE(n, 1); }
  else { header = Buffer.alloc(5); header[0] = 0xdd; header.writeUInt32BE(n, 1); }
  return Buffer.concat([header, ...items]);
}

// Records and request bodies are integer-keyed maps (1-based field/parameter
// numbers) in the revised MSSP v1 wire format.
function mpMap(pairs) {
  const n = pairs.length;
  let header;
  if (n <= 15) header = Buffer.from([0x80 | n]);
  else if (n <= 65535) { header = Buffer.alloc(3); header[0] = 0xde; header.writeUInt16BE(n, 1); }
  else { header = Buffer.alloc(5); header[0] = 0xdf; header.writeUInt32BE(n, 1); }
  return Buffer.concat([header, ...pairs.flatMap(([k, v]) => [k, v])]);
}

function msspFrame(kind, body) {
  const header = Buffer.alloc(16);
  header.writeUInt32BE(0x4d535350, 0); // "MSSP"
  header.writeUInt32BE(1, 4); // version
  // flags at [8..12] stay zero
  header.writeUInt32BE(kind, 12);
  return Buffer.concat([header, body]);
}

// ---- compile and load the AS test module ----

const out = join(mkdtempSync(join(tmpdir(), "syscall-corpus-test-")), "syscall_corpus_test.wasm");
execFileSync("npx", ["asc", "assembly/__tests__/syscall_corpus_test.ts", "--outFile", out, "--bindings", "esm", "--debug"], {
  cwd: here,
  stdio: "inherit",
});
const mod = await import(pathToFileURL(out.replace(/\.wasm$/, ".js")).href);

const results = [];
function report(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  " + detail : ""}`);
}

function toBuffer(actual) {
  return Buffer.from(actual.buffer, actual.byteOffset, actual.byteLength);
}

function bytesEqual(actual, segment) {
  return toBuffer(actual).equals(Buffer.from(segment.buffer, segment.byteOffset, segment.byteLength));
}

function diffDetail(actual, segment) {
  const a = toBuffer(actual);
  const b = Buffer.from(segment.buffer, segment.byteOffset, segment.byteLength);
  const n = Math.min(a.length, b.length);
  let at = -1;
  for (let i = 0; i < n; i++) {
    if (a[i] !== b[i]) { at = i; break; }
  }
  if (at === -1 && a.length !== b.length) at = n;
  return `len ${a.length} vs ${b.length}, first diff at ${at}` +
    (at >= 0 ? ` (0x${a[at]?.toString(16) ?? "eof"} vs 0x${b[at]?.toString(16) ?? "eof"})` : "");
}

// ---- sidecar expect comparison ----

// Deep comparison of two JSON values; returns "" when equal, otherwise a
// human-readable first-difference description.
function jsonDiff(actual, expect, path = "") {
  if (expect === null || typeof expect !== "object") {
    return Object.is(actual, expect)
      ? ""
      : `${path || "$"}: got ${JSON.stringify(actual)}, want ${JSON.stringify(expect)}`;
  }
  if (Array.isArray(expect)) {
    if (!Array.isArray(actual)) return `${path || "$"}: got ${JSON.stringify(actual)}, want an array`;
    if (actual.length !== expect.length) {
      return `${path || "$"}: array length ${actual.length} != ${expect.length}`;
    }
    for (let i = 0; i < expect.length; i++) {
      const d = jsonDiff(actual[i], expect[i], `${path}[${i}]`);
      if (d) return d;
    }
    return "";
  }
  if (actual === null || typeof actual !== "object" || Array.isArray(actual)) {
    return `${path || "$"}: got ${JSON.stringify(actual)}, want an object`;
  }
  const keys = Object.keys(expect);
  for (const k of Object.keys(actual)) {
    if (!keys.includes(k)) return `${path || "$"}: unexpected key ${JSON.stringify(k)}`;
  }
  for (const k of keys) {
    if (!(k in actual)) return `${path || "$"}: missing key ${JSON.stringify(k)}`;
    const d = jsonDiff(actual[k], expect[k], `${path}.${k}`);
    if (d) return d;
  }
  return "";
}

// Runs a wasm decode export and deep-compares the parsed result against the
// sidecar expect object.
function checkDecode(name, decode, expect) {
  let json;
  try {
    json = decode();
  } catch (e) {
    report(name, false, `wasm decode threw: ${e.message ?? e}`);
    return;
  }
  let actual;
  try {
    actual = JSON.parse(json);
  } catch {
    report(name, false, `invalid JSON from wasm: ${json}`);
    return;
  }
  const diff = jsonDiff(actual, expect);
  report(name, diff === "", diff ? `${diff}  (got ${json})` : "");
}

const segments = unpackSegments(corpus);
if (segments.length !== sidecar.frames.length) {
  console.log(`FAIL  segment count ${segments.length} != sidecar frame count ${sidecar.frames.length}`);
  process.exit(1);
}

for (const frame of sidecar.frames) {
  const segment = segments[frame.index];
  const kind = frame.message_kind;
  const label = `${frame.index} ${frame.message_kind_name} ${frame.direction}`;

  report(`${label}: header kind`, mod.headerKind(new Uint8Array(segment)) === kind);

  if (frame.direction === "request") {
    const encoded = mod.encodeRequest(kind);
    report(
      `${label}: request encode byte-exact`,
      bytesEqual(encoded, segment),
      bytesEqual(encoded, segment) ? "" : diffDetail(encoded, segment),
    );
    checkDecode(
      `${label}: request decode matches sidecar expect`,
      () => mod.decodeRequestJson(kind, new Uint8Array(segment)),
      frame.expect,
    );
  } else if (frame.direction === "response") {
    const encoded = mod.encodeOkResponse(kind);
    report(
      `${label}: response encode byte-exact`,
      bytesEqual(encoded, segment),
      bytesEqual(encoded, segment) ? "" : diffDetail(encoded, segment),
    );
    checkDecode(
      `${label}: response decode matches sidecar expect`,
      () => mod.decodeResponseJson(kind, new Uint8Array(segment)),
      frame.expect,
    );
    const reencoded = mod.reencodeOkResponse(kind, new Uint8Array(segment));
    report(
      `${label}: response decode+re-encode byte-identical`,
      bytesEqual(reencoded, segment),
      bytesEqual(reencoded, segment) ? "" : diffDetail(reencoded, segment),
    );
  } else if (frame.direction === "response_err") {
    const encoded = mod.encodeGetErrResponse();
    report(
      `${label}: err response encode byte-exact`,
      bytesEqual(encoded, segment),
      bytesEqual(encoded, segment) ? "" : diffDetail(encoded, segment),
    );
    checkDecode(
      `${label}: err response decode matches sidecar expect`,
      () => mod.decodeGetErrJson(new Uint8Array(segment)),
      frame.expect,
    );
    const reencoded = mod.reencodeGetErrResponse(new Uint8Array(segment));
    report(
      `${label}: err response decode+re-encode byte-identical`,
      bytesEqual(reencoded, segment),
      bytesEqual(reencoded, segment) ? "" : diffDetail(reencoded, segment),
    );
  } else {
    report(`${label}: known direction`, false, `unknown direction ${frame.direction}`);
  }
}

// ---- record/variant codec spot checks against an independent JS encoding ----

const uniErrorExpected = mpMap([
  [mpU64(1), mpU64(7)],
  [mpU64(2), mpStr("boom")],
  [mpU64(3), mpStr("src")],
  [mpU64(4), mpStr("loc")],
  [mpU64(5), mpArray([mpU64(1), mpU64(2), mpU64(3)])], // err_details as MP ARRAY, not bin
]);
const uniErrorActual = mod.encodeUniErrorSpot();
report(
  "spot UniError: encode byte-exact (err_details array quirk)",
  toBuffer(uniErrorActual).equals(uniErrorExpected),
  toBuffer(uniErrorActual).equals(uniErrorExpected) ? "" : `got ${toBuffer(uniErrorActual).toString("hex")}`,
);
report("spot UniError: decode values", mod.verifyUniErrorSpot(new Uint8Array(uniErrorExpected)) === true);

// UniResultSet { eof: true, row_set: [[ [scalar U32 42] ]], cursor: [9, 8] }:
// row = {1: [fields]}, field = UniDataValue [0 (scalar), UniScalarValue [5 (U32), 42]]
// (variants stay [tag, payload]; only records/request bodies are maps).
const uniResultSetExpected = mpMap([
  [mpU64(1), mpBool(true)],
  [mpU64(2), mpArray([mpMap([[mpU64(1), mpArray([mpArray([mpU64(0), mpArray([mpU64(5), mpU64(42)])])])]])])],
  [mpU64(3), mpArray([mpU64(9), mpU64(8)])], // cursor as MP ARRAY, not bin
]);
const uniResultSetActual = mod.encodeUniResultSetSpot();
report(
  "spot UniResultSet: encode byte-exact (cursor array quirk)",
  toBuffer(uniResultSetActual).equals(uniResultSetExpected),
  toBuffer(uniResultSetActual).equals(uniResultSetExpected) ? "" : `got ${toBuffer(uniResultSetActual).toString("hex")}`,
);
report("spot UniResultSet: decode values", mod.verifyUniResultSetSpot(new Uint8Array(uniResultSetExpected)) === true);

// Relation delta through the generated relation-update request encoder:
// {1: oid {1: 0, 2: 0}, 2: "", 3: [], 4: [], 5: [[3, 0, bin <05>]]} wrapped
// in the kind-22 frame (tuple items stay arrays; func-level list<u8> is bin).
const relationDeltaExpected = msspFrame(22, mpMap([
  [mpU64(1), mpMap([[mpU64(1), mpU64(0)], [mpU64(2), mpU64(0)]])],
  [mpU64(2), mpStr("")],
  [mpU64(3), mpArray([])],
  [mpU64(4), mpArray([])],
  [mpU64(5), mpArray([mpArray([mpU64(3), mpU64(0), mpBin(Buffer.from([0x05]))])])],
]));
const relationDeltaActual = mod.encodeRelationDeltaSpot();
report(
  "spot relation delta: encode byte-exact",
  toBuffer(relationDeltaActual).equals(relationDeltaExpected),
  toBuffer(relationDeltaActual).equals(relationDeltaExpected) ? "" : `got ${toBuffer(relationDeltaActual).toString("hex")}`,
);

const fsDirentExpected = mpMap([
  [mpU64(1), mpStr("a.txt")],
  [mpU64(2), mpBool(false)],
  [mpU64(3), mpU64(3)],
]);
const fsDirentActual = mod.encodeFsDirentSpot();
report(
  "spot fs dirent: encode byte-exact",
  toBuffer(fsDirentActual).equals(fsDirentExpected),
  toBuffer(fsDirentActual).equals(fsDirentExpected) ? "" : `got ${toBuffer(fsDirentActual).toString("hex")}`,
);
report("spot fs dirent: decode values", mod.verifyFsDirentSpot(new Uint8Array(fsDirentExpected)) === true);

const failed = results.filter((r) => !r.ok);
console.log(failed.length === 0 ? `\nALL ${results.length} CHECKS PASSED` : `\n${failed.length} CHECK(S) FAILED`);
process.exit(failed.length === 0 ? 0 : 1);
