"""Tests for the verification gate (no network or proof tools required)."""
import contextlib
import io
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))
sys.path.insert(0, str(HERE.parent))
import check
import impact
import scaffold
import toolchain
import doctor


class VerificationTests(unittest.TestCase):
    def test_stale_extraction_rejected(self):
        check.compare("same", "same")
        with self.assertRaisesRegex(RuntimeError, "stale"):
            check.compare("old", "new")

    def test_audit_rejects_missing_and_unapproved_dependencies(self):
        theorems = [name for target in check.TARGETS.values() for name in check.audited_names(target)]
        good = "\n".join(f"'{n}' depends on axioms: [propext, Classical.choice, Quot.sound]"
                         for n in theorems)
        check.audit_output(good, check.TARGETS)
        with self.assertRaisesRegex(RuntimeError, "Unapproved"):
            check.audit_output(good.replace("propext", "sorryAx"), check.TARGETS)
        with self.assertRaisesRegex(RuntimeError, "Missing"):
            check.audit_output("", check.TARGETS)

    def test_generated_rel_mirrors_source_layout(self):
        base = Path("verification/lean/crates")
        self.assertEqual(check.generated_rel({"source": "crates/common/mudu/src/utils/buf.rs"}),
                         base / "common/mudu/src/utils/buf/generated.lean")
        self.assertEqual(check.generated_rel({"source": "crates/db-kernel/wal/src/layout/mod.rs"}),
                         base / "db-kernel/wal/src/layout/generated.lean")
        self.assertEqual(check.generated_rel({"source": "crates/common/mudu/src/lib.rs"}),
                         base / "common/mudu/src/lib/generated.lean")

    def test_registered_targets_are_self_consistent(self):
        for name, target in check.TARGETS.items():
            self.assertTrue((check.tc.ROOT / target["source"]).is_file(), name)
            self.assertTrue((check.tc.ROOT / target["entry"]).is_file(), name)
            self.assertTrue((check.tc.ROOT / check.generated_rel(target)).is_file(), name)
            self.assertTrue(target["module"].endswith(".proof"), name)
            self.assertTrue(target["theorems"], name)
            self.assertTrue(target["functions"], name)

    def test_lakefile_roots_cover_registered_modules(self):
        check.check_roots_registered(check.tc.ROOT)
        self.assertTrue(check.expected_roots())

    def test_missing_lakefile_root_is_rejected(self):
        with patch.object(check, "lakefile_roots", return_value=set()):
            with self.assertRaisesRegex(RuntimeError, "roots miss"):
                check.check_roots_registered(check.tc.ROOT)

    def test_empty_mutation_registry_is_only_a_self_test_error(self):
        with patch.object(check, "TARGETS", {}), patch.object(check, "check"):
            self.assertEqual(check.mutations({}), [])
            with self.assertRaisesRegex(RuntimeError, "no self-test mutation"):
                check.self_test({})

    def test_every_mutation_is_run_independently(self):
        cases = check.mutations(check.TARGETS)
        self.assertGreaterEqual(len(cases), 7)
        with patch.object(check, "check"), patch.object(check, "self_test_mutation") as run:
            check.self_test(check.TARGETS)
        self.assertEqual(run.call_args_list, [unittest.mock.call(*case) for case in cases])

    def test_mutation_failure_is_not_masked_by_another_case(self):
        with patch.object(check, "check"), patch.object(check, "self_test_mutation") as run:
            run.side_effect = [None, RuntimeError("mutant passed")]
            with self.assertRaisesRegex(RuntimeError, "mutant passed"):
                check.self_test(check.TARGETS)
        self.assertEqual(run.call_count, 2)

    def test_each_mutation_has_a_unique_production_site(self):
        for name, target, mutation in check.mutations(check.TARGETS):
            source = (check.tc.ROOT / target["source"]).read_text()
            self.assertEqual(source.count(mutation["mutation_from"]), 1,
                             (name, mutation["mutation_note"]))

    def test_proof_error_pattern_matches_lean_diagnostic(self):
        target = next(iter(check.TARGETS.values()))
        pattern = check.proof_error_pattern(target)
        self.assertIsNotNone(pattern.match(
            "error: crates/common/mudu/src/utils/buf/proof.lean:42:5: unsolved goals"))
        self.assertIsNone(pattern.match("error: other/file.lean:1:1: sorry"))

    def test_download_rejects_wrong_checksum(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "artifact"
            with patch("urllib.request.urlopen", return_value=io.BytesIO(b"wrong")):
                with self.assertRaisesRegex(RuntimeError, "SHA256"):
                    toolchain.download("https://example.invalid/tool", path, "0" * 64)
            self.assertFalse(path.exists())

    def test_dry_run_does_not_download_or_run(self):
        with patch.object(toolchain, "prerequisites", return_value=[]), \
             patch.object(toolchain, "download") as download, \
             patch.object(toolchain, "run") as run:
            toolchain.install(dry_run=True)
            download.assert_not_called()
            run.assert_not_called()

    def test_missing_prerequisite_stops_install(self):
        with patch.object(toolchain, "prerequisites", return_value=["rustup"]), \
             patch.object(toolchain, "download") as download:
            with self.assertRaisesRegex(RuntimeError, "rustup"):
                toolchain.install()
            download.assert_not_called()

    def test_doctor_only_is_idempotent(self):
        entry = doctor.Entry("formal-verification", "formal", False, lambda: (True, "ready"), "verify")
        with patch.object(doctor, "collect_entries", return_value=[entry]), \
             patch("subprocess.run") as run, contextlib.redirect_stdout(io.StringIO()):
            self.assertEqual(doctor.cmd_install(["--only", "formal-verification"]), 0)
            run.assert_not_called()

    def test_doctor_failed_install_returns_error(self):
        entry = doctor.Entry("formal-verification", "formal", False, lambda: (False, "missing"),
                             "verify", install=[["not-executed"]])
        with patch.object(doctor, "collect_entries", return_value=[entry]), \
             patch("subprocess.run") as run, contextlib.redirect_stdout(io.StringIO()):
            run.return_value.returncode = 1
            self.assertEqual(doctor.cmd_install(["--only", "formal-verification"]), 1)

    def test_os_preview_does_not_install(self):
        with patch.dict("os.environ", {"DOCTOR_OS": "windows"}):
            self.assertIsNone(doctor.formal_verification_entry().install)

    def test_modified_tool_lock_is_detected(self):
        with tempfile.TemporaryDirectory() as tmp:
            stamp = Path(tmp) / "stamp.json"
            stamp.write_text('{"lock": "old", "binaries": {}}')
            with patch.object(toolchain, "STAMP", stamp), \
                 patch.object(toolchain, "prerequisites", return_value=[]):
                self.assertEqual(toolchain.probe()[0], False)


class RegistryValidationTests(unittest.TestCase):
    def _target(self, **overrides):
        target = {
            "source": "crates/common/mudu/src/utils/buf.rs",
            "entry": "verification/extract/Cargo.toml",
            "extract_module": "buf",
            "platform": "x86_64-unknown-linux-gnu",
            "features": [],
            "functions": ["sized_buf_len"],
            "module": "crates.common.mudu.src.utils.buf.proof",
            "status": "proven",
            "owner": "mudu",
            "assumptions": [],
            "theorems": ["sized_buf_proof.roundtrip"],
            "sanity": ["sized_buf_proof.sanity_roundtrip"],
            "allowed_axioms": ["propext"],
        }
        target.update(overrides)
        return target

    def test_real_registry_is_valid(self):
        check.validate_registry(check.tc.ROOT, check.TARGETS)

    def test_status_must_be_known(self):
        with self.assertRaisesRegex(RuntimeError, "status"):
            check.validate_registry(check.tc.ROOT, {"x": self._target(status="done")})

    def test_owner_and_assumptions_are_required(self):
        with self.assertRaisesRegex(RuntimeError, "owner"):
            check.validate_registry(check.tc.ROOT, {"x": self._target(owner="")})
        target = self._target()
        del target["assumptions"]
        with self.assertRaisesRegex(RuntimeError, "assumptions"):
            check.validate_registry(check.tc.ROOT, {"x": target})

    def test_proven_requires_theorems_and_sanity(self):
        with self.assertRaisesRegex(RuntimeError, "theorem"):
            check.validate_registry(check.tc.ROOT, {"x": self._target(theorems=[])})
        with self.assertRaisesRegex(RuntimeError, "sanity"):
            check.validate_registry(check.tc.ROOT, {"x": self._target(sanity=[])})

    def test_premise_references_must_resolve(self):
        bad = self._target(premises=[{"note": "n", "established_by": ["no/such/file.rs"]}])
        with self.assertRaisesRegex(RuntimeError, "not found"):
            check.validate_registry(check.tc.ROOT, {"x": bad})
        bad = self._target(premises=[{"note": "n", "established_by": [
            "crates/common/mudu/src/utils/buf.rs::no_such_symbol"]}])
        with self.assertRaisesRegex(RuntimeError, "no_such_symbol"):
            check.validate_registry(check.tc.ROOT, {"x": bad})
        good = self._target(premises=[{"note": "n", "established_by": [
            "crates/common/mudu/src/utils/buf.rs::write_sized_buf"]}])
        check.validate_registry(check.tc.ROOT, {"x": good})

    def test_broken_mutation_anchor_is_a_preflight_error(self):
        broken = self._target(mutations=[{
            "mutation_note": "m", "mutation_from": "no such code", "mutation_to": "x"}])
        with self.assertRaisesRegex(RuntimeError, "mutation site"):
            check.validate_registry(check.tc.ROOT, {"x": broken})


class LeanLintTests(unittest.TestCase):
    def test_comment_stripping_handles_nested_blocks(self):
        text = "def a := 1 -- sorry in a comment\n/- outer /- inner sorry -/ still comment -/\ndef b := 2\n"
        stripped = check.strip_lean_comments(text)
        self.assertNotIn("sorry", stripped)
        self.assertIn("def a := 1", stripped)
        self.assertIn("def b := 2", stripped)

    def test_lint_rejects_sorry_in_code(self):
        with tempfile.TemporaryDirectory() as tmp:
            lean = Path(tmp) / "verification/lean/pkg"
            lean.mkdir(parents=True)
            (lean / "proof.lean").write_text("theorem t : True := by sorry\n")
            with self.assertRaisesRegex(RuntimeError, "sorry"):
                check.lint_lean_sources(Path(tmp))

    def test_lint_accepts_comments_and_clean_code(self):
        with tempfile.TemporaryDirectory() as tmp:
            lean = Path(tmp) / "verification/lean/pkg"
            lean.mkdir(parents=True)
            (lean / "proof.lean").write_text(
                "-- sorry is forbidden here\n/- and admit in block comments -/\ntheorem t : True := by trivial\n")
            (lean / ".lake").mkdir()
            (lean / ".lake" / "ignored.lean").write_text("theorem t : True := by sorry\n")
            check.lint_lean_sources(Path(tmp))

    def test_lint_rejects_native_decide_and_axiom(self):
        with tempfile.TemporaryDirectory() as tmp:
            lean = Path(tmp) / "verification/lean/pkg"
            lean.mkdir(parents=True)
            (lean / "a.lean").write_text("example : True := by native_decide\n")
            with self.assertRaisesRegex(RuntimeError, "native_decide"):
                check.lint_lean_sources(Path(tmp))
            (lean / "a.lean").write_text("axiom my_fact : True\n")
            with self.assertRaisesRegex(RuntimeError, "axiom"):
                check.lint_lean_sources(Path(tmp))


class ImpactTests(unittest.TestCase):
    def test_crate_change_affects_only_that_crates_targets(self):
        affected = impact.affected_targets(
            ["crates/db-kernel/wal/src/part_chain.rs"], check.TARGETS)
        self.assertEqual(affected, {"append_plan", "decision", "frame_bounds", "part_chain", "truncate_plan"})

    def test_common_change_affects_everything(self):
        affected = impact.affected_targets(["crates/common/mudu/src/utils/buf.rs"], check.TARGETS)
        self.assertEqual(affected, set(check.TARGETS))

    def test_registry_and_scripts_affect_everything(self):
        for path in ("verification/targets.toml", "script/verification/check.py",
                     "verification/lean/lakefile.toml", "Makefile.toml"):
            self.assertEqual(impact.affected_targets([path], check.TARGETS), set(check.TARGETS), path)

    def test_unrelated_change_affects_nothing(self):
        self.assertEqual(impact.affected_targets(["doc/en/formal_verification.md"], check.TARGETS), set())

    def test_glob_matcher(self):
        self.assertTrue(impact.glob_covers("crates/db-kernel/wal/src/**", "crates/db-kernel/wal/src/part_chain.rs"))
        self.assertTrue(impact.glob_covers("crates/db-kernel/wal/src/**", "crates/db-kernel/wal/src/a/b/c.rs"))
        self.assertTrue(impact.glob_covers("crates/common/mudu/src/utils/buf*", "crates/common/mudu/src/utils/buf.rs"))
        self.assertFalse(impact.glob_covers("crates/db-kernel/wal/src/**", "crates/db-kernel/storage/src/page/x.rs"))
        self.assertTrue(impact.glob_covers("Makefile.toml", "Makefile.toml"))
        self.assertFalse(impact.glob_covers("Makefile.toml", "Makefile.toml.bak"))

    def test_workflow_covers_every_registered_target(self):
        problems, _ = impact.check_ci(
            check.tc.ROOT / ".github/workflows/formal-verification.yaml", check.TARGETS)
        self.assertEqual(problems, [])


class SweepCandidateTests(unittest.TestCase):
    def test_flips_are_single_site_and_skip_comments(self):
        source = "fn f() {\n    if a < b && c { } // x <= y\n    let _ = a <= b;\n}\n"
        candidates = check.sweep_candidates(source, self._no_mutations())
        notes = [c["note"] for c in candidates]
        self.assertIn("line 2: `<` -> `<=`", notes)
        self.assertIn("line 2: `&&` -> `||`", notes)
        self.assertIn("line 3: `<=` -> `<`", notes)
        self.assertEqual(len(notes), 3, notes)  # the comment site is skipped

    def test_registered_mutations_are_not_reduplicated(self):
        mutation = {"mutation_note": "m", "mutation_from": "a < b", "mutation_to": "a <= b"}
        candidates = check.sweep_candidates("fn f() { a < b }\n", {"mutations": [mutation]})
        self.assertEqual(candidates, [])

    def test_arrows_and_shifts_are_not_operator_sites(self):
        source = "fn f(x: i32) -> i32 { x >> 2 }\n"
        self.assertEqual(check.sweep_candidates(source, {"mutations": []}), [])

    def _no_mutations(self):
        return {"mutations": []}


class ReportTests(unittest.TestCase):
    def test_report_lists_every_target(self):
        with tempfile.TemporaryDirectory() as tmp:
            with patch.object(check.tc, "CACHE", Path(tmp)):
                check.report(check.TARGETS)
            text = (Path(tmp) / "report/verification-report.md").read_text()
            for name in check.TARGETS:
                self.assertIn(f"`{name}`", text)
            self.assertIn("Trust boundary", text)

    def test_sanity_theorems_join_the_axiom_audit(self):
        target = next(iter(check.TARGETS.values()))
        names = check.audited_names(target)
        self.assertEqual(names, list(target["theorems"]) + list(target["sanity"]))
        self.assertIn("sanity_", "".join(target["sanity"]))


class ScaffoldTests(unittest.TestCase):
    def test_module_base_mirrors_source(self):
        self.assertEqual(scaffold.module_base_from_source("crates/common/mudu/src/utils/buf.rs"),
                         "crates.common.mudu.src.utils.buf")
        self.assertEqual(scaffold.module_base_from_source("crates/db-kernel/wal/src/frame_bounds.rs"),
                         "crates.«db-kernel».wal.src.frame_bounds")
        self.assertEqual(scaffold.module_base_from_source("crates/common/mudu/src/utils/mod.rs"),
                         "crates.common.mudu.src.utils")

    def test_registry_entry_is_extracted_and_valid_toml(self):
        import tomllib
        entry = scaffold.render_registry_entry("widget", "crates/common/mudu/src/utils/buf.rs",
                                               "mudu", ["sized_buf_len"],
                                               "crates.common.mudu.src.utils.buf")
        parsed = tomllib.loads(entry)["target"]["widget"]
        self.assertEqual(parsed["status"], "extracted")
        self.assertEqual(parsed["theorems"], [])
        self.assertEqual(parsed["sanity"], [])
        self.assertEqual(parsed["module"], "crates.common.mudu.src.utils.buf.proof")

    def test_skeletons_pass_lean_lint(self):
        spec = scaffold.render_spec_lean("widget", "crates.common.mudu.src.utils.buf")
        proof = scaffold.render_proof_lean("widget", "crates.common.mudu.src.utils.buf")
        for text in (spec, proof):
            stripped = check.strip_lean_comments(text)
            for pattern, _label in check.FORBIDDEN_LEAN:
                self.assertIsNone(pattern.search(stripped))
        self.assertIn("import crates.common.mudu.src.utils.buf.generated", spec)
        self.assertIn("import crates.common.mudu.src.utils.buf.spec", proof)

    def test_extract_line_and_roots(self):
        line = scaffold.render_extract_line("crates/common/mudu/src/utils/buf.rs", "buf")
        self.assertIn('#[path = "../../crates/common/mudu/src/utils/buf.rs"]', line)
        self.assertEqual(scaffold.render_lakefile_roots("crates.common.mudu.src.utils.buf"),
                         ("crates.common.mudu.src.utils.buf.generated",
                          "crates.common.mudu.src.utils.buf.spec",
                          "crates.common.mudu.src.utils.buf.proof"))

    def test_plan_rejects_duplicate_registry_entry(self):
        with self.assertRaises(RuntimeError):
            scaffold.plan(check.tc.ROOT, name="buf",
                          source="crates/common/mudu/src/utils/buf.rs",
                          owner="mudu", functions=["sized_buf_len"], module_base=None)


class PremiseFreshnessTests(unittest.TestCase):
    def test_invocation_mapping_lib_and_test_target(self):
        cwd, package, target, symbol = check.premise_test_invocation(
            "crates/common/mudu_binding/src/universal/mp_wire.rs::encoder_matches_rmp_serde_byte_for_byte")
        self.assertEqual((cwd, package, target, symbol),
                         (Path("crates/common"), "mudu_binding", None,
                          "encoder_matches_rmp_serde_byte_for_byte"))
        cwd, package, target, symbol = check.premise_test_invocation(
            "crates/db-kernel/testing/tests/compat_golden/mp_primitives.rs::mp_primitives_v1_roundtrip")
        self.assertEqual((cwd, package, target, symbol),
                         (Path("crates/db-kernel"), "testing", "compat_golden",
                          "mp_primitives_v1_roundtrip"))

    def test_invocation_rejects_symbol_less_reference(self):
        with self.assertRaises(RuntimeError):
            check.premise_test_invocation("crates/common/mudu/src/utils/buf.rs")

    def test_every_registered_premise_reference_maps(self):
        # The full registry maps cleanly: no symbol-less reference is runnable,
        # so current entries must all carry symbols.
        for name, target in check.TARGETS.items():
            for premise in target.get("premises", []):
                for ref in premise["established_by"]:
                    self.assertIn("::", ref, f"{name}: {ref}")
                    check.premise_test_invocation(ref)

    def test_passing_test_line_detection(self):
        text = "test universal::mp_wire::tests::encoder_matches_rmp_serde_byte_for_byte ... ok\n"
        import re as _re
        pattern = _re.compile(r"(?m)^test (?:\S*::)?" + _re.escape(
            "encoder_matches_rmp_serde_byte_for_byte") + r" \.\.\. ok$")
        self.assertIsNotNone(pattern.search(text))
        self.assertIsNone(pattern.search("test other_test ... ok\n"))


class SpecDiffTests(unittest.TestCase):
    def test_declaration_signatures_single_and_multiline(self):
        text = """
/-- doc -/
def enc_uint (v : U64) : List U8 := if v.val ≤ 0x7f then [low_byte v.bv] else []

@[step] theorem write_success (s : Slot) (out : Aeneas.Std.Slice U8) (h : 32 ≤ out.length) :
    record_slot_core.encode_bytes s out
      ⦃ r => r = .Ok () ⦄ := by
  unfold x

private theorem helper (n : Nat) : n = n := rfl
"""
        decls = check.declaration_signatures(text)
        self.assertIn("enc_uint", decls)
        self.assertIn("write_success", decls)
        self.assertIn("helper", decls)
        self.assertIn("record_slot_core.encode_bytes s out", decls["write_success"])
        self.assertNotIn(":=", decls["write_success"])
        # comments stripped: the doc text must not leak into signatures
        self.assertNotIn("doc", decls["enc_uint"])


if __name__ == "__main__":
    unittest.main()
