//! XML layer-stackup reader (issue #1288, Phase 1b of #1251).
//!
//! Reads a flat, attribute-only XML stackup into dielectric layers
//! (z, thickness, ε) and conductor layers (z, thickness, σ) in the
//! shapes [`super::layout`] uses. Written from scratch against the
//! *shape* of the publicly documented gds2palace / openEMS stackup
//! format (named materials, a z-ordered layer list); no gds2palace code
//! or stackup file is copied, linked or vendored (those are GPL-3.0). The
//! element and attribute names below are this crate's own contract and are
//! pinned by `tests/fixtures/stackup_example.xml`, which is
//! self-authored. Real third-party stackups may need renaming to fit.
//!
//! ```xml
//! <Stackup Unit="1e-6">            <!-- metres per z unit; default 1e-6 -->
//!   <Materials>
//!     <Material Name="SiO2" Type="Dielectric" Permittivity="4.1"/>
//!     <Material Name="Cu"   Type="Conductor"  Conductivity="5.8e7"/>
//!   </Materials>
//!   <Layers>
//!     <Layer Name="ild" Material="SiO2" Zmin="0" Zmax="8"/>
//!     <Layer Name="m1"  Material="Cu"   Zmin="2" Zmax="3"/>
//!   </Layers>
//! </Stackup>
//! ```
//!
//! A `Dielectric` layer must have `Zmax > Zmin`; a `Conductor` layer
//! may have `Zmax == Zmin` (a zero-thickness sheet). A conductor material
//! without `Conductivity` is a perfect conductor (`sigma_s_m: None`).
//! Layers are returned sorted by `z_bottom`, whatever the file order.

use std::collections::HashMap;

/// Metres per z unit when `<Stackup>` has no `Unit` attribute (microns).
pub const DEFAULT_UNIT_M: f64 = 1e-6;

/// A dielectric slab.
#[derive(Debug, Clone, PartialEq)]
pub struct StackupDielectric {
    /// Layer name.
    pub name: String,
    /// Bottom of the slab, in stackup units.
    pub z_bottom: f64,
    /// Thickness (`> 0`).
    pub thickness: f64,
    /// Relative permittivity.
    pub eps_r: f64,
}

/// A conductor layer.
#[derive(Debug, Clone, PartialEq)]
pub struct StackupConductor {
    /// Layer name.
    pub name: String,
    /// Bottom of the layer, in stackup units.
    pub z_bottom: f64,
    /// Thickness (`≥ 0`; `0` = a sheet).
    pub thickness: f64,
    /// Conductivity in S/m; `None` = perfect conductor.
    pub sigma_s_m: Option<f64>,
}

/// A parsed stackup.
#[derive(Debug, Clone, PartialEq)]
pub struct Stackup {
    /// Metres per z unit.
    pub unit_m: f64,
    /// Dielectric layers, ascending `z_bottom`.
    pub dielectrics: Vec<StackupDielectric>,
    /// Conductor layers, ascending `z_bottom`.
    pub conductors: Vec<StackupConductor>,
}

/// Why a stackup failed to parse. One variant per failure class.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum StackupError {
    /// The text is not well-formed (unterminated tag / comment, bad
    /// attribute syntax, mismatched or missing `<Stackup>` root).
    #[error("malformed stackup XML: {0}")]
    MalformedXml(String),
    /// A required attribute is absent.
    #[error("<{element}> is missing required attribute `{attribute}`")]
    MissingAttribute {
        /// Element name.
        element: String,
        /// Attribute name.
        attribute: &'static str,
    },
    /// A numeric attribute does not parse as a finite number.
    #[error("<{element}> attribute `{attribute}` is not a finite number: {value:?}")]
    InvalidNumber {
        /// Element name.
        element: String,
        /// Attribute name.
        attribute: &'static str,
        /// The offending text.
        value: String,
    },
    /// A layer's thickness (`Zmax - Zmin`) is not allowed: `≤ 0` for a
    /// dielectric, `< 0` for a conductor.
    #[error("layer {layer:?} has invalid thickness {thickness} (Zmax - Zmin)")]
    NonPositiveThickness {
        /// Layer name.
        layer: String,
        /// `Zmax - Zmin`.
        thickness: f64,
    },
    /// A layer names a material that is not in `<Materials>`.
    #[error("layer {layer:?} references unknown material {material:?}")]
    UnknownMaterial {
        /// Layer name.
        layer: String,
        /// Material name.
        material: String,
    },
    /// A material `Type` is neither `Dielectric` nor `Conductor`.
    #[error("material {material:?} has unknown Type {kind:?}")]
    UnknownMaterialType {
        /// Material name.
        material: String,
        /// The offending `Type`.
        kind: String,
    },
}

