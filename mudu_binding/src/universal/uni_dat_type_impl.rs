use crate::universal::uni_dat_type::UniDatType;
use crate::universal::uni_primitive::UniPrimitive;
use mudu::common::into_result::ToResult;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dtp_array::DTPArray;
use mudu_type::dtp_kind::DTPKind;
use mudu_type::dtp_object::DTPRecord;
use std::collections::{HashMap, HashSet, VecDeque};

use crate::universal::uni_record_type::{UniRecordField, UniRecordType};

impl UniDatType {
    pub fn uni_to(self) -> RS<DatType> {
        let ty = match self {
            UniDatType::Primitive(primitive) => primitive.to()?,
            UniDatType::Array(inner) => {
                let ty = Box::into_inner(inner);
                let inner_ty = ty.uni_to()?;
                let array_kind = DTPKind::Array(Box::new(DTPArray::new(inner_ty)));
                DatType::from_id_param(DatTypeID::Array, Some(array_kind))
            }
            UniDatType::Record(inner) => {
                let mut fields = Vec::with_capacity(inner.record_fields.len());
                for field in inner.record_fields {
                    let field_ty = field.field_type.uni_to()?;
                    fields.push((field.field_name, field_ty));
                }
                let object_kind =
                    DTPKind::Record(Box::new(DTPRecord::new(inner.record_name.clone(), fields)));
                DatType::from_id_param(DatTypeID::Record, Some(object_kind))
            }
            _ => {
                return Err(m_error!(EC::TypeErr, "not supported type"));
            }
        };
        Ok(ty)
    }

    pub fn uni_from(ty: DatType) -> RS<UniDatType> {
        let uni_ty = if ty.dat_type_id().is_primitive_type() {
            let primitive = UniPrimitive::uni_from(ty)?;
            UniDatType::Primitive(primitive)
        } else {
            match ty.dat_type_id() {
                DatTypeID::Array => {
                    let param = ty.into_array_param();
                    let inner_ty = param.into_dat_type();
                    let inner_uni_ty = Self::uni_from(inner_ty)?;
                    UniDatType::Array(Box::new(inner_uni_ty))
                }
                DatTypeID::Record => {
                    let param = ty.into_record_param();
                    let mut record_fields = Vec::with_capacity(param.fields().len());
                    let (record_name, fields) = param.into();
                    for (name, f_ty) in fields {
                        let mu_f_ty = Self::uni_from(f_ty)?;
                        let field = UniRecordField {
                            field_name: name,
                            field_type: mu_f_ty,
                        };
                        record_fields.push(field);
                    }
                    let record = UniRecordType {
                        record_name,
                        record_fields,
                    };
                    UniDatType::Record(record)
                }
                _ => {
                    return Err(m_error!(EC::TypeErr, "unexpected type".to_string()));
                }
            }
        };
        Ok(uni_ty)
    }

    pub fn rewrite_inline(vec: Vec<Self>) -> RS<Vec<Self>> {
        _rewrite_inline(vec)
    }
}

fn _rewrite_inline(vec_ty: Vec<UniDatType>) -> RS<Vec<UniDatType>> {
    let mut record_ty = HashMap::new();
    let mut dependency = HashMap::new();
    for (i, ty) in vec_ty.iter().enumerate() {
        visit_ty_root(i as u64, ty, &mut record_ty, &mut dependency);
    }
    let mut dependency_index = HashMap::<u64, HashSet<u64>>::new();
    for (u, set) in dependency.iter() {
        for record_name in set.iter() {
            let v = record_ty.get(record_name).map_or_else(
                || {
                    Err(m_error!(
                        EC::NoneErr,
                        format!("No data type named {}", record_name)
                    ))
                },
                |v| Ok(*v),
            )?;
            // `u` requires `v` to be processed before it.
            dependency_index
                .entry(*u)
                .or_insert(Default::default())
                .insert(v);
        }
    }
    let mut records = HashMap::<String, UniDatType>::new();
    let mut vec_ty = vec_ty;
    let ordered_index = topological_sort(dependency_index);
    for index in ordered_index.iter() {
        let ty = &mut vec_ty[*index as usize];
        build_inline(ty, &mut records)?;
    }
    Ok(vec_ty)
}

