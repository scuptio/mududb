#!/usr/bin/env python3
"""Extract production Rust, check generation drift, build Lean, and audit axioms.

Actions:
- check: drift-check committed generated Lean, build proofs, audit axioms
- extract: regenerate the committed generated Lean (proofs not checked)
- self-test: every registered mutation must be rejected by its production proof
- drift: extraction plus drift comparison only; the fast pre-commit gate
- determinism: extract every selected target twice and byte-compare
- mutation-sweep: run pattern-generated candidate mutants and report the kill rate
- report: trust-boundary report from the registry (no toolchain required)
- new-target: scaffold a registry entry + extraction module + spec/proof skeletons
- premises: run the tests cited as premise evidence; references must still pass
- spec-diff: declaration-level diff of spec/proof Lean files against a base ref (advisory)

Target selection: --targets a,b selects by name; --affected-by/--affected-by-file,
--staged, and --diff REF map changed files to affected targets via impact.py.
Without selection options every registered target is processed, so adding a
target to the registry is sufficient.
"""
from __future__ import annotations

import argparse
from contextlib import nullcontext
import difflib
import json
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import tomllib

import impact
import scaffold
import toolchain as tc
from cli import normalized_argv

TARGETS = tomllib.loads((tc.ROOT / "verification/targets.toml").read_text())["target"]

STATUSES = ("extracted", "proven")

FORBIDDEN_LEAN = [
    (re.compile(r"\bsorry\b"), "sorry"),
    (re.compile(r"\badmit\b"), "admit"),
    (re.compile(r"(?m)^\s*axiom\s"), "axiom declaration"),
    (re.compile(r"\bnative_decide\b"), "native_decide"),
    (re.compile(r"\bimplemented_by\b"), "implemented_by"),
]

# Single-site operator flips used by the mutation sweep. Longer operators come
# first so `<=` is not matched as `<` followed by `=`.
SWEEP_FLIPS = [
    ("<=", "<"), ("<", "<="),
    (">=", ">"), (">", ">="),
    ("==", "!="), ("!=", "=="),
    ("&&", "||"), ("||", "&&"),
]


def stage(title: str):
    return tc.REPORT.stage(title) if tc.REPORT is not None else nullcontext()


# --- registry validation -------------------------------------------------

def validate_registry(root: Path, targets: dict) -> None:
    """Schema, premise references, and mutation anchors; cheap file reads only."""
    problems = []
    for name, target in targets.items():
        status = target.get("status")
        if status not in STATUSES:
            problems.append(f"{name}: status must be one of {STATUSES}, got {status!r}")
        if not target.get("owner", "").strip():
            problems.append(f"{name}: owner must be a non-empty string")
        if "assumptions" not in target:
            problems.append(f"{name}: assumptions list must be present (empty list is a valid, explicit choice)")
        if status == "proven":
            if not target.get("theorems"):
                problems.append(f"{name}: status=proven requires at least one registered theorem")
            if not target.get("sanity"):
                problems.append(f"{name}: status=proven requires at least one sanity theorem")
        for premise in target.get("premises", []):
            if not premise.get("note", "").strip():
                problems.append(f"{name}: premise without a note")
            for ref in premise.get("established_by", []):
                path, _, symbol = ref.partition("::")
                file = root / path
                if not file.is_file():
                    problems.append(f"{name}: premise reference not found: {path}")
                elif symbol and not re.search(r"\b" + re.escape(symbol) + r"\b", file.read_text(errors="replace")):
                    problems.append(f"{name}: premise symbol {symbol} not found in {path}")
        source_path = root / target["source"]
        if source_path.is_file():
            source = source_path.read_text()
            for mutation in target.get("mutations", []):
                if source.count(mutation["mutation_from"]) != 1:
                    problems.append(f"{name}: mutation site changed; update targets.toml: {mutation['mutation_note']}")
        else:
            problems.append(f"{name}: source not found: {target['source']}")
    if problems:
        raise RuntimeError("targets.toml registry validation failed:\n" + "\n".join("- " + p for p in problems))


# --- Lean source lint ------------------------------------------------------

def strip_lean_comments(text: str) -> str:
    """Remove -- line comments and nested /- -/ block comments, keeping newlines."""
    out = []
    i, depth = 0, 0
    while i < len(text):
        if depth == 0 and text.startswith("--", i):
            newline = text.find("\n", i)
            if newline == -1:
                break
            out.append("\n")
            i = newline + 1
        elif text.startswith("/-", i):
            depth += 1
            i += 2
        elif depth and text.startswith("-/", i):
            depth -= 1
            i += 2
        elif depth == 0:
            out.append(text[i])
            i += 1
        else:
            if text[i] == "\n":
                out.append("\n")
            i += 1
    return "".join(out)


def lint_lean_sources(root: Path) -> None:
    """Reject proof-weakening constructs in every committed Lean file."""
    lean_root = root / "verification/lean"
    findings = []
    for file in sorted(lean_root.rglob("*.lean")):
        if ".lake" in file.parts:
            continue
        stripped = strip_lean_comments(file.read_text(errors="replace"))
        for pattern, label in FORBIDDEN_LEAN:
            for match in pattern.finditer(stripped):
                line = stripped.count("\n", 0, match.start()) + 1
                findings.append(f"{file.relative_to(lean_root)}:{line}: {label}")
    if findings:
        raise RuntimeError("Forbidden constructs in committed Lean sources:\n" + "\n".join("- " + f for f in findings))