#[derive(Debug)]
struct Tag {
    name: String,
    attrs: HashMap<String, String>,
    /// `</name>` rather than an opening / self-closing tag.
    closing: bool,
    /// `<name … />`.
    self_closing: bool,
}

fn malformed<T>(msg: impl Into<String>) -> Result<T, StackupError> {
    Err(StackupError::MalformedXml(msg.into()))
}

/// Tokenise into tags, skipping the XML declaration, comments and text.
fn scan(xml: &str) -> Result<Vec<Tag>, StackupError> {
    let mut tags = Vec::new();
    let mut rest = xml;
    while let Some(i) = rest.find('<') {
        rest = &rest[i..];
        if let Some(after) = rest.strip_prefix("<!--") {
            let Some(end) = after.find("-->") else {
                return malformed("unterminated comment");
            };
            rest = &after[end + 3..];
            continue;
        }
        if let Some(after) = rest.strip_prefix("<?") {
            let Some(end) = after.find("?>") else {
                return malformed("unterminated processing instruction");
            };
            rest = &after[end + 2..];
            continue;
        }
        let Some(end) = rest.find('>') else {
            return malformed("unterminated tag");
        };
        let mut body = &rest[1..end];
        rest = &rest[end + 1..];
        let closing = body.starts_with('/');
        if closing {
            body = &body[1..];
        }
        let self_closing = body.ends_with('/');
        body = body.strip_suffix('/').unwrap_or(body);
        tags.push(parse_tag(body.trim(), closing, self_closing)?);
    }
    Ok(tags)
}

fn parse_tag(body: &str, closing: bool, self_closing: bool) -> Result<Tag, StackupError> {
    let name_end = body.find(|c: char| c.is_whitespace()).unwrap_or(body.len());
    let name = &body[..name_end];
    if name.is_empty() || !name.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return malformed(format!("bad element name {name:?}"));
    }
    let mut attrs = HashMap::new();
    let mut s = body[name_end..].trim_start();
    while !s.is_empty() {
        let Some(eq) = s.find('=') else {
            return malformed(format!("attribute without `=` in <{name}>"));
        };
        let key = s[..eq].trim();
        let after = s[eq + 1..].trim_start();
        let Some(quote) = after.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            return malformed(format!("unquoted value for `{key}` in <{name}>"));
        };
        let Some(close) = after[1..].find(quote) else {
            return malformed(format!("unterminated value for `{key}` in <{name}>"));
        };
        if key.is_empty()
            || attrs
                .insert(key.to_owned(), after[1..1 + close].to_owned())
                .is_some()
        {
            return malformed(format!("empty or duplicate attribute {key:?} in <{name}>"));
        }
        s = after[close + 2..].trim_start();
    }
    Ok(Tag {
        name: name.to_owned(),
        attrs,
        closing,
        self_closing,
    })
}

fn required<'a>(tag: &'a Tag, attribute: &'static str) -> Result<&'a str, StackupError> {
    tag.attrs
        .get(attribute)
        .map(String::as_str)
        .ok_or_else(|| StackupError::MissingAttribute {
            element: tag.name.clone(),
            attribute,
        })
}

fn number(tag: &Tag, attribute: &'static str, value: &str) -> Result<f64, StackupError> {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite())
        .ok_or_else(|| StackupError::InvalidNumber {
            element: tag.name.clone(),
            attribute,
            value: value.to_owned(),
        })
}

