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
                 over it: the interface's layouts, strings, fonts and its own pictures (panel \
                 frames, buttons, bars) come from the later files even where the older files \
                 hold a picture under the same id, while the pictures the world names (item, \
                 spell, skill and component icons) and any other record are the older files', \
                 read from the later ones only where the older lack them. Asked \
                 which \
                 iterations its data files hold (the DDD interrogation), it answers \
                 for the world it draws: the older portal and cell files, whose iteration is in \
                 their headers, as one run of it (2112 and 1593), and the later language file's \
                 own list. The older physics script tables are read in the later numbering of \
                 script types (those from 30 on were one lower then), so the scripts the later \
                 client plays by type, the materialize at login among them, find their rows; \
                 their clothing tables' dye ranges, which counted the colours of a 256-colour \
                 palette, are read in the later count of eight entries a colour; and their \
                 item icons, which carry no alpha, are transparent where they are pure black, as \
                 the later files' copies of the same icons are (their other images keep black \
                 opaque). The screens leave out what the \
                 world lacks (the systems the server announces, over the table of its announced \
                 era, else of the era the files imply): with no journal there is no journal \
                 button and no quest page, with no contract tracker no Contracts tab, with no \
                 titles no Titles tab, with no cloaks or trinkets no cloak or trinket slot on the \
                 paper doll, with no luminance no luminance section on the character sheet, with \
                 no housing no House tab on the map page and no house purchase window, and with \
                 no trade, tinkering or chess no secure-trade, salvage or chess window. A \
                 toolbar button hidden this way leaves no gap: the buttons after it in its row \
                 move up into its place.",
        why: "Dereth plays every era over one set of screens until the interface of the time is \
              rebuilt; the world itself is the older files' own.",
    },
    Divergence {
        id: "CD-011",
        title: "Landscape Detail Textures draws the ground's detail texture",
        retail: "The end-of-retail client keeps the Landscape Detail Textures preference in the \
                 profile but never reads it, so no detail texture is ever drawn over the ground. \
                 The clients before it, from at least February 2005 through 2012, read it, and \
                 the 2012 client's medium and higher graphics quality settings turned it on.",
        dereth: "With Render.LandscapeDetailTextures on, the region's landscape detail texture is \
                 drawn over the ground of the blocks around the player, as those earlier clients \
                 drew it: repeated across each square as often as the region says (four times at \
                 the end of retail), strongest within 10 metres of the eye and gone by 50. It is \
                 off by default, so a default profile draws what the end-of-retail client draws.",
        why: "The texture is in the data and the earlier clients drew it; the preference was \
              left behind when the end-of-retail client stopped reading it, so it is the \
              player's choice here.",
    },
    Divergence {
        id: "CD-012",
        title: "The ground and the sky of any world are a live choice of three eras' styles",
        retail: "A client from before Throne of Destiny drew its world with one of the two regions \
                 its files carry, and the choice followed its renderer: drawing with 3D hardware it \
                 used the region whose ground blends textures (with detail textures and a fuller \
                 sky), and drawing in software the one whose ground recolours a few shared ground \
                 pictures for each square, with a sky and light of its own. The end-of-retail \
                 client has one region. No client drew a world with another era's ground or sky, \
                 and none changed either while running.",
        dereth: "Terrain Mode and Sky Mode on the client options page (Render.Ground and \
                 Render.Sky in the profile) choose the ground and the sky separately, for any \
                 world: Palette Shift and Legacy Software are the older software region's, Legacy \
                 Blend and Legacy Hardware the older hardware region's, Modern Blend and Modern the \
                 end-of-retail region's. World Default is the world's own: the hardware region for \
                 an older world, as those clients drew on 3D hardware, and the end-of-retail region \
                 for the end-of-retail world. The terrain numbering is the same in every region, so \
                 each square takes the chosen style's texture for its own terrain; the world's \
                 land, scenery, buildings and sounds never change. A change takes effect on the \
                 next frame, rebuilding the ground or the sky in place. The detail textures, when                  turned on, are drawn under every ground style: the style's own where its region                  names them, else the world's own region's, else the end-of-retail region's. An older world's later \
                 styles come from the end-of-retail files beside it; the end-of-retail world's \
                 older styles from a folder holding an older portal.dat, given with \
                 --legacy-dat-dir or Render.LegacyDatDir, which only presentation reads. A style whose \
                 files are not there is refused with a message in the chat window, and the world \
                 keeps what it had.",
        why: "Every era can be played with the ground and sky its players saw on 3D hardware, the \
              software renderer's, or the end-of-retail ones; which one is a presentation choice, \
              not a rule of the era.",
    },
    Divergence {
        id: "CD-013",
        title: "A terrain type the chosen region does not name is drawn as its neighbours",
        retail: "A region's land surface lists a texture for every terrain number up to 31, but a \
                 number past the region's own terrain list is a filler there, another type's \
                 picture: the regions before Throne of Destiny name 27 types in 1999 and 31 from \
                 Dark Majesty on, and draw an unnamed type with Argila's tile (blended ground) or \
                 Semi-Barren Rock's colours (recoloured ground). The 1999 world uses type 31 for a \
                 road grid laid over deep sea; the end-of-retail region names type 31 (Desolate \
                 Lands) and its world uses it at three spots.",
        dereth: "Where a square's corner has a terrain type the region drawing the ground does not \
                 name, that corner is drawn as the terrain most common among its neighbours that \
                 the region does name, so the ground around it continues across it. Roads, water, \
                 heights and scenery are unchanged.",
        why: "A filler picture is not the ground that belongs there; the surrounding ground is \
              the closest thing the region has.",
    },
    Divergence {
        id: "CD-014",
        title: "The objects of any world can take another era's look",
        retail: "Every client drew its world's objects with its own data files: the players, \
                 creatures, items, buildings and scenery of a world from before Throne of \
                 Destiny in the older models and pictures, those of the end-of-retail world in \
                 the repainted later ones. No client drew one era's world with another era's \
                 objects.",
        dereth: "Object Mode on the client options page (Render.Objects in the profile, \
                 --object-visuals on the command line) draws any world's objects in World \
                 Default, Legacy or Modern look. Every object keeps the world's own parts, how \
                 they are joined and how they move; each part is drawn wholly in one era's look: \
                 the chosen era's model, surfaces, pictures and palettes where that era holds the \
                 same model, the world's own otherwise, never one era's model with the other's \
                 paint. A bare arm or hand is drawn as the chosen era's bare arm or hand, and a \
                 head with a character-creation hair style as that era's head for the style in \
                 the same place of its list, where it has one. A colour the chosen era lacks is \
                 its colour from the same place in the same choice of colours; a dye with no such \
                 colour is left off the chosen era's part, which keeps its own colour there. A \
                 part kept in the world's look beside parts in the chosen look takes the chosen \
                 era's colours where that era has them, so a later head on an older body has the \
                 older body's skin. A creature that \
                 era built with other models is drawn with them, moved by the world's \
                 animations, where each rests where the world's part rests. A \
                 clothing or paint change the world's server sends is put on the same surface of \
                 the other era's part, since the eras painted the same parts with different \
                 pictures; a part whose change has no such surface keeps the world's look. The \
                 paper doll wears the body's look. The landscape's statics and scenery are drawn \
                 whole in the chosen look where that era holds them as the same objects, and \
                 buildings where their shape is the world's. An interior is drawn room by room \
                 from the chosen era's own record of it where that era has the same room in the \
                 same place, with that era's paint and the world's furniture, unless its \
                 building keeps the world's look; other rooms stay the world's. The chosen \
                 era's rooms come from its cell file beside the world (cell.dat in the legacy \
                 folder, the end-of-retail cell file beside an older world). Collision, motion \
                 and everything the server agrees on stay the world's. A change takes effect on \
                 the next frame. Which of the other era's records are the same objects is worked \
                 out once per set of data files, a few milliseconds a frame from the moment the \
                 client starts, and kept; a look chosen before that is done keeps the objects as \
                 they are, says so in the chat window, and is drawn the frame it is ready. An \
                 older world's Modern look comes from the end-of-retail files \
                 beside it; the end-of-retail world's Legacy look from the folder given with \
                 --legacy-dat-dir or Render.LegacyDatDir. A look whose files are not there is \
                 refused with a message in the chat window.",
        why: "Which era's models and pictures a world is seen in is a presentation choice, not a \
              rule of the era, and the owner asked for both directions.",
    },
    Divergence {
        id: "CD-015",
        title: "The classic interface, chosen live",
        retail: "Each client had one interface: the clients before Throne of Destiny the one the \
                 game had before its 2005 redesign, drawn from their own portal.dat, and the \
                 end-of-retail client the redesigned one.",
        dereth: "Interface on either interface's client options page (UI.Interface in the \
                 profile) chooses Retail or Classic, and the client switches on the next frame, \
                 at the character screen or in the world. The classic interface draws its pictures and creation \
                 tables from the early-2005 portal.dat (the world's own on a world of that era, \
                 else the one in the folder given with --legacy-dat-dir or Render.LegacyDatDir) \
                 and its text with the system's fonts; without either it is refused with a \
                 message in the chat window and the retail interface stays. A switch keeps the \
                 game: the character, the selection and the world are the same, each interface \
                 keeps its own windows, and the chat lines of the last while are handed to the \
                 interface switched to; the journal is one file, written by the interface \
                 switched from and read again by the one switched to. Both interfaces' options \
                 pages edit the same preferences in the profile (the classic interface's own \
                 six, its inverted vertical mouse look, right-click mouse look (on at first) and \
                 stretched layout and whether its social window shows the Secure Trade page \
                 (off at first, greyed on worlds without trade), the Friends page and the \
                 Squelch page (on at first), are UI.Classic.*), and both bind keys in one key map \
                 file: each brings its own default scheme, and the player's keys lay over \
                 either, those held with Shift, Ctrl or Alt included.",
        why: "The owner asked for both interfaces over any world, switchable while playing, as \
              the landscape and object looks are, and for one set of options and keys behind \
              them.",
    },
    Divergence {
        id: "CD-016",
        title: "The retail interface's Create Spell page",
        retail: "Spell research had its own page in the magic window of the clients up to early \
                 2002, the formula laid from carried components and tested on a target. The \
                 end-of-retail client's magic window has only its Spells and Components tabs, \
                 and no client after the research page was removed could research a spell.",
        dereth: "On a world with spell research the retail interface's magic window gains a \
                 Create Spell tab beside Spells and Components, built by the client from the \
                 window's own tab, headings, buttons and scrollbar: a formula row of eight slots, \
                 a grid of the carried components and Test and Clear. It works as the classic \
                 interface's research page does: a component is laid by a double click or by \
                 dragging it from the grid or the pack onto the formula, which shows the item \
                 slot's drag hint. A world without spell research shows the shipped two tabs.",
        why: "The owner asked for spell research in the retail interface too, built in code \
              since the interface's data files are not changed.",
    },
    Divergence {
        id: "CD-017",
        title: "The character screen's message",
        retail: "Clients before Throne of Destiny showed the server's character screen message \
                 in the character screen's message box. The end-of-retail client has \
                 no such message and ignores it.",
        dereth: "The client reads the message, and the retail interface shows it on the \
                 character screen in a floating chat window right of the Create Character \
                 button, titled Announcements, with a scrollbar, a close button and no input \
                 row; the mouse wheel scrolls it. The classic interface shows it in its message \
                 box, as the clients of its era did.",
        why: "The owner asked for a world's message to its players to be seen in both \
              interfaces.",
    },
    Divergence {
        id: "CD-018",
        title: "The performance panel",
        retail: "The end-of-retail client shows no frame rate. Its benchmark and debug overlays \
                 have actions but no key and no handler.",
        dereth: "The Performance Panel option (Debug.PerformancePanel in the profile, a check \
                 box on the client options page), or a key the player binds to it \
                 (none is bound at first), shows a panel in the top left of the game: the frame rate, the mean and longest frame \
                 time over the last frames, and how long the input and network, interface, \
                 world, drawing and pacing parts of a frame take. The client draws it itself, \
                 over either interface and on any screen.",
        why: "The owner asked for a frame-rate panel that belongs to no interface.",
    },
    Divergence {
        id: "CD-019",
        title: "Every key that can be bound does something",
        retail: "The end-of-retail client lets a key be bound to Show Cloak and does nothing \
                 with it, and its key page lists only the actions its string table names.",
        dereth: "Show Cloak flips the show-cloak option as the other character-option keys flip \
                 theirs. This client's own actions (the performance panel, hold sidestep, the \
                 trade and spell-research windows and the character settings the classic \
                 interface had keys for) are listed on the retail interface's key page under \
                 their own names, in a Dereth section of the tab they belong to, and the retail \
                 interface answers each: hold sidestep steps sideways with the turning keys, the \
                 trade key shows or hides the secure-trade window, the spell-research key opens \
                 the magic window on its Create Spell tab, automatic shortcuts flips the \
                 character's option, inverted mouse look and mute when inactive flip the shared \
                 settings, and right-click mouse look and the stretched interface, which the \
                 retail interface has no mode for, flip the classic interface's settings and say \
                 so in the chat.",
        why: "The owner asked that every key that can be bound work when it is bound.",
    },
    Divergence {
        id: "CD-020",
        title: "The examined creature's model over its attribute list",
        retail: "The end-of-retail client draws the examine window's model where its viewport                  stands among the window's elements, before the attribute list laid over it, so                  the list's translucent rows shade the model where they cover it.",
        dereth: "The model is drawn over the attribute list's rows and under its text, so it is                  as bright behind the numbers as below them.",
        why: "The owner asked for the whole model to be drawn above the list's translucent               ground.",
    },
    Divergence {
        id: "CD-021",
        title: "One set of options pages for both interfaces",
        retail: "The end-of-retail client's options window had four pages of its own: Game and \
                 Support (with In-Game Help and two buttons that opened a support web page, now \
                 gone), Character Options under six headings, Chat Options and Client Options \
                 under six more (one of them Sync with Refresh Rate, which acts only in full \
                 screen). The early client's had three, with no chat page. The end-of-retail \
                 client started full screen, and its client options page's Defaults button put \
                 the resolution at 800 by 600 although the client starts at 1024 by 768.",
        dereth: "Both interfaces draw the same four pages from one set of options, each in its own \
                 look: Game and Support; Character Options under seven headings (Interface \
                 Behavior, World Display, Chat, Fellowship and Allegiance, Other Players, Allow \
                 Others to See Your, Combat and Movement); Chat Options; and Client Options under \
                 five (Sound, Display, Graphics Quality, Era Look, Camera and Mouse). A row stores \
                 its value where retail kept it, so either interface's page shows what the other \
                 set. A row for something the world's era lacks is greyed out. In-Game Help is \
                 not offered, Urgent Assistance and Report Abuse open the game's own forms, and \
                 there is no Sync with Refresh Rate row (the full screen is a borderless window). \
                 Manual Degrade Bias is greyed while Adaptive Degrade is on, and the landscape's \
                 detail texture has a row. The classic interface's Options page is its Game and \
                 Support page (Leave World, Exit Game, Configure Keyboard and the two forms), its \
                 Client page scrolls, its environment textures offer Very High, and its Chat page \
                 sets which messages its one chat window shows, by the groups the retail \
                 interface's main chat window has; Display Tooltips shows or hides the name of \
                 what is under the pointer. The client starts in a window at 1024 by 768, and \
                 the Defaults button puts full screen off and the resolution at 1024 by 768.",
        why: "The owner asked for one set of options behind both interfaces, grouped the same \
              way, with the dead rows gone, a windowed start and Defaults keeping the starting \
              size.",
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
