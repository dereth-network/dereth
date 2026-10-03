//! The options both interfaces show, as one set: four pages, each a list of headings, each
//! heading a list of rows.
//!
//! Each interface draws the same four pages in its own style: the Game / Support page's buttons,
//! the Character Options page (the character's own options, kept by the server), the Chat
//! Options page and the Client Options page (sound, display, graphics and camera, kept in the
//! profile). A row names where its value is kept ([`Value`]); nothing here keeps a value of its
//! own, so a row edited on one interface's page shows the change on the other's.
//!
//! A few rows belong to one interface only, because they mean something only there
//! ([`Shown`]): the classic interface's own mouse look, its stretched layout and its social
//! window's pages; the retail interface's mouse turning, its side-by-side vitals, its chat font
//! and its floating chat windows. A row for something the world's era does not have is drawn
//! greyed out, not left out ([`Needs`]).

use crate::options::config::PrefValueConst;
use crate::view::PlayerOption;

/// The four pages, in the order both interfaces' tabs list them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PageId {
    /// The buttons: leaving the world, the keys, the support forms.
    GameSupport,
    /// The character's own options, kept by the server.
    Character,
    /// The chat windows' options.
    Chat,
    /// Sound, display, graphics and camera, kept in the profile.
    Client,
}

/// One page.
#[derive(Debug, Clone, Copy)]
pub struct Page {
    pub id: PageId,
    pub title: &'static str,
    pub headings: &'static [Heading],
}

/// One heading of a page and the rows under it.
#[derive(Debug, Clone, Copy)]
pub struct Heading {
    pub title: &'static str,
    pub rows: &'static [Row],
}

/// Which interfaces show a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Both,
    Retail,
    Classic,
}

/// One of the two interfaces, as a page is drawn for it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Face {
    Retail,
    Classic,
}

impl Shown {
    /// Whether the interface `face` shows the row.
    #[must_use]
    pub const fn on(self, face: Face) -> bool {
        matches!(
            (self, face),
            (Self::Both, _) | (Self::Retail, Face::Retail) | (Self::Classic, Face::Classic)
        )
    }
}

/// What a row needs of the world's era to mean anything. A row whose need is not met is drawn
/// greyed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// Any world.
    Nothing,
    /// Houses.
    Housing,
    /// Rare items.
    Rares,
    /// Cloaks.
    Cloaks,
    /// Character titles.
    Titles,
}

impl Needs {
    /// Whether a world with `features` has what the row needs. A world whose era is not known
    /// has everything.
    #[must_use]
    pub fn met(self, features: Option<&dereth_primitives::EraFeatures>) -> bool {
        let Some(f) = features else { return true };
        match self {
            Self::Nothing => true,
            Self::Housing => f.housing,
            Self::Rares => f.pre_order_items_and_rares,
            Self::Cloaks => f.cloaks,
            Self::Titles => f.titles,
        }
    }
}

/// A button on the Game / Support page.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Act {
    /// Asks, then ends the character session.
    ExitToCharacterSelection,
    /// Quits the client.
    ExitGame,
    /// The key bindings page.
    ConfigureKeyboard,
    /// The mouse-turning preset: six Client Options rows and the wheel on the camera zoom.
    MouseTurningSettings,
    /// The in-game form that sends a request for help to the server.
    UrgentAssistance,
    /// The in-game form that sends a complaint to the server's abuse log.
    ReportAbuse,
}

