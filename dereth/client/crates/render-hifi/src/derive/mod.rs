//! Shaders derived from the shared shader source by anchored replacement, so a derived pipeline
//! keeps every stage operation, alpha test and constant of the pipeline it comes from and changes
//! only what its variant is for.
//!
//! Each replacement names an anchor: a piece of text that must appear exactly once in its scope,
//! which is the whole source or the body of one named function. A source that has changed so an
//! anchor no longer matches once fails the derivation, and the ordinary pipelines are untouched
//! by it: only the derived pipeline is missing, and its draws keep their ordinary shading.

use std::fmt;
use std::ops::Range;

use dereth_render::wgpu::sidecar::{legacy_shader_source, splat_shader_source};
use dereth_render::{PipelineKey, VertexFormat};

pub mod cache;
pub mod pipeline;
pub mod reshade;

/// What a derived pipeline is for.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Variant {
    /// Linear high dynamic range colour, normals and material class.
    Reshade,
    /// Depth only, from a light's point of view.
    ShadowDepth,
    /// Depth only with the alpha test, from a light's point of view.
    ShadowClip,
    /// The ordinary shading with wind sway on the vertices.
    SwayLegacy,
    /// The re-shaded output with wind sway on the vertices.
    SwayReshade,
    /// What each surface is made of -- albedo, its own light, its normal -- for light computed
    /// per pixel.
    Surface,
}

impl Variant {
    /// Every variant. A new variant is added here as well as to the enum.
    pub const ALL: [Self; 6] = [
        Self::Reshade,
        Self::ShadowDepth,
        Self::ShadowClip,
        Self::SwayLegacy,
        Self::SwayReshade,
        Self::Surface,
    ];
}

/// The key of one derived pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DerivedKey {
    /// The key of the pipeline it derives from.
    pub key: PipelineKey,
    /// Whether that pipeline is a landscape splat.
    pub splat: bool,
    /// What it is for.
    pub variant: Variant,
}

/// The key of one derived shader module. Every pipeline of one vertex format, splat or not,
/// shares the module, as the ordinary pipelines share theirs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ModuleKey {
    /// The vertex format the module is specialised for.
    pub format: VertexFormat,
    /// Whether it carries the landscape splat's pixel stage.
    pub splat: bool,
    /// What it is for.
    pub variant: Variant,
}

impl ModuleKey {
    /// Every module of `variant`: each vertex format, with and without the splat.
    pub fn all(variant: Variant) -> impl Iterator<Item = Self> {
        VertexFormat::all().into_iter().flat_map(move |format| {
            [false, true].map(|splat| Self {
                format,
                splat,
                variant,
            })
        })
    }
}

impl DerivedKey {
    /// The shader module this pipeline's stages come from.
    #[must_use]
    pub const fn module(&self) -> ModuleKey {
        ModuleKey {
            format: self.key.vertex_format,
            splat: self.splat,
            variant: self.variant,
        }
    }
}

/// Where an anchor is looked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    /// The whole source.
    Source,
    /// The body of the one function of this name: from `fn <name>(` to the closing brace at the
    /// start of a line.
    Function(&'static str),
}

impl fmt::Display for Scope {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Source => f.write_str("the source"),
            Self::Function(name) => write!(f, "fn {name}"),
        }
    }
}

/// One anchored replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Anchor {
    /// A short name for messages and tests.
    pub name: &'static str,
    /// Where `find` is looked for.
    pub scope: Scope,
    /// The text that must appear in `scope` exactly once.
    pub find: &'static str,
    /// What it becomes.
    pub with: String,
}

/// Why a derivation failed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeriveError {
    /// An anchor did not match exactly once.
    Anchor {
        /// The anchor.
        anchor: String,
        /// Where it was looked for.
        scope: String,
        /// How many times it matched.
        matches: usize,
    },
    /// A function scope's header did not appear exactly once.
    Scope {
        /// The function.
        function: String,
        /// How many times its header appeared.
        matches: usize,
    },
    /// No derivation exists yet for this variant.
    NotDerived(Variant),
}

impl fmt::Display for DeriveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Anchor {
                anchor,
                scope,
                matches,
            } => write!(
                f,
                "anchor {anchor:?} matched {matches} times in {scope}, not once"
            ),
            Self::Scope { function, matches } => {
                write!(f, "fn {function} appears {matches} times, not once")
            }
            Self::NotDerived(v) => write!(f, "no derivation for {v:?}"),
        }
    }
}

impl std::error::Error for DeriveError {}

