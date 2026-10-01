//! `geode inductance --spice` (issue #719, Epic #702 Phase 3) through the
//! real binary, on the golden coax / triax fixtures of
//! `inductance_golden.rs`.
//!
//! * The report's `spice_file` names the file with the right sha256.
//! * Each path is one SPICE node with a self inductor `L<i>_0 <node> 0`
//!   (source and sink both contact the PEC return wall = ground `0`); the
//!   triax `core`–`tube` mutual is a single `K1_2` statement, kept (`k ≈
//!   0.65`, far above the `1e-9` threshold).
//! * The network, re-read by an independent test-side parser, reproduces
//!   the report's `l_henry` (self terms from the `L` lines, mutual from
//!   `k √(L_ii L_jj)`).
//! * **Sign**: the triax with the tube's source / sink swapped reverses
//!   its reference current, so `L_12 < 0`; the export carries it as a
//!   negative `k` with the node order unchanged, and ngspice (when
//!   available) measures the negative mutual.
//! * **Optional ngspice check**: if an `ngspice` binary is runnable (the
//!   `GEODE_NGSPICE` environment variable, else `ngspice` on `PATH`), an
//!   AC analysis drives each path node in turn with a 1 A current source,
//!   leaves every other path node undriven (open circuit), and reads
//!   `L_jk = Im V_j / ω` against `l_henry`. ngspice is not a CI
//!   dependency: without it the check is skipped with a loud `SKIPPED`
//!   line on stderr.
//! * **Variants (issue #723)**: `--spice-positive-k` on the reversed triax
//!   declares the tube's inductor `0 -> tube` and emits `+|k|`, and
//!   `--spice-ret-pin` exposes the return as a last `ret` port; in both
//!   the *port-level* matrix — negative mutual included — is unchanged,
//!   checked by the parser and (when available) ngspice, with `ret` wired
//!   to a non-ground node in the testbench.

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
    Scratch::new("geode-lspice-test-", name)
}

fn geode(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_geode"))
        .args(args)
        .output()
        .expect("spawn geode")
}

/// Run `geode inductance <spec> --spice <out>` and return the report.
fn inductance_with_spice(spec: &Path, out: &Path) -> serde_json::Value {
    inductance_with_spice_flags(spec, out, &[])
}

/// [`inductance_with_spice`] with extra flags (`--spice-positive-k`, …).
fn inductance_with_spice_flags(spec: &Path, out: &Path, flags: &[&str]) -> serde_json::Value {
    let mut args = vec![
        "inductance",
        spec.to_str().unwrap(),
        "--spice",
        out.to_str().unwrap(),
    ];
    args.extend_from_slice(flags);
    let o = geode(&args);
    assert!(
        o.status.success(),
        "geode inductance --spice failed ({}):\nstderr: {}\nstdout: {}",
        o.status,
        String::from_utf8_lossy(&o.stderr),
        String::from_utf8_lossy(&o.stdout)
    );
    serde_json::from_slice(&o.stdout).expect("report is JSON")
}

fn l_matrix(report: &serde_json::Value) -> Vec<Vec<f64>> {
    report["l_henry"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|x| x.as_f64().unwrap())
                .collect()
        })
        .collect()
}

/// Independent reader of the emitted subset.
#[derive(Debug)]
struct Net {
    ports: Vec<String>,
    /// `(name, node_a, node_b, henry)`.
    inds: Vec<(String, String, String, f64)>,
    /// `(name, inductor_1, inductor_2, k)`.
    ks: Vec<(String, String, String, f64)>,
    comments: Vec<String>,
}

fn parse(text: &str) -> Net {
    let (mut ports, mut inds, mut ks, mut comments) = (None, Vec::new(), Vec::new(), Vec::new());
    let mut ended = false;
    for line in text.lines() {
        if let Some(c) = line.strip_prefix("* ") {
            comments.push(c.to_string());
            continue;
        }
        let t: Vec<&str> = line.split_whitespace().collect();
        match t[0] {
            ".subckt" => {
                assert_eq!(t[1], "LEXTRACT");
                ports = Some(t[2..].iter().map(|s| s.to_string()).collect::<Vec<_>>());
            }
            ".ends" => {
                assert_eq!(t[1..], ["LEXTRACT"]);
                ended = true;
            }
            l if l.starts_with('L') && !ended => {
                assert_eq!(t.len(), 4, "{line:?}");
                inds.push((t[0].into(), t[1].into(), t[2].into(), t[3].parse().unwrap()));
            }
            k if k.starts_with('K') && !ended => {
                assert_eq!(t.len(), 4, "{line:?}");
                ks.push((t[0].into(), t[1].into(), t[2].into(), t[3].parse().unwrap()));
            }
            _ => panic!("unexpected line {line:?}"),
        }
    }
    assert!(ended, "missing .ends");
    Net {
        ports: ports.expect(".subckt"),
        inds,
        ks,
        comments,
    }
}

