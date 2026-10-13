//! Pins for the DeviceLayout.jl SingleTransmon cross-check (issue #1092):
//! `benchmarks/transmon_devicelayout_crosscheck/results.toml`, written by
//! `examples/transmon_devicelayout_crosscheck.rs` in assemble mode from the
//! committed geode run records and Palace cells.
//!
//! Default tier: the artifact re-derives from its raw inputs (the Palace
//! `eig.csv` / `port-EPR.csv` files, the geode run records), the ladder
//! arithmetic (observed order, Richardson, band) re-derives from the raw
//! levels, the published values are the blog's, and the physics gates
//! hold: geode and Palace agree on the same mesh, Palace at DeviceLayout's
//! own discretization reproduces the published pair, and the converged
//! estimates sit within the stated distance of the published values.
//!
//! Release tier (`--ignored`): one A2 solve of the committed initial
//! order-1 mesh reproduces the `initial_h` record.

use std::path::PathBuf;

fn repo() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn results() -> toml::Value {
    let path = repo().join("benchmarks/transmon_devicelayout_crosscheck/results.toml");
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    toml::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn tget<'a>(v: &'a toml::Value, path: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in path.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {path} (at {k})"));
    }
    cur
}

fn tf(v: &toml::Value, path: &str) -> f64 {
    let x = tget(v, path);
    x.as_float()
        .or_else(|| x.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{path} is not a number"))
}

fn tarr(v: &toml::Value, path: &str) -> Vec<f64> {
    tget(v, path)
        .as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| {
            x.as_float()
                .or_else(|| x.as_integer().map(|i| i as f64))
                .unwrap()
        })
        .collect()
}

fn tstrs<'a>(v: &'a toml::Value, path: &str) -> Vec<&'a str> {
    tget(v, path)
        .as_array()
        .unwrap_or_else(|| panic!("{path} is not an array"))
        .iter()
        .map(|x| x.as_str().expect("string"))
        .collect()
}

fn tstr<'a>(v: &'a toml::Value, path: &str) -> &'a str {
    tget(v, path).as_str().unwrap_or_else(|| panic!("{path} is not a string"))
}

const TAGS: [&str; 2] = ["initial", "final"];
const PUBLISHED_F_GHZ: [[f64; 2]; 2] = [[4.14, 5.591], [3.005, 3.982]];
const PUBLISHED_PARAMS_UM: [[f64; 2]; 2] = [[620.0, 5000.0], [1217.678, 7243.955]];

/// The two lowest physical modes of a committed Palace cell, from its raw
/// outputs: the port-sheet artifact (|EPR| < 1e-6 on the junction port,
/// the last port-EPR column) is dropped, then the modes are in frequency
/// order as Palace writes them.
fn palace_raw(cell: &str) -> ((f64, f64), (f64, f64)) {
    let base = repo().join("reference/fixtures/transmon_palace/results_crosscheck_1092").join(cell);
    let read = |f: &str| std::fs::read_to_string(base.join(f)).unwrap_or_else(|e| panic!("{cell}/{f}: {e}"));
    let eig = read("eig.csv");
    let epr = read("port-EPR.csv");
    let f: Vec<f64> = eig
        .lines()
        .skip(1)
        .map(|l| l.split(',').nth(1).unwrap().trim().parse().unwrap())
        .collect();
    let ncol = epr.lines().next().unwrap().split(',').count();
    let p: Vec<f64> = epr
        .lines()
        .skip(1)
        .map(|l| l.split(',').nth(ncol - 1).unwrap().trim().parse::<f64>().unwrap().abs())
        .collect();
    assert_eq!(f.len(), p.len(), "{cell}");
    let kept: Vec<(f64, f64)> = f.into_iter().zip(p).filter(|(_, p)| *p >= 1e-6).collect();
    assert!(kept.len() >= 2, "{cell}: {kept:?}");
    assert!(kept[0].0 < kept[1].0, "{cell}: {kept:?}");
    (kept[0], kept[1])
}

