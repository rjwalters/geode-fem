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

/// Reads `{path, f, s_re, s_im, z0_re, z0_im, refs}` (JSON on stdin),
/// renormalizes the modal network with scikit-rf and prints the max
/// deviation from the file, plus the file's port names.
const RENORM_SCRIPT: &str = "\
import json, sys
import numpy as np
import skrf
d = json.load(sys.stdin)
f = skrf.Network(d['path'])
s = np.array(d['s_re']) + 1j * np.array(d['s_im'])
z0 = np.array(d['z0_re']) + 1j * np.array(d['z0_im'])
freq = skrf.Frequency.from_f(np.array(d['f']), unit='hz')
m = skrf.Network(frequency=freq, s=s, z0=z0, s_def='traveling')
m.renormalize(np.array(d['refs'], dtype=complex))
scale = max(1.0, float(np.abs(m.s).max()))
print(json.dumps({
    'version': skrf.__version__,
    'err': float(np.abs(m.s - f.s).max()) / scale,
    'df': float(np.abs(f.f - np.array(d['f'])).max()),
    'port_names': list(f.port_names) if f.port_names is not None else None,
}))
";

/// Independent scikit-rf oracle for a renormalized wave-port file (issue
/// #775): `skrf.Network(s = modal S, z0 = Z_c(f), s_def = 'traveling')
/// .renormalize(refs)` must equal the file's S to `tol` (scikit-rf's own
/// round-off is ~1e-8), and the file's port names must be `labels`.
/// `rows` is `(f_hz, row-major modal S [re, im], Z_c [re, im] per port)`,
/// ascending. Returns whether the check ran (loud `SKIPPED` otherwise).
pub fn check_renormalized(
    what: &str,
    path: &Path,
    rows: &[(f64, Vec<[f64; 2]>, Vec<[f64; 2]>)],
    refs: &[f64],
    labels: &[&str],
    tol: f64,
) -> bool {
    let py = python();
    if !available(&py) {
        eprintln!(
            "SKIPPED scikit-rf renormalization oracle ({what}): `{py}` is not runnable or cannot \
             `import skrf` (set GEODE_SKRF to a Python with scikit-rf installed to enable; not a \
             CI dependency)"
        );
        return false;
    }
    let n = refs.len();
    let mat = |m: &[[f64; 2]], k: usize| -> Vec<Vec<f64>> {
        m.chunks(n)
            .map(|r| r.iter().map(|z| z[k]).collect())
            .collect()
    };
    let input = serde_json::json!({
        "path": path.display().to_string(),
        "f": rows.iter().map(|r| r.0).collect::<Vec<_>>(),
        "s_re": rows.iter().map(|r| mat(&r.1, 0)).collect::<Vec<_>>(),
        "s_im": rows.iter().map(|r| mat(&r.1, 1)).collect::<Vec<_>>(),
        "z0_re": rows.iter().map(|r| r.2.iter().map(|z| z[0]).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "z0_im": rows.iter().map(|r| r.2.iter().map(|z| z[1]).collect::<Vec<_>>()).collect::<Vec<_>>(),
        "refs": refs,
    });
    let mut child = Command::new(&py)
        .args(["-c", RENORM_SCRIPT])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("run python");
    {
        use std::io::Write as _;
        let mut stdin = child.stdin.take().unwrap();
        stdin.write_all(input.to_string().as_bytes()).unwrap();
    }
    let out = child.wait_with_output().expect("python output");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        out.status.success(),
        "scikit-rf renormalization failed for {} ({what}):\n{stdout}\n{}",
        path.display(),
        String::from_utf8_lossy(&out.stderr)
    );
    let line = stdout.lines().last().expect("scikit-rf printed nothing");
    let v: serde_json::Value = serde_json::from_str(line).expect("scikit-rf JSON");
    let err = v["err"].as_f64().unwrap();
    assert!(
        v["df"].as_f64().unwrap() == 0.0,
        "{what}: frequencies differ"
    );
    assert!(
        err <= tol,
        "{what}: scikit-rf renormalize vs file S differ by {err:e} (tol {tol:e})"
    );
    let names: Vec<&str> = v["port_names"]
        .as_array()
        .expect("scikit-rf read port names")
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert_eq!(names, labels, "{what}: scikit-rf port names");
    eprintln!(
        "scikit-rf {} renormalized the modal S of {} ({what}) to [Reference]: max deviation \
         {err:.2e}, port names match",
        v["version"].as_str().unwrap_or("?"),
        path.display()
    );
    true
}
