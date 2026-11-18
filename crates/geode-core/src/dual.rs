//! Minimal forward-mode dual number shared by the geometry-kernel modules.
//!
//! One `Dual` (`re + du·ϵ`, `ϵ² = 0`) and a handful of 3-vector helpers give
//! exact forward-mode automatic differentiation of the closed-form element
//! kernels w.r.t. a single seeded node coordinate. It lives at the crate root
//! (not under `driven`) so the electrostatic [`crate::shape`] path does not
//! import from the driven-solver tree.

/// A first-order **dual number** `re + du·ϵ` (`ϵ² = 0`) for exact forward-mode
/// automatic differentiation of the closed-form element kernels (P1 stiffness
/// in [`crate::shape`], Nédélec / surface-Whitney kernels in
/// [`crate::driven::shape`]). Seeding one node coordinate with `du = 1` (all
/// others `du = 0`) and evaluating a kernel in dual arithmetic returns, in the
/// `.du` fields of its outputs, the exact partial derivatives w.r.t. that
/// coordinate.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Dual {
    pub(crate) re: f64,
    pub(crate) du: f64,
}

impl Dual {
    #[inline]
    pub(crate) fn cst(re: f64) -> Self {
        Self { re, du: 0.0 }
    }
    #[inline]
    pub(crate) fn var(re: f64) -> Self {
        Self { re, du: 1.0 }
    }
    #[inline]
    pub(crate) fn add(self, o: Self) -> Self {
        Self {
            re: self.re + o.re,
            du: self.du + o.du,
        }
    }
    #[inline]
    pub(crate) fn sub(self, o: Self) -> Self {
        Self {
            re: self.re - o.re,
            du: self.du - o.du,
        }
    }
    #[inline]
    pub(crate) fn mul(self, o: Self) -> Self {
        Self {
            re: self.re * o.re,
            du: self.du * o.re + self.re * o.du,
        }
    }
    #[inline]
    pub(crate) fn div(self, o: Self) -> Self {
        let inv = 1.0 / o.re;
        Self {
            re: self.re * inv,
            du: (self.du * o.re - self.re * o.du) * inv * inv,
        }
    }
    #[inline]
    pub(crate) fn neg(self) -> Self {
        Self {
            re: -self.re,
            du: -self.du,
        }
    }
    /// `|x|`, sub-gradient at the (measure-zero) kink taken as the right
    /// derivative. The element determinant is bounded away from zero on any
    /// valid mesh, so the kink is never hit here.
    #[inline]
    pub(crate) fn abs(self) -> Self {
        if self.re >= 0.0 { self } else { self.neg() }
    }
    /// `√x`, `du = x.du / (2√x.re)`. Used by the surface-Whitney twin
    /// `port_face_dual` in [`crate::driven::shape`] for the triangle area
    /// `½‖e₁×e₂‖` (the tet element kernels never take a square root). The port
    /// area is bounded away from zero on any valid boundary mesh, so the
    /// `x.re = 0` singularity is never hit.
    #[inline]
    pub(crate) fn sqrt(self) -> Self {
        let r = self.re.sqrt();
        Self {
            re: r,
            du: self.du / (2.0 * r),
        }
    }
    /// Multiply by an `f64` constant (a lifted scalar with zero tangent).
    #[inline]
    pub(crate) fn scale(self, c: f64) -> Self {
        Self {
            re: self.re * c,
            du: self.du * c,
        }
    }
}

#[inline]
pub(crate) fn dsub3(a: [Dual; 3], b: [Dual; 3]) -> [Dual; 3] {
    [a[0].sub(b[0]), a[1].sub(b[1]), a[2].sub(b[2])]
}
#[inline]
pub(crate) fn dcross3(a: [Dual; 3], b: [Dual; 3]) -> [Dual; 3] {
    [
        a[1].mul(b[2]).sub(a[2].mul(b[1])),
        a[2].mul(b[0]).sub(a[0].mul(b[2])),
        a[0].mul(b[1]).sub(a[1].mul(b[0])),
    ]
}
#[inline]
pub(crate) fn ddot3(a: [Dual; 3], b: [Dual; 3]) -> Dual {
    a[0].mul(b[0]).add(a[1].mul(b[1])).add(a[2].mul(b[2]))
}
/// Scale a dual 3-vector by an `f64` constant (a lifted scalar, zero tangent).
#[inline]
pub(crate) fn dscale3(s: f64, a: [Dual; 3]) -> [Dual; 3] {
    [a[0].scale(s), a[1].scale(s), a[2].scale(s)]
}
/// The `f64`-scalar linear combination `s·a + t·b` of two dual 3-vectors
/// (`s`, `t` are fixed reference barycentric weights with zero tangent).
#[inline]
pub(crate) fn dlc3(s: f64, a: [Dual; 3], t: f64, b: [Dual; 3]) -> [Dual; 3] {
    [
        a[0].scale(s).add(b[0].scale(t)),
        a[1].scale(s).add(b[1].scale(t)),
        a[2].scale(s).add(b[2].scale(t)),
    ]
}
