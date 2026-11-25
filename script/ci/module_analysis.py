#!/usr/bin/env python3
"""Run `mudud-arch` across every library crate in the workspace groups.

The default `recommend` mode aggregates every crate's findings into a single
prioritised remediation list (cycles, god modules, coupling, cohesion,
unstable dependencies, deep nesting). Per-crate output is available via the
other modes:

  recommend        aggregated remediation list (default)
  report           per-crate dependency graph report
  pub-visibility   per-crate unnecessary `pub` suggestions
  orphans          per-crate orphaned source files
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]

# Vendored dagre (Sugiyama layout with edge-crossing reduction) embedded into
# every generated HTML page.
_DAGRE_PATH = Path(__file__).resolve().parent / "vendor" / "dagre.min.js"
_DAGRE = _DAGRE_PATH.read_text(encoding="utf-8") if _DAGRE_PATH.exists() else ""

WORKSPACES = [
    "crates/common",
    "crates/db-kernel",
    "crates/sdk",
    "crates/tools",
    "example/mpk",
]

# Thresholds for the aggregated recommendation report.
GOD_MODULE_ITEMS = 50
COUPLING_DISTANCE = 0.5
LOW_COHESION_ITEMS = 10
LOW_COHESION_DENSITY = 0.02


def _paint(code: str, text: str) -> str:
    """Wraps `text` in an ANSI colour code when stdout is a TTY."""
    if sys.stdout.isatty():
        return f"\033[{code}m{text}\033[0m"
    return text


def red(text: str) -> str:
    return _paint("1;31", text)


def orange(text: str) -> str:
    return _paint("1;33", text)


def yellow(text: str) -> str:
    return _paint("33", text)


def gray(text: str) -> str:
    return _paint("90", text)


def bold(text: str) -> str:
    return _paint("1", text)


def cyan(text: str) -> str:
    return _paint("1;36", text)


def hyperlink(url: str, text: str) -> str:
    """Wraps `text` in an OSC 8 hyperlink when stdout is a TTY (clickable in
    modern terminals)."""
    if sys.stdout.isatty():
        return f"\033]8;;{url}\033\\{text}\033]8;;\033\\"
    return text


def discover_lib_crates() -> list[str]:
    """Returns manifest paths of every workspace crate with a library target."""
    manifests: set[str] = set()
    for workspace in WORKSPACES:
        ws_dir = ROOT / workspace
        if not ws_dir.exists():
            continue
        result = subprocess.run(
            ["cargo", "metadata", "--no-deps", "--format-version", "1"],
            cwd=ws_dir,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            print(f"warning: cargo metadata failed for {workspace}", file=sys.stderr)
            continue
        metadata = json.loads(result.stdout)
        for package in metadata.get("packages", []):
            if any(
                "lib" in target.get("kind", []) for target in package.get("targets", [])
            ):
                manifests.add(package["manifest_path"])
    return sorted(manifests)


def discover_crate_deps() -> tuple[list[str], list[list[str]]]:
    """Returns (crate_names, edges) for workspace-internal library crate
    dependencies, using each crate's library target name."""
    crate_names: set[str] = set()
    name_to_lib: dict[str, str] = {}
    deps_by_name: dict[str, list[str]] = {}
    for workspace in WORKSPACES:
        ws_dir = ROOT / workspace
        if not ws_dir.exists():
            continue
        result = subprocess.run(
            ["cargo", "metadata", "--format-version", "1"],
            cwd=ws_dir,
            capture_output=True,
            text=True,
        )
        if result.returncode != 0:
            continue
        metadata = json.loads(result.stdout)
        member_ids = set(metadata.get("workspace_members", []))
        for package in metadata.get("packages", []):
            if package.get("id") not in member_ids:
                continue
            lib = next(
                (t for t in package.get("targets", []) if "lib" in t.get("kind", [])),
                None,
            )
            if lib is not None:
                crate_names.add(lib["name"])
                name_to_lib[package["name"]] = lib["name"]
                deps_by_name[package["name"]] = [
                    d["name"]
                    for d in package.get("dependencies", [])
                    if "dev" not in (d.get("kind") or "normal")
                ]

    edges: set[tuple[str, str]] = set()
    for pkg_name, lib_name in name_to_lib.items():
        for dep_name in deps_by_name.get(pkg_name, []):
            to_lib = name_to_lib.get(dep_name)
            if to_lib is not None and to_lib != lib_name:
                edges.add((lib_name, to_lib))

    return sorted(crate_names), sorted(edges)


def analyze_crate(binary: str, manifest: str) -> dict | None:
    """Runs `report --json` for one crate and returns the parsed report."""
    result = subprocess.run(
        [binary, "report", "--manifest", manifest, "--json"],
        capture_output=True,
        text=True,
    )
    if result.returncode != 0:
        return None
    try:
        return json.loads(result.stdout)
    except json.JSONDecodeError:
        return None


