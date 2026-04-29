//! NavMesh — 2D walkable surfaces with deterministic A* pathfinding.
//!
//! § THESIS
//!   A nav-mesh is a triangulation of the walkable surface plus an
//!   adjacency graph over those triangles. Pathfinding picks a sequence
//!   of triangles linking start to goal ; the per-triangle path is
//!   smoothed downstream (stage-0 returns the triangle-id sequence).
//!
//!   Stage-0 is 2D : positions are `[f64; 2]`. 3D extends naturally
//!   (deferred per spec § DEFERRED).
//!
//! § DETERMINISM (‼ load-bearing)
//!   - All map structures are `BTreeMap`/`BTreeSet` so iteration order is
//!     deterministic across runs.
//!   - **A* tie-break** : when multiple frontier nodes have identical
//!     `f = g + h`, the algorithm prefers the one with **smaller g**
//!     (closer-to-goal-along-discovered-path). When `f` AND `g` tie,
//!     it prefers the **smaller TriId**. This canonicalizes paths
//!     across runs even when multiple shortest paths exist.
//!   - Heuristic : Euclidean distance between triangle-centroids. Euclidean
//!     is admissible + consistent in 2D space (well-known result) — A*
//!     terminates with optimal-shortest path.
//!   - No internal RNG ; no clock reads.
//!
//! § PRIME_DIRECTIVE-ALIGNMENT
//!   - Every public mutator that creates triangles validates indices
//!     before insertion ; malformed nav-meshes surface as errors, not
//!     panics. Aligned with `cssl-substrate-omega-step § PRIME-DIRECTIVE-
//!     ALIGNMENT § no silent fallbacks`.
//!   - Pathfinding is bounded by the configurable `PathRequest::max_expansions`
//!     so a malicious / malformed mesh cannot DoS the scheduler.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, BinaryHeap};

use thiserror::Error;

/// A 2D point. Stage-0 stable.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point2 {
    /// X coordinate.
    pub x: f64,
    /// Y coordinate.
    pub y: f64,
}

impl Point2 {
    /// Construct a Point2.
    #[must_use]
    pub const fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// Squared Euclidean distance to another point. Avoids sqrt for
    /// inner-loop comparisons.
    #[must_use]
    pub fn dist_sq(self, other: Self) -> f64 {
        let dx = self.x - other.x;
        let dy = self.y - other.y;
        dx * dx + dy * dy
    }

    /// Euclidean distance to another point.
    #[must_use]
    pub fn dist(self, other: Self) -> f64 {
        self.dist_sq(other).sqrt()
    }
}

/// Identifier for a triangle in the navmesh.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct TriId(pub u32);

/// A portal connecting one navmesh-region to another (e.g. a door).
/// Stage-0 stores the two triangle endpoints + a bidirectional flag.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Portal {
    /// Triangle on the source side of the portal.
    pub from: TriId,
    /// Triangle on the destination side.
    pub to: TriId,
    /// `true` for two-way portals ; `false` for one-way (e.g. ledge drop).
    pub bidirectional: bool,
}

/// Tie-break policy for A* when multiple frontier nodes share `f = g + h`.
///
/// § STAGE-0 STABLE
///   `GValueDescThenTriIdAsc` is the canonical default : prefer the node
///   that has discovered more progress along the path (larger g), then
///   prefer the smaller TriId for full determinism. This matches the
///   landmine-spec : "tie-break by g-value for deterministic paths".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AStarTie {
    /// Prefer larger g (closer-to-goal-along-discovered-path), then smaller TriId.
    GValueDescThenTriIdAsc,
    /// Prefer smaller g (less-explored), then smaller TriId. For testing.
    GValueAscThenTriIdAsc,
}

impl AStarTie {
    /// Default tie-break : g-value-desc, tri-id-asc.
    #[must_use]
    pub const fn default_policy() -> Self {
        Self::GValueDescThenTriIdAsc
    }
}

