//! Fixture adaptors.
//!
//! Staging home (Epic #414, Phase 2) for the fixture-output glue that the
//! standalone example crates would otherwise copy-paste: the TOML
//! write-to-disk tail, the indexed per-row TOML table seam, the ParaView
//! `.pvd` collection writer, and the frequency-sweep point helper used to
//! generate and record regression fixtures.
//!
//! Each example assembles its own bespoke `[meta]` / oracle / geometry
//! prose (those sections are unique per benchmark and not shared); the
//! reusable pieces extracted here are the parts that were genuinely
//! identical across crates.

use std::fs;
use std::io;
use std::path::Path;

/// Write an assembled TOML document to `path`, creating parent directories
/// and logging `wrote <path>` to stderr.
///
/// Centralises the `create_dir_all(parent)` + `fs::write` + `eprintln!`
/// tail that every example's fixture writer repeated verbatim.
pub fn write_toml(path: &Path, contents: &str) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, contents)?;
    eprintln!("wrote {}", path.display());
    Ok(())
}

/// A single fixture-results row that can serialise its own `key = value`
/// body into an indexed TOML table.
///
/// The shared [`push_rows`] driver emits one `[<TABLE_PREFIX>_<i>]` table
/// per row (followed by a blank line), matching the hand-rolled per-row
/// loop the RF-sweep / eigenmode example crates previously copy-pasted.
pub trait TomlRow {
    /// Table-name prefix; the emitted header for row `i` is
    /// `[<TABLE_PREFIX>_<i>]` (e.g. `point`, `mode`).
    const TABLE_PREFIX: &'static str;

    /// Append this row's `key = value` lines into `out`.
    ///
    /// Implementations write the body only — no table header and no
    /// trailing blank line; [`push_rows`] frames each row.
    fn write_fields(&self, out: &mut String);
}

/// Append `rows` to `out` as a sequence of indexed `[<prefix>_<i>]` TOML
/// tables, each followed by a blank line.
///
/// Replaces the `for (i, r) in rows.iter().enumerate() { ... }` table
/// loop duplicated across the example fixture writers.
pub fn push_rows<T: TomlRow>(out: &mut String, rows: &[T]) {
    for (i, r) in rows.iter().enumerate() {
        out.push_str(&format!("[{}_{i}]\n", T::TABLE_PREFIX));
        r.write_fields(out);
        out.push('\n');
    }
}

/// Write a ParaView `.pvd` collection mapping each frame's `.vtu` file to
/// a `timestep`, so ParaView (and `sweep_animate.py`) treats a frequency
/// sweep as a time-series.
///
/// `frames` is `(timestep, file_name)` pairs where `file_name` is the
/// frame's path **relative to the `.pvd`** (e.g. `E_0000.vtu`). The `.pvd`
/// is a tiny hand-rolled XML (no XML dependency).
pub fn write_pvd(path: &Path, frames: &[(f64, String)]) -> io::Result<()> {
    let mut s = String::new();
    s.push_str("<?xml version=\"1.0\"?>\n");
    s.push_str("<VTKFile type=\"Collection\" version=\"0.1\" byte_order=\"LittleEndian\">\n");
    s.push_str("  <Collection>\n");
    for (timestep, file) in frames {
        s.push_str(&format!(
            "    <DataSet timestep=\"{timestep}\" group=\"\" part=\"0\" file=\"{file}\"/>\n"
        ));
    }
    s.push_str("  </Collection>\n");
    s.push_str("</VTKFile>\n");
    fs::write(path, s)
}

/// Evenly-spaced sweep frequencies (GHz) over `[f_start_ghz, f_stop_ghz]`
/// inclusive.
///
/// A single-point sweep (`n <= 1`) returns just `f_start_ghz`.
pub fn sweep_freqs(f_start_ghz: f64, f_stop_ghz: f64, n: usize) -> Vec<f64> {
    let n = n.max(1);
    if n == 1 {
        return vec![f_start_ghz];
    }
    let step = (f_stop_ghz - f_start_ghz) / (n - 1) as f64;
    (0..n).map(|i| f_start_ghz + step * i as f64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    /// Unique scratch path under the system temp dir (no `tempfile` dep).
    fn scratch(name: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "geode-util-fixture-{}-{nanos}-{name}",
            std::process::id()
        ))
    }

    struct Pt {
        f_ghz: f64,
        q: f64,
    }

    impl TomlRow for Pt {
        const TABLE_PREFIX: &'static str = "point";
        fn write_fields(&self, out: &mut String) {
            out.push_str(&format!("f_ghz = {:.3e}\n", self.f_ghz));
            out.push_str(&format!("q = {:.3e}\n", self.q));
        }
    }

    #[test]
    fn push_rows_emits_indexed_tables() {
        let rows = vec![
            Pt {
                f_ghz: 1.0,
                q: 10.0,
            },
            Pt {
                f_ghz: 2.0,
                q: 20.0,
            },
        ];
        let mut s = String::new();
        push_rows(&mut s, &rows);
        let expected = "\
[point_0]
f_ghz = 1.000e0
q = 1.000e1

[point_1]
f_ghz = 2.000e0
q = 2.000e1

";
        assert_eq!(s, expected);
    }

    #[test]
    fn write_toml_creates_parents_and_writes() {
        let dir = scratch("toml");
        let path = dir.join("nested").join("results.toml");
        write_toml(&path, "[meta]\nx = 1\n").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "[meta]\nx = 1\n");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn sweep_freqs_inclusive_endpoints() {
        let f = sweep_freqs(1.0, 3.0, 3);
        assert_eq!(f, vec![1.0, 2.0, 3.0]);
        // Single-point sweep collapses to the start frequency.
        assert_eq!(sweep_freqs(2.5, 9.0, 1), vec![2.5]);
        assert_eq!(sweep_freqs(2.5, 9.0, 0), vec![2.5]);
    }

    #[test]
    fn write_pvd_emits_collection_xml() {
        let path = scratch("sweep.pvd");
        let frames = vec![
            (1.0_f64, "E_0000.vtu".to_string()),
            (2.0, "E_0001.vtu".to_string()),
        ];
        write_pvd(&path, &frames).unwrap();
        let xml = fs::read_to_string(&path).unwrap();
        assert!(xml.starts_with("<?xml version=\"1.0\"?>\n"));
        assert!(
            xml.contains("<DataSet timestep=\"1\" group=\"\" part=\"0\" file=\"E_0000.vtu\"/>")
        );
        assert!(
            xml.contains("<DataSet timestep=\"2\" group=\"\" part=\"0\" file=\"E_0001.vtu\"/>")
        );
        assert!(xml.trim_end().ends_with("</VTKFile>"));
        let _ = fs::remove_file(&path);
    }
}