def extract_findings(reports: list[tuple[str, dict]]) -> dict:
    """Extracts aggregated findings from every crate's JSON report."""
    cycles: list[tuple[str, list[str]]] = []
    god_modules: list[tuple[str, str, int, int]] = []
    coupling: list[tuple[str, str, float, float, float]] = []
    cohesion: list[tuple[str, str, float, int]] = []
    unstable: list[tuple[str, str, str]] = []
    deep: list[tuple[str, str, int]] = []
    pub_surface: list[tuple[str, str, int]] = []
    danger: list[tuple[str, str, int, int, int]] = []

    for _manifest, report in reports:
        crate = report["crate"]
        for component in report.get("cycles", []):
            if len(component) > 1:
                cycles.append((crate, component))
        for module in report.get("largest_modules", []):
            if module["items"] >= GOD_MODULE_ITEMS:
                god_modules.append(
                    (crate, module["module"], module["items"], module["afferent"])
                )
        for metric in report.get("coupling", []):
            if metric["distance"] > COUPLING_DISTANCE:
                coupling.append(
                    (
                        crate,
                        metric["module"],
                        metric["distance"],
                        metric["abstractness"],
                        metric["instability"],
                    )
                )
        for metric in report.get("cohesion", []):
            if metric["items"] >= LOW_COHESION_ITEMS and metric["density"] < LOW_COHESION_DENSITY:
                cohesion.append(
                    (crate, metric["module"], metric["density"], metric["items"])
                )
        for edge in report.get("unstable_dependencies", []):
            unstable.append((crate, edge["from"], edge["to"]))
        for entry in report.get("deep_nesting", []):
            deep.append((crate, entry["module"], entry["depth"]))
        for entry in report.get("pub_surface", []):
            if entry["count"] >= 15:
                pub_surface.append((crate, entry["module"], entry["count"]))
        for entry in report.get("dangerous_calls", []):
            danger.append(
                (
                    crate,
                    entry["module"],
                    entry["panics"],
                    entry["unwraps"],
                    entry["expects"],
                )
            )

    return {
        "cycles": cycles,
        "god_modules": god_modules,
        "coupling": coupling,
        "cohesion": cohesion,
        "unstable": unstable,
        "deep": deep,
        "pub_surface": pub_surface,
        "danger": danger,
    }


def summary_text(findings: dict, manifest_count: int, color: bool = True) -> str:
    """Builds the prioritised remediation list as text."""

    def paint(fn, text: str) -> str:
        return fn(text) if color else text

    cycles = findings["cycles"]
    god_modules = findings["god_modules"]
    coupling = findings["coupling"]
    cohesion = findings["cohesion"]
    unstable = findings["unstable"]
    deep = findings["deep"]
    pub_surface = findings["pub_surface"]
    danger = findings["danger"]

    total = (
        len(cycles)
        + len(god_modules)
        + len(coupling)
        + len(cohesion)
        + len(unstable)
        + len(deep)
    )

    lines: list[str] = []
    lines.append(paint(cyan, paint(bold, "# Module Coupling Analysis · Summary Remediation List")))
    lines.append(f"Analysed {manifest_count} crates, found {paint(bold, str(total))} findings.")
    lines.append("")

    lines.append(paint(red, f"## P0 · Cyclic dependencies ({len(cycles)}) — break into acyclic edges"))
    for crate, component in sorted(cycles, key=lambda c: -len(c[1])):
        lines.append(f"  [{paint(bold, crate)}] {len(component)} modules: {', '.join(component)}")
    if not cycles:
        lines.append("  none")
    lines.append("")

    lines.append(paint(orange, f"## P1 · God modules ({len(god_modules)}) — split by responsibility"))
    for crate, module, items, afferent in sorted(god_modules, key=lambda g: -g[2])[:15]:
        lines.append(f"  [{paint(bold, crate)}] {module} ({items} items, {afferent} depended-on)")
    if not god_modules:
        lines.append("  none")
    lines.append("")

    lines.append(paint(yellow, f"## P2 · High coupling ({len(coupling)}, Top 15) — distance D > {COUPLING_DISTANCE}"))
    for crate, module, distance, abstractness, instability in sorted(
        coupling, key=lambda c: -c[2]
    )[:15]:
        lines.append(
            f"  [{paint(bold, crate)}] {module} D={distance:.2f} A={abstractness:.2f} I={instability:.2f}"
        )
    if not coupling:
        lines.append("  none")
    lines.append("")

    lines.append(paint(yellow, f"## P2 · Low cohesion ({len(cohesion)}, Top 15) — density < {LOW_COHESION_DENSITY}"))
    for crate, module, density, items in sorted(cohesion, key=lambda c: c[2])[:15]:
        lines.append(f"  [{paint(bold, crate)}] {module} density={density:.3f} items={items}")
    if not cohesion:
        lines.append("  none")
    lines.append("")

    lines.append(paint(gray, f"## P3 · Unstable dependencies ({len(unstable)}) — stable depends on unstable"))
    for crate, from_module, to_module in sorted(unstable)[:15]:
        lines.append(f"  [{paint(bold, crate)}] {from_module} -> {to_module}")
    if not unstable:
        lines.append("  none")
    lines.append("")

    lines.append(paint(gray, f"## P3 · Deep nesting ({len(deep)}, Top 15) — path depth > 4"))
    for crate, module, depth in sorted(deep, key=lambda d: -d[2])[:15]:
        lines.append(f"  [{paint(bold, crate)}] {module} (depth={depth})")
    if not deep:
        lines.append("  none")
    lines.append("")

    lines.append(paint(orange, f"## P2 · Largest public surfaces ({len(pub_surface)}, Top 15)"))
    for crate, module, count in sorted(pub_surface, key=lambda p: -p[2])[:15]:
        lines.append(f"  [{paint(bold, crate)}] {module} ({count} pub items)")
    if not pub_surface:
        lines.append("  none")
    lines.append("")

    lines.append(paint(red, f"## P2 · Dangerous operations ({len(danger)}, Top 15)"))
    for crate, module, panics, unwraps, expects in sorted(
        danger, key=lambda d: -(d[2] + d[3] + d[4])
    )[:15]:
        lines.append(
            f"  [{paint(bold, crate)}] {module} (panic={panics}, unwrap={unwraps}, expect={expects})"
        )
    if not danger:
        lines.append("  none")

    return "\n".join(lines)