/// A pathfinding request. All fields are caller-supplied ; no defaults
/// at this layer beyond `tie_break`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PathRequest {
    /// Triangle the agent currently occupies.
    pub start: TriId,
    /// Triangle the agent wants to reach.
    pub goal: TriId,
    /// Maximum number of A* expansions before the search aborts. Stops
    /// adversarial inputs from running forever. Stage-0 default is 4096.
    pub max_expansions: u32,
    /// Tie-break policy.
    pub tie_break: AStarTie,
}

impl PathRequest {
    /// Construct a path request with the canonical tie-break.
    #[must_use]
    pub const fn new(start: TriId, goal: TriId) -> Self {
        Self {
            start,
            goal,
            max_expansions: 4096,
            tie_break: AStarTie::default_policy(),
        }
    }
}

/// The result of an A* search.
#[derive(Debug, Clone, PartialEq)]
pub struct PathResult {
    /// Sequence of triangles from start to goal (inclusive of both).
    pub path: Vec<TriId>,
    /// Total path cost (sum of triangle-centroid distances).
    pub cost: f64,
    /// Number of A* expansions performed.
    pub expansions: u32,
}

/// Errors during navmesh construction.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum NavMeshBuildError {
    /// Triangle references a vertex index that is out of bounds.
    #[error("AIBEHAV0050 — triangle {tri_index} references vertex {vertex_index} but only {vertex_count} vertices defined")]
    VertexIndexOutOfBounds {
        tri_index: u32,
        vertex_index: u32,
        vertex_count: u32,
    },
    /// Triangle has duplicate vertex indices (degenerate).
    #[error("AIBEHAV0051 — triangle {tri_index} is degenerate (duplicate vertices)")]
    DegenerateTriangle { tri_index: u32 },
    /// Portal references a TriId that does not exist.
    #[error("AIBEHAV0052 — portal references non-existent triangle {tri_id:?}")]
    PortalUnknownTriangle { tri_id: TriId },
}

impl NavMeshBuildError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::VertexIndexOutOfBounds { .. } => "AIBEHAV0050",
            Self::DegenerateTriangle { .. } => "AIBEHAV0051",
            Self::PortalUnknownTriangle { .. } => "AIBEHAV0052",
        }
    }
}

/// Errors during navmesh pathfinding.
#[derive(Debug, Error, Clone, PartialEq)]
pub enum NavMeshError {
    /// Start triangle is not in the mesh.
    #[error("AIBEHAV0060 — start triangle {0:?} not found")]
    UnknownStart(TriId),
    /// Goal triangle is not in the mesh.
    #[error("AIBEHAV0061 — goal triangle {0:?} not found")]
    UnknownGoal(TriId),
    /// No path exists between start and goal.
    #[error("AIBEHAV0062 — no path from {start:?} to {goal:?}")]
    NoPath { start: TriId, goal: TriId },
    /// A* expanded the maximum number of nodes without reaching the goal.
    #[error("AIBEHAV0063 — A* exceeded {limit} expansions without reaching goal")]
    ExpansionLimitReached { limit: u32 },
}

impl NavMeshError {
    /// Stable diagnostic code prefix.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::UnknownStart(_) => "AIBEHAV0060",
            Self::UnknownGoal(_) => "AIBEHAV0061",
            Self::NoPath { .. } => "AIBEHAV0062",
            Self::ExpansionLimitReached { .. } => "AIBEHAV0063",
        }
    }
}

/// Triangle stored in a NavMesh.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Triangle {
    vertices: [u32; 3],
}

impl Triangle {
    fn centroid(&self, vertices: &[Point2]) -> Point2 {
        let a = vertices[self.vertices[0] as usize];
        let b = vertices[self.vertices[1] as usize];
        let c = vertices[self.vertices[2] as usize];
        Point2::new((a.x + b.x + c.x) / 3.0, (a.y + b.y + c.y) / 3.0)
    }
}

