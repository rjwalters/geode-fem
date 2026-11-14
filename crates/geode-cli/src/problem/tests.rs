use super::*;

/// Issue #1265: every analysis kind has an explicit order-2 decision, and
/// each rejection names what it is rejecting (the match in `p2_support`
/// is exhaustive, so a new kind cannot skip this table).
#[test]
fn p2_support_table_covers_every_analysis() {
    for a in [
        Analysis::Driven,
        Analysis::Eigen,
        Analysis::Extract,
        Analysis::Capacitance,
        Analysis::Inductance,
    ] {
        match p2_support(a) {
            P2Support::Supported => assert!(matches!(a, Analysis::Driven | Analysis::Eigen)),
            P2Support::Unsupported(why) => assert!(why.contains(a.name()), "{why}"),
        }
    }
}

/// Issue #930: the default `"auto"` preconditioner resolves to `ams`
/// with no warning when nothing rules AMS out, and to `jacobi` with a
/// warning naming the reason and `jacobi` for each CLI fallback (a
/// material conflict, UPML, floating PEC — first one wins); an explicit
/// choice passes through unchanged and never warns.
#[test]
fn auto_preconditioner_resolves_to_ams_or_falls_back_to_jacobi_with_a_warning() {
    use crate::spec::PreconditionerSpec as P;
    let iterative = |preconditioner| SolverSpec::Iterative {
        tol: 1e-9,
        max_iters: 77,
        preconditioner,
    };
    let no_floating = || false;
    let floating = || true;
    assert_eq!(
        resolve_auto_preconditioner(iterative(P::Auto), None, false, no_floating),
        (iterative(P::Ams), None)
    );
    let cases = [
        (
            Some("materials[m].eps_r has Re = -1 <= 0".to_string()),
            false,
            false,
            "materials[m]",
        ),
        (None, true, false, "absorbing_regions"),
        (None, false, true, "floating conductors"),
        // The material reason wins over the later ones.
        (
            Some("materials[x] reason".to_string()),
            true,
            true,
            "materials[x]",
        ),
    ];
    for (material, upml, is_floating, needle) in cases {
        let (solver, warning) = resolve_auto_preconditioner(
            iterative(P::Auto),
            material,
            upml,
            if is_floating { floating } else { no_floating },
        );
        assert_eq!(solver, iterative(P::Jacobi), "{needle}");
        let w = warning.expect("a fallback warns");
        assert!(w.contains(needle), "{w}");
        assert!(w.contains("fell back to `jacobi`"), "{w}");
    }
    // Explicit choices: unchanged, no warning, even where `auto` would
    // fall back.
    for pc in [P::Jacobi, P::Ilu0, P::Ams] {
        assert_eq!(
            resolve_auto_preconditioner(iterative(pc), None, true, floating),
            (iterative(pc), None),
            "{pc:?}"
        );
    }
    assert_eq!(
        resolve_auto_preconditioner(SolverSpec::Direct {}, None, true, floating),
        (SolverSpec::Direct {}, None)
    );
}

#[test]
fn frequency_units_round_trip() {
    // 1 GHz on a micron mesh: k0 = 2π·1e9·1e-6 / c ≈ 2.0958e-5 rad/µm
    // (the spiral example's `ghz_to_omega_um(1.0)`).
    let f = to_frequency(1.0, FrequencyUnit::Ghz, 1e-6);
    assert!((f.k0 - 2.095845021951682e-5).abs() < 1e-18);
    assert_eq!(f.hz, 1e9);
    let back = to_frequency(f.k0, FrequencyUnit::K0, 1e-6);
    assert!((back.hz - 1e9).abs() / 1e9 < 1e-14);
    let hz = to_frequency(1e9, FrequencyUnit::Hz, 1e-6);
    assert_eq!(hz.k0, f.k0);
}

