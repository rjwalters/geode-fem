//! Bit-exact `f64` (de)serialization for checkpoints.
//!
//! `serde_json` writes floats as JSON numbers, but its default parser is
//! only *best-effort* precise (exact parsing is the opt-in `float_roundtrip`
//! feature, which this workspace does not enable). JSON numbers also cannot
//! carry `±inf`, which box bounds use for "unbounded". A resumed run must
//! continue **bit-identically**, so every float in a checkpoint goes through
//! this module instead:
//!
//! - A float is written as a string holding Rust's shortest round-trip
//!   exponential form, `format!("{v:e}")` (e.g. `"1.2345e-3"`, `"-0e0"`,
//!   `"inf"`, `"-inf"`). `NaN` is written as `"NaN"`.
//! - It is read back with `str::parse::<f64>`, which is correctly rounded.
//!   So `parse(format(v)).to_bits() == v.to_bits()` for every non-NaN `v`,
//!   including `-0.0` and the infinities.
//!
//! Plain JSON numbers are also accepted on input, so a hand-written
//! checkpoint (or `x0`) can say `1.5` instead of `"1.5e0"`.

use serde::de::{self, Deserializer, SeqAccess, Visitor};
use serde::ser::SerializeSeq;
use std::fmt;

/// The checkpoint string form of `v` (see the module docs).
pub fn format_f64(v: f64) -> String {
    if v.is_nan() {
        "NaN".to_string()
    } else {
        format!("{v:e}")
    }
}

/// Parses the checkpoint string form of a float.
///
/// # Errors
///
/// The string is not a float literal (`inf` / `-inf` / `NaN` included).
pub fn parse_f64(s: &str) -> Result<f64, String> {
    s.trim()
        .parse::<f64>()
        .map_err(|e| format!("invalid float {s:?}: {e}"))
}

struct F64Visitor;

impl<'de> Visitor<'de> for F64Visitor {
    type Value = f64;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("a float written as a string (e.g. \"1.5e0\", \"inf\") or a number")
    }

    fn visit_str<E: de::Error>(self, v: &str) -> Result<f64, E> {
        parse_f64(v).map_err(E::custom)
    }

    fn visit_f64<E: de::Error>(self, v: f64) -> Result<f64, E> {
        Ok(v)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_i64<E: de::Error>(self, v: i64) -> Result<f64, E> {
        Ok(v as f64)
    }

    #[allow(clippy::cast_precision_loss)]
    fn visit_u64<E: de::Error>(self, v: u64) -> Result<f64, E> {
        Ok(v as f64)
    }
}

struct ExactF64(f64);

impl<'de> serde::Deserialize<'de> for ExactF64 {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        d.deserialize_any(F64Visitor).map(ExactF64)
    }
}

/// `#[serde(with = "exact::f64s")]` for a single `f64`.
pub mod f64s {
    use super::{ExactF64, format_f64};
    use serde::{Deserialize, Deserializer, Serializer};

    /// Serializes one float as its exact string form.
    ///
    /// # Errors
    ///
    /// Propagates the serializer's error.
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn serialize<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&format_f64(*v))
    }

    /// Deserializes one float from its exact string form (or a number).
    ///
    /// # Errors
    ///
    /// Not a float.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
        ExactF64::deserialize(d).map(|e| e.0)
    }
}

/// `#[serde(with = "exact::vec")]` for a `Vec<f64>`.
pub mod vec {
    use super::{ExactF64, SeqAccess, SerializeSeq, Visitor, fmt, format_f64};
    use serde::{Deserializer, Serializer};

