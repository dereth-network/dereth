//! `cargo xtask shaders`: the client's shaders, compiled ahead of time and checked in.
//!
//! ```text
//! cargo xtask shaders [--dxil] [--msl] [--tools <dir>]
//! cargo xtask shaders --check
//! ```
//!
//! A crate with shaders keeps them in a `shaders/` folder of its own (`<crate>/shaders/`, or
//! `<crate>/tests/shaders/` for a test binary's): Slang sources, and `shaders.toml`, which names
//! the targets and each entry point (its source file, its function and its stage). The task
//! finds every such folder under `dereth/client/crates/`, compiles each entry point for each target
//! with the pinned `slangc`, and writes the folder's `compiled/`:
//!
//! - one blob per entry point and target, `<target>/<source>.<entry>.<ext>`;
//! - `manifest.toml`: the compiler, its options, and the SHA-256 of every source and every blob;
//! - `shaders.rs`: the index the crate `include!`s, an `Entry` enum whose blobs are embedded with
//!   `include_bytes!`. The including module defines the `Stage` the index names.
//!
//! All of it is checked in, so a plain `cargo build` needs no shader tool. A file is rewritten only
//! when its bytes change, so an edit to one entry point's source touches only the blobs that
//! source makes (and the manifest's lines for them).
//!
//! - **The compiler** is the pinned Slang release ([`fetch::SLANG_VERSION`]), fetched once into
//!   the per-user cache folder ([`fetch::cache_dir`]) and checked against its SHA-256; a download
//!   that does not match is refused and nothing is cached. `--tools <dir>` caches the tools
//!   somewhere else.
//! - **`--check`** compiles nothing and needs no compiler: it rebuilds the manifest from the sources
//!   and blobs on disk and the index from `shaders.toml`, compares them with the files, and fails
//!   on any difference, naming it (a source edited since its blobs were made, a blob that is not
//!   the one recorded, one missing or left over, a compiler or option that changed).
//! - **`--dxil` and `--msl`** also compile every entry point for D3D12's DXIL (with the pinned DXC,
//!   on Windows) and for Metal's shading language, into `<cargo target dir>/shaders/`, outside the
//!   tree. A crate checks a target in by naming it in its `shaders.toml`.

mod fetch;
#[cfg(test)]
mod tests;

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::package::sha256_hex;
use crate::util::{target_dir, workspace_root};

/// Where shader folders are looked for, relative to the workspace root.
const SEARCH: &str = "dereth/client/crates";

/// The file that makes a folder a shader folder.
const SPEC: &str = "shaders.toml";

/// The generated folder inside a shader folder, and its two generated files.
const COMPILED: &str = "compiled";
const MANIFEST: &str = "manifest.toml";
const INDEX: &str = "shaders.rs";

/// The options every compilation takes: matrices are column-major in memory (the CPU's `glam`
/// layout, read with `mul(m, v)`), and every warning is an error except the note that an entry
/// point's capabilities widen the target profile, which they are allowed to.
const COMMON_OPTIONS: &[&str] = &[
    "-matrix-layout-column-major",
    "-warnings-as-errors",
    "all",
    "-warnings-disable",
    "41012",
];

/// A compilation target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Target {
    /// SPIR-V 1.6 for Vulkan 1.3, entry points keeping their source names.
    Spirv,
    /// DXIL for D3D12, shader model 6.6.
    Dxil,
    /// Metal's shading language, as text.
    Msl,
}

impl Target {
    const ALL: [Self; 3] = [Self::Spirv, Self::Dxil, Self::Msl];

