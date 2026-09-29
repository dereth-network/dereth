//! Loader for the golden vector files in `empyrean/fixtures/vectors/<area>/<name>.json`.
//!
//! The files are produced by the ACE vector harness (the server's `ace-vectors`), which
//! runs ACE's own compiled code and the .NET runtime ACE targets. Nothing here is ported from ACE.
//!
//! Format (see the server's `ace-vectors/README.md`):
//!
//! ```text
//! { "cases": [ {"in": ..., "out": ...}, ... ],
//!   "generator": "ace-vectors <area>",
//!   "source": { "ace_commit", "culture", "file", "member", "notes"?, "runtime", "sdk" } }
//! ```
//!
//! Doubles are JSON numbers with a `.` or an exponent (negative zero is `-0.0`); floats are
//! written as their exact `double` value; NaN and the infinities are the strings `"NaN"`,
//! `"Infinity"` and `"-Infinity"`. A call that threw is `{"throws": "<exception type>"}`.
//! `serde_json`'s `float_roundtrip` feature is enabled so every double parses exactly.

use std::fmt;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

/// The ACE commit every vector file must name (the commit this port follows).
pub const ACE_COMMIT: &str = "47edade3bd3f6044b676d4eb877c4965c7eda62b";

/// The harness command prefix every file's `generator` starts with.
pub const GENERATOR_PREFIX: &str = "ace-vectors ";

/// A schema or I/O problem with a vector file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VectorError {
    /// The file concerned.
    pub path: PathBuf,
    /// What is wrong.
    pub message: String,
}

impl fmt::Display for VectorError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.path.display(), self.message)
    }
}

impl std::error::Error for VectorError {}

/// The `source` object: where the values came from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    /// The ACE commit the harness was built against.
    pub ace_commit: String,
    /// The culture numbers were formatted in (`en-US`, as ACE sets it).
    pub culture: String,
    /// The ACE source file (starting at `Source/`), or `None` for .NET runtime behaviour.
    pub file: Option<String>,
    /// The C# member evaluated.
    pub member: String,
    /// Free-form notes on the inputs and outputs.
    pub notes: Option<String>,
    /// The .NET runtime version that produced the file.
    pub runtime: String,
    /// The .NET SDK version that built the harness.
    pub sdk: String,
}

/// One input/output pair.
#[derive(Debug, Clone, PartialEq)]
pub struct Case {
    /// The `in` value (always an object).
    pub input: Value,
    /// The `out` value.
    pub output: Value,
}

/// A parsed vector file.
#[derive(Debug, Clone, PartialEq)]
pub struct VectorFile {
    /// Where it was read from.
    pub path: PathBuf,
    /// Provenance.
    pub source: Source,
    /// The command that regenerates it.
    pub generator: String,
    /// The cases, in file order.
    pub cases: Vec<Case>,
}

/// `empyrean/fixtures/vectors`, located from this crate's manifest directory.
#[must_use]
pub fn vectors_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/vectors")
}

/// Every `*.json` file under [`vectors_dir`], sorted by path.
///
/// # Errors
/// When the directory cannot be read.
pub fn all_files() -> Result<Vec<PathBuf>, VectorError> {
    let root = vectors_dir();
    let mut out = Vec::new();
    let mut stack = vec![root.clone()];
    while let Some(dir) = stack.pop() {
        let entries = std::fs::read_dir(&dir).map_err(|e| err(&dir, format!("read_dir: {e}")))?;
        for entry in entries {
            let path = entry
                .map_err(|e| err(&dir, format!("read_dir entry: {e}")))?
                .path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().is_some_and(|x| x == "json") {
                out.push(path);
            }
        }
    }
    out.sort();
    Ok(out)
}

/// Loads `<area>/<name>.json` from [`vectors_dir`].
///
/// # Panics
/// When the file is missing or malformed; this is meant for tests.
#[must_use]
pub fn load_named(area: &str, name: &str) -> VectorFile {
    let path = vectors_dir().join(area).join(format!("{name}.json"));
    load(&path).unwrap_or_else(|e| panic!("{e}"))
}

