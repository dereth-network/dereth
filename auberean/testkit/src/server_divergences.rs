//! The server's divergence register: where Yalain deliberately does something other than what the
//! January 2017 servers did, or where those servers' recordings are silent and Yalain chooses.
//!
//! A divergence is a **choice**, not a defect and not a feature still to be built. Each is one of
//! three [`Kind`]s: the recordings show what the servers did and Yalain does otherwise; the
//! recordings show nothing, so what Yalain does is its own; or the recordings show something too
//! seldom to model, so Yalain leaves it out. A place where Yalain reproduces an odd retail behavior
//! on purpose is not a divergence, because it matches retail, and neither is a feature the original
//! servers never had (the network's hub, the operator's commands), which the behavior registry
//! holds as tooling.
//!
//! Each [`Divergence`] is named, never numbered: `YD-` and a short lowercase name joined by hyphens
//! (`YD-talk-has-a-limit`), in alphabetical order, so changes made on separate branches never
//! compete for a number. Each carries an opaque evidence handle that resolves privately, and may
//! name the behavior-registry rows whose tests assert what Yalain does.
//!
//! The public list, `yalain/DIVERGENCES.md`, is generated from this table and the rows it names by
//! [`render`], and the `cpu` tests compare it against the file, so the file cannot drift from the
//! register. Rewriting it is a deliberate act: run those tests with [`REWRITE_VAR`] set.

use crate::behaviours;

/// What a divergence departs from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// The recordings show what the servers did, and Yalain does otherwise.
    Differs,
    /// No recording shows what the servers did; what Yalain does is its own choice.
    Unrecorded,
    /// The recordings show it too seldom, and for no cause they show, to model; Yalain leaves it out.
    Unmodeled,
}

impl Kind {
    /// The kind's word in the published list.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Differs => "differs",
            Self::Unrecorded => "unrecorded",
            Self::Unmodeled => "unmodeled",
        }
    }
}

/// One deliberate difference from the 2017 servers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence {
    /// `YD-` and a lowercase name joined by hyphens. Stable: an id is never reused.
    pub id: &'static str,
    pub kind: Kind,
    /// A short name for the divergence, in words a player or an operator would recognize.
    pub title: &'static str,
    /// What the 2017 servers did, or that no recording shows it, with the counts.
    pub retail: &'static str,
    /// What Yalain does instead.
    pub yalain: &'static str,
    /// Why, or what a player notices, in a line.
    pub why: &'static str,
    /// An opaque handle, `AC2-EVID-` and capital letters, digits and hyphens, that resolves to the
    /// evidence in the private index.
    pub evidence: &'static str,
    /// The behavior-registry rows whose tests assert what Yalain does, if any do.
    pub rows: &'static [&'static str],
}

/// The variable that makes the `cpu` tests rewrite the published list instead of comparing it.
pub const REWRITE_VAR: &str = "AUBEREAN_TEST_REWRITE";

/// Whether this run rewrites the published list.
#[must_use]
pub fn rewriting() -> bool {
    std::env::var_os(REWRITE_VAR).is_some_and(|v| v != "0" && !v.is_empty())
}

