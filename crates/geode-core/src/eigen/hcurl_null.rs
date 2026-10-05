//! The **exact gradient null space** of the order-generic H(curl) eigen
//! pencils (issue #871, Epic #836 Phase 2): its dimension in closed form,
//! the sparse discrete gradient that spans it, and the
//! gradient-fraction classifier the p=2 cavity solves use to tell a
//! curl-free Ritz pair from a physical mode.
//!
//! # The discrete gradient at p=1 and p=2
//!
//! The curl-curl stiffness `K` annihilates the gradients of the matching
//! scalar Lagrange space: P1 at p=1, P2 at p=2. In the hierarchical bases
//! of [`crate::elements::nedelec_p2`] the gradient is exact and sparse.
//!
//! - **Vertex hats.** `∇λ_a = Σ_b W_ba`, so a nodal field `v` maps to the
//!   Whitney coefficients `v_b − v_a` on every edge `a < b` (the p=1 `G`).
//!   At p=2 these are the `W` DOFs ([`HcurlSpace::edge_dofs`]`[0]`, oriented
//!   low→high).
//! - **Edge bubbles** (p=2 only). `∇(λ_aλ_b) = Q_ab` exactly, so the bubble
//!   coefficient `c_e` maps to the `Q` DOF of edge `e` with unit weight.
//! - The face DOFs `(φ0, φ1)` never appear in a gradient.
//!
//! # The exact dimension
//!
//! A scalar `φ = (v, c)` is **admissible** when `∇φ` lies in the kept DOF
//! span and has zero tangential trace on every impedance / London wall the
//! pencil carries (on such a wall `S ∇φ ≠ 0` otherwise, so `∇φ` would not be
//! null). Reading that off the DOF coefficients above:
//!
//! - every edge whose `W` DOF is eliminated (or that lies on a wall) forces
//!   `v_a = v_b`, so `v` is **constant on each connected component** of the
//!   constrained edge graph (a PEC wall, a London wall, …) — the floating
//!   potential of an isolated conductor;
//! - every edge whose `Q` DOF is eliminated (or that lies on a wall) forces
//!   `c_e = 0`; every other bubble is free.
//!
//! Gradients of admissible scalars are null; the gradient's own kernel is
//! the constants, one per connected component of the mesh. Hence
//!
//! ```text
//!   dim(gradient null) = n_free_nodes + n_free_bubbles
//!                        + Σ_domains (n_wall_components(d) − 1),
//! ```
//!
//! where a *free node* has no constrained incident edge, a *free bubble* is
//! the `Q` DOF of an unconstrained edge (p=2 only), and a domain with no
//! wall at all contributes `−1` (its constant). For a domain whose whole
//! boundary is one PEC wall this is the textbook "interior P2-Lagrange
//! DOFs = interior vertices + interior edges" at p=2 (interior vertices at
//! p=1).
//!
//! The full kernel of the reduced `K` is this space plus the discrete
//! **loop harmonics** (relative first cohomology: a field circulating
//! through a handle, e.g. the azimuthal field of an open-ended coax). They
//! are not gradients of single-valued scalars and are not counted here. The
//! fixtures that pin the count exactly (`tests/eigen_p2_tagged.rs`) are
//! simply connected, where that term is zero.
//!
//! # The gradient-fraction classifier
//!
//! With `G` the interior-restricted gradient over the admissible scalar
//! basis ([`GradientNullSpace`]), every physical eigenpair of
//! `K x = λ M x` (`λ ≠ 0`) satisfies `Gᵀ M x = 0` exactly, because
//! `Gᵀ K = (K G)ᵀ = 0`. The **gradient fraction**
//!
//! ```text
//!   g(x) = ‖G (GᵀMG)⁻¹ GᵀM x‖₂ / ‖x‖₂
//! ```
//!
//! is the size of the `M`-orthogonal (bilinear, for a complex `M`) gradient
//! component of `x`: exactly `0` for a physical mode and exactly `1` for a
//! gradient, independent of the shift, the mesh unit and the eigenvalue's
//! magnitude. It is the scale-free complement of the `λ ≤ null_tol_rel·σ`
//! magnitude filter, which can be fooled by a gradient Ritz value that
//! drifts upward (the #852 failure mode).

