//! The client divergence register: where the Dereth client deliberately does something the retail
//! client did not.
//!
//! A divergence is a **choice**, not a defect and not a feature still to be built: the retail
//! behaviour is known, this client does something else on purpose, and the reason is written
//! down. A place where this client reproduces an odd retail behaviour on purpose is not a
//! divergence, because it matches retail.
//!
//! Each [`Divergence`] has a stable `CD-###` id, and the behaviour rows that assert it name that
//! id in their `divergence` field. The link is required in both directions: a row whose `since` is
//! "this client" -- a claim read off this client's own rule rather than off retail -- must name a
//! divergence, and a divergence that no row names is a claim nothing asserts. A row stamped
//! "tooling" is a developer affordance, such as running with no server: it is recorded and
//! asserted but not published, and it names no divergence. Code at a
//! divergence's site may cite its id, in behaviour terms, the way the server's code cites its `V`
//! numbers.
//!
//! Each entry is a headline: what changes and, in a line, why. The rows that name the entry
//! carry the detail, and their tests the exact behaviour.
//!
//! The public list, `dereth/DIVERGENCES.md`, is generated from this table and the rows that
//! name each id, and [`render`] is compared against it by this module's own test, so the file
//! cannot drift from the registry. Rewriting it is the goldens' deliberate act: run this module's
//! tests with the golden rewrite variable ([`crate::golden::REWRITE_VAR`]) set.

use crate::behaviours::{self, Behaviour};

/// One deliberate difference from the retail client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence {
    /// `CD-` and three digits. Stable: an id is never reused, and a divergence that is withdrawn
    /// keeps its number out of circulation.
    pub id: &'static str,
    /// A short name for the divergence, in words a player would recognise.
    pub title: &'static str,
    /// What this client does differently from the retail client, in one plain sentence.
    pub change: &'static str,
    /// Why, or what the player notices, in one line.
    pub why: &'static str,
}

/// The ids of divergences that were withdrawn because the client turned out to behave as retail
/// does. Each keeps its number out of circulation, so the register skips it.
pub static WITHDRAWN: &[&str] = &["CD-020"];

