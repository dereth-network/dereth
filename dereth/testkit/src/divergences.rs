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
    /// What the retail client does.
    pub retail: &'static str,
    /// What this client does instead.
    pub dereth: &'static str,
    /// Why.
    pub why: &'static str,
}

/// Every divergence, in id order.
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "CD-001",
        title: "A resolution change keeps the window where it is",
        retail: "A new resolution picked while windowed, and the forced resolution change on \
                 logging out and in, resize the window about its old centre, so every size change \
                 moves it, and then pull it inside the desktop's usable area. Leaving full screen \
                 centres the window on the screen.",
        dereth: "The window's top-left corner stays where it was and only its size changes, and \
                 leaving full screen puts the window back with its top-left where it was before \
                 it went full screen. It moves only as far as it must to stay inside the usable \
                 area of its monitor, task bar excluded.",
        why: "A player who has put the window somewhere keeps it there; neither a resolution \
              change nor a trip to full screen and back is a request to move it.",
    },
    Divergence {
        id: "CD-002",
        title: "Full screen is a borderless window over the whole monitor",
        retail: "Full screen takes the display over in exclusive mode at the chosen resolution, \
                 switching the monitor's display mode, and places the picture at the desktop's \
                 origin.",
        dereth: "Full screen is a window with no frame covering the monitor the window is on, at \
                 that monitor's own size; the display mode is never changed, so the resolution \
                 setting does not apply in full screen. Sync to the display refresh still applies \
                 only in full screen with the sync preference on.",
        why: "A window that never changes the display mode leaves the rest of the player's \
              desktop, other monitors and overlays alone, and needs no mode switch to leave. At \
              the desktop's own resolution, the usual case, the picture is the same.",
    },
    Divergence {
        id: "CD-003",
        title: "Full screen does not float over other windows",
        retail: "The window asks to be kept in front of every other window when it is created and \
                 again on every switch to full screen, so it floats over everything even after \
                 switching away.",
        dereth: "The window is never kept on top, windowed or full screen: switching away really \
                 leaves it behind.",
        why: "Floating was needed only because the original's full screen borrowed the display \
              mode; with a borderless full screen it only gets in the player's way.",
    },
    Divergence {
        id: "CD-004",
        title: "A resolution is real screen pixels at any desktop scaling",
        retail: "The client does not declare itself aware of desktop scaling, so on a scaled \
                 monitor the desktop enlarges the whole window: at 105 per cent scaling a \
                 1920 by 1080 client is drawn larger than 1920 by 1080.",
        dereth: "The client is aware of each monitor's scaling. The configured resolution is the \
                 window's real pixels; a scaling change keeps the configured size windowed, or the \
                 monitor's own extent in full screen, and never rewrites the resolution setting.",
        why: "A resolution the player picks should be exactly that many pixels on the screen; a \
              window the desktop enlarges no longer fits the monitor it was sized for.",
    },
    Divergence {
        id: "CD-005",
        title: "Full screen applies in the world only",
        retail: "Full screen is a display mode held for the life of the process: a client set \
                 to full screen is full screen from the patch screen on, and the switch key \
                 toggles it on any screen.",
        dereth: "The screens before the world, from the patch screen to the character list, are \
                 always a window. The full-screen setting is kept, applied on entering the world \
                 and taken away on leaving it, and the switch key is refused outside the world.",
        why: "A borderless window cannot cover the whole screen on every platform before the \
              world starts, and on the way into the world full screen would fight the forced \
              change back from the pre-game resolution. A switch the next screen change would \
              silently undo is worse than one that does nothing.",
    },
    Divergence {
        id: "CD-006",
        title: "The window title names the account",
        retail: "The window's title is the game's name, fixed for the life of the process.",
        dereth: "The title is the client's name and the account it logged in with, spelled as \
                 typed, or the client's name alone when there is no account.",
        why: "More than one client is often open on one machine, for example two accounts \
              trading, and the task bar is the only place that tells them apart.",
    },
    Divergence {
        id: "CD-007",
        title: "The client's files live in a folder of its own",
        retail: "Preferences, key maps and the other files go in the game's own folder under \
                 Documents, the one a retail installation uses, and the key map a player has \
                 not named is called after the running program, acclient.keymap.",
        dereth: "They go in a folder named for this client: Dereth's client folder in the \
                 roaming application data on Windows, and in the platform's own settings place \
                 on macOS and other Unix systems. On Windows the first run copies the original \
                 game's folder's contents across. The key map a player has not named is always \
                 dereth.keymap, whatever the program file is called.",
        why: "This client's files are not the original's, and a folder of its own keeps them \
              from sharing a directory with a retail installation. A key map named after the \
              program would be lost whenever the program is renamed or copied under another \
              name.",
    },
    Divergence {
        id: "CD-008",
        title: "More than one client can run at once",
        retail: "The client refuses to start while another copy is running on the same machine, \
                 saying that a client is already running and that two cannot run on one machine.",
        dereth: "Any number of clients can run side by side on one machine, each with its own \
                 window and its own account.",
        why: "Players ran several accounts at once, trading between their own characters or \
              keeping a second character company, and the copies of the client they used in the \
              end had the refusal taken out; a rebuild that restored it would take that away.",
    },
    Divergence {
        id: "CD-009",
        title: "Brightness changes the game's picture and nothing else",
        retail: "The brightness setting reshapes the display's own colour curve, so it acts on \
                 everything the monitor shows: in a window it can brighten or darken the rest of \
                 the desktop along with the game.",
        dereth: "The same brightness curve is applied to the game's own picture as it is drawn, \
                 windowed or full screen, and the rest of the desktop and other windows are left \
                 exactly as they were.",
        why: "Changing the brightness of the player's whole desktop is a side effect nobody asked \
              for, and modern graphics interfaces no longer offer a display curve to a windowed \
              program. Inside the game the picture is the same.",
    },
    Divergence {
        id: "CD-010",
        title: "The world of February 2005 under today's screens",
        retail: "The client of February 2005 read its two data files, portal.dat and cell.dat, \
                 and drew its own interface over that world.",
        dereth: "Given those two files beside the end-of-retail ones (--world-dat-dir), the \
                 client draws the February 2005 world from them and the end-of-retail screens \
                 over it: the interface's layouts, strings, fonts and images come from the later \
                 files, and a record the older files lack is read from them too.",
        why: "Dereth plays every era over one set of screens until the interface of the time is \
              rebuilt; the world itself is the older files' own.",
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
    out.push_str("| id | what retail does | what Dereth does | why | behaviour rows / tests |\n");
    out.push_str("|---|---|---|---|---|\n");
    for d in DIVERGENCES {
        let rows: Vec<String> = rows_of(d.id)
            .map(|b| format!("`{}`: `{}`", b.id, b.station))
            .collect();
        out.push_str(&format!(
            "| {} | **{}.** {} | {} | {} | {} |\n",
            d.id,
            d.title,
            cell(d.retail),
            cell(d.dereth),
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
    /// one row, and the ids are `CD-` and three digits, unique and in order with no gap.
    #[test]
    fn every_divergence_is_numbered_in_order_and_named_by_a_row() {
        let mut seen = BTreeSet::new();
        for (n, d) in DIVERGENCES.iter().enumerate() {
            assert_eq!(
                d.id,
                format!("CD-{:03}", n + 1),
                "the register is not numbered in order from CD-001"
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
            for (what, text) in [
                ("title", d.title),
                ("retail", d.retail),
                ("dereth", d.dereth),
                ("why", d.why),
            ] {
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