fn build_inline(ty: &mut UniDatType, records: &mut HashMap<String, UniDatType>) -> RS<()> {
    match ty {
        UniDatType::Identifier(record_name) => {
            let record_ty = records.get(record_name).map_or_else(
                || {
                    Err(m_error!(
                        EC::NoneErr,
                        format!("no such record named {}", record_name)
                    ))
                },
                |t| Ok(t.clone()),
            )?;
            *ty = record_ty;
        }
        UniDatType::Record(record) => {
            let name = record.record_name.clone();
            records.insert(name, ty.clone());
        }
        UniDatType::Tuple(tuple) => {
            for field_ty in tuple.iter_mut() {
                build_inline(field_ty, records)?;
            }
        }
        UniDatType::Array(array_type) => {
            build_inline(array_type, records)?;
        }
        UniDatType::Option(option_type) => {
            build_inline(option_type, records)?;
        }
        UniDatType::Result(result) => {
            if let Some(ok) = &mut result.ok {
                build_inline(ok, records)?;
            }
            if let Some(err) = &mut result.err {
                build_inline(err, records)?;
            }
        }
        UniDatType::Box(box_type) => {
            build_inline(box_type, records)?;
        }
        _ => {}
    }
    Ok(())
}
fn visit_ty_root(
    index: u64,
    ty: &UniDatType,
    record_ty: &mut HashMap<String, u64>,
    dependency: &mut HashMap<u64, HashSet<String>>,
) {
    visit_ty(index, ty, record_ty, dependency);
}

fn visit_ty(
    index: u64,
    ty: &UniDatType,
    record_ty: &mut HashMap<String, u64>,
    dependency: &mut HashMap<u64, HashSet<String>>,
) {
    match ty {
        UniDatType::Identifier(ident_name) => {
            if let Some(set) = dependency.get_mut(&index) {
                set.insert(ident_name.clone());
            } else {
                let mut set = HashSet::new();
                set.insert(ident_name.clone());
                dependency.insert(index, set);
            }
        }
        UniDatType::Record(uni_record) => {
            let record_name = uni_record.record_name.clone();
            record_ty.insert(record_name.clone(), index);
            for field in uni_record.record_fields.iter() {
                visit_ty(index, &field.field_type, record_ty, dependency);
            }
        }
        UniDatType::Tuple(tuple) => {
            for field_ty in tuple.iter() {
                visit_ty(index, field_ty, record_ty, dependency);
            }
        }
        UniDatType::Array(array_type) => {
            visit_ty(index, array_type.as_ref(), record_ty, dependency);
        }
        UniDatType::Option(option_type) => {
            visit_ty(index, option_type, record_ty, dependency);
        }
        UniDatType::Result(result) => {
            if let Some(ok) = &result.ok {
                visit_ty(index, ok.as_ref(), record_ty, dependency);
            }
            if let Some(err) = &result.err {
                visit_ty(index, err.as_ref(), record_ty, dependency);
            }
        }
        UniDatType::Box(box_type) => {
            visit_ty(index, box_type.as_ref(), record_ty, dependency);
        }
        UniDatType::Primitive(_) => {}
        UniDatType::Binary => {}
    }
}