/// The AMS floating-PEC rule (issue #744): one PEC component (or
/// none) passes; a PEC island not connected to the outer wall is
/// rejected as `invalid_spec`, naming the island's groups and not the
/// wall's.
#[test]
fn ams_rejects_floating_pec_conductors() {
    // Unit cube corners 0..8 plus an interior triangle 8..11 and a
    // second interior triangle sharing an edge with it (11).
    let mut nodes: Vec<[f64; 3]> = (0..8)
        .map(|i| [(i & 1) as f64, ((i >> 1) & 1) as f64, ((i >> 2) & 1) as f64])
        .collect();
    nodes.extend([
        [0.4, 0.4, 0.5],
        [0.6, 0.4, 0.5],
        [0.5, 0.6, 0.5],
        [0.5, 0.5, 0.6],
    ]);
    let surf = |name: &str, tris: Vec<[u32; 3]>| Surface {
        name: name.to_string(),
        tag: 1,
        triangles: tris,
    };
    // Outer wall on the cube's boundary; a two-group interior island
    // (`m1` + `via` share the edge 8–9).
    let wall = surf("outer_boundary", vec![[0, 1, 2], [1, 2, 3], [2, 3, 6]]);
    let island = surf("m1", vec![[8, 9, 10]]);
    let island_b = surf("via", vec![[8, 9, 11]]);
    assert!(check_ams_floating_pec(&nodes, std::slice::from_ref(&wall)).is_ok());
    assert!(check_ams_floating_pec(&nodes, &[]).is_ok());
    // A lone interior conductor (no outer PEC) is its own reference.
    assert!(check_ams_floating_pec(&nodes, std::slice::from_ref(&island)).is_ok());

    let err = check_ams_floating_pec(&nodes, &[wall.clone(), island.clone(), island_b])
        .expect_err("floating island must be rejected");
    let CliError::InvalidSpec(msg) = err else {
        panic!("expected InvalidSpec, got {err:?}");
    };
    assert!(msg.contains("2 disconnected components"), "{msg}");
    assert!(msg.contains("`m1` + `via` is not connected"), "{msg}");
    assert!(msg.contains("outer PEC (`outer_boundary`)"), "{msg}");
    assert!(
        msg.contains("leontovich") && msg.contains("direct"),
        "{msg}"
    );
}

/// `wave_ports[].reference_ohm` (issue #775): optional, but when
/// given it must be finite and `> 0` (a non-finite value can only
/// come from TOML, which spells `nan` / `inf`).
#[test]
fn wave_port_reference_ohm_must_be_finite_and_positive() {
    let spec = |r: Option<f64>| -> ProblemSpec {
        let v = serde_json::json!({
            "schema_version": 1,
            "mesh": {"path": "m.msh", "length_unit_m": 1e-2},
            "boundary_conditions": {"pec": ["walls"]},
            "wave_ports": [{"physical_group": "wp"}],
            "frequencies": {"unit": "ghz", "values": [10.0]}
        });
        // `serde_json` cannot hold NaN / inf: set it on the struct.
        let mut s: ProblemSpec = serde_json::from_value(v).unwrap();
        s.wave_ports[0].reference_ohm = r;
        s
    };
    assert!(validate_open_boundaries(&spec(None)).is_ok());
    assert!(validate_open_boundaries(&spec(Some(50.0))).is_ok());
    for bad in [0.0, -50.0, f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        match validate_open_boundaries(&spec(Some(bad))) {
            Err(CliError::InvalidSpec(m)) => assert!(
                m.contains("wave_ports[wp].reference_ohm must be finite and > 0"),
                "{m}"
            ),
            other => panic!("reference_ohm = {bad}: expected InvalidSpec, got {other:?}"),
        }
    }
    // The TOML spelling reaches the same rule.
    let toml_spec: ProblemSpec = toml::from_str(
        "schema_version = 1\nmesh = { path = \"m.msh\", length_unit_m = 1e-2 }\n\
         frequencies = { unit = \"ghz\", values = [10.0] }\n\
         [[wave_ports]]\nphysical_group = \"wp\"\nreference_ohm = nan\n",
    )
    .unwrap();
    assert!(matches!(
        validate_open_boundaries(&toml_spec),
        Err(CliError::InvalidSpec(_))
    ));
}

/// Roughness rules and SI → natural conversion (issue #758).
#[test]
fn roughness_validation_and_units() {
    let bad = |r: RoughnessSpec, field: &str| match validate_roughness("c", &r) {
        Err(CliError::InvalidSpec(m)) => {
            assert!(
                m.contains(&format!("leontovich[c].roughness.{field}")),
                "{m}"
            );
        }
        other => panic!("{r:?}: expected InvalidSpec, got {other:?}"),
    };
    for rms_m in [-1e-6, f64::NAN, f64::INFINITY] {
        bad(RoughnessSpec::Hammerstad { rms_m }, "rms_m");
    }
    let huray = |ball_radius_m, n_balls, tile_area_m2| RoughnessSpec::Huray {
        ball_radius_m,
        n_balls,
        tile_area_m2,
    };
    for a in [0.0, -1e-6, f64::NAN, f64::INFINITY] {
        bad(huray(a, 1.0, 1e-10), "ball_radius_m");
    }
    for n in [-1.0, f64::NAN, f64::INFINITY] {
        bad(huray(1e-6, n, 1e-10), "n_balls");
    }
    for area in [0.0, -1e-10, f64::NAN, f64::INFINITY] {
        bad(huray(1e-6, 1.0, area), "tile_area_m2");
    }
    // Zero roughness (smooth) is accepted, as is a finite model.
    validate_roughness("c", &RoughnessSpec::Hammerstad { rms_m: 0.0 }).unwrap();
    validate_roughness("c", &huray(0.5e-6, 0.0, 1e-10)).unwrap();
    validate_roughness("c", &huray(0.5e-6, 14.0, 1e-10)).unwrap();

    // Lengths / L_unit, areas / L_unit².
    assert_eq!(
        roughness_natural(RoughnessSpec::Hammerstad { rms_m: 2e-6 }, 1e-6),
        SurfaceRoughness::HammerstadJensen { rms: 2.0 }
    );
    let SurfaceRoughness::Huray {
        ball_radius,
        n_balls,
        tile_area,
    } = roughness_natural(huray(0.5e-6, 14.0, 1e-10), 1e-6)
    else {
        panic!("huray")
    };
    assert!((ball_radius - 0.5).abs() < 1e-15);
    assert_eq!(n_balls, 14.0);
    assert!((tile_area - 100.0).abs() < 1e-12);
}