    /// Its name in `shaders.toml`, and its folder in `compiled/`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Spirv => "spirv",
            Self::Dxil => "dxil",
            Self::Msl => "msl",
        }
    }

    /// How documentation names it.
    fn label(self) -> &'static str {
        match self {
            Self::Spirv => "SPIR-V",
            Self::Dxil => "DXIL",
            Self::Msl => "Metal shading language",
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Spirv => "spv",
            Self::Dxil => "dxil",
            Self::Msl => "metal",
        }
    }

    /// Whether the blob is text, which is kept with LF line endings.
    fn is_text(self) -> bool {
        self == Self::Msl
    }

    fn options(self) -> &'static [&'static str] {
        match self {
            Self::Spirv => &[
                "-target",
                "spirv",
                "-profile",
                "spirv_1_6",
                "-fvk-use-entrypoint-name",
            ],
            Self::Dxil => &["-target", "dxil", "-profile", "sm_6_6", "-D", "DERETH_DXIL"],
            Self::Msl => &["-target", "metal"],
        }
    }

    /// The index's accessor for this target's blob, and the macro that embeds it.
    fn accessor(self) -> (&'static str, &'static str, &'static str) {
        match self {
            Self::Spirv => ("spirv_bytes", "&'static [u8]", "include_bytes!"),
            Self::Dxil => ("dxil_bytes", "&'static [u8]", "include_bytes!"),
            Self::Msl => ("msl_source", "&'static str", "include_str!"),
        }
    }

    fn parse(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|t| t.name() == name)
    }
}

/// A shader stage, as `shaders.toml` and `slangc` spell it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Vertex,
    Fragment,
    Compute,
    Mesh,
    Task,
}

impl Stage {
    const ALL: [Self; 5] = [
        Self::Vertex,
        Self::Fragment,
        Self::Compute,
        Self::Mesh,
        Self::Task,
    ];

    fn name(self) -> &'static str {
        match self {
            Self::Vertex => "vertex",
            Self::Fragment => "fragment",
            Self::Compute => "compute",
            Self::Mesh => "mesh",
            Self::Task => "task",
        }
    }

    /// `slangc -stage`'s spelling.
    fn slang(self) -> &'static str {
        match self {
            Self::Task => "amplification",
            other => other.name(),
        }
    }

    /// The `Stage` variant the index names.
    fn variant(self) -> &'static str {
        match self {
            Self::Vertex => "Vertex",
            Self::Fragment => "Fragment",
            Self::Compute => "Compute",
            Self::Mesh => "Mesh",
            Self::Task => "Task",
        }
    }
}

/// One entry point `shaders.toml` names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySpec {
    /// The source file, a plain file name in the shader folder.
    pub source: String,
    /// The function, which is also the entry point's name in the blob.
    pub entry: String,
    pub stage: Stage,
}

impl EntrySpec {
    /// Its blob's path inside `compiled/`, with forward slashes.
    #[must_use]
    pub fn blob(&self, target: Target) -> String {
        let stem = self.source.strip_suffix(".slang").unwrap_or(&self.source);
        format!(
            "{}/{stem}.{}.{}",
            target.name(),
            self.entry,
            target.extension()
        )
    }

    /// Its variant in the index's `Entry` enum: the source's stem and the function, in camel case.
    #[must_use]
    pub fn variant(&self) -> String {
        let stem = self.source.strip_suffix(".slang").unwrap_or(&self.source);
        camel(&format!("{stem}_{}", self.entry))
    }
}

/// `snake_case` or `kebab-case` words as `CamelCase`.
fn camel(s: &str) -> String {
    s.split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut c = w.chars();
            c.next().map_or_else(String::new, |f| {
                f.to_ascii_uppercase().to_string() + &c.as_str().to_ascii_lowercase()
            })
        })
        .collect()
}

