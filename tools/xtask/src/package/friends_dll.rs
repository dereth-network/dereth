//! The retail Asheron's Call 2 client's friends library, `auberean_friends.dll`, which the
//! launcher's packages carry beside the launcher (`package dereth` and `package auberean`), for
//! every system: the launcher starts the retail client with it on Windows, and a player who runs
//! the retail client under Wine elsewhere points it at the same file.
//!
//! ```text
//! cargo xtask package friends-dll [--out <dir>] [--allow-dirty]
//! ```
//!
//! **Built on Windows only.** The retail client is a 32-bit Windows program, so the library is
//! `i686-pc-windows-msvc`, with the C runtime linked in (no Visual C++ runtime on the player's
//! machine or under Wine), built with the release environment the launcher is built with. Only a
//! Windows host links it. `package friends-dll` builds and checks it and writes it to `<dir>`
//! (`<target dir>/package/friends-dll` by default); a launcher package takes that file with
//! `--friends-dll <file>` (or [`ENV`]), and a Windows host that is given none builds it itself. The
//! release workflows build it once, on Windows, and hand the same file to every system's job.
//!
//! **The check.** A 32-bit x86 Windows DLL that imports no Visual C++ runtime.

use std::path::{Path, PathBuf};

use super::dereth::{self, copy, mkdir, run_in};
use super::headers;
use super::targets::{self, Os};
use super::{read, sha256_hex, BuildFacts};
use crate::util::{target_dir, workspace_root};

/// The library's file name, beside the launcher.
pub const FILE: &str = "auberean_friends.dll";

/// Its package.
pub const PACKAGE: &str = "auberean-friends-dll";

/// The target it is built for: the retail client's.
pub const TRIPLE: &str = "i686-pc-windows-msvc";

/// The variable naming a library already built, for a host that cannot build one.
pub const ENV: &str = "DERETH_FRIENDS_DLL";

const IMAGE_FILE_MACHINE_I386: u16 = 0x14C;
const IMAGE_FILE_DLL: u16 = 0x2000;

pub const USAGE: &str = "usage: cargo xtask package friends-dll [--out <dir>] [--allow-dirty]";

/// The header check: a 32-bit x86 Windows DLL that imports no Visual C++ runtime. What it read,
/// or what is wrong.
pub fn check(bytes: &[u8]) -> Result<String, Vec<String>> {
    let pe = headers::parse_pe(bytes).map_err(|e| vec![e])?;
    let mut problems = Vec::new();
    if pe.machine != IMAGE_FILE_MACHINE_I386 || pe.pe32_plus {
        problems.push(format!(
            "machine {:#06x}{}: the retail client is 32-bit x86 ({IMAGE_FILE_MACHINE_I386:#06x}, PE32)",
            pe.machine,
            if pe.pe32_plus { ", PE32+" } else { "" }
        ));
    }
    let pe_at = bytes
        .get(0x3C..0x40)
        .map_or(0, |b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]) as usize);
    let characteristics = bytes
        .get(pe_at + 22..pe_at + 24)
        .map_or(0, |b| u16::from_le_bytes([b[0], b[1]]));
    if characteristics & IMAGE_FILE_DLL == 0 {
        problems.push("not a DLL".to_owned());
    }
    let runtime: Vec<&str> = pe
        .imports
        .iter()
        .filter(|i| headers::is_c_runtime_dll(i))
        .map(String::as_str)
        .collect();
    if !runtime.is_empty() {
        problems.push(format!(
            "imports the Visual C++ runtime ({}); the C runtime must be linked statically \
             (+crt-static)",
            runtime.join(", ")
        ));
    }
    if problems.is_empty() {
        Ok(format!("PE32 i386 DLL; imports {}", pe.imports.join(", ")))
    } else {
        Err(problems)
    }
}

/// Why this host cannot have the library for a package, if it cannot: one is given (`given`, or
/// [`ENV`]), or this host is Windows with the target installed.
pub fn refusal(given: Option<&Path>, host: &targets::Host) -> Option<String> {
    if given.is_some() || std::env::var_os(ENV).is_some_and(|v| !v.is_empty()) {
        return None;
    }
    if host.os != Some(Os::Windows) {
        return Some(format!(
            "a launcher package carries the retail client's friends library ({FILE}), which only \
             a Windows host builds: build it there with `cargo xtask package friends-dll`, then \
             pass --friends-dll <file> (or set {ENV})"
        ));
    }
    match &host.installed {
        Some(installed) if !installed.iter().any(|t| t == TRIPLE) => Some(format!(
            "the friends library ({FILE}) is built for {TRIPLE}, which is not installed: \
             `rustup target add {TRIPLE}`"
        )),
        _ => None,
    }
}

/// The library for a package, checked and staged in `stage`: the one given (`given`, or
/// [`ENV`]), else built here.
pub fn find(
    ws: &Path,
    facts: &BuildFacts,
    given: Option<&Path>,
    stage: &Path,
) -> Result<PathBuf, String> {
    let given = given.map(Path::to_path_buf).or_else(|| {
        std::env::var_os(ENV)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    });
    let from = match given {
        Some(file) => file,
        None => build(ws, facts)?,
    };
    let bytes = read(&from)?;
    let described = check(&bytes).map_err(|problems| {
        format!(
            "the header check refused {}:\n  {}",
            from.display(),
            problems.join("\n  ")
        )
    })?;
    println!("  {FILE}: {described}; sha256 {}", sha256_hex(&bytes));
    let staged = stage.join(FILE);
    copy(&from, &staged)?;
    Ok(staged)
}

/// Build the library with the release environment (the C runtime linked in, local paths
/// remapped, no link time): where cargo wrote it.
fn build(ws: &Path, facts: &BuildFacts) -> Result<PathBuf, String> {
    let windows = targets::find_dereth("x86_64-pc-windows-msvc")?;
    let mut env = dereth::build_env(ws, facts, windows);
    for (k, v) in &mut env {
        if k == "DERETH_BUILD_TARGET" {
            TRIPLE.clone_into(v);
        }
    }
    run_in(
        ws,
        "cargo",
        &[
            "build",
            "--locked",
            "--release",
            "-p",
            PACKAGE,
            "--target",
            TRIPLE,
        ],
        &env,
        &[],
    )
    .map_err(|e| format!("{e}; is {TRIPLE} installed? `rustup target add {TRIPLE}`"))?;
    Ok(target_dir().join(TRIPLE).join("release").join(FILE))
}

/// `cargo xtask package friends-dll ...` (the arguments after `friends-dll`).
pub fn package(args: &[String]) -> i32 {
    let mut out = None;
    let mut allow_dirty = false;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--out" => match it.next() {
                Some(v) => out = Some(PathBuf::from(v)),
                None => {
                    eprintln!("--out needs a value\n{USAGE}");
                    return 2;
                }
            },
            "--allow-dirty" => allow_dirty = true,
            other => {
                eprintln!("unexpected argument `{other}`\n{USAGE}");
                return 2;
            }
        }
    }
    let ws = workspace_root();
    let result = (|| {
        if let Some(why) = refusal(None, &super::host()) {
            return Err(why);
        }
        let facts = dereth::launcher_facts(&ws, allow_dirty)?;
        let dir = out.unwrap_or_else(|| target_dir().join("package").join("friends-dll"));
        mkdir(&dir)?;
        find(&ws, &facts, None, &dir)
    })();
    match result {
        Ok(file) => {
            println!("\npackage: done; the library is {}", file.display());
            0
        }
        Err(e) => {
            eprintln!("\npackage: FAILED\n{e}");
            1
        }
    }
}
