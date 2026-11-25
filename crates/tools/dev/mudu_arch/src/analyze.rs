//! Graph analysis: strongly-connected components, dependency hubs and
//! Robert Martin style coupling metrics.

use std::collections::{HashMap, HashSet};

use crate::collect::{Cycle, DegreeMap, ModuleNode};

/// The result of analysing one crate's dependency graph.
pub(crate) struct Analysis {
    pub cycles: Vec<Cycle>,
    pub indegree: DegreeMap,
    pub outdegree: DegreeMap,
    /// Per-module coupling metrics, only for modules that define types.
    pub coupling: Vec<CouplingMetrics>,
    /// Instability `I = Ce / (Ce + Ca)` for every module.
    pub instability: HashMap<String, f64>,
}

/// Instability / abstractness metrics for one module.
pub(crate) struct CouplingMetrics {
    pub module: String,
    /// Efferent coupling (number of modules this module depends on).
    pub efferent: usize,
    /// Afferent coupling (number of modules that depend on this module).
    pub afferent: usize,
    /// Instability `I = Ce / (Ce + Ca)`.
    pub instability: f64,
    /// Abstractness `A = abstract / (abstract + concrete)`.
    pub abstractness: f64,
    /// Distance from the main sequence `D = |A + I - 1|`.
    pub distance: f64,
}

/// Computes cycles, in/out degree and coupling metrics.
pub(crate) fn analyze(
    nodes: &[ModuleNode],
    modules: &HashSet<String>,
    uses: &HashMap<String, HashSet<String>>,
) -> Analysis {
    let mut indegree: DegreeMap = modules.iter().map(|m| (m.clone(), 0)).collect();
    let mut outdegree: DegreeMap = modules.iter().map(|m| (m.clone(), 0)).collect();
    for (from, targets) in uses {
        outdegree.insert(from.clone(), targets.len());
        for to in targets {
            *indegree.entry(to.clone()).or_insert(0) += 1;
        }
    }

    let cycles = strongly_connected_components(modules, uses);

    let mut instability: HashMap<String, f64> = HashMap::new();
    for module in modules {
        let efferent = *outdegree.get(module).unwrap_or(&0);
        let afferent = *indegree.get(module).unwrap_or(&0);
        let value = if efferent + afferent == 0 {
            0.0
        } else {
            efferent as f64 / (efferent + afferent) as f64
        };
        instability.insert(module.clone(), value);
    }

    let item_stats: HashMap<&str, (usize, usize)> = nodes
        .iter()
        .map(|node| {
            (
                node.path.as_str(),
                (node.abstract_count, node.concrete_count),
            )
        })
        .collect();
    let mut coupling = Vec::new();
    for module in modules {
        let (abstract_count, concrete_count) =
            item_stats.get(module.as_str()).copied().unwrap_or((0, 0));
        let total_types = abstract_count + concrete_count;
        if total_types == 0 {
            continue;
        }
        let efferent = *outdegree.get(module).unwrap_or(&0);
        let afferent = *indegree.get(module).unwrap_or(&0);
        let instability = *instability.get(module).unwrap_or(&0.0);
        let abstractness = abstract_count as f64 / total_types as f64;
        let distance = (abstractness + instability - 1.0).abs();
        coupling.push(CouplingMetrics {
            module: module.clone(),
            efferent,
            afferent,
            instability,
            abstractness,
            distance,
        });
    }
    coupling.sort_by(|a, b| {
        b.distance
            .partial_cmp(&a.distance)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Analysis {
        cycles,
        indegree,
        outdegree,
        coupling,
        instability,
    }
}

/// Tarjan's strongly-connected components, keeping only components that form a
/// real cycle (size > 1, or a self-loop).
fn strongly_connected_components(
    modules: &HashSet<String>,
    uses: &HashMap<String, HashSet<String>>,
) -> Vec<Cycle> {
    struct Tarjan<'a> {
        uses: &'a HashMap<String, HashSet<String>>,
        index: usize,
        indices: HashMap<String, usize>,
        low: HashMap<String, usize>,
        stack: Vec<String>,
        on_stack: HashSet<String>,
        components: Vec<Cycle>,
    }
    impl Tarjan<'_> {
        fn connect(&mut self, v: &str) {
            let idx = self.index;
            self.indices.insert(v.to_string(), idx);
            self.low.insert(v.to_string(), idx);
            self.index += 1;
            self.stack.push(v.to_string());
            self.on_stack.insert(v.to_string());
            for w in self.uses.get(v).map(|set| set.iter()).unwrap_or_default() {
                if let Some(&w_idx) = self.indices.get(w) {
                    if self.on_stack.contains(w) {
                        let low = self.low[v].min(w_idx);
                        self.low.insert(v.to_string(), low);
                    }
                } else {
                    self.connect(w);
                    let low = self.low[v].min(self.low[w]);
                    self.low.insert(v.to_string(), low);
                }
            }
            if self.low[v] == self.indices[v] {
                let mut component = Vec::new();
                loop {
                    let Some(w) = self.stack.pop() else {
                        return;
                    };
                    self.on_stack.remove(&w);
                    let done = w == v;
                    component.push(w);
                    if done {
                        break;
                    }
                }
                self.components.push(component);
            }
        }
    }

    let mut tarjan = Tarjan {
        uses,
        index: 0,
        indices: HashMap::new(),
        low: HashMap::new(),
        stack: Vec::new(),
        on_stack: HashSet::new(),
        components: Vec::new(),
    };
    for module in modules {
        if !tarjan.indices.contains_key(module) {
            tarjan.connect(module);
        }
    }

    let mut cycles: Vec<Cycle> = tarjan
        .components
        .into_iter()
        .filter(|component| {
            component.len() > 1
                || component
                    .first()
                    .is_some_and(|only| uses.get(only).is_some_and(|t| t.contains(only)))
        })
        .collect();
    cycles.sort_by_key(|component| std::cmp::Reverse(component.len()));
    cycles
}