fn geode_raw(label: &str) -> (Vec<f64>, Vec<f64>) {
    let path = repo().join(format!(
        "benchmarks/transmon_devicelayout_crosscheck/geode_runs/{label}.toml"
    ));
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let d: toml::Value = toml::from_str(&text).unwrap();
    assert_eq!(tstr(&d, "meta.float_dtype"), "F64", "{label}");
    assert_eq!(tstr(&d, "meta.label"), label);
    assert!(tf(&d, "run.max_residual_rel") <= 1e-6, "{label}");
    (tarr(&d, "run.f_ghz"), tarr(&d, "run.p"))
}

fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!((a - b).abs() <= tol, "{what}: {a} vs {b} (tol {tol})");
}

/// The artifact's published block is the blog's numbers.
#[test]
fn published_values_are_the_blog_values() {
    let d = results();
    for (i, tag) in TAGS.iter().enumerate() {
        let f = tarr(&d, &format!("published.{tag}_f_ghz"));
        assert_eq!(f, PUBLISHED_F_GHZ[i], "{tag}");
        close(tf(&d, &format!("published.{tag}_capacitor_length_um")), PUBLISHED_PARAMS_UM[i][0], 0.0, tag);
        close(tf(&d, &format!("published.{tag}_total_length_um")), PUBLISHED_PARAMS_UM[i][1], 0.0, tag);
        close(tf(&d, &format!("convergence.{tag}.published_f_q_ghz")), PUBLISHED_F_GHZ[i][0], 0.0, tag);
        close(tf(&d, &format!("convergence.{tag}.published_f_r_ghz")), PUBLISHED_F_GHZ[i][1], 0.0, tag);
    }
    assert!(tstr(&d, "published.discretization").contains("mesh_order 2, solver_order 2"));
}

/// Every Palace number in the artifact re-reads from its committed cell.
#[test]
fn palace_cells_re_read_from_their_raw_outputs() {
    let d = results();
    for tag in TAGS {
        for (kind, n) in [("p1", 3usize), ("p2", 2)] {
            let cells = tstrs(&d, &format!("palace.{tag}.{kind}_cells"));
            assert_eq!(cells.len(), n, "{tag} {kind}");
            let fq = tarr(&d, &format!("palace.{tag}.{kind}_f_q_ghz"));
            let fr = tarr(&d, &format!("palace.{tag}.{kind}_f_r_ghz"));
            let pq = tarr(&d, &format!("palace.{tag}.{kind}_p_q"));
            for (i, cell) in cells.iter().enumerate() {
                assert!(cell.starts_with(tag), "{cell}");
                let ((f0, p0), (f1, _)) = palace_raw(cell);
                close(fq[i], f0, 1e-6, cell);
                close(fr[i], f1, 1e-6, cell);
                close(pq[i], p0, 1e-6, cell);
            }
        }
        let dl = tstr(&d, &format!("devicelayout_run.{tag}.cell"));
        assert_eq!(dl, format!("dl_{tag}"));
        let ((f0, _), (f1, _)) = palace_raw(dl);
        close(tf(&d, &format!("devicelayout_run.{tag}.f_q_ghz")), f0, 1e-6, dl);
        close(tf(&d, &format!("devicelayout_run.{tag}.f_r_ghz")), f1, 1e-6, dl);
        let pec = tstr(&d, &format!("devicelayout_run.{tag}.pec_box_cell"));
        assert_eq!(pec, format!("dlpec_{tag}"));
        let ((f0, _), (f1, _)) = palace_raw(pec);
        close(tf(&d, &format!("devicelayout_run.{tag}.pec_box_f_q_ghz")), f0, 1e-6, pec);
        close(tf(&d, &format!("devicelayout_run.{tag}.pec_box_f_r_ghz")), f1, 1e-6, pec);
    }
}

