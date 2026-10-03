//! `geode check`: validate a problem spec + mesh without solving.

use std::path::Path;

use geode_core::assembly::nedelec::sparsity_pattern_from_tet_edges;

use crate::dispersion::DispersionModel;
use crate::error::CliError;
use crate::problem::{self, MaterialSource, Problem};
use crate::report::{
    CapacitanceSettingsSummary, CheckReport, ConductorSummary, CurrentPathSummary,
    DebyePoleSummary, DispersionSummary, EigenSettingsSummary, ExtractSettingsSummary,
    FrequencySummary, InductanceSettingsSummary, LeontovichSummary, MeshSummary, PecSummary,
    PortSummary, Provenance, RegionSummary, ResourceEstimate, RoughnessSummary,
    SilverMullerSummary, SolverSummary, UpmlSummary, WavePortMediumSummary, WavePortSummary,
};
use crate::spec::{DispersionSpec, FrequencyUnit, RoughnessSpec, SolverSpec};

/// Load + resolve the spec and summarize it (no assembly, no solve).
pub fn run(spec_path: &Path, provenance: Provenance) -> Result<CheckReport, CliError> {
    let p = problem::load(spec_path, None)?;
    Ok(CheckReport {
        provenance,
        kind: "check",
        status: "ok",
        mesh: mesh_summary(&p),
        regions: region_summaries(&p),
        pec: pec_summaries(&p),
        leontovich: p
            .leontovich
            .iter()
            .map(|l| LeontovichSummary {
                physical_group: l.surface.name.clone(),
                tag: l.surface.tag,
                n_triangles: l.surface.triangles.len(),
                conductivity_s_m: l.sigma_s_m,
                conductivity_natural: l.sigma_natural,
                roughness: l.roughness.map(|r| roughness_summary(r, l, &p)),
            })
            .collect(),
        silver_muller: silver_muller_summaries(&p),
        absorbing_regions: upml_summaries(&p),
        ports: port_summaries(&p),
        wave_ports: wave_port_summaries(&p),
        frequencies: p
            .frequencies
            .iter()
            .map(|f| FrequencySummary {
                frequency_hz: f.hz,
                k0: f.k0,
            })
            .collect(),
        solver: solver_summary(p.solver()),
        analysis: p.analysis.name(),
        eigen: eigen_settings_summary(&p),
        extract: extract_settings_summary(&p),
        capacitance: capacitance_settings_summary(&p),
        inductance: inductance_settings_summary(&p),
        // The H(curl)-pencil calibration does not apply to the scalar
        // electrostatic system: no fabricated estimate. (The inductance
        // system is a real Nédélec curl-curl, like the anchor's pencil.)
        resources: p.capacitance.is_none().then(|| resource_estimate(&p)),
    })
}

/// Echo of the resolved `inductance` section with the edge-DOF system
/// size (`None` unless an inductance spec).
pub fn inductance_settings_summary(p: &Problem) -> Option<InductanceSettingsSummary> {
    let ind = p.inductance.as_ref()?;
    Some(InductanceSettingsSummary {
        paths: ind
            .paths
            .iter()
            .map(|c| CurrentPathSummary {
                name: c.name.clone(),
                conductor: c.conductor_group.clone(),
                conductor_tag: c.conductor_tag,
                n_conductor_tets: c.n_conductor_tets,
                source: c.source.name.clone(),
                n_source_triangles: c.source.triangles.len(),
                n_source_nodes: c.source_nodes.len(),
                sink: c.sink.name.clone(),
                n_sink_triangles: c.sink.triangles.len(),
                n_sink_nodes: c.sink_nodes.len(),
            })
            .collect(),
        excitation: "open_path_conduction",
        return_path: "pec_wall_return",
        element: "nedelec1_tet",
        gauge: "tree_cotree",
        n_dof: p.edges.len(),
        n_free_dof: p.n_interior(),
        n_solves: ind.paths.len(),
    })
}

