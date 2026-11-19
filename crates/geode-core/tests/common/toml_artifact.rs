//! Shared TOML-artifact accessors for the `patch_conformal_*` integration
//! tests: dotted-key lookup plus the pinned-value assertion.

#![allow(dead_code)] // each test binary uses only a subset of these helpers

/// Walk a dotted key path (`"a.b.c"`) into a TOML value, panicking on a miss.
pub fn get<'a>(v: &'a toml::Value, dotted: &str) -> &'a toml::Value {
    let mut cur = v;
    for k in dotted.split('.') {
        cur = cur
            .get(k)
            .unwrap_or_else(|| panic!("missing key {dotted} (at {k})"));
    }
    cur
}

/// The number (float or integer) at `dotted`, as `f64`.
pub fn f(v: &toml::Value, dotted: &str) -> f64 {
    let c = get(v, dotted);
    c.as_float()
        .or_else(|| c.as_integer().map(|i| i as f64))
        .unwrap_or_else(|| panic!("{dotted} is not a number"))
}

/// Like [`f`], but a non-numeric value panics with the offending TOML value
/// (`expected a number, got ..`) via [`super::toml_num`]. The design C, Palace
/// loop and tuned targets pinned that diagnostic before the helpers were shared.
pub fn f_dbg(v: &toml::Value, dotted: &str) -> f64 {
    super::toml_num(get(v, dotted))
}

/// The string at `dotted`.
pub fn s<'a>(v: &'a toml::Value, dotted: &str) -> &'a str {
    get(v, dotted)
        .as_str()
        .unwrap_or_else(|| panic!("{dotted} is not a string"))
}

/// Assert `got` is within `tol` of the pinned value `want`.
#[track_caller]
pub fn pin(got: f64, want: f64, tol: f64, what: &str) {
    assert!(
        (got - want).abs() <= tol,
        "{what}: {got} differs from the pinned {want} by more than {tol}"
    );
}