/// Every geode number in the artifact re-reads from its committed run record.
#[test]
fn geode_runs_re_read_from_their_records() {
    let d = results();
    for tag in TAGS {
        let labels = tstrs(&d, &format!("geode.{tag}.labels"));
        assert_eq!(labels, vec![format!("{tag}_h"), format!("{tag}_h2")]);
        let fq = tarr(&d, &format!("geode.{tag}.f_q_ghz"));
        let fr = tarr(&d, &format!("geode.{tag}.f_r_ghz"));
        let pq = tarr(&d, &format!("geode.{tag}.p_q"));
        for (i, label) in labels.iter().enumerate() {
            let (f, p) = geode_raw(label);
            close(fq[i], f[0], 1e-6, label);
            close(fr[i], f[1], 1e-6, label);
            close(pq[i], p[0], 1e-6, label);
            assert!(p[0] > 0.9 && p[1] < 0.1, "{label}: qubit first, resonator second: {p:?}");
        }
    }
}

/// The ladder arithmetic re-derives from the raw levels.
#[test]
fn convergence_block_re_derives_from_the_levels() {
    let d = results();
    for (i, tag) in TAGS.iter().enumerate() {
        let c = |k: &str| format!("convergence.{tag}.{k}");
        let p1q = tarr(&d, &c("p1_f_q_ghz"));
        let p1r = tarr(&d, &c("p1_f_r_ghz"));
        let p2q = tarr(&d, &c("p2_f_q_ghz"));
        let p2r = tarr(&d, &c("p2_f_r_ghz"));
        assert_eq!(p1q, tarr(&d, &format!("palace.{tag}.p1_f_q_ghz")));
        assert_eq!(p1r, tarr(&d, &format!("palace.{tag}.p1_f_r_ghz")));
        assert_eq!(p2q, tarr(&d, &format!("palace.{tag}.p2_f_q_ghz")));
        assert_eq!(p2r, tarr(&d, &format!("palace.{tag}.p2_f_r_ghz")));
        let rich3 = |f: &[f64]| {
            let r = (f[0] - f[1]) / (f[1] - f[2]);
            assert!(r > 1.0, "{tag}: {f:?}");
            let p = r.log2();
            (p, f[2] + (f[2] - f[1]) / (2f64.powf(p) - 1.0))
        };
        let (oq, fq_inf) = rich3(&p1q);
        let (or_, fr_inf) = rich3(&p1r);
        close(tf(&d, &c("p1_observed_order_q")), oq, 1e-4, tag);
        close(tf(&d, &c("p1_observed_order_r")), or_, 1e-4, tag);
        close(tf(&d, &c("p1_richardson_f_q_ghz")), fq_inf, 1e-6, tag);
        close(tf(&d, &c("p1_richardson_f_r_ghz")), fr_inf, 1e-6, tag);
        let fq2 = p2q[1] + (p2q[1] - p2q[0]) / (2f64.powf(oq) - 1.0);
        let fr2 = p2r[1] + (p2r[1] - p2r[0]) / (2f64.powf(or_) - 1.0);
        close(tf(&d, &c("p2_richardson_f_q_ghz")), fq2, 1e-6, tag);
        close(tf(&d, &c("p2_richardson_f_r_ghz")), fr2, 1e-6, tag);
        // Order-1 (asymptotic) two-level extrapolations: f + (f - f_prev).
        assert_eq!(tf(&d, &c("asymptotic_order")), 1.0, "{tag}");
        let q1 = p1q[2] + (p1q[2] - p1q[1]);
        let r1 = p1r[2] + (p1r[2] - p1r[1]);
        let q2 = p2q[1] + (p2q[1] - p2q[0]);
        let r2 = p2r[1] + (p2r[1] - p2r[0]);
        close(tf(&d, &c("p1_order1_f_q_ghz")), q1, 1e-6, tag);
        close(tf(&d, &c("p1_order1_f_r_ghz")), r1, 1e-6, tag);
        close(tf(&d, &c("p2_order1_f_q_ghz")), q2, 1e-6, tag);
        close(tf(&d, &c("p2_order1_f_r_ghz")), r2, 1e-6, tag);
        let est_q = 0.5 * (q1 + q2);
        let est_r = 0.5 * (r1 + r2);
        close(tf(&d, &c("f_q_ghz")), est_q, 5e-5, tag);
        close(tf(&d, &c("f_r_ghz")), est_r, 5e-5, tag);
        let band_q = tarr(&d, &c("f_q_band_ghz"));
        let band_r = tarr(&d, &c("f_r_band_ghz"));
        let lo = |v: &[f64]| v.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = |v: &[f64]| v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let all_q = [q1, q2, fq_inf, fq2];
        let all_r = [r1, r2, fr_inf, fr2];
        close(band_q[0], lo(&all_q), 5e-5, tag);
        close(band_q[1], hi(&all_q), 5e-5, tag);
        close(band_r[0], lo(&all_r), 5e-5, tag);
        close(band_r[1], hi(&all_r), 5e-5, tag);
        let pubq = PUBLISHED_F_GHZ[i][0];
        let pubr = PUBLISHED_F_GHZ[i][1];
        close(tf(&d, &c("f_q_over_published_pct")), 100.0 * (est_q / pubq - 1.0), 2e-3, tag);
        close(tf(&d, &c("f_r_over_published_pct")), 100.0 * (est_r / pubr - 1.0), 2e-3, tag);
        let dl = tarr(&d, &c("devicelayout_run_over_published_pct"));
        let dlq = tf(&d, &format!("devicelayout_run.{tag}.f_q_ghz"));
        let dlr = tf(&d, &format!("devicelayout_run.{tag}.f_r_ghz"));
        close(dl[0], 100.0 * (dlq / pubq - 1.0), 2e-3, tag);
        close(dl[1], 100.0 * (dlr / pubr - 1.0), 2e-3, tag);
    }
}