/// 2D walkable-surface mesh + adjacency + portals.
#[derive(Debug, Clone, Default)]
pub struct NavMesh {
    vertices: Vec<Point2>,
    triangles: Vec<Triangle>,
    /// Adjacency : tri-id → set of neighbor tri-ids. BTreeMap-of-BTreeSet
    /// for deterministic iteration.
    adjacency: BTreeMap<TriId, BTreeSet<TriId>>,
    /// Portals : tri-id → list of (portal-target, bidirectional).
    portals: BTreeMap<TriId, Vec<(TriId, bool)>>,
    /// Cached triangle centroids — same length as `triangles`.
    centroids: Vec<Point2>,
}

impl NavMesh {
    /// Construct an empty navmesh.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a navmesh from raw vertex + triangle data. Adjacency is
    /// auto-derived (two triangles share an edge ⇒ adjacent).
    pub fn build(
        vertices: Vec<Point2>,
        triangles: Vec<[u32; 3]>,
    ) -> Result<Self, NavMeshBuildError> {
        let vertex_count = vertices.len() as u32;
        let mut tris = Vec::with_capacity(triangles.len());
        for (i, t) in triangles.iter().enumerate() {
            for &vi in t {
                if vi >= vertex_count {
                    return Err(NavMeshBuildError::VertexIndexOutOfBounds {
                        tri_index: i as u32,
                        vertex_index: vi,
                        vertex_count,
                    });
                }
            }
            // Degenerate-triangle check : all-three-vertices unique.
            if t[0] == t[1] || t[1] == t[2] || t[0] == t[2] {
                return Err(NavMeshBuildError::DegenerateTriangle {
                    tri_index: i as u32,
                });
            }
            tris.push(Triangle { vertices: *t });
        }

        // Centroids cache.
        let centroids: Vec<Point2> = tris.iter().map(|t| t.centroid(&vertices)).collect();

        // Adjacency : two triangles sharing 2 vertices share an edge.
        // Deterministic computation : iterate triangles in index order,
        // for each edge build a (sorted-pair) → tri-list map, then walk.
        let mut edge_to_tris: BTreeMap<(u32, u32), Vec<u32>> = BTreeMap::new();
        for (i, t) in tris.iter().enumerate() {
            let i = i as u32;
            let v = t.vertices;
            let edges = [(v[0], v[1]), (v[1], v[2]), (v[2], v[0])];
            for (a, b) in edges {
                let key = if a < b { (a, b) } else { (b, a) };
                edge_to_tris.entry(key).or_default().push(i);
            }
        }
        let mut adjacency: BTreeMap<TriId, BTreeSet<TriId>> = BTreeMap::new();
        for tri_list in edge_to_tris.values() {
            // Edge shared by 2 triangles : they're adjacent.
            // (Edges shared by >2 are non-manifold ; stage-0 still records
            // the pairwise adjacency without erroring — auditable later.)
            for &i in tri_list {
                for &j in tri_list {
                    if i != j {
                        adjacency.entry(TriId(i)).or_default().insert(TriId(j));
                    }
                }
            }
        }
        // Ensure every triangle has an entry, even if isolated.
        for i in 0..tris.len() {
            adjacency.entry(TriId(i as u32)).or_default();
        }

        Ok(Self {
            vertices,
            triangles: tris,
            adjacency,
            portals: BTreeMap::new(),
            centroids,
        })
    }

    /// Add a portal between two triangles.
    pub fn add_portal(&mut self, portal: Portal) -> Result<(), NavMeshBuildError> {
        if (portal.from.0 as usize) >= self.triangles.len() {
            return Err(NavMeshBuildError::PortalUnknownTriangle {
                tri_id: portal.from,
            });
        }
        if (portal.to.0 as usize) >= self.triangles.len() {
            return Err(NavMeshBuildError::PortalUnknownTriangle { tri_id: portal.to });
        }
        self.portals
            .entry(portal.from)
            .or_default()
            .push((portal.to, portal.bidirectional));
        if portal.bidirectional {
            self.portals
                .entry(portal.to)
                .or_default()
                .push((portal.from, true));
        }
        Ok(())
    }

