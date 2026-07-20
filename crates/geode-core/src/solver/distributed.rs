//! **Distributed matrix-free Krylov abstraction** + a single-process mock
//! transport (#546 Phase B).
//!
//! Phase A ([`crate::mesh::partition`]) cut the edge-DOF graph into balanced
//! parts and derived, for each part, which off-part DOFs its element-local
//! apply reads (the halo receive set) and the transpose send lists. What it did
//! **not** give us is a solver that runs *over* that decomposition. Every Krylov
//! loop in the crate is single-address-space:
//!
//! - [`crate::solver::ksp::Cocg`] runs its dots / axpys on host `Vec<c64>` over
//!   one assembled `faer` matrix.
//! - [`crate::solver::ksp_burn::BurnCocg`] keeps the iteration vectors on-device
//!   but as **one** monolithic tensor, applying the whole
//!   [`MatrixFreeNedelecOperator`] in one process.
//!
//! A distributed solve needs two operations those loops don't have:
//!
//! 1. a **partitioned operator apply** — each part applies its own
//!    [`MatrixFreeNedelecOperator`] over its owned elements and exchanges halo
//!    values with its neighbours before the scatter-add completes, and
//! 2. **distributed reductions** — the COCG inner products (`ρ = rᵀz`, `pᵀq`)
//!    become a local partial sum plus an all-reduce across parts.
//!
//! This module introduces the abstraction and validates it with a
//! **single-process mock transport** — no GPU, no cluster, no FFI. The whole
//! distributed Krylov loop runs and is tested on one machine.
//!
//! # The pieces
//!
//! - [`DistributedVector`] — a global edge-DOF vector held as one
//!   [`SplitComplex`] tensor pair **per
//!   part**. Each part's buffer is kept in *canonical* form: it is nonzero only
//!   on the DOFs that part **owns** (interior ∪ interface, per the #634
//!   [`Partition`]). Because ownership is a disjoint cover, distributed inner
//!   products are then a plain per-part dot summed across parts — no
//!   double-counting, no masks (see [`distributed_dot`]).
//! - [`Collective`] — the transport trait with exactly the two operations a
//!   distributed matrix-free solve needs: **halo exchange** (forward
//!   [`Collective::halo_gather`] + reverse [`Collective::halo_accumulate`]) and
//!   **all-reduce** ([`Collective::all_reduce_sum`]). [`LocalMock`] implements
//!   it in one process by moving halo / reduction data with an in-memory copy.
//! - [`DistributedOperator`] — wraps one [`MatrixFreeNedelecOperator`] per part
//!   (built over that part's owned elements, over the global edge numbering) and
//!   completes each `apply_k` / `apply_m` / `apply_combination` via a halo
//!   exchange keyed to the partition's send/receive lists.
//! - [`DistributedCocg`] — the complex-symmetric COCG of
//!   [`Cocg`](crate::solver::ksp::Cocg) / [`BurnCocg`](crate::solver::ksp_burn::BurnCocg)
//!   generalized over the abstraction: its inner products are
//!   local-partial-sum + all-reduce and its operator apply is the partitioned
//!   apply above.
//!
//! # Why the mock is exact
//!
//! The partitioned apply is a **reordering of the same sum** the single-process
//! matvec computes. For a DOF `d` owned by part `p`, the single-process
//! scatter-add forms `y[d] = Σ_e (contribution of element e incident to d)`.
//! The distributed apply lets part `p` compute the terms from its own elements
//! and every other part `q` compute the terms from *its* elements incident to
//! `d`; the reverse halo exchange ships those partials back to `p`, which sums
//! them. Same terms, same sum, to round-off. Distribution changes *where* work
//! runs, not the answer — which is exactly what the `P ∈ {1,2,4,8}`
//! spectrum-match tests assert.
//!
//! # The tensor-IR-vs-FFI collectives boundary (the epic's headline decision)
//!
//! #546's central open question is whether the two collectives — halo exchange
//! and all-reduce — can stay inside the Burn/cubecl tensor IR or must drop to an
//! NCCL / NVSHMEM FFI when Phase C runs on real multi-GPU hardware. Prototyping
//! them here, in software, answers it. The decision, recorded in full in
//! `docs/research/2026-07-20-distributed-collectives-ir-vs-ffi.md`:
//!
//! - **Halo exchange is expressible as tensor IR** *on each rank*. The gather
//!   (`select` at the halo-DOF index set) and the scatter-add
//!   (`scatter(…, Add)`) are the **same two primitives** the matrix-free matvec
//!   already uses ([`MatrixFreeNedelecOperator`] gathers each tet's six DOFs and
//!   scatter-adds the local result). The *index sets* are the static
//!   [`HaloChannel::dofs`](crate::mesh::partition::HaloChannel) lists from #634.
//!   What tensor IR cannot express is the **inter-rank data movement itself**:
//!   moving a neighbour's boundary values into this rank's ghost slots is a
//!   point-to-point transfer between two devices' address spaces, which has no
//!   representation in a single-device tensor graph. That transfer is the
//!   **FFI boundary** — `ncclSend`/`ncclRecv` (or NVSHMEM `put`) in Phase C. The
//!   mock stands in for exactly that transfer with an in-memory copy
//!   ([`LocalMock::halo_gather`] / [`LocalMock::halo_accumulate`]).
//! - **All-reduce is a two-layer split.** The *local partial reduction* is pure
//!   tensor IR (the `sum()` in [`SplitComplex::bilinear_dot`]
//!   already produces the per-rank partial). The *cross-rank combine* of those
//!   O(1) scalars is `ncclAllReduce` — a second, tiny FFI call. The mock's
//!   [`LocalMock::all_reduce_sum`] is that combine, on host, over the per-part
//!   partial scalars.
//!
//! So the boundary is crisp and the same for both collectives: **the arithmetic
//! stays tensor-native; only the inter-rank byte movement is FFI.** The
//! abstraction in this module is drawn on that line — [`Collective`] is the
//! narrow FFI seam, everything above it ([`DistributedOperator`],
//! [`DistributedCocg`]) is backend-generic tensor code that never names a
//! transport.

use burn::tensor::backend::Backend;
use burn::tensor::{Int, Tensor, TensorData};
use faer::c64;

use crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator;
use crate::assembly::p1::upload_mesh;
use crate::mesh::TetMesh;
use crate::mesh::partition::Partition;
use crate::solver::ksp::{KspError, KspReport};
use crate::solver::ksp_burn::SplitComplex;

// ---------------------------------------------------------------------------
// DistributedVector
// ---------------------------------------------------------------------------

/// A global edge-DOF vector spread across `k` parts, one on-device
/// [`SplitComplex`] pair per part.
///
/// **Canonical form.** Each part's buffer is full length (`n_edges`) but is only
/// authoritative — and, in canonical form, only nonzero — on the DOFs that part
/// **owns** (its `interior ∪ interface` set from the [`Partition`]). Ownership
/// is a disjoint cover, so a canonical distributed vector represents the global
/// vector without overlap: summing the parts reconstructs it exactly
/// ([`gather_global`](Self::gather_global)), and a distributed inner product is
/// a plain per-part dot summed across parts ([`distributed_dot`]).
///
/// The full-length-per-part storage is a single-process **mock** convenience: a
/// real distributed backend would store a compact local vector per rank
/// (owned + ghost slots only). The halo exchange moves only the interface DOFs
/// either way, so the abstraction is identical; only the mock's memory layout
/// differs.
#[derive(Debug, Clone)]
pub struct DistributedVector<B: Backend> {
    parts: Vec<SplitComplex<B>>,
    n_edges: usize,
}

impl<B: Backend> DistributedVector<B> {
    /// A zero distributed vector: `k` canonical parts of length `n_edges`.
    pub fn zeros(k: usize, n_edges: usize, device: &B::Device) -> Self {
        Self {
            parts: (0..k)
                .map(|_| SplitComplex::<B>::zeros(n_edges, device))
                .collect(),
            n_edges,
        }
    }

    /// Distribute a global host vector into canonical per-part form: part `p`
    /// keeps `global[d]` on the DOFs it owns and zero elsewhere.
    ///
    /// # Panics
    ///
    /// Panics if `global.len() != partition.n_dofs()`.
    pub fn from_global(global: &[c64], partition: &Partition, device: &B::Device) -> Self {
        let n_edges = partition.n_dofs();
        assert_eq!(
            global.len(),
            n_edges,
            "global vector length {} != partition n_dofs {}",
            global.len(),
            n_edges
        );
        let k = partition.k();
        let mut parts = Vec::with_capacity(k);
        for p in 0..k {
            let mut host = vec![c64::new(0.0, 0.0); n_edges];
            let pd = partition.part(p);
            for &d in pd.interior.iter().chain(pd.interface.iter()) {
                host[d as usize] = global[d as usize];
            }
            parts.push(SplitComplex::<B>::upload(&host, device));
        }
        Self { parts, n_edges }
    }

    /// Number of parts.
    pub fn n_parts(&self) -> usize {
        self.parts.len()
    }

    /// Operator dimension (global edge-DOF count).
    pub fn n_edges(&self) -> usize {
        self.n_edges
    }

    /// Immutable view of a part's buffer.
    pub fn part(&self, p: usize) -> &SplitComplex<B> {
        &self.parts[p]
    }

    /// Reassemble the global vector by summing the (disjoint) parts. Valid for a
    /// canonical vector; the sum reconstructs the single-process vector exactly.
    pub fn gather_global(&self) -> SplitComplex<B> {
        let mut re = self.parts[0].re.clone();
        let mut im = self.parts[0].im.clone();
        for p in self.parts.iter().skip(1) {
            re = re.add(p.re.clone());
            im = im.add(p.im.clone());
        }
        SplitComplex { re, im }
    }

    /// `self += s · rhs` (complex scalar `s`), per part. Preserves canonical
    /// form when both operands are canonical.
    pub fn axpy(&mut self, s: c64, rhs: &Self) {
        for (a, b) in self.parts.iter_mut().zip(rhs.parts.iter()) {
            a.axpy(s, b);
        }
    }

    /// `self = other + β · self` (the COCG `p ← z + β p` recurrence), per part.
    pub fn scale_add(&mut self, other: &Self, beta: c64) {
        for (a, b) in self.parts.iter_mut().zip(other.parts.iter()) {
            a.scale_add(b, beta);
        }
    }

    /// `self *= s` (real scalar), per part — the Lanczos normalization
    /// `v ← w / β`.
    pub fn scale_real(&mut self, s: f64) {
        for a in self.parts.iter_mut() {
            a.re = a.re.clone().mul_scalar(s);
            a.im = a.im.clone().mul_scalar(s);
        }
    }
}

// ---------------------------------------------------------------------------
// Collective transport trait + single-process mock
// ---------------------------------------------------------------------------

/// The distributed-solve transport: **halo exchange** (two directions) and
/// **all-reduce**. This is the narrow FFI seam of the abstraction (see the
/// module docs' IR-vs-FFI decision) — everything above it is backend-generic
/// tensor code that never names a transport.
///
/// A real Phase-C implementation backs these with `ncclSend`/`ncclRecv` (halo)
/// and `ncclAllReduce` (reduction); [`LocalMock`] backs them with an in-memory
/// copy so the whole loop runs and is tested on one machine.
pub trait Collective<B: Backend> {
    /// Number of parts (ranks) this transport serves.
    fn n_parts(&self) -> usize;

    /// **Forward** halo exchange: fill every part's ghost-DOF slots with the
    /// authoritative values from the DOFs' owners, per the partition's
    /// `halo_recv` lists. After this call a part's buffer holds correct values
    /// on `owned ∪ ghost` — exactly the DOFs its element-local apply reads.
    ///
    /// This is the point-to-point *receive* of a neighbour's boundary values;
    /// it is the FFI boundary in Phase C (mock: in-memory copy).
    fn halo_gather(&self, v: &mut DistributedVector<B>);

    /// **Reverse** halo exchange: each part ships the partial results it
    /// computed on its ghost DOFs back to those DOFs' owners, which sum them;
    /// the sender then zeroes its ghost slots (restoring canonical form). This
    /// is the transpose of [`halo_gather`](Self::halo_gather) and completes the
    /// distributed scatter-add.
    fn halo_accumulate(&self, v: &mut DistributedVector<B>);

    /// All-reduce (sum) of a per-part partial scalar across all parts. `partials`
    /// has length [`n_parts`](Self::n_parts); the returned scalar is the global
    /// sum every part would see. This is the cross-rank combine of the split
    /// reduction (`ncclAllReduce` in Phase C).
    fn all_reduce_sum(&self, partials: &[c64]) -> c64;
}

