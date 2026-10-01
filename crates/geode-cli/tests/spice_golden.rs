//! `geode capacitance --spice` (issue #715, Epic #702 Phase 3c) through
//! the real binary, on the golden coax / triax fixtures of
//! `capacitance_golden.rs`.
//!
//! * The report's `spice_file` names the file with the right sha256.
//! * The triax `inner` terminal is fully Faraday-shielded by `shield`, so
//!   its ground branch (`c_sigma_farad[0]`, ~1e-15 relative to the
//!   diagonal) is noise and is **dropped** with a comment, while the
//!   `shield` ground branch (~0.34 relative) and the `inner`–`shield`
//!   mutual branch are **kept**, bit-exact to the report.
//! * The network, re-read by an independent test-side parser, reproduces
//!   the report's Maxwell matrix.
//! * **Optional ngspice check**: if an `ngspice` binary is runnable (the
//!   `GEODE_NGSPICE` environment variable, else `ngspice` on `PATH`), an
//!   AC analysis drives each port in turn and measures the terminal
//!   admittances `I_k = jω Σ_j C_kj V_j` against `c_farad`. ngspice is not
//!   a CI dependency: without it the check is skipped with a loud
//!   `SKIPPED` line on stderr.

#[path = "support/scratch.rs"]
mod scratch_support;

use scratch_support::Scratch;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use sha2::{Digest, Sha256};

