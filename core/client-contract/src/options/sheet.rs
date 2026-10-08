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
//! window's pages; the modern interface's mouse turning, its side-by-side vitals, its chat font
//! and its floating chat windows. A row for something the world's era does not have is left out ([`Needs`]).

use crate::options::{config::PrefValueConst, interface::Interface};
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

/// A localized options caption with an honest literal fallback.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Text {
    pub table_enum: u32,
    pub token: Option<&'static str>,
    pub fallback: &'static str,
}
impl Text {
    #[must_use]
    pub const fn literal(fallback: &'static str) -> Self {
        Self {
            table_enum: 0x1000_0003,
            token: None,
            fallback,
        }
    }
    #[must_use]
    pub const fn preference(token: &'static str, fallback: &'static str) -> Self {
        Self {
            table_enum: 0x1000_0003,
            token: Some(token),
            fallback,
        }
    }
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
    pub text: Text,
    pub rows: &'static [Row],
}

/// Which interfaces show a row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shown {
    Both,
    Only(Interface),
}

impl Shown {
    /// Whether the interface `face` shows the row. The Horizon interface shows the modern
    /// interface's own rows: it plays with the modern key map, mouse and chat.
    #[must_use]
    pub const fn on(self, face: Interface) -> bool {
        matches!(
            (self, face),
            (Self::Both, _)
                | (
                    Self::Only(Interface::Modern),
                    Interface::Modern | Interface::Horizon
                )
                | (Self::Only(Interface::Classic), Interface::Classic)
        )
    }
}

/// What a row needs of the world's era to mean anything. A row whose need is not met is left out.
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
    /// Secure trade.
    Trade,
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
            Self::Trade => f.trade,
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
    /// The volume half of a sound row's UI restore value.
    pub volume_default: Option<f32>,
    /// Preference-table tokens for a slider's end captions.
    pub slider_ends: Option<(&'static str, &'static str)>,
    /// Interactive edits ask whether the applied value should be retained.
    pub confirm_change: bool,
    /// What the row does when its caption alone does not say, for a tooltip.
    pub note: Option<&'static str>,
}

impl Row {
    /// UI restore value for either preference owned by this row.
    #[must_use]
    pub fn default_for(&self, preference: &str) -> Option<PrefValueConst> {
        if preference == super::interface::INTERFACE {
            return None;
        }
        if self.preference() == Some(preference) {
            return self.default;
        }
        match self.value {
            Value::Sound { volume, .. } if preference == volume => {
                self.volume_default.map(PrefValueConst::Float)
            }
            _ => None,
        }
    }

    /// The caption `face` draws.
    #[must_use]
    pub fn caption_for(&self, face: Interface) -> &'static str {
        match (face, self.classic) {
            (Interface::Classic, Some((caption, _))) => caption,
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
        volume_default: None,
        slider_ends: None,
        confirm_change: false,
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
        volume_default: match value {
            Value::Sound { .. } => Some(1.0),
            _ => None,
        },
        ..row(caption, value)
    }
}

const fn ends(left: &'static str, right: &'static str, r: Row) -> Row {
    Row {
        slider_ends: Some((left, right)),
        ..r
    }
}
const fn confirmed(r: Row) -> Row {
    Row {
        confirm_change: true,
        ..r
    }
}

/// Static registered bounds shared by the option rows.
pub use super::preferences::range as preference_range;

/// UI restore values in the page's presentation order, including paired volume controls.
#[must_use]
pub fn defaults(page: PageId, face: Interface) -> Vec<(&'static str, PrefValueConst)> {
    rows_for(page, face)
        .flat_map(|row| {
            let names = match row.value {
                Value::Sound { on, volume } => [Some(on), Some(volume)],
                _ => [row.preference(), None],
            };
            names
                .into_iter()
                .flatten()
                .filter_map(move |p| row.default_for(p).map(|v| (p, v)))
        })
        .collect()
}

const fn button(caption: &'static str, act: Act) -> Row {
    row(caption, Value::Action(act))
}

use PlayerOption as P;
use PrefValueConst::{Bool, Float, Int};