/// One directional halo channel resolved to plain index lists (owner-agnostic
/// copy of a [`HaloChannel`](crate::mesh::partition::HaloChannel)).
#[derive(Debug, Clone)]
struct MockChannel {
    peer: usize,
    dofs: Vec<u32>,
}

/// Single-process [`Collective`] implementation. Runs all parts in one address
/// space and moves halo / reduction data by an **in-memory host copy** — the
/// software stand-in for the Phase-C NCCL/NVSHMEM transfers. No hardware, no
/// FFI.
///
/// Built from a #634 [`Partition`]; it captures each part's `halo_recv`
/// channels (the `(peer, dofs)` lists) once at construction, exactly the static
/// metadata a real transport would use to post its sends/receives.
#[derive(Debug, Clone)]
pub struct LocalMock {
    /// Per-part receive channels (peer = owner the DOFs are pulled from).
    recv: Vec<Vec<MockChannel>>,
    n_parts: usize,
    n_edges: usize,
}

impl LocalMock {
    /// Capture the halo maps of a [`Partition`] into a single-process transport.
    pub fn new(partition: &Partition) -> Self {
        let k = partition.k();
        let recv = (0..k)
            .map(|p| {
                partition
                    .part(p)
                    .halo_recv
                    .iter()
                    .map(|ch| MockChannel {
                        peer: ch.peer,
                        dofs: ch.dofs.clone(),
                    })
                    .collect()
            })
            .collect();
        Self {
            recv,
            n_parts: k,
            n_edges: partition.n_dofs(),
        }
    }
}

impl<B: Backend> Collective<B> for LocalMock {
    fn n_parts(&self) -> usize {
        self.n_parts
    }

    fn halo_gather(&self, v: &mut DistributedVector<B>) {
        assert_eq!(v.n_parts(), self.n_parts, "vector/transport part mismatch");
        assert_eq!(
            v.n_edges(),
            self.n_edges,
            "vector/transport length mismatch"
        );
        // Snapshot every part to host. The owner values we read (owned DOFs) are
        // disjoint from the ghost slots we write, so a snapshot is not strictly
        // required, but it keeps the copy order-independent and obviously
        // correct — the point of a mock.
        let snap: Vec<Vec<c64>> = v.parts.iter().map(|s| s.download()).collect();
        let mut out = snap.clone();
        for (p, recv_p) in self.recv.iter().enumerate() {
            for ch in recv_p {
                for &d in &ch.dofs {
                    // Pull the owner's authoritative value into this part's
                    // ghost slot.
                    out[p][d as usize] = snap[ch.peer][d as usize];
                }
            }
        }
        v.parts = out
            .iter()
            .map(|h| SplitComplex::<B>::upload(h, &device_of(&v.parts[0])))
            .collect();
    }

    fn halo_accumulate(&self, v: &mut DistributedVector<B>) {
        assert_eq!(v.n_parts(), self.n_parts, "vector/transport part mismatch");
        assert_eq!(
            v.n_edges(),
            self.n_edges,
            "vector/transport length mismatch"
        );
        let snap: Vec<Vec<c64>> = v.parts.iter().map(|s| s.download()).collect();
        let mut out = snap.clone();
        for (p, recv_p) in self.recv.iter().enumerate() {
            for ch in recv_p {
                let owner = ch.peer;
                for &d in &ch.dofs {
                    let di = d as usize;
                    // Ship this part's partial for the ghost DOF back to its
                    // owner, which sums it; then zero the sender's ghost slot.
                    out[owner][di] += snap[p][di];
                    out[p][di] = c64::new(0.0, 0.0);
                }
            }
        }
        v.parts = out
            .iter()
            .map(|h| SplitComplex::<B>::upload(h, &device_of(&v.parts[0])))
            .collect();
    }

    fn all_reduce_sum(&self, partials: &[c64]) -> c64 {
        partials.iter().fold(c64::new(0.0, 0.0), |a, &b| a + b)
    }
}

/// The device a split-complex pair lives on.
fn device_of<B: Backend>(s: &SplitComplex<B>) -> B::Device {
    s.re.device()
}

// ---------------------------------------------------------------------------
// Distributed inner products (local partial sum + all-reduce)
// ---------------------------------------------------------------------------

/// The **bilinear** (unconjugated) inner product `uᵀv` of two *canonical*
/// distributed vectors: a per-part [`SplitComplex::bilinear_dot`]
/// partial sum, combined by [`Collective::all_reduce_sum`]. Because canonical
/// vectors are nonzero only on owned DOFs and ownership is disjoint, the
/// all-reduce sums each DOF's contribution exactly once — no masks, no
/// double-counting.
pub fn distributed_dot<B: Backend>(
    coll: &dyn Collective<B>,
    u: &DistributedVector<B>,
    v: &DistributedVector<B>,
) -> c64 {
    let partials: Vec<c64> = (0..u.n_parts())
        .map(|p| u.part(p).bilinear_dot(v.part(p)))
        .collect();
    coll.all_reduce_sum(&partials)
}

/// The standard **Hermitian** Euclidean norm `√(Σ|v|²)` of a canonical
/// distributed vector — the residual measure for the stopping criterion,
/// matching what the single-process COCG paths report. Each part contributes its
/// partial sum-of-squares; the all-reduce combines them before the host `sqrt`.
pub fn distributed_norm<B: Backend>(coll: &dyn Collective<B>, v: &DistributedVector<B>) -> f64 {
    let partials: Vec<c64> = (0..v.n_parts())
        .map(|p| {
            let n = v.part(p).euclid_norm();
            c64::new(n * n, 0.0)
        })
        .collect();
    coll.all_reduce_sum(&partials).re.max(0.0).sqrt()
}

// ---------------------------------------------------------------------------
// DistributedOperator
// ---------------------------------------------------------------------------

