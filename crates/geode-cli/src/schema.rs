//! `geode schema spec|report|layout` (issue #709): the CLI's JSON
//! contracts as published **JSON Schema (draft 2020-12)** documents.
//!
//! The schemas are derived with [`schemars`] from the same serde types the
//! binary parses ([`ProblemSpec`], [`Layout`]) and emits (the report
//! structs), so they cannot drift from what `geode` actually accepts /
//! writes. `deny_unknown_fields` becomes `"additionalProperties": false`,
//! `#[serde(default)]` fields are optional, and the internally tagged
//! enums (`solver.mode`, `boundary.kind`) become `oneOf` branches keyed by
//! a `const` tag.
//!
//! * **spec** / **layout** — the *deserialize* contract: what a caller may
//!   write (defaulted fields optional).
//! * **report** — the *serialize* contract: what `geode` writes. The Rust
//!   side has no single report type (each kind is its own struct with a
//!   literal `kind` string), so the root is a hand-assembled `oneOf` over
//!   the eight report kinds; every branch pins `kind` (and `status`) with
//!   a `const`, so exactly one branch matches any report.
//!
//! The generated documents are committed under `crates/geode-cli/schemas/`
//! and `tests/schema.rs` fails if they drift from [`generate`].
//!
//! **Schema-valid is not `geode check`-valid.** Cross-field rules live in
//! `problem::load` (spec) and [`Layout::resolve_for`] (layout), not in serde,
//! and are not expressed in the schemas: which sections an analysis
//! requires or forbids, at most one analysis section, `mu_r ≠ 1` only in
//! inductance specs, which analyses take `eps_r_diag` / `mu_r_diag`, a physical group in at most one role, value ranges
//! (`> 0`, `im ≤ 0`, …), and physical-group names that must exist in the
//! mesh. `geode check` (and, for layouts, `geode mesh`) stays the
//! authority.

use std::io::Write;
use std::path::{Path, PathBuf};

use clap::{Args, ValueEnum};
use schemars::JsonSchema;
use schemars::generate::{SchemaGenerator, SchemaSettings};
use serde_json::{Map, Value};

use crate::mesh_cmd::MeshReport;
use crate::mesh_cmd::layout::Layout;
use crate::report::{
    CapacitanceReport, CheckReport, DrivenReport, EigenReport, ErrorReport, ExtractReport,
    InductanceReport,
};
use crate::spec::ProblemSpec;

/// The JSON Schema dialect every generated document declares.
pub const DRAFT_2020_12: &str = "https://json-schema.org/draft/2020-12/schema";

/// Which contract `geode schema` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub enum SchemaKind {
    /// Problem spec (input of check / driven / eigen / extract /
    /// capacitance / inductance).
    Spec,
    /// JSON report (output of every subcommand, discriminated by `kind`).
    Report,
    /// Layout (input of `geode mesh`).
    Layout,
}

/// `geode schema` arguments.
#[derive(Args)]
pub struct SchemaArgs {
    /// Which schema: `spec`, `report` or `layout`.
    #[arg(value_enum)]
    kind: SchemaKind,
    /// Write the schema here instead of stdout.
    #[arg(short = 'o', long = "output", value_name = "PATH")]
    output: Option<PathBuf>,
}

/// Caveat appended to the spec and layout descriptions.
const NOT_A_VALIDATOR: &str = "Structural contract only: a document that validates against this \
     schema can still be rejected at run time. Cross-field rules are enforced by the binary, not \
     the schema — which sections each analysis requires or forbids, at most one analysis section, \
     value ranges (> 0, im <= 0, ...), names that must resolve against the mesh / layout, a \
     physical group in at most one role. `geode check` (spec) and `geode mesh` (layout) are the \
     authority.";

