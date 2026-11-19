//! Windowed-peak averages on the L-bend (issue #1496): `geode conduction`
//! with the `--window-edge-m` group on the Trefethen (1984, Fig. 2(b))
//! L-shaped resistor, plus the uniform-bar checks of SI values, reference
//! ratios and conductor-intersection coverage.
//!
//! L geometry: unit-width L with outside legs of length 3, thickness 0.1
//! (mesh units = mm), conductivity 10 S/m — sheet resistance 1 Ω, so the
//! closed-form reference is `R / R_sheet = 4.55872841596`. The re-entrant
//! corner at the inside corner of the L makes the P1 current density
//! singular (`|J| ~ r^(-1/3)`): the per-element maximum grows under
//! uniform refinement while the four windowed averages converge at fixed
//! physical window size.

#[path = "support/msh.rs"]
mod msh;
#[path = "support/scratch.rs"]
mod scratch_support;

use std::path::Path;

use geode_core::mesh::TetMesh;
use scratch_support::Scratch;
use serde_json::{Value, json};

#[path = "support/cli.rs"]
mod cli_support;
use cli_support::geode;

/// Reference `R / R_sheet` (Trefethen 1984, Fig. 2(b) L-bend).
const R_REF: f64 = 4.55872841596;

/// Structured tet mesh of the box `[0,3]×[0,3]×[0,0.1]` with `n` cells
/// per unit width, keeping only the hexes of the unit-width L (outside
/// legs of length 3): those with `y ≤ 1` or `x ≤ 1`. One cell through the
/// thickness. Each kept hex splits into 6 right-handed tets sharing the
/// long diagonal (`cube_tet_mesh`'s pattern), so the tet counts match the
/// refinement table of issue #1495 (480 at n = 4, …, 30720 at n = 32).
fn l_bend_mesh(n: usize) -> (TetMesh, Vec<[u32; 3]>, Vec<[u32; 3]>) {
    let (nx, ny, nz) = (3 * n, 3 * n, 1usize);
    let hx = 3.0 / nx as f64;
    let hy = 3.0 / ny as f64;
    let hz = 0.1;
    let nps_x = nx + 1;
    let nps_y = ny + 1;
    let node_idx =
        |i: usize, j: usize, k: usize| -> u32 { (i + j * nps_x + k * nps_x * nps_y) as u32 };
    let all_nodes: Vec<[f64; 3]> = (0..=nz)
        .flat_map(|k| {
            (0..=ny).flat_map(move |j| {
                (0..=nx).map(move |i| [i as f64 * hx, j as f64 * hy, k as f64 * hz])
            })
        })
        .collect();

    let mut tets = Vec::new();
    let mut used = std::collections::BTreeSet::new();
    let mut keep = |c: [u32; 8]| {
        for v in c {
            used.insert(v);
        }
        tets.push([c[0], c[1], c[2], c[6]]);
        tets.push([c[0], c[2], c[3], c[6]]);
        tets.push([c[0], c[3], c[7], c[6]]);
        tets.push([c[0], c[7], c[4], c[6]]);
        tets.push([c[0], c[4], c[5], c[6]]);
        tets.push([c[0], c[5], c[1], c[6]]);
    };
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                // Hex centroid: keep the L, drop the void quadrant.
                let (xc, yc) = ((i as f64 + 0.5) * hx, (j as f64 + 0.5) * hy);
                if !(yc <= 1.0 || xc <= 1.0) {
                    continue;
                }
                let c = [
                    node_idx(i, j, k),
                    node_idx(i + 1, j, k),
                    node_idx(i + 1, j + 1, k),
                    node_idx(i, j + 1, k),
                    node_idx(i, j, k + 1),
                    node_idx(i + 1, j, k + 1),
                    node_idx(i + 1, j + 1, k + 1),
                    node_idx(i, j + 1, k + 1),
                ];
                keep(c);
            }
        }
    }
    // Drop unused nodes.
    let used: Vec<u32> = used.into_iter().collect();
    let mut remap = vec![u32::MAX; all_nodes.len()];
    let nodes: Vec<[f64; 3]> = used
        .iter()
        .enumerate()
        .map(|(new, &old)| {
            remap[old as usize] = new as u32;
            all_nodes[old as usize]
        })
        .collect();
    let tets: Vec<[u32; 4]> = tets
        .iter()
        .map(|t| {
            [
                remap[t[0] as usize],
                remap[t[1] as usize],
                remap[t[2] as usize],
                remap[t[3] as usize],
            ]
        })
        .collect();
    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: std::collections::BTreeMap::new(),
    };

    // End contacts: the faces at x = 3 (`right_end`) and y = 3 (`top_end`).
    let at = |f: &[u32; 3], axis: usize, v: f64| {
        f.iter()
            .all(|&n| (mesh.nodes[n as usize][axis] - v).abs() < 1e-12)
    };
    let right_end: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| at(f, 0, 3.0))
        .collect();
    let top_end: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| at(f, 1, 3.0))
        .collect();
    assert!(!right_end.is_empty() && !top_end.is_empty());
    (mesh, right_end, top_end)
}