/// The partitioned matrix-free Nédélec operator: one
/// [`MatrixFreeNedelecOperator`] per part over that part's owned elements (in
/// the global edge numbering), completing each apply with a halo exchange keyed
/// to the #634 send/receive lists.
///
/// Exposes the same three applies as the single-process operator — [`apply_k`],
/// [`apply_m`], [`apply_combination`] — each producing a canonical
/// [`DistributedVector`]. Summing that result ([`DistributedVector::gather_global`])
/// reproduces the single-process matvec to round-off.
///
/// [`apply_k`]: Self::apply_k
/// [`apply_m`]: Self::apply_m
/// [`apply_combination`]: Self::apply_combination
#[derive(Debug, Clone)]
pub struct DistributedOperator<B: Backend> {
    /// Per-part sub-operator over the part's owned elements.
    sub: Vec<MatrixFreeNedelecOperator<B>>,
    n_edges: usize,
    /// Stored `(α, β)` for the [`DistributedLinearOperator`] apply used by the
    /// Krylov loop; `apply_combination` takes explicit weights independently.
    combination: (f64, f64),
}

impl<B: Backend> DistributedOperator<B> {
    /// Build the partitioned operator from a mesh, its #634 [`Partition`], the
    /// per-element permittivity, and an optional PEC interior mask.
    ///
    /// Each part's sub-operator is a [`MatrixFreeNedelecOperator`] over the
    /// subset of tets the part owns, sharing the **global** node coordinates and
    /// edge numbering (so gather/scatter indices, and hence the halo maps, line
    /// up). The interior mask, if present, is attached to every sub-operator; it
    /// is a per-DOF projection that commutes with the element sum, so the
    /// distributed apply reproduces the masked interior operator.
    ///
    /// The default [`DistributedLinearOperator`] combination is `K` alone
    /// (`α = 1, β = 0`); set another with [`with_combination`](Self::with_combination).
    ///
    /// # Panics
    ///
    /// Panics if `epsilon_r.len() != mesh.n_tets()`, if `mesh.edges().len() !=
    /// partition.n_dofs()`, or if an interior mask of the wrong length is given.
    pub fn from_mesh(
        mesh: &TetMesh,
        partition: &Partition,
        epsilon_r: &[f64],
        interior_mask: Option<&[bool]>,
        device: &B::Device,
    ) -> Self {
        let n_edges = mesh.edges().len();
        assert_eq!(
            n_edges,
            partition.n_dofs(),
            "mesh edge count {} != partition n_dofs {}",
            n_edges,
            partition.n_dofs()
        );
        assert_eq!(
            epsilon_r.len(),
            mesh.n_tets(),
            "epsilon_r length {} != n_tets {}",
            epsilon_r.len(),
            mesh.n_tets()
        );
        if let Some(m) = interior_mask {
            assert_eq!(m.len(), n_edges, "interior_mask length must equal n_edges");
        }

        let te = mesh.tet_edges();
        let full_idx: Vec<[u32; 6]> = te
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].0))
            .collect();
        let full_sign: Vec<[i8; 6]> = te
            .iter()
            .map(|row| std::array::from_fn(|i| row[i].1))
            .collect();

        // Shared global node coordinates (the sub-operators index into these
        // through their own subset connectivity).
        let (nodes, _tets_full) = upload_mesh::<B>(mesh, device);

        let k = partition.k();
        let mut sub = Vec::with_capacity(k);
        for p in 0..k {
            let elems = &partition.part(p).elements;
            // Subset connectivity + edge tables + permittivity for this part.
            let tets_sub = upload_tets_subset::<B>(mesh, elems, device);
            let idx_sub: Vec<[u32; 6]> = elems.iter().map(|&e| full_idx[e as usize]).collect();
            let sign_sub: Vec<[i8; 6]> = elems.iter().map(|&e| full_sign[e as usize]).collect();
            let eps_sub: Vec<f64> = elems.iter().map(|&e| epsilon_r[e as usize]).collect();

            let mut op = MatrixFreeNedelecOperator::<B>::new(
                nodes.clone(),
                tets_sub,
                &idx_sub,
                &sign_sub,
                n_edges,
                &eps_sub,
            );
            if let Some(m) = interior_mask {
                op = op.with_mask(m);
            }
            sub.push(op);
        }

        Self {
            sub,
            n_edges,
            combination: (1.0, 0.0),
        }
    }

    /// Set the `(α, β)` weights of the `A = α K + β M` combination the
    /// [`DistributedLinearOperator`] apply (the one the Krylov loop drives) uses.
    #[must_use]
    pub fn with_combination(mut self, alpha: f64, beta: f64) -> Self {
        self.combination = (alpha, beta);
        self
    }

    /// Number of global edge DOFs (operator dimension).
    pub fn n_edges(&self) -> usize {
        self.n_edges
    }

    /// Number of parts.
    pub fn n_parts(&self) -> usize {
        self.sub.len()
    }

    /// Distributed `y = K · x` (curl-curl stiffness).
    pub fn apply_k(
        &self,
        x: &DistributedVector<B>,
        coll: &dyn Collective<B>,
    ) -> DistributedVector<B> {
        self.apply_with(x, coll, |op, half| op.apply_k(half))
    }

    /// Distributed `y = M · x` (ε-weighted mass).
    pub fn apply_m(
        &self,
        x: &DistributedVector<B>,
        coll: &dyn Collective<B>,
    ) -> DistributedVector<B> {
        self.apply_with(x, coll, |op, half| op.apply_m(half))
    }

    /// Distributed `y = (α K + β M) · x` in a single gather/scatter pass per
    /// part (the driven-style combination; `A = K − ω²M` is `α = 1, β = −ω²`).
    pub fn apply_combination(
        &self,
        x: &DistributedVector<B>,
        alpha: f64,
        beta: f64,
        coll: &dyn Collective<B>,
    ) -> DistributedVector<B> {
        self.apply_with(x, coll, |op, half| op.apply_combination(half, alpha, beta))
    }

    /// Shared partitioned-apply skeleton: forward halo-gather the operand, run
    /// each part's local apply on both split-complex halves, then reverse
    /// halo-accumulate the partial results into their owners.
    fn apply_with<F>(
        &self,
        x: &DistributedVector<B>,
        coll: &dyn Collective<B>,
        local: F,
    ) -> DistributedVector<B>
    where
        F: Fn(&MatrixFreeNedelecOperator<B>, Tensor<B, 1>) -> Tensor<B, 1>,
    {
        assert_eq!(
            x.n_parts(),
            self.sub.len(),
            "operand/operator part mismatch"
        );
        assert_eq!(
            x.n_edges(),
            self.n_edges,
            "operand length != operator n_edges"
        );

        // 1. Forward exchange: fill each part's ghost slots with authoritative
        //    input values (owned ∪ ghost now correct on every part).
        let mut xg = x.clone();
        coll.halo_gather(&mut xg);

        // 2. Per-part local apply on the real and imaginary halves.
        let parts: Vec<SplitComplex<B>> = (0..self.sub.len())
            .map(|p| {
                let re = local(&self.sub[p], xg.part(p).re.clone());
                let im = local(&self.sub[p], xg.part(p).im.clone());
                SplitComplex { re, im }
            })
            .collect();
        let mut y = DistributedVector {
            parts,
            n_edges: self.n_edges,
        };

        // 3. Reverse exchange: ship each part's ghost-DOF partials to their
        //    owners and sum, restoring canonical form.
        coll.halo_accumulate(&mut y);
        y
    }
}

