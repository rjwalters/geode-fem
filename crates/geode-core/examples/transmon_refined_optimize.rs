//! Refine, optimize, then validate the SAME geometry on another discretization.
//! The initial coarse harmonic geometry map is prolonged exactly between meshes;
//! refinement is outside differentiation. A discrete target hit is not convergence.
//! Run: cargo run -p geode-core --release --example transmon_refined_optimize
//! Environment: REFINED_OPT_CYCLES (default 8), REFINED_OPT_MAX_TETS (2 million),
//! REFINED_OPT_ORDER (1 or 2), REFINED_OPT_DIR (output directory). No reference benchmark is overwritten.

use geode_core::adapt::refine::{BisectionMesh, RefineOpts};
use geode_core::assembly::electrostatic::{
    AmgPcgOptions, Electrode, assemble_electrostatic, assemble_electrostatic_p2_sheets,
    extract_capacitance_amg_pcg_fields, extract_capacitance_p2_amg_pcg_fields, tet_bary_grads,
};
use geode_core::mesh::{
    MetalRole, TaggedTetMesh, TetMesh, TransmonFixture, read_transmon_smoke_fixture,
};
use geode_core::shape::min_tet_volume_ratio;
use geode_core::shape::refined_capacitance::{
    p1_energy_shape_derivatives, p2_energy_shape_derivatives,
};
use geode_core::shape::transmon_morph::{TransmonMorphRoles, TransmonMorphSpec, parameter_fields};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Instant;

const TARGET: f64 = 89.9;
const SCALE: [f64; 3] = [0.040959, 0.553015, 0.784636];

fn moved(base: &TetMesh, fields: &[Vec<[f64; 3]>], theta: &[f64; 3]) -> TetMesh {
    let mut mesh = base.clone();
    for (i, x) in mesh.nodes.iter_mut().enumerate() {
        for (field, step) in fields.iter().zip(theta) {
            for (d, xd) in x.iter_mut().enumerate() {
                *xd += step * field[i][d];
            }
        }
    }
    mesh
}
fn fixture(tagged: &TaggedTetMesh, original: &TransmonFixture) -> TransmonFixture {
    TransmonFixture {
        mesh: tagged.mesh.clone(),
        tags: original.tags,
        tet_physical_tags: tagged.tet_physical_tags.clone(),
        boundary_triangles: tagged.boundary_triangles.clone(),
        triangle_physical_tags: tagged.triangle_physical_tags.clone(),
    }
}
fn electrodes(fx: &TransmonFixture) -> (Vec<Electrode>, Vec<u32>) {
    let components = fx.split_metal_conductors();
    let nodes = |r| {
        components
            .iter()
            .find(|c| c.role == r)
            .unwrap()
            .nodes
            .clone()
    };
    (
        vec![
            Electrode {
                name: "island".into(),
                nodes: nodes(MetalRole::Island),
                voltage: 1.0,
            },
            Electrode {
                name: "feedline".into(),
                nodes: nodes(MetalRole::Feedline),
                voltage: 0.0,
            },
        ],
        nodes(MetalRole::Ground),
    )
}
struct Solve {
    c: f64,
    gradient: Vec<f64>,
    phi: Vec<f64>,
    elapsed: f64,
    true_residual: f64,
    iterations: Vec<usize>,
}
fn solve(fx: &TransmonFixture, fields: &[Vec<[f64; 3]>]) -> Solve {
    let order = std::env::var("REFINED_OPT_ORDER")
        .unwrap_or("1".into())
        .parse()
        .unwrap();
    solve_with_order(fx, fields, order)
}
fn solve_with_order(fx: &TransmonFixture, fields: &[Vec<[f64; 3]>], order: usize) -> Solve {
    let start = Instant::now();
    let (conductors, ground) = electrodes(fx);
    let eps = fx.epsilon_r_scalar();
    let rho = vec![0.0; fx.mesh.n_tets()];
    let (full, report, phis) = if order == 1 {
        let sys = assemble_electrostatic(&fx.mesh, &eps, &rho, &conductors, &ground).unwrap();
        extract_capacitance_amg_pcg_fields(&sys, &conductors, &ground, AmgPcgOptions::default())
            .unwrap()
    } else {
        assert_eq!(order, 2);
        let components = fx.split_metal_conductors();
        let tris = |role| {
            components
                .iter()
                .find(|c| c.role == role)
                .unwrap()
                .triangles
                .as_slice()
        };
        let (sys, dofs) = assemble_electrostatic_p2_sheets(
            &fx.mesh,
            &eps,
            &rho,
            &conductors,
            &[tris(MetalRole::Island), tris(MetalRole::Feedline)],
            &ground,
            tris(MetalRole::Ground),
        )
        .unwrap();
        extract_capacitance_p2_amg_pcg_fields(&sys, &dofs, AmgPcgOptions::default()).unwrap()
    };
    assert!(report.rel_residual.iter().all(|r| *r <= 1e-9));
    let vf = -full.c[1][0] / full.c[1][1];
    let phi: Vec<_> = phis[0]
        .iter()
        .zip(&phis[1])
        .map(|(a, b)| a + vf * b)
        .collect();
    let c = full.c[0][0] + vf * full.c[0][1];
    let gradient = if order == 1 {
        p1_energy_shape_derivatives(&fx.mesh, &eps, &phi, fields)
    } else {
        p2_energy_shape_derivatives(&fx.mesh, &eps, &phi, fields)
    }
    .unwrap()
    .iter()
    .map(|v| v * 1e15)
    .collect();
    let phi = phi[..fx.mesh.n_nodes()].to_vec();
    Solve {
        c: c * 1e15,
        gradient,
        phi,
        elapsed: start.elapsed().as_secs_f64(),
        true_residual: report.rel_residual.iter().copied().fold(0.0, f64::max),
        iterations: report.iterations,
    }
}

