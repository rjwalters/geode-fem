//! Independent (test-side) reader for the Touchstone 2.0 files `geode
//! --touchstone` writes, and the round-trip assertion against the JSON
//! report. Shared by `tests/cli.rs` and `tests/spiral_golden.rs` via
//! `#[path]`; deliberately not the crate's own `touchstone::parse`.

#![allow(dead_code)]

/// `(n_ports, reference_ohm, rows[(f_hz, s[row][col])])`.
pub type Parsed = (usize, Vec<f64>, Vec<(f64, Vec<Vec<[f64; 2]>>)>);

/// Parse the keyword / option-line / data subset of Touchstone 2.0.
pub fn parse(text: &str) -> Parsed {
    let (mut n, mut refs, mut nfreq, mut classic2) = (0, Vec::new(), 0, false);
    let (mut data, mut nums) = (false, Vec::<f64>::new());
    for line in text.lines() {
        let line = line.split('!').next().unwrap().trim();
        if line.is_empty() {
            continue;
        }
        if let Some(v) = line.strip_prefix("[Number of Ports]") {
            n = v.trim().parse().unwrap();
        } else if let Some(v) = line.strip_prefix("[Reference]") {
            refs = v.split_whitespace().map(|x| x.parse().unwrap()).collect();
        } else if let Some(v) = line.strip_prefix("[Number of Frequencies]") {
            nfreq = v.trim().parse().unwrap();
        } else if let Some(v) = line.strip_prefix("[Two-Port Data Order]") {
            classic2 = v.trim() == "21_12";
        } else if line == "[Network Data]" {
            data = true;
        } else if line == "[End]" {
            data = false;
        } else if line.starts_with('#') {
            let up = line.to_ascii_uppercase();
            assert!(up.starts_with("# HZ S RI"), "option line {line:?}");
        } else if data {
            nums.extend(line.split_whitespace().map(|x| x.parse::<f64>().unwrap()));
        } else {
            assert!(line.starts_with('['), "stray line {line:?}");
        }
    }
    assert!(text.starts_with('!') || text.starts_with("[Version] 2.0"));
    assert!(text.contains("\n[Version] 2.0\n") || text.starts_with("[Version] 2.0"));
    assert_eq!(refs.len(), n, "[Reference] count");
    let per = 1 + 2 * n * n;
    assert_eq!(nums.len() % per, 0);
    let rows: Vec<_> = nums
        .chunks(per)
        .map(|c| {
            let mut s = vec![vec![[0.0; 2]; n]; n];
            for k in 0..n * n {
                let (i, j) = if n == 2 && classic2 {
                    (k % n, k / n)
                } else {
                    (k / n, k % n)
                };
                s[i][j] = [c[1 + 2 * k], c[2 + 2 * k]];
            }
            (c[0], s)
        })
        .collect();
    assert_eq!(rows.len(), nfreq, "[Number of Frequencies]");
    (n, refs, rows)
}

/// Assert the Touchstone file at `path` carries exactly the report's
/// per-port references and `results[].s`, rows ascending in frequency,
/// and that `touchstone_file` names it with the right sha256.
pub fn assert_round_trip(report: &serde_json::Value, path: &std::path::Path) {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).expect("touchstone file written");
    let tf = &report["touchstone_file"];
    assert_eq!(tf["path"], path.display().to_string());
    assert_eq!(tf["sha256"], format!("{:x}", Sha256::digest(&bytes)));
    let (n, refs, rows) = parse(std::str::from_utf8(&bytes).unwrap());
    let ports = report["ports"].as_array().unwrap();
    assert_eq!(n, ports.len());
    for (r, p) in refs.iter().zip(ports) {
        assert_eq!(*r, p["resistance_ohm"].as_f64().unwrap());
    }
    let mut want: Vec<(f64, Vec<Vec<[f64; 2]>>)> = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            let s = r["s"]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| {
                    row.as_array()
                        .unwrap()
                        .iter()
                        .map(|c| [c[0].as_f64().unwrap(), c[1].as_f64().unwrap()])
                        .collect()
                })
                .collect();
            (r["frequency_hz"].as_f64().unwrap(), s)
        })
        .collect();
    want.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert!(rows.windows(2).all(|w| w[0].0 < w[1].0), "ascending rows");
    assert_eq!(rows.len(), want.len());
    for ((gf, gs), (wf, ws)) in rows.iter().zip(&want) {
        assert_eq!(gf, wf, "frequency");
        for (grow, wrow) in gs.iter().zip(ws) {
            for (g, w) in grow.iter().zip(wrow) {
                // Shortest round-trip formatting on both sides: exact.
                let d = (g[0] - w[0]).hypot(g[1] - w[1]);
                assert!(d <= 1e-15 * w[0].hypot(w[1]).max(1.0), "S {g:?} vs {w:?}");
            }
        }
    }
}