/// The inductance matrix the network represents.
fn inductance(net: &Net) -> Vec<Vec<f64>> {
    let n = net.ports.len();
    let mut l = vec![vec![0.0; n]; n];
    let mut idx_of = std::collections::HashMap::new();
    for (name, a, b, v) in &net.inds {
        assert_eq!(b, "0", "{name} must return to ground");
        let i = net.ports.iter().position(|p| p == a).expect("port node");
        assert_eq!(l[i][i], 0.0, "one inductor per path");
        l[i][i] = *v;
        idx_of.insert(name.clone(), i);
    }
    for (_, l1, l2, k) in &net.ks {
        let (i, j) = (idx_of[l1], idx_of[l2]);
        assert!(k.abs() < 1.0, "|k| < 1");
        let m = k * (l[i][i] * l[j][j]).sqrt();
        l[i][j] = m;
        l[j][i] = m;
    }
    l
}

/// The port-level inductance matrix of a network whose inductors may be
/// declared either way round against the return node `ret` (`0` or the
/// `ret` pin): an inductor declared `ret -> node` has its dot on the
/// return side, so its couplings enter the port matrix negated.
fn port_inductance(net: &Net, ret: &str) -> Vec<Vec<f64>> {
    let paths: Vec<&String> = net.ports.iter().filter(|p| *p != ret).collect();
    let n = paths.len();
    let mut l = vec![vec![0.0; n]; n];
    let mut idx_of = std::collections::HashMap::new();
    for (name, a, b, v) in &net.inds {
        let (node, sign) = match (a.as_str() == ret, b.as_str() == ret) {
            (false, true) => (a, 1.0),
            (true, false) => (b, -1.0),
            _ => panic!("{name} must run between a path node and {ret}"),
        };
        let i = paths.iter().position(|p| *p == node).expect("port node");
        l[i][i] = *v;
        idx_of.insert(name.clone(), (i, sign));
    }
    for (_, l1, l2, k) in &net.ks {
        let ((i, si), (j, sj)) = (idx_of[l1], idx_of[l2]);
        let m = si * sj * k * (l[i][i] * l[j][j]).sqrt();
        l[i][j] = m;
        l[j][i] = m;
    }
    l
}

/// The triax spec with the tube's source and sink swapped (so `L_12 < 0`),
/// written into `dir`.
fn reversed_triax_spec(dir: &Path) -> PathBuf {
    let spec_text = std::fs::read_to_string(fixtures().join("inductance_triax_smoke.toml"))
        .unwrap()
        .replace(
            "path = \"../../../geode-core/tests/fixtures/coax_inductance_smoke.msh\"",
            &format!(
                "path = {:?}",
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../geode-core/tests/fixtures/coax_inductance_smoke.msh")
                    .display()
                    .to_string()
            ),
        )
        .replace(
            "source = \"tube_in\"\nsink = \"tube_out\"",
            "source = \"tube_out\"\nsink = \"tube_in\"",
        );
    assert!(spec_text.contains("source = \"tube_out\""), "{spec_text}");
    let spec = dir.join("triax_reversed.toml");
    std::fs::write(&spec, spec_text).unwrap();
    spec
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
    let l = l_matrix(report);
    let max_diag = (0..l.len()).map(|i| l[i][i]).fold(0.0, f64::max);
    let back = inductance(net);
    for (i, row) in l.iter().enumerate() {
        for (j, want) in row.iter().enumerate() {
            let err = (back[i][j] - want).abs() / max_diag;
            assert!(err <= rel, "L[{i}][{j}] {} vs {want}: {err:e}", back[i][j]);
        }
    }
}