/// Physics gates. The tolerances are the measured margins rounded up; see
/// the artifact and the PR for the numbers behind them.
#[test]
fn same_mesh_agreement_and_published_reproduction() {
    let d = results();
    for (i, tag) in TAGS.iter().enumerate() {
        // geode A2 vs Palace order 1 on the same mesh, both levels.
        let gq = tarr(&d, &format!("same_mesh.{tag}.geode_over_palace_f_q_pct"));
        let gr = tarr(&d, &format!("same_mesh.{tag}.geode_over_palace_f_r_pct"));
        for (k, x) in gq.iter().chain(&gr).enumerate() {
            assert!(x.abs() <= SAME_MESH_MAX_PCT, "{tag} same-mesh entry {k}: {x} %");
        }
        // Palace at DeviceLayout's own discretization reproduces the blog.
        let dl = tarr(&d, &format!("convergence.{tag}.devicelayout_run_over_published_pct"));
        for x in &dl {
            assert!(x.abs() <= DL_RUN_MAX_PCT, "{tag}: DeviceLayout run vs published {x} %");
        }
        // The converged estimate sits above the published pair: the
        // published values are DeviceLayout's discretization, which is
        // under-resolved. Both modes, both geometries, by the measured
        // margins (rounded inward).
        let q = tf(&d, &format!("convergence.{tag}.f_q_over_published_pct"));
        let r = tf(&d, &format!("convergence.{tag}.f_r_over_published_pct"));
        assert!(q >= CONVERGED_ABOVE_PUBLISHED_MIN_PCT[i][0], "{tag}: converged f_q vs published {q} %");
        assert!(r >= CONVERGED_ABOVE_PUBLISHED_MIN_PCT[i][1], "{tag}: converged f_r vs published {r} %");
        let below = tget(&d, &format!("convergence.{tag}.published_below_band"))
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_bool().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(below, vec![true, true], "{tag}: the published pair lies below the band");
        let bq = tarr(&d, &format!("convergence.{tag}.f_q_band_ghz"));
        let br = tarr(&d, &format!("convergence.{tag}.f_r_band_ghz"));
        assert!(PUBLISHED_F_GHZ[i][0] < bq[0] && PUBLISHED_F_GHZ[i][1] < br[0], "{tag}");
        // The ladders contract monotonically toward the estimate.
        for key in ["p1_f_q_ghz", "p1_f_r_ghz", "p2_f_q_ghz", "p2_f_r_ghz"] {
            let v = tarr(&d, &format!("convergence.{tag}.{key}"));
            assert!(v.windows(2).all(|w| w[0] < w[1]), "{tag} {key}: {v:?}");
        }
    }
}