/// Every divergence, in id order.
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "CD-001",
        title: "A resolution change keeps the window where it is",
        change: "A resolution change, or leaving full screen, keeps the window's top-left corner \
                 instead of re-centring it.",
        why: "A window the player has placed stays where they put it.",
    },
    Divergence {
        id: "CD-002",
        title: "Full screen is a borderless window over the whole monitor",
        change: "Full screen covers the window's monitor at its own size and never changes the \
                 display mode, so the resolution setting does not apply there.",
        why: "Other monitors, windows and overlays are left alone.",
    },
    Divergence {
        id: "CD-003",
        title: "Full screen does not float over other windows",
        change: "The window is never kept on top of other windows, windowed or full screen.",
        why: "With a borderless full screen, floating only gets in the way.",
    },
    Divergence {
        id: "CD-004",
        title: "A resolution is real screen pixels at any desktop scaling",
        change: "The client is aware of desktop scaling, so the configured resolution is exactly \
                 that many pixels on any monitor.",
        why: "A window the desktop enlarges no longer fits the monitor it was sized for.",
    },
    Divergence {
        id: "CD-005",
        title: "Full screen applies in the world only",
        change: "The screens before the world are always a window; full screen is applied on \
                 entering the world and the switch key works only there.",
        why: "Before the world, full screen cannot be held reliably on every platform.",
    },
    Divergence {
        id: "CD-006",
        title: "The window title names the account",
        change: "The window title is the client's name and the account it logged in with.",
        why: "It tells several clients on one machine apart on the task bar.",
    },
    Divergence {
        id: "CD-007",
        title: "The client's files live in a folder of its own",
        change: "Preferences and key maps live in Dereth's own settings folder, copied from the \
                 original game's folder on the first run on Windows.",
        why: "The client never shares a directory with a retail installation.",
    },
    Divergence {
        id: "CD-008",
        title: "More than one client can run at once",
        change: "Any number of clients can run side by side on one machine.",
        why: "Players ran several accounts at once, and the clients they used allowed it.",
    },
    Divergence {
        id: "CD-009",
        title: "Brightness changes the game's picture and nothing else",
        change: "The brightness curve is applied to the game's own picture, not to the whole \
                 display.",
        why: "The rest of the desktop is left as it was; in the game the picture is the same.",
    },
    Divergence {
        id: "CD-010",
        title: "The world of February 2005 under today's screens",
        change: "Given the February 2005 data files (beside the later ones, or in \
                 --classic-dat-dir) and an Infiltration world (--era infiltration), the client \
                 draws that world \
                 under the end-of-retail screens, leaving out the windows and buttons for \
                 systems the world lacks.",
        why: "Every era is played over one set of screens.",
    },
    Divergence {
        id: "CD-011",
        title: "Landscape Detail Textures draws the ground's detail texture",
        change: "With the option on (it is off by default), the ground's detail texture is drawn \
                 near the player, as clients before the final one drew it.",
        why: "The final client kept the option but no longer read it.",
    },
    Divergence {
        id: "CD-012",
        title: "The ground and the sky of any world are a live choice of three eras' styles",
        change: "Terrain Mode and Sky Mode choose the ground and the sky separately, for any \
                 world, from three eras' styles, taking effect on the next frame.",
        why: "Which era's look a world is seen in is a presentation choice.",
    },
    Divergence {
        id: "CD-013",
        title: "A terrain type the chosen region does not name is drawn as its neighbours",
        change: "A ground corner whose terrain type the region does not name is drawn as its most \
                 common named neighbour, not as a filler picture.",
        why: "The surrounding ground is the closest thing the region has.",
    },
    Divergence {
        id: "CD-014",
        title: "The objects of any world can take another era's look",
        change: "Object Mode draws any world's creatures, items, buildings and scenery in their \
                 Classic or Modern look, part by part, while collision and motion stay the world's.",
        why: "Which era's models a world is seen in is a presentation choice.",
    },
    Divergence {
        id: "CD-015",
        title: "The classic interface, chosen live",
        change: "The Interface option switches between the modern and the classic interface on \
                 the next frame, keeping the game, the preferences and each interface's key map.",
        why: "Both interfaces are offered over any world, sharing one set of options.",
    },
    Divergence {
        id: "CD-016",
        title: "The modern interface's Create Spell page",
        change: "On a world with spell research, the modern interface's magic window gains a \
                 Create Spell tab that works as the classic research page does.",
        why: "Spell research is offered in both interfaces.",
    },
    Divergence {
        id: "CD-017",
        title: "The character screen's message",
        change: "The server's character screen message is shown: in a floating Announcements \
                 window in the modern interface, and in the message box in the classic one.",
        why: "A world's message to its players is seen in both interfaces.",
    },
    Divergence {
        id: "CD-018",
        title: "The performance panel",
        change: "An option, or a key the player binds, shows a frame-rate and frame-time panel \
                 over either interface.",
        why: "A frame-rate panel that belongs to no interface.",
    },
    Divergence {
        id: "CD-019",
        title: "Every key that can be bound does something",
        change: "Show Cloak works, and this client's own saved bindings are answered by the modern \
                 interface and named in key-conflict prompts.",
        why: "A key that can be bound works when it is bound.",
    },
    Divergence {
        id: "CD-021",
        title: "One set of options pages for both interfaces",
        change: "Both interfaces show the same four options pages over one set of options, with \
                 dead rows gone, and the client starts in a window at 1024 by 768.",
        why: "Either interface's pages show what the other set.",
    },
    Divergence {
        id: "CD-022",
        title: "One set of key bindings for both interfaces, a key map for each",
        change: "Both interfaces' key pages list one set of bindings, each interface keeps its \
                 own key maps, and rows that do nothing are left off.",
        why: "One set of available keys, with defaults and saved maps per interface.",
    },
    Divergence {
        id: "CD-023",
        title: "The classic Keyboard Configuration page binds every key",
        change: "The classic page binds each stance's combat action and the formerly permanent \
                 keys, accepts Shift, Ctrl and Alt, and defaults to the game's 2004 key map.",
        why: "Every classic key can be bound.",
    },
    Divergence {
        id: "CD-024",
        title: "A censored word keeps the quotes around it",
        change: "With Filter Language on, a censored word becomes four asterisks and the \
                 punctuation around it stays, so the line keeps its quotes.",
        why: "Losing the quotes was a fault.",
    },
    Divergence {
        id: "CD-025",
        title: "The 3D view fills any screen",
        change: "The world view fills the whole screen at any resolution, where retail's stopped \
                 at 3000 by 2000.",
        why: "Retail's limit left the backdrop showing on large screens.",
    },
    Divergence {
        id: "CD-026",
        title: "Degrade Distance applies on every world",
        change: "The Degrade Distance setting moves where detail changes on worlds from before \
                 Throne of Destiny too, where the 2005 client had no such setting and chose \
                 detail from the distance alone.",
        why: "One setting that does the same thing on every world.",
    },
    Divergence {
        id: "CD-027",
        title: "Chat availability does not depend on an open menu",
        change: "Disabling the current chat destination falls back to Say even while its window or menu row is absent.",
        why: "Rebuilding or switching the interface cannot leave an unavailable destination selected.",
    },
    Divergence {
        id: "CD-028",
        title: "Vendor carts track completed transfers",
        change: "Completed purchases and sales remove only their fulfilled basket rows; unfulfilled and opposite-basket rows remain until explicitly cleared or the vendor session ends. Finite stock remains listed while it is only in the buy basket.",
        why: "A basket describes outstanding choices, and adding an item to it does not purchase the stock.",
    },
    Divergence {
        id: "CD-029",
        title: "Clear is handled as an unregistered chat command",
        change: "The client forwards @clear to the shard instead of accepting it as a local command, and text-management help no longer lists it. Existing chat text stays visible.",
        why: "An unsupported local command must not silently accept a request it cannot perform.",
    },
    Divergence {
        id: "CD-030",
        title: "Successful research clears the tested formula",
        change: "A confirmed successful formula test clears the formula in either interface, including a successful test of an already-known spell. Failed tests keep the formula.",
        why: "The completed formula leaves room for the next experiment while failures remain available for correction.",
    },
    Divergence {
        id: "CD-031",
        title: "A world's own data is kept in an overlay over the installed files",
        change: "Data a server sends during the update exchange is written into that world's \
                 overlay folder (--overlay-dat-dir, else one per server in the per-user cache, \
                 or in a browser one per server in its own storage), \
                 never into the installed data files; a purge hides records there rather than \
                 deleting them. With a server that sends the world's overlay manifest, an \
                 overlay made against other files, for a blocked world or for another world's \
                 folder is refused, and every record is checked against the manifest.",
        why: "One set of installed files serves every world, each world's changes stay its own, \
              and a server cannot change the files the player installed.",
    },
    Divergence {
        id: "CD-032",
        title: "Logging off tears the world down",
        change: "When the server answers a log-off, the world the character left is torn down at \
                 once: its scene, its objects and its sounds. The final client left it standing \
                 behind the character screen until the next character entered.",
        why: "Nothing of a world the player has left goes on being seen or heard.",
    },
    Divergence {
        id: "CD-033",
        title: "The classic paper doll's later slots open from one button",
        change: "On a world with cloaks, trinkets or aetheria, the classic interface's paper doll \
                 has an accessories button beside the shield. It opens a flyout over the doll \
                 holding each aetheria sigil the character has unlocked over the cloak and the \
                 trinket, as slots that work as the doll's own do. A trinket or an aetheria let \
                 go on the figure goes into its own slot; the final client's figure refused them \
                 and took them only on their own slots. The classic interface never had these \
                 slots.",
        why: "Later worlds' equipment can be worn and seen in the classic interface, and the \
              doll keeps its own layout.",
    },
    Divergence {
        id: "CD-034",
        title: "A world's systems reach the client as one shared bitfield",
        change: "--era-features takes the world's whole set of systems as the bitfield the \
                 server's status ping and the launcher use (the table's version, then the bits \
                 in hex), as well as by name. A system the client's table and the bitfield's do \
                 not both have is left to the era's table, never read as missing.",
        why: "The server, the launcher and the client agree on one form, and a client older or \
              newer than its server still reads the systems they share.",
    },
    Divergence {
        id: "CD-035",
        title: "The logon version is the world's",
        change: "--logon-version sets the version string the login request carries. The final \
                 client always sent 1802; without the switch this client does too.",
        why: "A world whose server accepts only its own client's string can be played with this \
              one.",
    },
    Divergence {
        id: "CD-036",
        title: "A world can be played by its own client's rules",
        change: "--world-profile names a set of rules compiled into the client, the ones a \
                 world's own client changed: burden, run and jump, which weapons rule out a \
                 shield, what an appraisal and some refusals say, and a few more. The final \
                 client had one set of these rules, hard-coded; without the switch this client \
                 plays by it.",
        why: "A character on such a world carries, moves and reads what that world's server \
              expects.",
    },
    Divergence {
        id: "CD-037",
        title: "Opt-in experimental rendering effects under Horizon",
        change: "Under the Horizon interface, six boxes on its options page (better lighting, \
                 real-time sun shadows, bounced light, ambient occlusion, lamps and torches, sky \
                 and atmosphere) draw the world a second time with modern light, on the desktop \
                 wgpu renderer. Everything the game reads still comes from the ordinary frame, \
                 which is recorded in full either way. Every box is off by default; the classic \
                 and modern interfaces never draw the effects, and the browser client does not \
                 contain them. They draw only on the wgpu renderer, chosen by the player; the \
                 client never picks it for them. Heavier frames lower the frame rate, and three \
                 things follow the frame time as they do on a slower machine: the automatic \
                 degrade, so detail \
                 levels and which far objects are drawn can shift; through the degrade, which \
                 objects at the edge of the drawing distance can be picked and selected; and the \
                 client's physics, which steps by the time a frame took, so the predicted \
                 positions the client reports are integrated in other steps.",
        why: "The 1999 art drawn with today's graphics, for a player who asks for it, without \
              changing how the game plays.",
    },
    Divergence {
        id: "CD-038",
        title: "The time of day this client draws can be set from chat",
        change: "/tod (or @tod) is a chat command of this client's own: alone it says the time of \
                 day the sky is drawn at and whether that is the server's clock; given a fraction \
                 of the day from 0 to 1, or midnight, dawn, noon, dusk or night, it draws the sky \
                 and its light from that hour on; reset returns to the server's clock. It moves \
                 only this client's sky clock, the offset the --time-of-day switch sets at start, \
                 and is never sent to the server. The final client had no such command.",
        why: "A player can look at the world by day or by night without waiting for the hour.",
    },
    Divergence {
        id: "CD-039",
        title: "Smooth Animation under Horizon",
        change: "Under the Horizon interface, a box on its options page (Smooth Animation, off by \
                 default) draws every animated body between its animation's keyframes, at any \
                 frame rate, where the final client steps each part from one keyframe to the next. \
                 The animations are advanced, fire their hooks and move the bodies as they do with \
                 it off, and what bodies collide with is posed at the keyframes either way; the \
                 classic and modern interfaces never draw it.",
        why: "Bodies move as smoothly as the rest of the picture at high frame rates, for a \
              player who asks for it, without changing how the game plays.",
    },
    Divergence {
        id: "CD-040",
        title: "A camera drag hides the pointer once the drag has moved it",
        change: "While the mouse turns the camera the pointer is hidden and held still as the \
                 final client held it, but only once the drag has moved it past the drag \
                 threshold of about four pixels; the final client hid it the moment mouse look \
                 began. A click of the button that turns the camera, which examines or \
                 selects, never hides it. The press that began a drag is never taken for the \
                 first click of a double-click: a click at once after the drag, where the \
                 pointer is shown again, is a single click.",
        why: "A click does not make the pointer blink, and the browser client does not ask to \
              lock the pointer for every click. A quick turn of the camera followed by a click \
              selects or examines and never uses what is under the pointer.",
    },
    Divergence {
        id: "CD-041",
        title: "Smooth Movement under Horizon",
        change: "Under the Horizon interface, a box on its options page (Smooth Movement, off by \
                 default) draws every other player, creature and moving object gliding between \
                 the physics steps that move it thirty times a second, where the final client \
                 draws it where its last step left it. Each is drawn a step behind, on the way \
                 physics carried it, never ahead of where physics has it; anything put somewhere \
                 rather than stepped there (a teleport, a portal, a recall, a correction too far \
                 to walk) is drawn where it lands at once. Where each object is, what it collides \
                 with and what is sent are the same either way; what follows the drawing follows \
                 it (what it holds, its parts' lights and sounds, and the parts a click is tested \
                 against, so a click selects what is drawn under the pointer). The classic and \
                 modern interfaces never draw it.",
        why: "Other characters and creatures move as smoothly as the rest of the picture at high \
              frame rates, for a player who asks for it, without changing how the game plays.",
    },
];

