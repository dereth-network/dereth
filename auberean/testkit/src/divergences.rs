//! The places this client deliberately differs from retail. Each is named, never numbered:
//! `CD2-` and a short lowercase name joined by hyphens (`CD2-wider-view-distance`). A behaviour row
//! stamped `THIS_CLIENT` names the divergence it is part of.

/// One deliberate difference from retail.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Divergence {
    pub id: &'static str,
    /// What this client does differently, in words a player would recognize.
    pub says: &'static str,
}

/// Every divergence, in id order.
pub static DIVERGENCES: &[Divergence] = &[
    Divergence {
        id: "CD2-checked-script-machine",
        says: "Where the retail script machine would run past the end of a stack, a table or the \
               code (a native nothing is bound to, a stack overflow, a reference to nothing, an \
               integer division by zero), this one stops the script cleanly and says why instead \
               of crashing the game or going on with garbage.",
    },
    Divergence {
        id: "CD2-data-cache-worker-in-frame",
        says: "The data the game loads in the background is read and stored at the end of each \
               frame, everything waiting at once, where the original client does it on a thread \
               of its own and finishes at most 25 milliseconds of it each frame.",
    },
    Divergence {
        id: "CD2-data-read-again",
        says: "A record the data files hold that fails to read is read again before it counts as missing, so one bad read on a busy machine does not lose it for the rest of the session.",
    },
    Divergence {
        id: "CD2-files-stay-in-settings-folder",
        says: "The chat log and exported options are kept in the settings folder, by a bare name \
               only; the original client wrote them beside itself, or wherever a path named.",
    },
    Divergence {
        id: "CD2-files-written-in-memory",
        says: "Records the server sends while the game runs are stored in the data files as the \
               client stores them, but in memory over files only read, for the session: the files \
               on disk are never changed.",
    },
    Divergence {
        id: "CD2-friends-stream",
        says: "Friends, presence, the ignore list, the away state and tells to other worlds reach \
               the world over this network's own friends stream, with its own greeting, ticket, \
               keep-alive and waits between reconnects, instead of the original friends service's \
               connection.",
    },
    Divergence {
        id: "CD2-math-last-bit",
        says: "The scripts' sine, cosine, tangent, arc sine, arc cosine, arc tangent, logarithms \
               and exponential come from this client's own functions, which can differ from the \
               original's floating-point unit in the last bit; the cases that unit decides are \
               kept: a sine, cosine or tangent of a number from 2 to the 63rd is the number \
               itself, an invalid operation answers the unit's own not-a-number, the exponential \
               rounds its scaled argument as the unit does, and a quotient is mapped into -1 to 1 \
               with the original's own two over pi.",
    },
    Divergence {
        id: "CD2-newer-incarnation-keeps-what-waited",
        says: "What the server sends about a newer copy of an object before creating it is kept for \
               that copy when it replaces an older one the client still has; the original client \
               throws it away with the older copy. The original had mostly let older copies go \
               25 seconds after they left the world, which this client does only when a scene \
               places the bodies.",
    },
    Divergence {
        id: "CD2-normalize-as-on-sse2",
        says: "Making a vector or a ray's direction one long gives the answer the original's math \
               library gave on a processor with SSE2, starting from the exact reciprocal square \
               root; the original's answer varied in the last bits from one processor to another, \
               as each estimates that root its own way.",
    },
    Divergence {
        id: "CD2-number-limits-decided-exactly",
        says: "Whether a number a script reads from text is past the largest double or below the \
               least normal one is decided exactly from its first 21 digits; the original worked \
               it out in 80-bit arithmetic, which can answer the other way for a number within \
               about one part in 2 to the 64th of either limit.",
    },
    Divergence {
        id: "CD2-objects-made-before-their-records",
        says: "An object the server creates is made at once, and the records it is made from load \
               behind it; the original client makes it only once they have all loaded or been \
               refused.",
    },
    Divergence {
        id: "CD2-qualities-dump-answers",
        says: "Asked to write out an object's own qualities without its record's, this client \
               writes them once; the original never answers, repeating them until it runs out of \
               memory.",
    },
    Divergence {
        id: "CD2-script-step-budget",
        says: "Every script run has a step budget: one that runs too long is stopped instead of \
               freezing the game, where the retail client would hang.",
    },
    Divergence {
        id: "CD2-stops-on-the-disconnected-screen",
        says: "A record the server sends that the client cannot store ends the session on the \
               disconnected screen, which shows the client's line (it failed to save data) and \
               whose button leaves the game; the original client shows that line in an error box \
               and the program ends at once.",
    },
    Divergence {
        id: "CD2-window-title-names-the-account",
        says: "The window's title names the account the player logged in with (`Auberean | kara`), \
               as Dereth's does, so several clients open at once can be told apart on the task \
               bar; the retail client's title is the game's name alone.",
    },
];

/// The divergence with an id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|d| d.id == id)
}
