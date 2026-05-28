//! Build-time linker glue for the optional `arpack` feature.
//!
//! The default (and only feature-off) build is **pure Rust**: this script
//! does nothing. When `--features arpack` is on, we ask `pkg-config` for
//! the system `libarpack` and emit the standard `rustc-link-*` directives.
//!
//! Bindings to the ARPACK ICB (Iterative Common Bindings) C wrappers
//! `dsaupd_c` / `dseupd_c` are vendored as hand-written `extern "C"`
//! declarations in `src/arpack.rs` — there is no `bindgen` step here.
//! That keeps the build hermetic (no `clang` or `gfortran` toolchain
//! requirement) and side-steps the macOS Homebrew quirk where the
//! `arpack.pc` `includedir=${prefix}/include/arpack` points one level too
//! deep for `arpack-ng-sys`'s `#include <arpack/arpack.h>` shim to
//! resolve.
//!
//! See the README's `## System dependencies` section for the platform-
//! specific install one-liners.
//!
//! ## Environment overrides
//!
//! - `ARPACK_LIB_DIR` — directory containing `libarpack.{so,dylib,a}`.
//!   If set, used instead of pkg-config.
//! - `ARPACK_STATIC=1` — link `arpack` statically rather than dynamically.

fn main() {
    // Without the feature, this script is a no-op. The `pkg_config`
    // build-dependency is also gated on the feature in Cargo.toml, so we
    // can't even reference its types unless `arpack` is on.
    #[cfg(feature = "arpack")]
    arpack_link();
}

#[cfg(feature = "arpack")]
fn arpack_link() {
    println!("cargo:rerun-if-env-changed=ARPACK_LIB_DIR");
    println!("cargo:rerun-if-env-changed=ARPACK_STATIC");

    let static_link = std::env::var("ARPACK_STATIC")
        .map(|v| v == "1" || v.eq_ignore_ascii_case("true"))
        .unwrap_or(false);
    let kind = if static_link { "static" } else { "dylib" };

    if let Ok(dir) = std::env::var("ARPACK_LIB_DIR") {
        println!("cargo:rustc-link-search=native={dir}");
        println!("cargo:rustc-link-lib={kind}=arpack");
        return;
    }

    // pkg-config path: Homebrew ships /opt/homebrew/opt/arpack/lib/pkgconfig/arpack.pc;
    // Debian/Ubuntu's libarpack2-dev ships /usr/lib/x86_64-linux-gnu/pkgconfig/arpack.pc.
    let probe = pkg_config::Config::new()
        .statik(static_link)
        .probe("arpack");

    match probe {
        Ok(_) => {
            // pkg-config emits cargo:rustc-link-* itself; nothing else to do.
        }
        Err(e) => {
            eprintln!(
                "warning: pkg-config could not find `arpack` ({e}). \
                 Falling back to bare `-larpack` and hoping the linker can resolve it. \
                 Set ARPACK_LIB_DIR=/path/to/libarpack to override."
            );
            println!("cargo:rustc-link-lib={kind}=arpack");
        }
    }
}
