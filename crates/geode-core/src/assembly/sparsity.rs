//! Burn-free CPU-side sparsity pattern shared by the assembly modules.
//!
//! Lives outside [`crate::assembly::p1`] (which is tensor-backed) so the
//! electrostatic assembly builds without the `tensor` feature (issue #1365).
//! [`crate::assembly::p1::SparsityPattern`] remains a stable re-export.

/// CPU-side sparsity pattern: every `(row, col)` pair touched at least
/// once by the assembly. Always symmetric for P1 stiffness/mass.
#[derive(Debug, Clone, Default)]
pub struct SparsityPattern {
    pub rows: Vec<u32>,
    pub cols: Vec<u32>,
}

impl SparsityPattern {
    /// Returns the number of unique non-zero entries.
    pub fn nnz(&self) -> usize {
        self.rows.len()
    }
}
