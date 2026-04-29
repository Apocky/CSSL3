//! § cssl-render::graph — declarative render-graph
//! ═══════════════════════════════════════════════════════════════════════════
//!
//! § ROLE
//!   Declarative pass-DAG that the backend executes per frame. Each
//!   [`RenderPass`] declares its inputs (read attachments) + outputs
//!   (write attachments) ; the graph topologically sorts the passes so
//!   resource dependencies are honored. The backend translates the sorted
//!   pass list to backend-specific commands (Vulkan render-pass +
//!   subpasses, D3D12 explicit barriers, Metal MTLRenderCommandEncoder etc).
//!
//! § STANDARD PASSES (substrate canonical)
//!   The renderer's default pass set is :
//!   1. [`PassKind::ShadowPass`] (deferred — scaffolded, not wired)
//!   2. [`PassKind::GeometryPass`]   — opaque G-buffer / forward draws
//!   3. [`PassKind::LightingPass`]   — light-evaluation + IBL
//!   4. [`PassKind::TranslucentPass`] — alpha-blended draws
//!   5. [`PassKind::TonemapPass`]    — HDR → sRGB encode for swapchain
//!   6. [`PassKind::UiPass`]         — overlay + debug-draw
//!   This is the `RenderGraph::default_forward_pipeline()` constructor.
//!
//! § ATTACHMENT MODEL
//!   Resources flow through the graph as named [`AttachmentId`] handles.
//!   The geometry pass writes "color_hdr" + "depth" ; the lighting pass
//!   reads "color_hdr" + "depth" + "shadow_atlas" and writes "color_lit".
//!   Etc. Topo-sort uses these read/write sets to derive execution order.
//!
//! § FUTURE
//!   - Resource lifetime + alias-promotion (transient memory) — deferred.
//!   - Async-compute pass parallelism — deferred (needs queue-affinity surface).
//!   - Multi-view (split-screen / stereo / cubemap) — deferred (needs
//!     observer-frame integration with the projections H3 layer).

// ════════════════════════════════════════════════════════════════════════════
// § PassKind — discriminator for standard pass types
// ════════════════════════════════════════════════════════════════════════════

/// Kind of render pass. The backend chooses pipeline state + clear behavior
/// + sort order based on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PassKind {
    /// Shadow-map generation. Renders depth-only from each shadow-caster
    /// light's POV into a shadow atlas.
    ShadowPass,
    /// Opaque geometry pass. Forward or G-buffer depending on backend
    /// configuration. Writes color (HDR) + depth.
    GeometryPass,
    /// Lighting evaluation pass. Reads geometry-pass outputs, evaluates
    /// PBR lighting against scene lights + image-based lighting, writes
    /// lit color (HDR).
    LightingPass,
    /// Translucent / alpha-blended pass. Sorted back-to-front. Reads depth
    /// (for depth-test) but does NOT write depth (typical alpha-blend
    /// configuration).
    TranslucentPass,
    /// HDR → SDR tonemap. Reads lit color + emissive, applies tone curve,
    /// gamma-encodes to swapchain output (sRGB).
    TonemapPass,
    /// UI / overlay / debug pass. Renders 2D primitives + text + debug-
    /// draw lines on top of the tonemapped image. Substrate canonical :
    /// UI pass runs in linear space + bypasses tonemap.
    UiPass,
    /// Custom user-defined pass kind. The associated `u32` is a free-form
    /// id that the consumer + backend agree on.
    Custom(u32),
}

// ════════════════════════════════════════════════════════════════════════════
// § AttachmentId — resource handle in the graph
// ════════════════════════════════════════════════════════════════════════════

/// Newtype for the attachment-name id. Backend-side resource resolution
/// matches `AttachmentId` against an attachment-table at submit time.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AttachmentId(pub u32);

impl AttachmentId {
    /// Sentinel : "no attachment". A pass with `swapchain_target == NONE`
    /// is an off-screen / intermediate pass.
    pub const NONE: Self = Self(u32::MAX);

    /// True if this is a real attachment id.
    #[must_use]
    pub const fn is_valid(self) -> bool {
        self.0 != u32::MAX
    }
}

