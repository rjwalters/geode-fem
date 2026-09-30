//! Optional independent-reader check of a geode-written Touchstone file
//! with [scikit-rf](https://scikit-rf.org) (issue #713). Shared by
//! `tests/touchstone_skrf.rs` (a real `geode driven --touchstone` `.s1p`)
//! and the `src/touchstone.rs` unit tests (synthetic 2- and 5-port files
//! straight from `render`) via `#[path]`.
//!
//! scikit-rf is **not** a CI dependency. The Python interpreter is
//! `GEODE_SKRF` if set, else `python3` on `PATH`; when it is not runnable
//! or cannot `import skrf`, the check prints a loud `SKIPPED` line on
//! stderr and returns `false` (not a silent pass, not a failure). No
//! packages are ever installed.

#![allow(dead_code)]

use std::path::Path;
use std::process::Command;

/// `(f_hz, s[row][col] = [re, im])`, one entry per frequency.
pub type Rows = [(f64, Vec<Vec<[f64; 2]>>)];

/// Loads the file and prints `f`, `z0` and `s` as one JSON line.
const SCRIPT: &str = "\
import json, sys
import skrf
n = skrf.Network(sys.argv[1])
print(json.dumps({
    'version': skrf.__version__,
    'f': n.f.tolist(),
    'z0_re': n.z0.real.tolist(),
    'z0_im': n.z0.imag.tolist(),
    's_re': n.s.real.tolist(),
    's_im': n.s.imag.tolist(),
}))
";

/// The Python interpreter to try (`GEODE_SKRF`, else `python3`).
pub fn python() -> String {
    std::env::var("GEODE_SKRF").unwrap_or_else(|_| "python3".into())
}

/// Whether `python()` runs and can `import skrf`.
fn available(py: &str) -> bool {
    Command::new(py)
        .args(["-c", "import skrf"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-12 * a.abs().max(b.abs()).max(1e-300)
}

/// Load `path` in scikit-rf and assert its frequencies, per-port `z0`
/// (real, at every frequency) and S-matrices equal `refs` / `rows`
/// (`rows` ascending in frequency, as geode writes them). Returns whether
/// the check actually ran.
// Port-index loops over the dense S-matrix read clearest as ranges.
#[allow(clippy::needless_range_loop)]
pub fn check(what: &str, path: &Path, refs: &[f64], rows: &Rows) -> bool {
    let py = python();
    if !available(&py) {
        eprintln!(
            "SKIPPED scikit-rf check ({what}): `{py}` is not runnable or cannot `import skrf` \
             (set GEODE_SKRF to a Python with scikit-rf installed to enable; not a CI dependency)"
        );
        return false;
    }
    let out = Command::new(&py)
        .args(["-c", SCRIPT])
        .arg(path)
        .output()
        .expect("run python");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "scikit-rf failed to load {} ({what}):\n{stdout}\n{}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let line = stdout.lines().last().expect("scikit-rf printed nothing");
    let v: serde_json::Value = serde_json::from_str(line).expect("scikit-rf JSON");
    let f64s = |x: &serde_json::Value| -> Vec<f64> {
        x.as_array()
            .unwrap()
            .iter()
            .map(|y| y.as_f64().unwrap())
            .collect()
    };
    let f = f64s(&v["f"]);
    assert_eq!(f.len(), rows.len(), "{what}: frequency count");
    let n = refs.len();
    for (k, (fk, s)) in rows.iter().enumerate() {
        assert!(close(f[k], *fk), "{what}: f[{k}] {} vs {fk}", f[k]);
        let z0_re = f64s(&v["z0_re"][k]);
        let z0_im = f64s(&v["z0_im"][k]);
        assert_eq!(z0_re.len(), n, "{what}: port count");
        for p in 0..n {
            assert!(close(z0_re[p], refs[p]), "{what}: z0[{k}][{p}]");
            assert_eq!(z0_im[p], 0.0, "{what}: z0[{k}][{p}] is real");
        }
        for i in 0..n {
            let re = f64s(&v["s_re"][k][i]);
            let im = f64s(&v["s_im"][k][i]);
            for j in 0..n {
                let [wr, wi] = s[i][j];
                assert!(
                    close(re[j], wr) && close(im[j], wi),
                    "{what}: S{}{} at {fk} Hz: skrf {}{:+}j vs geode {wr}{wi:+}j",
                    i + 1,
                    j + 1,
                    re[j],
                    im[j]
                );
            }
        }
    }
    eprintln!(
        "scikit-rf {} loaded {} ({what}): {n}-port, {} frequencies, all values match",
        v["version"].as_str().unwrap_or("?"),
        path.display(),
        rows.len()
    );
    true
}