/// Echo of the resolved `capacitance` section with the scalar system size
/// (`None` unless a capacitance spec).
pub fn capacitance_settings_summary(p: &Problem) -> Option<CapacitanceSettingsSummary> {
    let c = p.capacitance.as_ref()?;
    let summary = |list: &[problem::Conductor]| -> Vec<ConductorSummary> {
        list.iter()
            .map(|k| ConductorSummary {
                physical_group: k.surface.name.clone(),
                tag: k.surface.tag,
                n_triangles: k.surface.triangles.len(),
                n_nodes: k.nodes.len(),
            })
            .collect()
    };
    let n_dof = p.tagged.mesh.n_nodes();
    Some(CapacitanceSettingsSummary {
        terminals: summary(&c.terminals),
        ground: summary(&c.ground),
        conductor_model: "non_driven_grounded",
        element: "p1_tet",
        n_dof,
        n_free_dof: n_dof - c.n_pinned(),
        // Two P1 nodes couple iff they share a tet, i.e. a tet edge.
        nnz_k: n_dof + 2 * p.edges.len(),
        n_solves: c.terminals.len(),
    })
}

/// Anchor `nnz(A)`: the full (pre-PEC-elimination) Nédélec pattern of the
/// 2026-07-15 1.16M-DOF transmon run
/// (`benchmarks/transmon_bench_cpu/geode_runs_1p16M_2026-07-15.log`).
pub const ANCHOR_NNZ: f64 = 20_467_522.0;
/// Anchor peak RSS (GB = 10⁹ B): GNU time's `Maximum resident set size`
/// 92 166 884 kB, in KiB (reported as "92.2 GB" in
/// `docs/research/geode-vs-palace-comparison.md` §2b).
pub const ANCHOR_PEAK_GB: f64 = 92.166_884 * 1.024;
/// Anchor wall time (s): `TOTAL_S = 565.531` (assembly + one real LU
/// factorization + the shift-invert Lanczos back-solves).
pub const ANCHOR_WALL_S: f64 = 565.531;
/// Complex (`c64`, driven / extract) vs real (`f64`, eigen) pencil:
/// bytes per stored factor entry. Uncalibrated (no complex anchor).
pub const COMPLEX_MEMORY_FACTOR: f64 = 2.0;
/// Complex vs real pencil: real flops per complex multiply-add.
/// Uncalibrated (no complex anchor).
pub const COMPLEX_TIME_FACTOR: f64 = 4.0;
/// Machine-readable calibration basis of [`ResourceEstimate`]; bump the
/// date / figures here when re-calibrating.
pub const CALIBRATION_BASIS: &str = "linear-in-nnz(A) extrapolation from one direct-LU anchor: \
     2026-07-15 transmon eigen, real pencil, 1157564 interior DOF, nnz(A)=20467522, COLAMD, \
     faer supernodal LU, 565.5 s wall / 92166884 KiB (94.4 GB) peak RSS on a 128 GB cloud box; complex pencil \
     x2 memory / x4 time (uncalibrated); order-of-magnitude only";

/// Machine-readable basis of an **iterative** [`ResourceEstimate`].
pub const ITERATIVE_BASIS: &str = "vector count (no measured anchor), 2026-09-28: nnz(A) operator \
     storage x3 + 16 Krylov vectors + per-tet assembly buffers; flops = complex SpMV (8 nnz) + \
     ~10 vector ops per iteration; no wall-time model";