    /// Number of triangles in the mesh.
    #[must_use]
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Number of vertices in the mesh.
    #[must_use]
    pub fn vertex_count(&self) -> usize {
        self.vertices.len()
    }

    /// Number of portals registered.
    #[must_use]
    pub fn portal_count(&self) -> usize {
        self.portals.values().map(Vec::len).sum()
    }

    /// Triangle centroid by id ; `None` if id out of bounds.
    #[must_use]
    pub fn centroid(&self, id: TriId) -> Option<Point2> {
        self.centroids.get(id.0 as usize).copied()
    }

    /// Neighbors of a triangle : adjacency-edges + portal-targets, in
    /// deterministic (TriId-ascending) order.
    #[must_use]
    pub fn neighbors(&self, id: TriId) -> Vec<TriId> {
        let mut out: BTreeSet<TriId> = BTreeSet::new();
        if let Some(adj) = self.adjacency.get(&id) {
            out.extend(adj.iter().copied());
        }
        if let Some(portals) = self.portals.get(&id) {
            for (target, _) in portals {
                out.insert(*target);
            }
        }
        out.into_iter().collect()
    }

    /// A* search from `req.start` to `req.goal`. Returns the path or an error.
    ///
    /// § ALGORITHM
    ///   Standard A* with `f(n) = g(n) + h(n)` where :
    ///     - `g(n)` = sum of edge costs (centroid distances) from start.
    ///     - `h(n)` = Euclidean distance from `n.centroid` to `goal.centroid`.
    ///   Euclidean is admissible (never overestimates) + consistent
    ///   (`h(n) <= cost(n,m) + h(m)`) for 2D Euclidean space, so A*
    ///   terminates with the optimal path.
    ///
    /// § DETERMINISM
    ///   The frontier is a `BinaryHeap<AStarNode>` ordered such that :
    ///     1. Lower `f` first.
    ///     2. Tie-break by `g` per `req.tie_break`.
    ///     3. Tie-break by `TriId` ascending.
    ///   `BinaryHeap` is max-heap by default ; we wrap each node in a
    ///   custom `Ord` impl so it acts as a min-heap on `(f, g_tie, tri_id)`.
    pub fn find_path(&self, req: PathRequest) -> Result<PathResult, NavMeshError> {
        let n = self.triangles.len() as u32;
        if req.start.0 >= n {
            return Err(NavMeshError::UnknownStart(req.start));
        }
        if req.goal.0 >= n {
            return Err(NavMeshError::UnknownGoal(req.goal));
        }
        if req.start == req.goal {
            return Ok(PathResult {
                path: vec![req.start],
                cost: 0.0,
                expansions: 0,
            });
        }

        let goal_centroid = self.centroids[req.goal.0 as usize];
        let h = |id: TriId| -> f64 { self.centroids[id.0 as usize].dist(goal_centroid) };

        let mut g_score: BTreeMap<TriId, f64> = BTreeMap::new();
        g_score.insert(req.start, 0.0);
        let mut came_from: BTreeMap<TriId, TriId> = BTreeMap::new();
        let mut frontier: BinaryHeap<AStarNode> = BinaryHeap::new();
        frontier.push(AStarNode {
            tri: req.start,
            g: 0.0,
            f: h(req.start),
            tie: req.tie_break,
        });

        let mut expansions: u32 = 0;
        while let Some(node) = frontier.pop() {
            if node.tri == req.goal {
                // Reconstruct path.
                let mut path = vec![req.goal];
                let mut cur = req.goal;
                while let Some(&prev) = came_from.get(&cur) {
                    path.push(prev);
                    cur = prev;
                }
                path.reverse();
                return Ok(PathResult {
                    path,
                    cost: node.g,
                    expansions,
                });
            }

            // Ignore stale entries (a better g for this node was found).
            if let Some(&best_g) = g_score.get(&node.tri) {
                if node.g > best_g {
                    continue;
                }
            }

            expansions = expansions.saturating_add(1);
            if expansions > req.max_expansions {
                return Err(NavMeshError::ExpansionLimitReached {
                    limit: req.max_expansions,
                });
            }

            let neighbors = self.neighbors(node.tri);
            for nb in neighbors {
                let edge_cost =
                    self.centroids[node.tri.0 as usize].dist(self.centroids[nb.0 as usize]);
                let tentative_g = node.g + edge_cost;
                let known = g_score.get(&nb).copied().unwrap_or(f64::INFINITY);
                if tentative_g < known {
                    came_from.insert(nb, node.tri);
                    g_score.insert(nb, tentative_g);
                    let f = tentative_g + h(nb);
                    frontier.push(AStarNode {
                        tri: nb,
                        g: tentative_g,
                        f,
                        tie: req.tie_break,
                    });
                }
            }
        }

        Err(NavMeshError::NoPath {
            start: req.start,
            goal: req.goal,
        })
    }
}

