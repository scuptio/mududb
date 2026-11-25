//! Report formatting: human-readable text and machine-readable JSON.

use std::cmp::Reverse;
use std::collections::{HashMap, HashSet};

use anyhow::Result;

use crate::analyze::{Analysis, CohesionMetric};
use crate::collect::{DangerCount, DegreeMap, ModuleNode};

/// Secondary metrics bundled together to keep report signatures short.
pub(crate) struct ExtraMetrics {
    pub cohesion: Vec<CohesionMetric>,
    pub deep: Vec<(String, usize)>,
    pub unstable: Vec<(String, String)>,
    pub cycle_break: Vec<(String, String)>,
    pub pub_surface: HashMap<String, usize>,
    pub danger: HashMap<String, DangerCount>,
}

/// Prints the human-readable report.
pub(crate) fn print_text(
    crate_name: &str,
    nodes: &[ModuleNode],
    uses: &HashMap<String, HashSet<String>>,
    analysis: &Analysis,
    extra: &ExtraMetrics,
) {
    let edge_count: usize = uses.values().map(HashSet::len).sum();
    println!(
        "crate `{crate_name}`: {} modules, {edge_count} use-edges",
        uses.len()
    );
    print_cycles_and_hubs(analysis, &extra.cycle_break);
    print_coupling_and_size(nodes, analysis, &extra.cohesion);
    print_structural_metrics(extra);
}

fn print_cycles_and_hubs(analysis: &Analysis, cycle_break: &[(String, String)]) {
    println!("\nCycles (strongly-connected components):");
    if analysis.cycles.is_empty() {
        println!("  none");
    }
    for component in &analysis.cycles {
        let mut names: Vec<&str> = component.iter().map(String::as_str).collect();
        names.sort_unstable();
        println!("  [{}] {}", component.len(), names.join(", "));
    }
    if !cycle_break.is_empty() {
        println!("\nCycle break candidates (remove these edges):");
        for (from, to) in cycle_break {
            println!("  {from} -> {to}");
        }
    }
    println!("\nHubs (most depended-upon, in-degree):");
    print_top(&analysis.indegree, 15);
    println!("\nMost coupled (out-degree):");
    print_top(&analysis.outdegree, 15);
}

fn print_coupling_and_size(nodes: &[ModuleNode], analysis: &Analysis, cohesion: &[CohesionMetric]) {
    println!("\nCoupling (worst distance from main sequence, D = |A + I - 1|):");
    for metric in analysis.coupling.iter().take(15) {
        println!(
            "  D={:.2}  A={:.2}  I={:.2}  {}  (Ce={}, Ca={})",
            metric.distance,
            metric.abstractness,
            metric.instability,
            metric.module,
            metric.efferent,
            metric.afferent
        );
    }

    println!("\nLargest modules (god-module candidates, by item count):");
    let mut largest: Vec<&ModuleNode> = nodes.iter().collect();
    largest.sort_by_key(|node| std::cmp::Reverse(node.item_count));
    for node in largest.iter().take(10) {
        let afferent = analysis.indegree.get(&node.path).copied().unwrap_or(0);
        println!(
            "  {:3} items  afferent={:3}  {}",
            node.item_count, afferent, node.path
        );
    }

    println!("\nLeast cohesive modules (internal ref density, items >= 10):");
    let mut shown = 0;
    for metric in cohesion {
        if metric.items < 10 || shown >= 10 {
            continue;
        }
        shown += 1;
        println!(
            "  density={:.3}  refs={:3}  items={:3}  {}",
            metric.density, metric.internal_refs, metric.items, metric.module
        );
    }
    if shown == 0 {
        println!("  none");
    }
}

