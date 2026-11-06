//! Named groups per account for the eight account-level admin commands, as the game kept them: an
//! admin character may give any admin command, and these eight are checked against the account's
//! own groups as well.
//!
//! The operator names the groups: a command listed in [`crate::ServerConfig::admin_commands`] may be
//! given only by an account in one of its groups, and a command listed nowhere is open to every
//! admin. What does one of these commands' work answers to its groups ([`answers_to`]). The console
//! is never refused. The check is on the server's record, the same for a command
//! said in game and a call from an admin character.

use yalain_store::Store;

use crate::admin::Issuer;
use crate::Server;

/// The eight commands' words, as the operator writes them.
pub const WORDS: [&str; 8] = [
    "ban",
    "boot",
    "fingerchar",
    "fingerid",
    "fingeraccount",
    "serverparam",
    "userlimit",
    "dumpadmintable",
];

/// The function each command's call has.
const CALLS: [(u32, &str); 8] = [
    (0x8201_0016, "ban"),
    (0x8201_0092, "boot"),
    (0x8201_001B, "fingerchar"),
    (0x8201_0023, "fingerid"),
    (0x8201_0027, "fingeraccount"),
    (0x8201_002B, "serverparam"),
    (0x8201_008B, "userlimit"),
    (0x8201_0075, "dumpadmintable"),
];

/// Is this word one of the eight (in any case)?
#[must_use]
pub fn is_command(word: &str) -> bool {
    WORDS.iter().any(|w| w.eq_ignore_ascii_case(word))
}

/// The command a call of the admin family gives, when it is one of the eight.
#[must_use]
pub fn word_of(function: u32) -> Option<&'static str> {
    CALLS.iter().find(|(f, _)| *f == function).map(|(_, w)| *w)
}

/// A comma-separated list of group names, trimmed; no empty one.
#[must_use]
pub fn read_list(line: &str) -> Vec<String> {
    line.split(',')
        .map(str::trim)
        .filter(|g| !g.is_empty())
        .map(str::to_owned)
        .collect()
}

/// The command whose groups another is kept to, for what does its work: giving a player's account a
/// new secret and lifting a ban answer to whoever may ban, and reading an account or the list of them
/// to whoever may finger one.
fn answers_to(word: &str) -> &str {
    match word {
        "set-accountpassword" | "unban" | "banlist" => "ban",
        "accountget" | "accountlist" => "fingeraccount",
        other => other,
    }
}

/// Why an issuer may not give a command: `None` when it may. The console always may; a word the
/// operator keeps to nothing is open to every admin.
#[must_use]
pub fn refusal<S: Store>(s: &Server<S>, by: &Issuer, word: &str) -> Option<String> {
    let Issuer::Account { groups, .. } = by else {
        return None;
    };
    let kept = s
        .cfg
        .admin_commands
        .iter()
        .find(|(w, _)| w.eq_ignore_ascii_case(answers_to(word)))
        .map(|(_, g)| g)?;
    let allowed = kept
        .iter()
        .any(|g| groups.iter().any(|h| h.eq_ignore_ascii_case(g)));
    (!allowed).then(|| format!("{word} needs the group {}.", kept.join(" or ")))
}

/// The registered commands and the groups each is kept to: what `/dumpadmintable` shows.
#[must_use]
pub fn table<S: Store>(s: &Server<S>) -> Vec<String> {
    WORDS
        .iter()
        .map(|word| {
            let kept = s.cfg.admin_commands.iter().find(|(w, _)| w == word);
            match kept {
                Some((_, groups)) => format!("{word}: {}", groups.join(", ")),
                None => format!("{word}: every admin"),
            }
        })
        .collect()
}