# --- paths and modules -----------------------------------------------------

def generated_rel(target: dict) -> Path:
    """Committed generated.lean path, mirroring the production source path."""
    source = Path(target["source"])
    if source.name == "mod.rs":
        mirror = source.parent
    elif source.name == "lib.rs":
        mirror = source.parent / "lib"
    else:
        mirror = source.with_suffix("")
    return Path("verification/lean") / mirror / "generated.lean"


def rust_path(target: dict) -> str:
    """Display path such as mudu::utils::buf, derived from the source path."""
    parts = Path(target["source"]).parts  # crates/<group>/<crate>/src/<module...>.rs
    modules = [Path(name).stem for name in parts[4:]]
    if modules and modules[-1] in ("lib", "mod"):
        modules.pop()
    return "::".join([parts[2], *modules])


def spec_module(target: dict) -> str:
    module = target["module"]
    if not module.endswith(".proof"):
        raise RuntimeError(f"Proof module must end with .proof: {module}")
    return module[: -len("proof")] + "spec"


def proof_error_pattern(target: dict) -> re.Pattern:
    rel = (generated_rel(target).parent / "proof.lean").relative_to("verification/lean")
    return re.compile("error: " + re.escape(rel.as_posix()) + ":")


def audited_names(target: dict) -> list[str]:
    return list(target.get("theorems", [])) + list(target.get("sanity", []))


def expected_roots() -> set[str]:
    """Every registered target contributes generated/spec/proof modules."""
    roots = set()
    for target in TARGETS.values():
        proof = target["module"]
        if not proof.endswith(".proof"):
            raise RuntimeError(f"Proof module must end with .proof: {proof}")
        base = proof[: -len(".proof")]
        roots |= {base + ".generated", base + ".spec", proof}
    return roots


def lakefile_roots(root: Path) -> set[str]:
    lakefile = tomllib.loads((root / "verification/lean/lakefile.toml").read_text())
    return {name for lib in lakefile.get("lean_lib", []) for name in lib.get("roots", [])}


def check_roots_registered(root: Path) -> None:
    """lake build skips proofs whose module is not a root; fail with a clear error."""
    missing = expected_roots() - lakefile_roots(root)
    if missing:
        raise RuntimeError("lakefile.toml roots miss registered modules; "
                           "lake build would skip their proofs: " + ", ".join(sorted(missing)))


# --- extraction --------------------------------------------------------------

def extract(root: Path, output: Path, name: str, target: dict) -> str:
    output.mkdir(parents=True, exist_ok=True)
    # Aeneas names its output .lean after the LLBC file; keep the crate name.
    package = tomllib.loads((root / target["entry"]).read_text())["package"]["name"]
    expected = "".join(part.capitalize() for part in re.split(r"[-_]", package)) + ".lean"
    llbc = output / f"{package}.llbc"
    module = target["extract_module"]
    argv = [str(tc.AENEAS / "charon"), "cargo", "--preset=aeneas", "--sysroot", "default"]
    for fun in target["functions"]:
        argv.extend(["--start-from", f"crate::{module}::{fun}"])
    argv.extend(["--dest-file", str(llbc), "--", "--manifest-path", target["entry"],
                 "--locked", "--offline", "--target", tc.CONFIG["platform"]])
    with stage(f"[{name}] Rust → LLBC: {len(target['functions'])} production functions"):
        tc.run(argv, cwd=root, log="extract-charon")
    with stage(f"[{name}] LLBC → Lean translation"):
        tc.run([str(tc.AENEAS / "aeneas"), "-backend", "lean", "-namespace", module,
                "-use-lean-modules", "false", "-no-progress-bar", "-warnings-as-errors",
                "-abort-on-error", "-dest", str(output), str(llbc)], cwd=root, log="extract-aeneas")
        files = list(output.glob("*.lean"))
        if len(files) != 1 or files[0].name != expected:
            raise RuntimeError(f"Unexpected extraction output for {name}: {files}")
        text = files[0].read_text()
    return "-- Generated by script/verification/check.py using Aeneas. Do not edit manually.\n" + text


def compare(expected: str, actual: str) -> None:
    if expected != actual:
        diff = "".join(difflib.unified_diff(expected.splitlines(True), actual.splitlines(True),
                                          fromfile="committed", tofile="fresh extraction"))
        raise RuntimeError("Generated Lean is stale. Run cargo make verify-extract and review the diff.\n" + diff)


def drift_check(root: Path, name: str, target: dict, fresh: str) -> None:
    with stage(f"[{name}] Generated Lean matches production source"):
        try:
            compare((root / generated_rel(target)).read_text(), fresh)
        except RuntimeError:
            if tc.REPORT:
                saved = tc.REPORT.directory / f"fresh-generated-{name}.lean"
                saved.write_text(fresh)
                tc.REPORT.emit("Fresh Lean", str(saved))
            raise


# --- build and audit -----------------------------------------------------------

