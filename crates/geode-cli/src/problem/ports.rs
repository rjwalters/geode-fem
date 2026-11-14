//! Moved out of `problem/mod.rs` (issue #1398): pure move, no logic change.

use super::*;

/// The resolved per-tet materials a wave port's fill is read from
/// ([`PortMaterials::resolve_fill`], issue #777).
pub(super) struct PortMaterials<'a> {
    pub(super) tagged: &'a TaggedTetMesh,
    pub(super) eps: &'a [c64],
    pub(super) eps_diag: &'a [Option<[c64; 3]>],
    pub(super) mu_diag: &'a [Option<[f64; 3]>],
    pub(super) mu_r: &'a [f64],
    pub(super) dispersion: &'a [DispersiveRegion],
    pub(super) frequencies: &'a [Frequency],
    pub(super) upml: &'a [UpmlRegion],
    pub(super) upml_of_tet: &'a [Option<usize>],
}

impl PortMaterials<'_> {
    fn group_name(&self, tag: i32) -> String {
        self.tagged
            .mesh
            .physical_groups
            .get(&(3, tag))
            .cloned()
            .unwrap_or_else(|| "<untagged>".to_string())
    }

    /// Tet `t`'s `ε_r` tensor diagonal (`[ε; 3]` for a scalar material),
    /// at `hz` for a dispersive one (`None`: its reference `ε_r`).
    fn eps_diag_of(&self, t: usize, hz: Option<f64>) -> [c64; 3] {
        if let Some(d) = self.eps_diag.get(t).copied().flatten() {
            return d;
        }
        let tag = self.tagged.tet_physical_tags[t];
        match (hz, self.dispersion.iter().find(|d| d.tag == tag)) {
            (Some(hz), Some(d)) => [d.model.eps(hz); 3],
            _ => [self.eps[t]; 3],
        }
    }

    /// Tet `t`'s `μ_r` tensor diagonal.
    fn mu_diag_of(&self, t: usize) -> [f64; 3] {
        self.mu_diag
            .get(t)
            .copied()
            .flatten()
            .unwrap_or([self.mu_r[t]; 3])
    }

    /// Whether tets `a` and `b` carry the same material by value (at every
    /// sweep frequency for a dispersive one).
    fn same_material(&self, a: usize, b: usize) -> bool {
        if self.mu_diag_of(a) != self.mu_diag_of(b)
            || self.eps_diag_of(a, None) != self.eps_diag_of(b, None)
        {
            return false;
        }
        self.dispersion.is_empty()
            || self
                .frequencies
                .iter()
                .all(|f| self.eps_diag_of(a, Some(f.hz)) == self.eps_diag_of(b, Some(f.hz)))
    }

    /// The homogeneous fill of wave port `surf` (unit normal `normal`):
    /// `invalid_spec` if a touching tet is a stretched UPML tet, if the
    /// touching tets differ in material (an inhomogeneous cross-section,
    /// issue #778), if `ε` / `μ` is not isotropic in the port plane, or
    /// if `Re ε_t·μ_n ≤ 0` at any sweep frequency (no propagating mode,
    /// issue #781).
    pub(super) fn resolve_fill(
        &self,
        surf: &Surface,
        normal: [f64; 3],
    ) -> Result<PortFill, CliError> {
        let name = &surf.name;
        let keys: std::collections::HashSet<[u32; 3]> =
            surf.triangles.iter().map(sorted3).collect();
        // Every tet with a face on the port (one per boundary face, two
        // per face of an internal port plane).
        let mut touching: Vec<usize> = Vec::new();
        for (t, &[a, b, c, d]) in self.tagged.mesh.tets.iter().enumerate() {
            if [[b, c, d], [a, c, d], [a, b, d], [a, b, c]]
                .iter()
                .any(|f| keys.contains(&sorted3(f)))
            {
                touching.push(t);
            }
        }
        let tet = *touching
            .first()
            .expect("a wave-port surface's triangles are tet faces (checked on resolution)");

        // A stretched absorbing-region tet is not a port medium.
        for &t in &touching {
            if let Some(i) = self.upml_of_tet.get(t).copied().flatten() {
                let r = &self.upml[i];
                let nodes = self.tagged.mesh.tets[t].map(|n| self.tagged.mesh.nodes[n as usize]);
                let c: [f64; 3] =
                    std::array::from_fn(|k| nodes.iter().map(|q| q[k]).sum::<f64>() / 4.0);
                if (0..3).any(|k| c[k] < r.air_lo[k] || c[k] > r.air_hi[k]) {
                    return Err(invalid(format!(
                        "wave port `{name}` touches the absorbing region `{}`: a stretched UPML \
                         medium is not a waveguide port medium — move the port face inside the \
                         region's inner wall (its air box)",
                        r.name
                    )));
                }
            }
        }

        // One material by value across every touching tet.
        let mut tags: BTreeMap<i32, usize> = BTreeMap::new();
        for &t in &touching {
            tags.entry(self.tagged.tet_physical_tags[t]).or_insert(t);
        }
        let groups: Vec<String> = {
            let mut g: Vec<String> = tags.keys().map(|&tag| self.group_name(tag)).collect();
            g.sort();
            g
        };
        if tags.values().any(|&t| !self.same_material(tet, t)) {
            // An inhomogeneous cross-section: the hybrid path (issue #807),
            // which models isotropic, non-magnetic fills only.
            self.check_hybrid_fill(
                name,
                &format!(
                    "its face touches more than one material: {}",
                    groups
                        .iter()
                        .map(|g| format!("`{g}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
                &tags,
            )?;
            return Ok(PortFill {
                tet,
                groups,
                normal_axis: None,
                inhomogeneous: true,
            });
        }

        // Transverse isotropy of the (diagonal) tensors.
        let eps_d = self.eps_diag_of(tet, None);
        let mu_d = self.mu_diag_of(tet);
        let isotropic = eps_d[1] == eps_d[0]
            && eps_d[2] == eps_d[0]
            && mu_d[1] == mu_d[0]
            && mu_d[2] == mu_d[0];
        let normal_axis = if isotropic {
            None
        } else {
            let fill = groups.join(", ");
            let Some(k) = (0..3).find(|&k| normal[k].abs() > 1.0 - 1e-9) else {
                return Err(invalid(format!(
                    "wave port `{name}`: the fill `{fill}` is anisotropic and the port normal \
                     [{:.6}, {:.6}, {:.6}] is not a coordinate axis, so the diagonal tensors are \
                     not isotropic in the port plane (hybrid modes, issue #778)",
                    normal[0], normal[1], normal[2]
                )));
            };
            const AXES: [&str; 3] = ["xx", "yy", "zz"];
            let (i, j) = ((k + 1) % 3, (k + 2) % 3);
            for (what, unequal) in [
                ("eps_r_diag", eps_d[i] != eps_d[j]),
                ("mu_r_diag", mu_d[i] != mu_d[j]),
            ] {
                if unequal {
                    return Err(invalid(format!(
                        "wave port `{name}`: the fill `{fill}` has unequal transverse \
                         {what}.{} and {what}.{} on a port normal to {}: a wave port needs a \
                         medium that is isotropic in the port plane (unequal transverse \
                         components hybridize the modes, issue #778)",
                        AXES[i],
                        AXES[j],
                        ["x", "y", "z"][k]
                    )));
                }
            }
            Some(k)
        };

        // A propagating TE mode needs Re ε_t·μ_n > 0 (the filled cutoff
        // k_c/√(Re ε_t·μ_n) is real): a plasma-like fill has none, and its
        // `cutoff_hz` would be NaN. Checked at every sweep frequency, since
        // a dispersive (e.g. Drude) fill can cross zero inside the sweep;
        // a non-dispersive fill evaluates to its constant ε here.
        let (_, mu_n) = transverse_and_normal(mu_d, normal_axis);
        let hzs: Vec<Option<f64>> = if self.frequencies.is_empty() {
            vec![None]
        } else {
            self.frequencies.iter().map(|f| Some(f.hz)).collect()
        };
        for hz in hzs {
            let (eps_t, _) = transverse_and_normal(self.eps_diag_of(tet, hz), normal_axis);
            let re = eps_t.re * mu_n;
            if re.is_nan() || re <= 0.0 {
                let at = hz.map_or_else(String::new, |hz| format!(" at {hz:e} Hz"));
                return Err(invalid(format!(
                    "wave port `{name}`: the fill `{}` has Re ε_t·μ_n = {re:e} ≤ 0{at} (ε_t = \
                     {:e}{:+e}j): a plasma-like fill carries no propagating mode and has no real \
                     cutoff, which wave ports do not support (issue #781) — fill the guide at the \
                     port face with a medium of Re ε > 0 over the whole sweep",
                    groups.join(", "),
                    eps_t.re,
                    eps_t.im
                )));
            }
        }

        Ok(PortFill {
            tet,
            groups,
            normal_axis,
            inhomogeneous: false,
        })
    }

    /// The fill rules of a **hybrid** port face (issue #807) over the
    /// touching tets `tags` (one representative tet per volume tag): every
    /// material isotropic with `μ_r = 1` (the mixed port pencil has a scalar
    /// `ε` and no permeability), and `Re ε_r > 0` at every sweep frequency
    /// (a plasma-like triangle has no port-mode meaning).
    fn check_hybrid_fill(
        &self,
        name: &str,
        why: &str,
        tags: &BTreeMap<i32, usize>,
    ) -> Result<(), CliError> {
        for (&tag, &t) in tags {
            let group = self.group_name(tag);
            if self.mu_diag.get(t).copied().flatten().is_some() || self.mu_r[t] != 1.0 {
                return Err(invalid(format!(
                    "wave port `{name}` is a hybrid port ({why}) and the fill `{group}` has \
                     μ_r ≠ 1: the hybrid port pencil is non-magnetic (μ_r = 1, Epic #778 scope) \
                     — use μ_r = 1 at the port face"
                )));
            }
            if self.eps_diag.get(t).copied().flatten().is_some() {
                return Err(invalid(format!(
                    "wave port `{name}` is a hybrid port ({why}) and the fill `{group}` is \
                     anisotropic (eps_r_diag): anisotropic ε on a hybrid port face is not \
                     supported (Epic #778 scope) — use an isotropic eps_r at the port face"
                )));
            }
            let hzs: Vec<Option<f64>> = if self.frequencies.is_empty() {
                vec![None]
            } else {
                self.frequencies.iter().map(|f| Some(f.hz)).collect()
            };
            for hz in hzs {
                let e = self.eps_diag_of(t, hz)[0];
                if e.re.is_nan() || e.re <= 0.0 {
                    let at = hz.map_or_else(String::new, |hz| format!(" at {hz:e} Hz"));
                    return Err(invalid(format!(
                        "wave port `{name}`: the hybrid port face's fill `{group}` has Re ε_r = \
                         {:e} ≤ 0{at}: a plasma-like fill has no port-mode meaning (issue #781) \
                         — use a medium of Re ε > 0 at the port face over the whole sweep",
                        e.re
                    )));
                }
            }
        }
        Ok(())
    }

    /// Route wave port `w` (spec entry `spec`): `Some` hybrid definition for
    /// an inhomogeneous face or a face with a floating conductor (issue
    /// #807), `None` for a geometric (homogeneous TE) port, which keeps the
    /// #777 path bit for bit. `edges` / `pec_mask` are the volume's edge
    /// table and PEC mask: a zero-thickness PEC sheet crossing the face
    /// becomes a conductor of the face through them.
    pub(super) fn route_hybrid(
        &self,
        w: &WavePortDef,
        spec: &WavePortSpec,
        edges: &[[u32; 2]],
        pec_mask: &[bool],
    ) -> Result<Option<HybridPortDef>, CliError> {
        let name = &w.surface.name;
        let mesh = &self.tagged.mesh;
        let faces = &w.surface.triangles;
        let face_err = |e: geode_core::driven::ports::PortFaceError| {
            invalid(format!("wave port `{name}`: hybrid port face: {e}"))
        };
        // Floating conductors (the geometry alone decides them).
        let ones = vec![1.0; mesh.n_tets()];
        let probe = HybridPortFace::from_volume(mesh, faces, &ones)
            .and_then(|f| f.with_interior_pec(edges, pec_mask));
        let n_conductors = match probe {
            Ok(f) => f.conductors.len(),
            // A homogeneous face the hybrid face cannot even be built on (e.g.
            // every face edge PEC) stays on the geometric path, unchanged.
            Err(_) if !w.fill.inhomogeneous => 0,
            Err(e) => return Err(face_err(e)),
        };
        let route = match (w.fill.inhomogeneous, n_conductors > 0) {
            (false, false) => return Ok(None),
            (true, false) => HybridRoute::Inhomogeneous,
            (false, true) => HybridRoute::InteriorConductor,
            (true, true) => HybridRoute::InhomogeneousWithInteriorConductor,
        };
        // Per face triangle, every tet it bounds (two on an internal plane).
        let keys: BTreeMap<[u32; 3], usize> = faces
            .iter()
            .enumerate()
            .map(|(i, t)| (sorted3(t), i))
            .collect();
        let mut tri_tets: Vec<Vec<usize>> = vec![Vec::new(); faces.len()];
        for (t, &[a, b, c, d]) in mesh.tets.iter().enumerate() {
            for f in [[b, c, d], [a, c, d], [a, b, d], [a, b, c]] {
                if let Some(&i) = keys.get(&sorted3(&f)) {
                    tri_tets[i].push(t);
                }
            }
        }
        let mut tags: BTreeMap<i32, usize> = BTreeMap::new();
        for tets in &tri_tets {
            for &t in tets {
                tags.entry(self.tagged.tet_physical_tags[t]).or_insert(t);
            }
            if let [t0, t1] = tets[..]
                && !self.same_material(t0, t1)
            {
                return Err(invalid(format!(
                    "wave port `{name}` is an internal port plane whose two sides differ in \
                     material (`{}` and `{}`) on the same face triangle: the hybrid port face \
                     takes one permittivity per triangle — put the port plane where both sides \
                     carry the same material",
                    self.group_name(self.tagged.tet_physical_tags[t0]),
                    self.group_name(self.tagged.tet_physical_tags[t1])
                )));
            }
        }
        if route == HybridRoute::InteriorConductor {
            self.check_hybrid_fill(
                name,
                &format!("its face carries {n_conductors} floating conductor(s)"),
                &tags,
            )?;
        }
        let dispersive = tags
            .keys()
            .any(|tag| self.dispersion.iter().any(|d| d.tag == *tag));
        let lossy = dispersive || tags.values().any(|&t| self.eps[t].im != 0.0);
        let face = if lossy {
            HybridPortFace::from_volume_lossy(mesh, faces, self.eps)
        } else {
            let re: Vec<f64> = self.eps.iter().map(|e| e.re).collect();
            HybridPortFace::from_volume(mesh, faces, &re)
        }
        .and_then(|f| f.with_interior_pec(edges, pec_mask))
        .map_err(face_err)?;

        // Options, with the CLI defaults (spec docs).
        let hs = spec.hybrid.unwrap_or_default();
        let k = w.a_inc.len();
        let max = face.max_modes();
        if k > max {
            return Err(invalid(format!(
                "wave port `{name}`: n_modes = {k} but the hybrid port face holds only {max} \
                 physical mode(s) (its free transverse DOF count) — refine the port face or \
                 lower n_modes"
            )));
        }
        let (n_term, termination_clamped) = match hs.n_termination_evanescent {
            Some(n) if k + n > max => {
                return Err(invalid(format!(
                    "wave port `{name}`: n_modes = {k} plus hybrid.n_termination_evanescent = \
                     {n} exceeds the {max} physical mode(s) the hybrid port face holds — refine \
                     the port face or request fewer"
                )));
            }
            Some(n) => (n, false),
            None => {
                let n = DEFAULT_N_TERMINATION_EVANESCENT.min(max - k);
                (n, n < DEFAULT_N_TERMINATION_EVANESCENT)
            }
        };
        let accuracy_on = hs.accuracy.unwrap_or(true);
        for (set, field) in [
            (hs.accuracy_threshold.is_some(), "accuracy_threshold"),
            (
                hs.impedance_accuracy_threshold.is_some(),
                "impedance_accuracy_threshold",
            ),
        ] {
            if !accuracy_on && set {
                return Err(invalid(format!(
                    "wave_ports[{name}].hybrid.{field} is set but hybrid.accuracy is false: the \
                     threshold applies to the accuracy estimate — drop one of them"
                )));
            }
        }
        let threshold = hs
            .accuracy_threshold
            .unwrap_or(geode_core::analytic::port_mode_accuracy::DEFAULT_ACCURACY_THRESHOLD);

        let impedance_definition = if face.conductors.is_empty() {
            if let Some(d) = spec.impedance_definition {
                return Err(invalid(format!(
                    "wave_ports[{name}].impedance_definition = \"{}\" needs a floating conductor \
                     on the port face (a line impedance is defined by its current / voltage), \
                     but this hybrid face has none (a partially filled waveguide) — drop \
                     impedance_definition; the modal S in the JSON report needs none",
                    d.name()
                )));
            }
            None
        } else {
            let d = spec
                .impedance_definition
                .unwrap_or(ImpedanceDefinition::PowerCurrent);
            if d != ImpedanceDefinition::PowerCurrent
                && let Some(c) = face
                    .conductors
                    .iter()
                    .position(|c| c.voltage_path.is_empty())
            {
                return Err(invalid(format!(
                    "wave_ports[{name}].impedance_definition = \"{}\" needs a voltage path from \
                     the shield to every conductor, but conductor {c} cannot be reached without \
                     crossing another conductor — use \"power_current\" (contour-independent)",
                    d.name()
                )));
            }
            Some(d)
        };
        let opts = HybridWavePortOpts {
            n_termination_evanescent: n_term,
            accuracy: accuracy_on.then_some(PortAccuracyOpts {
                threshold,
                observe_rate: true,
                // The attenuation estimate warns at the same threshold (#819
                // review).
                alpha_threshold: Some(threshold),
                // The line impedance `--touchstone` renormalizes to (#807
                // review): its own, looser default (`Z` converges at the
                // strip-edge singular rate, not O(h²)).
                impedance_threshold: Some(
                    hs.impedance_accuracy_threshold
                        .unwrap_or(DEFAULT_IMPEDANCE_ACCURACY_THRESHOLD),
                ),
                impedance: impedance_definition
                    .map_or(LineImpedance::PowerCurrent, crate::hybrid::line_impedance),
            }),
            ..HybridWavePortOpts::default()
        };
        Ok(Some(HybridPortDef {
            route,
            face,
            lossy,
            dispersive,
            opts,
            termination_clamped,
            impedance_definition,
        }))
    }

    /// The TE-only wave-port guard (issue #808): reject a port whose rim is
    /// not entirely on a conductor wall, and a sweep that reaches the
    /// port's lowest **TM** mode. Returns the face's TM-cutoff estimate.
    ///
    /// This applies to the homogeneous TE wave ports this loader builds;
    /// a port family with its own TM / hybrid modes (#778, #804) must not
    /// be routed through it.
    ///
    /// **Rim.** The port modes are solved with a PEC rim (`n × E = 0` and,
    /// for TM, `E_z = 0` on every rim edge). On a rim edge that lies on no
    /// `pec` / `leontovich` wall (a PMC symmetry plane, an unnamed natural
    /// surface) they are the modes of the wrong cross-section (a half guide
    /// with a PMC centre plane gets the full guide's TE₂₀, not TE₁₀), so S
    /// is silently wrong: rejected with `invalid_spec`, naming the edge
    /// count and the surface groups they lie on.
    ///
    /// **TM cutoff.** The port modal solve returns TE modes only, so a
    /// propagating TM channel has no termination: it reflects off the port
    /// face and the S-matrix is silently wrong (measured in `geode-core`'s
    /// `te_only_ports_above_tm11_give_length_dependent_s`: |S| of a height
    /// step moves by 0.44 with the feed length above TM₁₁, by 3.6e-3
    /// below). The threshold is [`TmCutoffEstimate::guard_k_c`]: the face
    /// P1 Dirichlet `E_z` eigenvalue Richardson-extrapolated over two
    /// uniform face refinements, less a margin `δ`. The face value alone
    /// is not safe: it is a Rayleigh-Ritz **upper** bound, and the 3-D
    /// Nédélec model's own TM cutoff sits below the continuum by numerical
    /// dispersion along the guide, by up to `≈ 0.0205·(k_c·h_n)²` on the
    /// meshes measured (issue #824, see `TM_GUARD_MARGIN`). `h_n` is the
    /// largest axial extent of a tet of the guide feeding the port, over
    /// its face and starting within three TM-cutoff wavelengths of the
    /// port plane ([`PortFaceProjection::guide_axial_mesh`] over
    /// [`tm_guard_axial_reach`] of `k_c` and the top in-fill sweep
    /// wavenumber; issue #845). It is not read off the face-adjacent tets
    /// alone: a fine layer at the port over coarser cells behind it would
    /// hide the coarse cells that set the 3-D cutoff. So
    /// `δ = max(TM_GUARD_MARGIN, TM_GUARD_AXIAL_COEFF·(k_c·h_n)²)`: 5 % up
    /// to `k_c·h_n ≈ 1.41`, wider on a coarser axial mesh, and a rejection
    /// then says how far to refine the guide (the whole window, not only
    /// the cells at the face) to admit the sweep. The margin covers the
    /// 3-D cutoff **only for boxes shorter than the window**: every box
    /// it was measured on was. On guides longer than the window, coarse
    /// ones, the guard is not a bound (issue #1005: at or above a p=1
    /// TM-like mode on 12 of 216 long Gmsh guides (20 by the sampled share), by up to 198.7 %;
    /// `tm_guard_margin`, "Long guides: where the guard fails"). The
    /// filled limit is `guard_k_c/√(Re ε_n·μ_t)` ([`PortMedium::tm_cutoff_k0`]),
    /// evaluated at every sweep frequency for a dispersive fill; a fill
    /// with `Re ε_n·μ_t ≤ 0` has no TM cutoff and is rejected outright.
    ///
    /// **Coarse axial mesh (warning).** Returned alongside the estimate
    /// when the top sweep frequency has `k·h_n > √(δ₀/C_h)` ≈ 1.41 in the
    /// fill (fewer than ~4.4 cells per wavelength along the guide): the
    /// sweep is admitted under the widened margin, and the warning names
    /// the spacing to refine to.
    ///
    /// **Coarser guide beyond the window (warning, issue #845).** A tet
    /// over the face further out than the window, coarse enough that its
    /// own guard (`h_n` read there) is at or below the top sweep
    /// wavenumber, can carry a 3-D TM mode the port cannot terminate. Its
    /// field reaches the port through the window's finer guide, where it
    /// is evanescent, at about `exp(−α·d)` (`tm_evanescent_leak`, `α`
    /// from the window's guard, `d` the distance of the nearest coarser
    /// tet). Above `TM_FAR_LEAK_WARN` (1 %) the sweep runs with a warning
    /// naming where the coarse section starts, the spacing to refine it
    /// to, and the frequency below which the leak is under 1 %. A
    /// rejection would be wrong here: the coarse cells may be a device
    /// region rather than the guide, and the leak is bounded.
    ///
    /// **Long, coarse guide (warning, issue #1005).** When the window's
    /// `k_c^TM·h_n` is above [`TM_GUARD_LONG_GUIDE_KH`] (2.5) and the
    /// mesh over the face extends beyond the window
    /// ([`GuideScan::far_extent`]), the sweep runs with a warning that the
    /// TM limit is not a bound there, how to leave the regime (refine the
    /// guide to `h ≤ 2.5/k_c`, or shorten it), and that the threshold marks
    /// the regime of the measured misses rather than predicting one.
    /// The operator ruled for a warning, not a rejection: tightening the
    /// margin would block meshes that solve today.
    pub(super) fn check_te_port_guard(
        &self,
        surf: &Surface,
        projection: &PortFaceProjection,
        fill: &PortFill,
        conductor_edges: &std::collections::HashSet<[u32; 2]>,
    ) -> Result<(TmCutoffEstimate, Option<String>), CliError> {
        let name = &surf.name;
        let pointer = "wave ports carry TE modes only, solved with a PEC rim (TM / hybrid port \
                       modes and other rim conditions are issues #778, #804)";
        let open: Vec<[u32; 2]> = projection
            .global_edges
            .iter()
            .zip(&projection.interior_edge_mask)
            .filter(|&(e, &interior)| !interior && !conductor_edges.contains(e))
            .map(|(e, _)| *e)
            .collect();
        if !open.is_empty() {
            let n_rim = projection
                .interior_edge_mask
                .iter()
                .filter(|&&i| !i)
                .count();
            let open_set: std::collections::HashSet<[u32; 2]> = open.iter().copied().collect();
            // The groups of the walls the open rim lies on: tagged
            // triangles with an open rim edge, other than the port's own.
            let sorted = |t: &[u32; 3]| {
                let mut t = *t;
                t.sort_unstable();
                t
            };
            let port_tris: std::collections::HashSet<[u32; 3]> =
                surf.triangles.iter().map(sorted).collect();
            let mut groups = std::collections::BTreeSet::new();
            for (t, &tag) in self
                .tagged
                .boundary_triangles
                .iter()
                .zip(&self.tagged.triangle_physical_tags)
            {
                if !port_tris.contains(&sorted(t))
                    && tri_edge_keys(t).iter().any(|e| open_set.contains(e))
                {
                    groups.insert(
                        self.tagged
                            .mesh
                            .physical_groups
                            .get(&(2, tag))
                            .cloned()
                            .unwrap_or_else(|| format!("tag {tag}")),
                    );
                }
            }
            let on = if groups.is_empty() {
                "they lie on no tagged surface (a natural / PMC-like boundary)".to_string()
            } else {
                format!(
                    "they lie on {}",
                    groups
                        .iter()
                        .map(|g| format!("`{g}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            let node = |n: u32| {
                let p = self.tagged.mesh.nodes[n as usize];
                format!("({:e}, {:e}, {:e})", p[0], p[1], p[2])
            };
            return Err(invalid(format!(
                "wave port `{name}`: {} of its {n_rim} rim edges are on no `pec` or \
                 `leontovich` wall ({on}; first edge {} – {}). The {pointer}, so on any other \
                 rim the port modes belong to the wrong cross-section and the S-parameters \
                 would be silently wrong — put the whole port rim on a pec or leontovich wall",
                open.len(),
                node(open[0][0]),
                node(open[0][1]),
            )));
        }
        let face_est = projection.tm_cutoff_estimate(None).map_err(|e| {
            invalid(format!(
                "wave port `{name}`: the TM cutoff of its cross-section could not be \
                 computed: {e}"
            ))
        })?;
        let mu_t = transverse_and_normal(self.mu_diag_of(fill.tet), fill.normal_axis).0;
        let medium = PortMedium {
            eps_t: c64::new(1.0, 0.0),
            mu_t,
            mu_n: 1.0,
        };
        // Per frequency: the axial ε_n and the geometric wavenumber in the
        // fill, k₀·√(Re ε_n·μ_t) (what the TM cutoff and the axial mesh
        // resolution are measured against).
        let mut rows = Vec::with_capacity(self.frequencies.len());
        for f in self.frequencies {
            let eps_n =
                transverse_and_normal(self.eps_diag_of(fill.tet, Some(f.hz)), fill.normal_axis).1;
            let d = eps_n.re * mu_t;
            if d.is_nan() || d <= 0.0 {
                return Err(invalid(format!(
                    "wave port `{name}`: the fill `{}` has Re ε_n·μ_t = {:e} ≤ 0 at {:e} Hz \
                     (axial ε_n = {:e}{:+e}j), so the port's TM modes propagate at every \
                     frequency (no TM cutoff), but {pointer} — fill the guide at the port face \
                     with a medium of Re ε_n > 0 over the whole sweep",
                    fill.groups.join(", "),
                    d,
                    f.hz,
                    eps_n.re,
                    eps_n.im
                )));
            }
            rows.push((f, eps_n, f.k0 * d.sqrt()));
        }
        // The top of the sweep in the fill (a dispersive fill need not
        // peak at the top frequency).
        let top = rows.iter().copied().max_by(|a, b| a.2.total_cmp(&b.2));
        // `h_n` is read over three TM-cutoff wavelengths of the guide
        // feeding the port, not just the tets on the face: the 3-D TM
        // cutoff is set by the guide's coarsest cells (issues #824, #845).
        let reach = tm_guard_axial_reach(face_est.k_c(), top.map_or(0.0, |r| r.2));
        // One pass over the guide's tets serves the window reading and every
        // "where the coarser cells start" lookup of the messages (issue #848).
        let scan = projection.guide_scan(&self.tagged.mesh);
        let guide = scan.axial_mesh(reach);
        let est = face_est.with_axial_spacing(guide.spacing);
        let k_c = est.guard_k_c();
        let axial = TmAxialMesh {
            name,
            est: &est,
            guide,
            scan: &scan,
            reach,
            top: top.map(|(f, _, k)| (f.hz, k)),
            limit_scale_hz: top.map(|(f, _, k)| f.hz / k),
        };
        for (f, eps_n, _) in rows {
            let limit_k0 = medium
                .tm_cutoff_k0(eps_n, k_c)
                .expect("Re ε_n·μ_t > 0 checked above");
            if f.k0 >= limit_k0 {
                let limit_hz = limit_k0 * f.hz / f.k0;
                let filled = if eps_n == c64::new(1.0, 0.0) && mu_t == 1.0 {
                    String::new()
                } else {
                    format!(
                        " in the fill `{}`, scaled by 1/√(Re ε_n·μ_t) with ε_n = {:e}{:+e}j, \
                         μ_t = {mu_t:e}",
                        fill.groups.join(", "),
                        eps_n.re,
                        eps_n.im
                    )
                };
                return Err(invalid(format!(
                    "wave port `{name}`: the sweep frequency {:e} Hz (k0 = {:.6}) is at or above \
                     the port's TM limit {limit_hz:e} Hz (k0 = {limit_k0:.6}{filled}): {} % \
                     below its lowest TM cutoff, estimated at k_c^TM = {:.6} per mesh unit \
                     (port-face value {:.6}, extrapolated over two face refinements; the \
                     margin covers the 3-D model's TM cutoff, which sits below the continuum \
                     on a coarse axial mesh{}). The {pointer}, so a propagating TM mode has no \
                     port termination and the S-parameters would be silently wrong — {}",
                    f.hz,
                    f.k0,
                    percent(est.margin()),
                    est.k_c(),
                    est.k_face,
                    axial.margin_note(),
                    axial.rejection_remedy(limit_hz),
                )));
            }
        }
        let warnings: Vec<String> = [
            axial.warning(),
            axial.far_warning(),
            axial.long_guide_warning(),
        ]
        .into_iter()
        .flatten()
        .collect();
        Ok((est, (!warnings.is_empty()).then(|| warnings.join("\n"))))
    }
}

/// The evanescent leak above which a coarser guide section beyond the TE
/// guard's window is warned about (issue #845): 1 % of the TM field's
/// amplitude reaching the port, the order of the S error it causes.
pub(super) const TM_FAR_LEAK_WARN: f64 = 0.01;

/// The measured worst-case coefficient of the 3-D model's TM cutoff
/// undershoot, `≈ 0.0205·(k_c·h_n)²` (issue #824: structured meshes
/// uniform along the guide; graded, stepped and unstructured meshes
/// measured lower with `h_n` read over the guide; `TM_GUARD_AXIAL_COEFF`
/// bounds it with headroom), quoted by the coarse-axial-mesh warning.
pub(super) const TM_AXIAL_UNDERSHOOT_MEASURED: f64 = 0.0205;

/// `s` with its first character upper-cased.
pub(super) fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map_or(String::new(), |f| f.to_uppercase().chain(c).collect())
}

/// `x` as a percentage with at most one decimal (`0.05` → `5`,
/// `0.0771` → `7.7`).
pub(super) fn percent(x: f64) -> String {
    let s = format!("{:.1}", 100.0 * x);
    s.strip_suffix(".0").map(str::to_string).unwrap_or(s)
}

/// The axial-mesh side of the TE-only TM guard of one wave port (issue
/// #824): the margin note, the rejection remedy and the coarse-mesh
/// warning, all in terms of `h_n` (the largest axial extent of a tet of
/// the guide within `reach` of the port) and `k·h_n`.
pub(super) struct TmAxialMesh<'a> {
    name: &'a str,
    est: &'a TmCutoffEstimate,
    /// The guide's axial mesh over and beyond the window.
    guide: GuideAxialMesh,
    /// The guide's tets, scanned once (`PortFaceProjection::guide_scan`):
    /// [`GuideScan::coarser_than_distance`] gives how near the port the
    /// first tet coarser than `h` starts, where a refinement to `h` has to
    /// begin.
    scan: &'a GuideScan,
    /// The axial window `h_n` was read over (mesh units, either side of
    /// the port plane; `tm_guard_axial_reach`).
    reach: f64,
    /// `(Hz, geometric k in the fill)` of the sweep point with the largest
    /// in-fill wavenumber; `None` for an empty sweep.
    top: Option<(f64, f64)>,
    /// Hz per unit of in-fill wavenumber at that point (maps a geometric
    /// wavenumber to a frequency).
    limit_scale_hz: Option<f64>,
}

impl TmAxialMesh<'_> {
    /// Whether the axial term widened the margin beyond `TM_GUARD_MARGIN`.
    fn widened(&self) -> bool {
        self.est.margin() > TM_GUARD_MARGIN
    }

    /// `"the tets coarser than h start D mesh units from the port"`, or
    /// `None` when there are none (where a refinement to `h` begins).
    fn starts(&self, h: f64) -> Option<String> {
        self.scan
            .coarser_than_distance(h)
            .map(|d| format!("the tets coarser than that start {d:.6} mesh units from the port"))
    }

    fn margin_note(&self) -> String {
        if !self.widened() {
            return String::new();
        }
        format!(
            "; widened from {} % because the tets of the guide within {:.6} mesh units of the \
             port (the guard's window) span up to h_n = {:.6} mesh units along it, \
             k_c^TM·h_n = {:.3}{}",
            percent(TM_GUARD_MARGIN),
            self.reach,
            self.est.axial_spacing,
            self.est.axial_kh(),
            if self.est.axial_kh() > TM_GUARD_MEASURED_KH {
                format!(", beyond the measured range k_c^TM·h_n ≤ {TM_GUARD_MEASURED_KH}")
            } else {
                String::new()
            }
        )
    }

    fn rejection_remedy(&self, limit_hz: f64) -> String {
        let below = format!("keep every sweep frequency below {limit_hz:e} Hz");
        if !self.widened() {
            return below;
        }
        match self
            .top
            .and_then(|(_, k)| self.est.axial_spacing_admitting(k))
        {
            Some(h) => format!(
                "refine the mesh along the guide feeding port `{}` to h ≤ {h:.6} mesh units \
                 along its axis (now {:.6}{}), over the whole guide within {:.6} mesh units of \
                 the port and not only at the face, to raise the limit above the top sweep \
                 frequency, or {below}",
                self.name,
                self.est.axial_spacing,
                self.starts(h).map_or(String::new(), |t| format!("; {t}")),
                self.reach
            ),
            None => format!(
                "{below} (refining the mesh along the guide feeding port `{}`, within {:.6} \
                 mesh units of the port and not only at the face, to h ≤ {:.6} mesh units \
                 along its axis restores the {} % margin, limit {:e} Hz, which the top sweep \
                 frequency is above as well)",
                self.name,
                self.reach,
                self.est.base_margin_axial_spacing(),
                percent(TM_GUARD_MARGIN),
                self.limit_scale_hz
                    .map_or(f64::NAN, |s| s * (1.0 - TM_GUARD_MARGIN) * self.est.k_c()),
            ),
        }
    }

    /// The coarser-guide-beyond-the-window warning (issue #845): a tet over
    /// the face beyond the window, coarse enough that the guard read there
    /// would not admit the top sweep wavenumber `k`, starts `d` mesh units
    /// from the port, and a TM field from it reaches the port at
    /// `exp(−√(g² − k²)·d)` > [`TM_FAR_LEAK_WARN`] through the window's
    /// guide (`g` its guard, below its own 3-D TM cutoff).
    fn far_warning(&self) -> Option<String> {
        let (hz, k) = self.top?;
        let d = self.guide.coarser_distance?;
        let far = self.est.with_axial_spacing(self.guide.far_spacing);
        if far.guard_k_c() > k {
            return None;
        }
        let g = self.est.guard_k_c();
        let leak = tm_evanescent_leak(g, k, d);
        if leak <= TM_FAR_LEAK_WARN {
            return None;
        }
        let scale = self.limit_scale_hz?;
        // The top wavenumber at which the leak falls to TM_FAR_LEAK_WARN.
        let alpha = (1.0 / TM_FAR_LEAK_WARN).ln() / d;
        let keep_below = Some(g * g - alpha * alpha)
            .filter(|x| *x > 0.0)
            .map(|x| format!("keep the sweep below {:e} Hz", scale * x.sqrt()));
        // Refine to the spacing that admits `k`, from where the tets
        // coarser than it start.
        let refine = self.est.axial_spacing_admitting(k).map(|h| {
            let from = self.scan.coarser_than_distance(h).unwrap_or(d);
            format!(
                "if that section is part of the guide feeding port `{}` (not a device region), \
                 refine it to h ≤ {h:.6} mesh units along the port axis from {from:.6} mesh \
                 units out",
                self.name
            )
        });
        let remedy = match (refine, keep_below) {
            (Some(r), Some(b)) => format!(
                " {}, or {b}, where the leak is under {} %.",
                capitalize(&r),
                percent(TM_FAR_LEAK_WARN)
            ),
            (Some(r), None) => format!(" {}.", capitalize(&r)),
            (None, Some(b)) => format!(
                " {}, where the leak is under {} %.",
                capitalize(&b),
                percent(TM_FAR_LEAK_WARN)
            ),
            (None, None) => String::new(),
        };
        Some(format!(
            "wave port `{}`: coarser mesh further along the guide: beyond the guard's window of \
             {:.6} mesh units, tets over the port face from {d:.6} mesh units from the port span \
             up to h = {:.6} mesh units along its axis, coarse enough for the 3-D model to carry \
             a TM mode at or below the top sweep frequency {hz:e} Hz (a guard read there would \
             be at {:e} Hz). The port carries TE modes only and does not terminate that field; \
             it reaches the port through the finer guide in between, where it is evanescent, at \
             up to ~{} % of its amplitude (exp(−α·d), α = {:.4} per mesh unit), so the \
             S-parameters near the top of the sweep can be off by up to as much.{remedy}",
            self.name,
            self.guide.reach,
            self.guide.far_spacing,
            scale * far.guard_k_c(),
            percent(leak),
            (g * g - k * k).max(0.0).sqrt(),
        ))
    }

    /// The long-coarse-guide warning (issue #1005, operator ruling: warn,
    /// do not reject): `k_c^TM·h_n` > [`TM_GUARD_LONG_GUIDE_KH`] and the
    /// guide footprint extends beyond the guard's window
    /// ([`GuideScan::far_extent`] > `reach`). There the p=1 guard is not a
    /// bound: on 12 of 216 (20 by the sampled share) long coarse Gmsh guides it was at or above a
    /// TM-like mode of the 3-D p=1 model. The threshold marks that regime;
    /// it does not predict a failure.
    fn long_guide_warning(&self) -> Option<String> {
        let k_c = self.est.k_c();
        let kh = self.est.axial_kh();
        if !(k_c.is_finite() && k_c > 0.0 && kh > TM_GUARD_LONG_GUIDE_KH) {
            return None;
        }
        let far = self.scan.far_extent();
        if !(self.reach.is_finite() && far > self.reach) {
            return None;
        }
        Some(format!(
            "wave port `{}`: long, coarse guide: the tets of the guide within {:.6} mesh units \
             of the port (the guard's window) span up to h_n = {:.6} mesh units along it, \
             k_c^TM·h_n = {kh:.3}, above {TM_GUARD_LONG_GUIDE_KH}, and the mesh over the port \
             face extends {far:.6} mesh units from the port, beyond that window. If the guide \
             continues that far, the TM limit is not a bound: on 216 long coarse Gmsh guides \
             measured, the p=1 limit was at or above a TM-like (coarse-mesh defect) mode of \
             the 3-D model on 12 (20 by the sampled share), by up to 198.7 % (about 3×; issues #990, #1005). This marks \
             a regime, not a predicted failure: 211 of those 216 guides are above \
             k_c^TM·h_n = {TM_GUARD_LONG_GUIDE_KH} and only 12 failed (20 by the sampled share), long guides finer than \
             that were not measured, and the threshold is 1.3 % below the lowest failing row \
             (2.532; issue #1041 re-checked the 2.5 threshold under the exact classifier and it held). The sweep runs. To leave the regime, refine the \
             mesh along the whole guide feeding port `{}` to h ≤ {:.6} mesh units along its \
             axis, or shorten the guide to within {:.6} mesh units of the port",
            self.name,
            self.reach,
            self.est.axial_spacing,
            self.name,
            TM_GUARD_LONG_GUIDE_KH / k_c,
            self.reach,
        ))
    }

    /// The coarse-axial-mesh warning: the top sweep frequency resolves its
    /// in-fill wavelength with `k·h_n > √(δ₀/C_h)` ≈ 1.41 (fewer than ~4.4
    /// cells per wavelength along the guide), where the 3-D model's TM
    /// cutoff undershoot outgrows the base margin and the port's TE modes
    /// are themselves under-resolved along the guide.
    fn warning(&self) -> Option<String> {
        let (hz, k) = self.top?;
        let h_n = self.est.axial_spacing;
        let k_c = self.est.k_c();
        if !(k_c.is_finite() && k_c > 0.0 && h_n > 0.0) {
            return None;
        }
        let kh_safe = (TM_GUARD_MARGIN / TM_GUARD_AXIAL_COEFF).sqrt();
        if k * h_n <= kh_safe {
            return None;
        }
        let scale = self.limit_scale_hz?;
        Some(format!(
            "wave port `{}`: coarse mesh along the guide at the port: the tets of the guide \
             within {:.6} mesh units of the port (the guard's window) span up to h_n = \
             {h_n:.6} mesh units along it, so k·h_n = {:.3} at the top sweep \
             frequency {hz:e} Hz ({:.1} cells per wavelength in the fill, fewer than the \
             {:.1} up to which the guard's base {} % margin covers the 3-D model's TM cutoff). \
             That cutoff sits up to ~{:.1} % below the continuum here, so the guard margin is \
             widened to {} % (TM limit {:e} Hz instead of {:e} Hz), and the S-parameters near \
             the top of the sweep carry axial discretization error. Refine the mesh along the \
             guide feeding port `{}` to h ≤ {:.6} mesh units along its axis{}, over the whole \
             guide within {:.6} mesh units of the port and not only at the face",
            self.name,
            self.reach,
            k * h_n,
            std::f64::consts::TAU / (k * h_n),
            std::f64::consts::TAU / kh_safe,
            percent(TM_GUARD_MARGIN),
            100.0 * TM_AXIAL_UNDERSHOOT_MEASURED * (k_c * h_n).powi(2),
            percent(self.est.margin()),
            scale * self.est.guard_k_c(),
            scale * (1.0 - TM_GUARD_MARGIN) * k_c,
            self.name,
            kh_safe / k,
            self.starts(kh_safe / k)
                .map_or(String::new(), |t| format!(" ({t})")),
            self.reach,
        ))
    }
}

/// Build the lumped ports: normalise `e_hat`, then derive `width` and
/// `length` from the port face's geometry wherever the spec omits them.
pub(super) fn resolve_lumped_ports(
    specs: &[LumpedPortSpec],
    tags: &[i32],
    tagged: &TaggedTetMesh,
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<Vec<Port>, CliError> {
    let mut ports = Vec::with_capacity(specs.len());
    for (p, t) in specs.iter().zip(tags) {
        let surf = surface(&p.physical_group, *t, "port")?;
        let norm = p.e_hat.iter().map(|x| x * x).sum::<f64>().sqrt();
        let e_hat = [p.e_hat[0] / norm, p.e_hat[1] / norm, p.e_hat[2] / norm];
        let (area, extent) = port_area_and_extent(tagged, &surf.triangles, e_hat);
        let length = p.length.unwrap_or(extent);
        if !(length.is_finite() && length > 0.0) {
            return Err(invalid(format!(
                "port `{}` has zero extent along e_hat — check e_hat or give `length`",
                p.physical_group
            )));
        }
        let width = p.width.unwrap_or(area / length);
        ports.push(Port {
            surface: surf,
            e_hat,
            width,
            length,
            geometry_derived: p.width.is_none() || p.length.is_none(),
            resistance_ohm: p.resistance_ohm,
            v_inc: c64::new(p.v_inc[0], p.v_inc[1]),
        });
    }
    Ok(ports)
}

/// The conductor walls' edge set (PEC / Leontovich): a wave-port rim edge
/// on one pins the TM field `E_z = 0` there (issue #808).
pub(super) fn conductor_edge_set(
    pec: &[Surface],
    leontovich: &[Leontovich],
) -> std::collections::HashSet<[u32; 2]> {
    pec.iter()
        .chain(leontovich.iter().map(|l| &l.surface))
        .flat_map(|s| s.triangles.iter().flat_map(tri_edge_keys))
        .collect()
}

/// Build the wave-port definitions: project each port face to its 2-D
/// cross-section and resolve the material fill behind it.
///
/// `tm` / `tm_warning` and `hybrid` are left unset: both are decided by
/// `route_wave_ports`, which needs the PEC mask this runs before.
pub(super) fn resolve_wave_ports(
    specs: &[WavePortSpec],
    tags: &[i32],
    materials: &PortMaterials<'_>,
    surface: &dyn Fn(&str, i32, &str) -> Result<Surface, CliError>,
) -> Result<Vec<WavePortDef>, CliError> {
    let mut wave_ports = Vec::with_capacity(specs.len());
    for (w, t) in specs.iter().zip(tags) {
        let surf = surface(&w.physical_group, *t, "wave_port")?;
        let projection =
            project_port_face(&materials.tagged.mesh, &surf.triangles).map_err(|e| {
                invalid(format!(
                    "wave port `{}` cannot be projected to a 2-D cross-section: {e}",
                    w.physical_group
                ))
            })?;
        let fill = materials.resolve_fill(&surf, projection.normal)?;
        let a_inc = match &w.a_inc {
            Some(a) => a.iter().map(|&[re, im]| c64::new(re, im)).collect(),
            None => vec![c64::new(1.0, 0.0); w.n_modes],
        };
        wave_ports.push(WavePortDef {
            surface: surf,
            projection,
            a_inc,
            fill,
            // Set below, once the Silver-Müller rim check has run.
            tm: None,
            tm_warning: None,
            reference_ohm: w.reference_ohm,
            // Set below, once the PEC mask is built.
            hybrid: None,
        });
    }
    Ok(wave_ports)
}

/// Route every wave port and run the rim / TM guards, once the PEC mask
/// the routing reads exists.
///
/// Fills in the `hybrid`, `tm` and `tm_warning` that `resolve_wave_ports`
/// left unset. The guards run in a fixed order, because each reports a
/// different message for an overlapping condition: the Silver-Müller rim
/// check first (its message is the specific one for that wall), then the
/// hybrid routing, then the per-route rim guards.
pub(super) fn route_wave_ports(
    spec: &ProblemSpec,
    wave_ports: &mut [WavePortDef],
    materials: &PortMaterials<'_>,
    silver_muller: &[Surface],
    edges: &[[u32; 2]],
    pec_mask: &[bool],
    conductor_edges: &std::collections::HashSet<[u32; 2]>,
) -> Result<(), CliError> {
    check_silver_muller_port_rims(wave_ports, silver_muller, edges, pec_mask)?;
    // Route each wave port (issue #807): an inhomogeneous face, or one with
    // a floating conductor, goes to the hybrid path; the rest stay
    // geometric (homogeneous TE), unchanged.
    for (w, ws) in wave_ports.iter_mut().zip(&spec.wave_ports) {
        w.hybrid = materials.route_hybrid(w, ws, edges, pec_mask)?;
    }
    validate_hybrid_routes(spec, wave_ports, materials)?;
    // Hybrid ports carry TE, TM and hybrid modes and have core's own
    // completeness check, so they bypass the TE-only guard below; their rim
    // must still be all conductor.
    for w in wave_ports.iter().filter(|w| w.hybrid.is_some()) {
        check_hybrid_port_rim(w, conductor_edges)?;
    }
    // TE-only wave ports (issue #808): the rim must be all conductor and
    // no sweep frequency may reach the TM limit. Runs after the
    // Silver-Müller rim check, whose message is more specific for that
    // wall. Geometric (homogeneous TE) ports only: the hybrid ports routed
    // above (#807) bypass this guard, and so its mesh-aware margin (#824).
    for w in wave_ports.iter_mut().filter(|w| w.hybrid.is_none()) {
        let (tm, warning) =
            materials.check_te_port_guard(&w.surface, &w.projection, &w.fill, conductor_edges)?;
        w.tm = Some(tm);
        w.tm_warning = warning;
    }
    Ok(())
}