/// `source` with its one occurrence of `anchor` replaced by `with`.
///
/// # Errors
/// [`DeriveError::Anchor`] when `anchor` does not occur exactly once.
pub fn replace_once(source: &str, anchor: &str, with: &str) -> Result<String, DeriveError> {
    let matches = source.matches(anchor).count();
    if matches != 1 {
        return Err(DeriveError::Anchor {
            anchor: anchor.to_owned(),
            scope: Scope::Source.to_string(),
            matches,
        });
    }
    Ok(source.replacen(anchor, with, 1))
}

/// The byte range of `scope` in `source`, or `None` when a function scope is absent.
///
/// # Errors
/// [`DeriveError::Scope`] when a function's header appears more than once.
pub fn scope_range(source: &str, scope: Scope) -> Result<Option<Range<usize>>, DeriveError> {
    let Scope::Function(name) = scope else {
        return Ok(Some(0..source.len()));
    };
    let header = format!("fn {name}(");
    let starts: Vec<usize> = source
        .match_indices(&header)
        .map(|(at, _)| at)
        // A header begins a line; `fn` inside a longer name or a comment does not count.
        .filter(|&at| at == 0 || source.as_bytes()[at - 1] == b'\n')
        .collect();
    match starts.as_slice() {
        [] => Ok(None),
        [start] => {
            let body = &source[*start..];
            let end = body
                .find("\n}\n")
                .map_or(source.len(), |at| *start + at + "\n}".len());
            Ok(Some(*start..end))
        }
        many => Err(DeriveError::Scope {
            function: name.to_owned(),
            matches: many.len(),
        }),
    }
}

/// How many times `anchor` matches in its scope of `source`: 0 when the scope is absent.
///
/// # Errors
/// [`DeriveError::Scope`] when the scope's function appears more than once.
pub fn matches(source: &str, anchor: &Anchor) -> Result<usize, DeriveError> {
    Ok(scope_range(source, anchor.scope)?.map_or(0, |r| source[r].matches(anchor.find).count()))
}

/// `source` with every anchor of `anchors` replaced, in order.
///
/// # Errors
/// [`DeriveError`] for the first anchor that does not match exactly once in its scope.
pub fn apply(source: &str, anchors: &[Anchor]) -> Result<String, DeriveError> {
    let mut text = source.to_owned();
    for anchor in anchors {
        let fail = |matches| DeriveError::Anchor {
            anchor: anchor.name.to_owned(),
            scope: anchor.scope.to_string(),
            matches,
        };
        let range = scope_range(&text, anchor.scope)?.ok_or_else(|| fail(0))?;
        let body = &text[range.clone()];
        let found: Vec<usize> = body.match_indices(anchor.find).map(|(at, _)| at).collect();
        let [at] = found.as_slice() else {
            return Err(fail(found.len()));
        };
        let at = range.start + at;
        text.replace_range(at..at + anchor.find.len(), &anchor.with);
    }
    Ok(text)
}

/// The shader text the device compiles for the ordinary pipelines of `format`: the splat's when
/// `splat`.
#[must_use]
pub fn legacy_source(format: VertexFormat, splat: bool) -> String {
    if splat {
        splat_shader_source(format)
    } else {
        legacy_shader_source(format)
    }
}

/// The anchors `module` applies to its ordinary source; for the surface variant, the anchors it
/// applies to the re-shaded text after the re-shade's own.
///
/// # Errors
/// [`DeriveError::NotDerived`] for a variant with no derivation yet.
pub fn anchors(module: ModuleKey) -> Result<Vec<Anchor>, DeriveError> {
    match module.variant {
        Variant::Reshade => Ok(reshade::anchors(module.format, module.splat)),
        Variant::Surface => Ok(crate::passes::lighting::derive::anchors(module.splat)),
        other => Err(DeriveError::NotDerived(other)),
    }
}

/// `module`'s shader text, derived from `source`, the ordinary text for its format.
///
/// # Errors
/// [`DeriveError`] when an anchor does not match once, or the variant has no derivation.
pub fn derive_from(source: &str, module: ModuleKey) -> Result<String, DeriveError> {
    match module.variant {
        Variant::Reshade => {
            let anchors = reshade::anchors(module.format, module.splat);
            Ok(apply(source, &anchors)? + &reshade::prelude(module.format))
        }
        Variant::Surface => {
            use crate::passes::lighting::derive as surface;
            let reshaded = derive_from(
                source,
                ModuleKey {
                    variant: Variant::Reshade,
                    ..module
                },
            )?;
            Ok(apply(&reshaded, &surface::anchors(module.splat))?
                + &surface::prelude(module.format, module.splat))
        }
        other => Err(DeriveError::NotDerived(other)),
    }
}

/// `module`'s shader text, derived from the text the device compiles for its ordinary pipelines.
///
/// # Errors
/// As [`derive_from`].
pub fn derive(module: ModuleKey) -> Result<String, DeriveError> {
    derive_from(&legacy_source(module.format, module.splat), module)
}