/// Upload the subset of a mesh's tet connectivity named by `elems` as a
/// `[m, 4]` Int tensor (global node indices preserved).
fn upload_tets_subset<B: Backend>(
    mesh: &TetMesh,
    elems: &[u32],
    device: &B::Device,
) -> Tensor<B, 2, Int> {
    let m = elems.len();
    let data: Vec<i32> = elems
        .iter()
        .flat_map(|&e| {
            mesh.tets[e as usize]
                .iter()
                .map(|&v| i32::try_from(v).expect("node index does not fit in i32"))
        })
        .collect();
    Tensor::<B, 2, Int>::from_data(TensorData::new(data, [m, 4]), device)
}

// ---------------------------------------------------------------------------
// DistributedLinearOperator seam + DistributedCocg
// ---------------------------------------------------------------------------

/// The linear-operator seam the distributed Krylov loop drives: a single
/// `y = A · x` apply over the distributed abstraction. [`DistributedOperator`]
/// implements it using its stored `(α, β)` combination
/// ([`with_combination`](DistributedOperator::with_combination)); a future
/// surface-corrected or complex-pencil operator can implement the same trait and
/// [`DistributedCocg`] drives it unchanged (mirroring the single-process
/// [`MatrixFreeComplexOperator`](crate::solver::ksp_burn) seam).
pub trait DistributedLinearOperator<B: Backend> {
    /// Operator dimension (global edge-DOF count).
    fn n_edges(&self) -> usize;
    /// Apply `y = A · x` over the distributed abstraction, using `coll` for the
    /// halo exchange its partitioned apply needs.
    fn apply(&self, x: &DistributedVector<B>, coll: &dyn Collective<B>) -> DistributedVector<B>;
}

impl<B: Backend> DistributedLinearOperator<B> for DistributedOperator<B> {
    fn n_edges(&self) -> usize {
        self.n_edges
    }
    fn apply(&self, x: &DistributedVector<B>, coll: &dyn Collective<B>) -> DistributedVector<B> {
        let (alpha, beta) = self.combination;
        self.apply_combination(x, alpha, beta, coll)
    }
}

/// **Distributed complex-symmetric COCG** — the generalization of
/// [`Cocg`](crate::solver::ksp::Cocg) / [`BurnCocg`](crate::solver::ksp_burn::BurnCocg)
/// over the distributed abstraction.
///
/// The recurrence, breakdown checks, relative-residual stopping criterion, and
/// final true-residual recompute are bit-for-bit the single-process algorithm;
/// only two operations are lifted to the distributed layer:
///
/// - every inner product (`ρ = rᵀz`, `pᵀq`) is a [`distributed_dot`]
///   (local partial sum + all-reduce), and
/// - the operator apply `q = A p` is the partitioned apply of a
///   [`DistributedLinearOperator`] (per-part local apply + halo exchange).
///
/// This is the **unpreconditioned** (`M = I`) COCG; a distributed Jacobi
/// preconditioner (an owner-accumulated diagonal + a local reciprocal-multiply)
/// slots in at the `z = M⁻¹ r` line and is deferred to a follow-on. On a real
/// pencil the bilinear `rᵀz` reduces to the real dot, so this degenerates to
/// distributed CG — the same COCG↔CG equivalence the single-process
/// `cocg_recovers_cg_on_real_spd_system` test gates.
#[derive(Debug, Clone, Copy)]
pub struct DistributedCocg {
    /// Relative-residual stopping criterion `‖r‖₂ ≤ tol·‖b‖₂`.
    pub tol: f64,
    /// Iteration budget; [`KspError::NotConverged`] if exhausted.
    pub max_iters: usize,
    /// Magnitude below which a bilinear inner product is treated as a breakdown.
    pub breakdown_tol: f64,
}

impl Default for DistributedCocg {
    fn default() -> Self {
        Self {
            tol: 1e-10,
            max_iters: 2000,
            breakdown_tol: 1e-300,
        }
    }
}

impl DistributedCocg {
    /// Convenience constructor — `tol` and `max_iters`, default breakdown
    /// threshold.
    pub fn new(tol: f64, max_iters: usize) -> Self {
        Self {
            tol,
            max_iters,
            ..Default::default()
        }
    }

