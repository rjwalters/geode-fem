//! Gmsh availability probe shared by the `geode` CLI integration tests that
//! generate a mesh with Gmsh (issue #1610).
//!
//! Include with `#[path = "support/gmsh.rs"] mod gmsh;`.
#![allow(dead_code)] // each test binary uses a different subset

use std::process::Command;

/// `true` if a Gmsh binary is runnable (`GEODE_GMSH` if set and non-empty,
/// else `gmsh` on `PATH`).
///
/// Otherwise the test is skipped *loudly* on stderr and `false` is
/// returned — unless `GEODE_REQUIRE_GMSH=1` (CI), where a missing Gmsh is a
/// panic rather than a silent pass.
pub fn gmsh_or_skip(test: &str) -> bool {
    let bin = std::env::var_os("GEODE_GMSH")
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "gmsh".into());
    let found = Command::new(&bin)
        .arg("--version")
        .output()
        .is_ok_and(|o| o.status.success());
    if found {
        return true;
    }
    let msg = format!(
        "gmsh ({}) is not runnable: install Gmsh (`apt-get install gmsh` / `brew install gmsh`) \
         or set GEODE_GMSH",
        bin.to_string_lossy()
    );
    if std::env::var("GEODE_REQUIRE_GMSH").is_ok_and(|v| v == "1") {
        panic!("GEODE_REQUIRE_GMSH=1 but {msg}");
    }
    eprintln!(
        "\n########################################################################\n\
         ## SKIPPING {test}: {msg}\n\
         ########################################################################\n"
    );
    false
}
