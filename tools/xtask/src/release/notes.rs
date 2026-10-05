//! A release's notes: the curated highlights of the product's `CHANGES.md`, then every commit
//! since the previous release that changed what the product ships, grouped under a few headings.
//!
//! `main` gets one commit per finished piece of work and its subject says what the product now
//! does, so the subjects are the changelog. A commit counts when it changes something the release
//! ships, and is neither a test nor a document:
//!
//! - **Dereth** (the client and the launcher): a file under `dereth/` or `core/`, or the web
//!   client's relay.
//! - **Empyrean** (the server): a file under `empyrean/`, or under one of the shared `core/`
//!   crates the server is built from when the commit changes nothing that only the client ships
//!   (a client change that touches a shared crate on the way is the client's).
//!
//! The other product's commits, CI's, the tools', test-only and documentation-only commits, the
//! releases' own version commits, and commits whose subject says they only restructure the code
//! ([`is_restructure`]) are left out. Each kept subject goes under one heading,
//! chosen from its words and then from where its files are; one the rules cannot place goes under
//! "Other changes" rather than a guess.
//!
//! `cargo xtask release-notes dereth|empyrean [--since <tag>] [--version <v>] [--out <file>]`
//! prints the notes (or writes them to `<file>`); each product's release workflow runs it on the
//! tag and puts the file at the start of the release's description.

use std::path::{Path, PathBuf};

use crate::package::version::Product;
use crate::package::{self, git, version};

/// Dereth's curated highlights, relative to the workspace root.
pub const CHANGES: &str = "dereth/CHANGES.md";

/// Empyrean's curated highlights, relative to the workspace root.
pub const EMPYREAN_CHANGES: &str = "empyrean/CHANGES.md";

/// `product`'s curated highlights, relative to the workspace root.
pub fn changes_file(product: Product) -> &'static str {
    match product {
        Product::Dereth => CHANGES,
        Product::Empyrean => EMPYREAN_CHANGES,
    }
}

/// The heading of the section that collects highlights until the next release.
pub const UNRELEASED: &str = "Unreleased";

/// One commit: its subject and the paths it changed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub subject: String,
    pub paths: Vec<String>,
    /// Every change to its shipped files is a comment or a blank line ([`comment_only`]).
    pub comment_only: bool,
}

/// A heading of the notes' "All changes", in the order they are printed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    Gameplay,
    GraphicsSound,
    Interface,
    Launcher,
    /// Empyrean's: the world's content, its spawns and the data it is built from.
    World,
    /// Empyrean's: configuration, updates, the databases and the importer.
    Operations,
    Fixes,
    Other,
}

impl Group {
    pub const ALL: [Self; 8] = [
        Self::Gameplay,
        Self::GraphicsSound,
        Self::Interface,
        Self::Launcher,
        Self::World,
        Self::Operations,
        Self::Fixes,
        Self::Other,
    ];

    pub fn heading(self) -> &'static str {
        match self {
            Self::Gameplay => "Gameplay",
            Self::GraphicsSound => "Graphics & sound",
            Self::Interface => "Interface",
            Self::Launcher => "Launcher",
            Self::World => "World and content",
            Self::Operations => "Operations",
            Self::Fixes => "Fixes",
            Self::Other => "Other changes",
        }
    }
}

/// Paths outside these are not part of what a Dereth release ships.
const SHIPPED: &[&str] = &["dereth/", "core/", "tools/web-relay/"];

/// The shared crates Empyrean is built from (its binaries' dependency tree); the rest of `core/`
/// is the client's alone.
const SERVER_CORE: &[&str] = &[
    "core/animation/",
    "core/assets/",
    "core/dat/",
    "core/physics/",
    "core/primitives/",
    "core/protocol/",
    "core/rules/",
    "core/transport/",
    "core/world-data/",
];

/// Whether `path` is a test, a fixture, a test kit or a document.
fn test_or_document(path: &str) -> bool {
    path.starts_with("dereth/testkit/")
        || path.starts_with("empyrean/testkit/")
        || path.contains("/tests/")
        || path.contains("/benches/")
        || path.contains("/fixtures/")
        || path.ends_with("/tests.rs")
        || path.ends_with("_tests.rs")
        || path.ends_with(".md")
}

