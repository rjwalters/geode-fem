//! Walk `target/criterion/` and write a clean TOML summary of every
//! bench/input combination's median + IQR to
//! `benchmarks/perf/baseline.toml`.
//!
//! Criterion writes per-bench results to
//! `target/criterion/<group>/<input?>/new/estimates.json`. The JSON
//! has the shape (with redundant fields elided):
//!
//! ```json
//! {
//!   "mean":   {"point_estimate": 1234.5, ...},
//!   "median": {"point_estimate": 1230.0, ...},
//!   "std_dev":{"point_estimate":   12.3, ...},
//!   "median_abs_dev":{"point_estimate": 8.1, ...}
//! }
//! ```
//!
//! All times are in **nanoseconds**.
//!
//! # Usage
//!
//! 1. Run the benches once: `cargo bench -p geode-core`.
//! 2. Then: `cargo run -p extract_baseline`.
//!
//! The extractor is **not** wired into `cargo bench` itself — keep
//! the two steps separate so re-running just the analysis is cheap
//! and side-effect-free with respect to the criterion HTML reports.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;
use geode_app::{App, Verbosity};
use serde::Deserialize;

/// Criterion's per-statistic shape inside `estimates.json`.
#[derive(Debug, Deserialize)]
struct Estimate {
    point_estimate: f64,
}

/// Subset of fields we care about from `estimates.json`.
#[derive(Debug, Deserialize)]
struct Estimates {
    median: Estimate,
    /// Median absolute deviation — proxy for IQR / robust spread.
    median_abs_dev: Estimate,
    mean: Estimate,
}

/// One row in the eventual baseline TOML.
#[derive(Debug)]
struct Row {
    group: String,
    input: Option<String>,
    median_ns: f64,
    median_abs_dev_ns: f64,
    mean_ns: f64,
}

/// Workspace root, two parents above `examples/extract_baseline/`.
///
/// `CARGO_MANIFEST_DIR` is the crate root (`examples/extract_baseline`);
/// walking up two levels (`examples/` → workspace root) lands on the same
/// directory the old `crates/geode-core` manifest dir reached, so
/// `target/criterion` and `benchmarks/perf/baseline.toml` resolve to the
/// identical repo paths as before the Epic #398 migration.
fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("crate parent")
        .parent()
        .expect("workspace parent")
        .to_path_buf()
}

/// Walk `target/criterion` and collect every `new/estimates.json`.
///
/// Criterion's layout is:
///
/// ```text
/// target/criterion/
///   <group>/
///     report/...        # group-level summary (skip)
///     <input>/
///       new/estimates.json
///       ...
/// ```
///
/// For ungrouped `bench_function` calls the inputs dir is omitted and
/// `estimates.json` lives directly under `<group>/new/`. Both cases
/// are handled.
fn collect_rows(criterion_dir: &Path) -> Vec<Row> {
    let mut rows = Vec::new();
    let Ok(read) = fs::read_dir(criterion_dir) else {
        eprintln!("no criterion output at {}", criterion_dir.display());
        return rows;
    };
    for group_entry in read.flatten() {
        let group_path = group_entry.path();
        if !group_path.is_dir() {
            continue;
        }
        let group_name = group_entry.file_name().to_string_lossy().to_string();
        // Skip top-level `report` directory which has no estimates.
        if group_name == "report" {
            continue;
        }

        // Case A: bench_function (no inputs) — estimates directly under the group.
        let direct = group_path.join("new").join("estimates.json");
        if direct.is_file() {
            if let Some(row) = parse_estimates(&direct, &group_name, None) {
                rows.push(row);
            }
            continue;
        }

        // Case B: bench_with_input — each child dir is a parameter point.
        let Ok(inner) = fs::read_dir(&group_path) else {
            continue;
        };
        for input_entry in inner.flatten() {
            let input_path = input_entry.path();
            if !input_path.is_dir() {
                continue;
            }
            let input_name = input_entry.file_name().to_string_lossy().to_string();
            if input_name == "report" {
                continue;
            }
            let est = input_path.join("new").join("estimates.json");
            if est.is_file()
                && let Some(row) = parse_estimates(&est, &group_name, Some(input_name))
            {
                rows.push(row);
            }
        }
    }

    // Stable ordering for diffability.
    rows.sort_by(|a, b| a.group.cmp(&b.group).then(a.input.cmp(&b.input)));
    rows
}