fn fixtures() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Fresh scratch dir for one test, removed on drop (also on panic).
fn scratch(name: &str) -> Scratch {
    Scratch::new("geode-spice-test-", name)
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Run `geode capacitance <spec> --spice <out>` and return the report.
fn capacitance_with_spice(spec: &Path, out: &Path) -> serde_json::Value {
    let o = geode(&[
        "capacitance",
        spec.to_str().unwrap(),
        "--spice",
        out.to_str().unwrap(),
    ]);
    assert!(
        o.status.success(),
        "geode capacitance --spice failed ({}):\nstderr: {}\nstdout: {}",
        o.status,
        String::from_utf8_lossy(&o.stderr),
        String::from_utf8_lossy(&o.stdout)
    );
    serde_json::from_slice(&o.stdout).expect("report is JSON")
}

fn f64s(v: &serde_json::Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}

fn c_matrix(report: &serde_json::Value) -> Vec<Vec<f64>> {
    report["c_farad"]
        .as_array()
        .unwrap()
        .iter()
        .map(f64s)
        .collect()
}

/// Independent reader of the emitted subset: `(ports, caps[(name, a, b,
/// farad)], comments)`.
type Net = (Vec<String>, Vec<(String, String, String, f64)>, Vec<String>);

fn parse(text: &str) -> Net {
    let (mut ports, mut caps, mut comments) = (None, Vec::new(), Vec::new());
    let mut ended = false;
    for line in text.lines() {
        if let Some(c) = line.strip_prefix("* ") {
            comments.push(c.to_string());
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        match t[0] {
            ".subckt" => {
                assert_eq!(t[1], "CEXTRACT");
                ports = Some(t[2..].iter().map(|s| s.to_string()).collect::<Vec<_>>());
            }
            ".ends" => {
                assert_eq!(t[1..], ["CEXTRACT"]);
                ended = true;
            }
            c if c.starts_with('C') && !ended => {
                assert_eq!(t.len(), 4, "{line:?}");
                caps.push((t[0].into(), t[1].into(), t[2].into(), t[3].parse().unwrap()));
            }
            _ => panic!("unexpected line {line:?}"),
        }
    }
    assert!(ended, "missing .ends");
    (ports.expect(".subckt"), caps, comments)
}

/// The Maxwell matrix the network represents.
fn maxwell(net: &Net) -> Vec<Vec<f64>> {
    let (ports, caps, _) = net;
    let n = ports.len();
    let idx = |s: &str| ports.iter().position(|p| p == s);
    let mut c = vec![vec![0.0; n]; n];
    for (_, a, b, v) in caps {
        match (idx(a), idx(b)) {
            (Some(i), None) => {
                assert_eq!(b, "0");
                c[i][i] += v;
            }
            (Some(i), Some(j)) => {
                c[i][i] += v;
                c[j][j] += v;
                c[i][j] -= v;
                c[j][i] -= v;
            }
            _ => panic!("unexpected nodes {a} {b}"),
        }
    }
    c
}

fn assert_file_ref(report: &serde_json::Value, path: &Path) -> String {
    let text = std::fs::read_to_string(path).unwrap();
    let sf = &report["spice_file"];
    assert_eq!(sf["path"], path.display().to_string());
    assert_eq!(
        sf["sha256"],
        format!("{:x}", Sha256::digest(text.as_bytes()))
    );
    text
}

fn assert_reproduces(report: &serde_json::Value, net: &Net, rel: f64) {
    let c = c_matrix(report);
    let max_diag = (0..c.len()).map(|i| c[i][i]).fold(0.0, f64::max);
    let back = maxwell(net);
    for (i, row) in c.iter().enumerate() {
        for (j, want) in row.iter().enumerate() {
            let err = (back[i][j] - want).abs() / max_diag;
            assert!(err <= rel, "C[{i}][{j}] {} vs {want}: {err:e}", back[i][j]);
        }
    }
}

#[test]
fn triax_drops_shielded_ground_branch_and_round_trips() {
    let dir = scratch("triax");
    let out = dir.join("triax.sp");
    // An existing file is overwritten.
    std::fs::write(&out, "stale\n").unwrap();
    let report = capacitance_with_spice(&fixtures().join("capacitance_triax_smoke.toml"), &out);
    let text = assert_file_ref(&report, &out);

    // Re-measure the kept / dropped figures from this run's report.
    let c = c_matrix(&report);
    let sigma = f64s(&report["c_sigma_farad"]);
    let max_diag = c[0][0].max(c[1][1]);
    eprintln!(
        "triax: c_sigma = {sigma:?} F; relative to max diag {max_diag:e}: {:e}, {:e}",
        sigma[0].abs() / max_diag,
        sigma[1] / max_diag
    );
    assert!(sigma[0].abs() / max_diag < 1e-12, "inner row sum is noise");
    assert!(sigma[1] / max_diag > 0.1, "shield ground branch is real");

    let net = parse(&text);
    assert_eq!(net.0, ["inner", "shield"]);
    // Values agree with the report to the last ulp or so (serde_json's
    // default float parser is not guaranteed bit-exact).
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-15 * b.abs();
    let mutual = -0.5 * (c[0][1] + c[1][0]);
    let shape: Vec<(&str, &str, &str)> = net
        .1
        .iter()
        .map(|(n, a, b, _)| (n.as_str(), a.as_str(), b.as_str()))
        .collect();
    assert_eq!(
        shape,
        [("C1_2", "inner", "shield"), ("C2_0", "shield", "0")],
        "kept branches"
    );
    assert!(close(net.1[0].3, mutual), "{} vs {mutual}", net.1[0].3);
    assert!(
        close(net.1[1].3, sigma[1]),
        "{} vs {}",
        net.1[1].3,
        sigma[1]
    );
    let dropped: Vec<&String> = net.2.iter().filter(|c| c.starts_with("dropped:")).collect();
    assert_eq!(dropped.len(), 1, "{text}");
    let value: f64 = dropped[0]
        .strip_prefix("dropped: C(inner, ground) = ")
        .and_then(|r| r.split(' ').next())
        .unwrap_or_else(|| panic!("{}", dropped[0]))
        .parse()
        .unwrap();
    assert!(close(value, sigma[0]), "{value} vs {}", sigma[0]);
    // Provenance header: version + sha, spec, mesh + sha256.
    let mesh_sha = report["mesh"]["sha256"].as_str().unwrap();
    assert!(net.2[0].starts_with("SPICE subcircuit written by geode "));
    assert!(net.2[1].starts_with("spec: ") && net.2[1].ends_with("capacitance_triax_smoke.toml"));
    assert!(net.2[2].starts_with("mesh: ") && net.2[2].ends_with(&format!("sha256={mesh_sha}")));

    assert_reproduces(&report, &net, 1e-12);
    ngspice_check("triax", &out, &c);
}

#[test]
fn coax_single_terminal_is_one_ground_capacitor() {
    let dir = scratch("coax");
    let out = dir.join("coax.sp");
    let report = capacitance_with_spice(&fixtures().join("capacitance_coax_smoke.json"), &out);
    let text = assert_file_ref(&report, &out);
    let net = parse(&text);
    let sigma = f64s(&report["c_sigma_farad"]);
    assert_eq!(net.0, ["inner"]);
    assert_eq!(net.1.len(), 1);
    let (name, a, b, v) = &net.1[0];
    assert_eq!(
        (name.as_str(), a.as_str(), b.as_str()),
        ("C1_0", "inner", "0")
    );
    assert!(
        (v - sigma[0]).abs() <= 1e-15 * sigma[0],
        "{v} vs {}",
        sigma[0]
    );
    assert!(!text.contains("dropped:"));
    assert_reproduces(&report, &net, 1e-15);
    ngspice_check("coax", &out, &c_matrix(&report));
}

#[test]
fn without_the_flag_there_is_no_spice_file() {
    let o = geode(&[
        "capacitance",
        fixtures()
            .join("capacitance_coax_smoke.json")
            .to_str()
            .unwrap(),
    ]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v.get("spice_file").is_none());
}

#[test]
fn spice_is_rejected_by_the_rf_subcommands() {
    // `--spice` is a flag of the static subcommands only (capacitance,
    // inductance): clap rejects it on the RF subcommands.
    let o = geode(&["driven", "spec.json", "--spice", "x.sp"]);
    assert!(!o.status.success());
    assert!(
        String::from_utf8_lossy(&o.stderr).contains("--spice"),
        "{}",
        String::from_utf8_lossy(&o.stderr)
    );
}

/// The ngspice binary to try (`GEODE_NGSPICE`, else `ngspice`).
fn ngspice_bin() -> String {
    std::env::var("GEODE_NGSPICE").unwrap_or_else(|_| "ngspice".into())
}

/// AC check in ngspice: drive port `k` with 1 V (others 0 V) at 1 MHz and
/// read `C_jk = −Im I(V_j) / ω` for every port `j`. Skipped loudly when
/// ngspice is not runnable.
// Column `k` / row `j` indexing of a dense matrix reads clearest as ranges.
#[allow(clippy::needless_range_loop)]
fn ngspice_check(what: &str, subckt: &Path, c: &[Vec<f64>]) {
    let bin = ngspice_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        eprintln!(
            "SKIPPED ngspice check ({what}): `{bin}` is not runnable (set GEODE_NGSPICE or put \
             ngspice on PATH to enable; not a CI dependency)"
        );
        return;
    }
    let n = c.len();
    let max_diag = (0..n).map(|i| c[i][i]).fold(0.0, f64::max);
    let dir = subckt.parent().unwrap();
    for k in 0..n {
        let ports: Vec<String> = (1..=n).map(|j| format!("p{j}")).collect();
        let mut deck = format!(
            "geode spice check\n.include {}\nX1 {} CEXTRACT\n",
            subckt.display(),
            ports.join(" ")
        );
        for j in 0..n {
            let ac = if j == k { 1 } else { 0 };
            deck += &format!("V{} p{} 0 DC 0 AC {ac}\n", j + 1, j + 1);
        }
        deck += ".control\nset numdgt=15\nac lin 1 1meg 1meg\n";
        for j in 1..=n {
            deck += &format!("let c{j} = -imag(i(v{j}))/(2*pi*1e6)\nprint c{j}\n");
        }
        deck += ".endc\n.end\n";
        let path = dir.join(format!("tb_{k}.cir"));
        std::fs::write(&path, deck).unwrap();
        let o = Command::new(&bin)
            .arg("-b")
            .arg(&path)
            .output()
            .expect("run ngspice");
        let stdout = String::from_utf8_lossy(&o.stdout);
        assert!(
            o.status.success(),
            "ngspice failed:\n{stdout}\n{}",
            String::from_utf8_lossy(&o.stderr)
        );
        for j in 0..n {
            let key = format!("c{} = ", j + 1);
            let got: f64 = stdout
                .lines()
                .find_map(|l| l.trim().strip_prefix(&key))
                .unwrap_or_else(|| panic!("no `{key}` in ngspice output:\n{stdout}"))
                .trim()
                .parse()
                .unwrap();
            let err = (got - c[j][k]).abs() / max_diag;
            eprintln!(
                "ngspice {what}: C[{j}][{k}] = {got:e} vs {:e} (rel {err:e})",
                c[j][k]
            );
            assert!(err <= 1e-9, "ngspice C[{j}][{k}] {got:e} vs {:e}", c[j][k]);
        }
    }
}