def audit_output(text: str, targets: dict) -> None:
    for target in targets.values():
        for theorem in audited_names(target):
            found = re.search(re.escape("'" + theorem + "'") + r" depends on axioms:\s*\[([^\]]*)\]", text)
            if not found:
                if f"'{theorem}' does not depend on any axioms" in text:
                    continue
                raise RuntimeError(f"Missing axiom audit output for {theorem}")
            names = {x.strip() for x in found[1].split(",") if x.strip()}
            unexpected = names - set(target["allowed_axioms"])
            if unexpected:
                raise RuntimeError(f"Unapproved axioms for {theorem}: {sorted(unexpected)}")


def build(root: Path, targets: dict) -> None:
    project = root / "verification/lean"
    with stage("Lean proof build"):
        tc.run([str(tc.LEAN / "bin/lake"), "build"], cwd=project, log="lean-build")
    total = sum(len(audited_names(target)) for target in targets.values())
    with stage(f"Axiom audit: {total} registered theorems and sanity checks"):
        audit_file = project / ".lake" / "axiom_audit.lean"
        lines = [f"import {target['module']}" for target in targets.values()]
        lines += [f"#print axioms {theorem}" for target in targets.values()
                  for theorem in audited_names(target)]
        audit_file.write_text("\n".join(lines) + "\n")
        log = tc.run([str(tc.LEAN / "bin/lake"), "env", "lean", str(audit_file)], cwd=project, log="axiom-audit")
        audit_output(log.read_text(), targets)
        if tc.REPORT:
            approved = sorted({axiom for target in targets.values() for axiom in target["allowed_axioms"]})
            tc.REPORT.emit("Axioms", "PASS: all registered theorems use only approved axioms: "
                           + ", ".join(approved), kind="pass")


def check(targets: dict) -> None:
    with tempfile.TemporaryDirectory(prefix="extract-", dir=tc.CACHE) as temp:
        for name, target in targets.items():
            fresh = extract(tc.ROOT, Path(temp) / name, name, target)
            drift_check(tc.ROOT, name, target, fresh)
    build(tc.ROOT, targets)
    if tc.REPORT:
        tc.REPORT.verified_functions = sum(len(target["functions"]) for target in targets.values())
        tc.REPORT.verified_theorems = sum(len(audited_names(target)) for target in targets.values())
    print("PASS: production source extraction, Lean proofs and axiom audit", flush=True)


def drift(targets: dict) -> None:
    with tempfile.TemporaryDirectory(prefix="extract-", dir=tc.CACHE) as temp:
        for name, target in targets.items():
            fresh = extract(tc.ROOT, Path(temp) / name, name, target)
            drift_check(tc.ROOT, name, target, fresh)
    print("PASS: generated Lean matches every selected production source (proofs not checked)", flush=True)


def determinism(targets: dict) -> None:
    with tempfile.TemporaryDirectory(prefix="determinism-", dir=tc.CACHE) as temp:
        for name, target in targets.items():
            first = extract(tc.ROOT, Path(temp) / f"{name}-a", name, target)
            second = extract(tc.ROOT, Path(temp) / f"{name}-b", name, target)
            with stage(f"[{name}] Extraction is byte-for-byte deterministic"):
                if first != second:
                    diff = "".join(difflib.unified_diff(first.splitlines(True), second.splitlines(True),
                                                        fromfile="first", tofile="second"))
                    raise RuntimeError(f"Extraction is not deterministic for {name}:\n{diff}")
    print("PASS: extraction is byte-for-byte deterministic for every selected target", flush=True)


# --- mutation self-test and sweep ---------------------------------------------

def mutations(targets: dict) -> list[tuple[str, dict, dict]]:
    """Each mutation is tested independently; a different failure cannot mask it."""
    cases = []
    for name, target in targets.items():
        cases.extend((name, target, variant) for variant in target.get("mutations", []))
    return cases


def self_test(targets: dict) -> None:
    check(targets)
    cases = mutations(targets)
    if not cases:
        raise RuntimeError("selected targets register no self-test mutation")
    for name, target, mutation in cases:
        self_test_mutation(name, target, mutation)
    if tc.REPORT:
        tc.REPORT.mutation_rejected = True
    print("PASS: every independent source mutation rejected by its production proof", flush=True)


def prepare_mutant_tree(root: Path, name: str, target: dict, mutation_from: str, mutation_to: str) -> None:
    """Isolated source copy with exactly one applied mutation; tools symlinked in."""
    # Extraction roots import all registered production modules, even
    # when Charon starts from only one target's entry points.
    for registered in TARGETS.values():
        source = root / registered["source"]
        source.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(tc.ROOT / registered["source"], source)
    source = root / target["source"]
    original = source.read_text()
    if original.count(mutation_from) != 1:
        raise RuntimeError(f"Mutation site for {name} changed; update targets.toml")
    source.write_text(original.replace(mutation_from, mutation_to))
    for entry in {item["entry"] for item in TARGETS.values()}:
        directory = Path(entry).parent
        shutil.copytree(tc.ROOT / directory, root / directory,
                        ignore=shutil.ignore_patterns("target"))
    project = root / "verification/lean"
    shutil.copytree(tc.ROOT / "verification/lean", project, ignore=shutil.ignore_patterns(".lake"))
    (root / "target/verification").mkdir(parents=True)
    (root / "target/verification/tools").symlink_to(tc.TOOLS, target_is_directory=True)
    (project / ".lake").mkdir()
    (project / ".lake/packages").symlink_to(tc.ROOT / "verification/lean/.lake/packages", target_is_directory=True)