use std::collections::HashMap;

use faer::sparse::linalg::solvers::Lu;
use faer::sparse::{SparseColMat, SparseColMatRef, Triplet};
use faer::{Mat, c64};

use crate::assembly::hcurl_space::HcurlSpace;
use crate::eigen::dense::EigenError;
use crate::elements::ElementOrder;
use crate::mesh::TetMesh;

/// The closed-form decomposition of the gradient null dimension
/// ([`GradientNullSpace::counts`]; see the module docs).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GradientNullCount {
    /// Element order of the space.
    pub order: ElementOrder,
    /// Nodes with no constrained incident edge (free vertex DOFs of the
    /// scalar space).
    pub free_nodes: usize,
    /// Free edge-bubble DOFs (`Q` kept and the edge on no wall); `0` at p=1.
    pub free_bubbles: usize,
    /// Connected components of the constrained edge graph (PEC walls,
    /// impedance / London walls, merged where they touch).
    pub wall_components: usize,
    /// Connected components of the mesh (one constant each).
    pub domains: usize,
    /// The gradient null dimension
    /// `free_nodes + free_bubbles + Σ_d (wall_components(d) − 1)`.
    pub dim: usize,
}

/// The interior-restricted discrete gradient `G` over the admissible scalar
/// basis of an [`HcurlSpace`] with a PEC mask (and optional walls), plus a
/// factorization of `Gᵀ M G` for the gradient-fraction classifier (see the
/// module docs).
///
/// Columns: one per free node (its hat), one per free edge bubble (p=2),
/// one per wall component (the sum of its nodes' hats, i.e. the floating
/// potential), with one column per domain dropped so `G` has full column
/// rank (a wall-free domain drops its first free node, any other domain
/// drops its first wall component).
#[derive(Debug, Clone)]
pub struct GradientNullSpace {
    n_interior: usize,
    /// Column `j` is `cols[j]`: `(interior row, coefficient)` pairs.
    cols: Vec<Vec<(usize, f64)>>,
    counts: GradientNullCount,
}

/// Union-find with path halving.
struct Dsu(Vec<usize>);
impl Dsu {
    fn new(n: usize) -> Self {
        Self((0..n).collect())
    }
    fn find(&mut self, mut a: usize) -> usize {
        while self.0[a] != a {
            self.0[a] = self.0[self.0[a]];
            a = self.0[a];
        }
        a
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.0[ra.max(rb)] = ra.min(rb);
        }
    }
}