/// Up-front memory / cost estimate for the spec's solve (no assembly
/// beyond the sparsity pattern, no factorization).
///
/// **Direct** (and every eigen spec — always direct LU): peak memory and
/// wall time scaled **linearly in `nnz(A)`** from the single 2026-07-15
/// anchor ([`CALIBRATION_BASIS`]), ×[`COMPLEX_MEMORY_FACTOR`] /
/// ×[`COMPLEX_TIME_FACTOR`] for the complex driven pencil, with one
/// factorization per frequency. LU fill grows super-linearly in
/// `nnz(A)`, so the model **over-estimates below** the anchor scale and
/// **under-estimates above** it; treat it as order-of-magnitude. A
/// symbolic-fill (`nnz(L)`) model is deliberately not used: at the same
/// anchor scale a fill-reducing ordering that won on symbolic fill was
/// OOM-killed at 128.5 GB by the real supernodal LU.
///
/// **Iterative**: memory is a vector count (operator storage, Krylov
/// vectors, per-tet assembly buffers), not an extrapolation; cost is
/// reported as floating-point operations per Krylov iteration and a
/// worst-case total at `max_iters`. There is no measured iterative
/// wall-time anchor, so no wall time is reported.
///
/// Both modes report `anchor_nnz_ratio` (`nnz(A)` over [`ANCHOR_NNZ`]) and
/// `above_anchor`, so a caller can tell which side of the anchor (and so
/// which bias direction) its mesh is on. `wall_time_per_factorization_s`
/// scales the anchor's *total* run ([`ANCHOR_WALL_S`]: assembly + LU +
/// back-solves), not an isolated factorization.
///
/// Local check (2026-09-28, Apple M3 Ultra, release build, all fixtures
/// far below the anchor): direct peak memory came out 1.4–5× **high**
/// and direct wall time 4–40× **high** on the spiral smoke / benchmark,
/// SLCFET and patch extract benchmarks and the PEC sphere; iterative
/// memory on the spiral smoke mesh ~1.6× **low** (process and mesh
/// overhead are not modelled). See `crates/geode-cli/README.md`.
pub fn resource_estimate(p: &Problem) -> ResourceEstimate {
    let tet_edges: Vec<[u32; 6]> = p
        .tagged
        .mesh
        .tet_edges()
        .iter()
        .map(|row| std::array::from_fn(|i| row[i].0))
        .collect();
    let nnz = sparsity_pattern_from_tet_edges(&tet_edges).nnz();
    let n = p.n_interior() as f64;
    let is_eigen = p.eigen.is_some();
    let n_paths = p.inductance.as_ref().map(|i| i.paths.len());
    // A lossless eigen pencil and inductance factor a real symmetric
    // system; a lossy / open eigen pencil is complex (issue #706).
    let is_real = (is_eigen && !p.has_complex_materials()) || n_paths.is_some();
    let n_channels: usize = p.wave_ports.iter().map(|w| w.a_inc.len()).sum();
    let n_rhs = if is_eigen {
        0
    } else if n_paths.is_some() {
        1
    } else if n_channels > 0 {
        // The SMW column solves, then the excitations (every lumped port
        // and wave channel of a mixed spec, issue #759).
        n_channels + p.ports.len() + n_channels
    } else {
        p.ports.len()
    };
    let n_factorizations = match n_paths {
        Some(n) => n,
        None if is_eigen => 1,
        None => p.frequencies.len(),
    };
    let scale = nnz as f64 / ANCHOR_NNZ;
    let base = ResourceEstimate {
        solver_mode: "direct",
        scalar: if is_real { "real" } else { "complex" },
        nnz_a: nnz,
        n_factorizations,
        n_rhs_per_frequency: n_rhs,
        anchor_nnz_ratio: scale,
        above_anchor: scale > 1.0,
        peak_memory_gb: 0.0,
        wall_time_s: None,
        wall_time_per_factorization_s: None,
        flops_per_iteration: None,
        flops_max: None,
        peak_memory_confidence: "order_of_magnitude",
        wall_time_confidence: Some("conservative_below_anchor"),
        calibration_basis: CALIBRATION_BASIS,
    };
    match (is_real, p.solver()) {
        (
            false,
            SolverSpec::Iterative {
                max_iters,
                preconditioner,
                ..
            },
        ) => {
            // Operator values (c64) + column indices (u64), ×3 for the
            // assembled pieces / preconditioner; 16 Krylov-class c64
            // vectors; eight 6×6 f64 per-tet element-matrix buffers
            // (assembly). A vector count, not an extrapolation.
            let n_tets = p.tagged.mesh.n_tets() as f64;
            let mut bytes = nnz as f64 * 24.0 * 3.0 + 16.0 * n * 16.0 + n_tets * 36.0 * 8.0 * 8.0;
            // Complex SpMV (4 mul + 4 add per entry) + ~10 vector ops.
            let mut per_iter = 8.0 * nnz as f64 + 80.0 * n;
            // ILU(0) (issue #708, 2026-09-30): a second c64 copy of A's
            // values plus a per-row (col, pos) index (16 B per entry, one
            // Vec header per row); each apply is two triangular sweeps
            // over the same pattern (≈ one more complex SpMV).
            if preconditioner == crate::spec::PreconditionerSpec::Ilu0 {
                bytes += nnz as f64 * (16.0 + 16.0) + n * 24.0;
                per_iter += 8.0 * nnz as f64;
            }
            // AMS (issue #744): a real copy of the SPD proxy (f64 value +
            // u64 index), the gradient `G` (2 nnz/edge) and vector-nodal
            // `Π` (6 nnz/edge), the nodal `GᵀPG` (~15 nnz/node) with its
            // exact sparse-LU factor (a 3-D Poisson-like fill, modelled as
            // 15·N·log₂N entries), and `ΠᵀPΠ` (3N rows, ~150 nnz/row) plus
            // its Gauss–Seidel CSR copy. Each apply runs the real V-cycle on
            // Re and Im: 2 proxy SpMVs, one LU solve and 4 symmetric GS
            // sweeps over `ΠᵀPΠ` each. Order of magnitude only.
            if preconditioner == crate::spec::PreconditionerSpec::Ams {
                let nodes = p.tagged.mesh.n_nodes() as f64;
                let lu_fill = 15.0 * nodes * nodes.max(2.0).log2();
                let pi_nnz = 450.0 * nodes;
                bytes += nnz as f64 * 16.0
                    + 8.0 * n * 16.0
                    + 15.0 * nodes * 16.0
                    + lu_fill * 16.0
                    + 2.0 * pi_nnz * 16.0
                    + 12.0 * n * 8.0;
                per_iter += 2.0 * (4.0 * nnz as f64 + 4.0 * lu_fill + 16.0 * pi_nnz + 16.0 * n);
            }
            ResourceEstimate {
                solver_mode: "iterative",
                n_factorizations: 0,
                peak_memory_gb: bytes / 1e9,
                flops_per_iteration: Some(per_iter),
                flops_max: Some(per_iter * (max_iters * n_rhs * p.frequencies.len()) as f64),
                wall_time_confidence: None,
                calibration_basis: ITERATIVE_BASIS,
                ..base
            }
        }
        _ => {
            let (mem_f, time_f) = if is_real {
                (1.0, 1.0)
            } else {
                (COMPLEX_MEMORY_FACTOR, COMPLEX_TIME_FACTOR)
            };
            let per = ANCHOR_WALL_S * scale * time_f;
            ResourceEstimate {
                peak_memory_gb: ANCHOR_PEAK_GB * scale * mem_f,
                wall_time_per_factorization_s: Some(per),
                wall_time_s: Some(per * n_factorizations as f64),
                ..base
            }
        }
    }
}