def self_test_mutation(name: str, target: dict, mutation: dict) -> None:
    with tempfile.TemporaryDirectory(prefix="mutation-", dir=tc.CACHE) as temp:
        root = Path(temp)
        with stage(f"[{name}] Prepare isolated mutation: {mutation['mutation_note']}"):
            prepare_mutant_tree(root, name, target, mutation["mutation_from"], mutation["mutation_to"])
        fresh = extract(root, root / "extracted" / name, name, target)
        rel = generated_rel(target)
        with stage(f"[{name}] Mutation changes the extracted implementation"):
            if fresh == (tc.ROOT / rel).read_text():
                raise RuntimeError(f"Mutation was not detected by extraction for {name}")
            (root / rel).write_text(fresh)
            if tc.REPORT:
                snapshots = tc.REPORT.directory / "mutant-sources" / Path(temp).name
                for relative in [Path(target["source"]), rel,
                                 rel.with_name("proof.lean"), rel.with_name("spec.lean")]:
                    dest = snapshots / relative
                    dest.parent.mkdir(parents=True, exist_ok=True)
                    shutil.copy2(root / relative, dest)
                tc.REPORT.emit("Sources", f"Mutant diagnostic snapshots: {snapshots}")
        with stage(f"[{name}] Mutant implementation and specification compile"):
            tc.run([str(tc.LEAN / "bin/lake"), "build", "+" + spec_module(target)],
                   cwd=root / "verification/lean", log="mutant-compile")
        with stage(f"[{name}] Original proof rejects {mutation['mutation_note']}"):
            reject_mutant_proof(name, target, root / "verification/lean")


def reject_mutant_proof(name: str, target: dict, project: Path) -> None:
    """The original proof must fail on the mutant; anything else is unsound."""
    try:
        tc.run([str(tc.LEAN / "bin/lake"), "build", "+" + target["module"]],
               cwd=project, log="mutant-proof")
    except tc.reporting.CommandError as exc:
        if not proof_error_pattern(target).match(exc.diagnostic):
            raise
        if tc.REPORT:
            tc.REPORT.emit("Expected", "Proof rejected the mutant after its implementation compiled", kind="pass")
            tc.REPORT.emit("Diagnostic", exc.diagnostic)
        return
    raise RuntimeError(f"UNSOUND SELF-TEST: {name}: mutant passed the proof")


# --- mutation sweep -------------------------------------------------------------

def _operator_site_ok(code: str, start: int, old: str) -> bool:
    """Reject occurrences that are part of a longer operator or Rust syntax."""
    before = code[start - 1] if start else ""
    after = code[start + len(old)] if start + len(old) < len(code) else ""
    if old == "<":
        return after not in "=<" and before not in "<>"
    if old == "<=":
        return before not in "<>"
    if old == ">":
        return after not in "=>" and before not in "<>-="
    if old == ">=":
        return before not in "<>-="
    if old in ("==", "!="):
        return after != "=" and before not in "=!<>"
    if old == "&&":
        return after != "&" and before != "&"
    if old == "||":
        return after != "|" and before != "|"
    return True


def sweep_candidates(source_text: str, target: dict) -> list[dict]:
    """One single-site operator flip per occurrence; registered mutations skipped.

    Searches only the code prefix of each line (text before `//`), but mutates
    the original line so comments elsewhere survive. Limitation: a `//` inside a
    string literal is treated as a comment start.
    """
    lines = source_text.split("\n")
    registered_mutants = set()
    for mutation in target.get("mutations", []):
        if source_text.count(mutation["mutation_from"]) == 1:
            registered_mutants.add(source_text.replace(mutation["mutation_from"], mutation["mutation_to"]))
    candidates = []
    for lineno, line in enumerate(lines, start=1):
        code = line.split("//", 1)[0]
        for start in range(len(code)):
            for old, new in SWEEP_FLIPS:
                if not code.startswith(old, start) or not _operator_site_ok(code, start, old):
                    continue
                mutated_line = line[:start] + new + line[start + len(old):]
                mutated = "\n".join(lines[:lineno - 1] + [mutated_line] + lines[lineno:])
                if mutated in registered_mutants:
                    break
                candidates.append({"line": lineno, "mutation_from": old, "mutation_to": new,
                                   "note": f"line {lineno}: `{old}` -> `{new}`", "_mutated": mutated})
                break
    return candidates


def sweep_run(name: str, target: dict, budget: int | None) -> dict:
    source_text = (tc.ROOT / target["source"]).read_text()
    candidates = sweep_candidates(source_text, target)
    if budget is not None:
        candidates = candidates[:budget]
    results = {"target": name, "candidates": len(candidates), "proof-killed": [],
               "compile-killed": [], "silent": [], "survived": []}
    for candidate in candidates:
        outcome = sweep_one(name, target, candidate)
        results[outcome].append(candidate["note"])
    return results