/// Reads and validates one vector file.
///
/// # Errors
/// On I/O failure, invalid JSON, or a schema violation.
pub fn load(path: &Path) -> Result<VectorFile, VectorError> {
    let text = std::fs::read_to_string(path).map_err(|e| err(path, format!("read: {e}")))?;
    parse(path, &text)
}

/// Parses and validates vector-file text.
///
/// # Errors
/// On invalid JSON or a schema violation.
pub fn parse(path: &Path, text: &str) -> Result<VectorFile, VectorError> {
    let root: Value = serde_json::from_str(text).map_err(|e| err(path, format!("json: {e}")))?;
    let top = root
        .as_object()
        .ok_or_else(|| err(path, "top level is not an object"))?;
    exact_keys(
        path,
        "top level",
        top,
        &["cases", "generator", "source"],
        &[],
    )?;

    let generator = top["generator"]
        .as_str()
        .ok_or_else(|| err(path, "generator is not a string"))?;
    if !generator.starts_with(GENERATOR_PREFIX) {
        return Err(err(
            path,
            format!("generator {generator:?} does not start with {GENERATOR_PREFIX:?}"),
        ));
    }

    let src = top["source"]
        .as_object()
        .ok_or_else(|| err(path, "source is not an object"))?;
    exact_keys(
        path,
        "source",
        src,
        &["ace_commit", "culture", "file", "member", "runtime", "sdk"],
        &["notes"],
    )?;
    let string = |key: &str| -> Result<String, VectorError> {
        src[key]
            .as_str()
            .map(str::to_owned)
            .ok_or_else(|| err(path, format!("source.{key} is not a string")))
    };
    let optional = |key: &str| -> Result<Option<String>, VectorError> {
        match src.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(s)) => Ok(Some(s.clone())),
            Some(_) => Err(err(
                path,
                format!("source.{key} is neither a string nor null"),
            )),
        }
    };
    let source = Source {
        ace_commit: string("ace_commit")?,
        culture: string("culture")?,
        file: optional("file")?,
        member: string("member")?,
        notes: optional("notes")?,
        runtime: string("runtime")?,
        sdk: string("sdk")?,
    };
    if let Some(file) = &source.file {
        if !file.starts_with("Source/") {
            return Err(err(
                path,
                format!("source.file {file:?} must start with Source/"),
            ));
        }
    }

    let raw_cases = top["cases"]
        .as_array()
        .ok_or_else(|| err(path, "cases is not an array"))?;
    let mut cases = Vec::with_capacity(raw_cases.len());
    for (i, case) in raw_cases.iter().enumerate() {
        let obj = case
            .as_object()
            .ok_or_else(|| err(path, format!("case {i} is not an object")))?;
        exact_keys(path, &format!("case {i}"), obj, &["in", "out"], &[])?;
        if !obj["in"].is_object() {
            return Err(err(path, format!("case {i}: `in` is not an object")));
        }
        cases.push(Case {
            input: obj["in"].clone(),
            output: obj["out"].clone(),
        });
    }

    Ok(VectorFile {
        path: path.to_owned(),
        source,
        generator: generator.to_owned(),
        cases,
    })
}

fn err(path: &Path, message: impl Into<String>) -> VectorError {
    VectorError {
        path: path.to_owned(),
        message: message.into(),
    }
}

fn exact_keys(
    path: &Path,
    what: &str,
    obj: &Map<String, Value>,
    required: &[&str],
    optional: &[&str],
) -> Result<(), VectorError> {
    for key in required {
        if !obj.contains_key(*key) {
            return Err(err(path, format!("{what}: missing key {key:?}")));
        }
    }
    for key in obj.keys() {
        if !required.contains(&key.as_str()) && !optional.contains(&key.as_str()) {
            return Err(err(path, format!("{what}: unexpected key {key:?}")));
        }
    }
    Ok(())
}

