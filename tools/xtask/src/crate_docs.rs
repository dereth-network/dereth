//! `cargo xtask crate-docs`: every crate says what it is, in one shape, and the workspace map
//! lists every crate.
//!
//! Each workspace crate's `lib.rs` (or `main.rs`, for a crate that is only a binary) opens with a
//! `//!` header in one shape:
//!
//! 1. its first sentence says what the crate is, and is the crate's Cargo `description`, word for
//!    word once the markdown is taken out;
//! 2. a paragraph opening `**Depends on**` names the workspace crates it depends on and, after
//!    `**Used by**`, what depends on it;
//! 3. a paragraph opening `**Must never**` states its rule (its `cargo xtask seams` row, where it
//!    has one);
//! 4. optionally, an orientation to its modules.
//!
//! The check reads `cargo metadata`, so the dependency statements are held to the manifests:
//! every workspace crate named after `**Depends on**` must be a dependency of some kind, every
//! production dependency must be named, and every crate named after `**Used by**` must really
//! depend on this one. The last is one-way on purpose: "used by nearly every crate" is a truthful
//! summary that a complete list would only make longer.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::util::{print_table, workspace_root, Outcome, Report};

/// One workspace crate, as the check sees it.
#[derive(Debug, Clone, Default)]
pub struct CrateInfo {
    pub name: String,
    pub description: Option<String>,
    /// The file whose `//!` header is the crate's: the library root, else the first binary's.
    pub header_file: PathBuf,
    /// Workspace crates it depends on in `[dependencies]` (optional ones included).
    pub normal: BTreeSet<String>,
    /// Workspace crates it depends on in any table.
    pub any: BTreeSet<String>,
}

/// The paragraph lead-ins a header must carry.
const DEPENDS: &str = "**Depends on**";
const USED_BY: &str = "**Used by**";
const NEVER: &str = "**Must never**";

/// The `//!` header a source file opens with, without the markers: `None` if the first line that
/// is not blank and not a plain `//` comment is anything else.
pub fn header_of(src: &str) -> Option<String> {
    let mut lines = src
        .lines()
        .skip_while(|l| l.trim().is_empty() || (l.starts_with("//") && !l.starts_with("//!")))
        .peekable();
    lines.peek().filter(|l| l.starts_with("//!"))?;
    let body: Vec<&str> = lines
        .take_while(|l| l.starts_with("//!"))
        .map(|l| {
            let rest = &l[3..];
            rest.strip_prefix(' ').unwrap_or(rest)
        })
        .collect();
    let text = body.join("\n");
    (!text.trim().is_empty()).then_some(text)
}

/// The header's paragraphs, each joined onto one line. Fenced code blocks are dropped.
fn paragraphs(header: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<&str> = Vec::new();
    let mut in_code = false;
    for line in header.lines() {
        if line.trim_start().starts_with("```") {
            in_code = !in_code;
            continue;
        }
        if in_code {
            continue;
        }
        if line.trim().is_empty() {
            if !cur.is_empty() {
                out.push(cur.join(" "));
                cur.clear();
            }
        } else {
            cur.push(line.trim());
        }
    }
    if !cur.is_empty() {
        out.push(cur.join(" "));
    }
    out
}

/// The first sentence of the header: its first paragraph up to the first `.`, `!` or `?` that is
/// followed by white space or ends the paragraph, not counting one inside a code span.
pub fn first_sentence(header: &str) -> String {
    let para = paragraphs(header).into_iter().next().unwrap_or_default();
    let chars: Vec<char> = para.chars().collect();
    let mut in_code = false;
    for (i, &c) in chars.iter().enumerate() {
        if c == '`' {
            in_code = !in_code;
        } else if !in_code
            && matches!(c, '.' | '!' | '?')
            && chars.get(i + 1).is_none_or(|n| n.is_whitespace())
        {
            return chars[..=i].iter().collect();
        }
    }
    para
}