/// Where a row's value is kept, and so what kind of control shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Value {
    /// A button.
    Action(Act),
    /// One of the character's options, kept by the server.
    Option(PlayerOption),
    /// A bit of the character's first option word that no option of the final client names
    /// (`mask`), kept by the server all the same.
    Bit { mask: u32 },
    /// A check box on a profile preference.
    Check(&'static str),
    /// A slider on a profile preference.
    Slider(&'static str),
    /// A drop-down on a profile preference.
    Menu(&'static str),
    /// A sound: its on/off preference (true is on) and its volume.
    Sound {
        on: &'static str,
        volume: &'static str,
    },
    /// One group of a chat window's message filter: `mask` of the window's filter.
    Filter { window: u32, mask: u64 },
    /// One of the chat windows' two opacities, a gameplay option property.
    Opacity(u32),
}

/// One row.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Row {
    pub caption: &'static str,
    pub value: Value,
    pub shown: Shown,
    pub needs: Needs,
    /// The classic interface's own wording for the row, where it has one, and whether that
    /// wording asks the opposite question (a check means the option is off).
    pub classic: Option<(&'static str, bool)>,
    /// The value the page's Defaults button gives a profile preference row; `None` for a row
    /// whose default is the character's, the chat window's, or none at all.
    pub default: Option<PrefValueConst>,
    /// What the row does when its caption alone does not say, for a tooltip.
    pub note: Option<&'static str>,
}

impl Row {
    /// The caption `face` draws.
    #[must_use]
    pub fn caption_for(&self, face: Face) -> &'static str {
        match (face, self.classic) {
            (Face::Classic, Some((caption, _))) => caption,
            _ => self.caption,
        }
    }

    /// Whether the classic interface's caption asks the opposite of the option.
    #[must_use]
    pub fn classic_inverted(&self) -> bool {
        self.classic.is_some_and(|(_, inverted)| inverted)
    }

    /// The profile preference the row edits, if it edits one (a sound row's on/off one).
    #[must_use]
    pub const fn preference(&self) -> Option<&'static str> {
        match self.value {
            Value::Check(p) | Value::Slider(p) | Value::Menu(p) => Some(p),
            Value::Sound { on, .. } => Some(on),
            _ => None,
        }
    }
}

const fn row(caption: &'static str, value: Value) -> Row {
    Row {
        caption,
        value,
        shown: Shown::Both,
        needs: Needs::Nothing,
        classic: None,
        default: None,
        note: None,
    }
}

const fn opt(caption: &'static str, option: PlayerOption) -> Row {
    row(caption, Value::Option(option))
}

/// An option the classic interface words its own way.
const fn opt_c(
    caption: &'static str,
    option: PlayerOption,
    classic: &'static str,
    inverted: bool,
) -> Row {
    Row {
        classic: Some((classic, inverted)),
        ..opt(caption, option)
    }
}

const fn only(shown: Shown, r: Row) -> Row {
    Row { shown, ..r }
}

const fn needs(needs: Needs, r: Row) -> Row {
    Row { needs, ..r }
}

const fn noted(note: &'static str, r: Row) -> Row {
    Row {
        note: Some(note),
        ..r
    }
}

const fn pref(caption: &'static str, value: Value, default: PrefValueConst) -> Row {
    Row {
        default: Some(default),
        ..row(caption, value)
    }
}

const fn button(caption: &'static str, act: Act) -> Row {
    row(caption, Value::Action(act))
}

use PlayerOption as P;
use PrefValueConst::{Bool, Float, Int};

// ---- Game / Support --------------------------------------------------------------------------

const GAME_SUPPORT: [Heading; 1] = [Heading {
    title: "Game and Support",
    rows: &[
        Row {
            classic: Some(("Leave World", false)),
            ..button("Exit to Character Selection", Act::ExitToCharacterSelection)
        },
        button("Exit Game", Act::ExitGame),
        button("Configure Keyboard", Act::ConfigureKeyboard),
        only(
            Shown::Retail,
            button("Use Mouse Turning Settings", Act::MouseTurningSettings),
        ),
        button("Urgent Assistance", Act::UrgentAssistance),
        button("Report Abuse", Act::ReportAbuse),
    ],
}];

// ---- Character Options -----------------------------------------------------------------------