/// A double: a JSON number, or one of the tokens `"NaN"`, `"Infinity"`, `"-Infinity"`.
#[must_use]
pub fn f64_of(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => match s.as_str() {
            "NaN" => Some(f64::NAN),
            "Infinity" => Some(f64::INFINITY),
            "-Infinity" => Some(f64::NEG_INFINITY),
            _ => None,
        },
        _ => None,
    }
}

/// A float, written as its exact double value.
#[must_use]
#[allow(clippy::cast_possible_truncation)]
pub fn f32_of(v: &Value) -> Option<f32> {
    // Exact: the file holds a double that is exactly representable as a float.
    f64_of(v).map(|d| d as f32)
}

/// A signed integer that fits `i64`.
#[must_use]
pub fn i64_of(v: &Value) -> Option<i64> {
    v.as_i64()
}

/// An unsigned integer that fits `u64`.
#[must_use]
pub fn u64_of(v: &Value) -> Option<u64> {
    v.as_u64()
}

/// The exception type of a `{"throws": "<type>"}` value, or `None` for a normal result.
#[must_use]
pub fn throws(v: &Value) -> Option<&str> {
    v.as_object()
        .filter(|o| o.len() == 1)
        .and_then(|o| o.get("throws"))
        .and_then(Value::as_str)
}

/// Bit-exact double equality that treats every NaN as equal and distinguishes `-0.0` from `0.0`.
#[must_use]
pub fn same_f64(a: f64, b: f64) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

/// [`same_f64`] for floats.
#[must_use]
pub fn same_f32(a: f32, b: f32) -> bool {
    (a.is_nan() && b.is_nan()) || a.to_bits() == b.to_bits()
}

/// Empyrean's brand rule for ACE's recorded text: where ACE's text names ACE's software, its commands, its sites or its tools, ours
/// names Empyrean's. The vectors stay the record of what ACE printed; a test that compares our
/// text with ACE's passes the vector through [`brand_ruled`] first.
///
/// Each entry is an exact substitution, applied in order. Each must occur in at least one vector
/// (checked by empyrean-common's `vectors::brand_rules_each_hit_a_vector`), so a re-recorded ACE whose
/// wording changed fails loudly instead of silently no longer being transformed.
pub const BRAND_RULES: &[(&str, &str)] = &[
    // The command listing's header names the help command.
    ("type acehelp < command >", "type emphelp < command >"),
    // The DAT warnings' site.
    ("https://emulator.ac/how-to-play", "https://dereth.network"),
    // `version_info_enabled`'s description names the version command.
    (
        "toggles the /aceversion player command",
        "toggles the /empversion player command",
    ),
    // `content_folder`'s description names ACE's assembly.
    (
        "defaults to Content folder found in same directory as ACE.Server.dll",
        "defaults to the Content folder in the server's working directory",
    ),
    // Exported Lifestoned JSON metadata (the comment before the bare author name).
    (
        "Weenie exported from ACEmulator world database using ACE.Adapter",
        "Weenie exported from Empyrean world database",
    ),
    ("\"ACE.Adapter\"", "\"Empyrean\""),
];

/// `text` with every [`BRAND_RULES`] substitution applied, in order.
#[must_use]
pub fn brand_ruled(text: &str) -> String {
    BRAND_RULES
        .iter()
        .fold(text.to_owned(), |text, (ace, ours)| text.replace(ace, ours))
}

/// [`brand_ruled`] over every string inside a JSON value (object keys untouched).
#[must_use]
pub fn brand_ruled_value(value: &Value) -> Value {
    match value {
        Value::String(s) => Value::String(brand_ruled(s)),
        Value::Array(items) => Value::Array(items.iter().map(brand_ruled_value).collect()),
        Value::Object(map) => Value::Object(
            map.iter()
                .map(|(k, v)| (k.clone(), brand_ruled_value(v)))
                .collect::<Map<String, Value>>(),
        ),
        other => other.clone(),
    }
}