/// `sweep` rules (issue #708): scalar, so a parsed spec suffices.
#[test]
fn validate_sweep_accepts_and_rejects() {
    let spec = |extra: serde_json::Value| -> ProblemSpec {
        let mut v = serde_json::json!({
            "schema_version": 1,
            "mesh": {"path": "m.msh", "length_unit_m": 1e-6},
            "ports": [{"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
            "frequencies": {"unit": "ghz", "values": [1.0, 2.0]},
            "sweep": {"adaptive": {}}
        });
        for (k, x) in extra.as_object().unwrap() {
            v[k] = x.clone();
        }
        serde_json::from_value(v).expect("spec parses")
    };
    let msg = |s: &ProblemSpec, a: Analysis| match validate_sweep(s, a) {
        Err(CliError::InvalidSpec(m)) => {
            assert!(!m.contains("  "), "stray whitespace: {m:?}");
            m
        }
        other => panic!("expected InvalidSpec, got {other:?}"),
    };
    // Defaults; Leontovich / Silver-Müller walls are fine.
    let ok = spec(serde_json::json!({
        "boundary_conditions": {
            "leontovich": [{"physical_group": "c", "conductivity_s_m": 5.8e7}],
            "silver_muller": ["o"]
        }
    }));
    let a = ok.sweep.as_ref().unwrap().adaptive.unwrap();
    assert_eq!((a.tolerance, a.max_snapshots), (1e-6, 20));
    validate_sweep(&ok, Analysis::Driven).unwrap();
    validate_sweep(&ok, Analysis::Extract).unwrap();
    // An empty `sweep` is the dense sweep.
    validate_sweep(&spec(serde_json::json!({"sweep": {}})), Analysis::Driven).unwrap();

    assert!(msg(&ok, Analysis::Eigen).contains("driven / extract specs only"));
    let m = msg(
        &spec(serde_json::json!({"absorbing_regions": [
            {"physical_group": "u", "thickness": 1.0, "sigma_0": 25.0}
        ]})),
        Analysis::Driven,
    );
    assert!(
        m.contains("absorbing_regions") && m.contains("remove"),
        "{m}"
    );
    // Wave ports, alone or mixed with lumped ports, are fine (#774).
    validate_sweep(
        &spec(serde_json::json!({"ports": [], "wave_ports": [{"physical_group": "w"}]})),
        Analysis::Driven,
    )
    .unwrap();
    validate_sweep(
        &spec(serde_json::json!({"wave_ports": [{"physical_group": "w"}]})),
        Analysis::Driven,
    )
    .unwrap();
    let m = msg(
        &spec(serde_json::json!({"solver": {"mode": "iterative"}})),
        Analysis::Driven,
    );
    assert!(m.contains("direct"), "{m}");
    for bad in [0.0, 1.0, -1e-6] {
        let m = msg(
            &spec(serde_json::json!({"sweep": {"adaptive": {"tolerance": bad}}})),
            Analysis::Driven,
        );
        assert!(m.contains("tolerance"), "{m}");
    }
    let m = msg(
        &spec(serde_json::json!({"sweep": {"adaptive": {"max_snapshots": 0}}})),
        Analysis::Driven,
    );
    assert!(m.contains("max_snapshots"), "{m}");
    // Unknown keys are rejected at parse time.
    assert!(
        serde_json::from_value::<crate::spec::SweepSpec>(
            serde_json::json!({"adaptive": {"tol": 1e-6}})
        )
        .is_err()
    );
}

/// `materials[].dispersion` rules (issue #757), all scalar: they fire
/// before the mesh is read, so a valid driven spec gets as far as the
/// (absent) mesh file.
#[test]
fn dispersion_validation() {
    let ds = serde_json::json!({
        "model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02, "f_ref_hz": 1e9
    });
    let spec = |material: serde_json::Value, extra: serde_json::Value| -> ProblemSpec {
        let mut v = serde_json::json!({
            "schema_version": 1,
            "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-6},
            "materials": [material],
            "ports": [{"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
            "frequencies": {"unit": "ghz", "values": [1.0, 2.0]}
        });
        for (k, x) in extra.as_object().unwrap() {
            if x.is_null() {
                v.as_object_mut().unwrap().remove(k);
            } else {
                v[k] = x.clone();
            }
        }
        serde_json::from_value(v).expect("spec parses")
    };
    let load = |s: ProblemSpec| load_parsed(s, Path::new("."), None);
    let msg = |s: ProblemSpec| match load(s) {
        Err(CliError::InvalidSpec(m)) => {
            assert!(!m.contains("  "), "stray whitespace: {m:?}");
            m
        }
        other => panic!("expected InvalidSpec, got {other:?}"),
    };
    let mat = |d: serde_json::Value| serde_json::json!({"physical_group": "sub", "dispersion": d});

    // Valid: defaults filled in, and scalar validation passes (the
    // load fails reading the mesh).
    let ok = spec(mat(ds.clone()), serde_json::json!({}));
    assert_eq!(
        ok.materials[0].dispersion,
        Some(crate::spec::DispersionSpec::DjordjevicSarkar {
            eps_r: 4.3,
            tan_delta: 0.02,
            f_ref_hz: 1e9,
            f_low_hz: 1e3,
            f_high_hz: 1e12,
        })
    );
    assert!(matches!(load(ok), Err(CliError::Io { .. })));
    // Extract and a dense `sweep` are fine too.
    let ok = spec(
        mat(ds.clone()),
        serde_json::json!({"extract": {}, "sweep": {}}),
    );
    assert!(matches!(load(ok), Err(CliError::Io { .. })));

    // eps_r alongside dispersion.
    let mut both = mat(ds.clone());
    both["eps_r"] = serde_json::json!([4.3, -0.086]);
    let m = msg(spec(both, serde_json::json!({})));
    assert!(m.contains("both `eps_r` and `dispersion`"), "{m}");

    // Model inputs and the fit.
    for (field, value, needle) in [
        ("eps_r", serde_json::json!(0.0), "eps_r"),
        ("tan_delta", serde_json::json!(-0.01), "tan_delta"),
        (
            "f_ref_hz",
            serde_json::json!(1e13),
            "f_low_hz < f_ref_hz < f_high_hz",
        ),
        (
            "f_low_hz",
            serde_json::json!(2e9),
            "f_low_hz < f_ref_hz < f_high_hz",
        ),
        (
            "f_high_hz",
            serde_json::json!(1e8),
            "f_low_hz < f_ref_hz < f_high_hz",
        ),
        (
            "f_low_hz",
            serde_json::json!(-1.0),
            "f_low_hz < f_ref_hz < f_high_hz",
        ),
    ] {
        let mut d = ds.clone();
        d[field] = value;
        let m = msg(spec(mat(d), serde_json::json!({})));
        assert!(
            m.contains("materials[sub].dispersion") && m.contains(needle),
            "{field}: {m}"
        );
    }
    let mut narrow = ds.clone();
    narrow["tan_delta"] = serde_json::json!(1.5);
    narrow["f_low_hz"] = serde_json::json!(0.5e9);
    narrow["f_high_hz"] = serde_json::json!(2e9);
    let m = msg(spec(mat(narrow), serde_json::json!({})));
    assert!(m.contains("eps_inf"), "{m}");

    // Unsupported analyses / sections.
    let no_drive = serde_json::json!({"ports": null, "frequencies": null});
    let with = |extra: serde_json::Value| {
        let mut e = no_drive.clone();
        for (k, x) in extra.as_object().unwrap() {
            e[k] = x.clone();
        }
        e
    };
    for (extra, needle) in [
        (
            with(serde_json::json!({"eigen": {"n_modes": 2, "unit": "ghz", "shift": 1.0}})),
            "geode eigen",
        ),
        (
            with(serde_json::json!({"capacitance": {"terminals": ["a"], "ground": ["g"]}})),
            "geode capacitance",
        ),
        (
            with(serde_json::json!({"inductance": {"paths": [
                {"name": "p", "conductor": "c", "source": "s", "sink": "t"}
            ]}})),
            "geode inductance",
        ),
        (
            serde_json::json!({"sweep": {"adaptive": {}}}),
            "sweep.adaptive",
        ),
        (
            // Lumped ports differentiate a dispersive forward (#1259); a
            // wave port's face solves are single-frequency.
            serde_json::json!({"sensitivity": {"parameters": [
                {"physical_group": "sub", "kind": "dispersion", "model_parameter": "eps_r"}
            ]}, "wave_ports": [{"physical_group": "w"}]}),
            "wave_ports",
        ),
    ] {
        let m = msg(spec(mat(ds.clone()), extra.clone()));
        assert!(
            m.contains("materials[sub].dispersion") && m.contains(needle),
            "{extra}: {m}"
        );
    }

    // Unknown model / stray key: parse errors.
    for bad in [
        serde_json::json!({"model": "drude_lorentz", "eps_r": 4.3}),
        serde_json::json!({
            "model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02,
            "f_ref_hz": 1e9, "tau_s": 1e-9
        }),
        serde_json::json!({"model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02}),
    ] {
        assert!(
            serde_json::from_value::<crate::spec::DispersionSpec>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// Debye / Drude spec blocks (issue #761): parse, prefixed input
/// errors, and the scalar AMS guard on `Re ε_r(f) ≤ 0`.
#[test]
fn debye_drude_validation_and_ams_guard() {
    let spec = |d: serde_json::Value, solver: serde_json::Value, ghz: &[f64]| -> ProblemSpec {
        serde_json::from_value(serde_json::json!({
            "schema_version": 1,
            "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-6},
            "materials": [{"physical_group": "sub", "dispersion": d}],
            "ports": [{"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}],
            "frequencies": {"unit": "ghz", "values": ghz},
            "solver": solver
        }))
        .expect("spec parses")
    };
    let load = |s: ProblemSpec| load_parsed(s, Path::new("."), None);
    let msg = |s: ProblemSpec| match load(s) {
        Err(CliError::InvalidSpec(m)) => {
            assert!(!m.contains("  "), "stray whitespace: {m:?}");
            m
        }
        other => panic!("expected InvalidSpec, got {other:?}"),
    };
    let direct = serde_json::json!({"mode": "direct"});
    let ams = serde_json::json!({"mode": "iterative", "preconditioner": "ams"});
    let tau = 1.0 / (2.0 * std::f64::consts::PI * 5e9);
    let debye = serde_json::json!({
        "model": "debye", "eps_inf": 10.0, "poles": [{"delta_eps": 2.0, "tau_s": tau}]
    });
    // Plasma at 8 GHz, γ at 1 GHz (ε∞ = 1): Re ε < 0 below √63 GHz.
    let two_pi = 2.0 * std::f64::consts::PI;
    let plasma = serde_json::json!({
        "model": "drude", "eps_inf": 1.0,
        "omega_p_rad_s": two_pi * 8e9, "gamma_rad_s": two_pi * 1e9
    });

    let parsed = spec(debye.clone(), direct.clone(), &[1.0]);
    assert_eq!(
        parsed.materials[0].dispersion,
        Some(crate::spec::DispersionSpec::Debye {
            eps_inf: 10.0,
            poles: vec![crate::spec::DebyePole {
                delta_eps: 2.0,
                tau_s: tau
            }],
        })
    );
    // Valid specs get as far as the (absent) mesh — Debye with AMS
    // (Re ε ≥ ε∞ > 0), Drude direct or iterative jacobi / ilu0 below
    // the crossover, and Drude AMS entirely above it.
    for (d, solver, ghz) in [
        (debye.clone(), ams.clone(), vec![1.0, 20.0]),
        (plasma.clone(), direct.clone(), vec![1.0, 20.0]),
        (
            plasma.clone(),
            serde_json::json!({"mode": "iterative", "preconditioner": "jacobi"}),
            vec![1.0, 20.0],
        ),
        (
            plasma.clone(),
            serde_json::json!({"mode": "iterative", "preconditioner": "ilu0"}),
            vec![1.0],
        ),
        (plasma.clone(), ams.clone(), vec![8.0, 20.0]),
    ] {
        assert!(
            matches!(
                load(spec(d.clone(), solver.clone(), &ghz)),
                Err(CliError::Io { .. })
            ),
            "{d} / {solver} / {ghz:?}"
        );
    }
    // The guard: AMS with any frequency below the crossover.
    let m = msg(spec(plasma.clone(), ams.clone(), &[1.0, 5.0, 10.0, 20.0]));
    for needle in [
        "materials[sub].dispersion (drude)",
        "Re eps_r(f) <= 0 at 2 of the 4 solved frequencies",
        "below 7.937254e9 Hz",
        "first 1.000000e9 Hz",
        "preconditioner = \"ams\"",
        "solver.mode = \"direct\"",
    ] {
        assert!(m.contains(needle), "{needle}: {m}");
    }
    // Lossless (γ = 0) plasma: Re ε < 0 below ω_p/√ε∞ = 8 GHz.
    let lossless = serde_json::json!({
        "model": "drude", "eps_inf": 1.0, "omega_p_rad_s": two_pi * 8e9, "gamma_rad_s": 0.0
    });
    assert!(matches!(
        load(spec(lossless.clone(), ams.clone(), &[9.0])),
        Err(CliError::Io { .. })
    ));
    let m = msg(spec(lossless, ams.clone(), &[7.0, 9.0]));
    assert!(
        m.contains("1 of the 2") && m.contains("below 8.000000e9 Hz"),
        "{m}"
    );

    // Prefixed model-input errors.
    for (d, needle) in [
        (
            serde_json::json!({"model": "debye", "eps_inf": 0.0, "poles": [{"delta_eps": 1.0, "tau_s": 1e-10}]}),
            "eps_inf",
        ),
        (
            serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": []}),
            "at least one pole",
        ),
        (
            serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": -1.0, "tau_s": 1e-10}]}),
            "poles[0].delta_eps",
        ),
        (
            serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": 1.0, "tau_s": 0.0}]}),
            "poles[0].tau_s",
        ),
        (
            serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 0.0, "gamma_rad_s": 1e9}),
            "omega_p_rad_s",
        ),
        (
            serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 1e10, "gamma_rad_s": -1.0}),
            "gamma_rad_s",
        ),
    ] {
        let m = msg(spec(d.clone(), direct.clone(), &[1.0]));
        assert!(
            m.contains("materials[sub].dispersion") && m.contains(needle),
            "{d}: {m}"
        );
    }
    // Missing / stray keys are parse errors.
    for bad in [
        serde_json::json!({"model": "debye", "eps_inf": 4.0}),
        serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": 1.0}]}),
        serde_json::json!({"model": "debye", "eps_inf": 4.0, "poles": [{"delta_eps": 1.0, "tau_s": 1e-10, "f_hz": 1e9}]}),
        serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 1e10}),
        serde_json::json!({"model": "drude", "eps_inf": 1.0, "omega_p_rad_s": 1e10, "gamma_rad_s": 0.0, "tan_delta": 0.0}),
    ] {
        assert!(
            serde_json::from_value::<crate::spec::DispersionSpec>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// `materials[].eps_r_diag` / `mu_r_diag` rules (issue #760) and the
/// generalized AMS guard (constant / diagonal `Re ε ≤ 0`), all scalar:
/// they fire before the (absent) mesh is read; a valid spec fails with
/// `Io` on the mesh instead.
#[test]
fn anisotropic_validation_and_ams_guard() {
    let diag_e = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.0], "zz": [9.0, -0.1]});
    let diag_e_real = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.0], "zz": [9.0, 0.0]});
    let diag_m = serde_json::json!({"xx": 1.0, "yy": 1.0, "zz": 2.0});
    let base = |analysis: &str| -> serde_json::Value {
        let mut v = serde_json::json!({
            "schema_version": 1,
            "mesh": {"path": "does-not-exist.msh", "length_unit_m": 1e-6},
        });
        match analysis {
            "driven" => {
                v["ports"] = serde_json::json!([
                    {"physical_group": "p", "e_hat": [0.0, 1.0, 0.0], "resistance_ohm": 50.0}
                ]);
                v["frequencies"] = serde_json::json!({"unit": "ghz", "values": [1.0, 2.0]});
            }
            "eigen" => {
                v["boundary_conditions"] = serde_json::json!({"pec": ["wall"]});
                v["eigen"] = serde_json::json!({"n_modes": 2, "shift": 1.0, "unit": "ghz"});
            }
            "capacitance" => {
                v["capacitance"] = serde_json::json!({"terminals": ["a"], "ground": ["g"]});
            }
            "inductance" => {
                v["boundary_conditions"] = serde_json::json!({"pec": ["wall"]});
                v["inductance"] = serde_json::json!({"paths": [
                    {"name": "p", "conductor": "c", "source": "s", "sink": "k"}
                ]});
            }
            _ => unreachable!(),
        }
        v
    };
    let spec =
        |analysis: &str, material: serde_json::Value, extra: serde_json::Value| -> ProblemSpec {
            let mut v = base(analysis);
            v["materials"] = serde_json::json!([material]);
            for (k, x) in extra.as_object().unwrap() {
                v[k] = x.clone();
            }
            serde_json::from_value(v).expect("spec parses")
        };
    let load = |s: ProblemSpec| load_parsed(s, Path::new("."), None);
    let msg = |s: ProblemSpec| match load(s) {
        Err(CliError::InvalidSpec(m)) => {
            assert!(!m.contains("  "), "stray whitespace: {m:?}");
            m
        }
        other => panic!("expected InvalidSpec, got {other:?}"),
    };
    let none = serde_json::json!({});
    let ams = serde_json::json!({"solver": {"mode": "iterative", "preconditioner": "ams"}});
    let mat = |fields: serde_json::Value| {
        let mut m = serde_json::json!({"physical_group": "sub"});
        for (k, x) in fields.as_object().unwrap() {
            m[k] = x.clone();
        }
        m
    };

    // Valid: parsed as given, scalar validation passes.
    let parsed = spec(
        "driven",
        mat(serde_json::json!({"eps_r_diag": diag_e, "mu_r_diag": diag_m})),
        none.clone(),
    );
    assert_eq!(
        parsed.materials[0].eps_r_diag,
        Some(crate::spec::EpsDiagSpec {
            xx: [3.0, 0.0],
            yy: [3.0, 0.0],
            zz: [9.0, -0.1]
        })
    );
    assert_eq!(
        parsed.materials[0].mu_r_diag,
        Some(crate::spec::MuDiagSpec {
            xx: 1.0,
            yy: 1.0,
            zz: 2.0
        })
    );
    for (analysis, fields, extra) in [
        (
            "driven",
            serde_json::json!({"eps_r_diag": diag_e, "mu_r_diag": diag_m}),
            none.clone(),
        ),
        (
            "driven",
            serde_json::json!({"eps_r_diag": diag_e}),
            serde_json::json!({"sweep": {"adaptive": {}}}),
        ),
        (
            "driven",
            serde_json::json!({"eps_r_diag": diag_e}),
            ams.clone(),
        ),
        (
            "eigen",
            serde_json::json!({"eps_r_diag": diag_e, "mu_r_diag": diag_m}),
            none.clone(),
        ),
        (
            "capacitance",
            serde_json::json!({"eps_r_diag": diag_e_real}),
            none.clone(),
        ),
        (
            "inductance",
            serde_json::json!({"mu_r_diag": diag_m}),
            none.clone(),
        ),
        // A negative constant Re ε stays legal without AMS.
        (
            "driven",
            serde_json::json!({"eps_r": [-2.0, -0.1]}),
            none.clone(),
        ),
    ] {
        assert!(
            matches!(
                load(spec(analysis, mat(fields.clone()), extra.clone())),
                Err(CliError::Io { .. })
            ),
            "{analysis} / {fields} / {extra}"
        );
    }

    // Rejections.
    let ds = serde_json::json!({
        "model": "djordjevic_sarkar", "eps_r": 4.3, "tan_delta": 0.02, "f_ref_hz": 1e9
    });
    let gain = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.1], "zz": [9.0, 0.0]});
    let neg_zz = serde_json::json!({"xx": [3.0, 0.0], "yy": [3.0, 0.0], "zz": [-1.0, 0.0]});
    let zero_mu = serde_json::json!({"xx": 1.0, "yy": 0.0, "zz": 2.0});
    for (analysis, fields, extra, needles) in [
        (
            "driven",
            serde_json::json!({"eps_r": [2.0, 0.0], "eps_r_diag": diag_e}),
            none.clone(),
            vec!["materials[sub]", "both `eps_r` and `eps_r_diag`"],
        ),
        (
            "driven",
            serde_json::json!({"eps_r_diag": diag_e, "dispersion": ds}),
            none.clone(),
            vec!["both `eps_r_diag` and `dispersion`"],
        ),
        (
            "driven",
            serde_json::json!({"mu_r": 2.0, "mu_r_diag": diag_m}),
            none.clone(),
            vec!["both `mu_r` and `mu_r_diag`"],
        ),
        (
            "driven",
            serde_json::json!({"eps_r_diag": gain}),
            none.clone(),
            vec!["eps_r_diag.yy", "gain"],
        ),
        (
            "driven",
            serde_json::json!({"mu_r_diag": zero_mu}),
            none.clone(),
            vec!["mu_r_diag.yy", "> 0"],
        ),
        (
            "inductance",
            serde_json::json!({"eps_r_diag": diag_e}),
            none.clone(),
            vec!["eps_r_diag has no effect on a magnetostatic solve"],
        ),
        (
            "capacitance",
            serde_json::json!({"mu_r_diag": diag_m}),
            none.clone(),
            vec!["mu_r_diag has no effect on an electrostatic solve"],
        ),
        (
            "capacitance",
            serde_json::json!({"eps_r_diag": diag_e}),
            none.clone(),
            vec!["eps_r_diag must be real with Re > 0"],
        ),
        (
            "eigen",
            serde_json::json!({"eps_r_diag": neg_zz}),
            none.clone(),
            vec!["eps_r_diag must have Re > 0 on every axis"],
        ),
        (
            "driven",
            serde_json::json!({"eps_r_diag": diag_e}),
            serde_json::json!({"sensitivity": {"parameters": [
                {"physical_group": "sub", "kind": "eps_r"}
            ]}}),
            vec![
                "`sensitivity` does not support anisotropic materials",
                "materials[sub]",
            ],
        ),
        (
            "inductance",
            serde_json::json!({"mu_r_diag": diag_m}),
            serde_json::json!({"sensitivity": {"parameters": [
                {"physical_group": "sub", "kind": "mu_r"}
            ]}}),
            vec!["`sensitivity` does not support anisotropic materials"],
        ),
        // The AMS guard (folded-in #769 review follow-up).
        (
            "driven",
            serde_json::json!({"eps_r": [-2.0, -0.1]}),
            ams.clone(),
            vec![
                "materials[sub].eps_r has Re = -2",
                "preconditioner = \"ams\"",
                "direct",
            ],
        ),
        (
            "driven",
            serde_json::json!({"eps_r": [0.0, -0.1]}),
            ams.clone(),
            vec!["materials[sub].eps_r has Re = 0"],
        ),
        (
            "driven",
            serde_json::json!({"eps_r_diag": neg_zz}),
            ams.clone(),
            vec![
                "materials[sub].eps_r_diag.zz has Re = -1",
                "preconditioner = \"ams\"",
            ],
        ),
    ] {
        let m = msg(spec(analysis, mat(fields.clone()), extra.clone()));
        for needle in needles {
            assert!(
                m.contains(needle),
                "{fields} / {extra}: `{needle}` not in {m}"
            );
        }
    }

    // Missing / stray components are parse errors.
    for bad in [
        serde_json::json!({"physical_group": "sub", "eps_r_diag": {"xx": [1.0, 0.0], "yy": [1.0, 0.0]}}),
        serde_json::json!({"physical_group": "sub", "eps_r_diag": {"xx": [1.0, 0.0], "yy": [1.0, 0.0], "zz": [1.0, 0.0], "xy": [0.0, 0.0]}}),
        serde_json::json!({"physical_group": "sub", "eps_r_diag": {"xx": 1.0, "yy": 1.0, "zz": 1.0}}),
        serde_json::json!({"physical_group": "sub", "mu_r_diag": {"xx": 1.0, "zz": 1.0}}),
        serde_json::json!({"physical_group": "sub", "mu_r_diag": {"xx": [1.0, 0.0], "yy": 1.0, "zz": 1.0}}),
    ] {
        assert!(
            serde_json::from_value::<crate::spec::MaterialSpec>(bad.clone()).is_err(),
            "{bad}"
        );
    }
}

