//! `cargo xtask release network-hub|network-ace <version> [--push] [--remote <name>]`: cut a
//! release of the Dereth Network's hub or of its ACE mod, with the steps the other releases take.
//!
//! - **The hub** carries its version in `network/hub/Cargo.toml` alone (the protocol crate keeps its
//!   own, which the hub's release workflow does not read); the edit updates `Cargo.lock` with it.
//!   Before tagging, the network's crates' tests and the conformance vectors run through the
//!   suite's binary as CI runs them, then the hub's release binary is built for this host and must
//!   name the version.
//! - **The ACE mod** carries its version twice, `Version` in `Meta.json` and `<Version>` in
//!   `Dereth.Network.csproj`, which must agree; the edit moves both together. Before tagging, the
//!   hub the harness links its worlds through is built and the mod's checks run as CI runs them.
//!   The mod itself, the harness's `targets` section and the patches applied with ACE's own Harmony
//!   need an ACE server's assemblies: they run when `DERETH_ACE_DIR` names an ACE server folder,
//!   and are left out, and said so, when it is unset. `dotnet` must be on `PATH`.
//!
//! Neither keeps a change log: its release workflow writes the notes from the commits since the
//! previous tag of the same kind, which the command previews once it has tagged. The workflows
//! read the version from the same files and refuse a tag that does not name it or is not on
//! `main`; they use no workspace tool, so a release can also be cut by hand
//! (`network/RELEASING.md`).

use std::path::{Path, PathBuf};
use std::process::Command;

use regex::Regex;

use super::{preconditions_tagged, push_commands, repo_state, RepoState};
use crate::package::{git, version};
use crate::util::{target_dir, workspace_root};

/// The hub's manifest, whose `[package] version` is the hub's version.
pub const HUB_MANIFEST: &str = "network/hub/Cargo.toml";

/// The mod's manifest for ACE's mod loader, whose `Version` is the mod's version.
pub const ACE_META: &str = "network/ace-mod/Dereth.Network/Meta.json";

/// The mod's project, whose `<Version>` (the assembly's) must equal `Meta.json`'s.
pub const ACE_PROJECT: &str = "network/ace-mod/Dereth.Network/Dereth.Network.csproj";

/// The folder the mod's `dotnet` commands run in, as CI runs them.
const MOD_DIR: &str = "network/ace-mod";

/// The built mod, from [`MOD_DIR`]: the mod's projects build into the workspace's `target/`.
const MOD_DLL: &str = "../../target/ace-mod/bin/Dereth.Network/release/Dereth.Network.dll";

/// The mod's release build folder, relative to the workspace.
const MOD_OUT: &str = "target/ace-mod/bin/Dereth.Network/release";

/// The files of the mod's release folder that its release archive must hold, as the release
/// workflow requires them.
pub const MOD_RELEASE_FILES: [&str; 4] = [
    "Dereth.Network.dll",
    "Meta.json",
    "Settings.json.example",
    "LICENSE",
];

/// The assemblies an ACE server folder must hold for the checks that need ACE.
pub const ACE_ASSEMBLIES: [&str; 2] = ["ACE.Server.dll", "0Harmony.dll"];

/// One of the Dereth Network's released products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Part {
    /// The hub: the world registry, the operator API and the router of the worlds' links.
    Hub,
    /// Dereth.Network, the ACE mod that puts an ACE world on the network.
    Ace,
}

impl Part {
    /// The product named on a command line: `network-hub` or `network-ace`.
    pub fn from_command_name(name: &str) -> Option<Self> {
        match name {
            "network-hub" => Some(Self::Hub),
            "network-ace" => Some(Self::Ace),
            _ => None,
        }
    }