/// A shader folder's `shaders.toml`, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    /// The targets checked in, in `Target`'s order.
    pub targets: Vec<Target>,
    pub entries: Vec<EntrySpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpecFile {
    targets: Vec<String>,
    #[serde(default)]
    entry: Vec<EntryFile>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EntryFile {
    source: String,
    entry: String,
    stage: String,
}

/// Reads `shaders.toml`'s text: every target and stage known, every source a plain `.slang` file
/// name, every function an identifier, and no two entry points sharing a blob.
pub fn parse_spec(text: &str) -> Result<Spec, String> {
    let file: SpecFile = toml::from_str(text).map_err(|e| format!("{SPEC}: {e}"))?;
    let mut targets = BTreeSet::new();
    for t in &file.targets {
        let known = Target::parse(t)
            .ok_or_else(|| format!("{SPEC}: unknown target {t:?} (known: spirv, dxil, msl)"))?;
        targets.insert(known);
    }
    if targets.is_empty() {
        return Err(format!("{SPEC}: `targets` names none"));
    }
    let mut entries = Vec::new();
    let mut variants = BTreeSet::new();
    for e in file.entry {
        let stage = Stage::ALL
            .into_iter()
            .find(|s| s.name() == e.stage)
            .ok_or_else(|| {
                format!(
                    "{SPEC}: {}: unknown stage {:?} (known: vertex, fragment, compute, mesh, task)",
                    e.source, e.stage
                )
            })?;
        let plain = |s: &str| {
            !s.is_empty()
                && s.chars()
                    .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        };
        let stem = e.source.strip_suffix(".slang").unwrap_or("");
        if !plain(stem) {
            return Err(format!(
                "{SPEC}: source {:?} is not a plain `.slang` file name in the folder",
                e.source
            ));
        }
        if !plain(&e.entry) || e.entry.contains('-') {
            return Err(format!(
                "{SPEC}: entry {:?} is not a function name",
                e.entry
            ));
        }
        let spec = EntrySpec {
            source: e.source,
            entry: e.entry,
            stage,
        };
        if !variants.insert(spec.variant()) {
            return Err(format!(
                "{SPEC}: {} in {} is named twice (or names the same blob as another)",
                spec.entry, spec.source
            ));
        }
        entries.push(spec);
    }
    Ok(Spec {
        targets: targets.into_iter().collect(),
        entries,
    })
}

/// A shader folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Root {
    pub dir: PathBuf,
    /// Its path from the workspace root, with forward slashes, for messages.
    pub rel: String,
}

/// Every shader folder under `dereth/client/crates/`: a crate's `shaders/` or `tests/shaders/`
/// that holds a `shaders.toml`, in path order.
#[must_use]
pub fn roots(ws: &Path) -> Vec<Root> {
    let mut out = Vec::new();
    let Ok(crates) = std::fs::read_dir(ws.join(SEARCH)) else {
        return out;
    };
    let mut crates: Vec<PathBuf> = crates.flatten().map(|e| e.path()).collect();
    crates.sort();
    for c in crates {
        for sub in ["shaders", "tests/shaders"] {
            let dir = c.join(sub);
            if dir.join(SPEC).is_file() {
                let rel = dir
                    .strip_prefix(ws)
                    .unwrap_or(&dir)
                    .to_string_lossy()
                    .replace('\\', "/");
                out.push(Root { dir, rel });
            }
        }
    }
    out
}

/// A text file's bytes with CRLF read as LF, so a checkout that converted line endings hashes the
/// same as the one the blobs were made from.
fn lf(bytes: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\r' && bytes.get(i + 1) == Some(&b'\n') {
            i += 1;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    out
}

/// The SHA-256 of every source in the folder (`shaders.toml` and every `.slang` file, outside
/// `compiled/`), keyed by path from the folder, with forward slashes.
pub fn source_hashes(dir: &Path) -> Result<BTreeMap<String, String>, String> {
    fn walk(base: &Path, dir: &Path, out: &mut BTreeMap<String, String>) -> Result<(), String> {
        let entries = std::fs::read_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        for e in entries.flatten() {
            let p = e.path();
            let rel = p
                .strip_prefix(base)
                .unwrap_or(&p)
                .to_string_lossy()
                .replace('\\', "/");
            if p.is_dir() {
                if rel != COMPILED {
                    walk(base, &p, out)?;
                }
            } else if rel == SPEC || p.extension().is_some_and(|x| x == "slang") {
                let bytes = std::fs::read(&p).map_err(|e| format!("{}: {e}", p.display()))?;
                out.insert(rel, sha256_hex(&lf(&bytes)));
            }
        }
        Ok(())
    }
    let mut out = BTreeMap::new();
    walk(dir, dir, &mut out)?;
    Ok(out)
}

/// One blob's record: its SHA-256 and its size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlobRecord {
    pub sha256: String,
    pub bytes: usize,
}

impl BlobRecord {
    fn of(bytes: &[u8]) -> Self {
        Self {
            sha256: sha256_hex(bytes),
            bytes: bytes.len(),
        }
    }
}

/// The compiler line every manifest records.
#[must_use]
pub fn compiler_line() -> String {
    format!("slangc {}", fetch::SLANG_VERSION)
}

