//! Compatibility router that plans and executes format/protocol migrations.

use crate::error::MigrateError;
use crate::handler::{MigrateHandler, MigrateOption};
use mudu::compat::{FormatKind, VersionRange};
use std::collections::{HashMap, VecDeque};
use std::fmt;

/// Provides versioned auxiliary payloads for individual migration steps.
///
/// Because the structure of `MigrateOption` itself can evolve, the router asks
/// for an option for each `(component, version)` pair rather than reusing a
/// single payload across the whole chain.
pub trait OptionProvider {
    /// Returns the auxiliary payload appropriate for `component` at `version`.
    fn get(&self, component: FormatKind, version: u32) -> Option<MigrateOption>;
}

/// An option provider that never returns a payload.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopOptionProvider;

impl OptionProvider for NoopOptionProvider {
    fn get(&self, _component: FormatKind, _version: u32) -> Option<MigrateOption> {
        None
    }
}

/// Direction of a single migration step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EdgeKind {
    /// Moving from the handler's `from` to its `to`.
    Upgrade,
    /// Moving from the handler's `to` to its `from`.
    Rollback,
}

/// A single directed edge in the migration graph.
#[derive(Clone)]
struct Edge {
    from: u32,
    to: u32,
    kind: EdgeKind,
    handler: MigrateHandler,
}

/// Plans and executes migrations between format versions.
#[derive(Debug, Default, Clone)]
pub struct CompatibilityRouter {
    current_versions: HashMap<FormatKind, u32>,
    min_supported_versions: HashMap<FormatKind, u32>,
    handlers: HashMap<FormatKind, Vec<MigrateHandler>>,
}

impl CompatibilityRouter {
    /// Creates an empty router.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the current/expected version for a component.
    pub fn set_current_version(&mut self, component: FormatKind, version: u32) {
        self.current_versions.insert(component, version);
    }

    /// Sets the lowest supported version for a component.
    ///
    /// Versions below this value are rejected with [`MigrateError::UnsupportedVersion`].
    pub fn set_min_supported_version(&mut self, component: FormatKind, version: u32) {
        self.min_supported_versions.insert(component, version);
    }

    /// Convenience to set both ends of the supported window at once.
    pub fn set_supported_window(&mut self, component: FormatKind, min: u32, current: u32) {
        self.min_supported_versions.insert(component, min);
        self.current_versions.insert(component, current);
    }

    /// Returns the current version for `component`, if any.
    pub fn current_version(&self, component: FormatKind) -> Option<u32> {
        self.current_versions.get(&component).copied()
    }

    /// Returns the minimum supported version for `component`, defaulting to `1`.
    pub fn min_supported_version(&self, component: FormatKind) -> u32 {
        self.min_supported_versions
            .get(&component)
            .copied()
            .unwrap_or(1)
    }

    /// Returns the supported version window for `component`.
    pub fn supported_window(&self, component: FormatKind) -> Option<VersionRange> {
        let min = self.min_supported_version(component);
        let max = self.current_version(component)?;
        Some(VersionRange::new(min, max))
    }

    /// Registers a migration handler for a component.
    pub fn register(&mut self, component: FormatKind, handler: MigrateHandler) {
        self.handlers.entry(component).or_default().push(handler);
    }

    /// Migrates `binary` from `from_version` to `to_version`.
    ///
    /// The router first validates that both versions fall inside the supported
    /// window, then searches for the shortest chain of registered handlers and
    /// applies it.
    pub fn migrate(
        &self,
        component: FormatKind,
        from_version: u32,
        to_version: u32,
        binary: &[u8],
        options: &dyn OptionProvider,
    ) -> Result<Vec<u8>, MigrateError> {
        if from_version == to_version {
            return Ok(binary.to_vec());
        }

        self.check_version(component, from_version)?;
        self.check_version(component, to_version)?;

        let path = self.find_path(component, from_version, to_version)?;
        let mut current = binary.to_vec();
        for (step, edge) in path.iter().enumerate() {
            let opt = options.get(component, edge.from);
            let result = match edge.kind {
                EdgeKind::Upgrade => (edge.handler.upgrade)(&current, opt.as_ref()),
                EdgeKind::Rollback => (edge.handler.rollback)(&current, opt.as_ref()),
            }
            .map_err(|e| match e {
                MigrateError::MigrationFailed { .. } => e,
                _ => MigrateError::MigrationFailed {
                    component,
                    from: edge.from,
                    to: edge.to,
                    step,
                    source: e.to_string(),
                },
            })?;
            current = result;
        }
        Ok(current)
    }

    /// Upgrades `binary` from `from_version` to the current version.
    pub fn upgrade_to_current(
        &self,
        component: FormatKind,
        from_version: u32,
        binary: &[u8],
        options: &dyn OptionProvider,
    ) -> Result<Vec<u8>, MigrateError> {
        let current =
            self.current_version(component)
                .ok_or_else(|| MigrateError::UnsupportedVersion {
                    component,
                    actual: from_version,
                    supported: VersionRange::new(0, 0),
                })?;
        self.migrate(component, from_version, current, binary, options)
    }

    /// Rolls `binary` back from the current version to `to_version`.
    pub fn rollback_from_current(
        &self,
        component: FormatKind,
        to_version: u32,
        binary: &[u8],
        options: &dyn OptionProvider,
    ) -> Result<Vec<u8>, MigrateError> {
        let current =
            self.current_version(component)
                .ok_or_else(|| MigrateError::UnsupportedVersion {
                    component,
                    actual: to_version,
                    supported: VersionRange::new(0, 0),
                })?;
        self.migrate(component, current, to_version, binary, options)
    }