    /// The name a person reads, as the release workflow titles the release.
    pub fn display_name(self) -> &'static str {
        match self {
            Self::Hub => "Dereth Network Hub",
            Self::Ace => "Dereth.Network ACE",
        }
    }

    /// The prefix of the product's release tags.
    pub fn tag_prefix(self) -> &'static str {
        match self {
            Self::Hub => "network-hub-v",
            Self::Ace => "network-ace-v",
        }
    }

    /// The product's release tag for `version`.
    pub fn tag_for(self, version: &str) -> String {
        format!("{}{version}", self.tag_prefix())
    }

    /// The files a version change edits and commits, relative to the workspace.
    pub fn version_files(self) -> &'static [&'static str] {
        match self {
            Self::Hub => &[HUB_MANIFEST, "Cargo.lock"],
            Self::Ace => &[ACE_META, ACE_PROJECT],
        }
    }

    /// The release workflow the tag starts.
    pub fn workflow(self) -> &'static str {
        match self {
            Self::Hub => ".github/workflows/release-network-hub.yml",
            Self::Ace => ".github/workflows/release-network-ace.yml",
        }
    }

    /// The paths whose commits the release workflow lists as the release's notes.
    pub fn notes_paths(self) -> &'static [&'static str] {
        match self {
            Self::Hub => &[
                "network/hub",
                "network/protocol",
                "docs/network/02-link-protocol.md",
                "docs/network/05-hub.md",
            ],
            Self::Ace => &[
                "network/ace-mod",
                "network/conformance/vectors",
                "docs/network",
            ],
        }
    }

    /// The version the tree at `ws` carries.
    pub fn version(self, ws: &Path) -> Result<String, String> {
        match self {
            Self::Hub => version::package_version(&read(ws, HUB_MANIFEST)?)
                .ok_or_else(|| format!("{HUB_MANIFEST} has no literal [package] version")),
            Self::Ace => ace_version(&read(ws, ACE_META)?, &read(ws, ACE_PROJECT)?),
        }
    }

    /// The version files at `ws` with the version set to `new`, as (file, text); nothing is
    /// written. The lock file is cargo's to update.
    pub fn edited(self, ws: &Path, new: &str) -> Result<Vec<(&'static str, String)>, String> {
        match self {
            Self::Hub => {
                let text = version::set_package_version(&read(ws, HUB_MANIFEST)?, new)
                    .map_err(|e| format!("{HUB_MANIFEST}: {e}"))?;
                Ok(vec![(HUB_MANIFEST, text)])
            }
            Self::Ace => {
                let (meta, project) =
                    set_ace_version(&read(ws, ACE_META)?, &read(ws, ACE_PROJECT)?, new)?;
                Ok(vec![(ACE_META, meta), (ACE_PROJECT, project)])
            }
        }
    }
}

fn read(ws: &Path, file: &str) -> Result<String, String> {
    std::fs::read_to_string(ws.join(file)).map_err(|e| format!("reading {file}: {e}"))
}