/// Text with its markdown taken out: link targets, brackets, code ticks and emphasis stars go, and
/// runs of white space become one space.
pub fn normalise(text: &str) -> String {
    let mut out = String::new();
    let mut rest = text;
    // `[label](target)` keeps its label.
    while let Some(i) = rest.find("](") {
        let Some(j) = rest[i..].find(')') else { break };
        out.push_str(&rest[..i]);
        rest = &rest[i + j + 1..];
    }
    out.push_str(rest);
    let stripped: String = out
        .chars()
        .filter(|c| !matches!(c, '`' | '*' | '[' | ']'))
        .collect();
    stripped.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Why a description is not plain text, if it is not.
pub fn description_problem(desc: &str) -> Option<String> {
    if desc.trim().is_empty() {
        return Some("the description is empty".to_owned());
    }
    if desc.contains(['`', '*', '[', ']']) {
        return Some(format!("the description carries markdown: {desc:?}"));
    }
    None
}

/// What is wrong with the header's shape.
pub fn shape_problems(header: &str) -> Vec<String> {
    let paras = paragraphs(header);
    let mut out = Vec::new();
    let depends = paras.iter().position(|p| p.starts_with(DEPENDS));
    let never = paras.iter().position(|p| p.starts_with(NEVER));
    match depends {
        None => out.push(format!("no paragraph opens with {DEPENDS}")),
        Some(0) => out.push(format!(
            "the {DEPENDS} paragraph is the first: the header must open with what the crate is"
        )),
        Some(d) => {
            let used_here = paras[d].contains(USED_BY);
            let used_next = paras.get(d + 1).is_some_and(|p| p.starts_with(USED_BY));
            if !used_here && !used_next {
                out.push(format!(
                    "the {DEPENDS} paragraph says nothing {USED_BY} (in it, or in the next paragraph)"
                ));
            }
        }
    }
    match (depends, never) {
        (_, None) => out.push(format!("no paragraph opens with {NEVER}")),
        (Some(d), Some(n)) if n < d => {
            out.push(format!("the {NEVER} paragraph comes before {DEPENDS}"))
        }
        _ => {}
    }
    out
}

/// The workspace crates named in code spans in `text`, other than `me`.
fn named(text: &str, workspace: &BTreeSet<String>, me: &str) -> BTreeSet<String> {
    text.split('`')
        .skip(1)
        .step_by(2)
        .filter(|s| workspace.contains(*s) && *s != me)
        .map(str::to_owned)
        .collect()
}

/// What the header says about dependencies that the manifests contradict.
///
/// `users` is the set of workspace crates that depend on this one in any table.
pub fn dependency_problems(
    krate: &CrateInfo,
    header: &str,
    workspace: &BTreeSet<String>,
    users: &BTreeSet<String>,
) -> Vec<String> {
    let paras = paragraphs(header);
    let Some(d) = paras.iter().position(|p| p.starts_with(DEPENDS)) else {
        return Vec::new(); // reported by the shape check
    };
    let (depends, used) = match paras[d].find(USED_BY) {
        Some(u) => (paras[d][..u].to_owned(), paras[d][u..].to_owned()),
        None => (
            paras[d].clone(),
            paras
                .get(d + 1)
                .filter(|p| p.starts_with(USED_BY))
                .cloned()
                .unwrap_or_default(),
        ),
    };
    let me = krate.name.as_str();
    let said_deps = named(&depends, workspace, me);
    let said_users = named(&used, workspace, me);
    let mut out = Vec::new();
    for n in said_deps.difference(&krate.any) {
        out.push(format!(
            "names `{n}` after {DEPENDS}, but it is not a dependency"
        ));
    }
    for n in krate.normal.iter().filter(|n| n.as_str() != me) {
        if !said_deps.contains(n) {
            out.push(format!(
                "depends on `{n}` but does not name it after {DEPENDS}"
            ));
        }
    }
    for n in said_users.difference(users) {
        out.push(format!(
            "names `{n}` after {USED_BY}, but `{n}` does not depend on it"
        ));
    }
    out
}

/// The workspace crates, from `cargo metadata`.
fn crates(ws: &Path) -> Result<Vec<CrateInfo>, String> {
    let out = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(ws)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "cargo metadata failed: {}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    let v: serde_json::Value =
        serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let packages = v["packages"]
        .as_array()
        .ok_or("cargo metadata: no packages")?;
    let names: BTreeSet<String> = packages
        .iter()
        .filter_map(|p| p["name"].as_str().map(str::to_owned))
        .collect();
    let mut result = Vec::new();
    for p in packages {
        let name = p["name"].as_str().unwrap_or_default().to_owned();
        let targets = p["targets"].as_array().cloned().unwrap_or_default();
        let kind_is = |t: &serde_json::Value, k: &str| {
            t["kind"]
                .as_array()
                .is_some_and(|ks| ks.iter().any(|x| x == k))
        };
        let target = targets
            .iter()
            .find(|t| kind_is(t, "lib") || kind_is(t, "rlib") || kind_is(t, "cdylib"))
            .or_else(|| targets.iter().find(|t| kind_is(t, "bin")))
            .ok_or_else(|| format!("{name}: no library or binary target"))?;
        let mut info = CrateInfo {
            name,
            description: p["description"].as_str().map(str::to_owned),
            header_file: PathBuf::from(target["src_path"].as_str().unwrap_or_default()),
            ..CrateInfo::default()
        };
        for d in p["dependencies"].as_array().into_iter().flatten() {
            let dep = d["name"].as_str().unwrap_or_default();
            if !names.contains(dep) {
                continue;
            }
            info.any.insert(dep.to_owned());
            if d["kind"].is_null() {
                info.normal.insert(dep.to_owned());
            }
        }
        result.push(info);
    }
    result.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(result)
}

/// Every crate-docs check, as rows for a table.
pub fn reports() -> Vec<Report> {
    let ws = workspace_root();
    let all = match crates(&ws) {
        Ok(c) => c,
        Err(e) => return vec![Report::new("crate-docs", Outcome::Fail, e)],
    };
    let workspace: BTreeSet<String> = all.iter().map(|c| c.name.clone()).collect();
    let mut users: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    for c in &all {
        for d in &c.any {
            users.entry(d.clone()).or_default().insert(c.name.clone());
        }
    }
    let (mut desc, mut head, mut same, mut shape, mut deps) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for c in &all {
        match &c.description {
            None => desc.push(format!("{}: no `description` in Cargo.toml", c.name)),
            Some(d) => {
                if let Some(p) = description_problem(d) {
                    desc.push(format!("{}: {p}", c.name));
                }
            }
        }
        let src = std::fs::read_to_string(&c.header_file).unwrap_or_default();
        let Some(header) = header_of(&src) else {
            head.push(format!(
                "{}: {} does not open with a `//!` header",
                c.name,
                c.header_file.display()
            ));
            continue;
        };
        if let Some(d) = &c.description {
            let first = normalise(&first_sentence(&header));
            if first != normalise(d) {
                same.push(format!(
                    "{}: the header's first sentence {first:?} is not the description {d:?}",
                    c.name
                ));
            }
        }
        shape.extend(
            shape_problems(&header)
                .into_iter()
                .map(|p| format!("{}: {p}", c.name)),
        );
        let none = BTreeSet::new();
        deps.extend(
            dependency_problems(c, &header, &workspace, users.get(&c.name).unwrap_or(&none))
                .into_iter()
                .map(|p| format!("{}: {p}", c.name)),
        );
    }
    let n = all.len();
    let row = |name: &str, problems: Vec<String>, ok: String| {
        if problems.is_empty() {
            Report::new(name, Outcome::Pass, ok)
        } else {
            for p in &problems {
                println!("crate-docs: {p}");
            }
            Report::new(
                name,
                Outcome::Fail,
                format!("{} problem(s); see above", problems.len()),
            )
        }
    };
    vec![
        row(
            "crate-docs: descriptions",
            desc,
            format!("{n} crates, each with a plain-text description"),
        ),
        row(
            "crate-docs: headers",
            head,
            format!("{n} crate roots open with a //! header"),
        ),
        row(
            "crate-docs: header = description",
            same,
            "each header's first sentence is its description".to_owned(),
        ),
        row(
            "crate-docs: header shape",
            shape,
            format!("{DEPENDS} ... {USED_BY}, then {NEVER}"),
        ),
        row(
            "crate-docs: dependency statements",
            deps,
            "what the headers name agrees with cargo metadata".to_owned(),
        ),
    ]
}

/// `cargo xtask crate-docs`.
pub fn crate_docs() -> i32 {
    print_table("xtask crate-docs", &reports())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ORACLE: none needed -- these calibrate the checker against planted violations, because a
    // check that silently stops firing is worse than none.

    const GOOD: &str = "\
//! The swept-sphere physics engine: cells, BSP trees and collision.
//!
//! **Depends on** `dereth-primitives` (its tests read `dereth-dat`). **Used by** the runtime
//! (`dereth-client-runtime`).
//!
//! **Must never** decode data files.
//!
//! ```text
//! not a paragraph `dereth-bogus`.
//! ```

pub mod step;
";

    fn ws() -> BTreeSet<String> {
        [
            "dereth-primitives",
            "dereth-dat",
            "dereth-physics",
            "dereth-client-runtime",
            "dereth-render",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    }

    fn physics() -> CrateInfo {
        CrateInfo {
            name: "dereth-physics".to_owned(),
            normal: ["dereth-primitives".to_owned()].into(),
            any: ["dereth-primitives".to_owned(), "dereth-dat".to_owned()].into(),
            ..CrateInfo::default()
        }
    }

    fn users() -> BTreeSet<String> {
        ["dereth-client-runtime".to_owned()].into()
    }

    #[test]
    fn a_well_formed_header_passes_every_check() {
        let h = header_of(GOOD).expect("header");
        assert_eq!(
            first_sentence(&h),
            "The swept-sphere physics engine: cells, BSP trees and collision."
        );
        assert!(shape_problems(&h).is_empty(), "{:?}", shape_problems(&h));
        assert!(dependency_problems(&physics(), &h, &ws(), &users()).is_empty());
    }

    #[test]
    fn a_leading_plain_comment_is_allowed_but_code_first_is_not() {
        let with_anchor = format!("// Ported from somewhere\n\n{GOOD}");
        assert!(header_of(&with_anchor).is_some());
        assert!(header_of("pub mod x;\n//! late\n").is_none());
        assert!(header_of("/// an item doc\npub mod x;\n").is_none());
        assert!(header_of("").is_none());
    }

    #[test]
    fn the_first_sentence_skips_dots_in_code_and_names() {
        let h = "The port of `Source/ACE.Common`, with shims. More.";
        assert_eq!(
            first_sentence(h),
            "The port of `Source/ACE.Common`, with shims."
        );
        let h = "A sentence with `a. b` code in it! Then more.";
        assert_eq!(first_sentence(h), "A sentence with `a. b` code in it!");
        // A first paragraph with no full stop is the sentence.
        assert_eq!(first_sentence("No stop here\n\nSecond."), "No stop here");
        // Wrapped lines join.
        assert_eq!(
            first_sentence("Wrapped\nover two lines. X"),
            "Wrapped over two lines."
        );
    }

    #[test]
    fn markdown_is_normalised_away_before_comparing() {
        assert_eq!(
            normalise("The **port** of `ACE.Common` and [`Foo`](crate::foo), [bar]."),
            "The port of ACE.Common and Foo, bar."
        );
        assert_eq!(normalise("a  \n  b"), "a b");
    }

    #[test]
    fn a_description_with_markdown_or_nothing_in_it_fails() {
        assert!(description_problem("Plain text.").is_none());
        assert!(description_problem("").is_some());
        assert!(description_problem("Has `code`.").is_some());
        assert!(description_problem("Has **bold**.").is_some());
    }

    #[test]
    fn a_header_missing_a_part_fails_the_shape_check() {
        let no_never = "What it is.\n\n**Depends on** x. **Used by** y.\n";
        assert_eq!(shape_problems(no_never).len(), 1);
        let no_depends = "What it is.\n\n**Must never** z.\n";
        assert_eq!(shape_problems(no_depends).len(), 1);
        let no_used = "What it is.\n\n**Depends on** x.\n\n**Must never** z.\n";
        assert_eq!(shape_problems(no_used).len(), 1);
        let used_own_para =
            "What it is.\n\n**Depends on** x.\n\n**Used by** y.\n\n**Must never** z.\n";
        assert!(shape_problems(used_own_para).is_empty());
        let depends_first = "**Depends on** x. **Used by** y.\n\n**Must never** z.\n";
        assert_eq!(shape_problems(depends_first).len(), 1);
        let never_first = "What.\n\n**Must never** z.\n\n**Depends on** x. **Used by** y.\n";
        assert_eq!(shape_problems(never_first).len(), 1);
    }

    #[test]
    fn a_dependency_statement_the_manifest_contradicts_fails() {
        // Names a crate it does not depend on.
        let h = GOOD.replace("(its tests read `dereth-dat`)", "and `dereth-render`");
        let h = header_of(&h).unwrap();
        let p = dependency_problems(&physics(), &h, &ws(), &users());
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("dereth-render"));

        // Leaves out a production dependency.
        let h = GOOD.replace("`dereth-primitives` (its", "nothing (its");
        let h = header_of(&h).unwrap();
        let p = dependency_problems(&physics(), &h, &ws(), &users());
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("does not name it"));

        // Names a user that does not depend on it.
        let h = GOOD.replace("(`dereth-client-runtime`)", "(`dereth-render`)");
        let h = header_of(&h).unwrap();
        let p = dependency_problems(&physics(), &h, &ws(), &users());
        assert_eq!(p.len(), 1, "{p:?}");
        assert!(p[0].contains("does not depend on it"));

        // A dev-dependency may be named but need not be.
        let h = header_of(&GOOD.replace(" (its tests read `dereth-dat`)", "")).unwrap();
        assert!(dependency_problems(&physics(), &h, &ws(), &users()).is_empty());
    }
}