/// Volume regions with applied permittivities.
pub fn region_summaries(p: &Problem) -> Vec<RegionSummary> {
    p.regions
        .iter()
        .map(|r| RegionSummary {
            physical_group: r.name.clone(),
            tag: r.tag,
            n_tets: r.n_tets,
            eps_r: [r.eps_r.re, r.eps_r.im],
            eps_r_source: match r.source {
                MaterialSource::Spec => "spec",
                MaterialSource::DefaultVacuum => "default_vacuum",
                MaterialSource::Dispersion => "dispersion",
            },
            mu_r: r.mu_r,
            dispersion: p
                .dispersion
                .iter()
                .find(|d| d.tag == r.tag)
                .map(|d| dispersion_summary(d, p)),
            eps_r_diag: r.eps_r_diag.map(|d| d.map(|e| [e.re, e.im])),
            mu_r_diag: r.mu_r_diag,
        })
        .collect()
}

/// PEC surfaces.
pub fn pec_summaries(p: &Problem) -> Vec<PecSummary> {
    p.pec
        .iter()
        .map(|s| PecSummary {
            physical_group: s.name.clone(),
            tag: s.tag,
            n_triangles: s.triangles.len(),
        })
        .collect()
}

/// Silver-Müller absorbing walls.
pub fn silver_muller_summaries(p: &Problem) -> Vec<SilverMullerSummary> {
    p.silver_muller
        .iter()
        .map(|s| SilverMullerSummary {
            physical_group: s.name.clone(),
            tag: s.tag,
            n_triangles: s.triangles.len(),
        })
        .collect()
}