def sweep_one(name: str, target: dict, candidate: dict) -> str:
    note = candidate["note"]
    with tempfile.TemporaryDirectory(prefix="sweep-", dir=tc.CACHE) as temp:
        root = Path(temp)
        with stage(f"[{name}] Sweep mutant {note}"):
            for registered in TARGETS.values():
                source = root / registered["source"]
                source.parent.mkdir(parents=True, exist_ok=True)
                if registered is target:
                    source.write_text(candidate["_mutated"])
                else:
                    shutil.copy2(tc.ROOT / registered["source"], source)
            for entry in {item["entry"] for item in TARGETS.values()}:
                directory = Path(entry).parent
                shutil.copytree(tc.ROOT / directory, root / directory,
                                ignore=shutil.ignore_patterns("target"))
            project = root / "verification/lean"
            shutil.copytree(tc.ROOT / "verification/lean", project, ignore=shutil.ignore_patterns(".lake"))
            (root / "target/verification").mkdir(parents=True)
            (root / "target/verification/tools").symlink_to(tc.TOOLS, target_is_directory=True)
            (project / ".lake").mkdir()
            (project / ".lake/packages").symlink_to(tc.ROOT / "verification/lean/.lake/packages", target_is_directory=True)
        try:
            fresh = extract(root, root / "extracted", name, target)
        except tc.reporting.CommandError:
            return "compile-killed"
        rel = generated_rel(target)
        if fresh == (tc.ROOT / rel).read_text():
            return "silent"
        (root / rel).write_text(fresh)
        try:
            tc.run([str(tc.LEAN / "bin/lake"), "build", "+" + spec_module(target)],
                   cwd=project, log="sweep-compile")
        except tc.reporting.CommandError:
            return "compile-killed"
        try:
            tc.run([str(tc.LEAN / "bin/lake"), "build", "+" + target["module"]],
                   cwd=project, log="sweep-proof")
        except tc.reporting.CommandError as exc:
            if proof_error_pattern(target).match(exc.diagnostic):
                return "proof-killed"
            raise
        return "survived"


def mutation_sweep(targets: dict, budget: int | None, list_only: bool) -> None:
    all_results = []
    for name, target in targets.items():
        if list_only:
            candidates = sweep_candidates((tc.ROOT / target["source"]).read_text(), target)
            for candidate in candidates:
                print(f"[{name}] candidate: {candidate['note']}", flush=True)
            all_results.append({"target": name, "candidates": len(candidates)})
            continue
        results = sweep_run(name, target, budget)
        all_results.append(results)
        killed = len(results["proof-killed"]) + len(results["compile-killed"])
        scored = results["candidates"] - len(results["silent"])
        rate = f"{killed}/{scored}" if scored else "no scored mutants"
        print(f"[{name}] kill rate {rate}; survived: {results['survived'] or 'none'}; "
              f"silent (extraction unchanged): {len(results['silent'])}", flush=True)
    outdir = tc.CACHE / "report"
    outdir.mkdir(parents=True, exist_ok=True)
    report_path = outdir / "mutation-sweep.json"
    report_path.write_text(json.dumps(all_results, indent=2) + "\n")
    print(f"Sweep report: {report_path}", flush=True)


# --- report ---------------------------------------------------------------------

TRUST_BOUNDARY = (
    "Proofs target the pinned Linux x86_64 extraction configuration. The Lean kernel, "
    "the Rust compiler, the Charon/Aeneas translation, and the Aeneas standard library "
    "models are trusted, not verified. Memory allocation failure, the operating system, "
    "concurrency, persistence, and disk failures are outside the proof boundary; Rust "
    "tests, fuzzing, and the internal deterministic simulation remain required."
)


def report(targets: dict) -> None:
    validate_registry(tc.ROOT, targets)
    outdir = tc.CACHE / "report"
    outdir.mkdir(parents=True, exist_ok=True)
    lines = ["# Verification Report", "",
             f"- Aeneas `{tc.CONFIG['aeneas_version']}`, Lean `{tc.CONFIG['lean_version']}`, "
             f"Rust `{tc.CONFIG['rust']}`, target `{tc.CONFIG['platform']}`",
             f"- Targets: {len(targets)}; functions: {sum(len(t['functions']) for t in targets.values())}; "
             f"audited theorems: {sum(len(t['theorems']) for t in targets.values())}; "
             f"sanity checks: {sum(len(t.get('sanity', [])) for t in targets.values())}; "
             f"registered self-test mutations: {len(mutations(targets))}", ""]
    lines += ["## Trust boundary", "", TRUST_BOUNDARY, ""]
    lines += ["## Targets", "", "| Target | Status | Owner | Functions | Theorems | Sanity | Mutations |",
              "| --- | --- | --- | --- | --- | --- | --- |"]
    for name, target in targets.items():
        lines.append(f"| `{name}` | {target['status']} | {target['owner']} | "
                     f"{len(target['functions'])} | {len(target['theorems'])} | "
                     f"{len(target.get('sanity', []))} | {len(target.get('mutations', []))} |")
    lines.append("")
    for name, target in targets.items():
        lines += [f"## `{name}` — {rust_path(target)}", ""]
        if target["assumptions"]:
            lines.append("Assumptions (trusted, not machine-checked):")
            lines += [f"- {item}" for item in target["assumptions"]]
        else:
            lines.append("Assumptions: none beyond the standard trust boundary.")
        lines.append("")
        premises = target.get("premises", [])
        if premises:
            lines.append("Premises discharged by the integration layer:")
            for premise in premises:
                refs = ", ".join(f"`{ref}`" for ref in premise["established_by"])
                lines.append(f"- {premise['note']} — established by {refs}")
        else:
            lines.append("Premises: none registered.")
        lines.append("")
    md = outdir / "verification-report.md"
    md.write_text("\n".join(lines) + "\n")
    data = {name: {"status": t["status"], "owner": t["owner"], "functions": t["functions"],
                   "theorems": t["theorems"], "sanity": t.get("sanity", []),
                   "assumptions": t["assumptions"],
                   "premises": [{"note": p["note"], "established_by": p["established_by"]}
                                for p in t.get("premises", [])],
                   "mutations": len(t.get("mutations", []))} for name, t in targets.items()}
    (outdir / "verification-report.json").write_text(json.dumps(data, indent=2) + "\n")
    print(f"PASS: report written to {md}", flush=True)