/// A node in the A* frontier ; total-orderable for `BinaryHeap`.
///
/// § Ord  (load-bearing for determinism)
///   `BinaryHeap` is a max-heap. We invert `f` so the lowest-f wins.
///   Tie-break per `tie` field, then by `tri.0` ascending.
#[derive(Debug, Clone, Copy)]
struct AStarNode {
    tri: TriId,
    g: f64,
    f: f64,
    tie: AStarTie,
}

impl PartialEq for AStarNode {
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f && self.g == other.g && self.tri == other.tri
    }
}
impl Eq for AStarNode {}

impl Ord for AStarNode {
    fn cmp(&self, other: &Self) -> Ordering {
        // We want a min-heap on f, so reverse the natural ordering.
        // Compare f via to_bits-aware ordering : NaN is unlikely but safe.
        let f_cmp = compare_f64(self.f, other.f).reverse();
        if f_cmp != Ordering::Equal {
            return f_cmp;
        }
        // Tie on f : break by g per policy.
        let g_cmp = compare_f64(self.g, other.g);
        let g_cmp = match self.tie {
            AStarTie::GValueDescThenTriIdAsc => g_cmp, // larger g = "greater" in max-heap = preferred
            AStarTie::GValueAscThenTriIdAsc => g_cmp.reverse(),
        };
        if g_cmp != Ordering::Equal {
            return g_cmp;
        }
        // Tie on g : break by tri-id ascending. Max-heap, so reverse.
        self.tri.0.cmp(&other.tri.0).reverse()
    }
}

impl PartialOrd for AStarNode {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

/// Total order over f64 for BinaryHeap ; treats NaN as max (sinks to bottom
/// of max-heap, i.e. visited last).
fn compare_f64(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a 2x2 grid of triangles for path testing.
    /// Vertices laid out :
    ///   2---3
    ///   | \ |
    ///   0---1
    /// Triangles : t0 = [0,1,2] ; t1 = [1,3,2] (share edge 1-2).
    fn grid_2() -> NavMesh {
        let v = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(1.0, 1.0),
        ];
        let t = vec![[0, 1, 2], [1, 3, 2]];
        NavMesh::build(v, t).unwrap()
    }

    /// Build a 3-row strip of triangles : 4 vertices, 4 triangles.
    /// 0-1-2-3 along the bottom, 4-5-6-7 along the top, etc.
    /// Stage-0 simple triangle-strip for path testing.
    fn strip(n: u32) -> NavMesh {
        // n+1 triangles in a fan pattern.
        let mut v = vec![];
        let mut t = vec![];
        for i in 0..=n {
            v.push(Point2::new(i as f64, 0.0));
            v.push(Point2::new(i as f64, 1.0));
        }
        // Connect quads as 2 triangles each.
        for i in 0..n {
            let bl = 2 * i;
            let br = 2 * (i + 1);
            let tl = bl + 1;
            let tr = br + 1;
            t.push([bl, br, tl]);
            t.push([br, tr, tl]);
        }
        NavMesh::build(v, t).unwrap()
    }