impl GradientNullSpace {
    /// Build the gradient basis of `space` on `mesh` under the DOF mask
    /// `pec_interior_mask` (`true` = kept, length `space.n_dofs()`), with
    /// `walls` the triangle lists of every surface term the pencil carries
    /// on its kept DOFs (London / impedance walls; PEC walls are read from
    /// the mask and need not be listed).
    ///
    /// Interior rows are numbered contiguously in full-DOF order (the
    /// convention of every cavity pencil).
    ///
    /// # Panics
    ///
    /// Panics if the mask length is not `space.n_dofs()`, if `mesh` is not
    /// the space's mesh, or if a wall triangle edge is not a mesh edge
    /// (callers validate walls first).
    pub fn build(
        space: &HcurlSpace,
        mesh: &TetMesh,
        pec_interior_mask: &[bool],
        walls: &[&[[u32; 3]]],
    ) -> Self {
        assert_eq!(pec_interior_mask.len(), space.n_dofs(), "mask length");
        assert_eq!(mesh.n_nodes(), space.n_nodes(), "space / mesh mismatch");
        let order = space.order();
        let n_nodes = mesh.n_nodes();
        let edges = space.edges();
        let n_edges = edges.len();

        let mut remap = vec![usize::MAX; space.n_dofs()];
        let mut n_interior = 0usize;
        for (g, &keep) in pec_interior_mask.iter().enumerate() {
            if keep {
                remap[g] = n_interior;
                n_interior += 1;
            }
        }

        // Edges on a wall (their whole tangential trace is penalized).
        let mut on_wall = vec![false; n_edges];
        for list in walls {
            for tri in *list {
                for (a, b) in [(tri[0], tri[1]), (tri[0], tri[2]), (tri[1], tri[2])] {
                    let key = [a.min(b), a.max(b)];
                    let ge = edges
                        .binary_search(&key)
                        .expect("wall triangle edge is not a mesh edge");
                    on_wall[ge] = true;
                }
            }
        }
        // W row / Q row (interior index or MAX) of every edge.
        let w_row: Vec<usize> = (0..n_edges)
            .map(|e| remap[space.edge_dofs(e)[0] as usize])
            .collect();
        let q_row: Vec<usize> = (0..n_edges)
            .map(|e| match order {
                ElementOrder::P1 => usize::MAX,
                ElementOrder::P2 => remap[space.edge_dofs(e)[1] as usize],
            })
            .collect();

        // Constrained edges: W eliminated or on a wall. Their endpoints are
        // tied to one potential value; components via union-find.
        let constrained: Vec<bool> = (0..n_edges)
            .map(|e| w_row[e] == usize::MAX || on_wall[e])
            .collect();
        let mut tied = vec![false; n_nodes];
        let mut walls_dsu = Dsu::new(n_nodes);
        for (e, &[a, b]) in edges.iter().enumerate() {
            if constrained[e] {
                tied[a as usize] = true;
                tied[b as usize] = true;
                walls_dsu.union(a as usize, b as usize);
            }
        }
        // Mesh connected components (domains).
        let mut dom = Dsu::new(n_nodes);
        for t in &mesh.tets {
            for i in 1..4 {
                dom.union(t[0] as usize, t[i] as usize);
            }
        }
        let used: Vec<bool> = {
            let mut u = vec![false; n_nodes];
            for t in &mesh.tets {
                for &v in t {
                    u[v as usize] = true;
                }
            }
            u
        };

        // Scalar columns, in deterministic order: free nodes, wall
        // components, bubbles. Track the first column of each kind per
        // domain for the constant pin.
        enum Col {
            Node(usize),
            Wall(usize),
            Bubble(usize),
        }
        let mut cols_spec: Vec<(Col, usize)> = Vec::new(); // (col, domain root)
        let mut wall_comp_of: HashMap<usize, usize> = HashMap::new(); // dsu root → wall col idx
        let mut free_nodes = 0usize;
        for n in 0..n_nodes {
            if !used[n] {
                continue;
            }
            let d = dom.find(n);
            if tied[n] {
                let r = walls_dsu.find(n);
                if let std::collections::hash_map::Entry::Vacant(v) = wall_comp_of.entry(r) {
                    v.insert(cols_spec.len());
                    cols_spec.push((Col::Wall(r), d));
                }
            } else {
                free_nodes += 1;
                cols_spec.push((Col::Node(n), d));
            }
        }
        let wall_components = wall_comp_of.len();
        let mut free_bubbles = 0usize;
        if order == ElementOrder::P2 {
            for e in 0..n_edges {
                if q_row[e] != usize::MAX && !on_wall[e] {
                    free_bubbles += 1;
                    let d = dom.find(edges[e][0] as usize);
                    cols_spec.push((Col::Bubble(e), d));
                }
            }
        }

        // Pin one column per domain: the first wall component if the domain
        // has one, else its first free node.
        let mut first_wall: HashMap<usize, usize> = HashMap::new();
        let mut first_node: HashMap<usize, usize> = HashMap::new();
        for (j, (c, d)) in cols_spec.iter().enumerate() {
            match c {
                Col::Wall(_) => {
                    first_wall.entry(*d).or_insert(j);
                }
                Col::Node(_) => {
                    first_node.entry(*d).or_insert(j);
                }
                Col::Bubble(_) => {}
            }
        }
        let mut domains: Vec<usize> = (0..n_nodes)
            .filter(|&n| used[n])
            .map(|n| dom.find(n))
            .collect();
        domains.sort_unstable();
        domains.dedup();
        let mut pinned = vec![false; cols_spec.len()];
        for d in &domains {
            if let Some(&j) = first_wall.get(d).or_else(|| first_node.get(d)) {
                pinned[j] = true;
            }
        }

        // Node → incident edges, for the hat / floating-potential columns.
        let mut incident: Vec<Vec<usize>> = vec![Vec::new(); n_nodes];
        for (e, &[a, b]) in edges.iter().enumerate() {
            incident[a as usize].push(e);
            incident[b as usize].push(e);
        }
        // Wall component → member nodes.
        let mut members: HashMap<usize, Vec<usize>> = HashMap::new();
        for n in 0..n_nodes {
            if used[n] && tied[n] {
                members.entry(walls_dsu.find(n)).or_default().push(n);
            }
        }

        let mut cols: Vec<Vec<(usize, f64)>> = Vec::new();
        for (j, (c, _)) in cols_spec.iter().enumerate() {
            if pinned[j] {
                continue;
            }
            let mut col: Vec<(usize, f64)> = Vec::new();
            match c {
                Col::Node(n) => {
                    for &e in &incident[*n] {
                        // v = hat_n: W_e coefficient v_b − v_a, (a < b).
                        let coeff = if edges[e][1] as usize == *n {
                            1.0
                        } else {
                            -1.0
                        };
                        debug_assert_ne!(w_row[e], usize::MAX);
                        col.push((w_row[e], coeff));
                    }
                }
                Col::Wall(r) => {
                    let mut acc: HashMap<usize, f64> = HashMap::new();
                    for &n in &members[r] {
                        for &e in &incident[n] {
                            let coeff = if edges[e][1] as usize == n { 1.0 } else { -1.0 };
                            *acc.entry(e).or_insert(0.0) += coeff;
                        }
                    }
                    let mut es: Vec<(usize, f64)> =
                        acc.into_iter().filter(|&(_, v)| v != 0.0).collect();
                    es.sort_unstable_by_key(|&(e, _)| e);
                    for (e, v) in es {
                        // An edge leaving the component is unconstrained,
                        // hence kept.
                        debug_assert_ne!(w_row[e], usize::MAX);
                        col.push((w_row[e], v));
                    }
                }
                Col::Bubble(e) => col.push((q_row[*e], 1.0)),
            }
            col.sort_unstable_by_key(|&(r, _)| r);
            cols.push(col);
        }

        let dim = cols.len();
        debug_assert_eq!(
            dim + domains.len(),
            free_nodes + free_bubbles + wall_components
        );
        Self {
            n_interior,
            cols,
            counts: GradientNullCount {
                order,
                free_nodes,
                free_bubbles,
                wall_components,
                domains: domains.len(),
                dim,
            },
        }
    }

