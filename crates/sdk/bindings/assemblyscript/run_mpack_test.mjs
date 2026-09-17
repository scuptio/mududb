// Runs the mpack boundary tests: compiles assembly/__tests__/mpack_test.ts
// with the local asc into a temp dir, compares the AS-encoded bytes against
// the rmp_serde-produced expected stream (assembly/__tests__/mpack_expected.hex),
// then checks decode round-trips.
// Usage: node run_mpack_test.mjs   (from crates/sdk/bindings/assemblyscript)
import { readFileSync } from "node:fs";
import { mkdtempSync } from "node:fs";
import { tmpdir } from "node:os";
import { execFileSync } from "node:child_process";
import { fileURLToPath, pathToFileURL } from "node:url";
import { dirname, join } from "node:path";

const here = dirname(fileURLToPath(import.meta.url));
const expectedHex = readFileSync(join(here, "assembly", "__tests__", "mpack_expected.hex"), "utf8").trim();
const expected = Buffer.from(expectedHex, "hex");

const out = join(mkdtempSync(join(tmpdir(), "mpack-test-")), "mpack_test.wasm");
execFileSync("npx", ["asc", "assembly/__tests__/mpack_test.ts", "--outFile", out, "--bindings", "esm", "--debug"], {
  cwd: here,
  stdio: "inherit",
});
const mod = await import(pathToFileURL(out.replace(/\.wasm$/, ".js")).href);

const results = [];
function report(name, ok, detail = "") {
  results.push({ name, ok, detail });
  console.log(`${ok ? "PASS" : "FAIL"}  ${name}${detail ? "  " + detail : ""}`);
}

const actual = mod.encodeBoundary();
const actualBuf = Buffer.from(actual.buffer, actual.byteOffset, actual.byteLength);
report(
  "encode: exact bytes vs rmp_serde stream",
  actualBuf.equals(expected),
  `actual ${actualBuf.length}B, expected ${expected.length}B` +
    (actualBuf.equals(expected)
      ? ""
      : `\n  actual:   ${actualBuf.toString("hex")}\n  expected: ${expectedHex}`)
);

report("decode: rmp_serde bytes -> AS values", mod.verifyDecode(new Uint8Array(expected)));
report("roundtrip: AS encode -> AS decode", mod.roundtrip());
report("tryNil: consume nil / not consume non-nil", mod.tryNilProbe());
report("float leniency across 0xCA/0xCB", mod.floatLeniency());
report("int decode leniency (i64 reads 0xCC)", mod.intLeniency());

let u64RejectsNegative = false;
try {
  mod.readU64FromNegfixint();
} catch {
  u64RejectsNegative = true; // rmp_serde errors with "expected u64" here
}
report("readU64 rejects negative encoding", u64RejectsNegative);

const failed = results.filter((r) => !r.ok);
console.log(failed.length === 0 ? `\nALL ${results.length} CHECKS PASSED` : `\n${failed.length} CHECK(S) FAILED`);
process.exit(failed.length === 0 ? 0 : 1);
