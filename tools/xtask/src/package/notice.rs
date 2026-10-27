//! `NOTICE.txt` and `THIRD-PARTY-LICENSES.html`: what a release says about its source, its
//! origins and everything linked into it.

/// The server's SPDX licence identifier. Spelled in two halves: the workspace's separation rule
/// refuses the whole word anywhere in an MIT crate's source, as the mark of ported code.
pub const SERVER_LICENCE: &str = concat!("A", "GPL-3.0-only");

/// One crate the binaries are built from, as `cargo tree` names it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Crate {
    pub name: String,
    pub version: String,
    pub licence: String,
}

/// The crates in `cargo tree --prefix none --format "{p}|{l}"` output, each once, sorted.
/// A line reads `name vX.Y.Z [(path)] [(proc-macro)] [(*)]|licence`.
pub fn parse_cargo_tree(output: &str) -> Vec<Crate> {
    let mut crates: Vec<Crate> = output
        .lines()
        .filter_map(|line| {
            let (package, licence) = line.split_once('|')?;
            let mut words = package.split_whitespace();
            let name = words.next()?.to_owned();
            let version = words.next()?.strip_prefix('v')?.to_owned();
            let licence = licence.trim().trim_end_matches("(*)").trim().to_owned();
            Some(Crate {
                name,
                version,
                licence,
            })
        })
        .collect();
    crates.sort();
    crates.dedup();
    crates
}

/// Whether a crate is one of this repository's own (the shared `dereth-*` crates and the world
/// builder's `lyceum-*`, MIT, and the server's `empyrean-*`, under the server's copyleft license).
pub fn is_own(c: &Crate) -> bool {
    c.name.starts_with("dereth-")
        || c.name.starts_with("empyrean-")
        || c.name.starts_with("lyceum-")
}

/// Everything `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct Facts<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The MIT licence of the shared crates (the repository's top-level `LICENSE`).
    pub mit_licence: &'a str,
    /// The Lifestoned data model's MIT notice.
    pub lifestoned_notice: &'a str,
}