/// Every divergence, in id order.
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "YD-a-line-past-the-longest-is-refused",
        kind: Kind::Unrecorded,
        title: "A line of talk has a longest",
        retail: "No recording shows a longest line: the longest recorded are 244 bytes of 167 \
                 sayings, 492 of 79 tells and 556 of 1,316 room lines.",
        yalain: "A line of talk past 5 KB reaches nobody, however it is said, and its speaker is \
                 told it is too long to say.",
        why: "One very long line would hold back everything sent after it to everyone near or in \
              the room.",
        evidence: "AC2-EVID-SRV-TALK-TOO-LONG",
        rows: &["server.chat.line-too-long-goes-nowhere"],
    },
    Divergence {
        id: "YD-a-line-to-a-hidden-channel-is-refused",
        kind: Kind::Unrecorded,
        title: "A line to a channel the player's windows hide is refused",
        retail: "A channel's room was heard only by players one of whose chat windows shows it (14 \
                 of 14 such players heard a line, 0 of 10 others); no recording holds a player \
                 sending a line to a channel all its windows hide.",
        yalain: "Such a line is refused, and nobody hears it.",
        why: "A player who cannot hear a room does not talk in it, as the channel list it is sent \
              leaves the channel out.",
        evidence: "AC2-EVID-SRV-CHAT-HIDDEN-REFUSED",
        rows: &["server.chat.filter-hidden-room-unheard"],
    },
    Divergence {
        id: "YD-butchery-mid-attack-not-refused",
        kind: Kind::Differs,
        title: "A butchery begun mid-attack never hangs",
        retail: "Of 18 butcheries refused as not possible in the player's mode, 17 had begun while \
                 the player attacked, 13 of them just after a walk to the corpse; each played the \
                 butchering animation first and never let it go, so the butcher's animation hung.",
        yalain: "A butchery is never refused for the player's mode: the butchering animation is let \
                 go as it plays, and the corpse is butchered.",
        why: "The hung animation is a defect; of the 90 butcheries begun mid-attack that went \
              through, the 2017 servers ended the attack first and butchered cleanly in 87.",
        evidence: "AC2-EVID-SRV-BUTCHERY-REFUSED-84",
        rows: &["server.craft.butchery-gathers-a-material"],
    },
    Divergence {
        id: "YD-calls-past-a-limit-are-dropped",
        kind: Kind::Unrecorded,
        title: "Game calls have a limit",
        retail: "No recording shows the servers limiting how fast a client sent game calls: the \
                 busiest of 1,605 connections sent 53 in a second, 287 in ten seconds and 905 in a \
                 minute.",
        yalain: "A client sending game calls past 2,000 at once and 200 a second has what is over \
                 dropped unread, and the rest go on as ever.",
        why: "One client must not hold up a world open to anyone; no 2017 client came within a tenth \
              of the limit.",
        evidence: "AC2-EVID-SRV-CALL-LIMIT",
        rows: &["server.operator.a-flood-of-calls-is-dropped"],
    },
    Divergence {
        id: "YD-general-error-on-a-live-creature-not-modeled",
        kind: Kind::Unmodeled,
        title: "No general error for an item used on a creature",
        retail: "An item used on a live creature was refused with a general error in 3 of 4,559 \
                 recorded uses on corpses and creatures, for no cause the recordings show.",
        yalain: "That refusal is never given: such a use is answered by the item's and the \
                 creature's own rules.",
        why: "Three cases with no cause to tell them apart are too few to model.",
        evidence: "AC2-EVID-SRV-USE-GENERAL-ERROR",
        rows: &[],
    },
    Divergence {
        id: "YD-run-moved-onto-a-player-outside-the-fellowship-asks-who-may-harm",
        kind: Kind::Unrecorded,
        title: "A run moved on to another player asks who may harm whom",
        retail: "A run moved on to another target named while attacking was refused within the \
                 fellowship (143 of 143, at a fellow or a pet), and the 46 runs that swung at the \
                 target named all swung at creatures; no recording holds a run moved on to a player \
                 outside the fellowship.",
        yalain: "Such a run goes on only where who may harm whom allows an attack on that player, as \
                 a run begun at the player would; where it does not, the attack fails with the \
                 reason and the run ends.",
        why: "Moving a run is a new attack on whoever it moves to, so it is held to the same rule.",
        evidence: "AC2-EVID-SRV-NEXT-TARGET-PLAYER",
        rows: &[
            "server.player-fights.a-run-goes-on-to-the-target-named",
            "server.player-fights.hit-is-pk-damage",
        ],
    },
    Divergence {
        id: "YD-run-moved-onto-an-item-or-a-players-corpse-not-modeled",
        kind: Kind::Unmodeled,
        title: "No item or player's corpse refusals for a run moved on",
        retail: "A run moved on to another target named while attacking was refused as aimed at an \
                 item in 11 recorded cases (the player's own, another's, a monster's or a player's: \
                 6, 1, 1 and 3) and at a player's corpse in 1.",
        yalain: "Neither refusal is given: a run moved on to an item or a player's corpse is refused \
                 as aimed at a target not alive, and ends.",
        why: "Twelve cases over five refusals are too few to model each one's own reason.",
        evidence: "AC2-EVID-SRV-NEXT-TARGET-UNMODELED",
        rows: &[],
    },
    Divergence {
        id: "YD-say-echo-carries-the-words-as-sent",
        kind: Kind::Differs,
        title: "A saying's echo is the words as sent",
        retail: "The saying echoed to its speaker carried the client's words with one number \
                 changed, the first object number among them, which the server numbered on its own \
                 (143 of 143 echoes were the client's words but for it).",
        yalain: "The echo carries the client's words exactly as it sent them.",
        why: "The number only ties the words' parts together; no difference a player sees is known.",
        evidence: "AC2-EVID-SRV-SAY-ECHO-NUMBER",
        rows: &["server.chat.say-heard-nearby"],
    },
    Divergence {
        id: "YD-sayings-reach-only-clients-that-know-the-speaker",
        kind: Kind::Differs,
        title: "Sayings go only to clients that know the speaker",
        retail: "A saying went to clients near the speaker whether or not they had been told of it: \
                 109 recorded sayings by others reached a client told of the speaker only \
                 afterwards, and 432 one never told of it while recording.",
        yalain: "A saying goes only to the clients that have been told of its speaker.",
        why: "A client holds a saying on an object it does not know until it learns of it; no \
              difference a player sees is measured.",
        evidence: "AC2-EVID-SRV-SAY-UNKNOWN-SPEAKER",
        rows: &["server.chat.say-heard-nearby"],
    },
    Divergence {
        id: "YD-skill-information-stamped-at-each-raise",
        kind: Kind::Differs,
        title: "A raised skill's information is stamped with the time",
        retail: "A raised skill's information sometimes kept the time cached in the answer before \
                 it for that skill: 218 of 1,336 raises after an earlier one, 150 of them within \
                 half a second of it.",
        yalain: "Each raise stamps the skill's information with the game time it is answered.",
        why: "Which answers kept the earlier time follows no rule the recordings show; no \
              difference a player sees is known.",
        evidence: "AC2-EVID-SRV-SKILL-RAISE-CACHED-TIME",
        rows: &["server.skills.raise"],
    },
    Divergence {
        id: "YD-talk-has-a-limit",
        kind: Kind::Unrecorded,
        title: "Talk has a limit",
        retail: "No recording shows a limit on how fast a player talked, and the game has no words \
                 for one, only squelch lists and reports of spam; the busiest player said 1 line in \
                 a second, 3 in ten seconds and 6 in a minute.",
        yalain: "A player's sayings, tells, emotes and room lines spend one limit, about ten a \
                 second: past it a line goes nowhere and the player is told, at most every few \
                 seconds.",
        why: "A world open to anyone needs a check on flooding the chat.",
        evidence: "AC2-EVID-SRV-TALK-LIMIT",
        rows: &["server.chat.talk-has-a-limit"],
    },
];