/// Reverses the direction of dependency edges in a graph.
///
/// # Arguments
/// * `dependency` - A HashMap where the key is a node, and the value is a set of nodes that the key depends on.
///                  Specifically, `dependency[u] = {v1, v2, ...}` means `u` requires `v1, v2, ...` to be processed before it.
///                  This represents directed edges from each vᵢ to u (vᵢ -> u).
///
/// # Returns
/// * `HashMap<u64, HashSet<u64>>` - A reversed adjacency list where:
///     - The key is a node v
///     - The value is a set of nodes {u₁, u₂, ...} that depend on v
///     - This represents directed edges from v to each uᵢ (v -> uᵢ)
///
/// ```
fn reverse_adj(dependency: &HashMap<u64, HashSet<u64>>) -> HashMap<u64, HashSet<u64>> {
    let mut reversed = HashMap::<u64, HashSet<u64>>::new();

    // Iterate through each node and its dependencies
    for (&dependent_node, dependencies) in dependency {
        // For each node that the current node depends on
        for &dependency_node in dependencies {
            // Add the current node to the set of nodes that depend on dependency_node
            reversed
                .entry(dependency_node)
                .or_insert_with(HashSet::new)
                .insert(dependent_node);
        }

        // Ensure all nodes appear in the reversed map, even if they have no incoming edges
        // This is optional but can be useful for completeness
        reversed.entry(dependent_node).or_insert_with(HashSet::new);
    }

    // Also ensure that nodes that only appear as dependencies (not as keys)
    // are included in the reversed map
    for dependencies in dependency.values() {
        for &node in dependencies {
            reversed.entry(node).or_insert_with(HashSet::new);
        }
    }

    reversed
}