// ---------------------------------------------------------------------
// Wave-port / mixed files (issue #775)
// ---------------------------------------------------------------------

/// Complex scalar of the independent oracle.
pub type C = faer::c64;

/// Gauss–Jordan inverse (partial pivoting) of a row-major `n × n` matrix.
fn inverse(a: &[C], n: usize) -> Vec<C> {
    let (zero, one) = (C::new(0.0, 0.0), C::new(1.0, 0.0));
    let mut m = a.to_vec();
    let mut inv: Vec<C> = (0..n * n)
        .map(|k| if k / n == k % n { one } else { zero })
        .collect();
    for col in 0..n {
        let piv = (col..n)
            .max_by(|&x, &y| m[x * n + col].norm().total_cmp(&m[y * n + col].norm()))
            .unwrap();
        assert!(m[piv * n + col].norm() > 1e-13, "singular oracle matrix");
        for c in 0..n {
            m.swap(piv * n + c, col * n + c);
            inv.swap(piv * n + c, col * n + c);
        }
        let d = one / m[col * n + col];
        for c in 0..n {
            m[col * n + c] *= d;
            inv[col * n + c] *= d;
        }
        for r in (0..n).filter(|&r| r != col) {
            let f = m[r * n + col];
            for c in 0..n {
                let (mc, ic) = (m[col * n + c], inv[col * n + c]);
                m[r * n + c] -= f * mc;
                inv[r * n + c] -= f * ic;
            }
        }
    }
    inv
}

fn matmul(a: &[C], b: &[C], n: usize) -> Vec<C> {
    (0..n * n)
        .map(|k| (0..n).map(|l| a[(k / n) * n + l] * b[l * n + k % n]).sum())
        .collect()
}

/// The **impedance-route** renormalization, deliberately not the
/// production Γ-form: with `D = diag(√Z_c)` (principal root),
/// `Z = D(I − S)⁻¹(I + S)D` and `S' = √R⁻¹(Z − R)(Z + R)⁻¹√R`.
pub fn z_route(s: &[C], n: usize, z_c: &[C], r: &[f64]) -> Vec<C> {
    let eye = |k: usize| {
        if k / n == k % n {
            C::new(1.0, 0.0)
        } else {
            C::new(0.0, 0.0)
        }
    };
    let i_minus: Vec<C> = (0..n * n).map(|k| eye(k) - s[k]).collect();
    let i_plus: Vec<C> = (0..n * n).map(|k| eye(k) + s[k]).collect();
    let core = matmul(&inverse(&i_minus, n), &i_plus, n);
    let d: Vec<C> = z_c.iter().map(|z| z.sqrt()).collect();
    let z: Vec<C> = (0..n * n).map(|k| d[k / n] * core[k] * d[k % n]).collect();
    let rr = |k: usize| eye(k) * r[k / n];
    let num: Vec<C> = (0..n * n).map(|k| z[k] - rr(k)).collect();
    let den: Vec<C> = (0..n * n).map(|k| z[k] + rr(k)).collect();
    let q = matmul(&num, &inverse(&den, n), n);
    (0..n * n)
        .map(|k| q[k] * (r[k % n] / r[k / n]).sqrt())
        .collect()
}

/// Row-major `S` of a report row.
pub fn json_s(row: &serde_json::Value) -> (Vec<C>, usize) {
    let s = row["s"].as_array().unwrap();
    let n = s.len();
    let flat = s
        .iter()
        .flat_map(|r| r.as_array().unwrap().iter())
        .map(|z| C::new(z[0].as_f64().unwrap(), z[1].as_f64().unwrap()))
        .collect();
    (flat, n)
}