// Flux-jump marking, excluding the actual conductor sheets where a normal
// displacement jump is physical. This is a heuristic indicator; the acceptance
// test is a separate solve on a more resolved discretization, not its value.
fn indicators(fx: &TransmonFixture, phi: &[f64]) -> Vec<f64> {
    let mesh = &fx.mesh;
    let eps = fx.epsilon_r_scalar();
    let mut pec = BTreeSet::new();
    for mut tri in fx.metal_triangles() {
        tri.sort_unstable();
        pec.insert(tri);
    }
    let mut faces: HashMap<[u32; 3], (usize, [f64; 3], f64)> = HashMap::new();
    let mut eta = vec![0.0; mesh.n_tets()];
    for (t, tet) in mesh.tets.iter().enumerate() {
        let coords = tet.map(|v| mesh.nodes[v as usize]);
        let grads = tet_bary_grads(&coords);
        let mut flux = [0.0; 3];
        for a in 0..4 {
            for (d, f) in flux.iter_mut().enumerate() {
                *f += eps[t] * phi[tet[a] as usize] * grads[a][d];
            }
        }
        for local in [[1, 2, 3], [0, 2, 3], [0, 1, 3], [0, 1, 2]] {
            let mut key = local.map(|a| tet[a]);
            key.sort_unstable();
            if pec.contains(&key) {
                continue;
            }
            let p = key.map(|v| mesh.nodes[v as usize]);
            let a: [f64; 3] = std::array::from_fn(|d| p[1][d] - p[0][d]);
            let b: [f64; 3] = std::array::from_fn(|d| p[2][d] - p[0][d]);
            let n = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            if let Some((other, f, e)) = faces.remove(&key) {
                let jump: f64 = (0..3).map(|d| (flux[d] - f[d]) * n[d]).sum();
                let area2 = n.iter().map(|x| x * x).sum::<f64>().sqrt();
                let value = jump * jump / area2.sqrt() / eps[t].max(e);
                eta[t] += value;
                eta[other] += value;
            } else {
                faces.insert(key, (t, flux, eps[t]));
            }
        }
    }
    eta
}
fn save(path: &PathBuf, history: &[Value], status: &str, theta: &[f64; 3]) {
    fs::write(
        path,
        serde_json::to_string_pretty(&json!({"target_scalar_capacitance_ff":TARGET,"optimization_order":std::env::var("REFINED_OPT_ORDER").unwrap_or("1".into()),
        "geometry_map":"fixed coarse junction-pinned harmonic fields; exact nested P1 prolongation",
        "convergence_threshold_relative":0.01,"status":status,"theta":theta,"history":history}))
        .unwrap(),
    )
    .unwrap();
}