fn required_number(tag: &Tag, attribute: &'static str) -> Result<f64, StackupError> {
    number(tag, attribute, required(tag, attribute)?)
}

enum Material {
    Dielectric(f64),
    Conductor(Option<f64>),
}

/// Parse a stackup XML document.
pub fn parse(xml: &str) -> Result<Stackup, StackupError> {
    let tags = scan(xml)?;
    // Well-formedness: balanced nesting, `Stackup` root.
    let mut stack: Vec<&str> = Vec::new();
    for t in &tags {
        if t.closing {
            if stack.pop() != Some(t.name.as_str()) {
                return malformed(format!("unexpected closing </{}>", t.name));
            }
        } else if !t.self_closing {
            stack.push(&t.name);
        }
    }
    if !stack.is_empty() {
        return malformed(format!("unclosed <{}>", stack[stack.len() - 1]));
    }
    let root = tags.iter().find(|t| !t.closing);
    let Some(root) = root.filter(|t| t.name == "Stackup") else {
        return malformed("root element must be <Stackup>");
    };
    let unit_m = match root.attrs.get("Unit") {
        Some(v) => number(root, "Unit", v)?,
        None => DEFAULT_UNIT_M,
    };
    if unit_m <= 0.0 {
        return Err(StackupError::InvalidNumber {
            element: "Stackup".into(),
            attribute: "Unit",
            value: unit_m.to_string(),
        });
    }

    let mut materials: HashMap<String, Material> = HashMap::new();
    for t in tags.iter().filter(|t| !t.closing && t.name == "Material") {
        let name = required(t, "Name")?.to_owned();
        let kind = required(t, "Type")?;
        let m = match kind {
            "Dielectric" => Material::Dielectric(required_number(t, "Permittivity")?),
            "Conductor" => Material::Conductor(match t.attrs.get("Conductivity") {
                Some(v) => Some(number(t, "Conductivity", v)?),
                None => None,
            }),
            other => {
                return Err(StackupError::UnknownMaterialType {
                    material: name,
                    kind: other.to_owned(),
                });
            }
        };
        materials.insert(name, m);
    }

    let mut dielectrics = Vec::new();
    let mut conductors = Vec::new();
    for t in tags.iter().filter(|t| !t.closing && t.name == "Layer") {
        let name = required(t, "Name")?.to_owned();
        let material = required(t, "Material")?;
        let z0 = required_number(t, "Zmin")?;
        let z1 = required_number(t, "Zmax")?;
        let thickness = z1 - z0;
        let Some(m) = materials.get(material) else {
            return Err(StackupError::UnknownMaterial {
                layer: name,
                material: material.to_owned(),
            });
        };
        match *m {
            Material::Dielectric(eps_r) => {
                if thickness <= 0.0 {
                    return Err(StackupError::NonPositiveThickness {
                        layer: name,
                        thickness,
                    });
                }
                dielectrics.push(StackupDielectric {
                    name,
                    z_bottom: z0,
                    thickness,
                    eps_r,
                });
            }
            Material::Conductor(sigma_s_m) => {
                if thickness < 0.0 {
                    return Err(StackupError::NonPositiveThickness {
                        layer: name,
                        thickness,
                    });
                }
                conductors.push(StackupConductor {
                    name,
                    z_bottom: z0,
                    thickness,
                    sigma_s_m,
                });
            }
        }
    }
    dielectrics.sort_by(|a, b| a.z_bottom.total_cmp(&b.z_bottom));
    conductors.sort_by(|a, b| a.z_bottom.total_cmp(&b.z_bottom));
    Ok(Stackup {
        unit_m,
        dielectrics,
        conductors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = include_str!("../../tests/fixtures/stackup_example.xml");

    #[test]
    fn parses_the_self_authored_fixture() {
        let s = parse(EXAMPLE).unwrap();
        assert_eq!(s.unit_m, 1e-6);
        // File lists the layers out of z-order; output is sorted.
        let d: Vec<_> = s.dielectrics.iter().map(|d| d.name.as_str()).collect();
        assert_eq!(d, ["substrate", "oxide", "passivation"]);
        assert_eq!(
            s.dielectrics[1],
            StackupDielectric {
                name: "oxide".into(),
                z_bottom: 0.0,
                thickness: 8.0,
                eps_r: 4.1
            }
        );
        assert_eq!(s.dielectrics[0].z_bottom, -20.0);
        assert_eq!(s.dielectrics[0].eps_r, 11.9);
        let c: Vec<_> = s.conductors.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(c, ["m1", "via_sheet", "top_pec"]);
        assert_eq!(s.conductors[0].thickness, 1.0);
        assert_eq!(s.conductors[0].sigma_s_m, Some(5.8e7));
        // Zero-thickness sheet entry is accepted.
        assert_eq!(s.conductors[1].thickness, 0.0);
        // Conductor material without Conductivity: perfect conductor.
        assert_eq!(s.conductors[2].sigma_s_m, None);
    }

    #[test]
    fn unit_attribute_is_honoured() {
        let x = r#"<Stackup Unit="1e-3"><Materials/><Layers/></Stackup>"#;
        assert_eq!(parse(x).unwrap().unit_m, 1e-3);
    }

    #[test]
    fn malformed_xml_variants() {
        for bad in [
            "<Stackup><Materials>",
            "<Stackup",
            "<!-- never closed <Stackup/>",
            "<Stackup></Materials>",
            "<Other/>",
            "",
            r#"<Stackup Unit=1e-6></Stackup>"#,
        ] {
            assert!(
                matches!(parse(bad), Err(StackupError::MalformedXml(_))),
                "{bad:?} -> {:?}",
                parse(bad)
            );
        }
    }

    fn with_layer(layer: &str) -> String {
        format!(
            r#"<Stackup><Materials>
            <Material Name="ox" Type="Dielectric" Permittivity="4"/>
            <Material Name="cu" Type="Conductor" Conductivity="1e7"/>
            </Materials><Layers>{layer}</Layers></Stackup>"#
        )
    }

    #[test]
    fn missing_attribute() {
        let e = parse(&with_layer(r#"<Layer Name="a" Material="ox" Zmin="0"/>"#)).unwrap_err();
        assert_eq!(
            e,
            StackupError::MissingAttribute {
                element: "Layer".into(),
                attribute: "Zmax"
            }
        );
    }

    #[test]
    fn non_numeric_value() {
        let e = parse(&with_layer(
            r#"<Layer Name="a" Material="ox" Zmin="abc" Zmax="1"/>"#,
        ))
        .unwrap_err();
        assert!(matches!(
            e,
            StackupError::InvalidNumber {
                attribute: "Zmin",
                ..
            }
        ));
        let nan = parse(&with_layer(
            r#"<Layer Name="a" Material="ox" Zmin="0" Zmax="NaN"/>"#,
        ));
        assert!(matches!(nan, Err(StackupError::InvalidNumber { .. })));
    }

    #[test]
    fn non_positive_thickness() {
        for l in [
            r#"<Layer Name="a" Material="ox" Zmin="1" Zmax="1"/>"#,
            r#"<Layer Name="a" Material="ox" Zmin="2" Zmax="1"/>"#,
            r#"<Layer Name="a" Material="cu" Zmin="2" Zmax="1"/>"#,
        ] {
            assert!(
                matches!(
                    parse(&with_layer(l)),
                    Err(StackupError::NonPositiveThickness { .. })
                ),
                "{l}"
            );
        }
        // Zero-thickness conductor sheet is fine.
        assert!(
            parse(&with_layer(
                r#"<Layer Name="a" Material="cu" Zmin="1" Zmax="1"/>"#
            ))
            .is_ok()
        );
    }

    #[test]
    fn unknown_material_and_type() {
        let e = parse(&with_layer(
            r#"<Layer Name="a" Material="zz" Zmin="0" Zmax="1"/>"#,
        ));
        assert!(matches!(e, Err(StackupError::UnknownMaterial { .. })));
        let x = r#"<Stackup><Materials><Material Name="m" Type="Magnet"/></Materials></Stackup>"#;
        assert!(matches!(
            parse(x),
            Err(StackupError::UnknownMaterialType { .. })
        ));
    }
}
