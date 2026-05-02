//! § resolver — recursive dependency resolution with cycle detection.
//!
//! § DESIGN
//!   Given a `Manifest` (the root) and a `PackageRegistry` mapping
//!   `(id, version) → Manifest`, walk the `depends_on` DAG transitively to
//!   produce the topologically-ordered list of `RequiredPackage`s the
//!   client must fetch + install before the root can run.
//!
//!   Cycle-detection uses the canonical DFS-3-coloring algorithm :
//!     WHITE = unvisited
//!     GRAY  = currently on the recursion stack
//!     BLACK = finished
//!
//!   Encountering a GRAY node during DFS = a back-edge = a cycle, fatal.
//!
//! § COMPLEXITY
//!   O(V + E) where V = unique packages, E = depends_on edges.
//!   Cycle-detect adds O(V) memory for the color map.

use crate::manifest::{Dependency, Manifest};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;

/// § Resolution outcome : a single (id, version) the client must install.
/// Exposed in topological order (deps-before-dependents).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct RequiredPackage {
    pub id: String,
    pub version: String,
}

/// § Errors during dependency resolution.
#[derive(Debug, Error, PartialEq, Eq)]
pub enum ResolveError {
    #[error("cycle detected involving package '{0}@{1}'")]
    Cycle(String, String),
    #[error("missing package '{0}@{1}' in registry")]
    Missing(String, String),
}

/// § Lookup interface : given an `(id, version)` key, return the manifest
/// describing that package. Implementations may pull from a local cache,
/// a remote service (e.g. apocky.com discovery), or both.
pub trait PackageRegistry {
    fn lookup(&self, id: &str, version: &str) -> Option<&Manifest>;
}

/// § Simple in-memory `BTreeMap` registry, used by tests + small clients.
#[derive(Debug, Default, Clone)]
pub struct InMemoryRegistry {
    packages: BTreeMap<(String, String), Manifest>,
}

impl InMemoryRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a manifest. The (id, version) key is computed from the manifest.
    pub fn insert(&mut self, manifest: Manifest) {
        let key = (manifest.id.clone(), manifest.version.clone());
        self.packages.insert(key, manifest);
    }
}

