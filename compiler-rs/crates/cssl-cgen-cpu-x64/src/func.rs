//! § X64Func + X64Block — top-level containers.
//!
//! § DESIGN
//!   An [`X64Func`] is the post-selection MIR-fn analog : signature + list of
//!   [`X64Block`]s + entry-block index + monotonic vreg counter. Each
//!   [`X64Block`] carries an ordered list of [`X64Inst`]s + a single
//!   terminator [`X64Term`].
//!
//!   The structure mirrors what the cranelift-frontend `Function` looks like
//!   after instruction-selection but before register-allocation : every value
//!   is a virtual register, every block ends in exactly one terminator, and
//!   block-ids are dense `u32`s starting at 0.

use crate::inst::{BlockId, X64Inst, X64Term};
use crate::vreg::{X64VReg, X64Width};

/// Function signature — vreg-typed param + result widths.
///
/// At G1 the signature is just a list of widths because vreg ids in the
/// signature are the entry-block parameters (allocated 1..=N). G3 (ABI
/// lowering) reads this signature to lay out the call frame ; G4 (encoder)
/// reads it to emit the correct prologue / epilogue.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct X64Signature {
    /// Parameter widths in order. Entry-block vregs are allocated at ids
    /// 1..=params.len() ; id 0 is reserved as the null-vreg sentinel.
    pub params: Vec<X64Width>,
    /// Result widths in order. Empty = void return.
    pub results: Vec<X64Width>,
}

impl X64Signature {
    /// Empty signature (`fn () -> ()`).
    #[must_use]
    pub const fn empty() -> Self {
        Self {
            params: Vec::new(),
            results: Vec::new(),
        }
    }

    /// New signature.
    #[must_use]
    pub const fn new(params: Vec<X64Width>, results: Vec<X64Width>) -> Self {
        Self { params, results }
    }
}

/// A basic block — ordered list of instructions ending in one terminator.
#[derive(Debug, Clone, PartialEq)]
pub struct X64Block {
    /// Block id (matches index into [`X64Func::blocks`]).
    pub id: BlockId,
    /// Sequence of instructions.
    pub insts: Vec<X64Inst>,
    /// Terminator. Always present after selection ; constructed via
    /// [`Self::with_terminator`].
    pub terminator: X64Term,
}

impl X64Block {
    /// New block with the given id + initial empty inst list + a placeholder
    /// terminator. Caller must fill in the terminator before sealing the
    /// block ; the placeholder is `Unreachable`.
    #[must_use]
    pub fn new(id: BlockId) -> Self {
        Self {
            id,
            insts: Vec::new(),
            terminator: X64Term::Unreachable,
        }
    }

    /// Replace the terminator. Returns `self` for fluent construction.
    #[must_use]
    pub fn with_terminator(mut self, t: X64Term) -> Self {
        self.terminator = t;
        self
    }

    /// Append an instruction.
    pub fn push(&mut self, inst: X64Inst) {
        self.insts.push(inst);
    }
}

/// A function in [`X64Inst`] form. Output of [`crate::select_function`] +
/// input to G2 register-allocator.
#[derive(Debug, Clone, PartialEq)]
pub struct X64Func {
    /// Source-form fn name (mirrors `MirFunc::name`).
    pub name: String,
    /// Signature.
    pub sig: X64Signature,
    /// Blocks in id order. Index N corresponds to `BlockId(N)`.
    pub blocks: Vec<X64Block>,
    /// Entry-block index (always 0 at G1 ; G2 may reorder for layout).
    pub entry: BlockId,
    /// Next unused vreg id. The selector allocates ids monotonically
    /// starting at 1 (id 0 is the null-sentinel). G2 treats this as the
    /// upper bound of the vreg id-space.
    pub next_vreg_id: u32,
}

impl X64Func {
    /// Build a new func with the given name + signature. The entry block is
    /// pre-created (block 0) with no instructions and a placeholder
    /// `Unreachable` terminator. Param vregs are allocated at ids 1..=N.
    #[must_use]
    pub fn new(name: impl Into<String>, sig: X64Signature) -> Self {
        let entry_block = X64Block::new(BlockId::ENTRY);
        let next_vreg_id = (sig.params.len() as u32) + 1; // +1 because id 0 is sentinel
        Self {
            name: name.into(),
            sig,
            blocks: vec![entry_block],
            entry: BlockId::ENTRY,
            next_vreg_id,
        }
    }

    /// Allocate a fresh vreg with the given width.
    pub fn fresh_vreg(&mut self, width: X64Width) -> X64VReg {
        let id = self.next_vreg_id;
        self.next_vreg_id = self.next_vreg_id.saturating_add(1);
        X64VReg::new(id, width)
    }

    /// Allocate a fresh block. Returns its id.
    pub fn fresh_block(&mut self) -> BlockId {
        let id = BlockId(self.blocks.len() as u32);
        self.blocks.push(X64Block::new(id));
        id
    }