/// `material_tensors` composes an anisotropic material inside a UPML
/// shell as `ε = diag(ε_r)·Λ`, `ν = Λ⁻¹·diag(1/μ_r)` (issue #760) —
/// diagonal, since the box stretch is — and leaves isotropic tets on
/// the scalar `ε_r·Λ` formula.
#[test]
fn material_tensors_compose_anisotropy_with_upml() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures");
    let mut spec = read_spec(&dir.join("patch_extract_smoke.json")).unwrap();
    spec.materials.push(
        serde_json::from_value(serde_json::json!({
            "physical_group": "upml",
            "eps_r_diag": {"xx": [2.0, -0.1], "yy": [3.0, 0.0], "zz": [4.0, 0.0]},
            "mu_r_diag": {"xx": 1.0, "yy": 2.0, "zz": 5.0}
        }))
        .unwrap(),
    );
    let p = load_parsed(spec, &dir, None).unwrap();
    assert!(p.is_anisotropic() && p.needs_tensor_materials());
    let centroids = tet_centroids(&p.tagged.mesh);
    let k0 = 0.05;
    let (eps_t, nu_t) = p.material_tensors(&p.eps, &centroids, k0);
    let upml_tag = p.tagged.physical_group_tag(3, "upml").unwrap();
    let sub_tag = p.tagged.physical_group_tag(3, "substrate").unwrap();
    let d = [c64::new(2.0, -0.1), c64::new(3.0, 0.0), c64::new(4.0, 0.0)];
    let m = [1.0, 2.0, 5.0];
    let (mut n_shell, mut n_sub) = (0, 0);
    for (t, &tag) in p.tagged.tet_physical_tags.iter().enumerate() {
        let r = &p.upml[0];
        let (lam, lam_inv) =
            box_upml_tensors(centroids[t], r.air_lo, r.air_hi, r.thickness, r.sigma_0, k0);
        for i in 0..3 {
            for j in 0..3 {
                if tag == upml_tag {
                    assert_eq!(eps_t[t][i][j], d[i] * lam[i][j]);
                    assert_eq!(nu_t[t][i][j], lam_inv[i][j] * (1.0 / m[j]));
                    if i != j {
                        assert_eq!(eps_t[t][i][j], c64::new(0.0, 0.0));
                    }
                } else if tag == sub_tag && p.upml_of_tet[t].is_none() {
                    let want = if i == j { p.eps[t] } else { c64::new(0.0, 0.0) };
                    assert_eq!(eps_t[t][i][j], want);
                    assert_eq!(nu_t[t][i][j], lam_inv[i][j]);
                }
            }
        }
        n_shell += usize::from(tag == upml_tag);
        n_sub += usize::from(tag == sub_tag);
    }
    assert!(n_shell > 0 && n_sub > 0);
    // The stretch really is complex inside the shell (the composition
    // is not a no-op).
    assert!(
        p.tagged
            .tet_physical_tags
            .iter()
            .zip(&eps_t)
            .any(|(&tag, e)| tag == upml_tag && e[0][0].im < -0.2)
    );
}

#[test]
fn hex_is_lowercase_two_digit() {
    assert_eq!(hex(&[0x00, 0xab, 0x0f]), "00ab0f");
}