/// Whether `path` is part of the shipped client or launcher: under a shipped folder, and neither a
/// test, a fixture, the test kit nor a document.
pub fn ships(path: &str) -> bool {
    SHIPPED.iter().any(|p| path.starts_with(p)) && !test_or_document(path)
}

/// Whether `path` is part of the shipped server: under `empyrean/` or a shared crate it is built
/// from, and neither a test, a fixture, the test kit nor a document.
pub fn server_ships(path: &str) -> bool {
    (path.starts_with("empyrean/") || SERVER_CORE.iter().any(|p| path.starts_with(p)))
        && !test_or_document(path)
}

/// Whether `path` is part of what `product` ships.
fn product_ships(product: Product, path: &str) -> bool {
    match product {
        Product::Dereth => ships(path),
        Product::Empyrean => server_ships(path),
    }
}

/// The paths of `commit` that `product`'s release ships. For Empyrean, a commit that changes
/// something only the client ships counts only for what it changes under `empyrean/`: its
/// shared-crate changes serve the client.
fn shipped_paths(product: Product, commit: &Commit) -> Vec<&str> {
    let shipped: Vec<&str> = commit
        .paths
        .iter()
        .map(String::as_str)
        .filter(|p| product_ships(product, p))
        .collect();
    if product == Product::Empyrean
        && !shipped.iter().any(|p| p.starts_with("empyrean/"))
        && commit.paths.iter().any(|p| ships(p) && !server_ships(p))
    {
        return Vec::new();
    }
    shipped
}

/// The heading a path's folder implies for `product`, when it implies one.
fn path_group(product: Product, path: &str) -> Option<Group> {
    match product {
        Product::Dereth => client_path_group(path),
        Product::Empyrean => server_path_group(path),
    }
}

/// The heading a server path's folder implies, when it implies one.
fn server_path_group(path: &str) -> Option<Group> {
    let under = |dirs: &[&str]| dirs.iter().any(|d| path.starts_with(d));
    if under(&[
        "empyrean/server/",
        "empyrean/import/",
        "empyrean/crates/store/",
        "empyrean/releases.toml",
    ]) {
        Some(Group::Operations)
    } else if under(&[
        "empyrean/crates/content/",
        "empyrean/crates/dat/",
        "empyrean/crates/tables/",
        "core/dat/",
        "core/world-data/",
    ]) {
        Some(Group::World)
    } else if under(&[
        "empyrean/crates/world/",
        "empyrean/crates/entity/",
        "core/physics/",
        "core/rules/",
    ]) {
        Some(Group::Gameplay)
    } else {
        None
    }
}

/// The heading a client path's folder implies, when it implies one.
fn client_path_group(path: &str) -> Option<Group> {
    let under = |dirs: &[&str]| dirs.iter().any(|d| path.starts_with(d));
    if under(&["dereth/launcher/"]) {
        Some(Group::Launcher)
    } else if under(&[
        "dereth/client/crates/render/",
        "dereth/client/crates/render-cpu/",
        "dereth/client/crates/world-render/",
        "core/audio/",
        "core/terrain/",
        "core/landscape/",
    ]) {
        Some(Group::GraphicsSound)
    } else if under(&[
        "dereth/client/crates/ui/",
        "dereth/client/crates/ui-screens/",
        "dereth/client/crates/input/",
        "dereth/client/crates/console/",
        "dereth/client/crates/clipboard/",
    ]) {
        Some(Group::Interface)
    } else if under(&["core/physics/", "core/rules/", "core/client-model/"]) {
        Some(Group::Gameplay)
    } else {
        None
    }
}