def recommend(binary: str, manifests: list[str], out_dir: str | None) -> int:
    """Prints per-crate reports and the remediation list, then writes the
    HTML pages (index + per-crate graphs) and a text log under `out_dir`."""
    reports = collect_reports(binary, manifests)
    findings = extract_findings(reports)

    log_lines: list[str] = []
    print("# Per-crate reports\n", flush=True)
    log_lines.append("# Per-crate reports")
    for manifest, _report in reports:
        print(f"\n=== {manifest} ===", flush=True)
        log_lines.append(f"\n=== {manifest} ===")
        result = subprocess.run(
            [binary, "report", "--manifest", manifest],
            capture_output=True,
            text=True,
            check=False,
        )
        if result.stdout:
            print(result.stdout, end="", flush=True)
            log_lines.append(result.stdout.rstrip("\n"))

    print("\n\n" + "=" * 60 + "\n", flush=True)
    print(summary_text(findings, len(reports), color=True), flush=True)
    log_lines.append("\n" + "=" * 60 + "\n")
    log_lines.append(summary_text(findings, len(reports), color=False))

    if out_dir:
        out_dir = Path(out_dir)
        graphs_dir = out_dir / "graphs"
        graphs_dir.mkdir(parents=True, exist_ok=True)
        crate_names, crate_edges = discover_crate_deps()
        all_module_files: dict[str, str] = {}
        for _manifest, report in reports:
            all_module_files.update(report.get("module_files", {}))

        index_html = generate_index_html(
            reports, findings, crate_names, crate_edges, all_module_files
        )
        (out_dir / "index.html").write_text(index_html, encoding="utf-8")

        crate_graph_html = generate_crate_graph_html(crate_names, crate_edges)
        (out_dir / "crate_graph.html").write_text(crate_graph_html, encoding="utf-8")

        metrics_html = generate_metrics_html()
        (out_dir / "metrics.html").write_text(metrics_html, encoding="utf-8")

        for manifest, report in reports:
            crate = report["crate"]
            module_html = generate_module_html(crate, report, all_module_files)
            (graphs_dir / f"{crate}.html").write_text(module_html, encoding="utf-8")

        (out_dir / "report.log").write_text("\n".join(log_lines) + "\n", encoding="utf-8")

        index_path = out_dir / "index.html"
        print(f"\nHTML report: {hyperlink('file://' + str(index_path), str(index_path))}", flush=True)
        print(f"Graphs dir: {hyperlink('file://' + str(graphs_dir), str(graphs_dir))}", flush=True)
        print(f"Log: {hyperlink('file://' + str(out_dir / 'report.log'), str(out_dir / 'report.log'))}", flush=True)

    return 0


def run_per_crate(binary: str, manifests: list[str], mode: str, as_json: bool) -> int:
    """Runs a per-crate analysis mode, one section per crate."""
    failed = 0
    for manifest in manifests:
        print(f"\n=== {manifest} ===")
        command = [binary, mode, "--manifest", manifest]
        if as_json and mode == "report":
            command.append("--json")
        if subprocess.run(command, check=False).returncode != 0:
            failed += 1
    print(f"\nmudu_arch: analysed {len(manifests)} crate(s), {failed} failed")
    return 1 if failed else 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--mode",
        choices=["recommend", "report", "pub-visibility", "orphans"],
        default="recommend",
    )
    parser.add_argument("--bin", default=None, help="path to the mudu_arch binary")
    parser.add_argument("--json", action="store_true", help="JSON output (report only)")
    parser.add_argument(
        "--html",
        default=str(ROOT / "target" / "analyze-modules"),
        help="output directory for the HTML report (recommend mode)",
    )
    parser.add_argument("--no-html", action="store_true", help="do not write the HTML report")
    args = parser.parse_args()

    binary = args.bin or str(ROOT / "target" / "debug" / "mudu_arch")
    if not Path(binary).exists():
        print(
            f"mudu_arch binary not found at {binary}; "
            "run `cargo build -p mudu_arch` first",
            file=sys.stderr,
        )
        return 1

    manifests = discover_lib_crates()
    if args.mode == "recommend":
        html_path = None if args.no_html else args.html
        return recommend(binary, manifests, html_path)
    return run_per_crate(binary, manifests, args.mode, args.json)


# ---------------------------------------------------------------------------
# HTML report
# ---------------------------------------------------------------------------