# --- new-target scaffold ------------------------------------------------------------

NEW_TARGET_NEXT_STEPS = """Next steps for the scaffolded target:
  1. Write the contract in the generated spec.lean (see the authoring guide in proof.lean).
  2. Prove it; register theorem names plus at least one sanity theorem in targets.toml.
  3. Add premises (integration tests discharging preconditions) and mutations.
  4. Flip status to "proven" once cargo make verify is green.
"""


def new_target(args) -> None:
    """Scaffold a target, extract it, and compile the skeleton spec/proof."""
    ok, detail = tc.probe()
    if not ok:
        raise RuntimeError(f"formal verification toolchain not installed ({detail}); "
                           "run cargo make doctor -- install --only formal-verification")
    functions = [f.strip() for f in (args.functions or "").split(",") if f.strip()]
    writes = scaffold.plan(tc.ROOT, name=args.name, source=args.source, owner=args.owner,
                           functions=functions, module_base=args.module)
    if args.dry_run:
        print(scaffold.render_plan(writes), flush=True)
        return
    scaffold.apply(tc.ROOT, writes)
    print(f"Scaffold applied: {args.name} (status=extracted)", flush=True)
    target = tomllib.loads((tc.ROOT / "verification/targets.toml").read_text())["target"][args.name]
    with stage(f"[{args.name}] Extract newly scaffolded target"):
        fresh = extract(tc.ROOT, tc.CACHE / "new-target" / args.name, args.name, target)
        (tc.ROOT / generated_rel(target)).write_text(fresh)
    with stage(f"[{args.name}] Lake build skeleton spec/proof"):
        tc.run([str(tc.LEAN / "bin/lake"), "build", "+" + target["module"]],
               cwd=tc.ROOT / "verification/lean", log="new-target-lean")
    print(NEW_TARGET_NEXT_STEPS, flush=True)


# --- premise freshness --------------------------------------------------------------

def premise_test_invocation(ref: str) -> tuple[Path, str, str | None, str]:
    """Map a `path::symbol` premise reference to (workspace dir, package, test target, symbol)."""
    path, _, symbol = ref.partition("::")
    if not symbol:
        raise RuntimeError(f"premise reference without a symbol (existence-only, cannot run): {ref}")
    parts = Path(path).parts
    if len(parts) < 5 or parts[0] != "crates":
        raise RuntimeError(f"premise reference outside crates/: {ref}")
    package = parts[2]
    test_target = None
    if parts[3] == "tests":
        test_target = Path(parts[4]).stem
    elif parts[3] != "src":
        raise RuntimeError(f"premise reference not under src/ or tests/: {ref}")
    return Path("crates") / parts[1], package, test_target, symbol


def premise_invocations(targets: dict) -> dict[tuple[Path, str, str | None], list[tuple[str, str]]]:
    """Group premise test invocations by (workspace dir, package, test target)."""
    invocations: dict[tuple[Path, str, str | None], list[tuple[str, str]]] = {}
    for name, target in targets.items():
        for premise in target.get("premises", []):
            for ref in premise.get("established_by", []):
                _, _, symbol = ref.partition("::")
                if not symbol:
                    continue
                cwd, package, test_target, sym = premise_test_invocation(ref)
                entry = (cwd, package, test_target)
                if (name, sym) not in invocations.setdefault(entry, []):
                    invocations[entry].append((name, sym))
    return invocations


def premise_skip_notes(targets: dict) -> list[str]:
    notes = []
    for name, target in targets.items():
        for premise in target.get("premises", []):
            for ref in premise.get("established_by", []):
                if "::" not in ref:
                    notes.append(f"{name}: {ref} (no symbol; existence-checked only)")
    return notes


def premises(targets: dict) -> None:
    """Run the tests cited as premise evidence; missing or failing references
    mean the trust-chain reference has rotted."""
    invocations = premise_invocations(targets)
    for note in premise_skip_notes(targets):
        print(f"NOTE: {note}", flush=True)
    failures: list[str] = []
    total_refs = sum(len(entries) for entries in invocations.values())
    with stage(f"Premise references: {total_refs} tests in {len(invocations)} cargo invocations"):
        for (cwd, package, test_target), entries in sorted(invocations.items()):
            argv = ["cargo", "test", "-p", package]
            if test_target:
                argv += ["--test", test_target]
            argv.append("--")
            argv += [sym for _, sym in entries]
            # Ordinary project tests run under the repository's default toolchain,
            # not the pinned extraction toolchain that tc.run forces.
            print(f"[premises] {' '.join(argv)} (in {cwd})", flush=True)
            result = subprocess.run(argv, cwd=tc.ROOT / cwd, capture_output=True, text=True)
            text = result.stdout + "\n" + result.stderr
            log_dir = tc.CACHE / "logs"
            log_dir.mkdir(parents=True, exist_ok=True)
            (log_dir / f"premises-{package}.log").write_text(text)
            if result.returncode:
                tail = "\n".join(text.splitlines()[-15:])
                raise RuntimeError(f"premise tests failed in {cwd} (package {package}):\n{tail}")
            for name, sym in entries:
                pattern = re.compile(r"(?m)^test (?:\S*::)?" + re.escape(sym) + r" \.\.\. ok$")
                if not pattern.search(text):
                    failures.append(f"{name}: {sym} did not pass in {cwd} (package {package})")
    if failures:
        raise RuntimeError("premise references rotted:\n" + "\n".join("- " + f for f in failures))
    print(f"PASS: {total_refs} premise references exist and pass", flush=True)