    /// Solve `A x = b` over the distributed abstraction, returning the canonical
    /// solution vector and a [`KspReport`]. `b` must be canonical
    /// ([`DistributedVector::from_global`]).
    ///
    /// # Errors
    ///
    /// [`KspError::DimMismatch`] on a shape mismatch, [`KspError::ZeroRhs`] if
    /// `b ≡ 0`, [`KspError::Breakdown`] on a vanishing bilinear inner product,
    /// and [`KspError::NotConverged`] when the iteration budget is exhausted.
    pub fn solve<Op: DistributedLinearOperator<B>, B: Backend>(
        &self,
        op: &Op,
        b: &DistributedVector<B>,
        coll: &dyn Collective<B>,
    ) -> Result<(DistributedVector<B>, KspReport), KspError> {
        let n = op.n_edges();
        if b.n_edges() != n {
            return Err(KspError::DimMismatch {
                n,
                what: "b",
                got: b.n_edges(),
            });
        }

        let b_norm = distributed_norm(coll, b);
        if b_norm == 0.0 {
            return Err(KspError::ZeroRhs);
        }
        let target = self.tol * b_norm;

        // x_0 = 0 ⇒ r_0 = b.
        let mut x = DistributedVector::<B>::zeros(b.n_parts(), n, &device_of(b.part(0)));
        let mut r = b.clone();

        // z = M⁻¹ r = r (identity), p = z, ρ = rᵀz.
        let mut p = r.clone();
        let mut rho = distributed_dot(coll, &r, &r);
        bd_check(rho, 0, "r^T z", self.breakdown_tol)?;

        for k in 0..self.max_iters {
            // q = A p.
            let q = op.apply(&p, coll);

            // α = ρ / (pᵀq).
            let pq = distributed_dot(coll, &p, &q);
            bd_check(pq, k, "p^T A p", self.breakdown_tol)?;
            let alpha = rho / pq;

            // x += α p; r −= α q.
            x.axpy(alpha, &p);
            r.axpy(-alpha, &q);

            // Tolerance check on the recursively maintained residual.
            let r_norm = distributed_norm(coll, &r);
            if r_norm <= target {
                let residual_rel = true_residual_rel(op, coll, &x, b, b_norm);
                return Ok((
                    x,
                    KspReport {
                        iters: k + 1,
                        residual_rel,
                        converged: residual_rel <= self.tol,
                    },
                ));
            }

            // z = M⁻¹ r = r; ρ_new = rᵀz; β = ρ_new/ρ.
            let rho_new = distributed_dot(coll, &r, &r);
            bd_check(rho_new, k + 1, "r^T z", self.breakdown_tol)?;
            let beta = rho_new / rho;
            rho = rho_new;

            // p = r + β p.
            p.scale_add(&r, beta);
        }

        let residual_rel = true_residual_rel(op, coll, &x, b, b_norm);
        Err(KspError::NotConverged {
            iter: self.max_iters,
            residual_rel,
            tol: self.tol,
        })
    }
}

/// Recompute `‖A x − b‖₂ / ‖b‖₂` with an explicit distributed matvec (the
/// reliable figure; the recursively maintained residual can drift).
fn true_residual_rel<Op: DistributedLinearOperator<B>, B: Backend>(
    op: &Op,
    coll: &dyn Collective<B>,
    x: &DistributedVector<B>,
    b: &DistributedVector<B>,
    b_norm: f64,
) -> f64 {
    let mut resid = op.apply(x, coll);
    resid.axpy(c64::new(-1.0, 0.0), b);
    distributed_norm(coll, &resid) / b_norm
}

