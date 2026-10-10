//! Issue #1003 design-spike probe. SCRATCH EVIDENCE, NOT A SHIPPED FEATURE.
//!
//! This file is NOT built by cargo (it lives under docs/). It produced every
//! "measured" number in `transmon-lumped-port-formulation.md`. It compares the
//! shipped distributed isotropic `S_Γ` port term against a directional surface
//! term (A1) and a rank-1 port-voltage term (A2) on the 133k transmon fixture.
//!
//! To reproduce (from the repo root, origin/main @ d7c95504):
//!
//! ```sh
//! cp docs/research/transmon-lumped-port-formulation.probe-1003.rs \
//!    crates/geode-core/examples/port_rank1_probe_1003.rs
//! cargo run -p geode-core --release --example port_rank1_probe_1003
//! # ungauged solves of all three variants at σ = 4.5 and 17.5 GHz
//! PROBE_1003_PROJ=1 cargo run -p geode-core --release --example port_rank1_probe_1003
//! # Gᵀb check + S_Γ and A2 through the bulk projector, σ = 4.5 GHz, N = 6
//! rm crates/geode-core/examples/port_rank1_probe_1003.rs
//! ```

use std::collections::HashMap;
use std::time::Instant;

use burn::tensor::Tensor;
use burn::tensor::backend::BackendTypes;
use faer::c64;
use faer::sparse::{SparseColMat, Triplet};

use geode_core::assembly::nedelec::{
    NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
};
use geode_core::assembly::p1::upload_mesh;
use geode_core::driven::ports::{assemble_port_flux, assemble_port_surface_mass};
use geode_core::eigen::lanczos::{InnerPreconditioner, InnerSolver, SparseShiftInvertLanczos};
use geode_core::eigen::transmon::{
    ReactiveElementNatural, frequency_hz_from_lambda, lambda_shift_for_frequency_hz,
};
use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
use geode_core::mesh::{TetMesh, read_transmon_smoke_fixture};
use geode_core::testing::TestBackend;

type B = TestBackend;
const M_PER_UNIT: f64 = 1e-6;
const L_H: f64 = 14.860e-9;
const C_F: f64 = 5.5e-15;

fn vals(t: Tensor<B, 1>) -> Vec<f64> {
    t.into_data().iter::<f64>().collect()
}

fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Own Whitney surface mass with global orientation `edges[e] = [a, b]`,
/// `N_e = λ_a ∇λ_b − λ_b ∇λ_a`. `proj = None` → isotropic `N_i·N_j`;
/// `Some(ê)` → directional `(N_i·ê)(N_j·ê)`.
fn own_surface_mass(
    mesh: &TetMesh,
    faces: &[[u32; 3]],
    lookup: &HashMap<(u32, u32), usize>,
    proj: Option<[f64; 3]>,
) -> Vec<(usize, usize, f64)> {
    let mut out = Vec::new();
    for tri in faces {
        let p: Vec<[f64; 3]> = tri.iter().map(|&n| mesh.nodes[n as usize]).collect();
        let e1 = sub(p[1], p[0]);
        let e2 = sub(p[2], p[0]);
        let area = 0.5 * {
            let c = cross(e1, e2);
            dot(c, c).sqrt()
        };
        let (g11, g12, g22) = (dot(e1, e1), dot(e1, e2), dot(e2, e2));
        let det = g11 * g22 - g12 * g12;
        let (i11, i12, i22) = (g22 / det, -g12 / det, g11 / det);
        let gl1 = [
            i11 * e1[0] + i12 * e2[0],
            i11 * e1[1] + i12 * e2[1],
            i11 * e1[2] + i12 * e2[2],
        ];
        let gl2 = [
            i12 * e1[0] + i22 * e2[0],
            i12 * e1[1] + i22 * e2[1],
            i12 * e1[2] + i22 * e2[2],
        ];
        let gl0 = [
            -gl1[0] - gl2[0],
            -gl1[1] - gl2[1],
            -gl1[2] - gl2[2],
        ];
        let g = [gl0, gl1, gl2];
        let ip = |a: [f64; 3], b: [f64; 3]| match proj {
            None => dot(a, b),
            Some(e) => dot(a, e) * dot(b, e),
        };
        let mm = |pp: usize, q: usize| area * if pp == q { 2.0 } else { 1.0 } / 12.0;
        // local edges as (local a, local b, global index) with global orientation
        let mut le = Vec::new();
        for (x, y) in [(0usize, 1usize), (1, 2), (0, 2)] {
            let (na, nb) = (tri[x], tri[y]);
            let key = (na.min(nb), na.max(nb));
            let gi = lookup[&key];
            // orient as global edges[gi] = [a, b]
            let (la, lb) = if na == key.0 { (x, y) } else { (y, x) };
            le.push((la, lb, gi));
        }
        for &(a, b, gi) in &le {
            for &(c, d, gj) in &le {
                // ∫ (λa gb − λb ga)·(λc gd − λd gc)
                let v = mm(a, c) * ip(g[b], g[d]) - mm(a, d) * ip(g[b], g[c])
                    - mm(b, c) * ip(g[a], g[d])
                    + mm(b, d) * ip(g[a], g[c]);
                out.push((gi, gj, v));
            }
        }
    }
    out
}