fn assert_header(net: &Net, report: &serde_json::Value, spec_suffix: &str) {
    let mesh_sha = report["mesh"]["sha256"].as_str().unwrap();
    assert!(net.comments[0].starts_with("SPICE subcircuit written by geode "));
    assert!(
        net.comments[1].starts_with("spec: ") && net.comments[1].ends_with(spec_suffix),
        "{}",
        net.comments[1]
    );
    assert!(
        net.comments[2].starts_with("mesh: ")
            && net.comments[2].ends_with(&format!("sha256={mesh_sha}"))
    );
}

/// The shape of the triax network: two self inductors, one coupling.
fn assert_triax_shape(net: &Net) {
    assert_eq!(net.ports, ["core", "tube"]);
    let shape: Vec<(&str, &str, &str)> = net
        .inds
        .iter()
        .map(|(n, a, b, _)| (n.as_str(), a.as_str(), b.as_str()))
        .collect();
    assert_eq!(shape, [("L1_0", "core", "0"), ("L2_0", "tube", "0")]);
    assert_eq!(net.ks.len(), 1);
    assert_eq!(
        (
            net.ks[0].0.as_str(),
            net.ks[0].1.as_str(),
            net.ks[0].2.as_str()
        ),
        ("K1_2", "L1_0", "L2_0")
    );
}

#[test]
fn triax_is_two_self_inductors_and_one_coupling() {
    let dir = scratch("triax");
    let out = dir.join("triax.sp");
    // An existing file is overwritten.
    std::fs::write(&out, "stale\n").unwrap();
    let report = inductance_with_spice(&fixtures().join("inductance_triax_smoke.toml"), &out);
    let text = assert_file_ref(&report, &out);
    let l = l_matrix(&report);
    let net = parse(&text);
    assert_triax_shape(&net);

    // Self terms agree with the report to the last ulp or so (serde_json's
    // default float parser is not guaranteed bit-exact).
    let close = |a: f64, b: f64| (a - b).abs() <= 1e-15 * b.abs();
    assert!(
        close(net.inds[0].3, l[0][0]),
        "{} vs {}",
        net.inds[0].3,
        l[0][0]
    );
    assert!(
        close(net.inds[1].3, l[1][1]),
        "{} vs {}",
        net.inds[1].3,
        l[1][1]
    );
    // Core and tube currents both flow source (z = 0) -> sink (z = L), so
    // the mutual is positive, and so is k.
    let k = l[0][1] / (l[0][0] * l[1][1]).sqrt();
    eprintln!("triax: L = {l:?} H, k = {k}");
    assert!(l[0][1] > 0.0 && k > 0.1 && k < 1.0, "{k}");
    assert!((net.ks[0].3 - k).abs() <= 1e-15, "{} vs {k}", net.ks[0].3);
    assert!(!text.contains("dropped:"), "{text}");
    assert_header(&net, &report, "inductance_triax_smoke.toml");

    assert_reproduces(&report, &net, 1e-15);
    ngspice_check("triax", &out, &l);
}

#[test]
fn reversed_tube_gives_negative_k_with_unchanged_node_order() {
    // The triax fixture with the tube's source and sink swapped: the tube's
    // reference current now runs z = L -> z = 0, opposite the core's.
    let dir = scratch("triax-rev");
    let spec_text = std::fs::read_to_string(fixtures().join("inductance_triax_smoke.toml"))
        .unwrap()
        .replace(
            "path = \"../../../geode-core/tests/fixtures/coax_inductance_smoke.msh\"",
            &format!(
                "path = {:?}",
                PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("../geode-core/tests/fixtures/coax_inductance_smoke.msh")
                    .display()
                    .to_string()
            ),
        )
        .replace(
            "source = \"tube_in\"\nsink = \"tube_out\"",
            "source = \"tube_out\"\nsink = \"tube_in\"",
        );
    assert!(spec_text.contains("source = \"tube_out\""), "{spec_text}");
    let spec = dir.join("triax_reversed.toml");
    std::fs::write(&spec, spec_text).unwrap();
    let out = dir.join("triax_reversed.sp");
    let report = inductance_with_spice(&spec, &out);
    let text = assert_file_ref(&report, &out);
    let l = l_matrix(&report);
    let net = parse(&text);
    // Same node order and element shape as the forward triax ...
    assert_triax_shape(&net);
    // ... but the mutual, and hence k, flips sign.
    let k = net.ks[0].3;
    eprintln!("reversed triax: L = {l:?} H, k = {k}");
    assert!(l[0][1] < 0.0, "{l:?}");
    assert!(k < -0.1 && k > -1.0, "{k}");
    assert_header(&net, &report, "triax_reversed.toml");
    assert_reproduces(&report, &net, 1e-15);
    ngspice_check("triax-reversed", &out, &l);
}