    /// The closed-form count (see the module docs).
    pub fn counts(&self) -> GradientNullCount {
        self.counts
    }

    /// The gradient null dimension, [`GradientNullCount::dim`].
    pub fn dim(&self) -> usize {
        self.counts.dim
    }

    /// Interior DOF count (the row dimension of `G`).
    pub fn n_interior(&self) -> usize {
        self.n_interior
    }

    /// Column `j` of `G` as `(interior row, coefficient)` pairs, sorted by
    /// row.
    pub fn column(&self, j: usize) -> &[(usize, f64)] {
        &self.cols[j]
    }

    /// `G φ` (length [`GradientNullSpace::n_interior`]).
    ///
    /// # Panics
    ///
    /// Panics if `phi.len() != self.dim()`.
    pub fn apply(&self, phi: &[c64]) -> Vec<c64> {
        assert_eq!(phi.len(), self.dim());
        let mut out = vec![c64::new(0.0, 0.0); self.n_interior];
        for (col, &p) in self.cols.iter().zip(phi) {
            for &(r, v) in col {
                out[r] += p * v;
            }
        }
        out
    }

    /// `Gᵀ y` (length [`GradientNullSpace::dim`]).
    ///
    /// # Panics
    ///
    /// Panics if `y.len() != self.n_interior()`.
    pub fn apply_transpose(&self, y: &[c64]) -> Vec<c64> {
        assert_eq!(y.len(), self.n_interior);
        self.cols
            .iter()
            .map(|col| col.iter().map(|&(r, v)| y[r] * v).sum())
            .collect()
    }