// ---- Game / Support --------------------------------------------------------------------------

const GAME_SUPPORT: [Heading; 1] = [Heading {
    title: "Game and Support",
    text: Text::literal("Game and Support"),
    rows: &[
        Row {
            classic: Some(("Leave World", false)),
            ..button("Exit to Character Selection", Act::ExitToCharacterSelection)
        },
        button("Exit Game", Act::ExitGame),
        button("Configure Keyboard", Act::ConfigureKeyboard),
        only(
            Shown::Only(Interface::Modern),
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
        text: Text::preference(
            "ID_CharacterOption_UIBehavior_Section",
            "Interface Behavior",
        ),
        rows: &[
            opt_c(
                "Keep Combat Targets in View",
                P::ViewCombatTarget,
                "Automatically keep combat targets in view",
                false,
            ),
            only(
                Shown::Only(Interface::Classic),
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
                Shown::Only(Interface::Modern),
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
        text: Text::preference("ID_CharacterOption_UIDisplay_Section", "World Display"),
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
        text: Text::preference("ID_CharacterOption_Chat_Section", "Chat"),
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
        text: Text::preference(
            "ID_CharacterOption_Grouping_Section",
            "Fellowship and Allegiance",
        ),
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
        text: Text::preference("ID_CharacterOption_OtherPlayers_Section", "Other Players"),
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
        text: Text::literal("Allow Others to See Your"),
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
        text: Text::preference(
            "ID_CharacterOption_CharacterBehavior_Section",
            "Combat and Movement",
        ),
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

/// The main chat window's filter before the player sets one: every kind of message but the one
/// meant for the speech bubbles over heads (`1 << 26`).
pub const MAIN_WINDOW_DEFAULT_FILTER: u64 = 0xFBFF_FFFF;

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
        rows[i].shown = Shown::Only(Interface::Modern);
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
        text: Text::preference("ID_ChatOption_GeneralOptions_Section", "Chat Windows"),
        rows: &[
            only(
                Shown::Only(Interface::Modern),
                row("Inactive Window Opacity", Value::Opacity(0x1000_0080)),
            ),
            only(
                Shown::Only(Interface::Modern),
                row("Active Window Opacity", Value::Opacity(0x1000_0081)),
            ),
            only(
                Shown::Only(Interface::Modern),
                pref(
                    "Chat Font",
                    Value::Menu(crate::options::names::CHAT_FONT_FACE),
                    Int(2),
                ),
            ),
            only(
                Shown::Only(Interface::Modern),
                pref(
                    "Chat Font Size",
                    Value::Menu(crate::options::names::CHAT_FONT_SIZE),
                    Int(1),
                ),
            ),
        ],
    },
    Heading {
        title: "Main Chat Window",
        text: Text::preference("ID_ChatOption_MainChatWindow_Section", "Main Chat Window"),
        rows: &MAIN_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 1",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow1_Section",
            "Floating Chat Window 1",
        ),
        rows: &FLOATY_1_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 2",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow2_Section",
            "Floating Chat Window 2",
        ),
        rows: &FLOATY_2_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 3",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow3_Section",
            "Floating Chat Window 3",
        ),
        rows: &FLOATY_3_FILTERS,
    },
    Heading {
        title: "Floating Chat Window 4",
        text: Text::preference(
            "ID_ChatOption_FloatyChatWindow4_Section",
            "Floating Chat Window 4",
        ),
        rows: &FLOATY_4_FILTERS,
    },
];

// ---- Client Options --------------------------------------------------------------------------

const CLIENT: [Heading; 5] = [
    Heading {
        title: "Sound",
        text: Text::preference("ID_Sound_SoundSection", "Sound Options"),
        rows: &[
            pref(
                "Sound Output",
                Value::Menu(crate::options::names::SOUND_FEATURES),
                Int(0),
            ),
            pref(
                "Sound Effects",
                Value::Sound {
                    on: crate::options::names::SOUND_DISABLED,
                    volume: crate::options::names::SOUND_VOLUME,
                },
                Bool(true),
            ),
            pref(
                "Ambient Sounds",
                Value::Sound {
                    on: crate::options::names::AMBIENT_SOUND_DISABLED,
                    volume: crate::options::names::AMBIENT_SOUND_VOLUME,
                },
                Bool(true),
            ),
            pref(
                "Interface Sounds",
                Value::Sound {
                    on: crate::options::names::INTERFACE_SOUND_DISABLED,
                    volume: crate::options::names::INTERFACE_SOUND_VOLUME,
                },
                Bool(true),
            ),
            pref(
                "Play Sounds Only When Active",
                Value::Check(crate::options::names::PLAY_SOUND_ONLY_WHEN_ACTIVE),
                Bool(true),
            ),
        ],
    },
    Heading {
        title: "Display",
        text: Text::literal("Display"),
        rows: &[
            pref(
                "Interface",
                Value::Menu(crate::options::interface::INTERFACE),
                Int(0),
            ),
            confirmed(pref(
                "Resolution",
                Value::Menu(crate::options::store::DISPLAY_RESOLUTION),
                Int(0x0400_0300),
            )),
            pref(
                "Full Screen",
                Value::Check(crate::options::names::DISPLAY_FULL_SCREEN),
                Bool(false),
            ),
            ends(
                "ID_Graphics_Value_Dark",
                "ID_Graphics_Value_Bright",
                pref(
                    "Screen Brightness",
                    Value::Slider(crate::options::names::SCREEN_BRIGHTNESS),
                    Float(0.0),
                ),
            ),
            ends(
                "ID_Graphics_Value_Narrow",
                "ID_Graphics_Value_Wide",
                pref(
                    "Field of View",
                    Value::Slider(crate::options::names::FIELD_OF_VIEW),
                    Float(90.0),
                ),
            ),
            pref(
                "Performance Panel",
                Value::Check(crate::options::performance::PERFORMANCE_PANEL),
                Bool(false),
            ),
            only(
                Shown::Only(Interface::Classic),
                pref(
                    "Stretch UI",
                    Value::Check(crate::options::classic::STRETCH_UI),
                    Bool(false),
                ),
            ),
            only(
                Shown::Only(Interface::Classic),
                needs(
                    Needs::Trade,
                    pref(
                        "Show Trade Tab",
                        Value::Check(crate::options::classic::SHOW_TRADE_TAB),
                        Bool(false),
                    ),
                ),
            ),
            only(
                Shown::Only(Interface::Classic),
                pref(
                    "Show Friends Tab",
                    Value::Check(crate::options::classic::SHOW_FRIENDS_TAB),
                    Bool(true),
                ),
            ),
            only(
                Shown::Only(Interface::Classic),
                pref(
                    "Show Squelch Tab",
                    Value::Check(crate::options::classic::SHOW_SQUELCH_TAB),
                    Bool(true),
                ),
            ),
        ],
    },
    Heading {
        title: "Graphics Quality",
        text: Text::literal("Graphics Quality"),
        rows: &[
            noted(
                "Lowers the detail of distant objects to hold the frame rate.",
                pref(
                    "Adaptive Degrade",
                    Value::Check(crate::options::names::AUTOMATIC_DEGRADES),
                    Bool(false),
                ),
            ),
            noted(
                "Speed or detail chosen by hand, used while Adaptive Degrade is off.",
                ends(
                    "ID_Graphics_Value_Speed",
                    "ID_Graphics_Value_Detail",
                    pref(
                        "Adaptive Degrade Bias",
                        Value::Slider(crate::options::names::GRAPHICS_PERFORMANCE),
                        Float(0.0),
                    ),
                ),
            ),
            ends(
                "ID_Graphics_Value_Close",
                "ID_Graphics_Value_Far",
                pref(
                    "Degrade Distance",
                    Value::Slider(crate::options::names::DEGRADE_DISTANCE),
                    Float(50.0),
                ),
            ),
            pref(
                "Landscape Texture Detail",
                Value::Menu(crate::options::names::LANDSCAPE_TEXTURE_DETAIL),
                Int(2),
            ),
            pref(
                "Environment Texture Detail",
                Value::Menu(crate::options::names::ENVIRONMENT_TEXTURE_DETAIL),
                Int(1),
            ),
            pref(
                "Texture Filtering",
                Value::Menu(crate::options::names::TEXTURE_FILTERING),
                Int(1),
            ),
            pref(
                "Landscape Draw Distance",
                Value::Menu(crate::options::names::LANDSCAPE_DRAW_DISTANCE),
                Int(8),
            ),
            pref(
                "Environment Detail Textures",
                Value::Check(crate::options::names::BUILDING_DETAIL_TEXTURES),
                Bool(true),
            ),
            pref(
                "Landscape Detail Textures",
                Value::Check(crate::options::names::LANDSCAPE_DETAIL_TEXTURES),
                Bool(false),
            ),
            pref(
                "Multiple Pass Alpha",
                Value::Check(crate::options::names::MULTI_PASS_ALPHA),
                Bool(false),
            ),
        ],
    },
    Heading {
        title: "Era Look",
        text: Text::literal("Era Look"),
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
        text: Text::literal("Camera and Mouse"),
        rows: &[
            ends(
                "ID_Graphics_Value_Soft",
                "ID_Graphics_Value_Hard",
                pref(
                    "Camera Stiffness",
                    Value::Slider(crate::options::names::CAMERA_STIFFNESS),
                    Float(0.45),
                ),
            ),
            ends(
                "ID_Graphics_Value_Slow",
                "ID_Graphics_Value_Fast",
                pref(
                    "Camera Adjustment Speed",
                    Value::Slider(crate::options::names::CAMERA_ADJUSTMENT_SPEED),
                    Float(40.0),
                ),
            ),
            pref(
                "Align Camera to Slope",
                Value::Check(crate::options::names::CAMERA_ALIGN_TO_SLOPE),
                Bool(true),
            ),
            pref(
                "Mouselook Sensitivity",
                Value::Slider(crate::options::names::MOUSE_LOOK_SENSITIVITY),
                Float(0.55),
            ),
            only(
                Shown::Only(Interface::Modern),
                pref(
                    "Invert Mouselook Axes",
                    Value::Check(crate::options::names::INVERT_MOUSE_LOOK_Y_AXIS),
                    Bool(false),
                ),
            ),
            only(
                Shown::Only(Interface::Classic),
                pref(
                    "Invert Mouse Look Up/Down",
                    Value::Check(crate::options::classic::INVERT_MOUSE_LOOK),
                    Bool(false),
                ),
            ),
            only(
                Shown::Only(Interface::Modern),
                pref(
                    "Turn Your Character with Camera Turning",
                    Value::Check(crate::options::names::USE_MOUSE_TURNING),
                    Bool(false),
                ),
            ),
            only(
                Shown::Only(Interface::Classic),
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
    face: Interface,
) -> impl Iterator<Item = (&'static Heading, Vec<&'static Row>)> {
    page(id).headings.iter().filter_map(move |h| {
        let rows: Vec<&'static Row> = h.rows.iter().filter(|r| r.shown.on(face)).collect();
        (!rows.is_empty()).then_some((h, rows))
    })
}

/// Every row of `page` that `face` shows, in order.
pub fn rows_for(id: PageId, face: Interface) -> impl Iterator<Item = &'static Row> {
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

/// The slider a check box greys out while the box is ticked: Adaptive Degrade Bias is the speed or
/// detail chosen by hand, and is used only while Adaptive Degrade is off.
pub const GREYED_WHILE_ON: [(&str, &str); 1] = [(
    crate::options::names::AUTOMATIC_DEGRADES,
    crate::options::names::GRAPHICS_PERFORMANCE,
)];

/// The captions under a slider's two ends, left then right, for the sliders the modern page
/// labels; both interfaces draw the same words.
pub const SLIDER_ENDS: [(&str, &str, &str); 6] = [
    (crate::options::names::CAMERA_STIFFNESS, "Soft", "Hard"),
    (
        crate::options::names::CAMERA_ADJUSTMENT_SPEED,
        "Slow",
        "Fast",
    ),
    (crate::options::names::FIELD_OF_VIEW, "Narrow", "Wide"),
    (crate::options::names::SCREEN_BRIGHTNESS, "Dark", "Bright"),
    (
        crate::options::names::GRAPHICS_PERFORMANCE,
        "Speed",
        "Detail",
    ),
    (crate::options::names::DEGRADE_DISTANCE, "Close", "Far"),
];

/// The two end captions of `preference`'s slider, if the modern page labels it.
#[must_use]
pub fn slider_ends(preference: &str) -> Option<(&'static str, &'static str)> {
    SLIDER_ENDS
        .iter()
        .find(|(p, _, _)| *p == preference)
        .map(|&(_, left, right)| (left, right))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the set's own shape; each interface's page is tested where it is drawn).
    use super::*;

    #[test]
    fn every_character_option_on_a_page_is_on_the_character_page_once() {
        let mut seen: Vec<PlayerOption> = rows_for(PageId::Character, Interface::Modern)
            .chain(rows_for(PageId::Character, Interface::Classic))
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
        for face in [Interface::Modern, Interface::Classic] {
            for p in PAGES {
                assert!(rows_for(p.id, face).count() > 0, "{face:?} {:?}", p.id);
            }
        }
        // The classic chat page is the main window's filter only.
        let classic_chat: Vec<&str> = headings_for(PageId::Chat, Interface::Classic)
            .map(|(h, _)| h.title)
            .collect();
        assert_eq!(classic_chat, ["Main Chat Window"]);
        assert_eq!(
            rows_for(PageId::Chat, Interface::Modern).count(),
            2 + 2 + 12 + 13 * 4
        );
        // The chat font's face and size are the chat page's, under the windows' opacity.
        let chat: Vec<Option<&str>> = rows_for(PageId::Chat, Interface::Modern)
            .take(4)
            .map(Row::preference)
            .collect();
        assert_eq!(
            chat[2..],
            [Some("UI.ChatFontFace"), Some("UI.ChatFontSize")]
        );
        assert!(!rows_for(PageId::Client, Interface::Modern)
            .any(|r| r.preference().is_some_and(|p| p.starts_with("UI.ChatFont"))));
    }

    #[test]
    fn each_interface_keeps_its_own_mouse_look_and_invert() {
        let has = |face, p: &str| rows_for(PageId::Client, face).any(|r| r.preference() == Some(p));
        assert!(has(Interface::Modern, "Input.InvertMouseLookYAxis"));
        assert!(!has(Interface::Classic, "Input.InvertMouseLookYAxis"));
        assert!(has(
            Interface::Classic,
            crate::options::classic::INVERT_MOUSE_LOOK
        ));
        assert!(!has(
            Interface::Modern,
            crate::options::classic::INVERT_MOUSE_LOOK
        ));
        assert!(has(Interface::Modern, "Input.UseMouseTurning"));
        assert!(!has(Interface::Classic, "Input.UseMouseTurning"));
        assert!(has(
            Interface::Classic,
            crate::options::classic::RIGHT_CLICK_MOUSE_LOOK
        ));
        // No row for vertical sync: it does nothing in a borderless window.
        assert!(!has(Interface::Modern, "Display.SyncToRefresh"));
        assert!(!has(Interface::Classic, "Display.SyncToRefresh"));
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
        let trade =
            row_of_preference(PageId::Client, crate::options::classic::SHOW_TRADE_TAB).unwrap();
        let mut features = dereth_primitives::EraFeatures::END_OF_RETAIL;
        assert!(trade.needs.met(Some(&features)));
        features.trade = false;
        assert!(!trade.needs.met(Some(&features)));
        assert!(trade.needs.met(None));
        assert!(row_of_option(P::DisableHouseRestrictionEffects)
            .unwrap()
            .needs
            .met(Some(&f)));
    }

    #[test]
    fn every_profile_row_has_a_default_and_a_registered_preference() {
        crate::options::store::init();
        for face in [Interface::Modern, Interface::Classic] {
            for r in rows_for(PageId::Client, face) {
                let p = r.preference().unwrap();
                assert!(r.default.is_some(), "{p}");
                assert!(crate::options::store::is_registered(p), "{p}");
            }
        }
    }
}
