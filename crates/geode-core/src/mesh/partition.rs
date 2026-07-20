//! Mesh / edge-DOF **partitioner** + halo (ghost-DOF) map — the buildable-now,
//! pure-CPU foundation of the distributed-solve epic (#546 Phase A).
//!
//! # Why this exists
//!
//! GEODE's matrix-free Nédélec apply
//! ([`MatrixFreeNedelecOperator`](crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator))
//! gathers each tet's six edge DOFs, applies a `6×6` local operator, and
//! scatter-adds the result back — all on one node. To distribute that apply
//! across devices (Phase B onward) we must first cut the edge-DOF graph into
//! balanced subdomains, decide which part **owns** each DOF, and — for each
//! part — name the off-part DOFs its element-local apply reads (the **halo** /
//! ghost receive set) together with the transpose **send** lists that feed the
//! scatter-add. This module produces exactly that data structure. It is pure
//! CPU, single-process: no transport, no hardware, no GPU.
//!
//! # Algorithm (geometric recursive bisection, reusing the #543 primitive)
//!
//! 1. **Element partition.** Recursively bisect the *elements* by the median
//!    centroid coordinate along the longest bounding-box axis — the same
//!    geometric-bisection primitive (`longest_bbox_axis` /
//!    `rank_split_along_axis`) that `coordinate_nested_dissection` uses to
//!    order DOFs for the direct LU. Splitting proportionally (`⌊k/2⌋` parts
//!    left, `⌈k/2⌉` right) yields exactly `k` balanced element parts for any
//!    `k ≥ 1`.
//! 2. **DOF ownership.** Each edge DOF is incident to one or more elements. Its
//!    **owner** is the lowest-numbered part among those elements (a
//!    deterministic tie-break). This makes ownership a disjoint cover: every
//!    DOF has exactly one owner. A DOF incident to more than one part is an
//!    **interface** DOF of its owner; a DOF whose incident elements all lie in
//!    the owner part is **interior**.
//! 3. **Halo / send maps.** For every element `e` in part `q` and every edge
//!    DOF `d` it reads: if `owner(d) = p ≠ q`, then part `q` must **receive**
//!    `d` from `p`, and part `p` must **send** `d` to `q`. The receive lists of
//!    `q` and the send lists of `p` are transposes of each other by
//!    construction.
//!
//! The result is a self-contained [`Partition`] value. A documented
//! [`Partition::build`] seam takes the mesh geometry directly, leaving room for
//! a future ParMETIS-class backend (the #546 open question A) behind the same
//! [`Partition`] interface — a backend need only produce an element→part
//! assignment; the ownership/halo derivation here is backend-independent.

use std::collections::{BTreeMap, BTreeSet};

use crate::eigen::ordering::{longest_bbox_axis, rank_split_along_axis};
use crate::mesh::TetMesh;

/// One directional halo channel between two parts: the sorted list of global
/// edge-DOF indices exchanged with `peer`.
///
/// In a receive list, `peer` is the **owner** the DOFs are pulled *from*; in a
/// send list, `peer` is the part the owned DOFs are pushed *to*. The `dofs` are
/// sorted ascending and duplicate-free, keyed to the
/// [`MatrixFreeNedelecOperator`](crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator)
/// edge-DOF numbering (the `tet_edge_idx` the operator gathers through).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HaloChannel {
    /// The peer part index this channel exchanges DOFs with.
    pub peer: usize,
    /// Global edge-DOF indices (sorted ascending, unique).
    pub dofs: Vec<u32>,
}