/// The option lines every manifest records: the common options, and each checked-in target's.
fn option_lines(targets: &[Target]) -> Vec<(String, String)> {
    let mut v = vec![("common".to_owned(), COMMON_OPTIONS.join(" "))];
    v.extend(
        targets
            .iter()
            .map(|t| (t.name().to_owned(), t.options().join(" "))),
    );
    v
}

/// `manifest.toml`'s text. Every line under a table header is one fact, so a difference reads as
/// the lines that changed.
#[must_use]
pub fn render_manifest(
    compiler: &str,
    targets: &[Target],
    sources: &BTreeMap<String, String>,
    blobs: &BTreeMap<String, BlobRecord>,
) -> String {
    let mut s = String::from(
        "# Written by `cargo xtask shaders`: the compiler, its options, and the SHA-256 of every\n\
         # source and blob. `cargo xtask shaders --check` compares it with the files. Regenerate it;\n\
         # never edit it.\n\n",
    );
    s.push_str(&format!("compiler = {compiler:?}\n\n[options]\n"));
    for (k, v) in option_lines(targets) {
        s.push_str(&format!("{k} = {v:?}\n"));
    }
    s.push_str("\n[sources]\n");
    for (k, v) in sources {
        s.push_str(&format!("{k:?} = {v:?}\n"));
    }
    s.push_str("\n[blobs]\n");
    for (k, b) in blobs {
        s.push_str(&format!(
            "{k:?} = {{ sha256 = {:?}, bytes = {} }}\n",
            b.sha256, b.bytes
        ));
    }
    s
}

/// `shaders.rs`'s text: the `Entry` enum and its accessors, the blobs embedded.
#[must_use]
pub fn render_index(spec: &Spec) -> String {
    let mut s = String::from(
        "// Written by `cargo xtask shaders` from `shaders.toml`. Regenerate it; never edit it.\n\n\
         /// The entry points `shaders.toml` names, in its order.\n\
         #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]\n\
         pub enum Entry {\n",
    );
    for e in &spec.entries {
        s.push_str(&format!(
            "    /// `{}` in `{}`, the {} stage.\n    {},\n",
            e.entry,
            e.source,
            e.stage.name(),
            e.variant()
        ));
    }
    s.push_str("}\n\nimpl Entry {\n    /// Every entry point, in `shaders.toml`'s order.\n");
    let all: Vec<String> = spec
        .entries
        .iter()
        .map(|e| format!("Self::{}", e.variant()))
        .collect();
    s.push_str(&format!(
        "    pub const ALL: [Self; {}] = [{}];\n",
        all.len(),
        all.join(", ")
    ));
    let mut accessor = |doc: &str, name: &str, ty: &str, arm: &dyn Fn(&EntrySpec) -> String| {
        s.push_str(&format!(
            "\n    /// {doc}\n    #[must_use]\n    pub const fn {name}(self) -> {ty} {{\n        match self {{\n"
        ));
        for e in &spec.entries {
            s.push_str(&format!(
                "            Self::{} => {},\n",
                e.variant(),
                arm(e)
            ));
        }
        s.push_str("        }\n    }\n");
    };
    accessor(
        "The source file that defines it.",
        "source",
        "&'static str",
        &|e| format!("{:?}", e.source),
    );
    accessor(
        "Its function, which is also its name in every blob.",
        "name",
        "&'static str",
        &|e| format!("{:?}", e.entry),
    );
    accessor("The stage it runs in.", "stage", "Stage", &|e| {
        format!("Stage::{}", e.stage.variant())
    });
    for &t in &spec.targets {
        let (name, ty, mac) = t.accessor();
        accessor(
            &format!("Its {} blob, as the compiler wrote it.", t.label()),
            name,
            ty,
            &|e| format!("{mac}({:?})", e.blob(t)),
        );
    }
    s.push_str("}\n");
    s
}

/// What compiles one entry point for one target. `slangc` in use; a stand-in in the tests.
pub trait Compiler {
    fn compile(&self, dir: &Path, entry: &EntrySpec, target: Target) -> Result<Vec<u8>, String>;
}