/// Same-mesh geode-vs-Palace tolerance (%), both modes, both levels.
const SAME_MESH_MAX_PCT: f64 = 0.05;
/// Palace at DeviceLayout's discretization vs the published pair (%).
const DL_RUN_MAX_PCT: f64 = 1.0;
/// Minimum margin (%) of the converged estimate above the published pair,
/// per geometry and mode (measured 7.3 / 2.7 initial, 8.6 / 2.7 final).
const CONVERGED_ABOVE_PUBLISHED_MIN_PCT: [[f64; 2]; 2] = [[7.0, 2.5], [8.0, 2.5]];

/// The static route (certified tensor C_Sigma through the circuit) lands
/// inside the eigen band and within this distance (%) of the estimate.
const STATIC_MAX_PCT: f64 = 0.5;

/// The static cross-check re-derives from the certified bracket and the
/// circuit constants, and agrees with the eigen estimate.
#[test]
fn static_c_sigma_route_agrees_with_the_eigen_estimate() {
    use geode_core::mesh::transmon::{JUNCTION_CAPACITANCE_F, JUNCTION_INDUCTANCE_H};
    use geode_core::quantum::transmon::{
        DEVICELAYOUT_COUPLING_K_BAND, DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H,
        coupled_lc_dressed_pair_hz, harmonic_frequency_hz,
    };
    let d = results();
    let text = std::fs::read_to_string(
        repo().join("benchmarks/transmon_diffopt/c_sigma_bracket_tensor.toml"),
    )
    .unwrap();
    let b: toml::Value = toml::from_str(&text).unwrap();
    let c = [tf(&b, "bracket_x0_tensor.lower_ff"), tf(&b, "bracket_x0_tensor.upper_ff")];
    assert_eq!(tarr(&d, "static_cross_check.initial.c_sigma_bracket_ff"), c.to_vec());
    let fr = tf(&d, "convergence.initial.f_r_ghz");
    close(tf(&d, "static_cross_check.initial.resonator_f_r_ghz"), fr, 0.0, "f_r");
    let lc = |c_ff: f64, l_s: f64| {
        harmonic_frequency_hz(JUNCTION_INDUCTANCE_H + l_s, c_ff * 1e-15 + JUNCTION_CAPACITANCE_F)
    };
    let lc_only = tarr(&d, "static_cross_check.initial.lc_only_f_q_band_ghz");
    close(lc_only[0], lc(c[1], 0.0) / 1e9, 5e-5, "lc lo");
    close(lc_only[1], lc(c[0], 0.0) / 1e9, 5e-5, "lc hi");
    let mut v = Vec::new();
    for cc in c {
        for l_s in [DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H.0, DEVICELAYOUT_LOOP_INDUCTANCE_BAND_H.1] {
            for k in [DEVICELAYOUT_COUPLING_K_BAND.0, DEVICELAYOUT_COUPLING_K_BAND.1] {
                v.push(coupled_lc_dressed_pair_hz(lc(cc, l_s), fr * 1e9, k).0 / 1e9);
            }
        }
    }
    let lo = v.iter().copied().fold(f64::INFINITY, f64::min);
    let hi = v.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let band = tarr(&d, "static_cross_check.initial.dressed_f_q_band_ghz");
    close(band[0], lo, 5e-5, "dressed lo");
    close(band[1], hi, 5e-5, "dressed hi");
    let eig = tarr(&d, "convergence.initial.f_q_band_ghz");
    assert!(eig[0] <= lo && hi <= eig[1], "static {lo}..{hi} inside eigen {eig:?}");
    assert_eq!(tget(&d, "static_cross_check.initial.inside_eigen_band").as_bool(), Some(true));
    let est = tf(&d, "convergence.initial.f_q_ghz");
    let off = 100.0 * (est / (0.5 * (lo + hi)) - 1.0);
    close(tf(&d, "static_cross_check.initial.eigen_estimate_minus_static_mid_pct"), off, 2e-3, "off");
    assert!(off.abs() <= STATIC_MAX_PCT, "eigen estimate vs static midpoint {off} %");
}