/// `Meta.json`'s `"Version": "..."` member, its value the one group.
fn meta_version_member() -> Regex {
    Regex::new(r#""Version"\s*:\s*"([^"]*)""#).expect("a valid pattern")
}

/// The project's `<Version>...</Version>` element, its value the one group.
fn project_version_element() -> Regex {
    Regex::new(r"<Version>([^<]*)</Version>").expect("a valid pattern")
}

/// The span of `re`'s group in its one match in `text`; an error naming `what` when there is no
/// match or more than one, either of which the release workflow would misread.
fn only_value(re: &Regex, text: &str, what: &str) -> Result<std::ops::Range<usize>, String> {
    let mut found = re.captures_iter(text);
    let first = found
        .next()
        .ok_or_else(|| format!("{what} has no version"))?;
    if found.next().is_some() {
        return Err(format!("{what} has more than one version"));
    }
    Ok(first.get(1).expect("the pattern has one group").range())
}

/// The version in the mod's `Meta.json` text.
pub fn meta_version(meta: &str) -> Result<String, String> {
    let at = only_value(&meta_version_member(), meta, "Meta.json")?;
    Ok(meta[at].to_owned())
}

/// The version in the mod's project text.
pub fn project_version(project: &str) -> Result<String, String> {
    let at = only_value(&project_version_element(), project, "Dereth.Network.csproj")?;
    Ok(project[at].to_owned())
}

/// The mod's version: `Meta.json`'s and the project's, which must agree.
pub fn ace_version(meta: &str, project: &str) -> Result<String, String> {
    let (m, p) = (meta_version(meta)?, project_version(project)?);
    if m == p {
        Ok(m)
    } else {
        Err(format!(
            "{ACE_META} says {m} and {ACE_PROJECT} says {p}: the mod's two versions must agree, \
             and its release workflow refuses them otherwise"
        ))
    }
}

/// `Meta.json` text with its version set to `new`, everything else kept.
pub fn set_meta_version(meta: &str, new: &str) -> Result<String, String> {
    let at = only_value(&meta_version_member(), meta, "Meta.json")?;
    Ok(format!("{}{new}{}", &meta[..at.start], &meta[at.end..]))
}

/// Project text with its `<Version>` set to `new`, everything else kept.
pub fn set_project_version(project: &str, new: &str) -> Result<String, String> {
    let at = only_value(&project_version_element(), project, "Dereth.Network.csproj")?;
    Ok(format!(
        "{}{new}{}",
        &project[..at.start],
        &project[at.end..]
    ))
}

/// The mod's two version files set to `new` together, as (`Meta.json`, the project): both are
/// edited or neither is, and both read back as `new`.
pub fn set_ace_version(meta: &str, project: &str, new: &str) -> Result<(String, String), String> {
    let meta = set_meta_version(meta, new)?;
    let project = set_project_version(project, new)?;
    let now = ace_version(&meta, &project)?;
    if now != new {
        return Err(format!("after the edit the mod carries {now}, not {new}"));
    }
    Ok((meta, project))
}

/// What the mod's release needs from this machine besides the repository.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Machine {
    /// `dotnet --version` answered.
    pub dotnet: bool,
    /// `DERETH_ACE_DIR` when it is set: the folder, and whether it holds [`ACE_ASSEMBLIES`].
    pub ace_dir: Option<(String, bool)>,
}

impl Machine {
    /// This machine as `part`'s release needs it: `dotnet` and the ACE server folder `ace_dir`
    /// (`DERETH_ACE_DIR`'s) are read for the mod alone.
    fn read(part: Part, ace_dir: Option<std::ffi::OsString>) -> Self {
        if part != Part::Ace {
            return Self::default();
        }
        let dotnet = Command::new("dotnet")
            .arg("--version")
            .output()
            .is_ok_and(|o| o.status.success());
        let ace_dir = ace_dir.filter(|v| !v.is_empty()).map(|v| {
            let dir = std::path::absolute(PathBuf::from(&v)).unwrap_or_else(|_| v.into());
            let whole = ACE_ASSEMBLIES.iter().all(|f| dir.join(f).is_file());
            (dir.display().to_string(), whole)
        });
        Self { dotnet, ace_dir }
    }
}

/// What a network release will do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    /// The version must be moved (and the move committed) first.
    pub bump: bool,
    pub tag: String,
    /// The files the version commit holds.
    pub files: &'static [&'static str],
    /// The ACE server folder the mod is checked against; `None` leaves out the checks that need
    /// ACE.
    pub ace: Option<String>,
}

/// Whether `part`'s `requested` may be released from `state` on `machine`, and how; every reason
/// it may not.
pub fn preconditions(
    part: Part,
    requested: &str,
    state: &RepoState,
    machine: &Machine,
) -> Result<Plan, Vec<String>> {
    let mut problems = Vec::new();
    if part == Part::Ace {
        if !machine.dotnet {
            problems.push(
                "`dotnet` is not on PATH, or has no SDK (`dotnet --version` did not answer): the \
                 mod is built and checked with the .NET SDK, 10 or newer; install it, or release \
                 the mod from a machine that has it"
                    .to_owned(),
            );
        }
        if let Some((dir, false)) = &machine.ace_dir {
            problems.push(format!(
                "DERETH_ACE_DIR is `{dir}`, which holds no {}: point it at an ACE server folder, \
                 or unset it to leave out the checks that need ACE",
                ACE_ASSEMBLIES.join(" and ")
            ));
        }
    }
    match preconditions_tagged(part.tag_for(requested), requested, state) {
        Ok(plan) if problems.is_empty() => Ok(Plan {
            bump: plan.bump,
            tag: plan.tag,
            files: part.version_files(),
            ace: match part {
                Part::Hub => None,
                Part::Ace => machine.ace_dir.clone().map(|(dir, _)| dir),
            },
        }),
        Ok(_) => Err(problems),
        Err(mut shared) => {
            shared.extend(problems);
            Err(shared)
        }
    }
}