impl Default for AttachmentId {
    fn default() -> Self {
        Self::NONE
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § PassId — newtype for pass index in graph
// ════════════════════════════════════════════════════════════════════════════

/// Newtype for the pass index in the graph. Returned by
/// `RenderGraph::add_pass`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PassId(pub u32);

// ════════════════════════════════════════════════════════════════════════════
// § RenderPass — single pass declaration
// ════════════════════════════════════════════════════════════════════════════

/// Single pass declaration. Read/write attachment sets are stored as
/// fixed-capacity arrays to avoid `Vec` allocation per pass.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RenderPass {
    /// Pass kind discriminator.
    pub kind: PassKind,
    /// Number of valid entries in `reads`.
    pub read_count: u8,
    /// Attachments this pass reads from.
    pub reads: [AttachmentId; MAX_ATTACHMENTS_PER_PASS],
    /// Number of valid entries in `writes`.
    pub write_count: u8,
    /// Attachments this pass writes to.
    pub writes: [AttachmentId; MAX_ATTACHMENTS_PER_PASS],
    /// Optional swapchain-output attachment. When set, this is the final
    /// pass in the chain and its output goes to the swapchain image.
    pub swapchain_target: AttachmentId,
}

/// Maximum read or write attachments per pass in stage-0.
pub const MAX_ATTACHMENTS_PER_PASS: usize = 8;

impl RenderPass {
    /// Construct a pass with given kind + empty attachment lists. Caller
    /// fills `reads` + `writes` afterward.
    #[must_use]
    pub const fn new(kind: PassKind) -> Self {
        Self {
            kind,
            read_count: 0,
            reads: [AttachmentId::NONE; MAX_ATTACHMENTS_PER_PASS],
            write_count: 0,
            writes: [AttachmentId::NONE; MAX_ATTACHMENTS_PER_PASS],
            swapchain_target: AttachmentId::NONE,
        }
    }

    /// Add a read attachment. Returns `Err` if the read array is full.
    pub fn read(mut self, a: AttachmentId) -> Self {
        if (self.read_count as usize) < MAX_ATTACHMENTS_PER_PASS {
            self.reads[self.read_count as usize] = a;
            self.read_count += 1;
        }
        self
    }

    /// Add a write attachment.
    pub fn write(mut self, a: AttachmentId) -> Self {
        if (self.write_count as usize) < MAX_ATTACHMENTS_PER_PASS {
            self.writes[self.write_count as usize] = a;
            self.write_count += 1;
        }
        self
    }

    /// Mark this pass as targeting the swapchain.
    pub fn swapchain(mut self, a: AttachmentId) -> Self {
        self.swapchain_target = a;
        self
    }

    /// Iterate over the valid read attachments.
    pub fn read_iter(&self) -> impl Iterator<Item = AttachmentId> + '_ {
        self.reads.iter().take(self.read_count as usize).copied()
    }

    /// Iterate over the valid write attachments.
    pub fn write_iter(&self) -> impl Iterator<Item = AttachmentId> + '_ {
        self.writes.iter().take(self.write_count as usize).copied()
    }

    /// True if this pass writes the given attachment.
    #[must_use]
    pub fn writes_to(&self, a: AttachmentId) -> bool {
        self.write_iter().any(|w| w == a)
    }

    /// True if this pass reads the given attachment.
    #[must_use]
    pub fn reads_from(&self, a: AttachmentId) -> bool {
        self.read_iter().any(|r| r == a)
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § RenderGraph — pass arena + topo-sort
// ════════════════════════════════════════════════════════════════════════════

/// Render graph : list of passes + topo-sort routine.
#[derive(Debug, Default, Clone)]
pub struct RenderGraph {
    /// Passes in registration order.
    pub passes: Vec<RenderPass>,
    /// Cached topological order, populated by `topo_sort`. Empty until sorted.
    pub topo_order: Vec<PassId>,
}

/// Errors produced by render-graph operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum GraphError {
    /// Topo-sort detected a cycle in the read/write dependency graph.
    #[error("render-graph: cycle detected through pass {0:?}")]
    Cycle(PassId),
    /// Pass id out of range.
    #[error("render-graph: invalid pass id {0:?}")]
    InvalidPass(PassId),
}

