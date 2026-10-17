# Decision record — the tensor-IR-vs-FFI collectives boundary (#546 Phase B)

**Status:** decided (Phase B, software-only).
**Context:** Epic #546 (distributed matrix-free solve). Depends on #634 (partition
+ halo maps). Implemented alongside `crates/geode-core/src/solver/distributed.rs`.
**Question:** Can the two collectives a distributed matrix-free Krylov solve
needs — **halo exchange** and **all-reduce** — stay inside the Burn / cubecl
tensor IR, or must they drop to an NCCL / NVSHMEM FFI when Phase C runs on real
multi-GPU hardware? This is the epic's headline design deliverable, and it is
answerable in software, before any cluster exists.

## TL;DR

**The arithmetic stays tensor-native; only the inter-rank byte movement is FFI.**
The boundary is the same for both collectives and it is crisp:

| Collective | Tensor-IR-native part (stays in Burn/cubecl) | FFI part (Phase C: NCCL/NVSHMEM) |
|---|---|---|
| **Halo exchange** | the per-rank `select` (gather at the halo index set) and `scatter(…, Add)` (scatter-add) — the *same two primitives the matvec already uses* | the point-to-point transfer of boundary values between two devices' address spaces (`ncclSend`/`ncclRecv`, or NVSHMEM `put`) |
| **All-reduce** | the per-rank partial reduction (`sum()` inside the bilinear dot) | the cross-rank combine of the resulting O(1) scalars (`ncclAllReduce`) |

The abstraction in `solver/distributed.rs` is drawn exactly on that line: the
[`Collective`] trait is the narrow FFI seam (its three methods are the only
inter-rank operations), and everything above it — `DistributedOperator`,
`DistributedCocg`, `distributed_dot`, `distributed_norm` — is backend-generic
tensor code that never names a transport. Swapping `LocalMock` for an
`NcclCollective` in Phase C changes no solver code.

## Why halo exchange splits this way

GEODE's matrix-free apply
(`assembly::nedelec_matvec::MatrixFreeNedelecOperator`) is, per rank:

1. **gather** each owned tet's six edge DOFs from the input vector
   (`x.select(0, edge_idx)`),
2. a batched `[n_elem,6,6]·[n_elem,6,1]` local apply, then
3. **scatter-add** the local results back (`scatter(0, edge_idx, …, Add)`).

To distribute it, each rank applies that kernel over *its* elements. Two things
have to cross the rank boundary, and both reduce to the *same* gather/scatter
primitives plus a transfer:

- **Before the local apply (forward, `halo_gather`):** a rank's elements read
  DOFs owned by neighbours (the #634 *ghost receive* set). Filling those ghost
  slots is a `select` of the neighbour's owned values (tensor IR) followed by a
  transfer of that small `[|halo|]` buffer into this rank's ghost slots. The
  index set is the static `HaloChannel::dofs` list from #634 — known at
  partition time, not recomputed per apply.
- **After the local apply (reverse, `halo_accumulate`):** a rank has computed a
  *partial* contribution to DOFs its neighbours own. Shipping those partials to
  the owner and summing them is a `select` on the sender + a transfer + a
  `scatter(…, Add)` on the owner. This is the exact transpose of the forward
  step and completes the global scatter-add.

What tensor IR **cannot** express is the transfer in the middle: moving a buffer
between two devices' address spaces has no representation in a single-device
tensor graph. That transfer — and only that transfer — is the FFI boundary.
NCCL's `Send`/`Recv` (or an NVSHMEM one-sided `put` into a pre-registered ghost
region) is the Phase-C realization; `LocalMock`'s in-memory host copy is the
Phase-B stand-in for it.

Note that halo exchange is **not** an all-gather: only interface DOFs move
(the `HaloChannel` lists), never the full vector. A naive "replicate the whole
input on every rank" design *could* be written as a pure tensor broadcast, but it
would move `O(n_edges)` per rank per apply instead of `O(interface)` and defeats
the point of partitioning. The mock deliberately moves only the halo lists, so it
measures the real communication volume the #634 metrics predict.