/// The rows that name `id` as their divergence, in registry order.
pub fn rows_of(id: &str) -> impl Iterator<Item = &'static Behaviour> + '_ {
    behaviours::all().filter(move |b| b.divergence == Some(id))
}

/// Look one divergence up by id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|d| d.id == id)
}

/// Where the published list lives, relative to this crate's manifest.
pub const PUBLISHED: &str = "../DIVERGENCES.md";

/// The published list, as Markdown.
#[must_use]
pub fn render() -> String {
    let mut out = String::new();
    for paragraph in [
        "# Deliberate divergences from the retail client".to_owned(),
        "The Dereth client rebuilds the retail Asheron's Call client, and this is the list of \
         places where it deliberately behaves differently. Each row is a choice: the retail \
         behaviour is known, and this client does something else on purpose. Defects and \
         features not yet built are not listed, and neither is anything this client reproduces \
         from retail on purpose, however odd. The code names a row by its id (`CD-001`). The \
         server's own list, against ACE, is \
         [`empyrean/DIVERGENCES.md`](../empyrean/DIVERGENCES.md)."
            .to_owned(),
        "Each row names the behaviour-registry rows that describe it and the test that asserts \
         each one, as a test path whose last parts can be passed to `cargo test` as a filter."
            .to_owned(),
        format!(
            "This file is generated from the register in `dereth-testkit` \
             (`src/divergences.rs`) and the registry rows that name each id; edit those, not \
             this file, and rewrite it with \
             `{}=1 cargo test -p dereth-testkit --lib divergences`.",
            crate::golden::REWRITE_VAR
        ),
    ] {
        out.push_str(&paragraph);
        out.push_str("\n\n");
    }
    out.push_str("| id | divergence | why | behaviour rows / tests |\n");
    out.push_str("|---|---|---|---|\n");
    for d in DIVERGENCES {
        let rows: Vec<String> = rows_of(d.id)
            .map(|b| format!("`{}`: `{}`", b.id, b.station))
            .collect();
        out.push_str(&format!(
            "| {} | **{}.** {} | {} | {} |\n",
            d.id,
            d.title,
            cell(d.change),
            cell(d.why),
            rows.join("<br>")
        ));
    }
    out
}

