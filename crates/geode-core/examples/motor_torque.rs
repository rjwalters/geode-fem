//! Locked-rotor **torque-vs-angle** capstone for the slotless surface-PM
//! machine (Epic #448, Phase 3b): sweep the rotor angle `θ_r`, solve the
//! driven magnetostatic field, extract the interaction torque with both the
//! Arkkio (preferred) and Maxwell-line estimators, and compare against the
//! exact closed-form [`SlotlessPmDriven::torque`] oracle.
//!
//! Run with:
//!
//! ```text
//!   cargo run -p geode-core --example motor_torque --release
//! ```
//!
//! Emits:
//!   * `benchmarks/motor/results.toml` — the committed benchmark fixture
//!     (`[meta]` + `[point_N]` tables: `theta_r, T_analytic, T_arkkio,
//!     T_line, rel_err_arkkio` per angle), consumed by
//!     `tools/viz/geode_viz/plots/motor.py` for the tearsheet overlay and by
//!     the README. Regenerate after any intentional change.
//!   * `benchmarks/motor/motor_torque.csv` — the same sweep as plain CSV
//!     (convenience for spreadsheet / quick plots).
//!   * `artifacts/viz/motor/motor_field.vtu` — the θ_r = 0 cross-section with
//!     the nodal `A_z` scalar and the per-cell `B` glyph vector, for a
//!     ParaView render (the tearsheet cross-section visual).
//!
//! Override the output roots with `$MOTOR_BENCH_DIR` / `$MOTOR_OUT_DIR`.
//!
//! The printed fine-mesh Arkkio L2 is the pinned benchmark value that the
//! CI-fast `tests/motor_torque_benchmark.rs` guards a coarser version of.

use std::fmt::Write as _;
use std::fs;
use std::io::Write as _;
use std::path::PathBuf;

use geode_core::analytic::slotless_pm::{MU_0, SlotlessPm, SlotlessPmDriven};
use geode_core::analytic::waveguide::{
    RadialGrading, TriMesh, disk_boundary_nodes, disk_tri_mesh_bands,
};
use geode_core::assembly::magnetostatic::{
    assemble_magnetostatic_pm, build_nu_r, radial_magnetization_source_rotated, recover_b_field,
    stator_winding_current,
};
use geode_core::assembly::torque::{arkkio_torque, maxwell_stress_torque};

const TAG_MAGNET: i32 = 1;
const TAG_GAP: i32 = 2;
const TAG_WIND: i32 = 3;