/// Whether `id` is a divergence's id: `YD-` and lowercase letters and digits in words joined by
/// single hyphens, beginning with a letter.
#[must_use]
pub fn is_id(id: &str) -> bool {
    id.strip_prefix("YD-").is_some_and(|name| {
        name.starts_with(|c: char| c.is_ascii_lowercase())
            && name.split('-').all(|word| {
                !word.is_empty()
                    && word
                        .chars()
                        .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
            })
    })
}

/// Whether `handle` is an opaque evidence handle: `AC2-EVID-` and capital letters, digits and
/// hyphens.
#[must_use]
pub fn is_handle(handle: &str) -> bool {
    handle.strip_prefix("AC2-EVID-").is_some_and(|rest| {
        !rest.is_empty()
            && rest
                .chars()
                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '-')
    })
}

/// Look one divergence up by id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|d| d.id == id)
}

/// Where the published list lives, relative to this crate's manifest.
pub const PUBLISHED: &str = "../../yalain/DIVERGENCES.md";

/// The test that asserts a behavior-registry row, by the row's id.
#[must_use]
pub fn station(row: &str) -> Option<&'static str> {
    behaviours::all().find(|b| b.id == row).map(|b| b.station)
}

/// The published list, as Markdown.
#[must_use]
pub fn render() -> String {
    let mut out = String::new();
    for paragraph in [
        "# Deliberate divergences from the retail servers".to_owned(),
        "Yalain rebuilds the Asheron's Call 2 servers from their recorded sessions, which end in \
         January 2017, and this is the list of places where it deliberately behaves differently. \
         Each row is a choice, of one of three kinds:"
            .to_owned(),
        "- **differs**: the recordings show what the servers did, and Yalain does otherwise;\n\
         - **unrecorded**: no recording shows what the servers did, so what Yalain does is its own \
         choice, listed so it is not taken for retail;\n\
         - **unmodeled**: the recordings show something too seldom, and for no cause they show, \
         to model, so Yalain leaves it out."
            .to_owned(),
        "Defects and features not yet built are not listed, and neither is anything Yalain \
         reproduces from the recordings on purpose, however odd, nor anything the original servers \
         never had. The code names a row by its id, `YD-` and a short name. The client's own list \
         is [`auberean/testkit/src/divergences.rs`](../auberean/testkit/src/divergences.rs)."
            .to_owned(),
        "Each row names the behavior-registry rows whose tests assert what Yalain does, as a test \
         path whose last parts can be passed to `cargo test` as a filter."
            .to_owned(),
        format!(
            "This file is generated from the register in `auberean-testkit` \
             (`src/server_divergences.rs`) and the registry rows it names; edit those, not this \
             file, and rewrite it with \
             `{REWRITE_VAR}=1 cargo test -p auberean-testkit --test cpu server_divergences`."
        ),
    ] {
        out.push_str(&paragraph);
        out.push_str("\n\n");
    }
    out.push_str("| id | kind | the 2017 servers | Yalain | why | tests |\n");
    out.push_str("|---|---|---|---|---|---|\n");
    for d in DIVERGENCES {
        let tests: Vec<String> = d
            .rows
            .iter()
            .map(|r| match station(r) {
                Some(s) => format!("`{r}`: `{s}`"),
                None => format!("`{r}`"),
            })
            .collect();
        out.push_str(&format!(
            "| {} | {} | **{}.** {} | {} | {} | {} |\n",
            d.id,
            d.kind.word(),
            cell(d.title),
            cell(d.retail),
            cell(d.yalain),
            cell(d.why),
            if tests.is_empty() {
                "-".to_owned()
            } else {
                tests.join("<br>")
            }
        ));
    }
    out
}

/// One table cell: a pipe would end the cell early.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}