# --- spec weakening review (advisory) ------------------------------------------------

DECL_RE = re.compile(
    r"(?m)^(?:\s*@\[[^\]]*\][^\S\n]*)*\s*(?:private\s+)?(?:theorem|lemma|def)\s+([«»\w.']+)")


def declaration_signatures(text: str) -> dict[str, str]:
    """Declaration name → normalized signature (header up to `:=`), comments stripped."""
    text = strip_lean_comments(text)
    decls: dict[str, str] = {}
    for match in DECL_RE.finditer(text):
        name = match.group(1)
        end = text.find(":=", match.end())
        if end == -1:
            continue
        decls[name] = " ".join(text[match.start():end].split())
    return decls


def git_show(ref: str, path: str) -> str | None:
    result = subprocess.run(["git", "-C", str(tc.ROOT), "show", f"{ref}:{path}"],
                            capture_output=True, text=True)
    return result.stdout if result.returncode == 0 else None


def spec_diff(ref: str) -> None:
    """Declaration-level diff of spec/proof Lean files against a base ref.

    Weakening is not mechanically decidable; this focuses human review on the
    changed contracts. Always advisory (exit 0).
    """
    changed = git_changed(f"{ref}...HEAD")
    files = [f for f in changed if f.startswith("verification/lean/")
             and (f.endswith("spec.lean") or f.endswith("proof.lean"))]
    if not files:
        print("PASS: no spec/proof Lean files changed", flush=True)
        return
    lines = [f"# Spec/proof declaration diff against `{ref}`", ""]
    changed_any = False
    for file in files:
        current_text = (tc.ROOT / file).read_text() if (tc.ROOT / file).is_file() else ""
        base_text = git_show(ref, file) or ""
        current = declaration_signatures(current_text)
        base = declaration_signatures(base_text)
        added = sorted(set(current) - set(base))
        removed = sorted(set(base) - set(current))
        changed_names = sorted(n for n in set(current) & set(base) if current[n] != base[n])
        if not added and not removed and not changed_names:
            continue
        changed_any = True
        lines += [f"## `{file}`", ""]
        for n in added:
            lines += [f"- **added** `{n}`: `{current[n]}`"]
        for n in removed:
            lines += [f"- **removed** `{n}`: was `{base[n]}`"]
        for n in changed_names:
            lines += [f"- **changed** `{n}`:", f"  - base: `{base[n]}`", f"  - head: `{current[n]}`"]
        lines.append("")
    outdir = tc.CACHE / "report"
    outdir.mkdir(parents=True, exist_ok=True)
    artifact = outdir / "spec-diff.md"
    artifact.write_text("\n".join(lines) + "\n")
    if changed_any:
        print(f"::warning::spec/proof declarations changed against {ref}; review "
              f"whether postconditions weakened — {artifact}", flush=True)
        print("\n".join(lines), flush=True)
    else:
        print(f"PASS: spec/proof declarations unchanged against {ref}", flush=True)


# --- target selection -------------------------------------------------------------

def git_changed(*args: str) -> list[str]:
    result = subprocess.run(["git", "-C", str(tc.ROOT), "diff", "--name-only", *args],
                            capture_output=True, text=True)
    if result.returncode:
        raise RuntimeError(f"git diff failed: {result.stderr.strip()}")
    return [line.strip() for line in result.stdout.splitlines() if line.strip()]


def collect_changed(args) -> list[str] | None:
    """None: no filtering. Empty list: explicitly nothing affected."""
    changed: list[str] | None = None

    def extend(paths):
        nonlocal changed
        changed = (changed or []) + list(paths)

    if args.affected_by is not None:
        extend(args.affected_by)
    if args.affected_by_file:
        extend(Path(args.affected_by_file).read_text().splitlines())
    if args.staged:
        extend(git_changed("--cached"))
    if args.diff:
        extend(git_changed(f"{args.diff}...HEAD"))
    return [line for line in changed if line] if changed is not None else None


def select_targets(args) -> dict:
    targets = TARGETS
    if args.targets:
        names = [name.strip() for name in args.targets.split(",") if name.strip()]
        unknown = set(names) - set(TARGETS)
        if unknown:
            raise RuntimeError(f"unknown targets: {sorted(unknown)}; registered: {sorted(TARGETS)}")
        targets = {name: TARGETS[name] for name in names}
    changed = collect_changed(args)
    if changed is not None:
        affected = impact.affected_targets(changed, TARGETS)
        targets = {name: target for name, target in targets.items() if name in affected}
    return targets


# --- main ---------------------------------------------------------------------------