// The small coarse mesh carries exactly the same polygonal final geometry.
// Palace can change polynomial order/refinement independently of our optimizer.
fn export_palace(fx: &TransmonFixture, out: &std::path::Path, mesh_name: &str) {
    let components = fx.split_metal_conductors();
    let mut sheet_tag = HashMap::new();
    for comp in components {
        let tag = match comp.role {
            MetalRole::Ground => 101,
            MetalRole::Island => 102,
            MetalRole::Feedline => 103,
        };
        for mut tri in comp.triangles {
            tri.sort_unstable();
            sheet_tag.insert(tri, tag);
        }
    }
    let mut f = std::io::BufWriter::new(fs::File::create(out.join(mesh_name)).unwrap());
    writeln!(
        f,
        "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n{}",
        fx.mesh.n_nodes()
    )
    .unwrap();
    for (i, x) in fx.mesh.nodes.iter().enumerate() {
        writeln!(f, "{} {:.17e} {:.17e} {:.17e}", i + 1, x[0], x[1], x[2]).unwrap();
    }
    writeln!(
        f,
        "$EndNodes\n$Elements\n{}",
        fx.mesh.n_tets() + fx.boundary_triangles.len()
    )
    .unwrap();
    let mut id = 1;
    for (tri, &tag) in fx.boundary_triangles.iter().zip(&fx.triangle_physical_tags) {
        let mut key = *tri;
        key.sort_unstable();
        let tag = sheet_tag.get(&key).copied().unwrap_or(tag);
        writeln!(
            f,
            "{id} 2 2 {tag} {tag} {} {} {}",
            tri[0] + 1,
            tri[1] + 1,
            tri[2] + 1
        )
        .unwrap();
        id += 1;
    }
    for (tet, &tag) in fx.mesh.tets.iter().zip(&fx.tet_physical_tags) {
        writeln!(
            f,
            "{id} 4 2 {tag} {tag} {} {} {} {}",
            tet[0] + 1,
            tet[1] + 1,
            tet[2] + 1,
            tet[3] + 1
        )
        .unwrap();
        id += 1;
    }
    writeln!(f, "$EndElements").unwrap();
    f.flush().unwrap();
    for order in 1..=3 {
        let config = json!({"Problem":{"Type":"Electrostatic","Output":format!("palace_p{order}"),"Verbose":1},
            "Model":{"Mesh":mesh_name,"L0":1.0},
            "Domains":{"Materials":[{"Attributes":[fx.tags.substrate],"Permittivity":(9.3+9.3+11.5)/3.0},{"Attributes":[fx.tags.vacuum],"Permittivity":1.0}]},
            "Boundaries":{"Ground":{"Attributes":[101]},"Terminal":[{"Index":1,"Attributes":[102]},{"Index":2,"Attributes":[103]}]},
            "Solver":{"Order":order,"Electrostatic":{"Save":0},"Linear":{"Type":"BoomerAMG","Tol":1e-10,"MaxIts":2000}}});
        fs::write(
            out.join(format!("palace_p{order}.json")),
            serde_json::to_string_pretty(&config).unwrap(),
        )
        .unwrap();
    }
}
fn main() {
    let cycles: usize = std::env::var("REFINED_OPT_CYCLES")
        .unwrap_or("8".into())
        .parse()
        .unwrap();
    let max_tets: usize = std::env::var("REFINED_OPT_MAX_TETS")
        .unwrap_or("2000000".into())
        .parse()
        .unwrap();
    let out = PathBuf::from(
        std::env::var("REFINED_OPT_DIR")
            .unwrap_or("benchmarks/transmon_diffopt/refined_optimization".into()),
    );
    fs::create_dir_all(&out).unwrap();
    let mut fx = read_transmon_smoke_fixture().unwrap();
    for p in &mut fx.mesh.nodes {
        for x in p {
            *x *= 1e-6;
        }
    }
    let roles =
        TransmonMorphRoles::identify(&fx, &fx.mesh, TransmonMorphSpec::fixture_default(1e-6))
            .unwrap();
    let mut fields = parameter_fields(&fx.mesh, &roles).unwrap().to_vec();
    let coarse_fields = fields.clone();
    let tagged = TaggedTetMesh {
        mesh: fx.mesh.clone(),
        tet_physical_tags: fx.tet_physical_tags.clone(),
        boundary_triangles: fx.boundary_triangles.clone(),
        triangle_physical_tags: fx.triangle_physical_tags.clone(),
    };
    let mut state = BisectionMesh::new(tagged).unwrap();
    let mut history = Vec::new();
    let mut theta = [0.0; 3];
    let mut previous: Option<f64> = None;
    let result_path = out.join("results.json");
    for cycle in 0..=cycles {
        let mut current = fixture(state.mesh(), &fx);
        current.mesh = moved(&state.mesh().mesh, &fields, &theta);
        let solved = solve_with_order(&current, &fields, 1);
        let shift = previous.map(|c| (solved.c - c) / c);
        println!(
            "cycle {cycle}: {} tets, C={:.9} fF, gradient={:?}, shift={shift:?}, {:.1}s",
            current.mesh.n_tets(),
            solved.c,
            solved.gradient,
            solved.elapsed
        );
        history.push(json!({"kind":"refinement","cycle":cycle,"n_tets":current.mesh.n_tets(),"n_nodes":current.mesh.n_nodes(),"theta":theta,"c_ff":solved.c,"gradient_ff":solved.gradient,"relative_shift":shift,"solve_s":solved.elapsed,"true_relative_residual":solved.true_residual,"iterations":solved.iterations}));
        save(&result_path, &history, "running", &theta);
        if cycle == cycles {
            break;
        }
        let eta = indicators(&current, &solved.phi);
        let mut order: Vec<usize> = (0..eta.len()).collect();
        order.sort_by(|&a, &b| eta[b].total_cmp(&eta[a]));
        let threshold = 0.5 * eta.iter().sum::<f64>();
        let mut sum = 0.0;
        let mut marked = Vec::new();
        for t in order {
            marked.push(t);
            sum += eta[t];
            if sum >= threshold {
                break;
            }
        }
        let mut trial_state = state.clone();
        let refined = trial_state.refine(&marked, &RefineOpts::default()).unwrap();
        println!(
            "refined {} marked -> {} tets",
            marked.len(),
            refined.mesh.mesh.n_tets()
        );
        if refined.mesh.mesh.n_tets() > max_tets {
            save(
                &result_path,
                &history,
                "refinement_budget_exceeded_before_next_solve",
                &theta,
            );
            break;
        }
        state = trial_state;
        let p = refined.node_prolongation.as_ref();
        fields = fields
            .iter()
            .map(|field| {
                let mut fine = vec![[0.0; 3]; p.nrows()];
                for (col, value) in field.iter().enumerate() {
                    for q in p.col_ptr()[col]..p.col_ptr()[col + 1] {
                        let row = p.row_idx()[q];
                        for (d, &v) in value.iter().enumerate() {
                            fine[row][d] += p.val()[q] * v;
                        }
                    }
                }
                fine
            })
            .collect();
        previous = Some(solved.c);
    }
    // Optimization is deliberately after the resolution study, on a fixed mesh.
    // A separate validation pass below must still establish design stability.
    let mut last = None;
    let mut cached = None;
    for step in 0..20 {
        let mut current = fixture(state.mesh(), &fx);
        current.mesh = moved(&state.mesh().mesh, &fields, &theta);
        let s = cached.take().unwrap_or_else(|| solve(&current, &fields));
        let ratio = min_tet_volume_ratio(&state.mesh().mesh, &current.mesh);
        println!(
            "opt {step}: C={:.9}, theta={theta:?}, ratio={ratio:.5}",
            s.c
        );
        history.push(json!({"kind":"optimization","step":step,"c_ff":s.c,"gradient_ff":s.gradient,"theta":theta,"min_volume_ratio":ratio,"solve_s":s.elapsed,"true_relative_residual":s.true_residual,"iterations":s.iterations}));
        save(&result_path, &history, "optimizing", &theta);
        if (s.c - TARGET).abs() < 0.005 {
            last = Some(s);
            break;
        }
        let norm: f64 = s
            .gradient
            .iter()
            .zip(SCALE)
            .map(|(g, w)| (g * w).powi(2))
            .sum();
        let mut delta: [f64; 3] =
            std::array::from_fn(|i| (TARGET - s.c) * s.gradient[i] * SCALE[i].powi(2) / norm);
        let trust = delta
            .iter()
            .zip(SCALE)
            .map(|(d, w)| (d / w).abs())
            .fold(0.0_f64, f64::max);
        if trust > 0.3 {
            for d in &mut delta {
                *d *= 0.3 / trust;
            }
        }
        let mut accepted = false;
        for _ in 0..14 {
            let trial = std::array::from_fn(|i| theta[i] + delta[i]);
            let m = moved(&state.mesh().mesh, &fields, &trial);
            if min_tet_volume_ratio(&state.mesh().mesh, &m) >= 0.25 {
                let mut candidate = current.clone();
                candidate.mesh = m;
                let t = solve(&candidate, &fields);
                if (t.c - TARGET).abs() < (s.c - TARGET).abs() {
                    theta = trial;
                    cached = Some(t);
                    accepted = true;
                    break;
                }
            }
            for d in &mut delta {
                *d *= 0.5;
            }
        }
        if !accepted {
            break;
        }
    }

    let mut export_fx = fx.clone();
    export_fx.mesh = moved(&fx.mesh, &coarse_fields, &theta);
    export_palace(&export_fx, &out, "final_coarse.msh");
    let coarse_check = solve(&export_fx, &coarse_fields);
    history.push(json!({"kind":"final_coarse_cross_solver_reference","c_ff":coarse_check.c,"gradient_ff":coarse_check.gradient}));
    if let Some(last) = last {
        let mut final_fx = fixture(state.mesh(), &fx);
        final_fx.mesh = moved(&state.mesh().mesh, &fields, &theta);
        let adaptive_export = out.join("adaptive");
        fs::create_dir_all(&adaptive_export).unwrap();
        export_palace(&final_fx, &adaptive_export, "final_coarse.msh");
        // FD uses fully re-solved perturbed fields, not a frozen-field energy.
        for i in 0..3 {
            let h = 1e-4;
            let mut tp = theta;
            tp[i] += h;
            let mut tm = theta;
            tm[i] -= h;
            let mut plus = final_fx.clone();
            plus.mesh = moved(&state.mesh().mesh, &fields, &tp);
            let mut minus = final_fx.clone();
            minus.mesh = moved(&state.mesh().mesh, &fields, &tm);
            let fd = (solve(&plus, &fields).c - solve(&minus, &fields).c) / (2.0 * h);
            let rel = (fd - last.gradient[i]).abs() / fd.abs().max(1e-12);
            assert!(rel < 1e-3, "final FD parameter {i}: {rel}");
            history.push(json!({"kind":"final_gradient_check","parameter":i,"h":h,"analytic_ff":last.gradient[i],"fd_ff":fd,"relative_error":rel}));
            save(&result_path, &history, "validating", &theta);
        }
        // Refine the final geometry independently of optimization. The
        // validation solves never update theta and do not feed its target.
        let eta = indicators(&final_fx, &last.phi);
        let mut ranked: Vec<usize> = (0..eta.len()).collect();
        ranked.sort_by(|&a, &b| eta[b].total_cmp(&eta[a]));
        let mut marked = Vec::new();
        let mut total = 0.0;
        let marking_threshold = 0.5 * eta.iter().sum::<f64>();
        for t in ranked {
            marked.push(t);
            total += eta[t];
            if total >= marking_threshold {
                break;
            }
        }
        let refined = state.refine(&marked, &RefineOpts::default()).unwrap();
        let p = refined.node_prolongation.as_ref();
        let fine_fields: Vec<Vec<[f64; 3]>> = fields
            .iter()
            .map(|field| {
                let mut fine = vec![[0.0; 3]; p.nrows()];
                for (col, value) in field.iter().enumerate() {
                    for q in p.col_ptr()[col]..p.col_ptr()[col + 1] {
                        for (d, &v) in value.iter().enumerate() {
                            fine[p.row_idx()[q]][d] += p.val()[q] * v;
                        }
                    }
                }
                fine
            })
            .collect();
        let mut fine_fx = fixture(&refined.mesh, &fx);
        fine_fx.mesh = moved(&refined.mesh.mesh, &fine_fields, &theta);
        let fine = solve(&fine_fx, &fine_fields);
        let change = (fine.c - last.c) / last.c;
        history.push(json!({"kind":"held_out_refinement","n_tets":fine_fx.mesh.n_tets(),"c_ff":fine.c,"relative_shift":change,"target_relative_error":(fine.c-TARGET)/TARGET,"gradient_ff":fine.gradient,"solve_s":fine.elapsed,"true_relative_residual":fine.true_residual,"iterations":fine.iterations}));
        println!(
            "held-out: {} tets C={:.9}, shift={:.4}%",
            fine_fx.mesh.n_tets(),
            fine.c,
            100.0 * change
        );
        save(
            &result_path,
            &history,
            if change.abs() <= 0.01 && (fine.c - TARGET).abs() / TARGET <= 0.01 {
                "one_local_refinement_passes_1pct_independent_validation_pending"
            } else {
                "discrete_target_reached_refinement_validation_failed"
            },
            &theta,
        );
    } else {
        save(&result_path, &history, "optimization_stalled", &theta);
    }
}