impl RenderGraph {
    /// Construct an empty graph.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of passes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.passes.len()
    }

    /// True if no passes.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.passes.is_empty()
    }

    /// Add a pass. Returns the new pass ID. Invalidates the cached topo
    /// order — caller should re-sort.
    pub fn add_pass(&mut self, pass: RenderPass) -> PassId {
        let id = PassId(self.passes.len() as u32);
        self.passes.push(pass);
        self.topo_order.clear();
        id
    }

    /// Borrow a pass by id.
    #[must_use]
    pub fn get(&self, id: PassId) -> Option<&RenderPass> {
        self.passes.get(id.0 as usize)
    }

    /// Topological sort the passes by their read/write dependency graph.
    /// Pass A depends on pass B if A reads an attachment B writes. The
    /// resulting order in `self.topo_order` honors all such dependencies :
    /// for any (A, B) where A depends on B, B comes before A.
    ///
    /// Returns `GraphError::Cycle` if a dependency cycle exists.
    pub fn topo_sort(&mut self) -> Result<(), GraphError> {
        let n = self.passes.len();
        if n == 0 {
            self.topo_order.clear();
            return Ok(());
        }

        // Build adjacency : `deps[i]` = passes that pass i depends on.
        // Also count predecessors per pass (in-degree).
        let mut in_degree = vec![0u32; n];
        let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n];

        for (a_idx, a) in self.passes.iter().enumerate() {
            for r in a.read_iter() {
                if !r.is_valid() {
                    continue;
                }
                // Find writers of `r`.
                for (b_idx, b) in self.passes.iter().enumerate() {
                    if a_idx == b_idx {
                        continue;
                    }
                    if b.writes_to(r) {
                        // a reads from r, b writes r ⇒ a depends on b ⇒
                        // edge b → a (b must run before a).
                        adj[b_idx].push(a_idx);
                        in_degree[a_idx] += 1;
                    }
                }
            }
        }

        // Kahn's algorithm.
        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order: Vec<PassId> = Vec::with_capacity(n);

        while let Some(idx) = queue.pop() {
            order.push(PassId(idx as u32));
            // Clone successors to avoid borrow issues.
            let successors: Vec<usize> = adj[idx].clone();
            for s in successors {
                in_degree[s] -= 1;
                if in_degree[s] == 0 {
                    queue.push(s);
                }
            }
        }

        if order.len() != n {
            // Find a pass that's still pending — that's where the cycle is.
            let stuck = (0..n)
                .find(|&i| in_degree[i] > 0)
                .map(|i| PassId(i as u32))
                .unwrap_or(PassId(0));
            return Err(GraphError::Cycle(stuck));
        }

        self.topo_order = order;
        Ok(())
    }

    /// Construct the substrate canonical forward-rendering pipeline :
    /// shadow → geometry → lighting → translucent → tonemap → ui.
    /// Useful as a starting point ; consumers can append custom passes
    /// before calling [`Self::topo_sort`].
    #[must_use]
    pub fn default_forward_pipeline() -> Self {
        let mut g = Self::new();

        // Attachment ids : 0=shadow_atlas, 1=color_hdr, 2=depth, 3=color_lit,
        //                   4=color_tonemapped, 5=swapchain.
        let shadow_atlas = AttachmentId(0);
        let color_hdr = AttachmentId(1);
        let depth = AttachmentId(2);
        let color_lit = AttachmentId(3);
        let color_tonemapped = AttachmentId(4);
        let swapchain = AttachmentId(5);

        // 1. Shadow pass : writes shadow_atlas.
        g.add_pass(RenderPass::new(PassKind::ShadowPass).write(shadow_atlas));
        // 2. Geometry pass : writes color_hdr + depth.
        g.add_pass(
            RenderPass::new(PassKind::GeometryPass)
                .write(color_hdr)
                .write(depth),
        );
        // 3. Lighting pass : reads color_hdr + depth + shadow_atlas, writes color_lit.
        g.add_pass(
            RenderPass::new(PassKind::LightingPass)
                .read(color_hdr)
                .read(depth)
                .read(shadow_atlas)
                .write(color_lit),
        );
        // 4. Translucent pass : reads depth, writes color_lit (additively).
        g.add_pass(
            RenderPass::new(PassKind::TranslucentPass)
                .read(depth)
                .write(color_lit),
        );
        // 5. Tonemap pass : reads color_lit, writes color_tonemapped.
        g.add_pass(
            RenderPass::new(PassKind::TonemapPass)
                .read(color_lit)
                .write(color_tonemapped),
        );
        // 6. UI pass : reads color_tonemapped, writes swapchain.
        g.add_pass(
            RenderPass::new(PassKind::UiPass)
                .read(color_tonemapped)
                .write(swapchain)
                .swapchain(swapchain),
        );

        g
    }
}