/// Cohesion metric for one module: how much its items reference each other.
pub(crate) struct CohesionMetric {
    pub module: String,
    pub items: usize,
    pub internal_refs: usize,
    /// Internal reference density `refs / (items * (items - 1))`.
    pub density: f64,
}

/// Computes intra-module cohesion for every module, ordered from least to
/// most cohesive. Low density with many items signals a low-cohesion
/// "junk drawer" module.
pub(crate) fn cohesion(nodes: &[ModuleNode]) -> Vec<CohesionMetric> {
    let mut metrics: Vec<CohesionMetric> = nodes
        .iter()
        .map(|node| {
            let items = node.item_count;
            let internal_refs = crate::collect::collect_internal_refs(node);
            let density = if items > 1 {
                internal_refs as f64 / (items * (items - 1)) as f64
            } else {
                1.0
            };
            CohesionMetric {
                module: node.path.clone(),
                items,
                internal_refs,
                density,
            }
        })
        .collect();
    metrics.sort_by(|a, b| {
        a.density
            .partial_cmp(&b.density)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    metrics
}

/// Modules nested deeper than `threshold` path segments, deepest first.
pub(crate) fn deep_nesting(modules: &HashSet<String>, threshold: usize) -> Vec<(String, usize)> {
    let mut deep: Vec<(String, usize)> = modules
        .iter()
        .map(|module| (module.clone(), module.split("::").count()))
        .filter(|(_, depth)| *depth > threshold)
        .collect();
    deep.sort_by_key(|(_, depth)| std::cmp::Reverse(*depth));
    deep
}

/// Edges from a stable module (`I < 0.5`) to an unstable one (`I > 0.5`), a
/// classic design smell (a stable abstraction depending on a volatile detail).
pub(crate) fn unstable_dependencies(
    uses: &HashMap<String, HashSet<String>>,
    instability: &HashMap<String, f64>,
) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for (from, targets) in uses {
        let from_instability = *instability.get(from).unwrap_or(&0.5);
        for to in targets {
            let to_instability = *instability.get(to).unwrap_or(&0.5);
            if from_instability < 0.5 && to_instability > 0.5 {
                edges.push((from.clone(), to.clone()));
            }
        }
    }
    edges.sort();
    edges
}

/// Finds, for each cycle, the edges whose removal breaks that cycle (the
/// "bridge" edges with no alternative path). Sorted and de-duplicated.
pub(crate) fn cycle_break_edges(
    uses: &HashMap<String, HashSet<String>>,
    cycles: &[Cycle],
) -> Vec<(String, String)> {
    let mut edges = Vec::new();
    for cycle in cycles {
        if cycle.len() <= 1 {
            continue;
        }
        let members: HashSet<&str> = cycle.iter().map(String::as_str).collect();
        for from in cycle {
            let Some(targets) = uses.get(from) else {
                continue;
            };
            for to in targets {
                if from == to || !members.contains(to.as_str()) {
                    continue;
                }
                if !has_alternative_path(from, to, uses, &members) {
                    edges.push((from.clone(), to.clone()));
                }
            }
        }
    }
    edges.sort();
    edges.dedup();
    edges
}

/// Whether `from` can still reach `to` through the cycle without using the
/// direct `from -> to` edge.
fn has_alternative_path(
    from: &str,
    to: &str,
    uses: &HashMap<String, HashSet<String>>,
    members: &HashSet<&str>,
) -> bool {
    let mut visited: HashSet<&str> = HashSet::new();
    visited.insert(from);
    let mut stack: Vec<&str> = Vec::new();
    if let Some(targets) = uses.get(from) {
        for next in targets {
            if next != to && members.contains(next.as_str()) && visited.insert(next) {
                stack.push(next);
            }
        }
    }
    while let Some(node) = stack.pop() {
        if node == to {
            return true;
        }
        if let Some(targets) = uses.get(node) {
            for next in targets {
                if members.contains(next.as_str()) && visited.insert(next) {
                    stack.push(next);
                }
            }
        }
    }
    false
}