    /// Factor `Gᵀ M G` for the gradient-fraction classifier, with `M` the
    /// interior mass of the pencil (complex symmetric; pass a real `M` as
    /// a zero-imaginary complex matrix with [`real_to_complex`]).
    ///
    /// # Errors
    ///
    /// [`EigenError::FaerGevd`] if `M` has the wrong shape or the sparse
    /// LU of `Gᵀ M G` fails.
    pub fn classifier<'a>(
        &'a self,
        m: SparseColMatRef<'a, usize, c64>,
    ) -> Result<GradientClassifier<'a>, EigenError> {
        if m.nrows() != self.n_interior || m.ncols() != self.n_interior {
            return Err(EigenError::FaerGevd(format!(
                "gradient classifier: M is {}×{}, G has {} interior rows",
                m.nrows(),
                m.ncols(),
                self.n_interior
            )));
        }
        // Row → (column, coeff) incidence of G.
        let mut row_cols: Vec<Vec<(usize, f64)>> = vec![Vec::new(); self.n_interior];
        for (j, col) in self.cols.iter().enumerate() {
            for &(r, v) in col {
                row_cols[r].push((j, v));
            }
        }
        let mut trips: Vec<Triplet<usize, usize, c64>> = Vec::new();
        for c in 0..m.ncols() {
            if row_cols[c].is_empty() {
                continue;
            }
            for (r, &v) in m.row_idx_of_col(c).zip(m.val_of_col(c)) {
                for &(jr, gr) in &row_cols[r] {
                    for &(jc, gc) in &row_cols[c] {
                        trips.push(Triplet::new(jr, jc, v * (gr * gc)));
                    }
                }
            }
        }
        let n = self.dim();
        let lu = if n == 0 {
            None
        } else {
            let l = SparseColMat::<usize, c64>::try_new_from_triplets(n, n, &trips)
                .map_err(|e| EigenError::FaerGevd(format!("GᵀMG assembly: {e:?}")))?;
            Some(
                l.as_ref()
                    .sp_lu()
                    .map_err(|e| EigenError::FaerGevd(format!("GᵀMG sparse LU: {e:?}")))?,
            )
        };
        Ok(GradientClassifier { null: self, m, lu })
    }
}

/// A factored `Gᵀ M G` that evaluates the gradient fraction
/// `g(x) = ‖G (GᵀMG)⁻¹ GᵀM x‖₂ / ‖x‖₂` of interior vectors (module docs).
pub struct GradientClassifier<'a> {
    null: &'a GradientNullSpace,
    m: SparseColMatRef<'a, usize, c64>,
    lu: Option<Lu<usize, c64>>,
}

impl std::fmt::Debug for GradientClassifier<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GradientClassifier")
            .field("dim", &self.null.dim())
            .field("n_interior", &self.null.n_interior())
            .finish()
    }
}