fn print_structural_metrics(extra: &ExtraMetrics) {
    println!("\nDeeply nested modules (path depth > 4):");
    if extra.deep.is_empty() {
        println!("  none");
    }
    for (module, depth) in extra.deep.iter().take(10) {
        println!("  depth={depth}  {module}");
    }

    println!("\nUnstable dependencies (stable I<0.5 -> unstable I>0.5):");
    if extra.unstable.is_empty() {
        println!("  none");
    }
    for (from, to) in extra.unstable.iter().take(15) {
        println!("  {from} -> {to}");
    }

    println!("\nLargest public surfaces (pub items per module, Top 15):");
    let mut surface: Vec<(&String, &usize)> = extra.pub_surface.iter().collect();
    surface.sort_by(|a, b| b.1.cmp(a.1));
    for (module, count) in surface.iter().take(15) {
        println!("  {count:3}  {module}");
    }

    println!("\nDangerous operations (panic!/unwrap()/expect(), Top 15):");
    let mut danger: Vec<(&String, &DangerCount)> = extra.danger.iter().collect();
    danger.sort_by_key(|(_, count)| Reverse(count.total()));
    if danger.is_empty() {
        println!("  none");
    }
    for (module, count) in danger.iter().take(15) {
        println!(
            "  panic={} unwrap={} expect={}  {module}",
            count.panics, count.unwraps, count.expects
        );
    }
}

fn print_top(degree: &DegreeMap, limit: usize) {
    let mut entries: Vec<(&String, &usize)> = degree.iter().collect();
    entries.sort_by(|a, b| b.1.cmp(a.1));
    for (module, value) in entries.iter().take(limit) {
        println!("  {value:3}  {module}");
    }
}

/// Prints the report as JSON.
pub(crate) fn print_json(
    crate_name: &str,
    nodes: &[ModuleNode],
    uses: &HashMap<String, HashSet<String>>,
    analysis: &Analysis,
    extra: &ExtraMetrics,
) -> Result<()> {
    let edge_count: usize = uses.values().map(HashSet::len).sum();
    let report = build_json_report(
        crate_name,
        uses.len(),
        edge_count,
        nodes,
        uses,
        analysis,
        extra,
    );
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn build_json_report(
    crate_name: &str,
    module_count: usize,
    edge_count: usize,
    nodes: &[ModuleNode],
    uses: &HashMap<String, HashSet<String>>,
    analysis: &Analysis,
    extra: &ExtraMetrics,
) -> serde_json::Value {
    serde_json::json!({
        "crate": crate_name,
        "modules": module_count,
        "edges": edge_count,
        "cycles": json_cycles(analysis),
        "dependency_edges": json_edges(uses),
        "module_files": json_module_files(nodes),
        "cycle_break_edges": json_cycle_break(&extra.cycle_break),
        "indegree": json_degree(&analysis.indegree),
        "outdegree": json_degree(&analysis.outdegree),
        "coupling": json_coupling(analysis),
        "largest_modules": json_largest(nodes, analysis),
        "cohesion": json_cohesion(&extra.cohesion),
        "deep_nesting": json_deep(&extra.deep),
        "unstable_dependencies": json_unstable(&extra.unstable),
        "pub_surface": json_pub_surface(&extra.pub_surface),
        "dangerous_calls": json_danger(&extra.danger),
    })
}

fn json_cycles(analysis: &Analysis) -> Vec<Vec<String>> {
    analysis
        .cycles
        .iter()
        .map(|component| {
            let mut names = component.clone();
            names.sort();
            names
        })
        .collect()
}

fn json_coupling(analysis: &Analysis) -> Vec<serde_json::Value> {
    analysis
        .coupling
        .iter()
        .map(|metric| {
            serde_json::json!({
                "module": metric.module,
                "efferent": metric.efferent,
                "afferent": metric.afferent,
                "instability": metric.instability,
                "abstractness": metric.abstractness,
                "distance": metric.distance,
            })
        })
        .collect()
}

fn json_largest(nodes: &[ModuleNode], analysis: &Analysis) -> Vec<serde_json::Value> {
    let mut largest: Vec<&ModuleNode> = nodes.iter().collect();
    largest.sort_by_key(|node| std::cmp::Reverse(node.item_count));
    largest
        .iter()
        .take(20)
        .map(|node| {
            serde_json::json!({
                "module": node.path,
                "items": node.item_count,
                "afferent": analysis.indegree.get(&node.path).copied().unwrap_or(0),
                "efferent": analysis.outdegree.get(&node.path).copied().unwrap_or(0),
            })
        })
        .collect()
}

fn json_degree(degree: &DegreeMap) -> serde_json::Map<String, serde_json::Value> {
    degree
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::json!(v)))
        .collect()
}