/// The JSON Schema for `kind`, as a JSON value (the document `geode
/// schema <kind>` prints).
pub fn generate(kind: SchemaKind) -> Value {
    match kind {
        SchemaKind::Spec => root::<ProblemSpec>(
            "GEODE problem spec (schema v1)",
            &format!(
                "Input of `geode check | driven | eigen | extract | capacitance | inductance`. \
                 One analysis per spec, chosen by which of the optional `eigen` / `extract` / \
                 `capacitance` / `inductance` sections is present (none = driven). JSON or TOML \
                 (same schema). {NOT_A_VALIDATOR}"
            ),
        ),
        SchemaKind::Layout => root::<Layout>(
            "GEODE layout (schema v1)",
            &format!(
                "Input of `geode mesh`: a planar (2.5-D) layer stack with rectilinear conductor \
                 polygons, lumped gap ports and inductance contacts. JSON or TOML (same schema). \
                 {NOT_A_VALIDATOR}"
            ),
        ),
        SchemaKind::Report => report(),
    }
}

/// Deserialize-contract root schema of `T` with a fixed title / description.
fn root<T: JsonSchema>(title: &str, description: &str) -> Value {
    let schema = SchemaSettings::draft2020_12()
        .for_deserialize()
        .into_generator()
        .into_root_schema_for::<T>();
    let mut v = schema.to_value();
    let obj = v.as_object_mut().expect("root schema is an object");
    obj.insert("$schema".into(), DRAFT_2020_12.into());
    obj.insert("title".into(), title.into());
    obj.insert("description".into(), description.into());
    v
}

/// The report schema: `oneOf` over every report kind (serialize contract).
fn report() -> Value {
    let mut generator: SchemaGenerator = SchemaSettings::draft2020_12()
        .for_serialize()
        .into_generator();
    let branches = vec![
        generator.subschema_for::<CheckReport>().to_value(),
        generator.subschema_for::<DrivenReport>().to_value(),
        generator.subschema_for::<EigenReport>().to_value(),
        generator.subschema_for::<ExtractReport>().to_value(),
        generator.subschema_for::<CapacitanceReport>().to_value(),
        generator.subschema_for::<InductanceReport>().to_value(),
        generator.subschema_for::<MeshReport>().to_value(),
        generator.subschema_for::<ErrorReport>().to_value(),
    ];
    let mut obj = Map::new();
    obj.insert("$schema".into(), DRAFT_2020_12.into());
    obj.insert("title".into(), "GEODE report (schema v1)".into());
    obj.insert(
        "description".into(),
        "Output of every `geode` subcommand except `schema`: exactly one JSON document, one of \
         eight kinds discriminated by the top-level `kind` (`check`, `driven`, `eigen`, \
         `extract`, `capacitance`, `inductance`, `mesh`, `error`). Each branch pins `kind` and \
         `status` with `const`, so exactly one branch matches. Report objects are open \
         (no `additionalProperties: false`): fields added within report schema v1 are additive, \
         and consumers should ignore fields they do not know."
            .into(),
    );
    obj.insert("oneOf".into(), Value::Array(branches));
    obj.insert(
        "$defs".into(),
        Value::Object(generator.take_definitions(false)),
    );
    Value::Object(obj)
}

/// `geode schema <kind>`: print the schema (pretty JSON + newline) to
/// stdout or `-o`. Unlike the other subcommands this writes a schema, not
/// a report, and a failure (I/O only) produces no error report.
pub fn run(a: SchemaArgs) -> Result<(), Box<dyn std::error::Error>> {
    let mut json = serde_json::to_string_pretty(&generate(a.kind))?;
    json.push('\n');
    match a.output.as_deref() {
        Some(path) => write_file(path, &json),
        None => {
            let mut out = std::io::stdout().lock();
            out.write_all(json.as_bytes())?;
            out.flush()?;
            Ok(())
        }
    }
}

fn write_file(path: &Path, json: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::write(path, json).map_err(|err| {
        Box::new(crate::error::CliError::Io {
            path: path.to_path_buf(),
            err,
        }) as Box<dyn std::error::Error>
    })
}