/// Write the L-bend mesh + spec; returns the spec path.
fn write_l_spec(dir: &Path, n: usize) -> std::path::PathBuf {
    let (mesh, right_end, top_end) = l_bend_mesh(n);
    let text = msh::write_msh_volumes(
        &mesh.nodes,
        &[(1, "L", mesh.tets.as_slice())],
        &[
            (11, "right_end", right_end.as_slice()),
            (12, "top_end", top_end.as_slice()),
        ],
    );
    let mesh_path = dir.join(format!("l_bend_{n}.msh"));
    std::fs::write(&mesh_path, text).unwrap();
    let spec = json!({
        "schema_version": 1,
        "mesh": {"path": mesh_path.to_str().unwrap(), "length_unit_m": 1e-3},
        "materials": [{"physical_group": "L", "conductivity_s_m": 10.0}],
        "conduction": {"terminals": ["right_end", "top_end"]}
    });
    let path = dir.join(format!("l_bend_{n}.json"));
    std::fs::write(&path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();
    path
}

fn run_l(dir: &Path, n: usize, window: &[&str]) -> Value {
    let spec = write_l_spec(dir, n);
    let args: Vec<&str> = ["conduction", spec.to_str().unwrap()]
        .into_iter()
        .chain(window.iter().copied())
        .collect();
    let out = geode(&args);
    let v: Value = serde_json::from_slice(&out.stdout).expect("JSON report");
    assert!(out.status.success(), "n={n}: {}", v["error"]["message"]);
    v
}

fn rel(a: f64, b: f64) -> f64 {
    (a - b).abs() / b.abs()
}

/// The four windowed metrics of a `windowed_peaks` report, by metric name.
fn metric<'a>(r: &'a Value, name: &str) -> &'a Value {
    r["windowed_peaks"]["peaks"]
        .as_array()
        .expect("peaks array")
        .iter()
        .find(|p| p["metric"] == name)
        .unwrap_or_else(|| panic!("no {name} in peaks"))
}