#[test]
fn coax_single_path_is_one_self_inductor() {
    let dir = scratch("coax");
    let out = dir.join("coax.sp");
    let report = inductance_with_spice(&fixtures().join("inductance_coax_smoke.json"), &out);
    let text = assert_file_ref(&report, &out);
    let net = parse(&text);
    let l = l_matrix(&report);
    assert_eq!(net.ports, ["core"]);
    assert_eq!(net.inds.len(), 1);
    assert!(net.ks.is_empty());
    let (name, a, b, v) = &net.inds[0];
    assert_eq!(
        (name.as_str(), a.as_str(), b.as_str()),
        ("L1_0", "core", "0")
    );
    assert!((v - l[0][0]).abs() <= 1e-15 * l[0][0], "{v} vs {}", l[0][0]);
    assert!(!text.contains("dropped:"));
    assert_header(&net, &report, "inductance_coax_smoke.json");
    assert_reproduces(&report, &net, 1e-15);
    ngspice_check("coax", &out, &l);
}

#[test]
fn positive_k_flips_the_reversed_tube_and_keeps_the_port_matrix() {
    let dir = scratch("triax-rev-posk");
    let spec = reversed_triax_spec(&dir);
    let out = dir.join("triax_reversed_posk.sp");
    let report = inductance_with_spice_flags(&spec, &out, &["--spice-positive-k"]);
    let text = assert_file_ref(&report, &out);
    let l = l_matrix(&report);
    let net = parse(&text);
    assert!(l[0][1] < 0.0, "{l:?}");
    assert_eq!(net.ports, ["core", "tube"]);
    let shape: Vec<(&str, &str, &str)> = net
        .inds
        .iter()
        .map(|(n, a, b, _)| (n.as_str(), a.as_str(), b.as_str()))
        .collect();
    // The tube (path 2) is flipped: declared 0 -> tube.
    assert_eq!(shape, [("L1_0", "core", "0"), ("L2_0", "0", "tube")]);
    let k = net.ks[0].3;
    eprintln!("reversed triax, positive-k: L = {l:?} H, k = {k}");
    assert!(k > 0.1 && k < 1.0, "{k}");
    assert!(
        net.comments
            .iter()
            .any(|c| c.starts_with("flipped: \"tube\" (L2_0 declared 0 -> tube")),
        "{text}"
    );
    // The port-level matrix, negative mutual included, is the report's.
    let back = port_inductance(&net, "0");
    let max_diag = l[0][0].max(l[1][1]);
    for i in 0..2 {
        for j in 0..2 {
            assert!(
                (back[i][j] - l[i][j]).abs() <= 1e-15 * max_diag,
                "L[{i}][{j}]"
            );
        }
    }
    ngspice_check_with("triax-reversed-positive-k", &out, &l, false);
}

#[test]
fn ret_pin_exposes_the_return_as_a_last_port() {
    let dir = scratch("triax-rev-ret");
    let spec = reversed_triax_spec(&dir);
    for (what, flags, flipped) in [
        ("ret", &["--spice-ret-pin"][..], false),
        (
            "ret-positive-k",
            &["--spice-ret-pin", "--spice-positive-k"][..],
            true,
        ),
    ] {
        let out = dir.join(format!("triax_reversed_{what}.sp"));
        let report = inductance_with_spice_flags(&spec, &out, flags);
        let text = assert_file_ref(&report, &out);
        let l = l_matrix(&report);
        let net = parse(&text);
        assert_eq!(net.ports, ["core", "tube", "ret"], "{what}");
        let tube = if flipped {
            ("L2_0", "ret", "tube")
        } else {
            ("L2_0", "tube", "ret")
        };
        let shape: Vec<(&str, &str, &str)> = net
            .inds
            .iter()
            .map(|(n, a, b, _)| (n.as_str(), a.as_str(), b.as_str()))
            .collect();
        assert_eq!(shape, [("L1_0", "core", "ret"), tube], "{what}");
        assert_eq!(net.ks[0].3 > 0.0, flipped, "{what}: {text}");
        let back = port_inductance(&net, "ret");
        let max_diag = l[0][0].max(l[1][1]);
        for i in 0..2 {
            for j in 0..2 {
                assert!(
                    (back[i][j] - l[i][j]).abs() <= 1e-15 * max_diag,
                    "{what}: L[{i}][{j}]"
                );
            }
        }
        ngspice_check_with(&format!("triax-reversed-{what}"), &out, &l, true);
    }
}