fn main() {
    // Five-band cross-section: bore / magnet / air-gap / stator winding /
    // outer air.  μ_r = 1 everywhere (open-space oracle regime).
    let (r1, r2, r3, r4, rout) = (0.030, 0.040, 0.045, 0.055, 0.20);
    let p = 2u32;
    let m0 = 1.2 / MU_0; // NdFeB remanence 1.2 T
    let j0 = 4.0e7; // stator peak current density (A/m²)
    let l = 0.05; // axial stack length (m)
    let radii = [0.0, r1, r2, r3, r4, rout];
    let n_rad = [18usize, 20, 16, 18, 26];
    let gradings = [RadialGrading::Uniform; 5];

    let (mesh, tags) = disk_tri_mesh_bands(&radii, 384, &n_rad, &gradings);
    println!(
        "driven slotless-PM cross-section: {} nodes, {} triangles",
        mesh.n_nodes(),
        mesh.n_tris()
    );

    let nu = build_nu_r(&tags, &[1.0; 5]);
    let bc = disk_boundary_nodes(&mesh, rout);
    let jz = stator_winding_current(&mesh, &tags, TAG_WIND, j0, p);
    let oracle = SlotlessPmDriven::new(SlotlessPm::new(r1, r2, m0, p), r3, r4, j0, l);
    let r_gap = 0.5 * (r2 + r3);
    let amp = oracle.torque_amplitude();

    // ── θ_r sweep over one electrical period (p θ_r ∈ [0, 2π)) ──────────
    let n_theta = 24usize;
    let mut rows: Vec<(f64, f64, f64, f64)> = Vec::with_capacity(n_theta + 1);
    let mut num_a = 0.0;
    let mut num_m = 0.0;
    let mut den = 0.0;
    let mut max_rel = 0.0_f64;
    let mut b_at_zero: Vec<[f64; 2]> = Vec::new();
    let mut a_at_zero: Vec<f64> = Vec::new();
    for k in 0..=n_theta {
        let theta_r = std::f64::consts::TAU * k as f64 / (n_theta as f64 * p as f64);
        let m = radial_magnetization_source_rotated(&mesh, &tags, TAG_MAGNET, m0, p, theta_r);
        let sys = assemble_magnetostatic_pm(&mesh, &nu, &jz, &m, &bc).expect("assemble");
        let a_z = sys.solve().expect("solve");
        let b = recover_b_field(&mesh, &a_z);
        let t_ark = arkkio_torque(&mesh, &tags, &b, TAG_GAP, r2, r3, l);
        let t_line = maxwell_stress_torque(&mesh, &b, r_gap, l, 180);
        let t_exact = oracle.torque(theta_r);
        rows.push((theta_r, t_exact, t_ark, t_line));
        if k < n_theta {
            num_a += (t_ark - t_exact).powi(2);
            num_m += (t_line - t_exact).powi(2);
            den += t_exact.powi(2);
            max_rel = max_rel.max((t_ark - t_exact).abs() / amp);
        }
        if k == 0 {
            b_at_zero = b;
            a_at_zero = a_z;
        }
    }
    let l2_a = (num_a / den).sqrt();
    let l2_m = (num_m / den).sqrt();

    println!("torque amplitude |T|_max = {amp:.4} N·m/m  ({p}-pole-pair, L = {l} m)");
    println!("locked-rotor T(θ_r) sweep ({n_theta} angles over one electrical period):");
    println!("  Arkkio (preferred) L2 vs analytic = {:.3}%", l2_a * 100.0);
    println!(
        "  Maxwell line-integral L2 vs analytic = {:.3}%",
        l2_m * 100.0
    );
    println!("  max |ΔT_arkkio| / |T|_max = {:.3}%", max_rel * 100.0);
    if l2_a <= 0.02 {
        println!("  ✓ meets the ≤2% target (Epic #448 AC #2 stretch goal)");
    } else if l2_a <= 0.05 {
        println!("  ✓ meets the ≤5% capstone bar (≤2% is the P2-Lagrange target)");
    } else {
        println!("  honest miss: exceeds the 5% bar — see convergence data / P2 follow-on");
    }

    // ── Emit benchmark TOML + CSV + VTU ────────────────────────────────
    let bench_dir = std::env::var("MOTOR_BENCH_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("benchmarks/motor"));
    fs::create_dir_all(&bench_dir).expect("create benchmark dir");

    // Committed benchmark fixture (mirrors the mie_sphere/spiral TOML shape:
    // a [meta] block + one [point_N] table per swept angle).
    let mut toml = String::with_capacity(1024 + rows.len() * 200);
    toml.push_str("# Auto-generated by `cargo run -p geode-core --release \\\n");
    toml.push_str("#   --example motor_torque`.\n");
    toml.push_str("# Do NOT edit by hand — regenerate after any intentional change.\n");
    toml.push_str(
        "# Consumed by `tests/motor_torque_benchmark.rs` (tolerances), \
         `tools/viz/geode_viz/plots/motor.py`, and the README.\n\n",
    );
    toml.push_str("[meta]\n");
    let _ = writeln!(
        toml,
        "description = \"Locked-rotor torque-vs-angle capstone (Epic #448 P3b): \
         a slotless surface-PM rotor driven by a theta-distributed stator winding \
         current sheet J_z = J0 cos(p*theta), Arkkio + Maxwell-line torque vs the \
         exact PM-vs-stator interaction-torque oracle T(theta_r) = \
         -(2*pi*L/mu0)*C_M*G_S*cos(p*theta_r).\""
    );
    let _ = writeln!(toml, "pole_pairs = {p}");
    let _ = writeln!(toml, "b_rem_t = 1.2");
    let _ = writeln!(toml, "j0_a_per_m2 = {j0:e}");
    let _ = writeln!(toml, "axial_length_m = {l}");
    let _ = writeln!(toml, "torque_amplitude_nm_per_m = {amp:.9e}");
    let _ = writeln!(toml, "nodes = {}", mesh.n_nodes());
    let _ = writeln!(toml, "arkkio_l2 = {l2_a:.9e}");
    let _ = writeln!(toml, "line_l2 = {l2_m:.9e}");
    let _ = writeln!(toml, "arkkio_max_rel = {max_rel:.9e}");
    toml.push_str(
        "notes = [\n  \"Arkkio (volume-averaged) is the preferred estimator; \
         the Maxwell line integral is recorded for the record.\",\n  \"P1 \
         piecewise-constant B; Arkkio volume-averaging cancels the pointwise \
         product noise so the interaction torque lands under the 2% target even \
         though the raw mid-gap field L2 sits at the documented ~2.4% P1 floor \
         (#463).\",\n  \"Self-torques (pure-PM and pure-stator) are ~zero by \
         symmetry; the graded number is the genuine PM-vs-stator interaction \
         torque (#448 AC #4 discriminator).\",\n]\n\n",
    );
    for (i, (th, ta, tk, tl)) in rows.iter().enumerate() {
        let rel = (tk - ta).abs() / amp;
        let _ = writeln!(toml, "[point_{i}]");
        let _ = writeln!(toml, "theta_r = {th:.9e}");
        let _ = writeln!(toml, "T_analytic = {ta:.9e}");
        let _ = writeln!(toml, "T_arkkio = {tk:.9e}");
        let _ = writeln!(toml, "T_line = {tl:.9e}");
        let _ = writeln!(toml, "rel_err_arkkio = {rel:.9e}\n");
    }
    let toml_path = bench_dir.join("results.toml");
    fs::write(&toml_path, &toml).expect("write benchmark TOML");
    println!("wrote benchmark fixture: {}", toml_path.display());

    let csv_path = bench_dir.join("motor_torque.csv");
    let mut csv = String::with_capacity(rows.len() * 64);
    csv.push_str("theta_r,T_analytic,T_arkkio,T_line\n");
    for (th, ta, tk, tl) in &rows {
        let _ = writeln!(csv, "{th:.9},{ta:.9},{tk:.9},{tl:.9}");
    }
    let mut f = fs::File::create(&csv_path).expect("write csv");
    f.write_all(csv.as_bytes()).expect("write csv bytes");
    println!("wrote torque sweep CSV: {}", csv_path.display());

    let out_dir = std::env::var("MOTOR_OUT_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("artifacts/viz/motor"));
    fs::create_dir_all(&out_dir).expect("create viz output dir");
    let vtu_path = out_dir.join("motor_field.vtu");
    write_tri_vtu(&vtu_path, &mesh, &a_at_zero, &b_at_zero, &tags).expect("write vtu");
    println!("wrote θ_r=0 field VTU: {}", vtu_path.display());
    println!(
        "  (ParaView: colour by A_z, add a Glyph filter on the cell vector B — \
         the tearsheet cross-section visual)"
    );
}

/// Minimal ASCII VTU writer for a 2-D triangular mesh with a nodal scalar
/// `A_z` and a per-cell vector `B` (padded to 3-D with `z = 0`), for
/// ParaView. Self-contained here so the example owns its output format.
fn write_tri_vtu(
    path: &std::path::Path,
    mesh: &TriMesh,
    a_z: &[f64],
    b: &[[f64; 2]],
    tags: &[i32],
) -> std::io::Result<()> {
    let n_nodes = mesh.n_nodes();
    let n_tris = mesh.n_tris();
    assert_eq!(a_z.len(), n_nodes);
    assert_eq!(b.len(), n_tris);
    assert_eq!(tags.len(), n_tris);

    let mut s = String::with_capacity(512 + n_nodes * 48 + n_tris * 48);
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"UnstructuredGrid\" version=\"1.0\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <UnstructuredGrid>\n");
    let _ = writeln!(
        s,
        "    <Piece NumberOfPoints=\"{n_nodes}\" NumberOfCells=\"{n_tris}\">"
    );

    s.push_str("      <Points>\n");
    s.push_str("        <DataArray type=\"Float64\" NumberOfComponents=\"3\" format=\"ascii\">\n");
    for [x, y] in &mesh.nodes {
        let _ = writeln!(s, "          {x} {y} 0");
    }
    s.push_str("        </DataArray>\n      </Points>\n");

    s.push_str("      <Cells>\n");
    s.push_str("        <DataArray type=\"Int64\" Name=\"connectivity\" format=\"ascii\">\n");
    for t in &mesh.tris {
        let _ = writeln!(s, "          {} {} {}", t[0], t[1], t[2]);
    }
    s.push_str("        </DataArray>\n");
    s.push_str("        <DataArray type=\"Int64\" Name=\"offsets\" format=\"ascii\">\n");
    for cell in 0..n_tris {
        let _ = writeln!(s, "          {}", 3 * (cell + 1));
    }
    s.push_str("        </DataArray>\n");
    s.push_str("        <DataArray type=\"UInt8\" Name=\"types\" format=\"ascii\">\n");
    for _ in 0..n_tris {
        s.push_str("          5\n"); // 5 == VTK_TRIANGLE
    }
    s.push_str("        </DataArray>\n      </Cells>\n");

    // Nodal A_z.
    s.push_str("      <PointData Scalars=\"A_z\">\n");
    s.push_str(
        "        <DataArray type=\"Float64\" Name=\"A_z\" NumberOfComponents=\"1\" format=\"ascii\">\n",
    );
    for v in a_z {
        let _ = writeln!(s, "          {v}");
    }
    s.push_str("        </DataArray>\n      </PointData>\n");

    // Per-cell B vector (padded to 3-D), |B|, and band tag.
    s.push_str("      <CellData Vectors=\"B\" Scalars=\"B_mag\">\n");
    s.push_str(
        "        <DataArray type=\"Float64\" Name=\"B\" NumberOfComponents=\"3\" format=\"ascii\">\n",
    );
    for [bx, by] in b {
        let _ = writeln!(s, "          {bx} {by} 0");
    }
    s.push_str("        </DataArray>\n");
    s.push_str(
        "        <DataArray type=\"Float64\" Name=\"B_mag\" NumberOfComponents=\"1\" format=\"ascii\">\n",
    );
    for [bx, by] in b {
        let _ = writeln!(s, "          {}", (bx * bx + by * by).sqrt());
    }
    s.push_str("        </DataArray>\n");
    s.push_str(
        "        <DataArray type=\"Int32\" Name=\"band\" NumberOfComponents=\"1\" format=\"ascii\">\n",
    );
    for &t in tags {
        let _ = writeln!(s, "          {t}");
    }
    s.push_str("        </DataArray>\n      </CellData>\n");

    s.push_str("    </Piece>\n  </UnstructuredGrid>\n</VTKFile>\n");
    fs::write(path, s)
}