/// The pinned `slangc`, with DXC's folder on its `PATH` when the DXIL target is wanted.
#[derive(Debug)]
pub struct Slangc {
    exe: PathBuf,
    dxc: Option<PathBuf>,
}

impl Slangc {
    /// The `slangc` in the Slang folder `dir`, refused unless it reports the pinned version.
    pub fn new(dir: &Path, dxc: Option<PathBuf>) -> Result<Self, String> {
        let exe = dir
            .join("bin")
            .join(format!("slangc{}", std::env::consts::EXE_SUFFIX));
        let out = Command::new(&exe)
            .arg("-v")
            .output()
            .map_err(|e| format!("running {}: {e}", exe.display()))?;
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
        if said.trim() != fetch::SLANG_VERSION {
            return Err(format!(
                "{} reports version {:?}, not the pinned {}; remove {} and run again",
                exe.display(),
                said.trim(),
                fetch::SLANG_VERSION,
                dir.display()
            ));
        }
        Ok(Self { exe, dxc })
    }
}

impl Compiler for Slangc {
    fn compile(&self, dir: &Path, entry: &EntrySpec, target: Target) -> Result<Vec<u8>, String> {
        let out = std::env::temp_dir().join(format!(
            "dereth-shaders-{}-{}.{}",
            std::process::id(),
            entry.variant(),
            target.extension()
        ));
        let mut cmd = Command::new(&self.exe);
        cmd.current_dir(dir)
            .arg(&entry.source)
            .args(["-entry", &entry.entry, "-stage", entry.stage.slang()])
            .args(COMMON_OPTIONS)
            .args(target.options())
            .args(["-I", "."])
            .arg("-o")
            .arg(&out);
        if let (Target::Dxil, Some(dxc)) = (target, &self.dxc) {
            let mut path = vec![dxc.clone()];
            path.extend(std::env::split_paths(
                &std::env::var_os("PATH").unwrap_or_default(),
            ));
            cmd.env(
                "PATH",
                std::env::join_paths(path).map_err(|e| e.to_string())?,
            );
        }
        let ran = cmd
            .output()
            .map_err(|e| format!("running {}: {e}", self.exe.display()))?;
        let said = format!(
            "{}{}",
            String::from_utf8_lossy(&ran.stdout),
            String::from_utf8_lossy(&ran.stderr)
        );
        let bytes = std::fs::read(&out);
        std::fs::remove_file(&out).ok();
        if !ran.status.success() {
            return Err(format!(
                "{} {} ({}): slangc failed\n{said}",
                entry.source,
                entry.entry,
                target.name()
            ));
        }
        let bytes = bytes.map_err(|e| {
            format!(
                "{} {} ({}): slangc wrote no blob: {e}\n{said}",
                entry.source,
                entry.entry,
                target.name()
            )
        })?;
        Ok(if target.is_text() { lf(&bytes) } else { bytes })
    }
}

/// Every blob `spec` asks for, compiled for `targets`.
fn compile_blobs(
    root: &Root,
    spec: &Spec,
    targets: &[Target],
    compiler: &dyn Compiler,
) -> Result<BTreeMap<String, Vec<u8>>, String> {
    for e in &spec.entries {
        if !root.dir.join(&e.source).is_file() {
            return Err(format!(
                "{}: {SPEC} names {}, which is not there",
                root.rel, e.source
            ));
        }
    }
    let mut blobs = BTreeMap::new();
    let mut failed = Vec::new();
    for &t in targets {
        for e in &spec.entries {
            match compiler.compile(&root.dir, e, t) {
                Ok(bytes) if bytes.is_empty() => failed.push(format!(
                    "{} {} ({}): the blob is empty",
                    e.source,
                    e.entry,
                    t.name()
                )),
                Ok(bytes) => {
                    blobs.insert(e.blob(t), bytes);
                }
                Err(e) => failed.push(e),
            }
        }
    }
    if failed.is_empty() {
        Ok(blobs)
    } else {
        Err(format!(
            "{}: {} of {} blob(s) failed to compile, so nothing was written\n{}",
            root.rel,
            failed.len(),
            failed.len() + blobs.len(),
            failed.join("\n")
        ))
    }
}