const CHARACTER: [Heading; 7] = [
    Heading {
        title: "Interface Behavior",
        rows: &[
            opt_c(
                "Keep Combat Targets in View",
                P::ViewCombatTarget,
                "Automatically keep combat targets in view",
                false,
            ),
            only(
                Shown::Classic,
                noted(
                    "Using or equipping an item you own puts it in the first free shortcut slot.",
                    row("Automatically Create Shortcuts", Value::Bit { mask: 1 }),
                ),
            ),
            opt("Salvage Multiple Materials at Once", P::SalvageMultiple),
            opt(
                "Use Main Pack as Default for Picking Up Items",
                P::MainPackPreferred,
            ),
            opt("Vivid Targeting Indicator", P::VividTargetingIndicator),
            opt("Display Tooltips", P::ShowTooltips),
            opt_c(
                "Show Coordinates By the Radar",
                P::CoordinatesOnRadar,
                "Show Coordinates Below The Radar",
                false,
            ),
            only(
                Shown::Retail,
                opt("Side By Side Vitals", P::SideBySideVitals),
            ),
            opt("Display Spell Durations", P::SpellDuration),
            opt_c(
                "Use Crafting Chance of Success Dialog",
                P::UseCraftSuccessDialog,
                "Display Crafting Chance of Success Dialog",
                false,
            ),
            needs(
                Needs::Rares,
                opt("Confirm Use of Rare Gems", P::ConfirmVolatileRareUse),
            ),
        ],
    },
    Heading {
        title: "World Display",
        rows: &[
            opt("Disable Most Weather Effects", P::DisableMostWeatherEffects),
            opt("Disable Distance Fog", P::DisableDistanceFog),
            opt("Always Daylight Outdoors", P::PersistentAtDay),
            needs(
                Needs::Housing,
                opt(
                    "Disable House Restriction Effects",
                    P::DisableHouseRestrictionEffects,
                ),
            ),
        ],
    },
    Heading {
        title: "Chat",
        rows: &[
            opt_c(
                "Stay in Chat Mode After Sending a Message",
                P::StayInChatMode,
                "Stay in Chat Mode after sending a message",
                false,
            ),
            opt("Display Timestamps", P::DisplayTimeStamps),
            opt("Filter Language", P::FilterLanguage),
            opt_c(
                "Listen to Allegiance Chat",
                P::HearAllegianceChat,
                "Global Allegiance Chat",
                false,
            ),
            opt("Listen to General Chat", P::HearGeneralChat),
            opt("Listen to Trade Chat", P::HearTradeChat),
            opt("Listen to LFG Chat", P::HearLFGChat),
            opt("Listen to Roleplay Chat", P::HearRoleplayChat),
            opt("Listen to Society Chat", P::HearSocietyChat),
            opt("Listen to PK Death Messages", P::HearPKDeaths),
        ],
    },
    Heading {
        title: "Fellowship and Allegiance",
        rows: &[
            opt_c(
                "Ignore Allegiance Requests",
                P::IgnoreAllegianceRequests,
                "Accept Allegiance Requests",
                true,
            ),
            opt_c(
                "Ignore Fellowship Requests",
                P::IgnoreFellowshipRequests,
                "Accept Fellowship Requests",
                true,
            ),
            opt(
                "Show Allegiance Logons",
                P::DisplayAllegianceLogonNotifications,
            ),
            opt("Share Fellowship Experience", P::FellowshipShareXP),
            opt("Share Fellowship Loot", P::FellowshipShareLoot),
            opt(
                "Automatically Accept Fellowship Requests",
                P::FellowshipAutoAcceptRequests,
            ),
        ],
    },
    Heading {
        title: "Other Players",
        rows: &[
            opt("Accept Corpse-Looting Permissions", P::AcceptLootPermits),
            opt("Attempt to Deceive Other Players", P::UseDeception),
            opt("Let Other Players Give You Items", P::AllowGive),
            opt("Ignore All Trade Requests", P::IgnoreTradeRequests),
            opt_c(
                "Drag Item to Player Opens Trade",
                P::DragItemOnPlayerOpensSecureTrade,
                "Drag item to player opens Secure Trade",
                false,
            ),
            opt("Show Your Helm or Head Gear", P::ShowHelm),
            needs(Needs::Cloaks, opt("Show Your Cloak", P::ShowCloak)),
        ],
    },
    Heading {
        title: "Allow Others to See Your",
        rows: &[
            opt_c(
                "Date of Birth",
                P::DisplayDateOfBirth,
                "Allow others to see your Date of Birth",
                false,
            ),
            opt_c("Age", P::DisplayAge, "Allow others to see your Age", false),
            opt_c(
                "Chess Rank",
                P::DisplayChessRank,
                "Allow others to see your Chess Rank",
                false,
            ),
            opt_c(
                "Fishing Skill",
                P::DisplayFishingSkill,
                "Allow others to see your Fishing Skill",
                false,
            ),
            opt_c(
                "Number of Deaths",
                P::DisplayNumberDeaths,
                "Allow others to see your Number of Deaths",
                false,
            ),
            needs(
                Needs::Titles,
                opt_c(
                    "Number of Titles",
                    P::DisplayNumberCharacterTitles,
                    "Allow others to see your Number of Titles",
                    false,
                ),
            ),
        ],
    },
    Heading {
        title: "Combat and Movement",
        rows: &[
            opt("Run as Default Movement", P::ToggleRun),
            opt_c(
                "Advanced Combat Interface",
                P::AdvancedCombatUI,
                "Advanced Combat Interface (No Panel)",
                false,
            ),
            opt("Auto Target", P::AutoTarget),
            opt("Automatically Repeat Attacks", P::AutoRepeatAttack),
            opt("Use Charge Attack", P::UseChargeAttack),
            opt("Lead Missile Targets", P::LeadMissileTargets),
            opt("Use Fast Missiles", P::UseFastMissiles),
        ],
    },
];