/// One command a release runs before it tags: `program` with `args`, in `dir` (relative to the
/// workspace).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    pub dir: &'static str,
    pub program: String,
    pub args: Vec<String>,
}

impl Step {
    fn new(dir: &'static str, program: &str, args: &[&str]) -> Self {
        Self {
            dir,
            program: program.to_owned(),
            args: args.iter().map(|a| (*a).to_owned()).collect(),
        }
    }

    fn line(&self) -> String {
        format!("{} {}", self.program, self.args.join(" "))
    }
}

/// `part`'s checks before it is tagged, as CI's network job runs them. `bin` is cargo's
/// `test-release` output folder; `ace` the ACE server folder the mod is checked against, if any;
/// `windows` whether this host is Windows (the harness's `units` section needs it, as in CI).
pub fn checks(part: Part, ace: Option<&str>, bin: &str, windows: bool) -> Vec<Step> {
    let exe = if windows { ".exe" } else { "" };
    // A cargo command in the workspace, locked and in the test-release profile, as CI runs it.
    let cargo = |command: &str, rest: &[&str]| {
        let mut args = vec![command, "--locked", "--profile", "test-release"];
        args.extend(rest);
        Step::new("", "cargo", &args)
    };
    match part {
        Part::Hub => vec![
            cargo(
                "test",
                &[
                    "-p",
                    "dereth-network-protocol",
                    "-p",
                    "dereth-network-hub",
                    "-p",
                    "dereth-network-conformance",
                ],
            ),
            cargo(
                "build",
                &[
                    "-p",
                    "dereth-network-hub",
                    "-p",
                    "dereth-network-conformance",
                    "--bins",
                ],
            ),
            Step::new(
                "",
                &format!("{bin}/dereth-network-conformance{exe}"),
                &["vectors"],
            ),
        ],
        Part::Ace => {
            let hub = format!("{bin}/dereth-network-hub{exe}");
            let ace_property = ace.map(|a| format!("-p:AceDirectory={a}"));
            let mut steps = vec![cargo("build", &["-p", "dereth-network-hub", "--bins"])];
            if let Some(p) = &ace_property {
                steps.push(Step::new(
                    MOD_DIR,
                    "dotnet",
                    &[
                        "build",
                        "Dereth.Network/Dereth.Network.csproj",
                        "-c",
                        "Release",
                        p,
                    ],
                ));
            }
            steps.push(Step::new(
                MOD_DIR,
                "dotnet",
                &["build", "tests/Harness.csproj", "-c", "Release"],
            ));
            if let Some(p) = &ace_property {
                steps.push(Step::new(
                    MOD_DIR,
                    "dotnet",
                    &[
                        "build",
                        "tests/PatchApply/PatchApply.csproj",
                        "-c",
                        "Release",
                        p,
                    ],
                ));
            }
            let mut harness = vec![
                "../../target/ace-mod/bin/Harness/release/Dereth.Network.Harness.dll",
                "--vectors",
                "../conformance/vectors",
                "--hub",
                hub.as_str(),
            ];
            match ace {
                Some(a) => harness.extend(["--ace", a, "--mod", MOD_DLL]),
                None => harness.extend(["--skip", "targets"]),
            }
            if !windows {
                harness.extend(["--skip", "units"]);
            }
            steps.push(Step::new(MOD_DIR, "dotnet", &harness));
            if let Some(a) = ace {
                steps.push(Step::new(
                    MOD_DIR,
                    "dotnet",
                    &[
                        "../../target/ace-mod/bin/PatchApply/release/Dereth.Network.PatchApply.dll",
                        "--ace",
                        a,
                        "--mod",
                        MOD_DLL,
                    ],
                ));
            }
            steps
        }
    }
}