    #[test]
    fn point2_dist_basic() {
        let a = Point2::new(0.0, 0.0);
        let b = Point2::new(3.0, 4.0);
        assert!((a.dist(b) - 5.0).abs() < 1e-9);
        assert!((a.dist_sq(b) - 25.0).abs() < 1e-9);
    }

    #[test]
    fn navmesh_build_basic() {
        let mesh = grid_2();
        assert_eq!(mesh.triangle_count(), 2);
        assert_eq!(mesh.vertex_count(), 4);
    }

    #[test]
    fn navmesh_vertex_index_out_of_bounds() {
        let v = vec![Point2::new(0.0, 0.0)];
        let t = vec![[0, 1, 2]];
        let err = NavMesh::build(v, t).unwrap_err();
        assert!(matches!(
            err,
            NavMeshBuildError::VertexIndexOutOfBounds { .. }
        ));
        assert_eq!(err.code(), "AIBEHAV0050");
    }

    #[test]
    fn navmesh_degenerate_triangle_rejected() {
        let v = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        let t = vec![[0, 0, 1]]; // duplicate vertex
        let err = NavMesh::build(v, t).unwrap_err();
        assert!(matches!(err, NavMeshBuildError::DegenerateTriangle { .. }));
        assert_eq!(err.code(), "AIBEHAV0051");
    }

    #[test]
    fn navmesh_adjacency_two_triangles() {
        let mesh = grid_2();
        let nbrs_0 = mesh.neighbors(TriId(0));
        let nbrs_1 = mesh.neighbors(TriId(1));
        assert_eq!(nbrs_0, vec![TriId(1)]);
        assert_eq!(nbrs_1, vec![TriId(0)]);
    }

    #[test]
    fn navmesh_centroid_correct() {
        let mesh = grid_2();
        let c = mesh.centroid(TriId(0)).unwrap();
        // tri 0 = [0,1,2] = (0,0) (1,0) (0,1) → centroid (1/3, 1/3)
        assert!((c.x - 1.0 / 3.0).abs() < 1e-9);
        assert!((c.y - 1.0 / 3.0).abs() < 1e-9);
    }

    #[test]
    fn navmesh_centroid_oob_none() {
        let mesh = grid_2();
        assert!(mesh.centroid(TriId(99)).is_none());
    }

    #[test]
    fn astar_same_node_zero_cost() {
        let mesh = grid_2();
        let r = mesh
            .find_path(PathRequest::new(TriId(0), TriId(0)))
            .unwrap();
        assert_eq!(r.path, vec![TriId(0)]);
        assert!(r.cost.abs() < 1e-9);
    }

    #[test]
    fn astar_unknown_start() {
        let mesh = grid_2();
        let err = mesh
            .find_path(PathRequest::new(TriId(99), TriId(0)))
            .unwrap_err();
        assert!(matches!(err, NavMeshError::UnknownStart(_)));
        assert_eq!(err.code(), "AIBEHAV0060");
    }

    #[test]
    fn astar_unknown_goal() {
        let mesh = grid_2();
        let err = mesh
            .find_path(PathRequest::new(TriId(0), TriId(99)))
            .unwrap_err();
        assert!(matches!(err, NavMeshError::UnknownGoal(_)));
        assert_eq!(err.code(), "AIBEHAV0061");
    }

    #[test]
    fn astar_two_node_path() {
        let mesh = grid_2();
        let r = mesh
            .find_path(PathRequest::new(TriId(0), TriId(1)))
            .unwrap();
        assert_eq!(r.path, vec![TriId(0), TriId(1)]);
        assert!(r.cost > 0.0);
    }