// ---- Chat Options ----------------------------------------------------------------------------

/// The chat windows' ids, as the options name them.
pub mod window {
    pub use crate::chat::interface::window::{FLOATY_1, FLOATY_2, FLOATY_3, FLOATY_4, MAIN};
}

/// The groups of a chat window's message filter, in the order the pages list them: the group's
/// caption and its mask of the filter. The first, Gameplay, is offered for the floating
/// windows only; the main window shows its messages always.
pub const FILTER_GROUPS: [(&str, u64); 13] = [
    ("Gameplay", 0x8391_2021),
    // Chat types 6, 21 and 22.
    ("Combat", 1 << 6 | 1 << 21 | 1 << 22),
    ("Magic", 0x0002_0080),
    ("Area Speech", 0x0000_1004),
    ("Tells", 0x0000_0018),
    ("Allegiance", 0x0004_0C00),
    ("Fellowship", 0x0008_0000),
    ("General", 0x0800_0000),
    ("Trade", 0x1000_0000),
    ("LFG", 0x2000_0000),
    ("Roleplay", 0x4000_0000),
    ("Society", 0x1_0000_0000),
    ("Errors", 0x0400_0000),
];

/// The filter rows of one window: `N` groups of [`FILTER_GROUPS`] from `from` on.
const fn filter_rows<const N: usize>(window: u32, from: usize) -> [Row; N] {
    let mut out = [row("", Value::Filter { window, mask: 0 }); N];
    let mut i = 0;
    while i < N {
        let (caption, mask) = FILTER_GROUPS[from + i];
        out[i] = row(caption, Value::Filter { window, mask });
        i += 1;
    }
    out
}

const fn floaty_rows(window: u32) -> [Row; 13] {
    let mut rows = filter_rows::<13>(window, 0);
    let mut i = 0;
    while i < 13 {
        rows[i].shown = Shown::Retail;
        i += 1;
    }
    rows
}

const MAIN_FILTERS: [Row; 12] = filter_rows::<12>(window::MAIN, 1);
const FLOATY_1_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_1);
const FLOATY_2_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_2);
const FLOATY_3_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_3);
const FLOATY_4_FILTERS: [Row; 13] = floaty_rows(window::FLOATY_4);

const CHAT: [Heading; 6] = [
    Heading {
        title: "Chat Windows",
        rows: &[
            only(
                Shown::Retail,
                row("Inactive Window Opacity", Value::Opacity(0x1000_0080)),
            ),
            only(
                Shown::Retail,
                row("Active Window Opacity", Value::Opacity(0x1000_0081)),
            ),
        ],
    },
    Heading {
        title: "Main Chat Window",
        rows: &MAIN_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 1",
        rows: &FLOATY_1_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 2",
        rows: &FLOATY_2_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 3",
        rows: &FLOATY_3_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 4",
        rows: &FLOATY_4_FILTERS,
    },
];

// ---- Client Options --------------------------------------------------------------------------