/// What [`checks`] leaves out on this machine, and why, for the output.
pub fn left_out(part: Part, ace: Option<&str>, windows: bool) -> Vec<String> {
    let mut out = Vec::new();
    if part != Part::Ace {
        return out;
    }
    if ace.is_none() {
        out.push(
            "DERETH_ACE_DIR is not set, so the checks that need ACE's assemblies are left out: \
             the mod itself is not built, and the harness's targets section and the patches \
             applied with ACE's Harmony do not run. The release workflow runs them against its \
             pinned ACE release; set DERETH_ACE_DIR to an ACE server folder to run them here."
                .to_owned(),
        );
    }
    if !windows {
        out.push(
            "the harness's units section is left out off Windows, as CI leaves it out: it checks \
             the text rules against Windows' own conversion."
                .to_owned(),
        );
    }
    out
}

/// The hub's answer to `--version` for `version`, as its release workflow checks it.
pub fn hub_version_line(version: &str) -> String {
    format!("dereth-network-hub {version}")
}

/// The release smoke test: the hub's release binary for this host, built and naming `requested`;
/// the mod's release folder, when the mod was built, holding what its archive needs, its
/// `Meta.json` naming `requested`.
fn smoke(part: Part, ws: &Path, requested: &str, mod_built: bool) -> Result<(), String> {
    match part {
        Part::Hub => {
            println!("\n=== building the hub's release binary for this host as a smoke test");
            let args = ["build", "--locked", "--release", "-p", "dereth-network-hub"];
            if !crate::util::run(ws, "cargo", &args) {
                return Err("the hub's release build failed".to_owned());
            }
            let bin = target_dir().join("release").join(format!(
                "dereth-network-hub{}",
                std::env::consts::EXE_SUFFIX
            ));
            let (ok, out) =
                crate::util::run_captured(ws, &bin.display().to_string(), &["--version"]);
            let want = hub_version_line(requested);
            if !ok || out.trim() != want {
                return Err(format!(
                    "{} --version answered `{}`, not `{want}`",
                    bin.display(),
                    out.trim()
                ));
            }
            println!("{want}");
        }
        Part::Ace if mod_built => {
            println!("\n=== the mod's release folder");
            let out = ws.join(MOD_OUT);
            let missing: Vec<&str> = MOD_RELEASE_FILES
                .iter()
                .copied()
                .filter(|f| !out.join(f).is_file())
                .collect();
            if !missing.is_empty() {
                return Err(format!("{MOD_OUT} lacks {}", missing.join(", ")));
            }
            let staged = meta_version(&read(ws, &format!("{MOD_OUT}/Meta.json"))?)?;
            if staged != requested {
                return Err(format!(
                    "{MOD_OUT}/Meta.json names {staged}, not {requested}"
                ));
            }
            println!("{MOD_OUT} holds {}", MOD_RELEASE_FILES.join(", "));
        }
        Part::Ace => println!(
            "\nnote: without ACE the mod is not built, so its release folder is not checked"
        ),
    }
    Ok(())
}

/// The notes the release workflow will write from the tag: the subjects of the commits since the
/// previous tag of the same kind that touch the product's paths.
fn notes_preview(ws: &Path, part: Part) -> Result<String, String> {
    let pattern = format!("{}*", part.tag_prefix());
    let previous = git(
        ws,
        &[
            "describe",
            "--tags",
            "--abbrev=0",
            "--match",
            &pattern,
            "HEAD^",
        ],
    )
    .ok();
    let range = previous.map_or_else(|| "HEAD".to_owned(), |p| format!("{p}..HEAD"));
    let mut args = vec!["log", "--no-merges", "--format=- %s", &range, "--"];
    args.extend(part.notes_paths());
    Ok(format!("## Changes\n\n{}\n", git(ws, &args)?))
}

/// `cargo xtask release network-hub|network-ace <version>`.
pub fn run(part: Part, requested: &str, push: bool, remote: &str) -> Result<(), String> {
    run_with(
        part,
        requested,
        push,
        remote,
        std::env::var_os("DERETH_ACE_DIR"),
    )
}