/// One table cell: a pipe would end the cell early.
fn cell(s: &str) -> String {
    s.replace('|', "\\|")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::behaviours::{RETAIL, THIS_CLIENT, TOOLING};
    use std::collections::BTreeSet;
    use std::path::Path;

    /// **A claim read off this client's own rule is a published divergence**, and only such a
    /// claim is: a `since: this client` row with no divergence would be a departure from retail
    /// that the public list leaves out, and a retail row naming one would put a retail behaviour
    /// on the list of differences. A tooling row is neither, and names none.
    #[test]
    fn a_row_names_a_divergence_exactly_when_it_is_read_off_this_client() {
        for b in behaviours::all() {
            assert_eq!(
                b.since == THIS_CLIENT,
                b.divergence.is_some(),
                "{}: since {:?} and divergence {:?} -- a row read off this client names the \
                 divergence it is part of, and no other row names one",
                b.id,
                b.since,
                b.divergence
            );
            assert!(
                [RETAIL, THIS_CLIENT, TOOLING].contains(&b.since),
                "{}: since {:?} is none of the three stamps",
                b.id,
                b.since
            );
            if b.since == TOOLING {
                assert!(
                    b.divergence.is_none(),
                    "{}: a tooling row is not published, so it names no divergence",
                    b.id
                );
            }
            if let Some(id) = b.divergence {
                assert!(
                    lookup(id).is_some(),
                    "{} names the divergence {id}, which the register does not hold",
                    b.id
                );
            }
        }
    }

    /// **A divergence nothing asserts is a claim with no test**, so every one is named by at least
    /// one row, and the ids are `CD-` and three digits, unique and in order with no gap but the
    /// withdrawn numbers.
    #[test]
    fn every_divergence_is_numbered_in_order_and_named_by_a_row() {
        let mut seen = BTreeSet::new();
        let mut expected = (1..)
            .map(|n| format!("CD-{n:03}"))
            .filter(|id| !WITHDRAWN.contains(&id.as_str()));
        for d in DIVERGENCES {
            assert_eq!(
                Some(d.id.to_owned()),
                expected.next(),
                "the register is not numbered in order from CD-001, less the withdrawn numbers"
            );
            assert!(seen.insert(d.id), "{} is listed twice", d.id);
            assert!(
                rows_of(d.id).next().is_some(),
                "{} is named by no behaviour row, so nothing asserts it",
                d.id
            );
        }
    }

    /// The register is public text, held to the rule the registry's sentences are: behaviour a
    /// player would recognise, with no address and no internal name.
    #[test]
    fn no_divergence_carries_a_retail_citation_or_an_internal_name() {
        for d in DIVERGENCES {
            for (what, text) in [("title", d.title), ("change", d.change), ("why", d.why)] {
                assert!(
                    text.len() > 20,
                    "{}'s {what} is too short to say anything",
                    d.id
                );
                for bad in ["0x", " @ ", "::", "`", "|", "\n"] {
                    assert!(
                        !text.contains(bad),
                        "{}'s {what} carries {bad:?}: {text:?}",
                        d.id
                    );
                }
            }
        }
    }

    /// **The published list is what the register generates.** A changed row, a new divergence or
    /// a moved test changes the file, and the change is a diff to read; with the golden rewrite
    /// variable set the file is rewritten instead.
    #[test]
    fn the_published_list_is_what_the_register_generates() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join(PUBLISHED);
        let want = render();
        if crate::golden::rewriting() {
            std::fs::write(&path, want.as_bytes())
                .unwrap_or_else(|e| panic!("rewriting {}: {e}", path.display()));
            return;
        }
        let have = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("{} did not load: {e}", path.display()))
            .replace("\r\n", "\n");
        assert!(
            have == want,
            "{} is not what the register generates; rewrite it with {}=1 and read the diff",
            path.display(),
            crate::golden::REWRITE_VAR
        );
    }
}