_CSS = """
* { box-sizing: border-box; }
body { font-family: -apple-system, 'Segoe UI', 'Noto Sans', sans-serif; margin: 0; color: #1f2328; background: #f6f8fa; }
header { background: linear-gradient(135deg, #24292f, #3a4149); color: #fff; padding: 24px 32px; }
header h1 { margin: 0 0 6px; font-size: 22px; }
header p { margin: 0; opacity: 0.75; font-size: 13px; }
main { max-width: 1180px; margin: 0 auto; padding: 24px; }
body.wide main { max-width: none; padding: 16px 24px; }
h2 { font-size: 18px; margin: 32px 0 12px; border-left: 4px solid #0969da; padding-left: 10px; }
h3 { font-size: 15px; margin: 18px 0 8px; color: #24292f; }
.dash { display: grid; grid-template-columns: repeat(auto-fit, minmax(150px, 1fr)); gap: 12px; margin: 20px 0; }
.card { background: #fff; border: 1px solid #d0d7de; border-radius: 8px; padding: 16px; text-align: center; }
a.card { text-decoration: none; display: block; }
a.card:hover { border-color: #0969da; box-shadow: 0 1px 4px rgba(9,105,218,0.15); }
.card .num { font-size: 30px; font-weight: 700; color: #0969da; }
.card .lbl { font-size: 12px; color: #57606a; margin-top: 4px; }
.card.p0 .num { color: #cf222e; } .card.p1 .num { color: #bc4c00; }
.card.p2 .num { color: #bf8700; } .card.p3 .num { color: #57606a; }
.summary { background: #fff; border: 1px solid #d0d7de; border-radius: 8px; padding: 16px 20px; }
.summary h3 { margin: 18px 0 8px; font-size: 14px; }
.summary h3.p0 { color: #cf222e; } .summary h3.p1 { color: #bc4c00; }
.summary h3.p2 { color: #bf8700; } .summary h3.p3 { color: #57606a; }
.summary ul { margin: 0; padding-left: 20px; font-size: 13px; }
.summary li { margin: 3px 0; font-family: ui-monospace, monospace; }
.note { font-size: 12px; color: #57606a; background: #fff; border: 1px dashed #d0d7de; border-radius: 8px; padding: 12px 16px; margin: 20px 0; }
.crate-graph { background: #fff; border: 1px solid #d0d7de; border-radius: 8px; padding: 8px; margin: 16px 0; overflow: auto; }
.crate-graph svg { width: 100%; height: auto; display: block; }
.crate-graph .cap { font-weight: 600; font-size: 13px; padding: 6px 8px; color: #24292f; }
.crate-graph svg { display: block; }
circle.cyc { stroke: #cf222e; stroke-width: 2.5; }
line { stroke: #b6c2cf; stroke-width: 1; }
line.cyc { stroke: #cf222e; stroke-width: 2; }
.node text { font-size: 9px; fill: #24292f; }
a.ml { color: #0969da; text-decoration: none; font-family: ui-monospace, monospace; }
a.ml:hover { text-decoration: underline; }
.crate-graph a { cursor: pointer; }
.crate-graph a text { fill: #24292f; font-family: ui-monospace, monospace; }
.edge { stroke: #9aa7b3; stroke-width: 1.2; }
line.cyc { stroke: #cf222e; stroke-width: 2; }
line.back { stroke: #e5484d; stroke-width: 2.2; stroke-dasharray: 7 4; }
.tip { position: fixed; display: none; background: #24292f; color: #fff; padding: 5px 8px; border-radius: 5px; font-size: 12px; font-family: ui-monospace, monospace; pointer-events: none; z-index: 10; }
.graph-link { margin: 20px 0; }
.graph-link a { color: #0969da; font-weight: 600; text-decoration: none; }
.graph-link a:hover { text-decoration: underline; }
.crate-list { list-style: none; padding: 0; column-count: 3; column-gap: 24px; font-size: 13px; }
.crate-list li { margin: 4px 0; font-family: ui-monospace, monospace; }
.crate-list a { color: #0969da; text-decoration: none; }
.crate-list a:hover { text-decoration: underline; }
.crate-list a.cyclic, .summary a.cyclic { color: #cf222e; }
a.back { color: #0969da; text-decoration: none; font-size: 13px; }
a.back:hover { text-decoration: underline; }
.meta { color: #57606a; font-size: 13px; }
.metric-def { font-size: 14px; line-height: 1.6; }
.metric-formula { font-size: 13px; line-height: 1.6; color: #24292f; background: #f6f8fa; border-left: 3px solid #d0d7de; padding: 8px 12px; }
.metric-formula code { font-family: ui-monospace, monospace; background: #eaeef2; padding: 1px 4px; border-radius: 3px; }
.metric-harms { font-size: 13px; line-height: 1.6; }
.metric-harms li { margin: 5px 0; }
"""