impl PackageRegistry for InMemoryRegistry {
    fn lookup(&self, id: &str, version: &str) -> Option<&Manifest> {
        self.packages.get(&(id.to_string(), version.to_string()))
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
enum Color {
    White,
    Gray,
    Black,
}

/// § Recursive dependency resolver with cycle detection.
///
/// Returns the topologically-ordered (deps-before-dependents) list of
/// `RequiredPackage`s, EXCLUDING the root itself. Caller can prepend the
/// root to the list if they want a complete install plan.
pub fn package_dependencies_resolve<R: PackageRegistry>(
    root: &Manifest,
    registry: &R,
) -> Result<Vec<RequiredPackage>, ResolveError> {
    let mut color: BTreeMap<(String, String), Color> = BTreeMap::new();
    let mut order: Vec<RequiredPackage> = Vec::new();
    let mut seen: BTreeSet<(String, String)> = BTreeSet::new();

    // Root is colored gray during the DFS but excluded from `order`.
    let root_key = (root.id.clone(), root.version.clone());
    color.insert(root_key.clone(), Color::Gray);

    for dep in &root.depends_on {
        dfs_visit(dep, registry, &mut color, &mut order, &mut seen)?;
    }
    color.insert(root_key, Color::Black);

    Ok(order)
}

fn dfs_visit<R: PackageRegistry>(
    dep: &Dependency,
    registry: &R,
    color: &mut BTreeMap<(String, String), Color>,
    order: &mut Vec<RequiredPackage>,
    seen: &mut BTreeSet<(String, String)>,
) -> Result<(), ResolveError> {
    let key = (dep.id.clone(), dep.version.clone());

    match color.get(&key).copied().unwrap_or(Color::White) {
        Color::Gray => {
            // Back-edge → cycle.
            return Err(ResolveError::Cycle(dep.id.clone(), dep.version.clone()));
        }
        Color::Black => {
            // Already fully resolved.
            return Ok(());
        }
        Color::White => {}
    }

    color.insert(key.clone(), Color::Gray);

    let manifest = registry
        .lookup(&dep.id, &dep.version)
        .ok_or_else(|| ResolveError::Missing(dep.id.clone(), dep.version.clone()))?;

    for child in &manifest.depends_on {
        dfs_visit(child, registry, color, order, seen)?;
    }

    color.insert(key.clone(), Color::Black);

    if seen.insert(key.clone()) {
        order.push(RequiredPackage {
            id: key.0,
            version: key.1,
        });
    }
    Ok(())
}

/// § Walk a remix-attribution chain back to its root, returning the chain
/// in deepest-first order. Cycle-detected to prevent malicious infinite
/// loops in the chain.
pub fn remix_chain_walk<R: PackageRegistry>(
    root: &Manifest,
    registry: &R,
) -> Result<Vec<RequiredPackage>, ResolveError> {
    let mut chain = Vec::new();
    let mut visited: BTreeSet<(String, String)> = BTreeSet::new();
    let mut current = root.clone();
    while let Some(remix) = current.remix_of.clone() {
        let key = (remix.id.clone(), remix.version.clone());
        if !visited.insert(key.clone()) {
            return Err(ResolveError::Cycle(remix.id, remix.version));
        }
        chain.push(RequiredPackage {
            id: remix.id.clone(),
            version: remix.version.clone(),
        });
        let next = registry
            .lookup(&remix.id, &remix.version)
            .ok_or_else(|| ResolveError::Missing(remix.id.clone(), remix.version.clone()))?;
        current = next.clone();
    }
    Ok(chain)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kind::ContentKind;
    use crate::manifest::{LicenseTier, Manifest, RemixAttribution};

    fn manifest_for(
        id: &str,
        version: &str,
        deps: Vec<(&str, &str)>,
        remix_of: Option<(&str, &str)>,
    ) -> Manifest {
        Manifest {
            id: id.to_string(),
            version: version.to_string(),
            kind: ContentKind::Scene,
            author_pubkey: [0u8; 32],
            name: id.to_string(),
            description: String::new(),
            depends_on: deps
                .into_iter()
                .map(|(d, v)| Dependency {
                    id: d.to_string(),
                    version: v.to_string(),
                })
                .collect(),
            remix_of: remix_of.map(|(id, v)| RemixAttribution {
                id: id.to_string(),
                version: v.to_string(),
                attribution: format!("forked from {id}@{v}"),
            }),
            tags: vec![],
            sigma_mask: 0,
            gift_economy_only: true,
            license: LicenseTier::Open,
        }
    }

    fn fixture_registry() -> InMemoryRegistry {
        let mut reg = InMemoryRegistry::new();
        reg.insert(manifest_for("a", "1.0.0", vec![], None));
        reg.insert(manifest_for("b", "1.0.0", vec![("a", "1.0.0")], None));
        reg.insert(manifest_for("c", "1.0.0", vec![("a", "1.0.0")], None));
        reg
    }

    #[test]
    fn resolve_no_deps_returns_empty() {
        let root = manifest_for("root", "1.0.0", vec![], None);
        let reg = InMemoryRegistry::new();
        let r = package_dependencies_resolve(&root, &reg).unwrap();
        assert!(r.is_empty());
    }

    #[test]
    fn resolve_linear_chain() {
        let reg = fixture_registry();
        let root = manifest_for("root", "1.0.0", vec![("b", "1.0.0")], None);
        let r = package_dependencies_resolve(&root, &reg).unwrap();
        // a (deepest) before b (which depends on a).
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].id, "a");
        assert_eq!(r[1].id, "b");
    }

    #[test]
    fn resolve_diamond_dedup() {
        let reg = fixture_registry();
        // root → b → a , root → c → a — `a` should appear once.
        let root = manifest_for(
            "root",
            "1.0.0",
            vec![("b", "1.0.0"), ("c", "1.0.0")],
            None,
        );
        let r = package_dependencies_resolve(&root, &reg).unwrap();
        assert_eq!(r.len(), 3);
        // a appears first (deepest).
        assert_eq!(r[0].id, "a");
        // b and c after.
        assert!(r.iter().any(|p| p.id == "b"));
        assert!(r.iter().any(|p| p.id == "c"));
    }