#[test]
fn l_bend_windowed_peaks_converge_pointwise_max_grows() {
    let dir = Scratch::new("geode-", "conduction-bend");
    let levels = [4usize, 8, 16, 32];
    // Fixed physical window across refinements: 0.5 mm cube (half the
    // leg width), large enough to resolve the coarsest level (largest
    // element edge there is 0.25·√3 ≈ 0.433 < 0.5).
    let window = [
        "--window-edge-m",
        "0.0005",
        "--j-ref-a-m2",
        "5.0",
        "--joule-ref-w-m3",
        "25.0",
    ];

    let names = [
        "vector_mean",
        "magnitude_mean",
        "magnitude_rms",
        "joule_mean",
    ];
    let mut series: Vec<[f64; 4]> = Vec::new();
    let mut pointwise: Vec<f64> = Vec::new();

    println!(
        "{:>4} {:>12} {:>12} {:>12} {:>12} {:>12} {:>10}",
        "n", "|<J>|", "<|J|>", "RMS", "<q>", "max|J|", "R/R_sheet"
    );
    for &n in &levels {
        let r = run_l(&dir, n, &window);
        assert_eq!(r["kind"], "conduction");
        let wp = &r["windowed_peaks"];
        assert_eq!(wp["window_edge_m"], json!(5.0e-4));
        assert_eq!(wp["normalization"], "conductor_intersection");
        assert_eq!(wp["j_ref_a_m2"], json!(5.0));
        assert_eq!(wp["joule_ref_w_m3"], json!(25.0));
        let vals: [f64; 4] = std::array::from_fn(|k| {
            let p = metric(&r, names[k]);
            let v = p["value"].as_f64().unwrap();
            let expected_ratio = if k == 3 { v / 25.0 } else { v / 5.0 };
            assert!(
                rel(p["reference_ratio"].as_f64().unwrap(), expected_ratio) < 1e-12,
                "ratio of {}",
                names[k]
            );
            let cov = p["conducting_volume_fraction"].as_f64().unwrap();
            assert!((0.0..=1.0).contains(&cov), "coverage {cov} at n={n}");
            // 0.5 mm exceeds the largest element edge (h·√3) at every
            // level, so no winning window is under-sampled here; the
            // warning path itself is exercised below on the coarse level
            // with a sub-element window.
            assert!(
                !p["undersampled"].as_bool().unwrap(),
                "unexpected undersampled at n={n}: {}",
                names[k]
            );
            v
        });
        let max_j = r["max_current_density_a_m2"].as_f64().unwrap();
        let rrs = (1.0 / r["g_siemens"][0][0].as_f64().unwrap()) * 10.0 * 1.0e-4;
        println!(
            "{:>4} {:>12.6} {:>12.6} {:>12.6} {:>12.6} {:>10.4} {:>10.5}",
            n, vals[0], vals[1], vals[2], vals[3], max_j, rrs
        );
        // Geometry check against the closed-form reference (the filer's
        // table: 1.08% error at n = 4, 0.07% at n = 32).
        assert!(
            rel(rrs, R_REF) < (if n == 4 { 0.02 } else { 0.005 }),
            "n={n}: {rrs}"
        );
        series.push(vals);
        pointwise.push(max_j);
    }

    // The per-element maximum of |J| GROWS under refinement — the
    // mesh-dependent quantity must never be read as a converged peak.
    for w in pointwise.windows(2) {
        assert!(w[1] > w[0], "pointwise max must grow: {pointwise:?}");
    }

    // The windowed averages CONVERGE at fixed physical window size.
    // Documented per-step tolerances, ~2x headroom over the measured
    // worst cases (first step 4.8e-2 on the Joule mean, second 2.6e-2,
    // third 8.7e-3): 0.10 / 0.05 / 0.02 — each step must clear its
    // bound, and every metric shrinks below 2% relative change by the
    // last refinement.
    let tolerances = [0.10_f64, 0.05, 0.02];
    for (k, &name) in names.iter().enumerate() {
        for (step, w) in series.windows(2).enumerate() {
            let d = (w[1][k] - w[0][k]).abs() / w[0][k].abs();
            println!("n-step {name}: Δ = {d:.4e}");
            assert!(
                d < tolerances[step],
                "{name}: relative change {d:.4e} over tolerance {}",
                tolerances[step]
            );
        }
    }
    // Physics sanity on the mean inequalities: RMS ≥ <|J|> ≥ |<J>| for
    // every level.
    for v in &series {
        assert!(v[2] >= v[1] - 1e-9 && v[1] >= v[0] - 1e-9, "{v:?}");
    }

    // The under-sampling warning: at the coarse level a sub-element
    // window (0.1 mm < element edge 0.433) must flag every winning
    // window, while the 0.5 mm convergence window above does not.
    let r = run_l(
        &dir,
        4,
        &[
            "--window-edge-m",
            "0.0001",
            "--j-ref-a-m2",
            "5.0",
            "--joule-ref-w-m3",
            "25.0",
        ],
    );
    for p in r["windowed_peaks"]["peaks"].as_array().unwrap() {
        assert!(
            p["undersampled"].as_bool().unwrap(),
            "sub-element window must warn: {p}"
        );
    }
}