const CLIENT: [Heading; 5] = [
    Heading {
        title: "Sound",
        rows: &[
            pref("Sound Output", Value::Menu("Sound.SoundFeatures"), Int(0)),
            pref(
                "Sound Effects",
                Value::Sound {
                    on: "Sound.SoundDisabled",
                    volume: "Sound.SoundVolume",
                },
                Bool(true),
            ),
            pref(
                "Ambient Sounds",
                Value::Sound {
                    on: "Sound.AmbientSoundDisabled",
                    volume: "Sound.AmbientSoundVolume",
                },
                Bool(true),
            ),
            pref(
                "Interface Sounds",
                Value::Sound {
                    on: "Sound.InterfaceSoundDisabled",
                    volume: "Sound.InterfaceSoundVolume",
                },
                Bool(true),
            ),
            pref(
                "Play Sounds Only When Active",
                Value::Check("Sound.PlaySoundOnlyWhenActive"),
                Bool(true),
            ),
        ],
    },
    Heading {
        title: "Display",
        rows: &[
            pref(
                "Interface",
                Value::Menu(crate::options::interface::INTERFACE),
                Int(0),
            ),
            pref(
                "Resolution",
                Value::Menu(crate::options::store::DISPLAY_RESOLUTION),
                Int(0x0400_0300),
            ),
            pref(
                "Full Screen",
                Value::Check("Display.FullScreen"),
                Bool(false),
            ),
            pref(
                "Screen Brightness",
                Value::Slider("Render.ScreenBrightness"),
                Float(0.0),
            ),
            pref(
                "Field of View",
                Value::Slider("Render.FieldOfView"),
                Float(90.0),
            ),
            pref(
                "Performance Panel",
                Value::Check(crate::options::performance::PERFORMANCE_PANEL),
                Bool(false),
            ),
            only(
                Shown::Classic,
                pref(
                    "Stretch UI",
                    Value::Check(crate::options::classic::STRETCH_UI),
                    Bool(false),
                ),
            ),
            only(
                Shown::Classic,
                pref(
                    "Show Trade Tab",
                    Value::Check(crate::options::classic::SHOW_TRADE_TAB),
                    Bool(false),
                ),
            ),
            only(
                Shown::Classic,
                pref(
                    "Show Friends Tab",
                    Value::Check(crate::options::classic::SHOW_FRIENDS_TAB),
                    Bool(true),
                ),
            ),
            only(
                Shown::Classic,
                pref(
                    "Show Squelch Tab",
                    Value::Check(crate::options::classic::SHOW_SQUELCH_TAB),
                    Bool(true),
                ),
            ),
            only(
                Shown::Retail,
                pref("Chat Font", Value::Menu("UI.ChatFontFace"), Int(2)),
            ),
            only(
                Shown::Retail,
                pref("Chat Font Size", Value::Menu("UI.ChatFontSize"), Int(1)),
            ),
        ],
    },
    Heading {
        title: "Graphics Quality",
        rows: &[
            Row {
                classic: Some(("Auto-Degrade", false)),
                ..noted(
                    "Lowers the detail of distant objects to hold the frame rate.",
                    pref(
                        "Adaptive Degrade",
                        Value::Check("Render.AutomaticDegrades"),
                        Bool(false),
                    ),
                )
            },
            Row {
                classic: Some(("Graphics Performance", false)),
                ..noted(
                    "Speed or detail chosen by hand, used while Adaptive Degrade is off.",
                    pref(
                        "Manual Degrade Bias",
                        Value::Slider("Render.GraphicsPerformance"),
                        Float(0.0),
                    ),
                )
            },
            pref(
                "Degrade Distance",
                Value::Slider("Render.DegradeDistance"),
                Float(50.0),
            ),
            pref(
                "Landscape Texture Detail",
                Value::Menu("Render.LandscapeTextureDetail"),
                Int(2),
            ),
            pref(
                "Environment Texture Detail",
                Value::Menu("Render.EnvironmentTextureDetail"),
                Int(1),
            ),
            pref(
                "Texture Filtering",
                Value::Menu("Render.TextureFiltering"),
                Int(1),
            ),
            pref(
                "Landscape Draw Distance",
                Value::Menu("Render.LandscapeDrawDistance"),
                Int(8),
            ),
            pref(
                "Environment Detail Textures",
                Value::Check("Render.BuildingDetailTextures"),
                Bool(true),
            ),
            pref(
                "Landscape Detail Textures",
                Value::Check("Render.LandscapeDetailTextures"),
                Bool(false),
            ),
            pref(
                "Multiple Pass Alpha",
                Value::Check("Render.MultiPassAlpha"),
                Bool(false),
            ),
        ],
    },
    Heading {
        title: "Era Look",
        rows: &[
            pref(
                "Terrain Mode",
                Value::Menu(crate::options::landscape::GROUND),
                Int(crate::options::landscape::WORLD_DEFAULT),
            ),
            pref(
                "Sky Mode",
                Value::Menu(crate::options::landscape::SKY),
                Int(crate::options::landscape::WORLD_DEFAULT),
            ),
            pref(
                "Object Mode",
                Value::Menu(crate::options::landscape::OBJECTS),
                Int(crate::options::landscape::WORLD_DEFAULT),
            ),
        ],
    },
    Heading {
        title: "Camera and Mouse",
        rows: &[
            pref(
                "Camera Stiffness",
                Value::Slider("Camera.Stiffness"),
                Float(0.45),
            ),
            pref(
                "Camera Adjustment Speed",
                Value::Slider("Camera.AdjustmentSpeed"),
                Float(40.0),
            ),
            pref(
                "Align Camera to Slope",
                Value::Check("Camera.AlignToSlope"),
                Bool(true),
            ),
            pref(
                "Mouselook Sensitivity",
                Value::Slider("Input.MouseLookSensitivity"),
                Float(0.55),
            ),
            only(
                Shown::Retail,
                pref(
                    "Invert Mouselook Axes",
                    Value::Check("Input.InvertMouseLookYAxis"),
                    Bool(false),
                ),
            ),
            only(
                Shown::Classic,
                pref(
                    "Invert Mouse Look Up/Down",
                    Value::Check(crate::options::classic::INVERT_MOUSE_LOOK),
                    Bool(false),
                ),
            ),
            only(
                Shown::Retail,
                pref(
                    "Turn Your Character with Camera Turning",
                    Value::Check("Input.UseMouseTurning"),
                    Bool(false),
                ),
            ),
            only(
                Shown::Classic,
                pref(
                    "Right-click Mouselook",
                    Value::Check(crate::options::classic::RIGHT_CLICK_MOUSE_LOOK),
                    Bool(true),
                ),
            ),
        ],
    },
];