fn parse_estimates(path: &Path, group: &str, input: Option<String>) -> Option<Row> {
    let bytes = fs::read(path).ok()?;
    let est: Estimates = serde_json::from_slice(&bytes).ok()?;
    Some(Row {
        group: group.to_string(),
        input,
        median_ns: est.median.point_estimate,
        median_abs_dev_ns: est.median_abs_dev.point_estimate,
        mean_ns: est.mean.point_estimate,
    })
}

/// Format nanoseconds as a human-readable string with auto-scaled units.
fn fmt_ns(ns: f64) -> String {
    let abs = ns.abs();
    if abs >= 1.0e9 {
        format!("{:.3} s", ns / 1.0e9)
    } else if abs >= 1.0e6 {
        format!("{:.3} ms", ns / 1.0e6)
    } else if abs >= 1.0e3 {
        format!("{:.3} µs", ns / 1.0e3)
    } else {
        format!("{:.3} ns", ns)
    }
}

fn write_toml(rows: &[Row], out_path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut s = String::new();
    s.push_str("# Auto-generated by `cargo run -p extract_baseline`.\n");
    s.push_str("# Times are medians + median-absolute-deviations (IQR proxy) over\n");
    s.push_str("# the criterion samples written under `target/criterion/`.\n");
    s.push_str("# All times in nanoseconds; the `human` field is a courtesy\n");
    s.push_str("# pretty-print.\n");
    s.push('\n');

    s.push_str("[meta]\n");
    s.push_str(
        "description = \"Performance baseline for geode-core assembly + eigensolve (issue #50).\"\n",
    );
    s.push_str(&format!("n_rows = {}\n", rows.len()));
    s.push('\n');

    // Group rows by bench group so the TOML is easy to skim.
    let mut by_group: BTreeMap<&str, Vec<&Row>> = BTreeMap::new();
    for r in rows {
        by_group.entry(r.group.as_str()).or_default().push(r);
    }

    for (group, group_rows) in by_group {
        for r in group_rows {
            let key = match &r.input {
                Some(input) => format!("{group}.{input}"),
                None => group.to_string(),
            };
            // Quote the table header so dotted/numeric inputs (e.g. "10")
            // do not collide with the dotted-table syntax.
            s.push_str(&format!("[\"{}\"]\n", key));
            s.push_str(&format!("group = \"{}\"\n", r.group));
            if let Some(input) = &r.input {
                s.push_str(&format!("input = \"{}\"\n", input));
            }
            s.push_str(&format!("median_ns = {:.3}\n", r.median_ns));
            s.push_str(&format!("median_abs_dev_ns = {:.3}\n", r.median_abs_dev_ns));
            s.push_str(&format!("mean_ns = {:.3}\n", r.mean_ns));
            s.push_str(&format!("human = \"{}\"\n", fmt_ns(r.median_ns)));
            s.push('\n');
        }
    }

    fs::create_dir_all(out_path.parent().expect("baseline parent"))?;
    fs::write(out_path, s)?;
    eprintln!("wrote {}", out_path.display());
    Ok(())
}

/// Extract a perf baseline TOML from the criterion output tree.
#[derive(Parser)]
#[command(
    about = "Walk target/criterion and write benchmarks/perf/baseline.toml (medians + IQR; issue #50)."
)]
struct Args {
    /// Verbosity (`-v` / `-q`), fed to the logging seam.
    #[command(flatten)]
    verbose: Verbosity,
}

impl App for Args {
    fn run(self) -> Result<(), Box<dyn std::error::Error>> {
        let root = workspace_root();
        let criterion_dir = root.join("target").join("criterion");
        let rows = collect_rows(&criterion_dir);
        if rows.is_empty() {
            return Err(format!(
                "no rows extracted — did you run `cargo bench -p geode-core` first? \
                 looked at {}",
                criterion_dir.display()
            )
            .into());
        }
        eprintln!(
            "extracted {} rows from {}",
            rows.len(),
            criterion_dir.display()
        );

        let out = root.join("benchmarks").join("perf").join("baseline.toml");
        write_toml(&rows, &out)?;

        eprintln!();
        eprintln!("{:<32} {:>12} {:>12}", "bench", "median", "mean");
        eprintln!("{}", "-".repeat(60));
        for r in &rows {
            let label = match &r.input {
                Some(i) => format!("{}/{}", r.group, i),
                None => r.group.clone(),
            };
            eprintln!(
                "{:<32} {:>12} {:>12}",
                label,
                fmt_ns(r.median_ns),
                fmt_ns(r.mean_ns)
            );
        }

        Ok(())
    }

    fn verbosity(&self) -> Verbosity {
        self.verbose
    }
}

fn main() -> ExitCode {
    geode_app::main::<Args>()
}
