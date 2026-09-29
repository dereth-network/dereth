//! Empyrean's own names and addresses: every piece of text that names the *server software*.
//!
//! Not ACE. Where ACE's text named itself (ACEmulator, its sites and its tracker), the port names
//! Empyrean instead; each such place is a recorded `brand` divergence. Text that names
//! the game ("Asheron's Call") or the world ("Dereth") is not branding and stays as ACE has it.

/// The server software's name.
pub const PRODUCT: &str = "Empyrean";

/// ACE's name for itself. Still accepted where players or shards carry it (the IOU author) and
/// still named where the text is about ACE (the welcome's "a fork of ACEmulator").
pub const ACE_PRODUCT: &str = "ACEmulator";

/// `Server.WorldName` when the configuration leaves it out (ACE's default was its own name).
pub const DEFAULT_WORLD_NAME: &str = "Empyrean";

/// The project's site: where players find the client and the correct dat files.
pub const SITE_URL: &str = "https://dereth.network";

/// The project's source repository.
pub const REPOSITORY_URL: &str = "https://github.com/dereth-network/dereth/";

/// Where bugs are reported (players' `@reportbug`, and the admin and log "report this" lines).
pub const ISSUES_URL: &str = "https://github.com/dereth-network/dereth/issues";

/// The author and modifier written into exported Lifestoned JSON (ACE writes `ACE.Adapter`).
pub const EXPORT_AUTHOR: &str = "Empyrean";

/// The change comment written into exported Lifestoned JSON.
pub const EXPORT_COMMENT: &str = "Weenie exported from Empyrean world database";

/// The server's public source when the build names no other: the repository its releases are cut
/// from. A build sets `EMPYREAN_BUILD_SOURCE_URL` (compile time; the server's release packaging passes it
/// through) so a fork's binaries point at the fork's repository. Nothing reads it at run time.
pub const SOURCE_URL: &str = match option_env!("EMPYREAN_BUILD_SOURCE_URL") {
    Some(url) => url,
    None => "https://github.com/dereth-network/dereth",
};

/// The target the binaries were compiled for: `EMPYREAN_BUILD_TARGET` at compile time (the release
/// packaging sets it to the target triple), else this build's architecture and operating system.
#[must_use]
pub fn build_target() -> String {
    match option_env!("EMPYREAN_BUILD_TARGET") {
        Some(target) => target.to_owned(),
        None => format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
    }
}

/// What a binary's `--version` prints: its name and version, then the commit, the build time and
/// the target it was built from and for, then where its source is ([`SOURCE_URL`]; no
/// configuration has been read).
#[must_use]
pub fn version_text(binary: &str) -> String {
    let (year, month, day, hour, minute, second) = crate::server_build_info::build_time();
    let built = if year <= 1 {
        "unknown".to_owned()
    } else {
        format!("{year:04}-{month:02}-{day:02} {hour:02}:{minute:02}:{second:02} UTC")
    };
    format!(
        "{binary} {version}\ncommit: {commit}\nbuilt: {built}\ntarget: {target}\nsource: {SOURCE_URL}\n",
        version = crate::server_build_info::VERSION,
        commit = crate::server_build_info::COMMIT,
        target = build_target(),
    )
}

/// The licence the server is under, as it is named to players.
pub const LICENCE: &str = "GNU Affero General Public License v3.0 only (AGPL-3.0-only)";

/// Where this server's source is: `server.source_url` when the configuration sets it (an operator
/// running modified code points it at that code), else [`SOURCE_URL`]. Before any configuration is
/// read, [`SOURCE_URL`].
#[must_use]
pub fn source_url() -> String {
    crate::config_manager::ConfigManager::try_config()
        .map(|c| c.server.source_url.trim().to_owned())
        .filter(|url| !url.is_empty())
        .unwrap_or_else(|| SOURCE_URL.to_owned())
}

/// The login welcome, sent as system chat on every login. Its third line is where this server's
/// source is (`url`, [`source_url`]), which is the offer AGPL-3.0 section 13 asks of a server run
/// for players over a network; `@source` repeats it with the licence.
#[must_use]
pub fn welcome(url: &str) -> String {
    format!(
        "Welcome to Dereth\n powered by {PRODUCT} (a fork of {ACE_PRODUCT})\n({url})\n\nFor more information on commands supported by this server, type @emphelp\n"
    )
}

/// What `@source` says.
#[must_use]
pub fn source_message(url: &str) -> String {
    format!("{PRODUCT} is free software, licensed under the {LICENCE}.\nThis server's source code: {url}")
}