_JS = """
function shortName(id) { return id.split('::').pop(); }

function dagreLayout(nodes, edges, width, height) {
  function doLayout(nodesep, ranksep) {
    const g = new dagre.graphlib.Graph();
    g.setGraph({ rankdir: 'LR', nodesep: nodesep, ranksep: ranksep, marginx: 24, marginy: 24 });
    g.setDefaultEdgeLabel(function() { return {}; });
    for (const n of nodes) {
      g.setNode(n.id, { width: Math.max(48, n.label.length * 7 + 24), height: 28 });
    }
    for (const [a, b] of edges) {
      g.setEdge(a.id, b.id);
    }
    dagre.layout(g);
    return g;
  }

  // Iterate on nodesep/ranksep so the aspect ratio lands near a readable
  // landscape (width roughly 1.4x height) instead of a wide flat strip.
  let nodesep = 24;
  let ranksep = 90;
  let g = doLayout(nodesep, ranksep);
  for (let i = 0; i < 4; i++) {
    const ratio = g.graph().width / Math.max(1, g.graph().height);
    if (ratio > 1.6) {
      ranksep *= 0.75;
      nodesep *= 1.25;
    } else if (ratio < 0.9) {
      ranksep *= 1.25;
      nodesep *= 0.8;
    } else {
      break;
    }
    g = doLayout(nodesep, ranksep);
  }

  for (const n of nodes) {
    const p = g.node(n.id);
    n.x = p.x;
    n.y = p.y;
  }
  return { width: Math.max(width, g.graph().width), height: Math.max(height, g.graph().height) };
}

function forceLayout(nodes, edges, width, height) {
  for (let i = 0; i < 400; i++) {
    for (const a of nodes) for (const b of nodes) {
      if (a === b) continue;
      let dx = a.x - b.x, dy = a.y - b.y;
      let d = Math.max(1, Math.sqrt(dx*dx + dy*dy));
      let f = 1400 / (d * d);
      if (d > 260) f = 0;
      a.vx += dx / d * f; a.vy += dy / d * f;
    }
    for (const [a, b] of edges) {
      let dx = b.x - a.x, dy = b.y - a.y;
      let d = Math.max(1, Math.sqrt(dx*dx + dy*dy));
      let f = (d - 90) * 0.015;
      a.vx += dx / d * f; a.vy += dy / d * f;
      b.vx -= dx / d * f; b.vy -= dy / d * f;
    }
    for (const n of nodes) {
      n.vx += (width/2 - n.x) * 0.002;
      n.vy += (height/2 - n.y) * 0.002;
      n.x += n.vx; n.y += n.vy;
      n.vx *= 0.82; n.vy *= 0.82;
      n.x = Math.max(n.r, Math.min(width - n.r, n.x));
      n.y = Math.max(n.r, Math.min(height - n.r, n.y));
    }
  }
}

function renderGraph(host, data) {
  const NS = 'http://www.w3.org/2000/svg';
  const isDag = data.cycles.length === 0;
  const width = 960;
  const height = Math.max(360, data.nodes.length * (isDag ? 22 : 15));

  const nodes = data.nodes.map(n => ({
    id: n.id, file: n.file || '', label: shortName(n.id),
    indegree: n.indegree || 0,
    r: 5 + Math.min(16, (n.indegree || 0) * 1.8),
    x: Math.random() * width, y: Math.random() * height,
    vx: 0, vy: 0,
  }));
  const idx = new Map(nodes.map(n => [n.id, n]));
  const edges = data.edges.filter(e => idx.has(e.from) && idx.has(e.to))
    .map(e => [idx.get(e.from), idx.get(e.to)]);
  const cyclic = new Set([].concat.apply([], data.cycles || []));
  const breakSet = new Set((data.cycle_break_edges || []).map(e => e.from + '\u0000' + e.to));
  const backEdges = edges.filter(([a, b]) => breakSet.has(a.id + '\u0000' + b.id));
  const dagEdges = edges.filter(([a, b]) => !breakSet.has(a.id + '\u0000' + b.id));

  let layoutW = width, layoutH = height;
  if (isDag || backEdges.length > 0) {
    // Break cycles first, lay out the resulting DAG with dagre, then draw the
    // cycle's back-edges on top (red) so the graph reads as a clean hierarchy.
    try {
      const layout = dagreLayout(nodes, dagEdges, width, height);
      layoutW = layout.width;
      layoutH = layout.height;
    } catch (err) {
      forceLayout(nodes, edges, width, height);
    }
  } else {
    forceLayout(nodes, edges, width, height);
  }

  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('viewBox', '0 0 ' + layoutW + ' ' + layoutH);
  svg.setAttribute('width', '100%');
  svg.style.cursor = 'grab';
  svg.style.background = '#fff';
  host.appendChild(svg);

  const defs = document.createElementNS(NS, 'defs');
  defs.innerHTML = '<marker id="arrow" viewBox="0 0 10 10" refX="9" refY="5" markerWidth="7" markerHeight="7" orient="auto"><path d="M 0 0 L 10 5 L 0 10 z" fill="#57606a"/></marker>';
  svg.appendChild(defs);
  const g = document.createElementNS(NS, 'g');
  svg.appendChild(g);

  const lineEls = [];
  for (const [a, b] of edges) {
    const l = document.createElementNS(NS, 'line');
    l.setAttribute('marker-end', 'url(#arrow)');
    l.classList.add('edge');
    if (breakSet.has(a.id + '\u0000' + b.id)) l.classList.add('back');
    if (cyclic.has(a.id) && cyclic.has(b.id)) l.classList.add('cyc');
    g.appendChild(l);
    lineEls.push([a, b, l]);
  }

  const adjacency = new Map();
  for (const n of nodes) adjacency.set(n.id, new Set([n.id]));
  for (const [a, b] of edges) {
    adjacency.get(a.id).add(b.id);
    adjacency.get(b.id).add(a.id);
  }
  function highlight(n) {
    const neighbors = adjacency.get(n.id);
    for (const m of nodes) {
      m.el.style.opacity = neighbors.has(m.id) ? '1' : '0.12';
    }
    for (const [x, y, l] of lineEls) {
      l.style.opacity = neighbors.has(x.id) && neighbors.has(y.id) ? '1' : '0.08';
    }
  }
  function clearHighlight() {
    for (const m of nodes) m.el.style.opacity = '1';
    for (const [, , l] of lineEls) l.style.opacity = '1';
  }

  const tooltip = document.createElement('div');
  tooltip.className = 'tip';
  document.body.appendChild(tooltip);

  for (const n of nodes) {
    const a = document.createElementNS(NS, 'a');
    if (n.file) { a.setAttribute('href', 'file://' + n.file); a.setAttribute('target', '_blank'); }
    a.setAttribute('transform', 'translate(' + n.x + ',' + n.y + ')');
    const c = document.createElementNS(NS, 'circle');
    c.setAttribute('r', n.r);
    if (cyclic.has(n.id)) c.setAttribute('fill', '#ff6b6b');
    else if (n.indegree > 8) c.setAttribute('fill', '#0969da');
    else c.setAttribute('fill', '#79b8ff');
    if (cyclic.has(n.id)) c.classList.add('cyc');
    const t = document.createElementNS(NS, 'text');
    t.setAttribute('x', n.r + 3); t.setAttribute('y', 3);
    t.textContent = n.label;
    a.appendChild(c); a.appendChild(t);
    g.appendChild(a);
    n.el = a;

    a.addEventListener('mouseenter', () => {
      tooltip.style.display = 'block';
      tooltip.textContent = n.id + ' (in-degree ' + n.indegree + ')';
      highlight(n);
    });
    a.addEventListener('mousemove', e => {
      tooltip.style.left = (e.clientX + 12) + 'px';
      tooltip.style.top = (e.clientY + 12) + 'px';
    });
    a.addEventListener('mouseleave', () => {
      tooltip.style.display = 'none';
      clearHighlight();
    });

    a.addEventListener('mousedown', e => {
      e.preventDefault(); e.stopPropagation();
      function move(ev) {
        const p = toSvgPoint(ev.clientX, ev.clientY);
        n.x = p.x; n.y = p.y;
        update();
      }
      function up() {
        window.removeEventListener('mousemove', move);
        window.removeEventListener('mouseup', up);
      }
      window.addEventListener('mousemove', move);
      window.addEventListener('mouseup', up);
    });
  }

  function toSvgPoint(clientX, clientY) {
    const pt = svg.createSVGPoint();
    pt.x = clientX; pt.y = clientY;
    return pt.matrixTransform(svg.getScreenCTM().inverse());
  }

  function update() {
    for (const [a, b, l] of lineEls) {
      let dx = b.x - a.x, dy = b.y - a.y;
      let d = Math.max(1, Math.sqrt(dx * dx + dy * dy));
      let ux = dx / d, uy = dy / d;
      // Shorten the line so it runs from the edge of the source node to the
      // edge of the target node, leaving the arrowhead visible.
      let sx = a.x + ux * a.r, sy = a.y + uy * a.r;
      let ex = b.x - ux * (b.r + 7), ey = b.y - uy * (b.r + 7);
      l.setAttribute('x1', sx); l.setAttribute('y1', sy);
      l.setAttribute('x2', ex); l.setAttribute('y2', ey);
    }
    for (const n of nodes) {
      n.el.setAttribute('transform', 'translate(' + n.x + ',' + n.y + ')');
    }
  }
  update();

  svg.addEventListener('wheel', e => {
    e.preventDefault();
    const vb = svg.viewBox.baseVal;
    const scale = e.deltaY > 0 ? 1.1 : 0.9;
    vb.width = Math.max(100, Math.min(6000, vb.width * scale));
    vb.height = Math.max(100, Math.min(6000, vb.height * scale));
  }, { passive: false });

  svg.addEventListener('mousedown', e => {
    if (e.target.closest('circle') || e.target.closest('text')) return;
    const vb = svg.viewBox.baseVal;
    const sx = e.clientX, sy = e.clientY, ox = vb.x, oy = vb.y;
    svg.style.cursor = 'grabbing';
    function move(ev) {
      const scale = vb.width / svg.clientWidth;
      vb.x = ox - (ev.clientX - sx) * scale;
      vb.y = oy - (ev.clientY - sy) * scale;
    }
    function up() {
      window.removeEventListener('mousemove', move);
      window.removeEventListener('mouseup', up);
      svg.style.cursor = 'grab';
    }
    window.addEventListener('mousemove', move);
    window.addEventListener('mouseup', up);
  });
}

function renderAll() {
  for (const entry of window.GRAPHS) {
    const host = document.getElementById(entry.id);
    if (!host) continue;
    try {
      renderGraph(host, entry.data);
    } catch (err) {
      host.textContent = 'Graph error: ' + err.message;
    }
  }
}
if (document.readyState === 'loading') {
  document.addEventListener('DOMContentLoaded', renderAll);
} else {
  renderAll();
}
"""