/// Release tier: one A2 solve of the committed initial order-1 mesh
/// reproduces the `initial_h` record (about 30 s on 28 cores, 3 GB).
#[test]
#[ignore = "release tier: one 133k-tet A2 eigensolve"]
fn initial_h_record_reproduces_release() {
    use burn::tensor::Tensor;
    use burn::tensor::backend::BackendTypes;
    use faer::c64;
    use geode_core::assembly::nedelec::{
        NedelecScatterMap, assemble_global_nedelec_with_full_tensors_sparse,
    };
    use geode_core::assembly::p1::upload_mesh;
    use geode_core::eigen::projection::solve_transmon_eigenmodes_projected_with_port_model;
    use geode_core::eigen::transmon::{
        JunctionPortModel, LumpedReactiveShunt, ReactiveElementNatural, TransmonPencil,
        lambda_shift_for_frequency_hz,
    };
    use geode_core::mesh::read_transmon_fixture_from_bytes;
    use geode_core::mesh::spiral::pec_interior_mask_from_triangles;
    use geode_core::mesh::transmon::{JUNCTION_CAPACITANCE_F, JUNCTION_INDUCTANCE_H};
    use geode_core::testing::TestBackend as B;

    let (f_rec, p_rec) = geode_raw("initial_h");
    let bytes = std::fs::read(repo().join("reference/fixtures/transmon_devicelayout/initial_o1.msh"))
        .expect("committed initial order-1 mesh");
    let fx = read_transmon_fixture_from_bytes(&bytes).expect("mesh");
    let device = <B as BackendTypes>::Device::default();
    let edges = fx.mesh.edges();
    let te = fx.mesh.tet_edges();
    let idx: Vec<[u32; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].0)).collect();
    let sgn: Vec<[i8; 6]> = te.iter().map(|r| std::array::from_fn(|i| r[i].1)).collect();
    let eps = fx.epsilon_tensor_r();
    let scatter = NedelecScatterMap::new(&idx);
    let (nodes_t, tets_t) = upload_mesh::<B>(&fx.mesh, &device);
    let identity: [[c64; 3]; 3] = std::array::from_fn(|i| {
        std::array::from_fn(|j| c64::new(if i == j { 1.0 } else { 0.0 }, 0.0))
    });
    let nu = vec![identity; fx.mesh.n_tets()];
    let sys = assemble_global_nedelec_with_full_tensors_sparse::<B>(
        nodes_t, tets_t, &sgn, &scatter, &eps, &nu,
    );
    let host = |t: Tensor<B, 1>| -> Vec<f64> { t.into_data().iter::<f64>().collect() };
    let (k_vals, m_vals) = (host(sys.k_re_vals), host(sys.m_re_vals));
    let metal = fx.metal_triangles();
    let exterior = fx.exterior_boundary_triangles();
    let mask = pec_interior_mask_from_triangles(&edges, &[metal.as_slice(), exterior.as_slice()]);
    let jport = fx.lumped_element_port();
    let element = ReactiveElementNatural::from_si(JUNCTION_INDUCTANCE_H, JUNCTION_CAPACITANCE_F, 1e-6);
    let pencil = TransmonPencil {
        scatter: &scatter,
        k_vals: &k_vals,
        m_vals: &m_vals,
        edges: &edges,
        mesh: &fx.mesh,
        shunt: LumpedReactiveShunt {
            faces: &jport.faces,
            length: jport.length,
            width: jport.width,
            element,
        },
        interior_mask: &mask,
    };
    let sigma = lambda_shift_for_frequency_hz(5.0e9, 1e-6);
    let (mut modes, _) = solve_transmon_eigenmodes_projected_with_port_model(
        &pencil,
        JunctionPortModel::PortVoltage { e_hat: jport.e_hat },
        sigma,
        3,
        1e-6,
    )
    .expect("A2 solve");
    modes.sort_by(|a, b| a.frequency_hz.total_cmp(&b.frequency_hz));
    for (i, m) in modes.iter().enumerate() {
        close(m.frequency_ghz(), f_rec[i], 2e-6, "frequency");
        close(m.participation, p_rec[i], 2e-5, "participation");
    }
}
