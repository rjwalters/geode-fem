//! Design gradients `dJ/dθ` of the ICEM sector torque over the **free-form
//! control-point design space** (issue #1179): the 19 geometric components
//! of [`IcemCpShapeMap`] (9 scalars and 10 control-point offsets) through
//! the shape adjoint of [`SectorShapeProblem`] (#1174), plus `LENGTH`,
//! `OPERATING_ANGLE` and `ScaleR`, and the step sweeps that re-move the
//! mesh.
//!
//! This extends `icem_design.rs` (#1177) to the wider space and reuses its
//! current rules, couplings, evaluations, step rows and Taylor window. The
//! non-geometric components and both current rules follow the definitions
//! documented there. The including crate root must declare `mod icem;`,
//! `mod icem_shape;` and `mod icem_design;` next to this module.
//!
//! # What lives here
//!
//! Only what is specific to the control-point space: the component count
//! and names, the offset index, [`ControlPointSpace`] (this space's
//! [`DesignSpace`] instance) and [`CpDesignPoint::from_reference`], which
//! takes the free-form offsets. The point, set-up, gradient and every
//! driver are [`icem_design`](crate::icem_design)'s generic items
//! instantiated at [`ControlPointSpace`] — the same code the 9-parameter
//! space runs, so a correction to the contraction reaches both spaces at
//! once (#1634).
#![allow(dead_code)] // each binary uses a subset

use geode_core::mesh::TaggedTriMesh;
use geode_core::mesh::icem_controlpoints::{
    IcemCpDesign, IcemCpShapeMap, N_CP_PARAMS, N_OFFSETS, OFFSET_NAMES, cp_param_names,
};
use geode_core::mesh::icem_parametric::{IcemParamError, N_PARAMS, PARAM_NAMES};
use serde_json::Value;

use crate::icem_design::{
    DesignSpace, N_NON_GEOM, SpaceGradient, SpacePoint, SpaceSetup, k_angle, k_length, k_scale,
};

// The generic drivers, instantiated at `ControlPointSpace` by the argument
// types below. Re-exported so `icem_cp_design::evaluate(&setup, …)` keeps
// naming this module, as it did when the body lived here.
#[allow(unused_imports)]
pub use crate::icem_design::{
    design_gradient, design_gradient_from, evaluate, objectives_at, step_sweep,
};

/// Components: 19 geometric, `LENGTH`, `OPERATING_ANGLE`, `ScaleR`.
pub const N_CP_DESIGN: usize = N_CP_PARAMS + N_NON_GEOM;
/// Index of `LENGTH`.
pub const KC_LENGTH: usize = k_length(N_CP_PARAMS);
/// Index of `OPERATING_ANGLE`.
pub const KC_ANGLE: usize = k_angle(N_CP_PARAMS);
/// Index of `ScaleR`.
pub const KC_SCALE: usize = k_scale(N_CP_PARAMS);

/// Component names: [`cp_param_names`] then the three non-geometric ones.
pub fn cp_design_names() -> [&'static str; N_CP_DESIGN] {
    let mut out = [""; N_CP_DESIGN];
    out[..N_CP_PARAMS].copy_from_slice(&cp_param_names());
    out[KC_LENGTH] = "LENGTH";
    out[KC_ANGLE] = "OPERATING_ANGLE";
    out[KC_SCALE] = "ScaleR";
    out
}

/// Index of an offset component.
pub fn offset_index(name: &str) -> usize {
    N_PARAMS
        + OFFSET_NAMES
            .iter()
            .position(|n| *n == name)
            .unwrap_or_else(|| panic!("unknown offset {name}"))
}

/// The 19-component free-form space of [`IcemCpShapeMap`] (#1179): the 9
/// scalars of the parametric space and the 10 control-point offsets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ControlPointSpace;

impl DesignSpace for ControlPointSpace {
    type Geo = IcemCpDesign;
    type Map = IcemCpShapeMap;
    type DesignArray = [f64; N_CP_DESIGN];
    type GeomArray = [f64; N_CP_PARAMS];

    const N_GEOM: usize = N_CP_PARAMS;

    fn geo_get(geo: &IcemCpDesign, k: usize) -> f64 {
        geo.values[k]
    }

    fn geo_set(geo: &mut IcemCpDesign, k: usize, v: f64) {
        geo.values[k] = v;
    }

    fn design_zeros() -> [f64; N_CP_DESIGN] {
        [0.0; N_CP_DESIGN]
    }

    fn geom_zeros() -> [f64; N_CP_PARAMS] {
        [0.0; N_CP_PARAMS]
    }

    fn map_new(
        tagged: &TaggedTriMesh,
        theta0: IcemCpDesign,
    ) -> Result<IcemCpShapeMap, IcemParamError> {
        IcemCpShapeMap::new(tagged, theta0)
    }

    fn positions(
        map: &IcemCpShapeMap,
        geo: &IcemCpDesign,
    ) -> Result<Vec<[f64; 2]>, IcemParamError> {
        map.positions(geo)
    }

    fn jacobian(
        map: &IcemCpShapeMap,
        geo: &IcemCpDesign,
    ) -> Result<Vec<Vec<[f64; 2]>>, IcemParamError> {
        map.jacobian(geo)
    }
}

/// A full design point of the control-point space.
pub type CpDesignPoint = SpacePoint<ControlPointSpace>;

/// The model on one mesh, its shape problem and the control-point map.
pub type CpDesignSetup = SpaceSetup<ControlPointSpace>;

/// The design gradient of both objectives over the 22 components.
pub type CpDesignGradient = SpaceGradient<ControlPointSpace>;

impl CpDesignPoint {
    /// Design `which` (`initial` or `optimized`) of `reference_data.json`
    /// with the free-form `offsets`.
    pub fn from_reference(reference: &Value, which: &str, offsets: &[f64; N_OFFSETS]) -> Self {
        let p = &reference["designs"][which]["parameters_by_source_registration_order"];
        let get = |n: &str| p[n].as_f64().expect("parameter");
        let mut scalars = geode_core::mesh::icem_parametric::IcemDesign {
            values: [0.0; N_PARAMS],
        };
        for (k, n) in PARAM_NAMES.iter().enumerate() {
            scalars.values[k] = get(n);
        }
        Self {
            geo: IcemCpDesign::from_parts(&scalars, offsets),
            length: get("LENGTH"),
            phi0_deg: get("OPERATING_ANGLE"),
            scale_r: get("ScaleR"),
        }
    }
}