fn json_cohesion(cohesion: &[CohesionMetric]) -> Vec<serde_json::Value> {
    cohesion
        .iter()
        .map(|metric| {
            serde_json::json!({
                "module": metric.module,
                "items": metric.items,
                "internal_refs": metric.internal_refs,
                "density": metric.density,
            })
        })
        .collect()
}

fn json_deep(deep: &[(String, usize)]) -> Vec<serde_json::Value> {
    deep.iter()
        .map(|(module, depth)| serde_json::json!({ "module": module, "depth": depth }))
        .collect()
}

fn json_unstable(unstable: &[(String, String)]) -> Vec<serde_json::Value> {
    unstable
        .iter()
        .map(|(from, to)| serde_json::json!({ "from": from, "to": to }))
        .collect()
}

fn json_edges(uses: &HashMap<String, HashSet<String>>) -> Vec<serde_json::Value> {
    let mut edges: Vec<serde_json::Value> = Vec::new();
    for (from, targets) in uses {
        for to in targets {
            edges.push(serde_json::json!({ "from": from, "to": to }));
        }
    }
    edges
}

fn json_module_files(nodes: &[ModuleNode]) -> serde_json::Map<String, serde_json::Value> {
    nodes
        .iter()
        .map(|node| {
            (
                node.path.clone(),
                serde_json::json!(node.file.to_string_lossy()),
            )
        })
        .collect()
}

fn json_cycle_break(cycle_break: &[(String, String)]) -> Vec<serde_json::Value> {
    cycle_break
        .iter()
        .map(|(from, to)| serde_json::json!({ "from": from, "to": to }))
        .collect()
}

fn json_pub_surface(pub_surface: &HashMap<String, usize>) -> Vec<serde_json::Value> {
    let mut surface: Vec<(&String, &usize)> = pub_surface.iter().collect();
    surface.sort_by(|a, b| b.1.cmp(a.1));
    surface
        .iter()
        .map(|(module, count)| serde_json::json!({ "module": module, "count": count }))
        .collect()
}

fn json_danger(danger: &HashMap<String, DangerCount>) -> Vec<serde_json::Value> {
    let mut entries: Vec<(&String, &DangerCount)> = danger.iter().collect();
    entries.sort_by_key(|(_, count)| Reverse(count.total()));
    entries
        .iter()
        .map(|(module, count)| {
            serde_json::json!({
                "module": module,
                "panics": count.panics,
                "unwraps": count.unwraps,
                "expects": count.expects,
            })
        })
        .collect()
}

/// Prints the dependency structure matrix as JSON (modules × modules adjacency).
pub(crate) fn print_dsm(uses: &HashMap<String, HashSet<String>>) -> Result<()> {
    let mut modules: Vec<String> = uses.keys().cloned().collect();
    modules.sort();
    let index: HashMap<&str, usize> = modules
        .iter()
        .enumerate()
        .map(|(i, m)| (m.as_str(), i))
        .collect();

    let matrix: Vec<Vec<usize>> = modules
        .iter()
        .map(|module| {
            modules
                .iter()
                .map(|other| {
                    if uses.get(module).is_some_and(|t| t.contains(other)) {
                        1
                    } else {
                        0
                    }
                })
                .collect()
        })
        .collect();

    let dsm = serde_json::json!({
        "modules": modules,
        "matrix": matrix,
        "_index": index.keys().collect::<Vec<_>>(),
    });
    println!("{}", serde_json::to_string_pretty(&dsm)?);
    Ok(())
}