#[test]
fn uniform_bar_window_si_values_ratios_and_coverage() {
    let dir = Scratch::new("geode-", "conduction-bend-bar");
    // Uniform bar [0,4]×[0,1]×[0,2] mm; the conducting slab is only
    // y ∈ [0.25, 0.5] mm — everything else is an unlisted (σ = 0)
    // insulator. The linear P1 solution gives |J| = σV/L exactly, and no
    // window fits fully inside the 0.25 mm slab, so every coverage is
    // partial and conductor-based (a mesh-coverage convention would
    // report at least 0.75).
    let n = [16usize, 4, 8];
    let (nx, ny, nz) = (n[0], n[1], n[2]);
    let h = [4.0 / nx as f64, 1.0 / ny as f64, 2.0 / nz as f64];
    let nps = [nx + 1, ny + 1, nz + 1];
    let node_idx =
        |i: usize, j: usize, k: usize| -> u32 { (i + j * nps[0] + k * nps[0] * nps[1]) as u32 };
    let nodes: Vec<[f64; 3]> = (0..nps[2])
        .flat_map(|k| {
            (0..nps[1]).flat_map(move |j| {
                (0..nps[0]).map(move |i| [i as f64 * h[0], j as f64 * h[1], k as f64 * h[2]])
            })
        })
        .collect();
    let mut tets = Vec::new();
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let c = [
                    node_idx(i, j, k),
                    node_idx(i + 1, j, k),
                    node_idx(i + 1, j + 1, k),
                    node_idx(i, j + 1, k),
                    node_idx(i, j, k + 1),
                    node_idx(i + 1, j, k + 1),
                    node_idx(i + 1, j + 1, k + 1),
                    node_idx(i, j + 1, k + 1),
                ];
                tets.push([c[0], c[1], c[2], c[6]]);
                tets.push([c[0], c[2], c[3], c[6]]);
                tets.push([c[0], c[3], c[7], c[6]]);
                tets.push([c[0], c[7], c[4], c[6]]);
                tets.push([c[0], c[4], c[5], c[6]]);
                tets.push([c[0], c[5], c[1], c[6]]);
            }
        }
    }
    let mesh = TetMesh {
        nodes,
        tets,
        physical_groups: std::collections::BTreeMap::new(),
    };
    // Conducting slab: the tets with centroid y ∈ (0.25, 0.5). Everything
    // else becomes an unlisted region, i.e. insulating.
    let mut bar_tets = Vec::new();
    let mut void_tets = Vec::new();
    for t in &mesh.tets {
        let yc = t.iter().map(|&v| mesh.nodes[v as usize][1]).sum::<f64>() / 4.0;
        if (0.25..0.5).contains(&yc) {
            bar_tets.push(*t);
        } else {
            void_tets.push(*t);
        }
    }
    assert_eq!(bar_tets.len() + void_tets.len(), mesh.n_tets());
    let at = |f: &[u32; 3], x: f64| {
        f.iter()
            .all(|&n| (mesh.nodes[n as usize][0] - x).abs() < 1e-12)
    };
    let left: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| at(f, 0.0))
        .collect();
    let right: Vec<[u32; 3]> = mesh
        .boundary_faces()
        .into_iter()
        .filter(|f| at(f, 4.0))
        .collect();
    let text = msh::write_msh_volumes(
        &mesh.nodes,
        &[
            (1, "bar", bar_tets.as_slice()),
            (2, "void", void_tets.as_slice()),
        ],
        &[
            (11, "left", left.as_slice()),
            (12, "right", right.as_slice()),
        ],
    );
    let mesh_path = dir.join("bar_slab.msh");
    std::fs::write(&mesh_path, text).unwrap();
    // Only the slab is listed: every other region is insulating.
    let spec = json!({
        "schema_version": 1,
        "mesh": {"path": mesh_path.to_str().unwrap(), "length_unit_m": 1e-3},
        "materials": [{"physical_group": "bar", "conductivity_s_m": 5.8e7}],
        "conduction": {"terminals": ["left", "right"]}
    });
    let spec_path = dir.join("bar_slab.json");
    std::fs::write(&spec_path, serde_json::to_vec_pretty(&spec).unwrap()).unwrap();

    let sigma = 5.8e7_f64;
    let l_m = 4.0 * 1e-3;
    let j_si = sigma * 1.0 / l_m; // V = 1 across the bar
    let joule_si = sigma * (1.0 / l_m).powi(2);

    // Without the group the report has no windowed section.
    let out = geode(&["conduction", spec_path.to_str().unwrap()]);
    let plain: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(plain.get("windowed_peaks").is_none());

    // 0.5 mm window; the insulating surroundings distinguish conductor
    // coverage from mesh coverage.
    let out = geode(&[
        "conduction",
        spec_path.to_str().unwrap(),
        "--window-edge-m",
        "0.0005",
        "--j-ref-a-m2",
        "1.0e9",
        "--joule-ref-w-m3",
        "1.0e12",
    ]);
    let r: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(out.status.success(), "{}", r["error"]["message"]);
    let wp = &r["windowed_peaks"];
    assert_eq!(wp["window_edge_m"], json!(5.0e-4));
    assert_eq!(wp["normalization"], "conductor_intersection");

    let names = ["vector_mean", "magnitude_mean", "magnitude_rms"];
    for name in names {
        let p = metric(&r, name);
        let v = p["value"].as_f64().unwrap();
        assert!(rel(v, j_si) < 1e-9, "{name}: {v} vs {j_si}");
        assert!(rel(p["reference_ratio"].as_f64().unwrap(), j_si / 1.0e9) < 1e-9);
    }
    let q = metric(&r, "joule_mean");
    assert!(rel(q["value"].as_f64().unwrap(), joule_si) < 1e-9);
    assert!(rel(q["reference_ratio"].as_f64().unwrap(), joule_si / 1.0e12) < 1e-9);

    // Partial coverage, conductor-intersection normalization: every
    // window straddles the insulator (the 0.25 mm slab is thinner than
    // the 0.5 mm window, so the conducting y-fraction is exactly 0.5
    // while x/z clipping costs up to 0.75²), giving a conducting fraction
    // in [0.328, 0.5]. A mesh-coverage convention — counting insulator
    // overlap as covered — would report ≥ 0.5625 for every window: the
    // assert below separates the two conventions whichever window wins
    // the round-off tie.
    for p in wp["peaks"].as_array().unwrap() {
        let cov = p["conducting_volume_fraction"].as_f64().unwrap();
        assert!(cov > 0.3 && cov < 0.53, "coverage {cov}: {p}");
    }
    // The winning windows sit on the conductor (J averages exact only if
    // the overlap is purely conducting material).
    assert!(
        !metric(&r, "vector_mean")["undersampled"].as_bool().unwrap(),
        "0.5 mm window resolves the uniform grid"
    );
}

#[test]
fn window_group_args_are_all_or_none_and_validated() {
    let dir = Scratch::new("geode-", "conduction-bend-args");
    let spec = write_l_spec(&dir, 4);

    // A lone reference is rejected at parse time.
    let out = geode(&["conduction", spec.to_str().unwrap(), "--j-ref-a-m2", "5.0"]);
    assert!(!out.status.success(), "partial group must fail");
    let err = String::from_utf8_lossy(&out.stderr);
    assert!(err.contains("--window-edge-m"), "{err}");

    // Non-positive / non-finite values are rejected by the value parser
    // (a leading-minus value is rejected by clap itself as an unknown
    // flag — either way the run fails).
    for bad in ["0.0", "-1.0", "nan", "inf"] {
        let out = geode(&[
            "conduction",
            spec.to_str().unwrap(),
            "--window-edge-m",
            bad,
            "--j-ref-a-m2",
            "5.0",
            "--joule-ref-w-m3",
            "25.0",
        ]);
        assert!(!out.status.success(), "edge {bad} must fail");
        let err = String::from_utf8_lossy(&out.stderr);
        assert!(
            err.contains("finite and > 0") || err.contains("unexpected argument"),
            "{bad}: {err}"
        );
    }
}