/// Word stems that place a subject under a heading. A stem of four letters or more matches any
/// word it begins; a shorter one matches the word itself or its plural.
const WORDS: &[(Group, &[&str])] = &[
    (
        Group::GraphicsSound,
        &[
            "draw", "drawn", "render", "particle", "light", "shadow", "texture", "mesh", "terrain",
            "sky", "water", "fog", "colour", "color", "sound", "audio", "music", "volume",
            "graphic", "shader", "mip", "sampler", "vulkan", "gpu", "pixel", "flame", "smoke",
        ],
    ),
    (
        Group::Interface,
        &[
            "key", "keyboard", "mouse", "cursor", "click", "window", "panel", "chat", "menu",
            "button", "ui", "tooltip", "hotkey", "quickbar", "slider", "font", "dialog", "toolbar",
            "radar", "screen",
        ],
    ),
    (
        Group::Gameplay,
        &[
            "combat",
            "attack",
            "spell",
            "magic",
            "melee",
            "missile",
            "move",
            "jump",
            "walk",
            "physics",
            "collision",
            "inventory",
            "item",
            "vendor",
            "shop",
            "buy",
            "sell",
            "trade",
            "quest",
            "fellowship",
            "allegiance",
            "skill",
            "vital",
            "health",
            "stamina",
            "corpse",
            "loot",
            "portal",
            "teleport",
            "emote",
        ],
    ),
    (Group::Launcher, &["launcher"]),
];

/// Word stems that place a server commit's subject under a heading, matched as [`WORDS`] are.
const SERVER_WORDS: &[(Group, &[&str])] = &[
    (
        Group::Gameplay,
        &[
            "combat",
            "attack",
            "weapon",
            "wield",
            "spell",
            "magic",
            "melee",
            "missile",
            "move",
            "jump",
            "physics",
            "collision",
            "inventory",
            "item",
            "vendor",
            "shop",
            "buy",
            "sell",
            "trade",
            "fellowship",
            "allegiance",
            "skill",
            "vital",
            "health",
            "stamina",
            "corpse",
            "portal",
            "teleport",
            "death",
        ],
    ),
    (
        Group::World,
        &[
            "landblock",
            "spawn",
            "generator",
            "monster",
            "creature",
            "npc",
            "weenie",
            "content",
            "dungeon",
            "dat",
            "quest",
            "recipe",
            "loot",
            "emote",
            "treasure",
        ],
    ),
    (
        Group::Operations,
        &[
            "config",
            "setting",
            "update",
            "upgrade",
            "database",
            "migration",
            "backup",
            "sqlite",
            "operator",
            "admin",
            "install",
            "setup",
            "release",
            "log",
            "websocket",
            "status",
            "shutdown",
            "restart",
            "startup",
            "importer",
            "fetch",
        ],
    ),
];

/// Words that mark a subject as a fix whatever it is about.
const FIX_WORDS: &[&str] = &[
    "fix",
    "fixes",
    "fixed",
    "crash",
    "crashes",
    "regression",
    "hang",
];

/// The subject's words, lower-cased, split at anything that is not a letter or a digit.
fn words(subject: &str) -> Vec<String> {
    subject
        .to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(str::to_owned)
        .collect()
}

fn stem_matches(stem: &str, word: &str) -> bool {
    if stem.len() >= 4 {
        word.starts_with(stem)
    } else {
        word == stem || word.strip_suffix('s') == Some(stem)
    }
}

/// The headings the subject's words name for `product`.
fn word_groups(product: Product, subject: &str) -> Vec<Group> {
    let words = words(subject);
    let table = match product {
        Product::Dereth => WORDS,
        Product::Empyrean => SERVER_WORDS,
    };
    let mut out: Vec<Group> = table
        .iter()
        .filter(|(_, stems)| {
            words
                .iter()
                .any(|w| stems.iter().any(|s| stem_matches(s, w)))
        })
        .map(|(g, _)| *g)
        .collect();
    out.sort();
    out
}

fn is_fix(subject: &str) -> bool {
    let words = words(subject);
    words.iter().any(|w| FIX_WORDS.contains(&w.as_str()))
        || words.windows(2).any(|p| p == ["no", "longer"])
}

/// The first words of a subject that restructures the code without changing what it does: a
/// shared copy, a moved or split file, a merged duplicate.
const RESTRUCTURE_LEADS: &[&str] = &[
    "Share",
    "Move",
    "Separate",
    "Split",
    "Extract",
    "Consolidate",
    "Integrate",
    "Organize",
    "Organise",
    "Group",
    "Reuse",
    "Unify",
    "Refactor",
    "Rename",
];