## Why all-reduce splits this way

A COCG inner product `ρ = rᵀz` over a partitioned vector is
`Σ_p (partial dot over part p's owned DOFs)`. Because #634 ownership is a
disjoint cover and the distributed vectors are kept in *canonical* form (nonzero
only on owned DOFs), the per-rank partial is literally
`SplitComplex::bilinear_dot` — four `mul` + `sum()` reductions, pure tensor IR,
already implemented for the single-process `BurnCocg`. The only cross-rank step
is summing the `P` resulting **scalars**. That is `ncclAllReduce` over a length-1
(here, one complex = two f32/f64) buffer — the cheapest possible collective.
`LocalMock::all_reduce_sum` is that combine on host.

So all-reduce is a *two-layer* operation and the layers land on opposite sides of
the boundary: reduction-of-vectors (tensor IR) then reduction-of-scalars (FFI).
The residual norm for the stopping criterion is the same shape: per-rank
`sum(|·|²)` (tensor IR) then a scalar all-reduce then a host `sqrt`.

## Consequences for Phase C

1. **The solver code is transport-agnostic and final.** `DistributedOperator`
   and `DistributedCocg` are written against `&dyn Collective<B>`. The Phase-C
   task is to implement `Collective` with NCCL, not to touch the Krylov loop.
2. **The FFI surface is tiny and well-typed:** three operations — send/recv a
   halo buffer (both directions) and all-reduce a scalar. This is the entire
   inter-rank API GEODE needs; everything else is single-device tensor code that
   already runs on any Burn backend.
3. **cubecl cannot host the transfer, and should not try.** NCCL/NVSHMEM own
   GPU-to-GPU RDMA, stream ordering, and topology; re-expressing point-to-point
   device transfers as cubecl kernels would duplicate that stack with no benefit.
   The right integration is a thin FFI `Collective` impl that hands NCCL the
   device pointers behind Burn's tensors, keeping the gather/scatter/reduction
   arithmetic in tensor IR on either side of the call.
4. **Overlap is a Phase-C optimization, not an abstraction change.** The
   interior-DOF local apply (no halo dependency) can run while halo transfers are
   in flight; the `Collective` seam already separates the transfer
   (`halo_gather`/`halo_accumulate`) from the compute (`DistributedOperator`),
   so a Phase-C impl can post non-blocking NCCL ops and overlap without any
   change above the seam.

## What Phase B validated in software

`solver/distributed.rs` + its tests demonstrate the decision holds end to end
with zero hardware:

- The distributed apply (`K`, `M`, `K − cM`) reproduces the single-process
  `MatrixFreeNedelecOperator` matvec to round-off for `P ∈ {1,2,4,8}`
  (`distributed_apply_matches_single_process`).
- The distributed COCG reproduces the single-process solve
  (`distributed_cocg_matches_single_process`).
- The Lanczos Ritz spectrum of the matrix-free pencil is invariant across
  `P ∈ {1,2,4,8}` on both a cube fixture (always-run) and the real 133k-tet
  transmon smoke fixture (`#[ignore]` release-tier) — distribution changes where
  work runs, not the answer.

### Scope note (honest boundary of the Phase-B spectrum gate)

The volumetric, tet-only matrix-free operator does not carry the transmon's
lumped-port reactive shunt — a *surface* term the tet kernel structurally cannot
absorb (the same Phase-3 limitation documented for `ksp_burn`). So the transmon
gate proves the distribution-invariance of the **matrix-free operator's**
spectrum on the transmon geometry, not the full shift-invert `transmon_bench`
eigenvalues (which include the shunt). Wiring the distributed COCG into the
production shift-invert Lanczos with the shunt is Phase-C integration; the
collectives boundary decided here is what that integration builds on.