    /// Serializes a vector of floats element-wise in exact string form.
    ///
    /// # Errors
    ///
    /// Propagates the serializer's error.
    pub fn serialize<S: Serializer>(v: &[f64], s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(v.len()))?;
        for x in v {
            seq.serialize_element(&format_f64(*x))?;
        }
        seq.end()
    }

    struct VecVisitor;

    impl<'de> Visitor<'de> for VecVisitor {
        type Value = Vec<f64>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an array of floats")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<f64>, A::Error> {
            let mut out = Vec::with_capacity(a.size_hint().unwrap_or(0));
            while let Some(ExactF64(x)) = a.next_element()? {
                out.push(x);
            }
            Ok(out)
        }
    }

    /// Deserializes a vector of floats.
    ///
    /// # Errors
    ///
    /// Not an array of floats.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
        d.deserialize_seq(VecVisitor)
    }
}

/// `#[serde(with = "exact::vec2")]` for a `Vec<Vec<f64>>`.
pub mod vec2 {
    use super::{SeqAccess, SerializeSeq, Visitor, fmt};
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    #[derive(Serialize, Deserialize)]
    struct Row(#[serde(with = "super::vec")] Vec<f64>);

    /// Serializes a vector of float vectors.
    ///
    /// # Errors
    ///
    /// Propagates the serializer's error.
    pub fn serialize<S: Serializer>(v: &[Vec<f64>], s: S) -> Result<S::Ok, S::Error> {
        let mut seq = s.serialize_seq(Some(v.len()))?;
        for row in v {
            seq.serialize_element(&Row(row.clone()))?;
        }
        seq.end()
    }

    struct Vec2Visitor;

    impl<'de> Visitor<'de> for Vec2Visitor {
        type Value = Vec<Vec<f64>>;

        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("an array of arrays of floats")
        }

        fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Vec<Vec<f64>>, A::Error> {
            let mut out = Vec::with_capacity(a.size_hint().unwrap_or(0));
            while let Some(Row(r)) = a.next_element()? {
                out.push(r);
            }
            Ok(out)
        }
    }

    /// Deserializes a vector of float vectors.
    ///
    /// # Errors
    ///
    /// Not an array of arrays of floats.
    pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Vec<f64>>, D::Error> {
        d.deserialize_seq(Vec2Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_is_bit_exact() {
        let vals = [
            0.0,
            -0.0,
            1.0,
            -1.5,
            0.1 + 0.2,
            f64::MIN_POSITIVE,
            5e-324,
            f64::MAX,
            -f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            std::f64::consts::PI,
            1.0 / 3.0,
            123_456_789.123_456_78,
        ];
        for v in vals {
            let s = format_f64(v);
            let back = parse_f64(&s).unwrap();
            assert_eq!(back.to_bits(), v.to_bits(), "{v:?} -> {s} -> {back:?}");
        }
        assert!(parse_f64(&format_f64(f64::NAN)).unwrap().is_nan());
    }

    #[test]
    fn serde_round_trip_with_numbers_accepted() {
        #[derive(serde::Serialize, serde::Deserialize, PartialEq, Debug)]
        struct T {
            #[serde(with = "f64s")]
            a: f64,
            #[serde(with = "vec")]
            b: Vec<f64>,
            #[serde(with = "vec2")]
            c: Vec<Vec<f64>>,
        }
        let t = T {
            a: f64::NEG_INFINITY,
            b: vec![0.1, -0.0, f64::INFINITY],
            c: vec![vec![1.0 / 7.0], vec![]],
        };
        let s = serde_json::to_string(&t).unwrap();
        let back: T = serde_json::from_str(&s).unwrap();
        assert_eq!(back.a.to_bits(), t.a.to_bits());
        for (x, y) in back.b.iter().zip(&t.b) {
            assert_eq!(x.to_bits(), y.to_bits());
        }
        assert_eq!(back.c, t.c);
        let hand: T = serde_json::from_str(r#"{"a": 2, "b": [1.5, "inf"], "c": [[3]]}"#).unwrap();
        assert_eq!(hand.a, 2.0);
        assert_eq!(hand.b, vec![1.5, f64::INFINITY]);
        assert_eq!(hand.c, vec![vec![3.0]]);
    }
}