/// Words that mark a subject about the code itself (its dependencies, documentation, citations
/// or naming) rather than about what the product does.
const RESTRUCTURE_WORDS: &[&str] = &[
    "dependencies",
    "documentation",
    "citations",
    "consistent",
    "consistently",
];

/// Phrases that mark a subject about the code's own plumbing or its test harness.
const RESTRUCTURE_PHRASES: &[&str] = &[
    "dispatch",
    "adapter",
    "headless",
    " lints",
    "with each instance",
    "through shared",
    "shared display",
    "protocol features",
    "dead-code",
    " panic ",
    "formatting after",
    "upgrade declaration",
];

/// Whether `subject` describes a change to the code alone: restructuring, removing dead code,
/// naming or documentation. Such a commit changes nothing a player or an operator sees, so the
/// notes leave it out; a subject that says what the product now does is kept.
pub fn is_restructure(subject: &str) -> bool {
    let first = subject.split_whitespace().next().unwrap_or("");
    if RESTRUCTURE_LEADS.contains(&first) {
        return true;
    }
    let lower = subject.to_ascii_lowercase();
    if [
        "remove unused ",
        "remove the unused ",
        "remove inert ",
        "use owning ",
    ]
    .iter()
    .any(|lead| lower.starts_with(lead))
    {
        return true;
    }
    words(subject)
        .iter()
        .any(|w| RESTRUCTURE_WORDS.contains(&w.as_str()))
        || RESTRUCTURE_PHRASES.iter().any(|p| lower.contains(p))
}

/// A release's own version commit: `Dereth 0.2.0`, `Empyrean 0.1.1`, or either followed by `: ...`.
pub fn is_release_commit(subject: &str) -> bool {
    let head = subject.split(':').next().unwrap_or(subject);
    let mut parts = head.split_whitespace();
    matches!(
        (parts.next(), parts.next(), parts.next()),
        (Some("Dereth" | "Empyrean"), Some(v), None) if version::parse_release_version(v).is_ok()
    )
}

/// The heading `commit` goes under in `product`'s notes, or `None` when it is left out of them.
///
/// A commit that changes only the launcher is the launcher's. Otherwise a subject that says it
/// fixes something is a fix; then the one heading the subject's words name; then, when the words
/// name none or several, the one heading the commit's folders imply (among the words' headings,
/// when there are any); and "Other changes" when there is still no single answer.
pub fn group(product: Product, commit: &Commit) -> Option<Group> {
    if is_release_commit(&commit.subject) || is_restructure(&commit.subject) {
        return None;
    }
    let shipped = shipped_paths(product, commit);
    if shipped.is_empty() || commit.comment_only {
        return None;
    }
    if shipped
        .iter()
        .all(|p| path_group(product, p) == Some(Group::Launcher))
    {
        return Some(Group::Launcher);
    }
    if is_fix(&commit.subject) {
        return Some(Group::Fixes);
    }
    let by_words = word_groups(product, &commit.subject);
    if let [only] = by_words.as_slice() {
        return Some(*only);
    }
    let mut by_paths: Vec<Group> = shipped
        .iter()
        .filter_map(|p| path_group(product, p))
        .filter(|g| by_words.is_empty() || by_words.contains(g))
        .collect();
    by_paths.sort();
    by_paths.dedup();
    Some(match by_paths.as_slice() {
        [only] => *only,
        _ => Group::Other,
    })
}

/// A subject as a line of the notes: a leading lower-case area label (`client: `, `render: `) is
/// dropped and the first letter capitalised.
pub fn display_subject(subject: &str) -> String {
    let s = subject.trim();
    let s = match s.split_once(": ") {
        Some((label, rest))
            if !rest.is_empty()
                && label.len() <= 30
                && label.starts_with(|c: char| c.is_ascii_lowercase())
                && label
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || " -_".contains(c)) =>
        {
            rest
        }
        _ => s,
    };
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// The kept commits' lines under each heading that has any, headings in order and lines in the
/// order given (oldest first).
pub fn grouped(product: Product, commits: &[Commit]) -> Vec<(Group, Vec<String>)> {
    Group::ALL
        .iter()
        .filter_map(|&g| {
            let lines: Vec<String> = commits
                .iter()
                .filter(|c| group(product, c) == Some(g))
                .map(|c| display_subject(&c.subject))
                .collect();
            (!lines.is_empty()).then_some((g, lines))
        })
        .collect()
}

