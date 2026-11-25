"""Scaffolding for registering a new formal-verification target.

`check.py new-target` uses this module to generate and apply the four writes a
new target needs: the extraction `#[path]` module line, the `targets.toml`
registry entry, the `spec.lean`/`proof.lean` skeletons, and the lakefile roots.
The generators are pure functions so the unit tests can pin them without
touching the repository.
"""
from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re

EXTRACT_ENTRY = "verification/extract/Cargo.toml"
EXTRACT_LIB = "verification/extract/lib.rs"
LAKEFILE = "verification/lean/lakefile.toml"
PLATFORM = "x86_64-unknown-linux-gnu"
ALLOWED_AXIOMS = ["propext", "Classical.choice", "Quot.sound"]

NAME_RE = re.compile(r"^[a-z][a-z0-9_]*$")

AUTHORING_GUIDE = """/- Authoring guide for the proof of this target (delete when the target
   flips to status = "proven"):

   Aeneas-friendly production shapes, learned from the codec proofs:
   - prefer arithmetic (`* 256 +`) over bitwise packing at proof boundaries;
     `>>` and `%` by constants stay val-level and are fine
   - if-chains over `match` on scalar literals (literal matches are painful
     to reduce in Lean)
   - unrolled fixed-width reads over `from_be_bytes` arrays
   - encoders as `&mut Vec<u8>` push/extend_from_slice; decoders as
     `&[u8] -> Result<(T, usize), Error>` returning consumed byte counts

   Proof patterns:
   - encode contract:
     `write_x out v ⦃ out' => out'.val = out.val ++ spec_enc v ⦄`
   - decode contract:
     `read_x input ⦃ r => ∃ n, r = .Ok (v, n) ∧ n.val = (spec_enc v).length ⦄`
   - rejection: `read_x input ⦃ r => r = .Err ... ⦄` for short/foreign inputs
   - step through the WP monad with `step` / `step with <lemma>`; capacity side
     conditions go to `scalar_tac` (derive length facts before the step)
   - slice indexing in specs uses `input.val[i]!`; convert step hypotheses with
     `getElem!_pos`; marker constants unfold with `simp only […MARKER_X]`
   - usize literals are opaque to the kernel on the platform type; `2 ^ n` is
     not a numeral for omega — `show` the numeral form first

   Loops and predicates, learned from the Raft quorum/vote proofs:
   - write Rust loops as `while i < s.len()` with an explicit index; range
     for-loops have no `Step` model in this Aeneas version
   - compare `Option<u64>` payloads by `match o { Some(n) => n == c, None => .. }`;
     `==` on the Option itself extracts as a Charon `axiom` and fails lint
   - prove loops with `loop.spec_decr_nat`; give measure/inv/post as
     *type-annotated* lambdas (`fun (x : Usize × Usize) => ...`), otherwise the
     implicit loop-state type stays unassigned and `apply` dies in isDefEq
   - `step` consumes pure `ok` binds together with the next real bind and the
     trailing `ok`; a loop proof may need `set_option maxHeartbeats 1000000 in`
   - `split` on `match o with ...` substitutes the scrutinee (no equation
     hypothesis); `split` on `if c` yields `h : c = true` / `¬ c = true`
   - full `simp` normalizes `l[i]!` to `l[i]?.getD _` and `a != b` to
     `¬ a = b`; either keep helper hypotheses in that normal form or
     `cases h : <bool-expr>` before simp so the condition becomes a literal
   - list-level counting lemmas: `List.take_add_one` (`take_succ` is
     deprecated), `List.filter_append`, `List.length_zip`, `List.getElem_zip`,
     `List.ne_nil_of_length_pos` + `List.exists_mem_of_ne_nil` for witnesses
   - `scalar_tac` can loop (maxRecDepth) once Boolean-equality hypotheses
     (`b = false`, `(x != y) = true`) mix with val-level arithmetic facts;
     discharge pure Nat arithmetic conjuncts with plain `omega` instead
   - Boolean predicate splitting: `bne_iff_ne`, `Bool.and_eq_true`

   Sanity template (one concrete instance per theorem class; sanity theorems
   keep the specification satisfiable on concrete inputs):

   /-- Sanity: ... -/
   theorem sanity_example :
       <the extracted function> <concrete args> ⦃ r => <expected> ⦄ := by
     have h := <the main theorem> <concrete args>
     apply WP.spec_mono h
     intro r hr
     -- cases r; rw [show (42#u64 : U64).val = 42 from rfl] at hr; omega/decide
-/
"""


@dataclass
class ScaffoldWrite:
    """One filesystem write planned by the scaffold."""
    kind: str  # "create" | "append" | "insert-roots"
    path: Path
    text: str
    roots: tuple[str, ...] = ()


def stem_of(source: str) -> str:
    return Path(source).stem


def module_base_from_source(source: str) -> str:
    """Mirror the production source path as the Lean module prefix.

    crates/<group>/<crate>/src/<mod...>.rs → crates.<group>.<crate>.src.<mod...>;
    mod.rs maps to its directory, lib.rs to <dir>.lib; hyphenated segments get
    Lean's «» quoting. Same mirroring as check.py's generated_rel.
    """
    parts = list(Path(source).parts)
    leaf = parts[-1]
    if leaf == "mod.rs":
        parts = parts[:-1]
    elif leaf == "lib.rs":
        parts = parts[:-1] + ["lib"]
    else:
        parts = parts[:-1] + [Path(leaf).stem]
    return ".".join(f"«{part}»" if "-" in part else part for part in parts)


