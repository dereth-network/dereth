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
        id: "CD2-a-bodys-reach-is-worked-out-each-frame",
        says: "How far away a body is drawn is worked out fresh each frame. The original keeps the \
               reach of the body's last draw, and at very high frame rates its adaptive scale can \
               fall to its floor and stay there, so a body past 20 m isn't drawn again until the \
               camera comes closer.",
    },
    Divergence {
        id: "CD2-assistance-opens-issues-page",
        says: "Request Assistance opens this project's issues page; the original client opened the \
               publisher's support site, which is gone. The box shown when the browser cannot be \
               started is titled Auberean Error, where the original's named Asheron's Call.",
    },
    Divergence {
        id: "CD2-checked-script-machine",
        says: "Where the retail script machine would run past the end of a stack, a table or the \
               code (a native nothing is bound to, a stack overflow, a reference to nothing, an \
               integer division by zero), this one stops the script cleanly and says why instead \
               of crashing the game or going on with garbage.",
    },
    Divergence {
        id: "CD2-craft-page-list-takes-a-tall-window",
        says: "On a window taller than 768, a craft skill's page keeps its head at its own height \
               and its display at the page's foot, and the list takes the rest, so rows can be \
               clicked; the retail client stretches the head over the list, where it takes the \
               clicks meant for the rows.",
    },
    Divergence {
        id: "CD2-data-cache-worker-in-frame",
        says: "The data the game loads in the background is read and stored at the end of each \
               frame, everything waiting at once, where the original client does it on a thread \
               of its own and finishes at most 25 milliseconds of it each frame.",
    },
    Divergence {
        id: "CD2-data-files-from-one-folder",
        says: "The data files are opened from the one install folder named; the first region's \
               cell file is always used, and a language with no file falls back to English.",
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
               connection. A room's chat goes only through the chat service: against a server that \
               says it has none, nothing is sent and the script says it failed, where the original \
               sends it through the friends service.",
    },
    Divergence {
        id: "CD2-full-screen-is-borderless",
        says: "Full screen is a borderless window over the whole monitor and never changes the \
               machine's display mode; the display's size says what the picture is drawn at, and \
               it is scaled to the monitor. The original client took the monitor to a mode of \
               that size.",
    },
    Divergence {
        id: "CD2-full-screen-keeps-its-shape",
        says: "In full screen at a shape other than the monitor's, the picture keeps its shape in \
               the middle with black bars around it. The original client's display mode filled \
               the monitor, so the picture was stretched.",
    },
    Divergence {
        id: "CD2-help-buttons-do-not-show",
        says: "The panels have no help button: the retail client's opens a help window this \
               client does not have.",
    },
    Divergence {
        id: "CD2-input-method-is-always-offered",
        says: "Wherever you can type, your system's input method is offered. The original client \
               offered it only with its preference on or a command line switch.",
    },
    Divergence {
        id: "CD2-lists-iterations-it-need-not-hold",
        says: "Asked which data it holds, the client also lists the portal file iterations a server \
               gives what it adds for a retail client's script calls, which this client doesn't \
               need, so an admin's login isn't held up for minutes downloading them.",
    },
    Divergence {
        id: "CD2-mac-window-goes-by-points",
        says: "On macOS a window's size is in points, and the picture is drawn at that size and \
               scaled to fill the window, so a Retina screen shows it as large as any other. The \
               original client ran only where a window's size is in the screen's pixels.",
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
               refused. Scripts find it at once, except a move's answer asking for the item it moves, which finds it from the next upkeep.",
    },
    Divergence {
        id: "CD2-placed-boxes-hold-the-animation",
        says: "Seen through a doorway, a placed object that moves is left out only when the box \
               about its pose is out of sight. The original's box for such an object stands where \
               its mesh is stored, however far its animation takes it.",
    },
    Divergence {
        id: "CD2-qualities-dump-answers",
        says: "Asked to write out an object's own qualities without its record's, this client \
               writes them once; the original never answers, repeating them until it runs out of \
               memory.",
    },
    Divergence {
        id: "CD2-restore-sent-in-the-clear",
        says: "Bringing back a deleted character goes to the server as a plain message on the \
               login queue; the original client sealed it on a queue of its own under keys only \
               the original servers held.",
    },
    Divergence {
        id: "CD2-screenshot-is-the-window",
        says: "A screenshot is the window as it stands, interface included, saved at the window's \
               size. The original drew the world again for it, in as many tiles as its two \
               preferences asked (up to 10 by 10), and saved a JPEG in its own folder.",
    },
    Divergence {
        id: "CD2-script-sends-in-the-clear",
        says: "A script call the game marks for encryption goes to the server as a plain script \
               call, as every other does; the original client wrapped such calls under keys only \
               the original servers held.",
    },
    Divergence {
        id: "CD2-script-step-budget",
        says: "Every script run has a step budget: one that runs too long is stopped instead of \
               freezing the game, where the retail client would hang.",
    },
    Divergence {
        id: "CD2-several-clients-at-once",
        says: "Several clients can run at once on one machine. The original refused a second \
               copy with an error dialog and closed.",
    },
    Divergence {
        id: "CD2-stops-on-the-disconnected-screen",
        says: "A record the server sends that the client cannot store ends the session on the \
               disconnected screen, which shows the client's line (it failed to save data) and \
               whose button leaves the game; the original client shows that line in an error box \
               and the program ends at once.",
    },
    Divergence {
        id: "CD2-the-window-can-be-any-size",
        says: "The game's window can be resized by hand, and the display's size preference then \
               holds its size; the original client's window had no resizing frame. A first start \
               is a window with antialiasing on, where the original's configuration program \
               chose full screen and, below the top quality, no antialiasing.",
    },
    Divergence {
        id: "CD2-wheel-scrolls-the-window",
        says: "The wheel over any part of a window turns its nearest scroll bar; the retail client scrolls only when the pointer is on a list, a text or a bar.",
    },
    Divergence {
        id: "CD2-window-title-names-the-account",
        says: "The window's title names the account the player logged in with (`Auberean | kara`), \
               as Dereth's does, so several clients open at once can be told apart on the task \
               bar; the retail client's title is the game's name alone.",
    },
    Divergence {
        id: "CD2-world-overlay",
        says: "A world can change the game's records with a folder of its own laid over the data                files, which this client reads through and never writes; the original client knew                only its own files, which a download changed in place.",
    },
];

/// The divergence with an id.
#[must_use]
pub fn lookup(id: &str) -> Option<&'static Divergence> {
    DIVERGENCES.iter().find(|d| d.id == id)
}