/// The four pages, in tab order.
pub const PAGES: [Page; 4] = [
    Page {
        id: PageId::GameSupport,
        title: "Game / Support",
        headings: &GAME_SUPPORT,
    },
    Page {
        id: PageId::Character,
        title: "Character Options",
        headings: &CHARACTER,
    },
    Page {
        id: PageId::Chat,
        title: "Chat Options",
        headings: &CHAT,
    },
    Page {
        id: PageId::Client,
        title: "Client Options",
        headings: &CLIENT,
    },
];

/// One page by its id.
#[must_use]
pub fn page(id: PageId) -> &'static Page {
    match id {
        PageId::GameSupport => &PAGES[0],
        PageId::Character => &PAGES[1],
        PageId::Chat => &PAGES[2],
        PageId::Client => &PAGES[3],
    }
}

/// The headings of `page` that `face` shows, each with the rows it shows, in order; a heading
/// with no row for `face` is left out.
pub fn headings_for(
    id: PageId,
    face: Face,
) -> impl Iterator<Item = (&'static Heading, Vec<&'static Row>)> {
    page(id).headings.iter().filter_map(move |h| {
        let rows: Vec<&'static Row> = h.rows.iter().filter(|r| r.shown.on(face)).collect();
        (!rows.is_empty()).then_some((h, rows))
    })
}

/// Every row of `page` that `face` shows, in order.
pub fn rows_for(id: PageId, face: Face) -> impl Iterator<Item = &'static Row> {
    page(id)
        .headings
        .iter()
        .flat_map(|h| h.rows.iter())
        .filter(move |r| r.shown.on(face))
}

/// The row of `page` that edits `preference`, if there is one.
#[must_use]
pub fn row_of_preference(id: PageId, preference: &str) -> Option<&'static Row> {
    page(id)
        .headings
        .iter()
        .flat_map(|h| h.rows.iter())
        .find(|r| {
            r.preference() == Some(preference)
                || matches!(r.value, Value::Sound { volume, .. } if volume == preference)
        })
}

/// The row of the Character Options page that edits `option`.
#[must_use]
pub fn row_of_option(option: PlayerOption) -> Option<&'static Row> {
    page(PageId::Character)
        .headings
        .iter()
        .flat_map(|h| h.rows.iter())
        .find(|r| r.value == Value::Option(option))
}

/// The slider a check box greys out while the box is ticked: Degrade Bias is the speed or detail
/// chosen by hand, and is used only while Adaptive Degrade is off.
pub const GREYED_WHILE_ON: [(&str, &str); 1] =
    [("Render.AutomaticDegrades", "Render.GraphicsPerformance")];

