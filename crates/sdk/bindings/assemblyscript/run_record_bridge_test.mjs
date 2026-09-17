// Runs the record bridge tests: compiles assembly/__tests__/record_bridge_test.ts
// with the local asc into a temp dir, then checks the bridge in both
// directions against an independent minimal-form MessagePack encoding
// (mirroring mpack.ts / rmp_serde 1.3.x rules):
//   - recordFieldValues: host-style record-case envelope -> integer-keyed
//     map bytes (byte-exact),
//   - the full guest decode path (envelope bytes -> UniDataValueCodec ->
//     recordFieldValues -> ProfileCodec.decode) including the lenient
//     i32 0/1 -> bool read the host's no-Bool vocabulary requires,
//   - recordFromFieldValues: generated-codec map bytes -> record-case
//     envelope (byte-exact), including option gap-fill (a middle omitted
//     field becomes the Null scalar) and trailing-omission shrink,
//   - isNullDatum and the non-record rejection.
// Usage: node run_record_bridge_test.mjs   (from crates/sdk/bindings/assemblyscript)
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));

const out = join(mkdtempSync(join(tmpdir(), "record-bridge-test-")), "record_bridge_test.wasm");
execFileSync(
  "npx",
  ["asc", "assembly/__tests__/record_bridge_test.ts", "--outFile", out, "--bindings", "esm", "--debug"],
  { cwd: here, stdio: "inherit" },
);
const mod = await import(pathToFileURL(out.replace(/\.wasm$/, ".js")).href);

const results = [];
function report(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  " + detail : ""}`);
}

function bytesEqual(actual, expected) {
  const a = Buffer.from(actual.buffer, actual.byteOffset, actual.byteLength);
  if (a.equals(expected)) return true;
  report("  bytes differ", false, `\n    actual   ${a.toString("hex")}\n    expected ${expected.toString("hex")}`);
  return false;
}

// ---- independent minimal-form MessagePack encoder ----

function mpUint(v) {
  v = BigInt(v);
  if (v <= 0x7fn) return Buffer.from([Number(v)]);
  if (v <= 0xffn) return Buffer.from([0xcc, Number(v)]);
  if (v <= 0xffffn) {
    const b = Buffer.alloc(3);
    b[0] = 0xcd;
    b.writeUInt16BE(Number(v), 1);
    return b;
  }
  if (v <= 0xffffffffn) {
    const b = Buffer.alloc(5);
    b[0] = 0xce;
    b.writeUInt32BE(Number(v), 1);
    return b;
  }
  const b = Buffer.alloc(9);
  b[0] = 0xcf;
  b.writeBigUInt64BE(v, 1);
  return b;
}

function mpStr(s) {
  const t = Buffer.from(s, "utf8");
  if (t.length <= 31) return Buffer.concat([Buffer.from([0xa0 | t.length]), t]);
  if (t.length <= 255) return Buffer.concat([Buffer.from([0xd9, t.length]), t]);
  const b = Buffer.alloc(3);
  b[0] = 0xda;
  b.writeUInt16BE(t.length, 1);
  return Buffer.concat([b, t]);
}

function mpF64(v) {
  const b = Buffer.alloc(9);
  b[0] = 0xcb;
  b.writeDoubleBE(v, 1);
  return b;
}

function mpArr(items) {
  return Buffer.concat([Buffer.from([0x90 | items.length]), ...items]);
}

function mpMap(entries) {
  return Buffer.concat([Buffer.from([0x80 | entries.length]), ...entries.flat()]);
}

// ---- the host envelope wire forms (uni-data-value / uni-scalar-value) ----

const scalar = (tag, payload) => Buffer.concat([Buffer.from([0x92, 0x00, 0x92, tag]), payload]);
const strDatum = (s) => scalar(14, mpStr(s));
const i32Datum = (v) => scalar(6, mpUint(v));
const i64Datum = (v) => scalar(9, mpUint(v));
const f64Datum = (v) => scalar(12, mpF64(v));
const nullDatum = () => scalar(21, Buffer.from([0x00]));
const blobDatum = (bytes) => scalar(15, mpArr(bytes.map(mpUint)));
const arrDatum = (items) => Buffer.concat([Buffer.from([0x92, 0x01]), mpArr(items)]);
const recDatum = (fields) => Buffer.concat([Buffer.from([0x92, 0x02]), mpArr(fields)]);
const envField = (name, datum) => mpMap([
  [mpUint(1), mpStr(name)],
  [mpUint(2), datum],
]);

// ---- expected values ----