/// Writes `bytes` to `path` unless it already holds them. Returns whether it wrote.
fn write_if_changed(path: &Path, bytes: &[u8]) -> Result<bool, String> {
    if std::fs::read(path).is_ok_and(|old| old == bytes) {
        return Ok(false);
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("{}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(true)
}

/// Every file under `dir`, keyed by path from it with forward slashes.
fn files_under(dir: &Path) -> Vec<String> {
    crate::util::source_files(dir, &[])
        .into_iter()
        .filter_map(|p| {
            p.strip_prefix(dir)
                .ok()
                .map(|r| r.to_string_lossy().replace('\\', "/"))
        })
        .collect()
}

/// What one folder's compilation wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    /// Blobs per target: `(count, bytes)`.
    pub per_target: BTreeMap<Target, (usize, usize)>,
    /// Files whose bytes changed (blobs, the manifest, the index), and files removed.
    pub changed: Vec<String>,
    pub removed: Vec<String>,
}

/// Compiles one folder's checked-in targets and writes its `compiled/`.
pub fn compile_root(root: &Root, compiler: &dyn Compiler) -> Result<Written, String> {
    let text = std::fs::read_to_string(root.dir.join(SPEC))
        .map_err(|e| format!("{}/{SPEC}: {e}", root.rel))?;
    let spec = parse_spec(&text).map_err(|e| format!("{}: {e}", root.rel))?;
    let blobs = compile_blobs(root, &spec, &spec.targets, compiler)?;
    let compiled = root.dir.join(COMPILED);
    let mut written = Written::default();
    let mut records = BTreeMap::new();
    for (path, bytes) in &blobs {
        if write_if_changed(&compiled.join(path), bytes)? {
            written.changed.push(path.clone());
        }
        records.insert(path.clone(), BlobRecord::of(bytes));
        let target = Target::parse(path.split('/').next().unwrap_or("")).unwrap_or(Target::Spirv);
        let slot = written.per_target.entry(target).or_default();
        slot.0 += 1;
        slot.1 += bytes.len();
    }
    let manifest = render_manifest(
        &compiler_line(),
        &spec.targets,
        &source_hashes(&root.dir)?,
        &records,
    );
    let index = render_index(&spec);
    for (name, text) in [(MANIFEST, &manifest), (INDEX, &index)] {
        if write_if_changed(&compiled.join(name), text.as_bytes())? {
            written.changed.push(name.to_owned());
        }
    }
    for f in files_under(&compiled) {
        if !blobs.contains_key(&f) && f != MANIFEST && f != INDEX {
            let p = compiled.join(&f);
            std::fs::remove_file(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            written.removed.push(f);
        }
    }
    for t in Target::ALL {
        // An emptied target folder goes too; one that still holds a file is kept.
        std::fs::remove_dir(compiled.join(t.name())).ok();
    }
    Ok(written)
}

/// What `--check` finds wrong with one folder: each a sentence naming the file. Empty when the
/// checked-in blobs, manifest and index are exactly what the sources make.
#[must_use]
pub fn check_root(root: &Root) -> Vec<String> {
    let mut found = Vec::new();
    let at = |what: String| format!("{}: {what}", root.rel);
    let text = match std::fs::read_to_string(root.dir.join(SPEC)) {
        Ok(t) => t,
        Err(e) => return vec![at(format!("{SPEC}: {e}"))],
    };
    let spec = match parse_spec(&text) {
        Ok(s) => s,
        Err(e) => return vec![at(e)],
    };
    let compiled = root.dir.join(COMPILED);
    let sources = match source_hashes(&root.dir) {
        Ok(s) => s,
        Err(e) => return vec![at(e)],
    };
    let mut blobs = BTreeMap::new();
    for &t in &spec.targets {
        for e in &spec.entries {
            let path = e.blob(t);
            match std::fs::read(compiled.join(&path)) {
                Ok(bytes) => {
                    let bytes = if t.is_text() { lf(&bytes) } else { bytes };
                    blobs.insert(path, BlobRecord::of(&bytes));
                }
                Err(_) => found.push(at(format!(
                    "{COMPILED}/{path} is missing: run `cargo xtask shaders`"
                ))),
            }
        }
    }
    let expected = render_manifest(&compiler_line(), &spec.targets, &sources, &blobs);
    match std::fs::read(compiled.join(MANIFEST)) {
        Ok(recorded) => {
            let recorded = String::from_utf8_lossy(&lf(&recorded)).into_owned();
            found.extend(
                manifest_differences(&expected, &recorded)
                    .into_iter()
                    .map(at),
            );
        }
        Err(_) => found.push(at(format!(
            "{COMPILED}/{MANIFEST} is missing: run `cargo xtask shaders`"
        ))),
    }
    let index = render_index(&spec);
    match std::fs::read(compiled.join(INDEX)) {
        Ok(recorded) if lf(&recorded) == index.as_bytes() => {}
        Ok(_) => found.push(at(format!(
            "{COMPILED}/{INDEX} is not the index {SPEC} makes: run `cargo xtask shaders`"
        ))),
        Err(_) => found.push(at(format!(
            "{COMPILED}/{INDEX} is missing: run `cargo xtask shaders`"
        ))),
    }
    let asked: BTreeSet<String> = spec
        .targets
        .iter()
        .flat_map(|&t| spec.entries.iter().map(move |e| e.blob(t)))
        .collect();
    for f in files_under(&compiled) {
        if !asked.contains(&f) && f != MANIFEST && f != INDEX {
            found.push(at(format!(
                "{COMPILED}/{f} is not a blob {SPEC} asks for: run `cargo xtask shaders`"
            )));
        }
    }
    found
}

/// The facts on which two manifests differ, as sentences. Each fact is one line, keyed by its
/// table and its key.
fn manifest_differences(expected: &str, recorded: &str) -> Vec<String> {
    fn facts(text: &str) -> BTreeMap<(String, String), String> {
        let mut table = String::new();
        let mut out = BTreeMap::new();
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(t) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
                t.clone_into(&mut table);
                continue;
            }
            if let Some((k, v)) = line.split_once(" = ") {
                out.insert(
                    (table.clone(), k.trim_matches('"').to_owned()),
                    v.to_owned(),
                );
            }
        }
        out
    }
    let (want, have) = (facts(expected), facts(recorded));
    let mut out = Vec::new();
    let keys: BTreeSet<&(String, String)> = want.keys().chain(have.keys()).collect();
    for key in keys {
        let (table, name) = (key.0.as_str(), key.1.as_str());
        let sentence = match (want.get(key), have.get(key)) {
            (Some(a), Some(b)) if a == b => continue,
            (Some(a), Some(b)) => match table {
                "sources" => format!("{name} changed since its blobs were compiled"),
                "blobs" => format!("{COMPILED}/{name} is not the blob the manifest records"),
                "options" => {
                    format!("the {name} options are now {a}, the blobs were made with {b}")
                }
                _ => format!("{name} is now {a}, the blobs were made with {b}"),
            },
            (Some(_), None) => match table {
                "sources" => format!("{name} is new since the blobs were compiled"),
                "blobs" => format!("{COMPILED}/{name} is not in the manifest"),
                _ => format!("the manifest records no {name}"),
            },
            (None, Some(_)) => match table {
                "sources" => format!("{name} is gone since the blobs were compiled"),
                "blobs" => {
                    format!("the manifest records {COMPILED}/{name}, which nothing asks for")
                }
                _ => format!("the manifest records a {name} nothing uses"),
            },
            (None, None) => continue,
        };
        out.push(format!("{sentence}: run `cargo xtask shaders`"));
    }
    out
}