def collect_reports(binary: str, manifests: list[str]) -> list[tuple[str, dict]]:
    """Collects every crate's full JSON report."""
    reports: list[tuple[str, dict]] = []
    for manifest in manifests:
        report = analyze_crate(binary, manifest)
        if report is not None:
            reports.append((manifest, report))
    return reports


def _page(title: str, subtitle: str, body: str, graphs: list[dict], wide: bool = False) -> str:
    """Wraps a body in the shared HTML page skeleton."""
    body_class = ' class="wide"' if wide else ''
    return f"""<!DOCTYPE html>
<html lang="en"><head><meta charset="utf-8">
<title>{title}</title>
<style>{_CSS}</style></head><body{body_class}>
<header><h1>{title}</h1><p>{subtitle}</p></header>
<main>{body}</main>
<script>{_DAGRE}</script>
<script>window.GRAPHS = {json.dumps(graphs)};</script>
<script>{_JS}</script>
</body></html>"""


def build_crate_graph(crate_names: list[str], crate_edges: list[list[str]]) -> dict:
    """Builds the workspace crate dependency graph data."""
    crate_indegree: dict[str, int] = {name: 0 for name in crate_names}
    for from_crate, to_crate in crate_edges:
        crate_indegree[to_crate] = crate_indegree.get(to_crate, 0) + 1
    return {
        "id": "graph-crates",
        "crate": "workspace",
        "data": {
            "nodes": [
                {"id": name, "indegree": crate_indegree.get(name, 0), "file": ""}
                for name in crate_names
            ],
            "edges": [
                {"from": from_crate, "to": to_crate}
                for from_crate, to_crate in crate_edges
            ],
            "cycles": [],
        },
    }