def preflight() -> None:
    with stage("Pinned toolchain and prerequisite checks"):
        ok, detail = tc.probe()
        if not ok:
            raise RuntimeError(detail + "; run cargo make doctor -- install --only formal-verification")
    with stage("Registry: schema, premise references, mutation anchors"):
        validate_registry(tc.ROOT, TARGETS)
    with stage("Committed Lean sources free of sorry/admit/axiom/native_decide"):
        lint_lean_sources(tc.ROOT)
    with stage("Lake roots cover every registered module"):
        check_roots_registered(tc.ROOT)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("action", choices=["check", "extract", "self-test", "drift",
                                           "determinism", "mutation-sweep", "report",
                                           "new-target", "premises", "spec-diff"])
    parser.add_argument("--targets", help="comma-separated target names")
    parser.add_argument("--affected-by", nargs="*", help="changed files; empty means none affected")
    parser.add_argument("--affected-by-file", help="file with one changed path per line")
    parser.add_argument("--staged", action="store_true", help="use git staged files as the change set")
    parser.add_argument("--diff", metavar="REF", help="use git diff REF...HEAD as the change set")
    parser.add_argument("--skip-if-no-tools", action="store_true",
                        help="exit 0 with a message when the formal toolchain is not installed")
    parser.add_argument("--budget", type=int, default=None, help="mutation-sweep: max candidates per target")
    parser.add_argument("--list-only", action="store_true", help="mutation-sweep: only list candidates")
    parser.add_argument("--name", help="new-target: target name (registry key)")
    parser.add_argument("--source", help="new-target: production source path")
    parser.add_argument("--owner", help="new-target: owning area")
    parser.add_argument("--functions", help="new-target: comma-separated extraction entry points")
    parser.add_argument("--module", help="new-target: Lean proof module (default: mirror the source path)")
    parser.add_argument("--dry-run", action="store_true", help="new-target: print the planned writes only")
    args = parser.parse_args(normalized_argv())
    try:
        targets = select_targets(args)
    except RuntimeError as exc:
        print(str(exc), flush=True)
        return 1
    if args.action == "report":
        try:
            report(targets)
            return 0
        except RuntimeError as exc:
            print(str(exc), flush=True)
            return 1
    if args.action == "new-target":
        try:
            for required in ("name", "source", "owner", "functions"):
                if not getattr(args, required):
                    raise RuntimeError(f"new-target requires --{required}")
            new_target(args)
            return 0
        except RuntimeError as exc:
            print(str(exc), flush=True)
            return 1
    if args.action == "premises":
        try:
            if not targets:
                print("PASS: no verification targets affected by the selected changes", flush=True)
                return 0
            premises(targets)
            return 0
        except RuntimeError as exc:
            print(str(exc), flush=True)
            return 1
    if args.action == "spec-diff":
        if not args.diff:
            print("spec-diff requires --diff REF", flush=True)
            return 1
        spec_diff(args.diff)
        return 0
    if args.skip_if_no_tools:
        ok, detail = tc.probe()
        if not ok:
            print(f"SKIP: formal verification toolchain not installed ({detail}); "
                  "run cargo make doctor -- install --only formal-verification", flush=True)
            return 0
    if not targets:
        print("PASS: no verification targets affected by the selected changes", flush=True)
        return 0
    mutation_count = len(mutations(targets))
    n = len(targets)
    totals = {"check": 3 * n + 6, "extract": 3 * n + 4, "drift": 3 * n + 4,
              "determinism": 5 * n + 4, "self-test": 3 * n + 6 * mutation_count + 6,
              "mutation-sweep": 4}
    if args.action == "mutation-sweep" and not args.list_only:
        planned = 0
        for target in targets.values():
            candidates = sweep_candidates((tc.ROOT / target["source"]).read_text(), target)
            planned += min(len(candidates), args.budget) if args.budget is not None else len(candidates)
        totals["mutation-sweep"] = 3 * planned + 4
    report_obj = tc.reporting.RunReport(
        tc.CACHE, args.action, totals.get(args.action, 3 * n + 4),
        ", ".join(rust_path(target) for target in targets.values()),
        {"Aeneas": tc.CONFIG["aeneas_version"], "Lean": tc.CONFIG["lean_version"],
         "Rust": tc.CONFIG["rust"], "Target": tc.CONFIG["platform"]},
    )
    tc.REPORT = report_obj
    success, failure = False, None
    try:
        preflight()
        if args.action == "extract":
            with tempfile.TemporaryDirectory(prefix="extract-", dir=tc.CACHE) as temp:
                for name, target in targets.items():
                    fresh = extract(tc.ROOT, Path(temp) / name, name, target)
                    with stage(f"[{name}] Update generated Lean (proofs not checked)"):
                        (tc.ROOT / generated_rel(target)).write_text(fresh)
        elif args.action == "self-test":
            self_test(targets)
        elif args.action == "drift":
            drift(targets)
        elif args.action == "determinism":
            determinism(targets)
        elif args.action == "mutation-sweep":
            mutation_sweep(targets, args.budget, args.list_only)
        else:
            check(targets)
        success = True
        return 0
    except (OSError, RuntimeError) as exc:
        failure = str(exc)
        return 1
    except KeyboardInterrupt:
        failure = "Interrupted by user; completed stage logs are preserved."
        return 130
    finally:
        report_obj.finish(success, failure)
        tc.REPORT = None


if __name__ == "__main__":
    raise SystemExit(main())