/// Per-part decomposition data: owned elements, the interior/interface split of
/// its owned DOFs, and the halo receive / send channels.
#[derive(Debug, Clone, Default)]
pub struct PartData {
    /// Element indices owned by this part (`elem_part[e] == this part`).
    pub elements: Vec<u32>,
    /// Owned DOFs whose incident elements *all* lie in this part (no exchange).
    pub interior: Vec<u32>,
    /// Owned DOFs incident to at least one element in another part (the
    /// separator DOFs this part is the authoritative owner of).
    pub interface: Vec<u32>,
    /// Ghost receive channels: off-part DOFs this part's elements read, grouped
    /// by the owner part they must be received from. Sorted by `peer`.
    pub halo_recv: Vec<HaloChannel>,
    /// Send channels: owned DOFs other parts read, grouped by the destination
    /// part. Sorted by `peer`. The exact transpose of the peers' `halo_recv`.
    pub halo_send: Vec<HaloChannel>,
}

impl PartData {
    /// Number of DOFs owned by this part (`interior + interface`).
    pub fn n_owned(&self) -> usize {
        self.interior.len() + self.interface.len()
    }

    /// Total number of ghost DOFs this part must receive across all peers.
    pub fn halo_recv_volume(&self) -> usize {
        self.halo_recv.iter().map(|c| c.dofs.len()).sum()
    }

    /// Total number of owned DOFs this part must send across all peers.
    pub fn halo_send_volume(&self) -> usize {
        self.halo_send.iter().map(|c| c.dofs.len()).sum()
    }
}

/// A balanced `k`-way partition of a mesh's edge-DOF graph with per-part halo
/// (ghost) maps. See the [module docs](self) for the construction.
#[derive(Debug, Clone)]
pub struct Partition {
    k: usize,
    n_dofs: usize,
    /// Owner part of every edge DOF, length `n_dofs`.
    owner: Vec<u32>,
    /// Part assignment of every element, length `n_elem`.
    elem_part: Vec<u32>,
    /// Per-part data, length `k`.
    parts: Vec<PartData>,
}

impl Partition {
    /// Build a `k`-way partition directly from a [`TetMesh`].
    ///
    /// Derives the edge-DOF numbering ([`TetMesh::edges`] /
    /// [`TetMesh::tet_edges`]) and the per-element centroids from the mesh, then
    /// delegates to [`Partition::build`]. The DOF numbering matches what
    /// [`MatrixFreeNedelecOperator`](crate::assembly::nedelec_matvec::MatrixFreeNedelecOperator)
    /// gathers through, so the halo maps are directly usable to drive a
    /// distributed matrix-free apply.
    ///
    /// # Panics
    ///
    /// Panics if `k == 0`.
    pub fn from_tet_mesh(mesh: &TetMesh, k: usize) -> Self {
        let n_dofs = mesh.edges().len();
        let tet_edge_idx: Vec<[u32; 6]> = mesh
            .tet_edges()
            .iter()
            .map(|row| {
                let mut out = [0u32; 6];
                for (slot, &(idx, _sign)) in out.iter_mut().zip(row.iter()) {
                    *slot = idx;
                }
                out
            })
            .collect();
        let centroids: Vec<[f64; 3]> = mesh
            .tets
            .iter()
            .map(|t| {
                let mut c = [0.0f64; 3];
                for &v in t {
                    let p = mesh.nodes[v as usize];
                    c[0] += p[0];
                    c[1] += p[1];
                    c[2] += p[2];
                }
                [c[0] * 0.25, c[1] * 0.25, c[2] * 0.25]
            })
            .collect();
        Self::build(n_dofs, &tet_edge_idx, &centroids, k)
    }

    /// Build a `k`-way partition from the raw pieces: the DOF count, the
    /// per-element edge-DOF indices, and the per-element centroids.
    ///
    /// This is the backend seam. The element→part assignment is produced here
    /// by geometric recursive bisection; a future ParMETIS-class backend would
    /// replace only that step (see [`Partition::from_elem_part`]) — the
    /// ownership and halo derivation below is backend-independent.
    ///
    /// # Panics
    ///
    /// Panics if `k == 0`, if `elem_centroids.len() != tet_edge_idx.len()`, or
    /// if any edge index in `tet_edge_idx` is `>= n_dofs`.
    pub fn build(
        n_dofs: usize,
        tet_edge_idx: &[[u32; 6]],
        elem_centroids: &[[f64; 3]],
        k: usize,
    ) -> Self {
        assert!(k >= 1, "partition needs at least one part");
        assert_eq!(
            elem_centroids.len(),
            tet_edge_idx.len(),
            "one centroid per element"
        );
        let elem_part = geometric_kway_partition(elem_centroids, k);
        Self::from_elem_part(n_dofs, tet_edge_idx, elem_part, k)
    }