    #[test]
    fn resolve_missing_package_rejected() {
        let reg = InMemoryRegistry::new();
        let root = manifest_for("root", "1.0.0", vec![("missing", "1.0.0")], None);
        let r = package_dependencies_resolve(&root, &reg);
        assert!(matches!(r, Err(ResolveError::Missing(..))));
    }

    #[test]
    fn resolve_cycle_rejected() {
        // a → b → a (cycle).
        let mut reg = InMemoryRegistry::new();
        reg.insert(manifest_for("a", "1.0.0", vec![("b", "1.0.0")], None));
        reg.insert(manifest_for("b", "1.0.0", vec![("a", "1.0.0")], None));
        let root = manifest_for("root", "1.0.0", vec![("a", "1.0.0")], None);
        let r = package_dependencies_resolve(&root, &reg);
        assert!(matches!(r, Err(ResolveError::Cycle(..))));
    }

    #[test]
    fn resolve_self_cycle_rejected() {
        // a → a (self-loop).
        let mut reg = InMemoryRegistry::new();
        reg.insert(manifest_for("a", "1.0.0", vec![("a", "1.0.0")], None));
        let root = manifest_for("root", "1.0.0", vec![("a", "1.0.0")], None);
        let r = package_dependencies_resolve(&root, &reg);
        assert!(matches!(r, Err(ResolveError::Cycle(..))));
    }

    #[test]
    fn resolve_root_self_dep_rejected() {
        // root → root (root cycles back to itself directly).
        let mut reg = InMemoryRegistry::new();
        reg.insert(manifest_for(
            "root",
            "1.0.0",
            vec![("root", "1.0.0")],
            None,
        ));
        let root = manifest_for("root", "1.0.0", vec![("root", "1.0.0")], None);
        let r = package_dependencies_resolve(&root, &reg);
        assert!(matches!(r, Err(ResolveError::Cycle(..))));
    }

    #[test]
    fn remix_chain_walks_back() {
        // c was remixed from b which was remixed from a.
        let mut reg = InMemoryRegistry::new();
        reg.insert(manifest_for("a", "1.0.0", vec![], None));
        reg.insert(manifest_for("b", "1.0.0", vec![], Some(("a", "1.0.0"))));
        reg.insert(manifest_for("c", "1.0.0", vec![], Some(("b", "1.0.0"))));
        let root = reg.lookup("c", "1.0.0").cloned().unwrap();
        let chain = remix_chain_walk(&root, &reg).unwrap();
        assert_eq!(chain.len(), 2);
        assert_eq!(chain[0].id, "b");
        assert_eq!(chain[1].id, "a");
    }

    #[test]
    fn remix_chain_no_remix_is_empty() {
        let reg = fixture_registry();
        let root = manifest_for("standalone", "1.0.0", vec![], None);
        let chain = remix_chain_walk(&root, &reg).unwrap();
        assert!(chain.is_empty());
    }

    #[test]
    fn remix_chain_cycle_rejected() {
        // a remixed from b which is remixed from a (loop).
        let mut reg = InMemoryRegistry::new();
        reg.insert(manifest_for("a", "1.0.0", vec![], Some(("b", "1.0.0"))));
        reg.insert(manifest_for("b", "1.0.0", vec![], Some(("a", "1.0.0"))));
        let root = reg.lookup("a", "1.0.0").cloned().unwrap();
        let r = remix_chain_walk(&root, &reg);
        assert!(matches!(r, Err(ResolveError::Cycle(..))));
    }

    #[test]
    fn remix_chain_missing_upstream_rejected() {
        let reg = InMemoryRegistry::new();
        let root = manifest_for("downstream", "1.0.0", vec![], Some(("absent", "1.0.0")));
        let r = remix_chain_walk(&root, &reg);
        assert!(matches!(r, Err(ResolveError::Missing(..))));
    }
}