/// The text of a `## ` heading, when `line` is one.
fn heading(line: &str) -> Option<&str> {
    line.strip_prefix("## ").map(str::trim)
}

/// Whether a section heading names `name`: `Unreleased`, or a version followed by its date.
fn names(heading: &str, name: &str) -> bool {
    heading.split_whitespace().next() == Some(name)
}

/// The body of `CHANGES.md`'s section named `name` (trimmed), or `None` when it has none.
pub fn section(changes: &str, name: &str) -> Option<String> {
    let mut lines = changes.lines();
    lines.find(|l| heading(l).is_some_and(|h| names(h, name)))?;
    let body: Vec<&str> = lines.take_while(|l| heading(l).is_none()).collect();
    Some(body.join("\n").trim().to_owned())
}

/// `CHANGES.md` with its Unreleased section turned into release `version`'s, dated `date`, and a
/// new empty Unreleased section above it.
pub fn stamp(changes: &str, version: &str, date: &str) -> Result<String, String> {
    if section(changes, version).is_some() {
        return Err(format!("CHANGES.md already has a section for {version}"));
    }
    let mut out = String::with_capacity(changes.len() + 32);
    let mut stamped = false;
    for line in changes.split_inclusive('\n') {
        if !stamped && heading(line).is_some_and(|h| h == UNRELEASED) {
            out.push_str(&format!("## {UNRELEASED}\n\n## {version} ({date})\n"));
            stamped = true;
        } else {
            out.push_str(line);
        }
    }
    if stamped {
        Ok(out)
    } else {
        Err(format!("CHANGES.md has no `## {UNRELEASED}` section"))
    }
}

/// Today's date (UTC) as `YYYY-MM-DD`.
pub fn today() -> String {
    let secs = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    date_of_day(i64::try_from(secs / 86_400).unwrap_or(0))
}

/// The civil date `days` days after 1970-01-01, as `YYYY-MM-DD`.
pub fn date_of_day(days: i64) -> String {
    // Days since 0000-03-01, counted in 400-year eras of 146097 days.
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = yoe + era * 400 + i64::from(month <= 2);
    format!("{year:04}-{month:02}-{day:02}")
}