#[cfg(test)]
mod tests {
    //! Behaviour: none (the set's own shape; each interface's page is tested where it is drawn).
    use super::*;

    #[test]
    fn every_character_option_on_a_page_is_on_the_character_page_once() {
        let mut seen: Vec<PlayerOption> = rows_for(PageId::Character, Face::Retail)
            .chain(rows_for(PageId::Character, Face::Classic))
            .filter_map(|r| match r.value {
                Value::Option(o) => Some(o),
                _ => None,
            })
            .collect();
        seen.sort_unstable();
        seen.dedup();
        // Every option but the two set elsewhere: the radar's lock and the friends panel's
        // appear-offline box.
        let mut want: Vec<PlayerOption> = PlayerOption::ALL
            .into_iter()
            .filter(|o| !matches!(o, P::LockUI | P::AppearOffline))
            .collect();
        want.sort_unstable();
        assert_eq!(seen, want);
        let all: Vec<&Row> = page(PageId::Character)
            .headings
            .iter()
            .flat_map(|h| h.rows.iter())
            .collect();
        for o in want {
            assert_eq!(
                all.iter().filter(|r| r.value == Value::Option(o)).count(),
                1,
                "{o:?}"
            );
        }
    }

    #[test]
    fn the_four_pages_are_in_tab_order_and_each_face_draws_every_page() {
        let ids: Vec<PageId> = PAGES.iter().map(|p| p.id).collect();
        assert_eq!(
            ids,
            [
                PageId::GameSupport,
                PageId::Character,
                PageId::Chat,
                PageId::Client
            ]
        );
        for face in [Face::Retail, Face::Classic] {
            for p in PAGES {
                assert!(rows_for(p.id, face).count() > 0, "{face:?} {:?}", p.id);
            }
        }
        // The classic chat page is the main window's filter only.
        let classic_chat: Vec<&str> = headings_for(PageId::Chat, Face::Classic)
            .map(|(h, _)| h.title)
            .collect();
        assert_eq!(classic_chat, ["Main Chat Window"]);
        assert_eq!(
            rows_for(PageId::Chat, Face::Retail).count(),
            2 + 12 + 13 * 4
        );
    }

    #[test]
    fn each_interface_keeps_its_own_mouse_look_and_invert() {
        let has = |face, p: &str| rows_for(PageId::Client, face).any(|r| r.preference() == Some(p));
        assert!(has(Face::Retail, "Input.InvertMouseLookYAxis"));
        assert!(!has(Face::Classic, "Input.InvertMouseLookYAxis"));
        assert!(has(
            Face::Classic,
            crate::options::classic::INVERT_MOUSE_LOOK
        ));
        assert!(!has(
            Face::Retail,
            crate::options::classic::INVERT_MOUSE_LOOK
        ));
        assert!(has(Face::Retail, "Input.UseMouseTurning"));
        assert!(!has(Face::Classic, "Input.UseMouseTurning"));
        assert!(has(
            Face::Classic,
            crate::options::classic::RIGHT_CLICK_MOUSE_LOOK
        ));
        // No row for vertical sync: it does nothing in a borderless window.
        assert!(!has(Face::Retail, "Display.SyncToRefresh"));
        assert!(!has(Face::Classic, "Display.SyncToRefresh"));
    }

    #[test]
    fn a_row_the_era_lacks_is_marked_and_met_by_an_era_that_has_it() {
        let f = dereth_primitives::EraFeatures::INFILTRATION;
        let cloak = row_of_option(P::ShowCloak).unwrap();
        assert!(!cloak.needs.met(Some(&f)));
        assert!(cloak
            .needs
            .met(Some(&dereth_primitives::EraFeatures::END_OF_RETAIL)));
        assert!(cloak.needs.met(None));
        assert!(row_of_option(P::DisableHouseRestrictionEffects)
            .unwrap()
            .needs
            .met(Some(&f)));
    }

    #[test]
    fn every_profile_row_has_a_default_and_a_registered_preference() {
        crate::options::store::init();
        for face in [Face::Retail, Face::Classic] {
            for r in rows_for(PageId::Client, face) {
                let p = r.preference().unwrap();
                assert!(r.default.is_some(), "{p}");
                if p != "Render.LandscapeDetailTextures" {
                    assert!(crate::options::store::is_registered(p), "{p}");
                }
            }
        }
    }
}