    /// Assemble the ownership + halo data structure from a *given*
    /// element→part assignment.
    ///
    /// This is the backend-independent core: any partitioner (geometric here, or
    /// a future graph partitioner) that produces a length-`n_elem`
    /// `elem_part` in `0..k` gets the identical ownership rule, interior /
    /// interface classification, and transpose-consistent halo maps.
    ///
    /// # Panics
    ///
    /// Panics if `elem_part.len() != tet_edge_idx.len()`, if any part index is
    /// `>= k`, or if any edge index is `>= n_dofs`.
    pub fn from_elem_part(
        n_dofs: usize,
        tet_edge_idx: &[[u32; 6]],
        elem_part: Vec<u32>,
        k: usize,
    ) -> Self {
        assert!(k >= 1, "partition needs at least one part");
        assert_eq!(
            elem_part.len(),
            tet_edge_idx.len(),
            "one part assignment per element"
        );

        // 1. DOF ownership: owner(d) = min incident element part. `multi[d]`
        //    records whether `d` is incident to more than one distinct part
        //    (hence an interface DOF of its owner).
        let mut owner = vec![u32::MAX; n_dofs];
        let mut multi = vec![false; n_dofs];
        for (e, edges) in tet_edge_idx.iter().enumerate() {
            let p = elem_part[e];
            assert!((p as usize) < k, "element part index out of range");
            for &d in edges {
                let d = d as usize;
                assert!(d < n_dofs, "edge index out of range");
                if owner[d] == u32::MAX {
                    owner[d] = p;
                } else if owner[d] != p {
                    multi[d] = true;
                    if p < owner[d] {
                        owner[d] = p;
                    }
                }
            }
        }

        // 2. Per-part owned-DOF interior / interface split.
        let mut parts: Vec<PartData> = vec![PartData::default(); k];
        for (e, &p) in elem_part.iter().enumerate() {
            parts[p as usize].elements.push(e as u32);
        }
        for d in 0..n_dofs {
            let ow = owner[d];
            if ow == u32::MAX {
                // A DOF referenced by no element; skip (does not occur for an
                // edge numbering derived from the mesh, but keep it robust).
                continue;
            }
            let pd = &mut parts[ow as usize];
            if multi[d] {
                pd.interface.push(d as u32);
            } else {
                pd.interior.push(d as u32);
            }
        }

        // 3. Halo receive / send maps. For every element `e` in part `q` reading
        //    DOF `d` owned by `p != q`: `q` receives `d` from `p`; `p` sends `d`
        //    to `q`. BTreeSet keeps the DOF lists sorted + deduplicated; the two
        //    directions are populated from the same (d, p, q) so they are exact
        //    transposes.
        let mut recv: Vec<BTreeMap<u32, BTreeSet<u32>>> = vec![BTreeMap::new(); k];
        let mut send: Vec<BTreeMap<u32, BTreeSet<u32>>> = vec![BTreeMap::new(); k];
        for (e, edges) in tet_edge_idx.iter().enumerate() {
            let q = elem_part[e];
            for &d in edges {
                let p = owner[d as usize];
                if p != q {
                    recv[q as usize].entry(p).or_default().insert(d);
                    send[p as usize].entry(q).or_default().insert(d);
                }
            }
        }
        for p in 0..k {
            parts[p].halo_recv = recv[p]
                .iter()
                .map(|(&peer, dofs)| HaloChannel {
                    peer: peer as usize,
                    dofs: dofs.iter().copied().collect(),
                })
                .collect();
            parts[p].halo_send = send[p]
                .iter()
                .map(|(&peer, dofs)| HaloChannel {
                    peer: peer as usize,
                    dofs: dofs.iter().copied().collect(),
                })
                .collect();
        }

        Self {
            k,
            n_dofs,
            owner,
            elem_part,
            parts,
        }
    }