// The integer-keyed map the generated ProfileCodec consumes/produces. A
// null option field crosses as nil (the host sends the Null scalar).
function expectedFieldMap(withHome) {
  const entries = [
    [mpUint(1), mpStr("Ada")],
    [mpUint(2), mpUint(7)],
    [mpUint(3), mpUint(1)],
    withHome
      ? [mpUint(4), mpMap([
          [mpUint(1), mpStr("Paris")],
          [mpUint(2), mpStr("75001")],
        ])]
      : [mpUint(4), Buffer.from([0xc0])],
    [mpUint(5), mpArr([mpStr("x"), mpStr("yy")])],
    [mpUint(6), mpF64(2.5)],
    [mpUint(7), mpUint(5000000000)],
    [mpUint(8), mpArr([mpUint(1), mpUint(2), mpUint(255)])],
    [mpUint(9), mpStr("ace")],
  ];
  return mpMap(entries);
}

// The host-style record-case envelope (positional, names empty).
function expectedEnvelope(withHome, withNickname) {
  const fields = [
    envField("", strDatum("Ada")),
    envField("", i32Datum(7)),
    envField("", i32Datum(1)),
    withHome
      ? envField("", recDatum([envField("", strDatum("Paris")), envField("", strDatum("75001"))]))
      : envField("", nullDatum()),
    envField("", arrDatum([strDatum("x"), strDatum("yy")])),
    envField("", f64Datum(2.5)),
    envField("", i64Datum(5000000000)),
    // A byte array leaves the guest as the homogeneous integer-array wire
    // form, so the bridge wraps it as an Array datum of i32 items.
    envField("", arrDatum([i32Datum(1), i32Datum(2), i32Datum(255)])),
  ];
  if (withNickname) {
    fields.push(envField("", strDatum("ace")));
  }
  return recDatum(fields);
}

// The host-supplied param envelope: the UniDataValue record case
// `[2, [field, ...]]` (first field named: names are ignored). A null option
// field crosses as the Null scalar at its declaration position.
// `withUnknown` appends a 10th positional entry: the bridge maps it to key
// 10, which the generated ProfileCodec does not declare and skips (lenient
// decode).
function hostEnvelopeBytes({ withHome = true, withNickname = true, withUnknown = false } = {}) {
  const fields = [
    envField("display-name", strDatum("Ada")),
    envField("", i32Datum(7)),
    envField("", i32Datum(1)), // vip crosses as i32 0/1 (no Bool family)
    withHome
      ? envField("", recDatum([envField("", strDatum("Paris")), envField("", strDatum("75001"))]))
      : envField("", nullDatum()),
    envField("", arrDatum([strDatum("x"), strDatum("yy")])),
    envField("", f64Datum(2.5)),
    envField("", i64Datum(5000000000)),
    envField("", blobDatum([1, 2, 255])),
  ];
  if (withNickname) {
    fields.push(envField("", strDatum("ace")));
  }
  if (withUnknown) {
    fields.push(envField("", strDatum("ignored")));
  }
  return recDatum(fields);
}

// ---- checks ----

report(
  "recordFieldValues byte-exact (with home)",
  bytesEqual(mod.envelopeToFieldMap(true), expectedFieldMap(true)),
);
report(
  "recordFieldValues byte-exact (home omitted)",
  bytesEqual(mod.envelopeToFieldMap(false), expectedFieldMap(false)),
);

const summary = mod.decodeProfileSummary(hostEnvelopeBytes());
report(
  "guest decode path (i32 bool, nested record, blob, i64)",
  summary === "Ada|7|true|Paris,75001|x,yy|2.5|5000000000|0102ff|ace",
  summary,
);
const sparseSummary = mod.decodeProfileSummary(
  hostEnvelopeBytes({ withHome: false, withNickname: false }),
);
report(
  "guest decode defaults missing fields",
  sparseSummary === "Ada|7|true|<null>|x,yy|2.5|5000000000|0102ff|<null>",
  sparseSummary,
);
const unknownSummary = mod.decodeProfileSummary(hostEnvelopeBytes({ withUnknown: true }));
report(
  "guest decode skips undeclared extra fields",
  unknownSummary === "Ada|7|true|Paris,75001|x,yy|2.5|5000000000|0102ff|ace",
  unknownSummary,
);

report(
  "recordFromFieldValues byte-exact (full)",
  bytesEqual(mod.envelopeFromProfile(true, true), expectedEnvelope(true, true)),
);
report(
  "recordFromFieldValues gap-fills omitted option with Null",
  bytesEqual(mod.envelopeFromProfile(false, true), expectedEnvelope(false, true)),
);
report(
  "recordFromFieldValues shrinks trailing omitted option",
  bytesEqual(mod.envelopeFromProfile(false, false), expectedEnvelope(false, false)),
);

report("isNullDatum", mod.nullDatumCheck() === true);

let rejected = false;
try {
  mod.rejectNonRecord();
} catch {
  rejected = true;
}
report("recordFieldValues rejects a non-record datum", rejected);

const failed = results.filter((r) => !r.ok);
console.log(failed.length === 0 ? `\nALL ${results.length} CHECKS PASSED` : `\n${failed.length} CHECK(S) FAILED`);
process.exit(failed.length === 0 ? 0 : 1);