/// Breakdown guard on a bilinear inner-product scalar — mirrors the
/// single-process `bd_check`.
fn bd_check(val: c64, iter: usize, kind: &'static str, tol: f64) -> Result<(), KspError> {
    let mag2 = val.re * val.re + val.im * val.im;
    if !val.re.is_finite() || !val.im.is_finite() || mag2 < tol * tol {
        Err(KspError::Breakdown {
            iter,
            kind,
            value_re: val.re,
            value_im: val.im,
        })
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;
    use crate::testing::TestBackend;
    use burn::tensor::backend::BackendTypes;

    type Bk = TestBackend;

    fn dev() -> <Bk as BackendTypes>::Device {
        <Bk as BackendTypes>::Device::default()
    }

    /// Deterministic non-degenerate global seed vector of length `n`.
    fn seed_global(n: usize) -> Vec<c64> {
        (0..n)
            .map(|i| {
                let x = i as f64;
                c64::new((0.7 * x + 0.3).sin() + 1.1, 0.0)
            })
            .collect()
    }

    /// A canonical distributed vector reassembles to the global vector it was
    /// built from, and its distributed norm matches the global norm — for every
    /// P (the disjoint-cover invariant the dots rely on).
    #[test]
    fn canonical_roundtrip_and_norm() {
        let mesh = cube_tet_mesh(3, 1.0);
        let n = mesh.edges().len();
        let g = seed_global(n);
        let gnorm = g.iter().map(|z| z.norm_sqr()).sum::<f64>().sqrt();
        for k in [1usize, 2, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            let coll = LocalMock::new(&part);
            let dv = DistributedVector::<Bk>::from_global(&g, &part, &dev());
            // gather_global == g
            let back = dv.gather_global().download();
            for (a, b) in back.iter().zip(g.iter()) {
                assert!((a - b).norm() < 1e-12, "k={k}: {a:?} != {b:?}");
            }
            // distributed norm == global norm
            let dn = distributed_norm(&coll, &dv);
            assert!((dn - gnorm).abs() < 1e-9 * gnorm.max(1.0), "k={k}");
        }
    }

    /// **Forward halo gather** fills each part's ghost slots with the *owner's*
    /// authoritative value: after a gather of a canonical vector, part `p`'s
    /// entry for a ghost DOF `d` (owner `q`) equals the global value `g[d]`.
    #[test]
    fn halo_gather_fills_ghosts_with_owner_values() {
        let mesh = cube_tet_mesh(4, 1.0);
        let n = mesh.edges().len();
        let g = seed_global(n);
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            let coll = LocalMock::new(&part);
            let mut dv = DistributedVector::<Bk>::from_global(&g, &part, &dev());
            <LocalMock as Collective<Bk>>::halo_gather(&coll, &mut dv);
            for p in 0..k {
                let host = dv.part(p).download();
                for ch in &part.part(p).halo_recv {
                    for &d in &ch.dofs {
                        assert!(
                            (host[d as usize] - g[d as usize]).norm() < 1e-12,
                            "k={k} part {p}: ghost DOF {d} not filled with owner value"
                        );
                    }
                }
            }
        }
    }

    /// **Reverse halo accumulate** sums each part's ghost-DOF partials into the
    /// owner and zeroes the senders. Seed every part with `1.0` on every DOF its
    /// elements touch (owned ∪ ghost); after accumulate the owner of an interface
    /// DOF holds exactly the count of parts that touch it, and every part is
    /// canonical (zero off its owned set). This is the distributed scatter-add
    /// the matvec relies on, checked in isolation.
    #[test]
    fn halo_accumulate_sums_partials_into_owner() {
        let mesh = cube_tet_mesh(4, 1.0);
        let n = mesh.edges().len();
        let te = mesh.tet_edges();
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            let coll = LocalMock::new(&part);
            // Expected owner value = number of DISTINCT parts whose elements
            // touch each DOF (= 1 owner + its ghost-holders).
            let mut touching: Vec<std::collections::BTreeSet<usize>> =
                vec![std::collections::BTreeSet::new(); n];
            let mut parts_vec: Vec<SplitComplex<Bk>> = Vec::with_capacity(k);
            for p in 0..k {
                let mut host = vec![c64::new(0.0, 0.0); n];
                for &e in &part.part(p).elements {
                    for &(d, _s) in &te[e as usize] {
                        host[d as usize] = c64::new(1.0, 0.0);
                        touching[d as usize].insert(p);
                    }
                }
                parts_vec.push(SplitComplex::<Bk>::upload(&host, &dev()));
            }
            let mut dv = DistributedVector {
                parts: parts_vec,
                n_edges: n,
            };
            <LocalMock as Collective<Bk>>::halo_accumulate(&coll, &mut dv);
            for p in 0..k {
                let host = dv.part(p).download();
                for d in 0..n {
                    if part.owner_of(d) == p {
                        let expect = touching[d].len() as f64;
                        assert!(
                            (host[d].re - expect).abs() < 1e-12 && host[d].im.abs() < 1e-12,
                            "k={k} owner {p} DOF {d}: got {} want {expect}",
                            host[d].re
                        );
                    } else {
                        assert!(
                            host[d].norm() < 1e-12,
                            "k={k} part {p} DOF {d}: non-owned slot not zeroed"
                        );
                    }
                }
            }
        }
    }

    /// **The load-bearing correctness gate.** The distributed apply
    /// (`K`, `M`, and a `K − c M` combination) reproduces the single-process
    /// [`MatrixFreeNedelecOperator`] matvec to round-off, for P ∈ {1,2,4,8}.
    /// Distribution changes where work runs, not the answer.
    #[test]
    fn distributed_apply_matches_single_process() {
        let mesh = cube_tet_mesh(4, 1.0);
        let n = mesh.edges().len();
        let eps = vec![1.0_f64; mesh.n_tets()];
        let g = seed_global(n);

        // Single-process reference operator over all elements.
        let te = mesh.tet_edges();
        let idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
        let sign: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
        let (nodes, tets) = upload_mesh::<Bk>(&mesh, &dev());
        let single = MatrixFreeNedelecOperator::<Bk>::new(nodes, tets, &idx, &sign, n, &eps);
        let xg: Tensor<Bk, 1> = Tensor::from_data(
            TensorData::new(g.iter().map(|z| z.re).collect::<Vec<_>>(), [n]),
            &dev(),
        );
        let ref_k: Vec<f64> = single
            .apply_k(xg.clone())
            .into_data()
            .iter::<f64>()
            .collect();
        let ref_m: Vec<f64> = single
            .apply_m(xg.clone())
            .into_data()
            .iter::<f64>()
            .collect();
        let ref_c: Vec<f64> = single
            .apply_combination(xg, 1.0, -0.5)
            .into_data()
            .iter::<f64>()
            .collect();

        let scale = ref_k.iter().fold(0.0_f64, |a, &b| a.max(b.abs())).max(1.0);
        for k in [1usize, 2, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            let coll = LocalMock::new(&part);
            let op = DistributedOperator::<Bk>::from_mesh(&mesh, &part, &eps, None, &dev());
            let x = DistributedVector::<Bk>::from_global(&g, &part, &dev());

            let got_k = op.apply_k(&x, &coll).gather_global().download();
            let got_m = op.apply_m(&x, &coll).gather_global().download();
            let got_c = op
                .apply_combination(&x, 1.0, -0.5, &coll)
                .gather_global()
                .download();
            for i in 0..n {
                assert!(
                    (got_k[i].re - ref_k[i]).abs() < 1e-9 * scale,
                    "k={k} K dof {i}"
                );
                assert!(
                    (got_m[i].re - ref_m[i]).abs() < 1e-9 * scale,
                    "k={k} M dof {i}"
                );
                assert!(
                    (got_c[i].re - ref_c[i]).abs() < 1e-9 * scale,
                    "k={k} combo dof {i}"
                );
                // Real input ⇒ imaginary half stays zero.
                assert!(got_k[i].im.abs() < 1e-9 * scale, "k={k} K imag leak");
            }
        }
    }

    /// The distributed COCG solve of an SPD system `A = K + c M` is independent
    /// of the number of parts: P ∈ {2,4,8} reproduce the P = 1 (single-process)
    /// solution and iteration count. The mass-dominant shift keeps `A`
    /// well-conditioned so the loop converges in budget.
    #[test]
    fn distributed_cocg_matches_single_process() {
        let mesh = cube_tet_mesh(3, 1.0);
        let n = mesh.edges().len();
        let eps = vec![1.0_f64; mesh.n_tets()];
        let b_global = seed_global(n);

        let solve_for = |k: usize| {
            let part = Partition::from_tet_mesh(&mesh, k);
            let coll = LocalMock::new(&part);
            let op = DistributedOperator::<Bk>::from_mesh(&mesh, &part, &eps, None, &dev())
                .with_combination(1.0, 50.0); // A = K + 50 M (SPD, well conditioned)
            let b = DistributedVector::<Bk>::from_global(&b_global, &part, &dev());
            let (x, report) = DistributedCocg::new(1e-10, 5000)
                .solve(&op, &b, &coll)
                .expect("distributed COCG converges");
            (x.gather_global().download(), report)
        };

        let (ref_x, ref_report) = solve_for(1);
        assert!(ref_report.converged, "P=1 must converge: {ref_report:?}");
        let scale = ref_x.iter().fold(0.0_f64, |a, z| a.max(z.norm())).max(1.0);
        for k in [2usize, 4, 8] {
            let (x, report) = solve_for(k);
            assert!(report.converged, "P={k}: {report:?}");
            // The iteration count may differ by a step or two: the distributed
            // all-reduce sums the residual in a different floating-point order,
            // so the norm can cross the tolerance one iteration earlier or later.
            // The converged solution is the invariant that must match.
            assert!(
                (report.iters as i64 - ref_report.iters as i64).abs() <= 3,
                "P={k} iteration count {} drifted far from reference {}",
                report.iters,
                ref_report.iters
            );
            for i in 0..n {
                assert!(
                    (x[i] - ref_x[i]).norm() < 1e-7 * scale,
                    "P={k} solution dof {i} differs"
                );
            }
        }
    }

    /// `m`-step Lanczos on an SPD distributed operator, with full
    /// reorthogonalization, returning the ascending Ritz eigenvalues. Uses only
    /// distributed matvecs + distributed dots, so it runs identically at any P.
    fn lanczos_ritz<Op: DistributedLinearOperator<Bk>>(
        op: &Op,
        coll: &dyn Collective<Bk>,
        seed: &DistributedVector<Bk>,
        m: usize,
    ) -> Vec<f64> {
        use faer::{Mat, Side};
        let mut basis: Vec<DistributedVector<Bk>> = Vec::with_capacity(m);
        let mut alpha: Vec<f64> = Vec::with_capacity(m);
        let mut beta: Vec<f64> = Vec::with_capacity(m);

        let mut v = seed.clone();
        let s = distributed_norm(coll, &v);
        v.scale_real(1.0 / s);

        for _ in 0..m {
            let mut w = op.apply(&v, coll);
            let a = distributed_dot(coll, &v, &w).re;
            // Full reorthogonalization against the stored basis (and current v).
            w.axpy(c64::new(-a, 0.0), &v);
            for bvec in &basis {
                let c = distributed_dot(coll, bvec, &w).re;
                w.axpy(c64::new(-c, 0.0), bvec);
            }
            basis.push(v.clone());
            alpha.push(a);
            let bnorm = distributed_norm(coll, &w);
            if bnorm < 1e-12 {
                break;
            }
            if alpha.len() < m {
                beta.push(bnorm);
            }
            w.scale_real(1.0 / bnorm);
            v = w;
        }

        let kdim = alpha.len();
        let t = Mat::<f64>::from_fn(kdim, kdim, |i, j| {
            if i == j {
                alpha[i]
            } else if i + 1 == j {
                beta[i]
            } else if j + 1 == i {
                beta[j]
            } else {
                0.0
            }
        });
        t.as_ref()
            .self_adjoint_eigenvalues(Side::Lower)
            .expect("tridiagonal eigenvalues")
    }

    /// **Spectrum-match on a cube fixture.** The Lanczos Ritz spectrum of the
    /// matrix-free `A = K + M` operator is identical (to tolerance) whether
    /// computed single-process (P = 1) or through the P-part distributed mock,
    /// for P ∈ {1,2,4,8}. The fast, always-run analog of the transmon gate.
    #[test]
    fn distributed_spectrum_matches_single_process() {
        let mesh = cube_tet_mesh(3, 1.0);
        let n = mesh.edges().len();
        let eps = vec![1.0_f64; mesh.n_tets()];
        let seed = seed_global(n);
        let m = 24;

        let ritz_for = |k: usize| {
            let part = Partition::from_tet_mesh(&mesh, k);
            let coll = LocalMock::new(&part);
            let op = DistributedOperator::<Bk>::from_mesh(&mesh, &part, &eps, None, &dev())
                .with_combination(1.0, 1.0); // A = K + M (SPD)
            let sd = DistributedVector::<Bk>::from_global(&seed, &part, &dev());
            lanczos_ritz(&op, &coll, &sd, m)
        };

        let reference = ritz_for(1);
        let n_cmp = reference.len().min(8);
        let scale = reference.last().copied().unwrap_or(1.0).abs().max(1.0);
        for k in [2usize, 4, 8] {
            let spec = ritz_for(k);
            assert_eq!(spec.len(), reference.len(), "P={k} Krylov dim drift");
            for i in 0..n_cmp {
                assert!(
                    (spec[i] - reference[i]).abs() < 1e-6 * scale,
                    "P={k} Ritz[{i}] = {} != {}",
                    spec[i],
                    reference[i]
                );
            }
        }
    }

    /// **The transmon spectrum-match gate (release-tier).** On the real
    /// DeviceLayout transmon smoke fixture with PEC interior masking, the
    /// Lanczos Ritz spectrum of the matrix-free `A = K + M` pencil is identical
    /// across P ∈ {1,2,4,8} — the distributed mock reproduces the single-process
    /// transmon spectrum exactly. `#[ignore]` (release-tier) because it edge-
    /// numbers and partitions the full 133k-tet fixture and builds P
    /// sub-operators; run with `--ignored --nocapture`.
    ///
    /// Scope note: the volumetric tet-only matrix-free operator does not carry
    /// the transmon's lumped-port reactive shunt (a surface term the tet kernel
    /// structurally cannot absorb — Phase 3 of #302), so this gates the
    /// distribution-invariance of the *matrix-free operator's* spectrum on the
    /// transmon geometry, not the full shift-invert `transmon_bench` eigenvalues.
    /// Wiring the distributed COCG into the production shift-invert Lanczos with
    /// the shunt is Phase C integration.
    #[test]
    #[ignore = "release-tier: partitions the full 133k-tet transmon fixture and builds P sub-operators; run with --ignored --nocapture"]
    fn transmon_distributed_spectrum_matches_single_process() {
        use crate::mesh::read_transmon_smoke_fixture;
        use crate::mesh::spiral::pec_interior_mask_from_triangles;

        let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
        let mesh = &fx.mesh;
        let n = mesh.edges().len();
        let eps = fx.epsilon_r_scalar();
        let edges = mesh.edges();
        let metal = fx.metal_triangles();
        let exterior = fx.exterior_boundary_triangles();
        let mask =
            pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);

        let seed = seed_global(n);
        let m = 20;

        let ritz_for = |k: usize| {
            let part = Partition::from_tet_mesh(mesh, k);
            let coll = LocalMock::new(&part);
            let op = DistributedOperator::<Bk>::from_mesh(mesh, &part, &eps, Some(&mask), &dev())
                .with_combination(1.0, 1.0);
            let sd = DistributedVector::<Bk>::from_global(&seed, &part, &dev());
            lanczos_ritz(&op, &coll, &sd, m)
        };

        let reference = ritz_for(1);
        let scale = reference.last().copied().unwrap_or(1.0).abs().max(1.0);
        eprintln!("transmon matrix-free A=K+M spectrum (lowest Ritz values):");
        for (i, r) in reference.iter().take(8).enumerate() {
            eprintln!("  Ritz[{i}] = {r:.6e}");
        }
        for k in [2usize, 4, 8] {
            let spec = ritz_for(k);
            assert_eq!(spec.len(), reference.len(), "P={k} Krylov dim drift");
            for i in 0..reference.len().min(8) {
                assert!(
                    (spec[i] - reference[i]).abs() < 1e-5 * scale,
                    "P={k} Ritz[{i}] = {} != {}",
                    spec[i],
                    reference[i]
                );
            }
        }
    }
}