    /// Number of parts.
    pub fn k(&self) -> usize {
        self.k
    }

    /// Total number of edge DOFs.
    pub fn n_dofs(&self) -> usize {
        self.n_dofs
    }

    /// Number of elements.
    pub fn n_elem(&self) -> usize {
        self.elem_part.len()
    }

    /// Owner part of edge DOF `dof`.
    ///
    /// # Panics
    ///
    /// Panics if `dof >= n_dofs`.
    pub fn owner_of(&self, dof: usize) -> usize {
        self.owner[dof] as usize
    }

    /// Part index that owns element `elem`.
    ///
    /// # Panics
    ///
    /// Panics if `elem >= n_elem`.
    pub fn elem_part(&self, elem: usize) -> usize {
        self.elem_part[elem] as usize
    }

    /// Per-part decomposition data.
    pub fn parts(&self) -> &[PartData] {
        &self.parts
    }

    /// Decomposition data for a single part.
    ///
    /// # Panics
    ///
    /// Panics if `part >= k`.
    pub fn part(&self, part: usize) -> &PartData {
        &self.parts[part]
    }

    /// Summary cut-quality metrics (load balance, interface / halo volume).
    pub fn metrics(&self) -> PartitionMetrics {
        let owned: Vec<usize> = self.parts.iter().map(PartData::n_owned).collect();
        let owned_min = owned.iter().copied().min().unwrap_or(0);
        let owned_max = owned.iter().copied().max().unwrap_or(0);
        let interface_total: usize = self.parts.iter().map(|p| p.interface.len()).sum();
        let halo_recv_total: usize = self.parts.iter().map(PartData::halo_recv_volume).sum();
        let halo_recv_max = self
            .parts
            .iter()
            .map(PartData::halo_recv_volume)
            .max()
            .unwrap_or(0);
        PartitionMetrics {
            k: self.k,
            n_dofs: self.n_dofs,
            owned_min,
            owned_max,
            interface_total,
            halo_recv_total,
            halo_recv_max,
        }
    }
}

/// Cut-quality summary of a [`Partition`] (issue #546 Phase A measurement).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartitionMetrics {
    /// Number of parts.
    pub k: usize,
    /// Total edge-DOF count.
    pub n_dofs: usize,
    /// Fewest DOFs owned by any single part.
    pub owned_min: usize,
    /// Most DOFs owned by any single part.
    pub owned_max: usize,
    /// Total interface (separator) DOFs, counted once at their owner.
    pub interface_total: usize,
    /// Total ghost DOFs received, summed over parts (the global halo volume).
    pub halo_recv_total: usize,
    /// Largest per-part ghost receive volume.
    pub halo_recv_max: usize,
}

impl PartitionMetrics {
    /// Load-balance ratio `owned_max / owned_min` (`1.0` is perfectly balanced;
    /// larger is worse). Returns `f64::INFINITY` if some part owns nothing.
    pub fn load_balance(&self) -> f64 {
        if self.owned_min == 0 {
            f64::INFINITY
        } else {
            self.owned_max as f64 / self.owned_min as f64
        }
    }
}

/// Recursive geometric `k`-way partition of `centroids`, returning a
/// length-`centroids.len()` element→part assignment in `0..k`.
///
/// Reuses the shared geometric-bisection primitive
/// ([`longest_bbox_axis`] / [`rank_split_along_axis`]) from the #543 ordering
/// module. Splits proportionally so any `k ≥ 1` yields exactly `k` parts.
fn geometric_kway_partition(centroids: &[[f64; 3]], k: usize) -> Vec<u32> {
    let n = centroids.len();
    let mut part = vec![0u32; n];
    let all: Vec<usize> = (0..n).collect();
    let mut next_part = 0u32;
    kway_recurse(&all, centroids, k, &mut part, &mut next_part);
    debug_assert_eq!(next_part as usize, k, "recursion must emit exactly k parts");
    part
}

