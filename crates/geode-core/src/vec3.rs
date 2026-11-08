//! Shared 3-vector helpers (`[f64; 3]`) and the sorted-face-key helper.
//!
//! One home for the arithmetic that assembly, adaptivity, mesh and element
//! modules previously each re-declared privately.

#[inline]
pub(crate) fn sub3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

#[inline]
pub(crate) fn cross3(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

#[inline]
pub(crate) fn norm3(a: [f64; 3]) -> f64 {
    (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt()
}

#[inline]
pub(crate) fn dot3(a: [f64; 3], b: [f64; 3]) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

#[inline]
pub(crate) fn scale3(a: [f64; 3], s: f64) -> [f64; 3] {
    [a[0] * s, a[1] * s, a[2] * s]
}

/// Signed tet volume `V = det[p1-p0, p2-p0, p3-p0] / 6` (positive for
/// conventionally-oriented tets). The single home for this formula.
#[inline]
pub fn tet_signed_volume(p: &[[f64; 3]; 4]) -> f64 {
    dot3(sub3(p[1], p[0]), cross3(sub3(p[2], p[0]), sub3(p[3], p[0]))) / 6.0
}

/// Sort a triangle's three node ids ascending (canonical face key).
#[inline]
pub(crate) fn sorted3(mut t: [u32; 3]) -> [u32; 3] {
    t.sort_unstable();
    t
}
