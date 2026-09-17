// Runs the MSSP lenient-decode vector tests for the mgen-generated
// AssemblyScript codec: compiles assembly/__tests__/syscall_corpus_test.ts
// with the local asc into a temp dir (the same entry the syscall corpus
// runner uses — it exports the decoders), then drives it against the
// hand-built NON-canonical frames in
// crates/db-kernel/testing/fixtures/golden/v1/lenient_decode_v1.bin
// (8 vectors: integer width widening, unsigned markers for signed values,
// wide negative ints, record map key reordering, unknown/skipped map keys,
// missing request parameters, missing record fields/variant cases).
// The vectors are not re-encodable by the canonical encoder, so no encode or
// re-encode comparison is done; for every vector the runner checks, through
// the generated AS codec running in wasm:
//   - the header routes to the expected message kind,
//   - decodeRequestJson/decodeResponseJson (selected by the sidecar
//     direction) deep-equals the sidecar `expect` object from
//     lenient_decode_v1.json, which is the single source of truth.
// Usage: node run_lenient_decode_test.mjs   (from crates/sdk/bindings/assemblyscript)
import { readFileSync, mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const fixtureDir = join(here, "..", "..", "..", "db-kernel", "testing", "fixtures", "golden", "v1");
const corpus = readFileSync(join(fixtureDir, "lenient_decode_v1.bin"));
const sidecar = JSON.parse(readFileSync(join(fixtureDir, "lenient_decode_v1.json"), "utf8"));

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

// ---- compile and load the AS test module ----

const out = join(mkdtempSync(join(tmpdir(), "lenient-decode-test-")), "syscall_corpus_test.wasm");
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
if (segments.length !== sidecar.vectors.length) {
  console.log(`FAIL  segment count ${segments.length} != sidecar vector count ${sidecar.vectors.length}`);
  process.exit(1);
}

for (const vector of sidecar.vectors) {
  const segment = segments[vector.index];
  const kind = vector.message_kind;
  const label = `${vector.index} ${vector.kind} (${vector.message_kind_name} ${vector.direction})`;

  report(`${label}: header kind`, mod.headerKind(new Uint8Array(segment)) === kind);

  if (vector.direction === "request") {
    checkDecode(
      `${label}: request decode matches sidecar expect`,
      () => mod.decodeRequestJson(kind, new Uint8Array(segment)),
      vector.expect,
    );
  } else if (vector.direction === "response") {
    checkDecode(
      `${label}: response decode matches sidecar expect`,
      () => mod.decodeResponseJson(kind, new Uint8Array(segment)),
      vector.expect,
    );
  } else {
    report(`${label}: known direction`, false, `unknown direction ${vector.direction}`);
  }
}

const failed = results.filter((r) => !r.ok);
console.log(failed.length === 0 ? `\nALL ${results.length} CHECKS PASSED` : `\n${failed.length} CHECK(S) FAILED`);
process.exit(failed.length === 0 ? 0 : 1);
