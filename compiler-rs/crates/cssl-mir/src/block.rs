//! MIR blocks + regions (structured-by-construction).
//!
//! § DESIGN
//!   An [`MirBlock`] is a named list of operations (basic-block-like). An
//!   [`MirRegion`] is a sequence of blocks (MLIR region-semantics). Structured
//!   control-flow ops (scf.if / scf.for / scf.while + cssl.region.enter/exit) own
//!   one or more regions.
//!
//!   At stage-0 every CSSLv3 fn compiles to exactly one region with one block
//!   (`^entry`) that contains the top-level ops. Structured control-flow inside
//!   the body becomes nested `MirOp { regions: [inner_region] }`.

use crate::op::CsslOp;
use crate::value::{MirType, MirValue, ValueId};

/// A basic block within a region.
#[derive(Debug, Clone)]
pub struct MirBlock {
    /// Block label, e.g., `"entry"`. MLIR uses `^name`.
    pub label: String,
    /// Block arguments (parameters) — each a typed SSA value.
    pub args: Vec<MirValue>,
    /// Operations in source order.
    pub ops: Vec<MirOp>,
}

impl MirBlock {
    /// Build an empty block with the given label.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            args: Vec::new(),
            ops: Vec::new(),
        }
    }

    /// The canonical `"entry"` block shape used by every fn body.
    #[must_use]
    pub fn entry(args: Vec<MirValue>) -> Self {
        Self {
            label: "entry".into(),
            args,
            ops: Vec::new(),
        }
    }

    /// Append an op to this block.
    pub fn push(&mut self, op: MirOp) {
        self.ops.push(op);
    }
}

/// A region : a sequence of blocks (MLIR region semantics).
#[derive(Debug, Clone, Default)]
pub struct MirRegion {
    pub blocks: Vec<MirBlock>,
}

impl MirRegion {
    /// Empty region.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Region with a single entry-block carrying the given args.
    #[must_use]
    pub fn with_entry(args: Vec<MirValue>) -> Self {
        Self {
            blocks: vec![MirBlock::entry(args)],
        }
    }

    /// Append a block.
    pub fn push(&mut self, block: MirBlock) {
        self.blocks.push(block);
    }

    /// The entry block, if present.
    #[must_use]
    pub fn entry(&self) -> Option<&MirBlock> {
        self.blocks.first()
    }

    /// The entry block mutably, if present.
    pub fn entry_mut(&mut self) -> Option<&mut MirBlock> {
        self.blocks.first_mut()
    }
}

/// A single MIR operation : dialect op + operands + results + optional nested regions
/// + attribute dictionary.
///
/// Attributes are stored as a `Vec<(String, String)>` pairs at stage-0 — structured
/// attribute types (IFC-label, cap, effect-row, source-loc per `specs/15` § DIALECT
/// DEFINITION) are T6-phase-2 work.
#[derive(Debug, Clone)]
pub struct MirOp {
    /// Op variant from the dialect (or `Std` with a free-form name).
    pub op: CsslOp,
    /// Source-form name (used for `Std` ; otherwise matches `op.name()`).
    pub name: String,
    /// Operand values.
    pub operands: Vec<ValueId>,
    /// Result values (typed).
    pub results: Vec<MirValue>,
    /// Attribute dictionary (key-value pairs).
    pub attributes: Vec<(String, String)>,
    /// Nested regions (for `scf.if` / `scf.for` / `cssl.region` / etc.).
    pub regions: Vec<MirRegion>,
}

impl MirOp {
    /// Build a new op with the canonical name.
    #[must_use]
    pub fn new(op: CsslOp) -> Self {
        let name = op.name().to_string();
        Self {
            op,
            name,
            operands: Vec::new(),
            results: Vec::new(),
            attributes: Vec::new(),
            regions: Vec::new(),
        }
    }

    /// Build a `Std` op with a caller-supplied name.
    #[must_use]
    pub fn std(name: impl Into<String>) -> Self {
        Self {
            op: CsslOp::Std,
            name: name.into(),
            operands: Vec::new(),
            results: Vec::new(),
            attributes: Vec::new(),
            regions: Vec::new(),
        }
    }

    /// Builder : add an operand.
    #[must_use]
    pub fn with_operand(mut self, v: ValueId) -> Self {
        self.operands.push(v);
        self
    }

    /// Builder : add a result.
    #[must_use]
    pub fn with_result(mut self, id: ValueId, ty: MirType) -> Self {
        self.results.push(MirValue::new(id, ty));
        self
    }

    /// Builder : add an attribute.
    #[must_use]
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.push((key.into(), value.into()));
        self
    }

    /// Builder : add a nested region.
    #[must_use]
    pub fn with_region(mut self, region: MirRegion) -> Self {
        self.regions.push(region);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{MirBlock, MirOp, MirRegion};
    use crate::op::CsslOp;
    use crate::value::{IntWidth, MirType, MirValue, ValueId};

    #[test]
    fn block_build_and_push() {
        let mut b = MirBlock::new("entry");
        let op = MirOp::new(CsslOp::GpuBarrier);
        b.push(op);
        assert_eq!(b.ops.len(), 1);
        assert_eq!(b.label, "entry");
    }

    #[test]
    fn region_with_entry_has_entry_block() {
        let args = vec![MirValue::new(ValueId(0), MirType::Int(IntWidth::I32))];
        let r = MirRegion::with_entry(args.clone());
        assert_eq!(r.blocks.len(), 1);
        assert_eq!(r.entry().unwrap().args.len(), 1);
    }

    #[test]
    fn mir_op_builder_chain() {
        let op = MirOp::new(CsslOp::HandlePack)
            .with_operand(ValueId(0))
            .with_operand(ValueId(1))
            .with_result(ValueId(2), MirType::Handle)
            .with_attribute("source_loc", "<test>:1:1");
        assert_eq!(op.name, "cssl.handle.pack");
        assert_eq!(op.operands.len(), 2);
        assert_eq!(op.results.len(), 1);
        assert_eq!(op.attributes.len(), 1);
    }

    #[test]
    fn std_op_uses_supplied_name() {
        let op = MirOp::std("arith.addi");
        assert_eq!(op.name, "arith.addi");
        assert_eq!(op.op, CsslOp::Std);
    }

    #[test]
    fn region_with_nested_block() {
        let mut outer = MirRegion::with_entry(Vec::new());
        let inner = MirRegion::with_entry(Vec::new());
        if let Some(b) = outer.entry_mut() {
            b.push(MirOp::new(CsslOp::RegionEnter).with_region(inner));
        }
        assert_eq!(outer.blocks[0].ops[0].regions.len(), 1);
    }
}