def rust_path_from_source(source: str) -> str:
    parts = Path(source).parts
    modules = [Path(name).stem for name in parts[4:]]
    if modules and modules[-1] in ("lib", "mod"):
        modules.pop()
    return "::".join([parts[2], *modules])


def render_extract_line(source: str, stem: str) -> str:
    return f'#[path = "../../{source}"]\npub mod {stem};\n'


def render_registry_entry(name: str, source: str, owner: str, functions: list[str],
                          module_base: str) -> str:
    functions_toml = ", ".join(f'"{f}"' for f in functions)
    return f'''[target.{name}]
source = "{source}"
entry = "{EXTRACT_ENTRY}"
extract_module = "{stem_of(source)}"
platform = "{PLATFORM}"
features = []
functions = [{functions_toml}]
module = "{module_base}.proof"
status = "extracted"
allowed_axioms = ["propext", "Classical.choice", "Quot.sound"]
owner = "{owner}"
assumptions = []
theorems = []
sanity = []
# To reach status = "proven": write the contract in spec.lean, prove it in
# proof.lean, register the theorem names in `theorems` plus at least one
# `sanity` theorem, then add premises and mutations:
# [[target.{name}.premises]]
# note = "..."
# established_by = ["crates/.../test.rs::test_name"]
#
# [[target.{name}.mutations]]
# mutation_note = "..."
# mutation_from = "<exact production source line>"
# mutation_to = "<mutated line>"
'''


def render_spec_lean(name: str, module_base: str) -> str:
    stem = name
    return f'''import {module_base}.generated
open Aeneas Std Result

namespace {stem}_spec

/- Specification functions go here: state the contract as pure Lean
   definitions (canonical byte layouts, arithmetic postconditions), not as a
   restatement of the implementation. -/

end {stem}_spec
'''


def render_proof_lean(name: str, module_base: str) -> str:
    return f'''import {module_base}.spec
open Aeneas Std Result
open {name}_spec

namespace {name}_proof

{AUTHORING_GUIDE}
end {name}_proof
'''


def render_lakefile_roots(module_base: str) -> tuple[str, str, str]:
    return (f"{module_base}.generated", f"{module_base}.spec", f"{module_base}.proof")


def lean_dir_from_source(root: Path, source: str) -> Path:
    """The directory holding generated/spec/proof for the source; mirrors
    check.py's generated_rel (mod.rs maps to its directory, lib.rs to <dir>/lib)."""
    src = Path(source)
    if src.name == "mod.rs":
        mirror = src.parent
    elif src.name == "lib.rs":
        mirror = src.parent / "lib"
    else:
        mirror = src.with_suffix("")
    return root / "verification/lean" / mirror


def plan(root: Path, *, name: str, source: str, owner: str,
         functions: list[str], module_base: str | None) -> list[ScaffoldWrite]:
    """Validate and plan the four writes; raises RuntimeError on misuse."""
    if not NAME_RE.match(name):
        raise RuntimeError(f"target name must match {NAME_RE.pattern}: {name!r}")
    if not (root / source).is_file():
        raise RuntimeError(f"source not found: {source}")
    if not functions:
        raise RuntimeError("at least one function is required")
    if not owner.strip():
        raise RuntimeError("owner must be a non-empty string")
    if (root / "verification/targets.toml").read_text().count(f"[target.{name}]"):
        raise RuntimeError(f"target already registered: {name}")
    stem = stem_of(source)
    extract_lib = (root / EXTRACT_LIB).read_text()
    if f"pub mod {stem};" in extract_lib:
        raise RuntimeError(f"extraction module already present: {stem}")
    base = module_base or module_base_from_source(source)
    lean_dir = lean_dir_from_source(root, source)
    if (lean_dir / "generated.lean").exists() or (lean_dir / "spec.lean").exists():
        raise RuntimeError(f"Lean directory already populated: {lean_dir}")
    return [
        ScaffoldWrite("append", root / EXTRACT_LIB, render_extract_line(source, stem)),
        ScaffoldWrite("append", root / "verification/targets.toml",
                      "\n" + render_registry_entry(name, source, owner, functions, base)),
        ScaffoldWrite("create", lean_dir / "spec.lean", render_spec_lean(name, base)),
        ScaffoldWrite("create", lean_dir / "proof.lean", render_proof_lean(name, base)),
        ScaffoldWrite("insert-roots", root / LAKEFILE, "", render_lakefile_roots(base)),
    ]


def render_plan(writes: list[ScaffoldWrite]) -> str:
    lines = []
    for write in writes:
        lines.append(f"--- {write.kind}: {write.path}")
        if write.kind == "insert-roots":
            lines.extend(f"    root: {root}" for root in write.roots)
        else:
            lines.append(write.text)
    return "\n".join(lines)


def apply(root: Path, writes: list[ScaffoldWrite]) -> None:
    for write in writes:
        if write.kind == "create":
            write.path.parent.mkdir(parents=True, exist_ok=True)
            write.path.write_text(write.text)
        elif write.kind == "append":
            with write.path.open("a") as fh:
                fh.write(write.text)
        elif write.kind == "insert-roots":
            lakefile = write.path.read_text()
            marker = "]"
            index = lakefile.rfind(marker)
            if index == -1:
                raise RuntimeError(f"{write.path}: no closing bracket found")
            insertion = "".join(f', "{root_}"' for root_ in write.roots)
            write.path.write_text(lakefile[:index] + insertion + lakefile[index:])
        else:
            raise RuntimeError(f"unknown write kind: {write.kind}")
