// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/ServerBuildInfo_Static.cs, Source/ACE.Server/ServerBuildInfo_Dynamic.cs
//! Port of `Source/ACE.Server/ServerBuildInfo_Static.cs` (and the values of
//! `ServerBuildInfo_Dynamic.cs`).
//!
//! It lives in empyrean-common rather than the server binary so the version handler (empyrean-world) and
//! the commands (empyrean-command) can reach it. ACE's build writes `ServerBuildInfo_Dynamic.cs` from
//! git and the clock at compile time; here the same values come from the build environment when it
//! sets them.

use crate::dotnet::DotNetDateTime;

// ACE: ServerBuildInfo.Branch
// DIVERGE: ACE's build writes the git branch into the source; here `EMPYREAN_BUILD_BRANCH` at compile time, else "unknown".
pub const BRANCH: &str = match option_env!("EMPYREAN_BUILD_BRANCH") {
    Some(b) => b,
    None => "unknown",
};

// ACE: ServerBuildInfo.Commit
// DIVERGE: the git commit from `EMPYREAN_BUILD_COMMIT` at compile time, else forty zeros.
pub const COMMIT: &str = match option_env!("EMPYREAN_BUILD_COMMIT") {
    Some(c) => c,
    None => "0000000000000000000000000000000000000000",
};

// ACE: ServerBuildInfo.Version
// DIVERGE: the crate version stands in for ACE's release number.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

// ACE: ServerBuildInfo.Build
// DIVERGE: no build counter; `EMPYREAN_BUILD_NUMBER` at compile time, else "0".
pub const BUILD: &str = match option_env!("EMPYREAN_BUILD_NUMBER") {
    Some(b) => b,
    None => "0",
};

// ACE: ServerBuildInfo.BuildYear, ServerBuildInfo.BuildMonth, ServerBuildInfo.BuildDay, ServerBuildInfo.BuildHour, ServerBuildInfo.BuildMinute, ServerBuildInfo.BuildSecond
/// The compile time as (year, month, day, hour, minute, second).
// DIVERGE: no compile clock; `EMPYREAN_BUILD_UTC` ("yyyyMMddHHmmss") at compile time, else 0001-01-01 00:00:00.
#[must_use]
pub fn build_time() -> (i32, i32, i32, i32, i32, i32) {
    let Some(s) = option_env!("EMPYREAN_BUILD_UTC")
        .filter(|s| s.len() == 14 && s.bytes().all(|b| b.is_ascii_digit()))
    else {
        return (1, 1, 1, 0, 0, 0);
    };
    let n = |a: usize, b: usize| s[a..b].parse::<i32>().unwrap_or(0);
    (n(0, 4), n(4, 6), n(6, 8), n(8, 10), n(10, 12), n(12, 14))
}

// ACE: ServerBuildInfo.CommitID
/// The first seven characters of the commit.
///
/// # Panics
/// For a commit shorter than seven characters (ACE's `ArgumentOutOfRangeException`).
#[must_use]
pub fn commit_id() -> String {
    let id: String = COMMIT.chars().take(7).collect();
    assert!(
        id.chars().count() == 7,
        "ACE: ArgumentOutOfRangeException (Commit.Substring(0, 7))"
    );
    id
}

// ACE: ServerBuildInfo.CompilationTimestampUtc
#[must_use]
pub fn compilation_timestamp_utc() -> DotNetDateTime {
    let (year, month, day, hour, minute, second) = build_time();
    DotNetDateTime::new_hms(year, month, day, hour, minute, second)
}

// ACE: ServerBuildInfo.FullVersion
/// `{Version}.{Build}.{CompilationTimestampUtc:yyyyMMddHHmmss}-{Branch}-{CommitID}`.
#[must_use]
pub fn full_version() -> String {
    format!(
        "{VERSION}.{BUILD}.{}-{BRANCH}-{}",
        compilation_timestamp_utc().format("yyyyMMddHHmmss"),
        commit_id()
    )
}

// ACE: ServerBuildInfo.GetVersionInfo
/// The server binaries' version and compile time, the world database's version (its base and
/// patch versions and last-modified time), the build mode, and where the server's source is.
///
/// DIVERGE: ACE adds "Server is running inside a Container" when `DOTNET_RUNNING_IN_CONTAINER` is
/// true; Empyrean has no container special case.
// DIVERGE: `DatabaseManager.World.GetVersion()` is read by the caller (this crate has no database) and passed in; a null row is the caller's `NullReferenceException`.
#[must_use]
pub fn get_version_info(
    base_version: Option<&str>,
    patch_version: Option<&str>,
    last_modified: DotNetDateTime,
) -> String {
    // DIVERGE: ACE's line ends `: ACEmulator`; ours ends with our name, in the same layout (brand).
    let mut msg = format!(
        "Server binaries version {} - compiled {} : {}\n",
        full_version(),
        compilation_timestamp_utc().format("ddd MMM d HH:mm:ss yyyy"),
        crate::brand::PRODUCT
    );

    msg += &format!(
        "Server database version Base: {} Patch: {} - compiled {}\n",
        base_version.unwrap_or(""),
        patch_version.unwrap_or(""),
        last_modified.format("ddd MMM d HH:mm:ss yyyy")
    );

    if cfg!(debug_assertions) {
        msg += "Server is compiled in DEBUG mode\n";
    } else {
        msg += "Server is compiled in RELEASE mode\n";
    }
    // DIVERGE: a last line with where this server's source is (the AGPL source offer; ACE makes none).
    msg += &format!("Server source: {}\n", crate::brand::source_url());
    msg
}

// ACE: ServerBuildInfo.GetServerVersion
/// `new Version(Version + "." + Build)`: the version's numeric components.
///
/// # Panics
/// On a component that is not a non-negative number, or fewer than two or more than four
/// (`System.Version`'s exceptions).
#[must_use]
pub fn get_server_version() -> Vec<i32> {
    version_components(VERSION, BUILD)
}

/// The numeric components of `version` and `build`, as [`get_server_version`] reads them.
///
/// # Panics
/// As [`get_server_version`].
#[must_use]
pub fn version_components(version: &str, build: &str) -> Vec<i32> {
    // DIVERGE: a release version may carry a pre-release or build suffix (`0.1.0-rc.1`,
    // `0.1.0+abc`); only its numeric core is a `System.Version` component. ACE's versions never
    // carry one.
    let core = version.split(['-', '+']).next().unwrap_or(version);
    let s = format!("{core}.{build}");
    let parts: Vec<i32> = s
        .split('.')
        .map(|p| {
            p.trim()
                .parse::<i32>()
                .ok()
                .filter(|v| *v >= 0)
                .expect("ACE: FormatException / ArgumentOutOfRangeException (new Version)")
        })
        .collect();
    assert!(
        (2..=4).contains(&parts.len()),
        "ACE: ArgumentException (new Version: {s})"
    );
    parts
}