/// Matched box-UPML shells with their derived inner walls.
pub fn upml_summaries(p: &Problem) -> Vec<UpmlSummary> {
    p.upml
        .iter()
        .map(|u| UpmlSummary {
            physical_group: u.name.clone(),
            tag: u.tag,
            n_tets: u.n_tets,
            n_tets_stretched: u.n_tets_stretched,
            thickness: u.thickness,
            sigma_0: u.sigma_0,
            air_box_lo: u.air_lo,
            air_box_hi: u.air_hi,
        })
        .collect()
}

/// Wave ports (cross-section geometry; `modes` unsolved = `null`).
pub fn wave_port_summaries(p: &Problem) -> Vec<WavePortSummary> {
    p.wave_ports
        .iter()
        .enumerate()
        .map(|(index, w)| WavePortSummary {
            index,
            physical_group: w.surface.name.clone(),
            tag: w.surface.tag,
            n_triangles: w.surface.triangles.len(),
            n_port_edges: w.projection.edges.len(),
            n_interior_port_edges: w.projection.n_interior_edges(),
            area: w.projection.area,
            normal: w.projection.normal,
            n_modes: w.a_inc.len(),
            a_inc: w.a_inc.iter().map(|a| [a.re, a.im]).collect(),
            modes: None,
            medium: {
                let m = p.port_medium(w);
                WavePortMediumSummary {
                    physical_groups: w.fill.groups.clone(),
                    eps_r_t: [m.eps_t.re, m.eps_t.im],
                    mu_r_t: m.mu_t,
                    mu_r_n: m.mu_n,
                }
            },
            tm_k_c: w
                .tm
                .map(|t| t.k_face.min(t.k_extrapolated))
                .filter(|k| k.is_finite()),
            tm_k_c_face: w.tm.map(|t| t.k_face).filter(|k| k.is_finite()),
            tm_limit_hz: p
                .port_tm_limit_k0(w)
                .map(|k0| problem::to_frequency(k0, FrequencyUnit::K0, p.length_unit_m()).hz),
            reference_ohm: w.reference_ohm,
        })
        .collect()
}

/// Echo of the resolved `eigen` section (`None` for a driven spec).
pub fn eigen_settings_summary(p: &Problem) -> Option<EigenSettingsSummary> {
    p.eigen.map(|e| EigenSettingsSummary {
        n_modes: e.n_modes,
        shift_hz: e.shift.hz,
        shift_k0: e.shift.k0,
        sigma: e.sigma(),
        max_iters: e.max_iters,
        tol: e.tol,
        residual_tol: e.residual_tol,
    })
}

/// Echo of the resolved `extract` section (`None` unless an extract spec).
pub fn extract_settings_summary(p: &Problem) -> Option<ExtractSettingsSummary> {
    p.extract.as_ref().map(|x| ExtractSettingsSummary {
        anchor_source: x.source.name(),
        anchor_frequencies: x
            .anchors
            .iter()
            .map(|&i| FrequencySummary {
                frequency_hz: p.frequencies[i].hz,
                k0: p.frequencies[i].k0,
            })
            .collect(),
        l0_rel_tol: x.l0_rel_tol,
    })
}

/// Mesh summary with DOF counts.
pub fn mesh_summary(p: &Problem) -> MeshSummary {
    MeshSummary {
        path: p.mesh_path.display().to_string(),
        sha256: p.mesh_sha256.clone(),
        length_unit_m: p.length_unit_m(),
        n_nodes: p.tagged.mesh.n_nodes(),
        n_tets: p.tagged.mesh.n_tets(),
        n_edges: p.edges.len(),
        n_interior: p.n_interior(),
    }
}

/// Port summaries in matrix order.
pub fn port_summaries(p: &Problem) -> Vec<PortSummary> {
    p.ports
        .iter()
        .enumerate()
        .map(|(index, port)| PortSummary {
            index,
            physical_group: port.surface.name.clone(),
            tag: port.surface.tag,
            n_triangles: port.surface.triangles.len(),
            e_hat: port.e_hat,
            width: port.width,
            length: port.length,
            geometry_derived: port.geometry_derived,
            resistance_ohm: port.resistance_ohm,
            v_inc: [port.v_inc.re, port.v_inc.im],
        })
        .collect()
}