/// Performs topological sorting on a directed graph represented by dependencies.
///
/// # Arguments
/// * `dependency` - A HashMap where the key is a node, and the value is a set of nodes that the key depends on.
///                  In other words, `dependency[u] = {v1, v2, ...}` means `u` requires `v1, v2, ...` to be processed before it.
///
/// # Returns
/// * `Vec<u64>` - A topological ordering of nodes if the graph is acyclic.
///                If a cycle is detected, returns an empty vector.
///
/// # Algorithm
/// This implementation uses Kahn's algorithm:
/// 1. Compute in-degree for each node (number of incoming edges).
/// 2. Initialize a queue with all nodes having zero in-degree.
/// 3. Repeatedly remove nodes from the queue, add them to the result,
///    and decrease the in-degree of their dependents.
/// 4. If all nodes are processed, return the topological order.
///    Otherwise, a cycle exists.
fn topological_sort(dependency: HashMap<u64, HashSet<u64>>) -> Vec<u64> {
    // If the graph is empty, return an empty vector immediately
    if dependency.is_empty() {
        return Vec::new();
    }

    // Step 1: Collect all nodes from both keys and values
    let mut all_nodes = HashSet::new();

    // Add all nodes that appear as keys
    for &node in dependency.keys() {
        all_nodes.insert(node);
    }

    // Add all nodes that appear as dependencies
    for deps in dependency.values() {
        for &node in deps {
            all_nodes.insert(node);
        }
    }

    // Step 2: Compute in-degree for each node
    let mut in_degree: HashMap<u64, usize> = HashMap::new();

    // Initialize all in-degrees to 0
    for &node in &all_nodes {
        in_degree.insert(node, 0);
    }

    // Calculate actual in-degrees
    let reserve_dependency = reverse_adj(&dependency);
    for (node, deps) in &reserve_dependency {
        for dep in deps {
            *in_degree.entry(dep.clone()).or_insert(0) += 1;
        }
        // Ensure the key node itself is in the map (with its current in-degree or 0)
        in_degree.entry(*node).or_insert(0);
    }

    // Step 3: Initialize queue with nodes having zero in-degree
    let mut queue: VecDeque<u64> = VecDeque::new();
    for (node, degree) in in_degree.iter_mut() {
        if *degree == 0 {
            queue.push_back(*node);
        }
    }
    for i in queue.iter() {
        in_degree.remove(&i);
    }

    // Step 4: Process nodes in topological order
    let mut result = Vec::new();
    let mut remaining_deps = reserve_dependency;

    while let Some(current) = queue.pop_front() {
        result.push(current);

        // Also check if current node itself has dependencies that are now satisfied
        // (This handles nodes that only appear as keys, not as dependencies)
        if remaining_deps.contains_key(&current) {
            if let Some(deps) = remaining_deps.get(&current) {
                if deps.is_empty() {
                    remaining_deps.remove(&current);
                } else {
                    for dependent in deps {
                        // Decrease in-degree of the dependent
                        if let Some(degree) = in_degree.get_mut(&dependent) {
                            *degree -= 1;
                            // If in-degree becomes zero, add to queue
                            if *degree == 0 {
                                queue.push_back(*dependent);
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 5: Check if topological ordering includes all nodes
    if result.len() == all_nodes.len() {
        result
    } else {
        // Cycle detected
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::universal::uni_dat_type_impl::topological_sort;
    use std::collections::{HashMap, HashSet};

    #[test]
    fn test_empty_dependency() {
        let dependency: HashMap<u64, HashSet<u64>> = HashMap::new();
        let reversed = reverse_adj(&dependency);
        assert!(reversed.is_empty());
    }

    #[test]
    fn test_single_node_no_dependencies() {
        let mut dependency = HashMap::new();
        dependency.insert(1, HashSet::new());

        let reversed = reverse_adj(&dependency);
        assert_eq!(reversed.len(), 1);
        assert!(reversed.contains_key(&1));
        assert!(reversed[&1].is_empty());
    }

    #[test]
    fn test_simple_dependency_chain() {
        let mut dependency = HashMap::new();
        dependency.insert(1, HashSet::from([2])); // 1 depends on 2
        dependency.insert(2, HashSet::from([3])); // 2 depends on 3
        dependency.insert(3, HashSet::new()); // 3 has no dependencies

        let reversed = reverse_adj(&dependency);

        // Check all nodes are present
        assert_eq!(reversed.len(), 3);
        assert!(reversed.contains_key(&1));
        assert!(reversed.contains_key(&2));
        assert!(reversed.contains_key(&3));

        // Check reversed edges
        // Original: 2->1, 3->2
        // Reversed: 1->2, 2->3
        assert_eq!(reversed[&1], HashSet::new()); // Nothing depends on 1
        assert_eq!(reversed[&2], HashSet::from([1])); // 1 depends on 2
        assert_eq!(reversed[&3], HashSet::from([2])); // 2 depends on 3
    }

    #[test]
    fn test_multiple_dependents() {
        let mut dependency = HashMap::new();
        dependency.insert(1, HashSet::from([3])); // 1 depends on 3
        dependency.insert(2, HashSet::from([3])); // 2 depends on 3
        dependency.insert(3, HashSet::new()); // 3 has no dependencies

        let reversed = reverse_adj(&dependency);

        // Check reversed edges
        assert_eq!(reversed[&1], HashSet::new()); // Nothing depends on 1
        assert_eq!(reversed[&2], HashSet::new()); // Nothing depends on 2
        assert_eq!(reversed[&3], HashSet::from([1, 2])); // Both 1 and 2 depend on 3
    }

    #[test]
    fn test_diamond_structure() {
        let mut dependency = HashMap::new();
        dependency.insert(1, HashSet::from([2, 3])); // 1 depends on 2 and 3
        dependency.insert(2, HashSet::from([4])); // 2 depends on 4
        dependency.insert(3, HashSet::from([4])); // 3 depends on 4
        dependency.insert(4, HashSet::new()); // 4 has no dependencies

        let reversed = reverse_adj(&dependency);

        // Check all nodes are present
        assert_eq!(reversed.len(), 4);

        // Check reversed edges
        // Original: 2->1, 3->1, 4->2, 4->3
        // Reversed: 1->{2,3}, 2->4, 3->4, 4->{}
        assert_eq!(reversed[&1], HashSet::new()); // Nothing depends on 1
        assert_eq!(reversed[&2], HashSet::from([1])); // 1 depends on 2
        assert_eq!(reversed[&3], HashSet::from([1])); // 1 depends on 3
        assert_eq!(reversed[&4], HashSet::from([2, 3])); // Both 2 and 3 depend on 4
    }

    #[test]
    fn test_cycle_dependency() {
        let mut dependency = HashMap::new();
        dependency.insert(1, HashSet::from([2])); // 1 depends on 2
        dependency.insert(2, HashSet::from([3])); // 2 depends on 3
        dependency.insert(3, HashSet::from([1])); // 3 depends on 1 (cycle)

        let reversed = reverse_adj(&dependency);

        // Check all nodes are present
        assert_eq!(reversed.len(), 3);

        // Check reversed edges
        // Original: 2->1, 3->2, 1->3
        // Reversed: 1->{3}, 2->{1}, 3->{2}
        assert_eq!(reversed[&1], HashSet::from([3])); // 3 depends on 1
        assert_eq!(reversed[&2], HashSet::from([1])); // 1 depends on 2
        assert_eq!(reversed[&3], HashSet::from([2])); // 2 depends on 3
    }

    #[test]
    fn test_nodes_only_as_dependencies() {
        let mut dependency = HashMap::new();
        dependency.insert(1, HashSet::from([2])); // 1 depends on 2
        // Node 2 only appears as a dependency, not as a key

        let reversed = reverse_adj(&dependency);

        // Both nodes should be present
        assert_eq!(reversed.len(), 2);
        assert!(reversed.contains_key(&1));
        assert!(reversed.contains_key(&2));

        // Check reversed edges
        assert_eq!(reversed[&1], HashSet::new()); // Nothing depends on 1
        assert_eq!(reversed[&2], HashSet::from([1])); // 1 depends on 2
    }

    #[test]
    fn test_empty_graph() {
        let deps: HashMap<u64, HashSet<u64>> = HashMap::new();
        let result = topological_sort(deps);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_node() {
        let mut deps = HashMap::new();
        deps.insert(1, HashSet::new());
        let result = topological_sort(deps);
        assert_eq!(result, vec![1]);
    }

    #[test]
    fn test_linear_dependencies() {
        let mut deps = HashMap::new();
        deps.insert(1, HashSet::from([2]));
        deps.insert(2, HashSet::from([3]));
        deps.insert(3, HashSet::new());

        let result = topological_sort(deps);
        // Valid order: 3 -> 2 -> 1
        assert!(result == vec![3, 2, 1] || result == vec![2, 3, 1]);
        // Check relative ordering constraints
        let pos: HashMap<_, _> = result.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        assert!(pos[&3] < pos[&2]);
        assert!(pos[&2] < pos[&1]);
    }

    #[test]
    fn test_diamond_dependencies() {
        let mut deps = HashMap::new();
        deps.insert(1, HashSet::from([2, 3]));
        deps.insert(2, HashSet::from([4]));
        deps.insert(3, HashSet::from([4]));
        deps.insert(4, HashSet::new());

        let result = topological_sort(deps);
        // 4 must come before both 2 and 3, and 2/3 must come before 1
        let pos: HashMap<_, _> = result.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        assert!(pos[&4] < pos[&2]);
        assert!(pos[&4] < pos[&3]);
        assert!(pos[&2] < pos[&1]);
        assert!(pos[&3] < pos[&1]);
    }

    #[test]
    fn test_cycle_detection() {
        let mut deps = HashMap::new();
        deps.insert(1, HashSet::from([2]));
        deps.insert(2, HashSet::from([3]));
        deps.insert(3, HashSet::from([1])); // Cycle: 1 -> 2 -> 3 -> 1

        let result = topological_sort(deps);
        assert!(result.is_empty()); // Should return empty vec for cycle
    }

    #[test]
    fn test_disconnected_components() {
        let mut deps = HashMap::new();
        deps.insert(1, HashSet::from([2]));
        deps.insert(2, HashSet::new());
        deps.insert(3, HashSet::from([4]));
        deps.insert(4, HashSet::new());

        let result = topological_sort(deps);
        assert_eq!(result.len(), 4);
        let pos: HashMap<_, _> = result.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        assert!(pos[&2] < pos[&1]);
        assert!(pos[&4] < pos[&3]);
    }

    #[test]
    fn test_isolated_node() {
        let mut deps = HashMap::new();
        deps.insert(1, HashSet::from([2]));
        deps.insert(2, HashSet::new());
        deps.insert(3, HashSet::new()); // Isolated node, only appears as key

        let result = topological_sort(deps);
        assert_eq!(result.len(), 3);
        // All nodes should be present
        assert!(result.contains(&1));
        assert!(result.contains(&2));
        assert!(result.contains(&3));
        // Check ordering constraint
        let pos: HashMap<_, _> = result.iter().enumerate().map(|(i, &v)| (v, i)).collect();
        assert!(pos[&2] < pos[&1]);
    }
}