/// `--check` over every shader folder: prints each finding, and returns 0 when there are none.
#[must_use]
pub fn check_all(ws: &Path) -> i32 {
    let roots = roots(ws);
    let mut found = Vec::new();
    for r in &roots {
        found.extend(check_root(r));
    }
    for f in &found {
        println!("  {f}");
    }
    println!(
        "shaders --check: {} folder(s), {} finding(s)",
        roots.len(),
        found.len()
    );
    i32::from(!found.is_empty())
}

/// The arguments `shaders` takes.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub check: bool,
    /// Targets compiled outside the tree as well.
    pub extra: Vec<Target>,
    pub tools: Option<PathBuf>,
}

pub fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--check" => out.check = true,
            "--dxil" => out.extra.push(Target::Dxil),
            "--msl" => out.extra.push(Target::Msl),
            "--tools" => {
                out.tools = Some(PathBuf::from(it.next().ok_or("--tools needs a folder")?));
            }
            other => return Err(format!("unknown argument {other:?}")),
        }
    }
    if out.check && (!out.extra.is_empty() || out.tools.is_some()) {
        return Err("--check compiles nothing, so it takes no other argument".into());
    }
    Ok(out)
}

/// `cargo xtask shaders`.
pub fn shaders(args: &[String]) -> i32 {
    let args = match parse_args(args) {
        Ok(a) => a,
        Err(e) => {
            eprintln!(
                "shaders: {e}\nusage: cargo xtask shaders [--dxil] [--msl] [--tools <dir>] | --check"
            );
            return 2;
        }
    };
    let ws = workspace_root();
    if args.check {
        return check_all(&ws);
    }
    match compile_all(&ws, &args) {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("shaders: {e}");
            1
        }
    }
}