    /// Validates that a migration path exists without actually running it.
    pub fn validate_path(
        &self,
        component: FormatKind,
        from_version: u32,
        to_version: u32,
    ) -> Result<usize, MigrateError> {
        if from_version == to_version {
            return Ok(0);
        }
        self.check_version(component, from_version)?;
        self.check_version(component, to_version)?;
        let path = self.find_path(component, from_version, to_version)?;
        Ok(path.len())
    }

    fn check_version(&self, component: FormatKind, version: u32) -> Result<(), MigrateError> {
        let min = self.min_supported_version(component);
        let current = self.current_version(component).unwrap_or(min);
        let range = VersionRange::new(min, current);
        if range.contains(version) {
            Ok(())
        } else {
            Err(MigrateError::UnsupportedVersion {
                component,
                actual: version,
                supported: range,
            })
        }
    }

    fn component_edges(&self, component: FormatKind) -> Vec<Edge> {
        let min = self.min_supported_version(component);
        let max = self.current_version(component).unwrap_or(min);
        let mut edges = Vec::new();
        if let Some(handlers) = self.handlers.get(&component) {
            for handler in handlers {
                // Only expose edges that stay inside the supported window.
                if handler.from >= min
                    && handler.from <= max
                    && handler.to >= min
                    && handler.to <= max
                {
                    edges.push(Edge {
                        from: handler.from,
                        to: handler.to,
                        kind: EdgeKind::Upgrade,
                        handler: handler.clone(),
                    });
                    edges.push(Edge {
                        from: handler.to,
                        to: handler.from,
                        kind: EdgeKind::Rollback,
                        handler: handler.clone(),
                    });
                }
            }
        }
        edges
    }

    fn find_path(
        &self,
        component: FormatKind,
        from: u32,
        to: u32,
    ) -> Result<Vec<Edge>, MigrateError> {
        let edges = self.component_edges(component);
        let mut adjacency: HashMap<u32, Vec<Edge>> = HashMap::new();
        for edge in edges {
            adjacency.entry(edge.from).or_default().push(edge);
        }

        let mut visited: HashMap<u32, Edge> = HashMap::new();
        let mut queue: VecDeque<u32> = VecDeque::new();
        queue.push_back(from);

        while let Some(version) = queue.pop_front() {
            if version == to {
                break;
            }
            if let Some(outgoing) = adjacency.get(&version) {
                for edge in outgoing {
                    if !visited.contains_key(&edge.to) && edge.to != from {
                        visited.insert(edge.to, edge.clone());
                        queue.push_back(edge.to);
                    }
                }
            }
        }

        if !visited.contains_key(&to) && from != to {
            return Err(MigrateError::MissingHandler {
                component,
                from,
                to,
            });
        }

        // Reconstruct path from `to` back to `from`.
        let mut path = Vec::new();
        let mut cursor = to;
        while cursor != from {
            let edge = visited
                .get(&cursor)
                .cloned()
                .ok_or(MigrateError::MissingHandler {
                    component,
                    from,
                    to,
                })?;
            cursor = edge.from;
            path.push(edge);
        }
        path.reverse();
        Ok(path)
    }
}

impl fmt::Debug for Edge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Edge")
            .field("from", &self.from)
            .field("to", &self.to)
            .field("kind", &self.kind)
            .finish()
    }
}

/// Global compatibility router used by format decode entry points.
///
/// The router is installed once at application startup (usually by
/// `mudu_kernel::compat::install_compatibility_router`). After installation it
/// is immutable and can be accessed from any crate without threading a router
/// reference through every call stack.
pub mod global {
    use super::*;
    use std::sync::OnceLock;

    static ROUTER: OnceLock<CompatibilityRouter> = OnceLock::new();

    /// Installs the global router. Returns `Err(router)` if a router has
    /// already been installed.
    pub fn install(router: CompatibilityRouter) -> Result<(), Box<CompatibilityRouter>> {
        ROUTER.set(router).map_err(Box::new)
    }

    /// Returns the installed router, if any.
    pub fn router() -> Option<&'static CompatibilityRouter> {
        ROUTER.get()
    }

    /// Returns `true` if a router has been installed.
    pub fn is_installed() -> bool {
        ROUTER.get().is_some()
    }

    /// Returns the current version for `component` according to the installed
    /// router, if any.
    pub fn current_version(component: FormatKind) -> Option<u32> {
        router().and_then(|r| r.current_version(component))
    }

    /// Upgrades `binary` from `from_version` to the current version using the
    /// installed router.
    ///
    /// Fails if no router is installed or if no migration path exists.
    pub fn upgrade_to_current(
        component: FormatKind,
        from_version: u32,
        binary: &[u8],
        options: &dyn OptionProvider,
    ) -> Result<Vec<u8>, MigrateError> {
        let router = router().ok_or_else(|| MigrateError::MigrationFailed {
            component,
            from: from_version,
            to: 0,
            step: 0,
            source: "compatibility router is not installed".to_string(),
        })?;
        router.upgrade_to_current(component, from_version, binary, options)
    }
}

#[cfg(test)]
#[path = "router_test.rs"]
mod router_test;