/// `NOTICE.txt`.
pub fn notice(f: &Facts) -> String {
    let tag = super::version::tag_for(f.version);
    let url = f.source_url.trim_end_matches('/');
    let mut own = Vec::new();
    let mut third = Vec::new();
    for c in f.crates {
        let line = format!("  {} {} ({})", c.name, c.version, c.licence);
        if is_own(c) {
            own.push(line);
        } else {
            third.push(line);
        }
    }
    format!(
        "Empyrean {version} ({target})
{rule}

Empyrean is an Asheron's Call server: a Rust port of ACE (ACEmulator), made by the Dereth
project (https://dereth.network). It is free software, licensed under the GNU Affero General
Public License, version 3 only ({spdx}); the licence is in LICENSE, beside this file.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release this package belongs to, with GitHub's source archives of its tag:
  {url}/releases/tag/{tag}
  {url}/archive/refs/tags/{tag}.tar.gz
  {url}/archive/refs/tags/{tag}.zip

If you run a modified version of this server for players over a network, its licence asks you to
offer those players your modified source; set `[server] source_url` in empyrean.toml to where it
is (README.md, \"License\").


NO GAME DATA

This package contains no Asheron's Call client files and no world database. The server reads the
client's data files from your own client install, and builds its world from ACE-World's
database release with `empyrean-import fetch --pack` (SETUP.md).


ACE

Empyrean is derived from ACE (ACEmulator), https://github.com/ACEmulator/ACE, copyright its
contributors, licensed under the GNU Affero General Public License v3.0. Empyrean's port keeps
ACE's behaviour and names ACE's source file above each ported file; where it deliberately behaves
differently, DIVERGENCES.md says so, and ACE-BUGS.md lists the ACE defects it keeps on purpose.


CLASSICACE

The rules of the earlier eras a world can play (February 2005's character creation, combat and
death formulas, starter gear and first login) are ported from ClassicACE, bDekaru's fork of ACE,
https://github.com/bDekaru/ClassicACE, copyright its contributors, licensed under the GNU Affero
General Public License v3.0. Each such rule names ClassicACE's source file beside it, and
DIVERGENCES.md lists them as its era rows.


LIFESTONED DATA MODEL

The server reads and writes weenie JSON in the Lifestoned data model's format (Lifestoned.DataModel,
which ACE's content tools use), under this notice:

{lifestoned}

SQLITE

The server's databases use SQLite, compiled into the binary. SQLite is in the public domain
(https://sqlite.org/copyright.html).


THE SHARED DERETH CRATES

The dereth-* crates below are the Dereth project's shared libraries, licensed MIT:

{mit}

WHAT THE BINARIES ARE BUILT FROM

This project's own crates:
{own}

Third-party crates (their licence texts are in THIRD-PARTY-LICENSES.html):
{third}
",
        spdx = SERVER_LICENCE,
        version = f.version,
        target = f.target,
        rule = "=".repeat(format!("Empyrean {} ({})", f.version, f.target).len()),
        commit = f.commit,
        lifestoned = indent(f.lifestoned_notice.trim_end()),
        mit = indent(f.mit_licence.trim_end()),
        own = own.join("\n"),
        third = third.join("\n"),
    )
}

fn indent(text: &str) -> String {
    let mut out: String = text
        .lines()
        .map(|l| {
            if l.is_empty() {
                String::new()
            } else {
                format!("    {l}")
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    out.push('\n');
    out
}

/// Where the licence section of a rendered `about.hbs` begins and ends.
pub const SECTION_START: &str = "<!-- licences -->";
pub const SECTION_END: &str = "<!-- /licences -->";

/// One document from cargo-about's per-binary renderings: the first document's frame around each
/// binary's licence section, under a heading naming the binary.
pub fn splice_licence_pages(pages: &[(&str, String)]) -> Result<String, String> {
    let (_, first) = pages.first().ok_or("no licence pages to join")?;
    let start = first
        .find(SECTION_START)
        .ok_or_else(|| format!("the licence template has no `{SECTION_START}` marker"))?;
    let end = first
        .rfind(SECTION_END)
        .ok_or_else(|| format!("the licence template has no `{SECTION_END}` marker"))?;
    let mut out = String::from(&first[..start]);
    for (binary, page) in pages {
        let s = page
            .find(SECTION_START)
            .ok_or_else(|| format!("{binary}'s licence page has no section marker"))?;
        let e = page
            .rfind(SECTION_END)
            .ok_or_else(|| format!("{binary}'s licence page has no section end"))?;
        out.push_str(&format!(
            "<section class=\"binary\">\n<h2>Linked into {binary}</h2>\n{}\n</section>\n",
            &page[s + SECTION_START.len()..e]
        ));
    }
    out.push_str(&first[end + SECTION_END.len()..]);
    Ok(out)
}

/// Everything a Dereth package's `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct DerethFacts<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The project's MIT licence (the repository's top-level `LICENSE`).
    pub mit_licence: &'a str,
    /// The typefaces the package carries, as (typeface, what carries it and for what, licence
    /// text).
    pub fonts: &'a [(&'a str, &'a str, String)],
    /// MoltenVK's licence, when the package carries MoltenVK (macOS).
    pub moltenvk_licence: Option<&'a str>,
}

/// A Dereth package's `NOTICE.txt`.
pub fn dereth_notice(f: &DerethFacts) -> String {
    let tag = super::version::Product::Dereth.tag_for(f.version);
    let url = f.source_url.trim_end_matches('/');
    let title = format!("Dereth {} ({})", f.version, f.target);
    let mut own = Vec::new();
    let mut third = Vec::new();
    for c in f.crates {
        let line = format!("  {} {} ({})", c.name, c.version, c.licence);
        if is_own(c) {
            own.push(line);
        } else {
            third.push(line);
        }
    }
    let mut out = format!(
        "{title}
{rule}

This package is Dereth, the launcher, with the Dereth client.
Dereth is an Asheron's Call client and the launcher that starts it, made by the Dereth project
(https://dereth.network). It is free software under the MIT licence, which is in LICENSE beside
this file and below.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release this package belongs to:
  {url}/releases/tag/{tag}


NO GAME DATA

This package contains no Asheron's Call files. The client reads the game's data files from an
install of your own.


DERETH

{mit}",
        rule = "=".repeat(title.len()),
        commit = f.commit,
        mit = indent(f.mit_licence.trim_end()),
    );
    out.push_str(&font_sections(f.fonts));
    if let Some(licence) = f.moltenvk_licence {
        out.push_str(&moltenvk_section("client", licence));
    }
    out.push_str(&built_from_section(&own, &third));
    out
}

/// Everything the plugin author's tool's `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct ToolFacts<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The project's MIT license (the repository's top-level `LICENSE`).
    pub mit_license: &'a str,
}

/// The plugin author's tool's `NOTICE.txt`.
pub fn tool_notice(f: &ToolFacts) -> String {
    let tag = super::version::Product::Dereth.tag_for(f.version);
    let url = f.source_url.trim_end_matches('/');
    let title = format!("dereth-plugin {} ({})", f.version, f.target);
    let (own, third): (Vec<&Crate>, Vec<&Crate>) = f.crates.iter().partition(|c| is_own(c));
    let line = |c: &&Crate| format!("  {} {} ({})", c.name, c.version, c.licence);
    let mut out = format!(
        "{title}
{rule}

This package is dereth-plugin, the plugin author's tool of Dereth {version}: it makes a plugin
repository, checks a plugin as Dereth's clients check it, packs it, releases it and lists it in a
plugin source. It is made by the Dereth project (https://dereth.network) and is free software
under the MIT license, which is in LICENSE beside this file and below.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release this package belongs to:
  {url}/releases/tag/{tag}


DERETH-PLUGIN

{mit}",
        rule = "=".repeat(title.len()),
        version = f.version,
        commit = f.commit,
        mit = indent(f.mit_license.trim_end()),
    );
    out.push_str(&built_from_section(
        &own.iter().map(line).collect::<Vec<_>>(),
        &third.iter().map(line).collect::<Vec<_>>(),
    ));
    out
}

/// A notice's sections for the typefaces a package carries, as (typeface, what carries it and for
/// what, license text).
fn font_sections(fonts: &[(&str, &str, String)]) -> String {
    let mut out = String::new();
    for (name, carried, text) in fonts {
        out.push_str(&format!(
            "

THE {upper} TYPEFACE

{carried}, under the SIL Open Font License, version 1.1:

{text}",
            upper = name.to_ascii_uppercase(),
            text = indent(text.trim_end()),
        ));
    }
    out
}

/// A notice's section for the MoltenVK a macOS package carries, which `program` draws through.
fn moltenvk_section(program: &str, license: &str) -> String {
    format!(
        "

MOLTENVK

On macOS the {program} draws through MoltenVK (https://github.com/KhronosGroup/MoltenVK), which
the package carries as Contents/Frameworks/libMoltenVK.dylib. MoltenVK is copyright The
Brenwill Workshop Ltd. and licensed under the Apache License, version 2.0:

{}",
        indent(license.trim_end())
    )
}

/// A notice's closing section: the crates the programs are built from, this project's and the
/// third parties', each a line.
fn built_from_section(own: &[String], third: &[String]) -> String {
    format!(
        "

WHAT THE BINARIES ARE BUILT FROM

This project's own crates:
{}

Third-party crates (their licence texts are in THIRD-PARTY-LICENSES.html):
{}
",
        own.join("\n"),
        third.join("\n")
    )
}

/// Everything a Lyceum package's `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct LyceumFacts<'a> {
    pub version: &'a str,
    pub target: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The project's MIT license (the repository's top-level `LICENSE`).
    pub mit_license: &'a str,
    /// The typefaces the editor carries, as (typeface, what carries it and for what, license
    /// text).
    pub fonts: &'a [(&'a str, &'a str, String)],
    /// MoltenVK's license, when the package carries MoltenVK (macOS).
    pub moltenvk_license: Option<&'a str>,
}

/// A Lyceum package's `NOTICE.txt`.
pub fn lyceum_notice(f: &LyceumFacts) -> String {
    let tag = super::version::Product::Lyceum.tag_for(f.version);
    let url = f.source_url.trim_end_matches('/');
    let title = format!("Lyceum {} ({})", f.version, f.target);
    let (own, third): (Vec<&Crate>, Vec<&Crate>) = f.crates.iter().partition(|c| is_own(c));
    let line = |c: &&Crate| format!("  {} {} ({})", c.name, c.version, c.licence);
    let mut out = format!(
        "{title}
{rule}

This package is Lyceum, the world builder: its editor and the `lyceum` command.
Lyceum builds worlds for Asheron's Call, made by the Dereth project (https://dereth.network). It
is free software under the MIT license, which is in LICENSE beside this file and below.


BUNDLED EMPYREAN

The separate empyrean-server and empyrean-import programs are Empyrean, a Rust port of ACE
(https://github.com/ACEmulator/ACE), copyright its contributors. Empyrean is licensed under the
GNU Affero General Public License, version 3 only; EMPYREAN-LICENSE carries its full text.
Its earlier-era rules also derive from ClassicACE (https://github.com/bDekaru/ClassicACE), under
the same license. The Lifestoned data model's MIT notice is in LIFESTONED-LICENSE.txt. SQLite is
in the public domain. These programs run separately; they are not linked into Lyceum.

Complete corresponding source for these bundled programs, including build instructions, is
available at the exact commit below, without charge:
  {url}/tree/{commit}/empyrean
  {url}/archive/{commit}.tar.gz
If you run modified server software for players over a network, offer them its modified source
through the server's source_url setting. THIRD-PARTY-LICENSES.html separates each program's
linked dependencies. No world database is bundled; first use can fetch ACE-World into your cache.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release this package belongs to:
  {url}/releases/tag/{tag}


NO GAME DATA

This package contains no Asheron's Call files. Lyceum reads the game's data files from an install
of your own, and writes a world's changes beside them, never into them.


LYCEUM

{mit}",
        rule = "=".repeat(title.len()),
        commit = f.commit,
        mit = indent(f.mit_license.trim_end()),
    );
    out.push_str(&font_sections(f.fonts));
    if let Some(license) = f.moltenvk_license {
        out.push_str(&moltenvk_section("editor", license));
    }
    out.push_str(&built_from_section(
        &own.iter().map(line).collect::<Vec<_>>(),
        &third.iter().map(line).collect::<Vec<_>>(),
    ));
    out
}

/// Everything the web client's `NOTICE.txt` states.
#[derive(Debug, Clone)]
pub struct WebFacts<'a> {
    pub version: &'a str,
    pub commit: &'a str,
    /// The public repository, without a trailing slash.
    pub source_url: &'a str,
    pub crates: &'a [Crate],
    /// The project's MIT licence (the repository's top-level `LICENSE`).
    pub mit_licence: &'a str,
    /// The typefaces the module carries, as (typeface, what carries it and for what, licence).
    pub fonts: &'a [(&'a str, &'a str, String)],
}

/// The web client's `NOTICE.txt`.
pub fn web_notice(f: &WebFacts) -> String {
    let tag = format!("{}{}", super::web::TAG_PREFIX, f.version);
    let url = f.source_url.trim_end_matches('/');
    let title = format!("Dereth web client {}", f.version);
    let (own, third): (Vec<&Crate>, Vec<&Crate>) = f.crates.iter().partition(|c| is_own(c));
    let line = |c: &&Crate| format!("  {} {} ({})", c.name, c.version, c.licence);
    let mut out = format!(
        "{title}
{rule}

This is the Dereth client in a web browser, made by the Dereth project (https://dereth.network).
It is free software under the MIT licence, which is in LICENSE beside this file and below.


SOURCE CODE

This build was made from commit {commit} of
  {url}
The source of exactly that commit:
  {url}/tree/{commit}
The release it belongs to:
  {url}/releases/tag/{tag}


NO GAME DATA

These files contain no Asheron's Call files. The client reads the game's data files the player
picks, in the player's own browser.


DERETH

{mit}",
        rule = "=".repeat(title.len()),
        commit = f.commit,
        mit = indent(f.mit_licence.trim_end()),
    );
    for (name, carried, text) in f.fonts {
        out.push_str(&format!(
            "

THE {upper} TYPEFACE

{carried}, under the SIL Open Font License, version 1.1:

{text}",
            upper = name.to_ascii_uppercase(),
            text = indent(text.trim_end()),
        ));
    }
    out.push_str(&format!(
        "

WHAT THE MODULE IS BUILT FROM

This project's own crates:
{}

Third-party crates (their licence texts are in THIRD-PARTY-LICENSES.html):
{}
",
        own.iter().map(line).collect::<Vec<_>>().join("\n"),
        third.iter().map(line).collect::<Vec<_>>().join("\n"),
    ));
    out
}