/// `text` with each wrapped paragraph or list item on one line: GitHub shows every line break of a
/// release's description, so a line wrapped in `CHANGES.md` would break mid-sentence there. A line
/// joins the one before it unless either is blank or it starts a list item or a heading.
pub fn unwrap_lines(text: &str) -> String {
    let starts_block = |l: &str| {
        let l = l.trim_start();
        l.starts_with("- ")
            || l.starts_with("* ")
            || l.starts_with('#')
            || l.split_once(". ")
                .is_some_and(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
    };
    let mut out: Vec<String> = Vec::new();
    for line in text.lines() {
        let joins = !line.trim().is_empty()
            && !starts_block(line)
            && out.last().is_some_and(|prev| !prev.trim().is_empty());
        match out.last_mut() {
            Some(prev) if joins => {
                prev.push(' ');
                prev.push_str(line.trim());
            }
            _ => out.push(line.trim_end().to_owned()),
        }
    }
    out.join("\n")
}

/// `product`'s notes: the highlights (when there are any), then all changes, then the compare
/// link.
pub fn render(
    product: Product,
    highlights: Option<&str>,
    groups: &[(Group, Vec<String>)],
    since: &str,
    compare: Option<&str>,
) -> String {
    let mut out = String::new();
    if let Some(h) = highlights.map(str::trim).filter(|h| !h.is_empty()) {
        out.push_str("## Highlights\n\n");
        out.push_str(&unwrap_lines(h));
        out.push_str("\n\n");
    }
    out.push_str("## All changes\n\n");
    if groups.is_empty() {
        let what = match product {
            Product::Dereth => "the client or the launcher",
            Product::Empyrean => "the server",
        };
        out.push_str(&format!("No change to {what} since {since}.\n\n"));
    }
    for (g, lines) in groups {
        out.push_str(&format!("### {}\n\n", g.heading()));
        for l in lines {
            out.push_str(&format!("- {l}\n"));
        }
        out.push('\n');
    }
    if let Some(url) = compare {
        out.push_str(&format!("**Full changes:** {url}\n"));
    }
    out
}

/// Whether a unified diff (`git show -U0`) changes nothing but comments: every added or removed
/// line of a Rust file is blank or a `//` comment (`//`, `///`, `//!`), and no other file
/// changes. A diff that changes no line at all (a rename, a binary file) is not comment-only.
pub fn comment_only(diff: &str) -> bool {
    let mut rust = false;
    let mut changed = false;
    for line in diff.lines() {
        if let Some(header) = line.strip_prefix("diff --git ") {
            rust = header.ends_with(".rs");
            continue;
        }
        if line.starts_with("Binary files ") {
            return false;
        }
        if line.starts_with("+++") || line.starts_with("---") {
            continue;
        }
        let Some(body) = line.strip_prefix('+').or_else(|| line.strip_prefix('-')) else {
            continue;
        };
        if !rust {
            return false;
        }
        let body = body.trim();
        if !(body.is_empty() || body.starts_with("//")) {
            return false;
        }
        changed = true;
    }
    changed
}

/// The commits in `range` (oldest first), merges left out; whether each is comment-only is read
/// over the files `product` ships.
fn commits(ws: &Path, product: Product, range: &str) -> Result<Vec<Commit>, String> {
    let log = git(
        ws,
        &[
            "-c",
            "core.quotePath=false",
            "log",
            "--no-merges",
            "--reverse",
            "--format=%x1e%H%x1f%s",
            "--name-only",
            range,
        ],
    )?;
    log.split('\u{1e}')
        .filter(|c| !c.trim().is_empty())
        .map(|c| {
            let mut lines = c.lines();
            let (hash, subject) = lines
                .next()
                .unwrap_or_default()
                .split_once('\u{1f}')
                .unwrap_or_default();
            let paths: Vec<String> = lines
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .map(str::to_owned)
                .collect();
            let shipped: Vec<&str> = paths
                .iter()
                .map(String::as_str)
                .filter(|p| product_ships(product, p))
                .collect();
            let comment_only = if shipped.is_empty() {
                false
            } else {
                let mut args = vec![
                    "-c",
                    "core.quotePath=false",
                    "show",
                    "--format=",
                    "-U0",
                    hash,
                    "--",
                ];
                args.extend(shipped);
                comment_only(&git(ws, &args)?)
            };
            Ok(Commit {
                subject: subject.trim().to_owned(),
                paths,
                comment_only,
            })
        })
        .collect()
}

/// What `release-notes` was asked for.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Request {
    /// The previous release's tag; by default the product's newest final (not pre-release) tag
    /// (`dereth-v*`, `empyrean-v*`) before the notes' end.
    pub since: Option<String>,
    /// The release the notes are for: when its tag exists, the tag ends the range and
    /// `CHANGES.md` is read as the tag has it; its section is the highlights when there is one,
    /// the Unreleased section otherwise.
    pub version: Option<String>,
    /// Where to write the notes instead of printing them.
    pub out: Option<PathBuf>,
    /// The repository the compare link names; by default the one the build environment names
    /// (`DERETH_BUILD_SOURCE_URL`, `EMPYREAN_BUILD_SOURCE_URL`), else the public repository.
    pub source_url: Option<String>,
}