def generate_index_html(
    reports: list[tuple[str, dict]],
    findings: dict,
    crate_names: list[str],
    crate_edges: list[list[str]],
    all_module_files: dict[str, str],
) -> str:
    """Builds the index page: dashboard, crate graph, summary and per-crate
    links (each crate's module graph is a separate page)."""

    def module_link(module: str) -> str:
        file = all_module_files.get(module)
        if file:
            return (
                f'<a class="ml" href="file://{file}" target="_blank" '
                f'title="{html_escape(file)}">{html_escape(module)}</a>'
            )
        return html_escape(module)

    def module_list(modules: list[str]) -> str:
        return ", ".join(module_link(m) for m in modules)

    cycles = findings["cycles"]
    god_modules = findings["god_modules"]
    coupling = findings["coupling"]
    cohesion = findings["cohesion"]
    unstable = findings["unstable"]
    deep = findings["deep"]
    pub_surface = findings["pub_surface"]
    danger = findings["danger"]

    summary_parts = []
    summary_parts.append(
        f'<h3 class="p0">P0 · Cyclic dependencies ({len(cycles)})</h3><ul>'
        + "".join(
            f'<li><a class="cyclic" href="graphs/{html_escape(crate)}.html">'
            f"{html_escape(crate)}</a> ({len(c)} modules): {module_list(c)}</li>"
            for crate, c in sorted(cycles, key=lambda c: -len(c[1]))
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p1">P1 · God modules ({len(god_modules)})</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(module)} ({items} items, {aff} depended-on)</li>"
            for crate, module, items, aff in sorted(god_modules, key=lambda g: -g[2])[:15]
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p2">P2 · High coupling ({len(coupling)}, Top 15)</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(module)} D={d:.2f} A={a:.2f} I={i:.2f}</li>"
            for crate, module, d, a, i in sorted(coupling, key=lambda c: -c[2])[:15]
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p2">P2 · Low cohesion ({len(cohesion)}, Top 15)</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(module)} density={density:.3f} items={items}</li>"
            for crate, module, density, items in sorted(cohesion, key=lambda c: c[2])[:15]
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p3">P3 · Unstable dependencies ({len(unstable)})</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(a)} → {module_link(b)}</li>"
            for crate, a, b in sorted(unstable)[:15]
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p3">P3 · Deep nesting ({len(deep)}, Top 15)</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(module)} (depth={depth})</li>"
            for crate, module, depth in sorted(deep, key=lambda d: -d[2])[:15]
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p2">P2 · Largest public surfaces ({len(pub_surface)}, Top 15)</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(module)} ({count} pub items)</li>"
            for crate, module, count in sorted(pub_surface, key=lambda p: -p[2])[:15]
        )
        + "</ul>"
    )
    summary_parts.append(
        f'<h3 class="p2">P2 · Dangerous operations ({len(danger)}, Top 15)</h3><ul>'
        + "".join(
            f"<li>[{crate}] {module_link(module)} (panic={panics}, unwrap={unwraps}, expect={expects})</li>"
            for crate, module, panics, unwraps, expects in sorted(
                danger, key=lambda d: -(d[2] + d[3] + d[4])
            )[:15]
        )
        + "</ul>"
    )

    total = (
        len(cycles)
        + len(god_modules)
        + len(coupling)
        + len(cohesion)
        + len(unstable)
        + len(deep)
        + len(pub_surface)
        + len(danger)
    )

    notes = [
        "<b>D</b> (distance from the main sequence) = |abstractness A + instability I − 1|, closer to 0 is better.",
        "<b>I</b> (instability) = efferent/(efferent+afferent); higher means more volatile (depends on many, depended-on by few).",
        "<b>Density</b> (cohesion) = intra-module item references / (items×(items−1)); lower means a 'junk drawer' module.",
        "Red circles = nodes in a cycle (SCC); red edges = intra-cycle edges. Larger nodes = higher in-degree (hub).",
    ]

    cyclic_crates = {crate for crate, _ in cycles}
    cycle_count: dict[str, int] = {}
    for crate, _ in cycles:
        cycle_count[crate] = cycle_count.get(crate, 0) + 1
    crate_links = "".join(
        f'<li><a href="graphs/{html_escape(crate)}.html"{("" if crate not in cyclic_crates else " class=\"cyclic\"")}>{html_escape(crate)}</a> '
        f'({report.get("modules", 0)} modules'
        + (f", {cycle_count[crate]} cycles" if crate in cyclic_crates else "")
        + ")</li>"
        for _manifest, report in sorted(reports, key=lambda r: r[1]["crate"])
        for crate in [report["crate"]]
    )

    body = f"""<div class="dash">
 <div class="card"><div class="num">{len(reports)}</div><div class="lbl">Crates</div></div>
 <a class="card p0" href="metrics.html#cyclic-deps"><div class="num">{len(cycles)}</div><div class="lbl">Cyclic deps</div></a>
 <a class="card p1" href="metrics.html#god-modules"><div class="num">{len(god_modules)}</div><div class="lbl">God modules</div></a>
 <a class="card p2" href="metrics.html#high-coupling"><div class="num">{len(coupling)}</div><div class="lbl">High coupling</div></a>
 <a class="card p2" href="metrics.html#low-cohesion"><div class="num">{len(cohesion)}</div><div class="lbl">Low cohesion</div></a>
 <a class="card p3" href="metrics.html#unstable-deps"><div class="num">{len(unstable)}</div><div class="lbl">Unstable deps</div></a>
</div>
<p class="graph-link"><a href="metrics.html">Metrics reference — definitions, formulas and harms →</a></p>
<div class="note">{"<br>".join(notes)}</div>
<h2>Summary Remediation List</h2>
<div class="summary">{''.join(summary_parts)}</div>
<h2>Workspace</h2>
<p class="graph-link"><a href="crate_graph.html">Open workspace crate dependency graph →</a></p>
<h3>Crates</h3>
<ul class="crate-list">{crate_links}</ul>"""

    return _page(
        "MuduDB Module Coupling Analysis Report",
        f"Analysed {len(reports)} crates · found {total} findings",
        body,
        [],
    )


def generate_crate_graph_html(crate_names: list[str], crate_edges: list[list[str]]) -> str:
    """Builds the standalone workspace crate dependency graph page."""
    crate_graph = build_crate_graph(crate_names, crate_edges)
    body = """<p><a class="back" href="index.html">← back to report</a></p>
<div class="crate-graph"><div class="cap">Library crates and their dependencies (arrow points from dependent to dependency)</div><div id="graph-crates"></div></div>"""
    return _page(
        "Workspace Crate Dependency Graph",
        "scroll to zoom · drag to pan · drag a node to move · click a node to open its source file",
        body,
        [crate_graph],
        wide=True,
    )


