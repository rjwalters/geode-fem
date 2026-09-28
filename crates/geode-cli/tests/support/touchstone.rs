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