/// `product`'s notes for `request`, read from the repository at `ws`.
pub fn notes(ws: &Path, product: Product, request: &Request) -> Result<String, String> {
    let changes_file = changes_file(product);
    let tag = request
        .version
        .as_deref()
        .map(|v| {
            version::parse_release_version(v)?;
            Ok::<_, String>(product.tag_for(v))
        })
        .transpose()?;
    let tagged = tag.as_deref().filter(|t| {
        git(
            ws,
            &[
                "rev-parse",
                "--verify",
                "--quiet",
                &format!("refs/tags/{t}"),
            ],
        )
        .is_ok()
    });
    let end = tagged.unwrap_or("HEAD");
    let since = match &request.since {
        Some(s) => s.clone(),
        None => git(
            ws,
            &[
                "describe",
                "--tags",
                "--abbrev=0",
                "--match",
                &format!("{}*", product.tag_prefix()),
                // A pre-release's tag carries a hyphen after the version: the notes of a final
                // release cover everything since the previous final one.
                "--exclude",
                &format!("{}*-*", product.tag_prefix()),
                &format!("{end}^"),
            ],
        )
        .map_err(|e| {
            format!(
                "no earlier final {}* tag; name one with --since ({e})",
                product.tag_prefix()
            )
        })?,
    };
    let commits = commits(ws, product, &format!("{since}..{end}"))?;
    // A tagged release's highlights are the file as the tag has it (empty when it has none);
    // otherwise the working tree's.
    let changes = match tagged {
        Some(t) => git(ws, &["show", &format!("{t}:{changes_file}")]).unwrap_or_default(),
        None => std::fs::read_to_string(ws.join(changes_file))
            .map_err(|e| format!("reading {changes_file}: {e}"))?,
    };
    let highlights = request
        .version
        .as_deref()
        .and_then(|v| section(&changes, v))
        .or_else(|| section(&changes, UNRELEASED));
    let source_env = match product {
        Product::Dereth => package::dereth::SOURCE_URL_ENV,
        Product::Empyrean => "EMPYREAN_BUILD_SOURCE_URL",
    };
    let source = request
        .source_url
        .clone()
        .or_else(|| std::env::var(source_env).ok())
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| package::DEFAULT_SOURCE_URL.to_owned());
    let compare = format!(
        "{}/compare/{since}...{}",
        source.trim_end_matches('/'),
        tag.as_deref().unwrap_or("main")
    );
    Ok(render(
        product,
        highlights.as_deref(),
        &grouped(product, &commits),
        &since,
        Some(&compare),
    ))
}

const USAGE: &str =
    "usage: cargo xtask release-notes dereth|empyrean [--since <tag>] [--version <version>] \
                     [--out <file>]";

fn parse(args: &[String]) -> Result<(Product, Request), String> {
    let mut it = args.iter();
    let product = it
        .next()
        .and_then(|a| Product::from_command_name(a))
        .ok_or_else(|| USAGE.to_owned())?;
    let mut request = Request::default();
    while let Some(a) = it.next() {
        let mut value = || it.next().cloned().ok_or_else(|| USAGE.to_owned());
        match a.as_str() {
            "--since" => request.since = Some(value()?),
            "--version" => request.version = Some(value()?),
            "--out" => request.out = Some(PathBuf::from(value()?)),
            other => return Err(format!("unexpected argument `{other}`\n{USAGE}")),
        }
    }
    Ok((product, request))
}

/// `cargo xtask release-notes ...`.
pub fn release_notes(args: &[String]) -> i32 {
    let (product, request) = match parse(args) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("{e}");
            return 2;
        }
    };
    let ws = crate::util::workspace_root();
    let text = match notes(&ws, product, &request) {
        Ok(t) => t,
        Err(e) => {
            eprintln!("release-notes: {e}");
            return 1;
        }
    };
    match &request.out {
        Some(out) => {
            if let Some(dir) = out.parent().filter(|d| !d.as_os_str().is_empty()) {
                if let Err(e) = std::fs::create_dir_all(dir) {
                    eprintln!("release-notes: creating {}: {e}", dir.display());
                    return 1;
                }
            }
            if let Err(e) = std::fs::write(out, &text) {
                eprintln!("release-notes: writing {}: {e}", out.display());
                return 1;
            }
            println!("wrote {}", out.display());
        }
        None => print!("{text}"),
    }
    0
}

#[cfg(test)]
mod tests;