def generate_module_html(crate: str, report: dict, all_module_files: dict[str, str]) -> str:
    """Builds a single crate's module dependency graph page."""
    module_files = report.get("module_files", {})
    nodes = [
        {
            "id": module,
            "indegree": report.get("indegree", {}).get(module, 0),
            "file": module_files.get(module, ""),
        }
        for module in sorted(report.get("indegree", {}).keys())
    ]
    graph = {
        "id": "graph-" + crate,
        "crate": crate,
        "data": {
            "nodes": nodes,
            "edges": report.get("dependency_edges", []),
            "cycles": report.get("cycles", []),
            "cycle_break_edges": report.get("cycle_break_edges", []),
        },
    }
    cycles = report.get("cycles", [])
    body = f"""<p><a class="back" href="../index.html">← back to report</a></p>
<p class="meta">Modules: {len(nodes)} · Edges: {len(report.get("dependency_edges", []))} · Cycles: {len(cycles)} · arrow points from dependent to dependency</p>
<div class="crate-graph"><div class="cap">{html_escape(crate)}</div><div id="graph-{crate}"></div></div>"""
    return _page(
        f"{crate} · Module Dependency Graph",
        "scroll to zoom · drag to pan · drag a node to move · click a node to open its source file",
        body,
        [graph],
        wide=True,
    )


def html_escape(text: str) -> str:
    return (
        text.replace("&", "&amp;")
        .replace("<", "&lt;")
        .replace(">", "&gt;")
    )


_METRICS = [
    {
        "id": "cyclic-deps",
        "title": "Cyclic Dependencies",
        "def": "Modules that reference each other in a cycle — A depends on B, B depends on C, C depends on A. Reported as strongly-connected components (SCC) of the module dependency graph with more than one member.",
        "formula": "Build the directed module graph from <code>use</code> statements and fully-qualified path references (<code>crate::…</code>, <code>super::…</code>), then run Tarjan's strongly-connected-component algorithm. A component of size &gt; 1 (or a self-loop) is a cycle. The <code>cycle_break_edges</code> list names the edges whose removal breaks each cycle.",
        "harms": [
            "The members can only be understood together, never in isolation.",
            "Any edit ripples around the whole cycle, making every change risky.",
            "Initialisation and teardown order becomes hard to reason about.",
            "Testing one member forces you to construct the entire cycle.",
        ],
    },
    {
        "id": "god-modules",
        "title": "God Modules",
        "def": "A module that owns too many items (functions, structs, traits, …) and is depended on by many others. It mixes unrelated responsibilities in one place.",
        "formula": "Count each module's owned items (<code>item_count</code>: top-level fn/struct/enum/trait/const/…). A module with <code>item_count ≥ 50</code> is reported, alongside its afferent coupling (how many modules depend on it).",
        "harms": [
            "Violates the single-responsibility principle; hard to name and navigate.",
            "A change fans out to every dependent module, exploding regression scope.",
            "Concentrates merge conflicts — many people edit the same file.",
            "Responsibilities interleave, so the module cannot be tested in isolation.",
        ],
    },
    {
        "id": "high-coupling",
        "title": "High Coupling",
        "def": "A module whose position on the Robert Martin 'main sequence' is far from ideal — either concrete but heavily depended-on, or abstract but depending on too much detail.",
        "formula": "Instability <code>I = Ce / (Ce + Ca)</code> (Ce = efferent/depends-on count, Ca = afferent/depended-on count). Abstractness <code>A = abstract items / (abstract + concrete items)</code>. Distance from the main sequence <code>D = |A + I − 1|</code>; <code>D &gt; 0.5</code> is reported.",
        "harms": [
            "A concrete-but-stable module (A≈0, I≈0) is a dangerous hub: any change propagates everywhere.",
            "An abstract-but-volatile module (A≈1, I≈1) drags abstractions into implementation details.",
            "Change propagation is uncontrolled; modules cannot be reused independently.",
            "The system becomes rigid — adding a feature feels like rebuilding the foundation.",
        ],
    },
    {
        "id": "low-cohesion",
        "title": "Low Cohesion",
        "def": "A module whose items barely reference each other — a 'junk drawer' where unrelated things are grouped together.",
        "formula": "Intra-module reference density = <code>internal references / (items × (items − 1))</code>. A module with <code>items ≥ 10</code> and density <code>&lt; 0.02</code> is reported.",
        "harms": [
            "The module has no clear theme; readers cannot tell what it is responsible for.",
            "Changing one feature risks touching unrelated features in the same file.",
            "Tests must cover unrelated behaviour, making them hard to write and maintain.",
            "High-cohesion modules are easy to name and reuse; low-cohesion ones are avoided.",
        ],
    },
    {
        "id": "unstable-deps",
        "title": "Unstable Dependencies",
        "def": "A stable module (depended on by many) depending on an unstable module (depends on many, depended on by few) — the dependency points the wrong way.",
        "formula": "For every dependency edge <code>from → to</code>, report it when <code>I(from) &lt; 0.5</code> and <code>I(to) &gt; 0.5</code>, where <code>I = Ce / (Ce + Ca)</code>.",
        "harms": [
            "Violates the dependency-inversion principle: stable cores should not depend on volatile detail.",
            "A change in the unstable detail forces the stable structure to change, making the system fragile.",
            "It is the seed of 'bad dependency' — the ideal is stable ← abstraction ← unstable.",
        ],
    },
]


def generate_metrics_html() -> str:
    """Builds the standalone metrics reference page (definition, formula, harm)."""
    sections = []
    for m in _METRICS:
        harms = "".join(f"<li>{h}</li>" for h in m["harms"])
        sections.append(
            f'<h2 id="{m["id"]}">{m["title"]}</h2>'
            f'<p class="metric-def">{m["def"]}</p>'
            f'<h3>How it is computed</h3><p class="metric-formula">{m["formula"]}</p>'
            f'<h3>Why it matters</h3><ul class="metric-harms">{harms}</ul>'
        )
    body = '<p><a class="back" href="index.html">← back to report</a></p>\n' + "\n".join(sections)
    return _page(
        "Metrics Reference",
        "Definitions, formulas and harms of the module-coupling metrics",
        body,
        [],
    )


if __name__ == "__main__":
    sys.exit(main())