/// Echo of the solver selection.
pub fn solver_summary(s: SolverSpec) -> SolverSummary {
    match s {
        SolverSpec::Direct {} => SolverSummary {
            mode: "direct",
            tol: None,
            max_iters: None,
        },
        SolverSpec::Iterative { tol, max_iters, .. } => SolverSummary {
            mode: "iterative",
            tol: Some(tol),
            max_iters: Some(max_iters),
        },
    }
}

/// Echo a rough wall's model and its `K(f)` over the requested
/// frequencies (issue #758).
fn roughness_summary(
    r: RoughnessSpec,
    l: &crate::problem::Leontovich,
    p: &problem::Problem,
) -> RoughnessSummary {
    let k = p
        .frequencies
        .iter()
        .map(|f| l.roughness_k(f.k0).expect("rough wall"))
        .collect();
    match r {
        RoughnessSpec::Hammerstad { rms_m } => RoughnessSummary {
            model: "hammerstad",
            rms_m: Some(rms_m),
            ball_radius_m: None,
            n_balls: None,
            tile_area_m2: None,
            k,
        },
        RoughnessSpec::Huray {
            ball_radius_m,
            n_balls,
            tile_area_m2,
        } => RoughnessSummary {
            model: "huray",
            rms_m: None,
            ball_radius_m: Some(ball_radius_m),
            n_balls: Some(n_balls),
            tile_area_m2: Some(tile_area_m2),
            k,
        },
    }
}

/// Echo a dispersive region's model, its fit / derived parameters and
/// `ε_r(f)` over the requested frequencies (issues #757, #761).
fn dispersion_summary(d: &problem::DispersiveRegion, p: &Problem) -> DispersionSummary {
    let eps_r_at_frequencies = p
        .frequencies
        .iter()
        .map(|f| {
            let e = d.model.eps(f.hz);
            [e.re, e.im]
        })
        .collect();
    let base = DispersionSummary {
        model: d.model.name(),
        eps_r: None,
        tan_delta: None,
        f_ref_hz: None,
        f_low_hz: None,
        f_high_hz: None,
        eps_inf: 0.0,
        delta_eps: None,
        poles: None,
        omega_p_rad_s: None,
        gamma_rad_s: None,
        re_eps_zero_hz: None,
        eps_r_at_frequencies,
    };
    match (&d.spec, &d.model) {
        (
            &DispersionSpec::DjordjevicSarkar {
                eps_r,
                tan_delta,
                f_ref_hz,
                f_low_hz,
                f_high_hz,
            },
            DispersionModel::DjordjevicSarkar(m),
        ) => DispersionSummary {
            eps_r: Some(eps_r),
            tan_delta: Some(tan_delta),
            f_ref_hz: Some(f_ref_hz),
            f_low_hz: Some(f_low_hz),
            f_high_hz: Some(f_high_hz),
            eps_inf: m.eps_inf,
            delta_eps: Some(m.delta_eps),
            ..base
        },
        (_, DispersionModel::Debye(m)) => DispersionSummary {
            eps_inf: m.eps_inf,
            delta_eps: Some(m.delta_eps()),
            poles: Some(
                m.poles
                    .iter()
                    .map(|pole| DebyePoleSummary {
                        delta_eps: pole.delta_eps,
                        tau_s: pole.tau_s,
                        f_relax_hz: 1.0 / (2.0 * std::f64::consts::PI * pole.tau_s),
                    })
                    .collect(),
            ),
            ..base
        },
        (_, DispersionModel::Drude(m)) => DispersionSummary {
            eps_inf: m.eps_inf,
            omega_p_rad_s: Some(m.omega_p_rad_s),
            gamma_rad_s: Some(m.gamma_rad_s),
            re_eps_zero_hz: m.re_eps_zero_hz(),
            ..base
        },
        (_, DispersionModel::DjordjevicSarkar(_)) => {
            unreachable!("the model is built from its own spec")
        }
    }
}