// ════════════════════════════════════════════════════════════════════════════
// § Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_graph_has_no_passes() {
        let g = RenderGraph::new();
        assert!(g.is_empty());
        assert_eq!(g.len(), 0);
    }

    #[test]
    fn add_pass_returns_sequential_ids() {
        let mut g = RenderGraph::new();
        let a = g.add_pass(RenderPass::new(PassKind::GeometryPass));
        let b = g.add_pass(RenderPass::new(PassKind::LightingPass));
        assert_eq!(a, PassId(0));
        assert_eq!(b, PassId(1));
    }

    #[test]
    fn pass_builder_chains_attachments() {
        let p = RenderPass::new(PassKind::GeometryPass)
            .read(AttachmentId(1))
            .read(AttachmentId(2))
            .write(AttachmentId(3));
        assert_eq!(p.read_count, 2);
        assert_eq!(p.write_count, 1);
        assert!(p.reads_from(AttachmentId(1)));
        assert!(p.reads_from(AttachmentId(2)));
        assert!(p.writes_to(AttachmentId(3)));
    }

    #[test]
    fn pass_builder_caps_at_max() {
        let mut p = RenderPass::new(PassKind::GeometryPass);
        for i in 0..(MAX_ATTACHMENTS_PER_PASS as u32 + 5) {
            p = p.read(AttachmentId(i));
        }
        // Should NOT panic ; should saturate at MAX.
        assert_eq!(p.read_count as usize, MAX_ATTACHMENTS_PER_PASS);
    }

    #[test]
    fn topo_sort_independent_passes() {
        // Two passes with no dependency : either order is valid.
        let mut g = RenderGraph::new();
        g.add_pass(RenderPass::new(PassKind::GeometryPass).write(AttachmentId(0)));
        g.add_pass(RenderPass::new(PassKind::TonemapPass).write(AttachmentId(1)));
        g.topo_sort().unwrap();
        assert_eq!(g.topo_order.len(), 2);
    }

    #[test]
    fn topo_sort_dependent_chain_orders_correctly() {
        // A writes 0 ; B reads 0 writes 1 ; C reads 1.  Order : A < B < C.
        let mut g = RenderGraph::new();
        let a = g.add_pass(RenderPass::new(PassKind::GeometryPass).write(AttachmentId(0)));
        let b = g.add_pass(
            RenderPass::new(PassKind::LightingPass)
                .read(AttachmentId(0))
                .write(AttachmentId(1)),
        );
        let c = g.add_pass(RenderPass::new(PassKind::TonemapPass).read(AttachmentId(1)));
        g.topo_sort().unwrap();
        let order = &g.topo_order;
        assert_eq!(order.len(), 3);
        let pos_a = order.iter().position(|p| *p == a).unwrap();
        let pos_b = order.iter().position(|p| *p == b).unwrap();
        let pos_c = order.iter().position(|p| *p == c).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }

    #[test]
    fn topo_sort_diamond_orders_root_before_leaves() {
        //   A writes 0
        //   B reads 0, writes 1
        //   C reads 0, writes 2
        //   D reads 1+2
        let mut g = RenderGraph::new();
        let a = g.add_pass(RenderPass::new(PassKind::GeometryPass).write(AttachmentId(0)));
        let b = g.add_pass(
            RenderPass::new(PassKind::LightingPass)
                .read(AttachmentId(0))
                .write(AttachmentId(1)),
        );
        let c = g.add_pass(
            RenderPass::new(PassKind::Custom(1))
                .read(AttachmentId(0))
                .write(AttachmentId(2)),
        );
        let d = g.add_pass(
            RenderPass::new(PassKind::TonemapPass)
                .read(AttachmentId(1))
                .read(AttachmentId(2)),
        );
        g.topo_sort().unwrap();
        let order = &g.topo_order;
        let pos_a = order.iter().position(|p| *p == a).unwrap();
        let pos_b = order.iter().position(|p| *p == b).unwrap();
        let pos_c = order.iter().position(|p| *p == c).unwrap();
        let pos_d = order.iter().position(|p| *p == d).unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_a < pos_c);
        assert!(pos_b < pos_d);
        assert!(pos_c < pos_d);
    }

    #[test]
    fn topo_sort_cycle_detected() {
        // A reads 0 writes 1 ; B reads 1 writes 0 — cycle.
        let mut g = RenderGraph::new();
        g.add_pass(
            RenderPass::new(PassKind::GeometryPass)
                .read(AttachmentId(0))
                .write(AttachmentId(1)),
        );
        g.add_pass(
            RenderPass::new(PassKind::LightingPass)
                .read(AttachmentId(1))
                .write(AttachmentId(0)),
        );
        let res = g.topo_sort();
        assert!(matches!(res, Err(GraphError::Cycle(_))));
    }

    #[test]
    fn default_forward_pipeline_has_six_passes() {
        let g = RenderGraph::default_forward_pipeline();
        assert_eq!(g.len(), 6);
    }

    #[test]
    fn default_forward_pipeline_topo_sorts_clean() {
        let mut g = RenderGraph::default_forward_pipeline();
        g.topo_sort().unwrap();
        assert_eq!(g.topo_order.len(), 6);
    }

    #[test]
    fn default_forward_pipeline_shadow_before_lighting() {
        let mut g = RenderGraph::default_forward_pipeline();
        g.topo_sort().unwrap();
        // Shadow pass id is 0 ; lighting pass id is 2 (per default constructor).
        let shadow_pos = g.topo_order.iter().position(|p| *p == PassId(0)).unwrap();
        let lighting_pos = g.topo_order.iter().position(|p| *p == PassId(2)).unwrap();
        assert!(shadow_pos < lighting_pos);
    }

    #[test]
    fn default_forward_pipeline_geometry_before_lighting() {
        let mut g = RenderGraph::default_forward_pipeline();
        g.topo_sort().unwrap();
        // Geometry = 1 ; Lighting = 2.
        let geom_pos = g.topo_order.iter().position(|p| *p == PassId(1)).unwrap();
        let lit_pos = g.topo_order.iter().position(|p| *p == PassId(2)).unwrap();
        assert!(geom_pos < lit_pos);
    }

    #[test]
    fn default_forward_pipeline_ui_runs_last() {
        let mut g = RenderGraph::default_forward_pipeline();
        g.topo_sort().unwrap();
        // UI pass is id 5, must come after tonemap (id 4).
        let ui_pos = g.topo_order.iter().position(|p| *p == PassId(5)).unwrap();
        let tone_pos = g.topo_order.iter().position(|p| *p == PassId(4)).unwrap();
        assert!(tone_pos < ui_pos);
    }

    #[test]
    fn add_pass_invalidates_cached_topo_order() {
        let mut g = RenderGraph::default_forward_pipeline();
        g.topo_sort().unwrap();
        assert!(!g.topo_order.is_empty());
        g.add_pass(RenderPass::new(PassKind::Custom(99)));
        assert!(g.topo_order.is_empty());
    }

    #[test]
    fn attachment_id_default_is_invalid() {
        assert!(!AttachmentId::default().is_valid());
        assert_eq!(AttachmentId::default(), AttachmentId::NONE);
    }

    #[test]
    fn pass_kind_custom_carries_id() {
        let p = RenderPass::new(PassKind::Custom(42));
        assert_eq!(p.kind, PassKind::Custom(42));
    }
}