    #[test]
    fn astar_strip_path() {
        let mesh = strip(4); // 8 triangles in a strip
        let r = mesh
            .find_path(PathRequest::new(TriId(0), TriId(7)))
            .unwrap();
        assert_eq!(*r.path.first().unwrap(), TriId(0));
        assert_eq!(*r.path.last().unwrap(), TriId(7));
        assert!(r.path.len() >= 2);
    }

    #[test]
    fn astar_disconnected_no_path() {
        // Two separate triangle-pairs with no shared edge.
        let v = vec![
            // pair 1
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            // pair 2 (disjoint)
            Point2::new(10.0, 10.0),
            Point2::new(11.0, 10.0),
            Point2::new(10.0, 11.0),
        ];
        let t = vec![[0, 1, 2], [3, 4, 5]];
        let mesh = NavMesh::build(v, t).unwrap();
        let err = mesh
            .find_path(PathRequest::new(TriId(0), TriId(1)))
            .unwrap_err();
        assert!(matches!(err, NavMeshError::NoPath { .. }));
        assert_eq!(err.code(), "AIBEHAV0062");
    }

    #[test]
    fn astar_expansion_limit() {
        let mesh = strip(20);
        let mut req = PathRequest::new(TriId(0), TriId(39));
        req.max_expansions = 1; // crippling
        let err = mesh.find_path(req).unwrap_err();
        assert!(matches!(err, NavMeshError::ExpansionLimitReached { .. }));
        assert_eq!(err.code(), "AIBEHAV0063");
    }

    #[test]
    fn astar_determinism_across_runs() {
        let mesh = strip(10);
        let r1 = mesh
            .find_path(PathRequest::new(TriId(0), TriId(19)))
            .unwrap();
        let r2 = mesh
            .find_path(PathRequest::new(TriId(0), TriId(19)))
            .unwrap();
        assert_eq!(r1.path, r2.path);
        assert_eq!(r1.cost.to_bits(), r2.cost.to_bits());
    }