/// [`run`], with the ACE server folder the mod is checked against given as `ace_dir` in place of
/// `DERETH_ACE_DIR`.
pub fn run_with(
    part: Part,
    requested: &str,
    push: bool,
    remote: &str,
    ace_dir: Option<std::ffi::OsString>,
) -> Result<(), String> {
    let ws = workspace_root();
    let name = part.display_name();
    let machine = Machine::read(part, ace_dir);
    let state = repo_state(&ws, part.tag_prefix(), part.version(&ws)?, remote)?;
    let plan = preconditions(part, requested, &state, &machine).map_err(|p| p.join("\n"))?;

    if plan.bump {
        println!(
            "=== setting the {name}'s version to {requested} (from {})",
            state.current
        );
        for (file, text) in part.edited(&ws, requested)? {
            std::fs::write(ws.join(file), text).map_err(|e| format!("writing {file}: {e}"))?;
        }
        if part == Part::Hub
            && !crate::util::run(&ws, "cargo", &["update", "--workspace", "--offline"])
        {
            return Err(format!(
                "`cargo update --workspace` failed for Cargo.lock; {HUB_MANIFEST} is edited but \
                 nothing is committed"
            ));
        }
        let now = part.version(&ws)?;
        if now != requested {
            return Err(format!(
                "after the edit the tree carries {now}, not {requested}"
            ));
        }
        let message = format!("{name} {requested}");
        let mut args = vec!["commit", "-m", &message, "--"];
        args.extend(plan.files);
        git(&ws, &args)?;
        println!("committed: {message}");
    } else {
        println!("=== the tree already carries {requested}; nothing to bump");
    }

    let head = git(&ws, &["rev-parse", "HEAD"])?;
    let undo = if plan.bump {
        "\nThe version commit stays; fix the failure and run this again (it will not bump twice), \
         or drop the commit with `git reset --hard HEAD~1`."
    } else {
        ""
    };
    println!("\n=== the {name}'s checks on {head}");
    let windows = cfg!(windows);
    for note in left_out(part, plan.ace.as_deref(), windows) {
        println!("note: {note}");
    }
    let bin = target_dir().join("test-release").display().to_string();
    for step in checks(part, plan.ace.as_deref(), &bin, windows) {
        let args: Vec<&str> = step.args.iter().map(String::as_str).collect();
        if !crate::util::run(&ws.join(step.dir), &step.program, &args) {
            return Err(format!("`{}` failed; no tag was made.{undo}", step.line()));
        }
    }
    smoke(part, &ws, requested, plan.ace.is_some())
        .map_err(|e| format!("{e}; no tag was made.{undo}"))?;

    let message = format!("{name} {requested}");
    git(&ws, &["tag", "-a", &plan.tag, "-m", &message, &head])?;
    println!("\ntagged {} at {head}", plan.tag);
    println!(
        "\n=== the release notes the workflow will write from this tag: the network keeps no \
         CHANGES.md, so they are the commits since the previous {}* tag that touch {}\n",
        part.tag_prefix(),
        part.notes_paths().join(", ")
    );
    match notes_preview(&ws, part) {
        Ok(text) => println!("{text}"),
        Err(e) => println!("note: the release notes could not be previewed: {e}"),
    }

    let commands = push_commands(remote, &plan.tag);
    if push {
        for c in &commands {
            let args: Vec<&str> = c[1..].iter().map(String::as_str).collect();
            if !crate::util::run(&ws, "git", &args) {
                return Err(format!("`{}` failed; run it again by hand", c.join(" ")));
            }
        }
        println!(
            "\npushed. When the tag reaches GitHub, {} builds, checks and drafts the release; \
             publish the draft after reviewing it (network/RELEASING.md).",
            part.workflow()
        );
    } else {
        println!("\nNothing is pushed. To publish, run:");
        for c in &commands {
            println!("    {}", c.join(" "));
        }
        println!(
            "When the tag reaches GitHub, {} builds, checks and drafts the release \
             (network/RELEASING.md). To undo instead: `git tag -d {}`{}.",
            part.workflow(),
            plan.tag,
            if plan.bump {
                " and `git reset --hard HEAD~1`"
            } else {
                ""
            }
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests;