impl GradientClassifier<'_> {
    /// The gradient fraction of the complex interior vector `x`: `0` for a
    /// physical mode, `1` for a gradient (module docs). `0` when the
    /// gradient space is empty or `x = 0`.
    ///
    /// # Panics
    ///
    /// Panics if `x.len()` is not the interior dimension.
    pub fn gradient_fraction(&self, x: &[c64]) -> f64 {
        use faer::linalg::solvers::Solve;
        assert_eq!(x.len(), self.null.n_interior(), "vector length");
        let norm = |v: &[c64]| v.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        let nx = norm(x);
        let Some(lu) = self.lu.as_ref() else {
            return 0.0;
        };
        if nx == 0.0 {
            return 0.0;
        }
        let mut mx = vec![c64::new(0.0, 0.0); x.len()];
        for (j, &xj) in x.iter().enumerate() {
            if xj == c64::new(0.0, 0.0) {
                continue;
            }
            for (i, v) in self.m.row_idx_of_col(j).zip(self.m.val_of_col(j)) {
                mx[i] += *v * xj;
            }
        }
        let y = self.null.apply_transpose(&mx);
        let mut rhs = Mat::<c64>::from_fn(y.len(), 1, |i, _| y[i]);
        lu.solve_in_place(rhs.as_mut());
        let phi: Vec<c64> = (0..y.len()).map(|i| rhs[(i, 0)]).collect();
        norm(&self.null.apply(&phi)) / nx
    }

    /// [`GradientClassifier::gradient_fraction`] of a real vector.
    pub fn gradient_fraction_real(&self, x: &[f64]) -> f64 {
        let xc: Vec<c64> = x.iter().map(|&v| c64::new(v, 0.0)).collect();
        self.gradient_fraction(&xc)
    }
}

/// The gradient fraction above which a Ritz pair is classified as
/// curl-free (gradient null space) rather than physical. The fraction is
/// `0` for a physical mode and `1` for a gradient to the solver's accuracy,
/// so the midpoint separates them with the widest margin.
pub const GRADIENT_FRACTION_CUT: f64 = 0.5;

/// A real sparse matrix as a zero-imaginary complex one (same pattern).
///
/// # Errors
///
/// [`EigenError::FaerGevd`] if the copy fails to assemble.
pub fn real_to_complex(
    a: SparseColMatRef<'_, usize, f64>,
) -> Result<SparseColMat<usize, c64>, EigenError> {
    let mut trips = Vec::with_capacity(a.compute_nnz());
    for j in 0..a.ncols() {
        for (i, &v) in a.row_idx_of_col(j).zip(a.val_of_col(j)) {
            trips.push(Triplet::new(i, j, c64::new(v, 0.0)));
        }
    }
    SparseColMat::<usize, c64>::try_new_from_triplets(a.nrows(), a.ncols(), &trips)
        .map_err(|e| EigenError::FaerGevd(format!("real → complex copy: {e:?}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    fn walls_of_cube(mesh: &TetMesh) -> Vec<[u32; 3]> {
        mesh.boundary_faces()
    }

    /// The count formula on the unit cube: all-PEC at p=2 is interior
    /// vertices + interior edges; no PEC is all nodes + all edges − 1.
    #[test]
    fn counts_match_interior_scalar_dofs() {
        let mesh = cube_tet_mesh(2, 1.0);
        let bnd = walls_of_cube(&mesh);
        for order in [ElementOrder::P1, ElementOrder::P2] {
            let space = HcurlSpace::build(&mesh, order);
            let mask = space.pec_interior_mask(&mesh, &[&bnd]).unwrap();
            let null = GradientNullSpace::build(&space, &mesh, &mask, &[]);
            let interior_nodes = 1; // the cube centre
            let bnd_edges: std::collections::BTreeSet<[u32; 2]> = bnd
                .iter()
                .flat_map(|t| {
                    let mut s = *t;
                    s.sort_unstable();
                    [[s[0], s[1]], [s[0], s[2]], [s[1], s[2]]]
                })
                .collect();
            let interior_edges = space.n_edges() - bnd_edges.len();
            let want = match order {
                ElementOrder::P1 => interior_nodes,
                ElementOrder::P2 => interior_nodes + interior_edges,
            };
            assert_eq!(null.dim(), want, "{order:?}");
            assert_eq!(null.counts().wall_components, 1);

            let free = vec![true; space.n_dofs()];
            let null = GradientNullSpace::build(&space, &mesh, &free, &[]);
            let want = match order {
                ElementOrder::P1 => mesh.n_nodes() - 1,
                ElementOrder::P2 => mesh.n_nodes() + space.n_edges() - 1,
            };
            assert_eq!(null.dim(), want, "{order:?} free");
        }
    }
}