/// Recursive worker for [`geometric_kway_partition`]. Emits exactly `k` part ids
/// (drawn in order from `next_part`) over `subset`, splitting proportionally.
fn kway_recurse(
    subset: &[usize],
    centroids: &[[f64; 3]],
    k: usize,
    part: &mut [u32],
    next_part: &mut u32,
) {
    if k <= 1 {
        let p = *next_part;
        *next_part += 1;
        for &e in subset {
            part[e] = p;
        }
        return;
    }
    if subset.len() < 2 {
        // Too few elements to bisect further: the first requested part takes
        // whatever is here, the remaining `k - 1` are emitted empty so the tree
        // still yields exactly `k` parts. (Does not occur when n_elem >= k for
        // a reasonably balanced mesh.)
        for i in 0..k {
            let p = *next_part;
            *next_part += 1;
            if i == 0 {
                for &e in subset {
                    part[e] = p;
                }
            }
        }
        return;
    }

    let k_left = k / 2;
    let axis = longest_bbox_axis(subset, centroids);
    // Proportional cut: give the left subtree a fraction k_left/k of the
    // elements, clamped so both sides are non-empty.
    let cut = (((subset.len() as f64) * (k_left as f64) / (k as f64)).round() as usize)
        .clamp(1, subset.len() - 1);
    let (left, right) = rank_split_along_axis(subset, centroids, axis, cut);
    kway_recurse(&left, centroids, k_left, part, next_part);
    kway_recurse(&right, centroids, k - k_left, part, next_part);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mesh::cube_tet_mesh;

    /// A DOF's owner must be one of the parts of its incident elements — the
    /// ownership rule cannot invent a part no incident element belongs to.
    fn assert_owner_is_incident(p: &Partition, tet_edge_idx: &[[u32; 6]]) {
        // owner(d) must equal the min part over incident elements.
        let k = p.k();
        let mut min_part = vec![u32::MAX; p.n_dofs()];
        for (e, edges) in tet_edge_idx.iter().enumerate() {
            let part = p.elem_part(e) as u32;
            for &d in edges {
                let d = d as usize;
                min_part[d] = min_part[d].min(part);
            }
        }
        for (d, &mp) in min_part.iter().enumerate() {
            if mp != u32::MAX {
                assert_eq!(p.owner_of(d), mp as usize, "owner != min incident part");
                assert!(p.owner_of(d) < k);
            }
        }
    }

    fn tet_edge_idx_of(mesh: &TetMesh) -> Vec<[u32; 6]> {
        mesh.tet_edges()
            .iter()
            .map(|row| {
                let mut out = [0u32; 6];
                for (slot, &(idx, _s)) in out.iter_mut().zip(row.iter()) {
                    *slot = idx;
                }
                out
            })
            .collect()
    }

    #[test]
    fn partition_is_a_valid_disjoint_cover() {
        let mesh = cube_tet_mesh(6, 1.0);
        let n_dofs = mesh.edges().len();
        for k in [1usize, 2, 3, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            assert_eq!(part.k(), k);
            assert_eq!(part.n_dofs(), n_dofs);

            // Every DOF has exactly one owner in 0..k, and the union of the
            // parts' owned (interior ∪ interface) DOF sets is all of 0..n_dofs
            // with no double-ownership.
            let mut owned_count = vec![0usize; n_dofs];
            for (p_idx, pd) in part.parts().iter().enumerate() {
                for &d in pd.interior.iter().chain(pd.interface.iter()) {
                    owned_count[d as usize] += 1;
                    assert_eq!(
                        part.owner_of(d as usize),
                        p_idx,
                        "owned DOF listed under the wrong part"
                    );
                }
            }
            for (d, &count) in owned_count.iter().enumerate() {
                assert_eq!(count, 1, "DOF {d} is not owned exactly once");
                assert!(part.owner_of(d) < k);
            }

            // Element parts also form a disjoint cover.
            let total_elems: usize = part.parts().iter().map(|p| p.elements.len()).sum();
            assert_eq!(
                total_elems,
                mesh.n_tets(),
                "elements not covered exactly once"
            );
        }
    }

    #[test]
    fn owner_is_min_incident_part() {
        let mesh = cube_tet_mesh(5, 1.0);
        let tei = tet_edge_idx_of(&mesh);
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            assert_owner_is_incident(&part, &tei);
        }
    }

    #[test]
    fn every_owned_element_dof_is_owned_or_haloed() {
        // Coverage criterion: for each part, every edge DOF of every element it
        // owns is either owned by that part or present in its halo receive set.
        let mesh = cube_tet_mesh(6, 1.0);
        let tei = tet_edge_idx_of(&mesh);
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            for (p_idx, pd) in part.parts().iter().enumerate() {
                // available = owned ∪ halo-received.
                let mut available: BTreeSet<u32> = BTreeSet::new();
                available.extend(pd.interior.iter().copied());
                available.extend(pd.interface.iter().copied());
                for ch in &pd.halo_recv {
                    available.extend(ch.dofs.iter().copied());
                }
                for &e in &pd.elements {
                    for &d in &tei[e as usize] {
                        assert!(
                            available.contains(&d),
                            "part {p_idx}: element {e} DOF {d} is neither owned nor haloed"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn send_and_receive_lists_are_transposes() {
        // Symmetry: part p receives set S from q  <=>  part q sends the same S
        // to p.
        let mesh = cube_tet_mesh(6, 1.0);
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            for (p_idx, pd) in part.parts().iter().enumerate() {
                for ch in &pd.halo_recv {
                    // q = ch.peer must have a matching send channel to p with
                    // exactly the same DOFs.
                    let q = part.part(ch.peer);
                    let matching =
                        q.halo_send
                            .iter()
                            .find(|s| s.peer == p_idx)
                            .unwrap_or_else(|| {
                                panic!("part {} has no send channel to {p_idx}", ch.peer)
                            });
                    assert_eq!(
                        matching.dofs, ch.dofs,
                        "recv(p={p_idx}<-q={}) != send(q={}->p={p_idx})",
                        ch.peer, ch.peer
                    );
                }
                // And the reverse: every send channel has a matching receive.
                for ch in &pd.halo_send {
                    let q = part.part(ch.peer);
                    let matching =
                        q.halo_recv
                            .iter()
                            .find(|r| r.peer == p_idx)
                            .unwrap_or_else(|| {
                                panic!("part {} has no recv channel from {p_idx}", ch.peer)
                            });
                    assert_eq!(matching.dofs, ch.dofs, "send/recv mismatch");
                }
            }
        }
    }

    #[test]
    fn halo_dofs_are_never_self_owned() {
        // A halo receive DOF must be owned by its declared peer, never by the
        // receiving part.
        let mesh = cube_tet_mesh(5, 1.0);
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(&mesh, k);
            for (p_idx, pd) in part.parts().iter().enumerate() {
                for ch in &pd.halo_recv {
                    assert_ne!(ch.peer, p_idx, "part receives from itself");
                    for &d in &ch.dofs {
                        assert_eq!(
                            part.owner_of(d as usize),
                            ch.peer,
                            "received DOF not owned by the declared peer"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn single_part_has_no_halo() {
        let mesh = cube_tet_mesh(4, 1.0);
        let part = Partition::from_tet_mesh(&mesh, 1);
        assert_eq!(part.k(), 1);
        let pd = part.part(0);
        assert!(pd.halo_recv.is_empty(), "k=1 must have no receive channels");
        assert!(pd.halo_send.is_empty(), "k=1 must have no send channels");
        assert!(pd.interface.is_empty(), "k=1 must have no interface DOFs");
        assert_eq!(pd.interior.len(), mesh.edges().len(), "k=1 owns all DOFs");
        assert_eq!(pd.n_owned(), part.n_dofs());
    }

    /// Issue #546 Phase A cut-quality measurement on the **real** 133k-tet
    /// transmon DeviceLayout mesh at `k = 2, 4, 8`. Reports separator / halo
    /// size and load balance, and re-checks the partition invariants at scale.
    /// `#[ignore]` (release-tier) because it loads and edge-numbers the full
    /// 133k-tet fixture; run with `--ignored --nocapture` to see the table.
    #[test]
    #[ignore = "release-tier: partitions the full 133k-tet transmon fixture; run with --ignored --nocapture"]
    fn transmon_133k_cut_quality_report() {
        use crate::mesh::read_transmon_smoke_fixture;
        let fx = read_transmon_smoke_fixture().expect("load transmon fixture");
        let mesh = &fx.mesh;
        let n_dofs = mesh.edges().len();
        let tei = tet_edge_idx_of(mesh);
        eprintln!(
            "\ntransmon fixture: n_tets={} n_nodes={} n_edges(DOFs)={}",
            mesh.n_tets(),
            mesh.n_nodes(),
            n_dofs
        );
        eprintln!(
            "{:>3} {:>10} {:>10} {:>12} {:>12} {:>12} {:>10}",
            "k", "owned_min", "owned_max", "load_bal", "interface", "halo_total", "halo_max"
        );
        for k in [2usize, 4, 8] {
            let part = Partition::from_tet_mesh(mesh, k);
            let m = part.metrics();
            // Invariants must still hold at scale.
            assert_owner_is_incident(&part, &tei);
            let mut owned_count = vec![0u8; n_dofs];
            for pd in part.parts() {
                for &d in pd.interior.iter().chain(pd.interface.iter()) {
                    owned_count[d as usize] += 1;
                }
            }
            assert!(
                owned_count.iter().all(|&c| c == 1),
                "every DOF owned exactly once at k={k}"
            );
            eprintln!(
                "{:>3} {:>10} {:>10} {:>12.4} {:>12} {:>12} {:>10}",
                k,
                m.owned_min,
                m.owned_max,
                m.load_balance(),
                m.interface_total,
                m.halo_recv_total,
                m.halo_recv_max
            );
        }
    }

    #[test]
    fn metrics_are_balanced_on_a_uniform_cube() {
        // The structured cube is geometrically uniform, so recursive median
        // bisection should keep the parts well balanced and the halo small
        // relative to the owned volume.
        let mesh = cube_tet_mesh(8, 1.0);
        let part = Partition::from_tet_mesh(&mesh, 4);
        let m = part.metrics();
        assert_eq!(m.k, 4);

        // The *element* partition (the compute load) is tightly balanced by the
        // proportional rank-median bisection.
        let elem_counts: Vec<usize> = part.parts().iter().map(|p| p.elements.len()).collect();
        let e_min = *elem_counts.iter().min().unwrap();
        let e_max = *elem_counts.iter().max().unwrap();
        assert!(
            e_max as f64 / e_min as f64 <= 1.02,
            "element load imbalance {e_max}/{e_min} too large"
        );

        // Owned-DOF balance is looser: the deterministic min-part owner
        // tie-break assigns every interface DOF to its lowest-numbered incident
        // part, which skews owned counts toward low part indices. It should
        // still be within a small constant.
        assert!(
            m.load_balance() < 1.35,
            "cube partition owned-DOF balance too skewed, got {}",
            m.load_balance()
        );
        // Interface DOFs must be a small fraction of the total on a coarse cut.
        assert!(
            m.interface_total * 3 < m.n_dofs,
            "interface {} unexpectedly large vs n_dofs {}",
            m.interface_total,
            m.n_dofs
        );
        assert!(m.halo_recv_total >= m.interface_total);
    }
}