fn reduced(
    base: Option<(&[u32], &[u32], &[f64])>,
    extra: &[(usize, usize, f64)],
    idx: &[Option<usize>],
    dim: usize,
) -> SparseColMat<usize, f64> {
    let mut t = Vec::new();
    if let Some((r, c, v)) = base {
        for k in 0..v.len() {
            if let (Some(i), Some(j)) = (idx[r[k] as usize], idx[c[k] as usize]) {
                t.push(Triplet::new(i, j, v[k]));
            }
        }
    }
    for &(r, c, v) in extra {
        if let (Some(i), Some(j)) = (idx[r], idx[c]) {
            t.push(Triplet::new(i, j, v));
        }
    }
    SparseColMat::<usize, f64>::try_new_from_triplets(dim, dim, &t).unwrap()
}

fn quad(a: &SparseColMat<usize, f64>, x: &[f64]) -> f64 {
    let (cp, ri, v) = (a.col_ptr(), a.row_idx(), a.val());
    let mut acc = 0.0;
    for j in 0..a.ncols() {
        for k in cp[j]..cp[j + 1] {
            acc += x[ri[k]] * v[k] * x[j];
        }
    }
    acc
}

fn main() {
    let f = read_transmon_smoke_fixture().expect("fixture");
    let mesh = &f.mesh;
    let edges = mesh.edges();
    let te = mesh.tet_edges();
    let tidx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
    let tsgn: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
    let metal = f.metal_triangles();
    let ext = f.exterior_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), ext.as_slice()]);
    let mut idx = vec![None; edges.len()];
    let mut dim = 0;
    for (e, &k) in mask.iter().enumerate() {
        if k {
            idx[e] = Some(dim);
            dim += 1;
        }
    }
    let scatter = NedelecScatterMap::new(&tidx);
    let dev = <B as BackendTypes>::Device::default();
    let (nt, tt) = upload_mesh::<B>(mesh, &dev);
    let id: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu = vec![id; mesh.n_tets()];
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nt,
        tt,
        &tsgn,
        &scatter,
        &f.epsilon_tensor_r(),
        &nu,
    );
    let kv = vals(sys.k_re_vals);
    let mv = vals(sys.m_re_vals);
    let pat = scatter.pattern();

    let port = f.lumped_element_port();
    let el = ReactiveElementNatural::from_si(L_H, C_F, M_PER_UNIT);
    let (ell, w) = (port.length, port.width);
    eprintln!(
        "port: {} faces, e_hat {:?}, ℓ = {ell:.4}, w = {w:.4}, area = ℓw = {:.4}",
        port.faces.len(),
        port.e_hat,
        ell * w
    );

    // Patch node census.
    let mut patch_nodes: Vec<u32> = port.faces.iter().flatten().copied().collect();
    patch_nodes.sort();
    patch_nodes.dedup();
    let mut pec_node = vec![false; mesh.n_nodes()];
    for t in metal.iter().chain(ext.iter()) {
        for &n in t {
            pec_node[n as usize] = true;
        }
    }
    let free_patch_nodes = patch_nodes.iter().filter(|&&n| !pec_node[n as usize]).count();
    let mut patch_edges = std::collections::BTreeSet::new();
    for t in &port.faces {
        for (x, y) in [(0, 1), (1, 2), (0, 2)] {
            patch_edges.insert((t[x].min(t[y]), t[x].max(t[y])));
        }
    }
    let lookup: HashMap<(u32, u32), usize> = edges
        .iter()
        .enumerate()
        .map(|(i, e)| ((e[0].min(e[1]), e[0].max(e[1])), i))
        .collect();
    let interior_patch_edges = patch_edges
        .iter()
        .filter(|k| mask[lookup[*k]])
        .count();
    eprintln!(
        "patch: {} nodes ({} not on PEC), {} edges ({} interior/kept)",
        patch_nodes.len(),
        free_patch_nodes,
        patch_edges.len(),
        interior_patch_edges
    );

    // Self-checks of orientation: own isotropic vs library S_Γ; own flux.
    let s_lib = assemble_port_surface_mass(mesh, &port.faces, &edges);
    let s_own = own_surface_mass(mesh, &port.faces, &lookup, None);
    let lib = reduced(None, &s_lib, &(0..edges.len()).map(Some).collect::<Vec<_>>(), edges.len());
    let own = reduced(None, &s_own, &(0..edges.len()).map(Some).collect::<Vec<_>>(), edges.len());
    let mut diff = 0.0_f64;
    let mut scale = 0.0_f64;
    for j in 0..lib.ncols() {
        let mut col = HashMap::new();
        for k in lib.col_ptr()[j]..lib.col_ptr()[j + 1] {
            *col.entry(lib.row_idx()[k]).or_insert(0.0) += lib.val()[k];
        }
        for k in own.col_ptr()[j]..own.col_ptr()[j + 1] {
            *col.entry(own.row_idx()[k]).or_insert(0.0) -= own.val()[k];
        }
        for k in lib.col_ptr()[j]..lib.col_ptr()[j + 1] {
            scale = scale.max(lib.val()[k].abs());
        }
        for v in col.values() {
            diff = diff.max(v.abs());
        }
    }
    eprintln!("orientation check: max|S_lib − S_own| = {diff:.3e} (scale {scale:.3e})");
    let flux = assemble_port_flux(mesh, &port.faces, port.e_hat, &edges);
    // uniform-field interpolant u_e = ê·(x_b − x_a)
    let u: Vec<f64> = edges
        .iter()
        .map(|e| dot(port.e_hat, sub(mesh.nodes[e[1] as usize], mesh.nodes[e[0] as usize])))
        .collect();
    let mut su = vec![0.0; edges.len()];
    for &(r, c, v) in &s_lib {
        su[r] += v * u[c];
    }
    let mut fdiff = 0.0_f64;
    let mut fmax = 0.0_f64;
    for e in 0..edges.len() {
        fdiff = fdiff.max((su[e] - flux[e]).abs());
        fmax = fmax.max(flux[e].abs());
    }
    let usu: f64 = (0..edges.len()).map(|e| u[e] * su[e]).sum();
    eprintln!(
        "flux check: max|S_Γ u − f| = {fdiff:.3e} (max f {fmax:.3e}); uᵀS_Γu = {usu:.4} vs area {:.4}",
        ell * w
    );

    // Port voltage functional b = f / w, restricted to interior DOFs.
    let mut b_red = vec![0.0; dim];
    let mut b_full_trips = Vec::new();
    let bsupp: Vec<usize> = (0..edges.len()).filter(|&e| flux[e] != 0.0).collect();
    for &e in &bsupp {
        if let Some(r) = idx[e] {
            b_red[r] = flux[e] / w;
        }
    }
    for &i in &bsupp {
        for &j in &bsupp {
            b_full_trips.push((i, j, flux[i] * flux[j] / (w * w)));
        }
    }
    eprintln!("b support: {} edges", bsupp.len());

    let kscale = ell / (w * el.l_natural);
    let mscale = el.c_natural * ell / w;
    let base = (&pat.rows[..], &pat.cols[..], &kv[..]);
    let basem = (&pat.rows[..], &pat.cols[..], &mv[..]);
    let sc = |tr: &[(usize, usize, f64)], s: f64| -> Vec<(usize, usize, f64)> {
        tr.iter().map(|&(r, c, v)| (r, c, s * v)).collect()
    };
    let s_dir = own_surface_mass(mesh, &port.faces, &lookup, Some(port.e_hat));
    let variants: Vec<(&str, Vec<(usize, usize, f64)>, Vec<(usize, usize, f64)>)> = vec![
        ("S_Gamma (shipped)", sc(&s_lib, kscale), sc(&s_lib, mscale)),
        ("A1 directional", sc(&s_dir, kscale), sc(&s_dir, mscale)),
        (
            "A2 rank-1 b bT",
            sc(&b_full_trips, 1.0 / el.l_natural),
            sc(&b_full_trips, el.c_natural),
        ),
    ];
    let s_iso_red = reduced(None, &s_lib, &idx, dim);
    let n_modes = 8;
    if std::env::var("PROBE_1003_PROJ").is_ok() {
        use geode_core::eigen::projection::{
            InteriorGradient, MOrthogonalGradientProjector, ProjectedShiftInvertLanczos,
        };
        let grad = InteriorGradient::build(&edges, &mask, &idx, mesh.n_nodes(), dim);
        let g = grad.matrix();
        // ‖Gᵀ b‖_∞ and ‖Gᵀ S_Γ‖ (max abs entry)
        let (gcp, gri, gv) = (g.col_ptr(), g.row_idx(), g.val());
        let mut gtb = 0.0_f64;
        let mut gts = 0.0_f64;
        for col in 0..g.ncols() {
            let mut acc = 0.0;
            let mut row_s = vec![0.0; dim];
            let mut touched = false;
            for p in gcp[col]..gcp[col + 1] {
                acc += gv[p] * b_red[gri[p]];
                // (Gᵀ S)[col, :] = Σ_r G[r,col] S[r,:]
                let r = gri[p];
                let (scp, sri, sv) = (s_iso_red.col_ptr(), s_iso_red.row_idx(), s_iso_red.val());
                // S symmetric: row r == column r
                for q in scp[r]..scp[r + 1] {
                    row_s[sri[q]] += gv[p] * sv[q];
                    touched = true;
                }
            }
            gtb = gtb.max(acc.abs());
            if touched {
                gts = gts.max(row_s.iter().fold(0.0_f64, |a, &b| a.max(b.abs())));
            }
        }
        eprintln!("max|Gᵀ b| = {gtb:.3e}   max|Gᵀ S_Γ| = {gts:.3e}");
        for (name, kp, mp) in &variants {
            if name.starts_with("A1") {
                continue;
            }
            let k = reduced(Some(base), kp, &idx, dim);
            let m = reduced(Some(basem), mp, &idx, dim);
            let proj = MOrthogonalGradientProjector::build(&grad, m.as_ref()).unwrap();
            let solver = ProjectedShiftInvertLanczos {
                sigma: lambda_shift_for_frequency_hz(4.5e9, M_PER_UNIT),
                max_iters: 96,
                tol: 1e-8,
                reproject_threshold: 1e-8,
            };
            let t0 = Instant::now();
            let (pairs, diag) = solver
                .smallest_nonnull_eigenpairs(k.as_ref(), m.as_ref(), &proj, 6)
                .unwrap();
            eprintln!(
                "== {name} + BULK projector P, σ = 4.5 GHz, N = 6 nonnull ({:.1}s); residuals {:?}",
                t0.elapsed().as_secs_f64(),
                diag.mode_residual_rels
                    .iter()
                    .map(|r| format!("{r:.1e}"))
                    .collect::<Vec<_>>()
            );
            for p in &pairs {
                let v: f64 = b_red.iter().zip(&p.vector).map(|(a, b)| a * b).sum();
                let e_iso = kscale * quad(&s_iso_red, &p.vector);
                eprintln!(
                    "  f = {:9.4} GHz  V²/L share = {:.4}",
                    frequency_hz_from_lambda(p.lambda, M_PER_UNIT) / 1e9,
                    if e_iso > 0.0 { v * v / el.l_natural / e_iso } else { 0.0 }
                );
            }
        }
        return;
    }
    for (name, kp, mp) in &variants {
        let k = reduced(Some(base), kp, &idx, dim);
        let m = reduced(Some(basem), mp, &idx, dim);
        let kp_red = reduced(None, kp, &idx, dim);
        for &sig_ghz in &[4.5_f64, 17.5] {
            let t0 = Instant::now();
            let solver = SparseShiftInvertLanczos {
                sigma: lambda_shift_for_frequency_hz(sig_ghz * 1e9, M_PER_UNIT),
                max_iters: 96,
                tol: 1e-8,
                inner: InnerSolver::Direct,
                precond: InnerPreconditioner::Jacobi,
            };
            let pairs = solver
                .smallest_eigenpairs(k.as_ref(), m.as_ref(), n_modes)
                .expect("solve");
            eprintln!(
                "== {name} @ σ = {sig_ghz} GHz ({:.1}s)",
                t0.elapsed().as_secs_f64()
            );
            for p in &pairs {
                let fghz = frequency_hz_from_lambda(p.lambda, M_PER_UNIT) / 1e9;
                let x = &p.vector;
                let kt = quad(&k, x);
                let part = if kt > 0.0 { quad(&kp_red, x) / kt } else { 0.0 };
                let v: f64 = b_red.iter().zip(x).map(|(a, b)| a * b).sum();
                // energy in the uniform (port-voltage) part vs isotropic patch energy
                let e_iso = kscale * quad(&s_iso_red, x);
                let e_v = v * v / el.l_natural;
                let share = if e_iso > 0.0 { e_v / e_iso } else { 0.0 };
                eprintln!(
                    "  f = {fghz:9.4} GHz  λ = {:+.4e}  p_stiff = {part:.4}  V²/L share of S_Γ energy = {share:.4}",
                    p.lambda
                );
            }
        }
    }
}