/// `Z_c` (ohms) of JSON channel `channel` in report row `row`, entirely
/// from the report: a lumped port's `resistance_ohm`, a wave channel's
/// `η₀·k₀·μ_t/β` from `wave_channels[].beta`, `k0` and
/// `wave_ports[].medium.mu_r_t`.
pub fn json_z_c(report: &serde_json::Value, row: &serde_json::Value, channel: usize) -> C {
    let ports = report["ports"].as_array().unwrap();
    if channel < ports.len() {
        return C::new(ports[channel]["resistance_ohm"].as_f64().unwrap(), 0.0);
    }
    let ch = row["wave_channels"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["channel"].as_u64() == Some(channel as u64))
        .expect("wave channel in the row");
    let beta = C::new(
        ch["beta"][0].as_f64().unwrap(),
        ch["beta"][1].as_f64().unwrap(),
    );
    let port = ch["port"].as_u64().unwrap() as usize;
    let mu_t = report["wave_ports"][port]["medium"]["mu_r_t"]
        .as_f64()
        .unwrap();
    let k0 = row["k0"].as_f64().unwrap();
    C::new(geode_core::constants::ETA_0_OHM * k0 * mu_t, 0.0) / beta
}

/// Per report row (ascending in frequency): the modal sub-block of the
/// `kept` JSON channels and each kept channel's `Z_c`.
pub fn modal_sub_blocks(report: &serde_json::Value, kept: &[usize]) -> Vec<(f64, Vec<C>, Vec<C>)> {
    let mut rows: Vec<(f64, Vec<C>, Vec<C>)> = report["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            let (s, n_all) = json_s(row);
            let s = &s;
            let sub = kept
                .iter()
                .flat_map(|&i| kept.iter().map(move |&j| s[i * n_all + j]))
                .collect();
            let z_c = kept.iter().map(|&c| json_z_c(report, row, c)).collect();
            (row["frequency_hz"].as_f64().unwrap(), sub, z_c)
        })
        .collect();
    rows.sort_by(|a, b| a.0.total_cmp(&b.0));
    rows
}

/// Nested `[re, im]` rows of a flat row-major matrix.
pub fn nested(m: &[C], n: usize) -> Vec<Vec<[f64; 2]>> {
    m.chunks(n)
        .map(|r| r.iter().map(|z| [z.re, z.im]).collect())
        .collect()
}

/// Assert the wave-port / mixed Touchstone file at `path` (named by the
/// report's `touchstone_file`) has `[Reference]` `refs`, one port per
/// `kept` JSON channel, `! Port[k]` lines `labels`, and, per row, the
/// [`z_route`] renormalization of the report's modal sub-block to within
/// `tol` (max-entry, relative to `max(1, max |S'|)`). Returns the parsed
/// file.
pub fn assert_wave_round_trip(
    report: &serde_json::Value,
    path: &std::path::Path,
    kept: &[usize],
    refs: &[f64],
    labels: &[&str],
    tol: f64,
) -> Parsed {
    use sha2::{Digest, Sha256};
    let bytes = std::fs::read(path).expect("touchstone file written");
    let tf = &report["touchstone_file"];
    assert_eq!(tf["path"], path.display().to_string());
    assert_eq!(tf["sha256"], format!("{:x}", Sha256::digest(&bytes)));
    let text = std::str::from_utf8(&bytes).unwrap();
    let parsed = parse(text);
    let (n, file_refs, rows) = &parsed;
    assert_eq!(*n, kept.len(), "one Touchstone port per kept channel");
    assert_eq!(file_refs, refs, "[Reference]");
    let names: Vec<&str> = text
        .lines()
        .filter_map(|l| l.strip_prefix("! Port["))
        .map(|l| l.split_once("] = ").unwrap().1)
        .collect();
    assert_eq!(names, labels, "! Port[k] lines");
    let want = modal_sub_blocks(report, kept);
    assert_eq!(rows.len(), want.len());
    assert!(rows.windows(2).all(|w| w[0].0 < w[1].0), "ascending rows");
    for ((gf, gs), (wf, s, z_c)) in rows.iter().zip(&want) {
        assert_eq!(gf, wf, "frequency");
        let w = z_route(s, *n, z_c, refs);
        let scale = w.iter().map(|z| z.norm()).fold(1.0, f64::max);
        let err = gs
            .iter()
            .flatten()
            .zip(&w)
            .map(|(g, w)| (C::new(g[0], g[1]) - w).norm())
            .fold(0.0, f64::max);
        assert!(
            err <= tol * scale,
            "{wf} Hz: file vs Z-route renormalization differ by {err:e} (tol {tol:e})"
        );
    }
    parsed
}