    /// Return the param vreg for parameter index `i`. Param vregs are
    /// allocated at ids `1..=params.len()` (id 0 is the null-sentinel).
    ///
    /// # Panics
    /// Panics if `i >= self.sig.params.len()`.
    #[must_use]
    pub fn param_vreg(&self, i: usize) -> X64VReg {
        let width = self.sig.params[i];
        // ‼ Convention : params occupy vreg ids 1..=N (id 0 reserved as
        //   sentinel). The selector mints them in this order at fn-entry.
        X64VReg::new((i as u32) + 1, width)
    }

    /// Push an instruction onto the given block.
    ///
    /// # Panics
    /// Panics if `block.0 >= self.blocks.len()` (debug-asserts on bounds).
    pub fn push_inst(&mut self, block: BlockId, inst: X64Inst) {
        debug_assert!(
            (block.0 as usize) < self.blocks.len(),
            "block id out of range"
        );
        self.blocks[block.0 as usize].push(inst);
    }

    /// Set the terminator on the given block.
    ///
    /// # Panics
    /// Panics if `block.0 >= self.blocks.len()` (debug-asserts on bounds).
    pub fn set_terminator(&mut self, block: BlockId, t: X64Term) {
        debug_assert!(
            (block.0 as usize) < self.blocks.len(),
            "block id out of range"
        );
        self.blocks[block.0 as usize].terminator = t;
    }
}

#[cfg(test)]
mod tests {
    use super::{BlockId, X64Block, X64Func, X64Signature, X64Term};
    use crate::inst::X64Inst;
    use crate::vreg::{X64VReg, X64Width};

    #[test]
    fn empty_signature() {
        let s = X64Signature::empty();
        assert!(s.params.is_empty());
        assert!(s.results.is_empty());
    }

    #[test]
    fn signature_new_records_widths() {
        let s = X64Signature::new(vec![X64Width::I32, X64Width::I64], vec![X64Width::I32]);
        assert_eq!(s.params.len(), 2);
        assert_eq!(s.results.len(), 1);
        assert_eq!(s.results[0], X64Width::I32);
    }

    #[test]
    fn block_new_has_unreachable_placeholder() {
        let b = X64Block::new(BlockId(2));
        assert_eq!(b.id, BlockId(2));
        assert!(b.insts.is_empty());
        assert_eq!(b.terminator, X64Term::Unreachable);
    }

    #[test]
    fn block_with_terminator_overrides() {
        let b = X64Block::new(BlockId(1)).with_terminator(X64Term::Jmp { target: BlockId(2) });
        assert_eq!(b.terminator, X64Term::Jmp { target: BlockId(2) });
    }

    #[test]
    fn func_new_creates_entry_block() {
        let sig = X64Signature::new(vec![X64Width::I32, X64Width::I32], vec![X64Width::I32]);
        let f = X64Func::new("add", sig);
        assert_eq!(f.name, "add");
        assert_eq!(f.blocks.len(), 1);
        assert_eq!(f.entry, BlockId::ENTRY);
        // Param vregs at ids 1, 2 ; next-vreg-id starts at 3.
        assert_eq!(f.next_vreg_id, 3);
    }

    #[test]
    fn func_fresh_vreg_increments() {
        let mut f = X64Func::new("foo", X64Signature::empty());
        assert_eq!(f.next_vreg_id, 1); // 0 is sentinel ; counter starts at 1
        let v0 = f.fresh_vreg(X64Width::I32);
        let v1 = f.fresh_vreg(X64Width::F32);
        assert_eq!(v0.id, 1);
        assert_eq!(v1.id, 2);
        assert_ne!(v0, v1);
        assert_eq!(v0.width, X64Width::I32);
        assert_eq!(v1.width, X64Width::F32);
    }

    #[test]
    fn func_fresh_block_returns_monotonic_ids() {
        let mut f = X64Func::new("foo", X64Signature::empty());
        let b1 = f.fresh_block();
        let b2 = f.fresh_block();
        assert_eq!(b1, BlockId(1));
        assert_eq!(b2, BlockId(2));
        assert_eq!(f.blocks.len(), 3); // entry + 2 fresh
    }

    #[test]
    fn func_param_vreg_uses_one_based_ids() {
        let sig = X64Signature::new(vec![X64Width::I32, X64Width::F32], vec![]);
        let f = X64Func::new("foo", sig);
        // Param 0 → vreg id 1 ; param 1 → vreg id 2.
        let p0 = f.param_vreg(0);
        let p1 = f.param_vreg(1);
        assert_eq!(p0, X64VReg::new(1, X64Width::I32));
        assert_eq!(p1, X64VReg::new(2, X64Width::F32));
    }

    #[test]
    fn func_push_inst_appends_to_block() {
        let mut f = X64Func::new("foo", X64Signature::empty());
        let v = f.fresh_vreg(X64Width::I32);
        f.push_inst(
            BlockId::ENTRY,
            X64Inst::MovImm {
                dst: v,
                imm: crate::inst::X64Imm::I32(42),
            },
        );
        assert_eq!(f.blocks[0].insts.len(), 1);
    }

    #[test]
    fn func_set_terminator_updates_block() {
        let mut f = X64Func::new("foo", X64Signature::empty());
        f.set_terminator(BlockId::ENTRY, X64Term::Ret { operands: vec![] });
        assert_eq!(f.blocks[0].terminator, X64Term::Ret { operands: vec![] });
    }
}