#[test]
fn spice_variant_flags_require_spice() {
    for flag in ["--spice-positive-k", "--spice-ret-pin"] {
        let o = geode(&[
            "inductance",
            fixtures()
                .join("inductance_coax_smoke.json")
                .to_str()
                .unwrap(),
            flag,
        ]);
        assert!(!o.status.success(), "{flag} accepted without --spice");
    }
}

#[test]
fn without_the_flag_there_is_no_spice_file() {
    let o = geode(&[
        "inductance",
        fixtures()
            .join("inductance_coax_smoke.json")
            .to_str()
            .unwrap(),
    ]);
    assert!(o.status.success());
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    assert!(v.get("spice_file").is_none());
}

/// The ngspice binary to try (`GEODE_NGSPICE`, else `ngspice`).
fn ngspice_bin() -> String {
    std::env::var("GEODE_NGSPICE").unwrap_or_else(|_| "ngspice".into())
}

/// AC check in ngspice: drive path `k`'s node with a 1 A AC current source
/// at 1 MHz, every other path node undriven (open circuit — grounding it
/// would short the induced mutual voltage), and read `L_jk = Im V(p_j) /
/// ω` for every path `j`. Skipped loudly when ngspice is not runnable.
// Column `k` / row `j` indexing of a dense matrix reads clearest as ranges.
fn ngspice_check(what: &str, subckt: &Path, l: &[Vec<f64>]) {
    ngspice_check_with(what, subckt, l, false)
}

/// [`ngspice_check`]; with `ret_pin`, the subcircuit's last port `ret` is
/// wired to a non-ground node `r` (tied to `0` only through a 1 Ω DC
/// reference resistor that carries no AC current), the source drives `r ->
/// p_k`, and `L_jk = Im (V_j − V_r) / ω`.
// Column `k` / row `j` indexing of a dense matrix reads clearest as ranges.
#[allow(clippy::needless_range_loop)]
fn ngspice_check_with(what: &str, subckt: &Path, l: &[Vec<f64>], ret_pin: bool) {
    let bin = ngspice_bin();
    if Command::new(&bin).arg("--version").output().is_err() {
        eprintln!(
            "SKIPPED ngspice check ({what}): `{bin}` is not runnable (set GEODE_NGSPICE or put \
             ngspice on PATH to enable; not a CI dependency)"
        );
        return;
    }
    let n = l.len();
    let max_diag = (0..n).map(|i| l[i][i]).fold(0.0, f64::max);
    let dir = subckt.parent().unwrap();
    for k in 0..n {
        let mut ports: Vec<String> = (1..=n).map(|j| format!("p{j}")).collect();
        let r = if ret_pin { "r" } else { "0" };
        if ret_pin {
            ports.push(r.to_string());
        }
        let mut deck = format!(
            "geode inductance spice check\n.include {}\nX1 {} LEXTRACT\n",
            subckt.display(),
            ports.join(" ")
        );
        if ret_pin {
            deck += "RDC r 0 1\n";
        }
        deck += &format!("I{} {r} p{} DC 0 AC 1\n", k + 1, k + 1);
        deck += ".control\nset numdgt=15\nac lin 1 1meg 1meg\n";
        for j in 1..=n {
            let v = if ret_pin {
                format!("v(p{j})-v(r)")
            } else {
                format!("v(p{j})")
            };
            deck += &format!("let l{j} = imag({v})/(2*pi*1e6)\nprint l{j}\n");
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
            let key = format!("l{} = ", j + 1);
            let got: f64 = stdout
                .lines()
                .find_map(|line| line.trim().strip_prefix(&key))
                .unwrap_or_else(|| panic!("no `{key}` in ngspice output:\n{stdout}"))
                .trim()
                .parse()
                .unwrap();
            let err = (got - l[j][k]).abs() / max_diag;
            eprintln!(
                "ngspice {what}: L[{j}][{k}] = {got:e} vs {:e} (rel {err:e})",
                l[j][k]
            );
            assert!(err <= 1e-9, "ngspice L[{j}][{k}] {got:e} vs {:e}", l[j][k]);
        }
    }
}