fn compile_all(ws: &Path, args: &Args) -> Result<(), String> {
    let roots = roots(ws);
    let specs: Vec<Spec> = roots
        .iter()
        .map(|r| {
            std::fs::read_to_string(r.dir.join(SPEC))
                .map_err(|e| format!("{}/{SPEC}: {e}", r.rel))
                .and_then(|t| parse_spec(&t).map_err(|e| format!("{}: {e}", r.rel)))
        })
        .collect::<Result<_, _>>()?;
    let cache = match &args.tools {
        Some(d) => d.clone(),
        None => fetch::cache_dir().ok_or(
            "no per-user cache folder: set LOCALAPPDATA (Windows) or HOME, or pass --tools <dir>",
        )?,
    };
    let (os, arch) = (std::env::consts::OS, std::env::consts::ARCH);
    let pin = fetch::slang_pin(os, arch)
        .ok_or_else(|| format!("Slang publishes no release for {os} {arch}"))?;
    let slang = fetch::ensure(&cache, &pin, fetch::keep_slang)?;
    let wants_dxil = args.extra.contains(&Target::Dxil)
        || specs.iter().any(|s| s.targets.contains(&Target::Dxil));
    let dxc = if wants_dxil {
        let pin = fetch::dxc_pin(os)
            .ok_or("the DXIL target is compiled on Windows, where DXC's release runs")?;
        Some(fetch::ensure(&cache, &pin, fetch::keep_dxc)?)
    } else {
        None
    };
    let compiler = Slangc::new(&slang, dxc)?;
    println!(
        "{} {}, licence and notices in {}",
        compiler_line(),
        compiler.exe.display(),
        slang.display()
    );
    let mut extra_failed = 0;
    for (root, spec) in roots.iter().zip(&specs) {
        let w = compile_root(root, &compiler)?;
        for (t, (n, bytes)) in &w.per_target {
            println!("{}: {n} {} blob(s), {bytes} bytes", root.rel, t.name());
        }
        for f in &w.changed {
            println!("  wrote {COMPILED}/{f}");
        }
        for f in &w.removed {
            println!("  removed {COMPILED}/{f}");
        }
        let extra: Vec<Target> = args
            .extra
            .iter()
            .copied()
            .filter(|t| !spec.targets.contains(t))
            .collect();
        for &t in &extra {
            let out = target_dir().join("shaders").join(&root.rel);
            match compile_blobs(root, spec, &[t], &compiler) {
                Ok(blobs) => {
                    let total: usize = blobs.values().map(Vec::len).sum();
                    for (path, bytes) in &blobs {
                        write_if_changed(&out.join(path), bytes)?;
                    }
                    println!(
                        "  {} {} blob(s), {total} bytes, outside the tree in {}",
                        blobs.len(),
                        t.name(),
                        out.display()
                    );
                }
                Err(e) => {
                    println!("  {}: {e}", t.name());
                    extra_failed += 1;
                }
            }
        }
    }
    if extra_failed == 0 {
        Ok(())
    } else {
        Err(format!(
            "{extra_failed} target(s) compiled outside the tree failed; the checked-in blobs \
             are written"
        ))
    }
}