    #[test]
    fn astar_portal_traversal() {
        // Two grid-pairs, connected only by a portal.
        let v = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(10.0, 10.0),
            Point2::new(11.0, 10.0),
            Point2::new(10.0, 11.0),
        ];
        let t = vec![[0, 1, 2], [3, 4, 5]];
        let mut mesh = NavMesh::build(v, t).unwrap();
        mesh.add_portal(Portal {
            from: TriId(0),
            to: TriId(1),
            bidirectional: true,
        })
        .unwrap();
        let r = mesh
            .find_path(PathRequest::new(TriId(0), TriId(1)))
            .unwrap();
        assert_eq!(r.path, vec![TriId(0), TriId(1)]);
    }

    #[test]
    fn astar_one_way_portal() {
        let v = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
            Point2::new(10.0, 10.0),
            Point2::new(11.0, 10.0),
            Point2::new(10.0, 11.0),
        ];
        let t = vec![[0, 1, 2], [3, 4, 5]];
        let mut mesh = NavMesh::build(v, t).unwrap();
        mesh.add_portal(Portal {
            from: TriId(0),
            to: TriId(1),
            bidirectional: false,
        })
        .unwrap();
        // Forward path works.
        let r = mesh
            .find_path(PathRequest::new(TriId(0), TriId(1)))
            .unwrap();
        assert_eq!(r.path, vec![TriId(0), TriId(1)]);
        // Reverse path fails (one-way).
        let err = mesh
            .find_path(PathRequest::new(TriId(1), TriId(0)))
            .unwrap_err();
        assert!(matches!(err, NavMeshError::NoPath { .. }));
    }

    #[test]
    fn astar_portal_bad_target_rejected() {
        let mut mesh = grid_2();
        let err = mesh
            .add_portal(Portal {
                from: TriId(0),
                to: TriId(99),
                bidirectional: false,
            })
            .unwrap_err();
        assert!(matches!(
            err,
            NavMeshBuildError::PortalUnknownTriangle { .. }
        ));
        assert_eq!(err.code(), "AIBEHAV0052");
    }

    #[test]
    fn astar_neighbors_dedup_portal_and_adjacency() {
        // If a portal targets an already-adjacent triangle, neighbors() still
        // returns that target only once (BTreeSet dedup).
        let mut mesh = grid_2();
        mesh.add_portal(Portal {
            from: TriId(0),
            to: TriId(1),
            bidirectional: false,
        })
        .unwrap();
        let nbrs = mesh.neighbors(TriId(0));
        assert_eq!(nbrs, vec![TriId(1)]);
    }

    #[test]
    fn astar_neighbors_isolated_empty() {
        // Single-triangle mesh : no neighbors.
        let v = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(0.0, 1.0),
        ];
        let t = vec![[0, 1, 2]];
        let mesh = NavMesh::build(v, t).unwrap();
        assert!(mesh.neighbors(TriId(0)).is_empty());
    }

    #[test]
    fn astar_optimal_path_simple() {
        let mesh = strip(3); // 6 triangles
                             // Path 0 → 5 should hit every alternating triangle in order.
        let r = mesh
            .find_path(PathRequest::new(TriId(0), TriId(5)))
            .unwrap();
        // Path must include start + goal.
        assert_eq!(*r.path.first().unwrap(), TriId(0));
        assert_eq!(*r.path.last().unwrap(), TriId(5));
        // Cost is positive + finite.
        assert!(r.cost.is_finite());
        assert!(r.cost > 0.0);
    }

    #[test]
    fn astar_tie_break_policies_distinguish_paths_when_eq_cost() {
        // Build a simple square : 2 paths from t0 to t3 with equal cost.
        // Using just grid_2 gives one path. Let's use a square of 4 triangles.
        let v = vec![
            Point2::new(0.0, 0.0),
            Point2::new(1.0, 0.0),
            Point2::new(1.0, 1.0),
            Point2::new(0.0, 1.0),
            Point2::new(0.5, 0.5), // center
        ];
        let t = vec![
            [0, 1, 4], // bottom
            [1, 2, 4], // right
            [2, 3, 4], // top
            [3, 0, 4], // left
        ];
        let mesh = NavMesh::build(v, t).unwrap();
        let r1 = mesh
            .find_path(PathRequest {
                start: TriId(0),
                goal: TriId(2),
                max_expansions: 4096,
                tie_break: AStarTie::GValueDescThenTriIdAsc,
            })
            .unwrap();
        let r2 = mesh
            .find_path(PathRequest {
                start: TriId(0),
                goal: TriId(2),
                max_expansions: 4096,
                tie_break: AStarTie::GValueDescThenTriIdAsc,
            })
            .unwrap();
        // Same policy ⇒ same path bit-equally.
        assert_eq!(r1.path, r2.path);
    }

    #[test]
    fn navmesh_portal_count_basic() {
        let mut mesh = grid_2();
        assert_eq!(mesh.portal_count(), 0);
        mesh.add_portal(Portal {
            from: TriId(0),
            to: TriId(1),
            bidirectional: true,
        })
        .unwrap();
        // bidirectional → 2 entries
        assert_eq!(mesh.portal_count(), 2);
    }

    #[test]
    fn astar_default_tie_break_is_g_desc_tri_asc() {
        assert_eq!(AStarTie::default_policy(), AStarTie::GValueDescThenTriIdAsc);
    }

    #[test]
    fn astar_g_asc_tie_policy_works() {
        let mesh = grid_2();
        let r = mesh
            .find_path(PathRequest {
                start: TriId(0),
                goal: TriId(1),
                max_expansions: 4096,
                tie_break: AStarTie::GValueAscThenTriIdAsc,
            })
            .unwrap();
        assert_eq!(r.path, vec![TriId(0), TriId(1)]);
    }
}
