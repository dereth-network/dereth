//! The character-generation wizard — mode `0x1000000B` — and its six pages.
//!
//! The character-generation table gives the *gameplay* meaning of the six pages: heritage and
//! profession tables, skill credits, appearance palettes, and starting towns. This module supplies
//! the wizard chrome, the six pages' element ids and types, and the progress state machine.
//!
//! # What is wired
//!
//! The model ([`dereth_chargen::CharGenState`]) and the path that actually creates
//! a character: **heritage → town → name → Finish → `0xF656` → `0xF643` → the new `0xF658` → log
//! the new character straight in**.
//!
//! The pages: the **profession** page's seven template buttons, six attribute sliders and six
//! locks; the **skills** page's list, its `+`/`-` and its credit meter; and the **appearance**
//! page's nine part rows, two gender buttons, two tabs, nine colour spots and shade slider. The
//! progress-state write's **Olthoi page-skipping** — the one place the wizard's graph is not linear
//! — is here too, and so is [`Cg3dView`], the turntable's model.
//!
//! The preview space receives the turntable's physics body, so the turntable draws. Character
//! randomization means the wizard opens on a rolled character rather than on the client's empty
//! one, and the *Random* button has its five non-summary arms. The two generators it draws from are
//! kept apart the way the client keeps them -- the dice roll (`ran2`) for the heritage and the
//! gender, the C-runtime `rand()` for the start area, the appearance, the clothing and the
//! template. See [`dereth_chargen::CharGenRng`].
//!
//! The appearance page's nine colour spots and its shade disk are *runtime-generated surfaces* --
//! the colour-spot and gradient-disk builders blit a template into a local surface and recolour it
//! rather than reading a picture out of the dat -- and applying a chosen colour to the model is
//! the appearance update's `ObjDesc` block.

use std::rc::Rc;

pub use dereth_chargen::palette::PaletteSample;

#[cfg(test)]
use dereth_assets::tables::{CharGen, SkillTable};
use dereth_primitives::num::to_i32;
use dereth_primitives::DataId;
use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::persist::CharacterSet;
use dereth_ui::props::PropertyValue;
use dereth_ui::{
    ElemHandle, ElementId, ElementMessage, ElementType, ListenerId, MessageId, UiError, UiMode,
    UiSystem,
};

use crate::bind::{bind_children, child, Bound, ChildBinding};
use crate::element_types::ty;
use crate::panels::listbox::ListBoxWidget;
use dereth_chargen::{
    Attr, CgVerification, CharGenState, SkillAdvancementClass, HERITAGE_GEAR_KNIGHT,
    HERITAGE_OLTHOI, HERITAGE_OLTHOI_ACID,
};

/// The screen's root: layout enum `0x10000039`, element `0x100003CC`.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0039);
const ROOT: ElementId = ElementId(0x1000_03CC);

/// The wizard chrome's children, by element id.
pub const CHROME: &[ChildBinding] = &[
    child("left_button", 0x1000_03C6),
    child("right_button", 0x1000_03C7),
    child("finish_button", 0x1000_03C8),
    child("help_button", 0x1000_03C9),
    child("exit_button", 0x1000_03CA),
    child("random_button", 0x1000_03CB),
    child("progress_bar", 0x1000_03CE),
    child("main_menu", 0x1000_03CF),
    child("master_page", 0x1000_03D0),
];

/// The left arrow. On [`EcgProgress::Hertage`] it is **Exit**, not "back" — see
/// [`CharGenScreen::previous_page`].
pub const LEFT_BUTTON: ElementId = ElementId(0x1000_03C6);
/// The right arrow. The progress-state write hides it on the summary page.
pub const RIGHT_BUTTON: ElementId = ElementId(0x1000_03C7);
/// The Finish button, which the shipped layout starts hidden and the progress-state write shows on
/// the summary page **only**.
pub const FINISH_BUTTON: ElementId = ElementId(0x1000_03C8);
/// The Help button. The button handler has **no arm for it**, so pressing Help does nothing in the
/// retail client either.
pub const HELP_BUTTON: ElementId = ElementId(0x1000_03C9);
/// The Exit button, the same confirm dialog the left arrow raises on page 1.
pub const EXIT_BUTTON: ElementId = ElementId(0x1000_03CA);
/// The Random button — randomises the current page on five pages and asks for confirmation on the
/// summary page.
pub const RANDOM_BUTTON: ElementId = ElementId(0x1000_03CB);

/// The shipped `Dialog` layout, enum **2** → `0x2100003C`
/// (`dereth_ui::framework::TableResolver::retail_prefix`).
///
/// The wizard builds every dialog out of this one
/// layout; `dereth_ui::dialog::DialogKind::root_element_id` is the root-per-kind table.
pub const DIALOG_LAYOUT: LayoutEnum = LayoutEnum(2);

/// The string token the exit step hashes into the `StringInfo` it hands the dialog as property
/// `0xC5`, from table enum `0x10000002`, i.e. [`ERROR_STRING_TABLE`].
pub const EXIT_WARNING_STRING: &str = "ID_CharGen_ExitWarning";

/// The dialog's property `0xC5`, the literal — *Randomizing your character will reset all
/// customizations you have made to appearance and abilities.* Table enum `0x10000002`, as every
/// other one on this screen.
pub const RANDOMIZE_WARNING_STRING: &str = "ID_CharGen_RandomizeWarning";
/// The dialog's property `0xC5`, the literal.
pub const CREDIT_WARNING_STRING: &str = "ID_CharGen_CreditWarning";
/// The dialog's property `0xC5`, the literal.
pub const TOD_WARNING_STRING: &str = "ID_CharGen_ToDRequiredWarning";

/// The wizard's progress state — note `Hertage` *(sic)*, kept as the client spells it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum EcgProgress {
    Invalid = 0,
    /// Heritage — the client's own typo.
    Hertage = 1,
    Profession = 2,
    Skills = 3,
    Appearance = 4,
    Town = 5,
    Summary = 6,
}

impl EcgProgress {
    /// The six real pages, in wizard order.
    pub const PAGES: [Self; 6] = [
        Self::Hertage,
        Self::Profession,
        Self::Skills,
        Self::Appearance,
        Self::Town,
        Self::Summary,
    ];

    /// The page's own element id and registered element type.
    #[must_use]
    pub const fn page(self) -> Option<(ElementId, ElementType)> {
        Some(match self {
            Self::Hertage => (ElementId(0x1000_03D1), ty::CG_HERITAGE_PAGE),
            Self::Profession => (ElementId(0x1000_03D2), ty::CG_PROFESSION_PAGE),
            Self::Skills => (ElementId(0x1000_03D3), ty::CG_SKILLS_PAGE),
            Self::Appearance => (ElementId(0x1000_03D4), ty::CG_APPEARANCE_PAGE),
            Self::Town => (ElementId(0x1000_03D5), ty::CG_TOWN_PAGE),
            Self::Summary => (ElementId(0x1000_03D6), ty::CG_SUMMARY_PAGE),
            Self::Invalid => return None,
        })
    }

    /// The page-select button in the strip.
    #[must_use]
    pub const fn select_button(self) -> Option<ElementId> {
        Some(match self {
            Self::Hertage => ElementId(0x1000_03EF),
            Self::Profession => ElementId(0x1000_03F0),
            Self::Skills => ElementId(0x1000_03F1),
            Self::Appearance => ElementId(0x1000_03F2),
            Self::Town => ElementId(0x1000_03F3),
            Self::Summary => ElementId(0x1000_03F4),
            Self::Invalid => return None,
        })
    }

    fn from_index(i: usize) -> Option<Self> {
        Self::PAGES.get(i).copied()
    }

    fn index(self) -> Option<usize> {
        Self::PAGES.iter().position(|p| *p == self)
    }
}

/// The six dialog contexts the character-generation screen tracks.
///
/// They are six **separate** context values on the screen, and the close-dialog handler
/// demultiplexes an answer by comparing the incoming context against each in turn — which is why
/// they are kept apart here rather than collapsed into one handle; see [`CharGenDialog::kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum CharGenDialog {
    Exit,
    PleaseWait,
    ErrorMessage,
    CreditWarning,
    RandomizeWarning,
    ToDRequired,
}

/// The context the dialog factory returns for each of the five raisable contexts -- the
/// **factory's** handle, which is what the screen's context fields actually hold in the client, as
/// distinct from the element the host builds for it.
///
/// **The screen keeps both**, because they answer different questions: the context is what
/// the close-dialog notice demultiplexes on and what the dialog factory's close
/// is given, and the element is what has to be deleted and what carries the answer.
/// Holding only the element would keep the screen off the factory's queue.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
struct DialogQueueSlots {
    exit: Option<u64>,
    error_message: Option<u64>,
    credit_warning: Option<u64>,
    randomize_warning: Option<u64>,
    tod_required: Option<u64>,
}

impl DialogQueueSlots {
    fn slot(&mut self, c: CharGenDialog) -> Option<&mut Option<u64>> {
        Some(match c {
            CharGenDialog::Exit => &mut self.exit,
            CharGenDialog::ErrorMessage => &mut self.error_message,
            CharGenDialog::CreditWarning => &mut self.credit_warning,
            CharGenDialog::RandomizeWarning => &mut self.randomize_warning,
            CharGenDialog::ToDRequired => &mut self.tod_required,
            // The please-wait context is written only by the constructor and the destructor; see
            // [`CharGenDialog::kind`].
            CharGenDialog::PleaseWait => return None,
        })
    }

    const fn get(self, c: CharGenDialog) -> Option<u64> {
        match c {
            CharGenDialog::Exit => self.exit,
            CharGenDialog::ErrorMessage => self.error_message,
            CharGenDialog::CreditWarning => self.credit_warning,
            CharGenDialog::RandomizeWarning => self.randomize_warning,
            CharGenDialog::ToDRequired => self.tod_required,
            CharGenDialog::PleaseWait => None,
        }
    }
}

impl CharGenDialog {
    /// Every context this screen can actually raise, in field order.
    ///
    /// [`Self::PleaseWait`] is **not** here: see [`Self::kind`].
    pub const RAISED: [Self; 5] = [
        Self::Exit,
        Self::ErrorMessage,
        Self::CreditWarning,
        Self::RandomizeWarning,
        Self::ToDRequired,
    ];

    /// Property **0x8E** — which of the seven dialog kinds each creation path asks for.
    ///
    /// Each kind is an enum passed alongside the context returned by the dialog factory, as in the
    /// character-management dialogs.
    ///
    /// | function | string | 0x8E | kind | context |
    /// |---|---|---:|---|---:|
    /// | the exit step | `ID_CharGen_ExitWarning` | 1 | `Confirmation` | exit warning |
    /// | *(none)* | — | — | — | please-wait slot |
    /// | caller-supplied dialog | *(the caller's `StringInfo`)* | 3 | `Message` | error message |
    /// | the credit warning dialog build | `ID_CharGen_CreditWarning` | 1 | `Confirmation` | credit warning |
    /// | the randomize warning dialog build | `ID_CharGen_RandomizeWarning` | 1 | `Confirmation` | randomize warning |
    /// | the expansion warning dialog build | `ID_CharGen_ToDRequiredWarning` | 3 | `Message` | expansion warning |
    ///
    /// All five produced dialogs are modal; the shared dialog builder writes that property
    /// unconditionally.
    ///
    /// **The same reading from the other end.** Close-dialog handling reads property `0x8E` and
    /// handles **1** and **3** and nothing else. Its kind-1 arm distinguishes the exit warning (→
    /// queue UI mode `0x1000000A`), credit warning (→ finish without the credit check), and
    /// randomize warning; its kind-3 arm clears please-wait, error-message, and expansion-warning
    /// contexts and does nothing else, because a message has no answer. Those are exactly the six
    /// fields above, each in the arm its property `0x8E` puts it in.
    ///
    /// **`PleaseWait` has no producer anywhere in the retail client.** Its context is written in
    /// exactly two places — construction and destruction of the character-generation screen, both
    /// storing zero — so the wizard, unlike character management, never raises a `Wait` dialog. The
    /// finish path's only two dialogs are the credit-warning dialog and an **inline** kind-3
    /// no-name message (`ID_CharGen_NoNameWarning`) whose returned context is **discarded**. So
    /// this arm returns `None`: the context is recorded, as the client records it, and no element
    /// is ever built for it.
    #[must_use]
    pub const fn kind(self) -> Option<dereth_ui::dialog::DialogKind> {
        use dereth_ui::dialog::DialogKind as K;
        Some(match self {
            Self::Exit | Self::CreditWarning | Self::RandomizeWarning => K::Confirmation,
            Self::ErrorMessage | Self::ToDRequired => K::Message,
            Self::PleaseWait => return None,
        })
    }

    /// The children whose element message **1** this context's subclass treats as the answer, in
    /// (accept, cancel) order:
    ///
    /// * The confirmation dialog's element-message handler — `0x17` / `0x19`, and `id == 0x17`
    ///   being the yes test is what makes **button 1 yes**;
    /// * The message dialog's element-message handler — `0x26`, its one button.
    ///
    /// **The table itself is `dereth_ui`'s and is deliberately not repeated here.**
    /// `dereth_ui::dialog::DialogKind::answer_children` is the per-subclass table, so this is one
    /// hop onto it rather than a second copy of it. The two `Message` dialogs answer on `0x26`, not
    /// the confirmation dialog's `0x17`/`0x19`.
    #[must_use]
    pub const fn answer_children(self) -> (Option<ElementId>, Option<ElementId>) {
        match self.kind() {
            Some(k) => k.answer_children(),
            // `PleaseWait` has no element and therefore no buttons.
            None => (None, None),
        }
    }
}

/// The message dialog's single button (element `0x26`), named locally so that
/// [`Screen::on_element_message`]'s arm for it reads as an id rather than as a path.
pub const MESSAGE_BUTTON: ElementId = dereth_ui::dialog::base::child::MESSAGE_BUTTON;

/// The framework's own listener id; see `charmgmt`'s `ME` for why every screen needs one.
const ME: ListenerId = ListenerId::External(LAYOUT.0);

/// The client's switch, in the order the buttons are laid out down the page. Every pair is taken
/// from the client's own mapping, not inferred from the order: the ids are **not** contiguous and
/// **not** monotonic in the heritage number (`0x10000591` is Penumbraen, 10, and sits between
/// Shadowbound and Gearknight in the layout).
///
/// Heritage numbers match ACE's `HeritageGroup`.
pub const HERITAGE_BUTTONS: [(ElementId, u32); 13] = [
    (ElementId(0x1000_03BF), 1),  // Aluvian
    (ElementId(0x1000_03C1), 2),  // Gharu'ndim
    (ElementId(0x1000_03C2), 3),  // Sho
    (ElementId(0x1000_03C3), 4),  // Viamontian — Throne of Destiny only
    (ElementId(0x1000_0590), 5),  // Shadowbound / Umbraen
    (ElementId(0x1000_05A9), 6),  // Gearknight
    (ElementId(0x1000_05E8), 7),  // Tumerok
    (ElementId(0x1000_05F1), 8),  // Lugian
    (ElementId(0x1000_05C4), 9),  // Empyrean
    (ElementId(0x1000_0591), 10), // Penumbraen
    (ElementId(0x1000_05BF), 11), // Undead
    (ElementId(0x1000_05C7), HERITAGE_OLTHOI),
    (ElementId(0x1000_05C8), HERITAGE_OLTHOI_ACID),
];

/// The heritage page's last two child lookups.
pub mod heritage_page {
    use dereth_ui::ElementId;
    /// The description pane — the update fills it with four string runs.
    pub const TEXT: ElementId = ElementId(0x1000_03C4);
    /// The background — the picture behind the preview, one state per heritage.
    pub const BACKGROUND: ElementId = ElementId(0x1000_03BE);
}

/// The char-gen heritage page's update, one row per heritage: the background state it puts the
/// picture into, the `_BonusSkills_Trained` token and the description token.
///
/// **The background states are read from the shipped client, not guessed**: thirteen state
/// arguments, each paired with the heritage button it belongs to. The thirteen buttons are
/// contiguous and appear in the page initialisation's own order.
///
/// Note the two pairs that share a description: Shadowbound (5) and Penumbraen (10) both show
/// `ID_CharGen_ShadText` — the same arm in the client — while keeping **different** backgrounds
/// (`0x10000058` and `0x10000059`).
pub const HERITAGE_PAGE: [(u32, dereth_ui::StateId, &str, &str); 13] = [
    (
        1,
        dereth_ui::StateId(0x1000_0021),
        "ID_CharGen_AluvianText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(1).unwrap(),
    ),
    (
        2,
        dereth_ui::StateId(0x1000_0022),
        "ID_CharGen_GaruText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(2).unwrap(),
    ),
    (
        3,
        dereth_ui::StateId(0x1000_0023),
        "ID_CharGen_ShoText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(3).unwrap(),
    ),
    (
        4,
        dereth_ui::StateId(0x1000_0024),
        "ID_CharGen_ViaText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(4).unwrap(),
    ),
    (
        5,
        dereth_ui::StateId(0x1000_0058),
        "ID_CharGen_ShadText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(5).unwrap(),
    ),
    (
        6,
        dereth_ui::StateId(0x1000_005A),
        "ID_CharGen_GearText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(6).unwrap(),
    ),
    (
        7,
        dereth_ui::StateId(0x1000_005F),
        "ID_CharGen_AunTText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(7).unwrap(),
    ),
    (
        8,
        dereth_ui::StateId(0x1000_0060),
        "ID_CharGen_LugText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(8).unwrap(),
    ),
    (
        9,
        dereth_ui::StateId(0x1000_005C),
        "ID_CharGen_EmpText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(9).unwrap(),
    ),
    (
        10,
        dereth_ui::StateId(0x1000_0059),
        "ID_CharGen_ShadText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(10).unwrap(),
    ),
    (
        11,
        dereth_ui::StateId(0x1000_005B),
        "ID_CharGen_UndText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(11).unwrap(),
    ),
    (
        HERITAGE_OLTHOI,
        dereth_ui::StateId(0x1000_005D),
        "ID_CharGen_OlthoiText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(HERITAGE_OLTHOI).unwrap(),
    ),
    (
        HERITAGE_OLTHOI_ACID,
        dereth_ui::StateId(0x1000_005E),
        "ID_CharGen_OlthoiAcidText_BonusSkills_Trained",
        dereth_presentation::creation::heritage_description_token(HERITAGE_OLTHOI_ACID).unwrap(),
    ),
];

/// The two heritage-independent runs the update puts above the per-heritage ones.
pub const HERITAGE_SKILLS_HEADER: &str = "ID_CharGen_Heritage_StartingSkills_Header";
/// See [`HERITAGE_SKILLS_HEADER`].
pub const HERITAGE_SKILLS_BODY: &str = "ID_CharGen_Heritage_StartingSkills";
/// See [`HERITAGE_SKILLS_HEADER`].
pub const HERITAGE_BONUS_HEADER: &str = "ID_CharGen_Heritage_BonusSkills_Trained_Header";

/// The one heritage that the heritage-page switch gates on the expansion entitlement.
///
/// Only Viamontian: the other twelve cases fall straight through to the heritage write, the two
/// Olthoi included. The document's "blocks heritages that need the Throne of Destiny expansion" is
/// exactly one heritage in this build.
pub const TOD_HERITAGE: u32 = 4;

/// The town page's element-message path sets the town and then the start area.
///
/// It computes `start area = town - Holtburg`, while the Sanamar case bypasses that arithmetic with
/// literal start area 3. So start areas index the character-generation data's **global**
/// starter-area list, in the order Holtburg, Shoushi, Yaraq, Sanamar.
pub const TOWN_BUTTONS: [(ElementId, u32); 4] = [
    (ElementId(0x1000_040D), 0), // Holtburg
    (ElementId(0x1000_040F), 1), // Shoushi
    (ElementId(0x1000_040E), 2), // Yaraq
    (ElementId(0x1000_040B), 3), // Sanamar — Throne of Destiny only
];

/// The start area the town page gates on the expansion entitlement.
pub const TOD_START_AREA: u32 = 3;

const WORLD_TOWN_BUTTON_BASE: u32 = 0x7F01_0000;

/// Pixels a map unit, should the map element be missing.
const DEFAULT_MAP_SCALE: f64 = 480.0 / town_page::MAP_SPAN;

/// A live element's description with its live children's own, so a copy needs nothing resolved
/// from the data files.
fn full_desc(ui: &UiSystem, h: ElemHandle) -> Option<dereth_ui::ElementDesc> {
    let mut d = ui.node(h)?.desc.clone();
    d.children.clear();
    for c in ui.children(h) {
        let cd = full_desc(ui, c)?;
        d.children.insert(cd.element_id, cd);
    }
    Some(d)
}

/// No data files: every description handed to the builder here is already whole.
#[derive(Debug)]
struct NoAssets;
impl dereth_primitives::AssetSource for NoAssets {
    fn read(&self, id: DataId) -> Result<Vec<u8>, dereth_primitives::AssetError> {
        Err(dereth_primitives::AssetError::NotFound(id))
    }
    fn exists(&self, _: DataId) -> bool {
        false
    }
    fn iter_type(&self, _: dereth_primitives::DataType) -> Box<dyn Iterator<Item = DataId> + '_> {
        Box::new(std::iter::empty())
    }
}

/// The town page's page initialisation and the town write.
pub mod town_page {
    use dereth_ui::{ElementId, StateId};
    /// The description pane.
    pub const TEXT: ElementId = ElementId(0x1000_0409);
    /// The town's name, drawn large above the pane. It carries the same four states as the page
    /// itself, which is what the town write's call on the page reaches.
    pub const TITLE: ElementId = ElementId(0x1000_0408);
    /// The town write's "not this one" state — the four map pins all get it first.
    pub const PIN_OFF: StateId = StateId(1);
    /// …and the chosen one gets this.
    pub const PIN_ON: StateId = StateId(6);
    /// The page's own state per **start area**, i.e. the town write's `town - Holtburg`.
    ///
    /// **Not in state-id order.** The town setter is a four-way
    /// switch on `town - 1`, and its branches give
    /// Holtburg `0x10000034`, **Shoushi `0x10000037`**, Yaraq `0x10000036` and **Sanamar
    /// `0x10000035`**. The title element `0x10000408` carries a `StringInfo` per state, and those
    /// four strings resolve to the four town names in exactly that pairing — which is what caught
    /// the ascending-order guess this array first held.
    pub const TITLE_STATES: [StateId; 4] = [
        StateId(0x1000_0034),
        StateId(0x1000_0037),
        StateId(0x1000_0036),
        StateId(0x1000_0035),
    ];
    /// The client's per-town token, substituted into `ID_CharGen_TownHowTo`.
    pub const TEXT_TOKENS: [&str; 4] = [
        "ID_CharGen_HoltText",
        "ID_CharGen_ShoushiText",
        "ID_CharGen_YaraqText",
        "ID_CharGen_SanamarText",
    ];
    /// The frame the town-string write puts the town's own text inside.
    pub const HOW_TO: &str = "ID_CharGen_TownHowTo";
    /// The map of Dereth the pins sit on.
    pub const MAP: ElementId = ElementId(0x1000_040A);
    /// A pin's dot, under its name.
    pub const PIN_DOT: ElementId = ElementId(0x1000_040C);
    /// A pin's name.
    pub const PIN_NAME: ElementId = ElementId(0x1000_0409);
    /// How many map units (a tenth of a degree, 240 metres) the map spans, edge to edge: the
    /// world's 255 landblocks of 192 metres.
    pub const MAP_SPAN: f64 = 255.0 * 192.0 / 240.0;
    /// Where each pin's town lies, as map coordinates (east, north): Holtburg, Shoushi and Yaraq.
    /// Sanamar's start is the only one by its town, so its dot needs no side.
    pub const TOWN_CENTRES: [Option<(f64, f64)>; 4] = [
        Some((33.6, 42.1)),
        Some((73.1, -33.6)),
        Some((-1.8, -21.5)),
        None,
    ];
    /// The least distance from a town's dot to one of its starts' dots.
    pub const DOT_DISTANCE: f64 = 28.0;

    /// A position as map coordinates (east, north): the landblock's place on the world grid plus
    /// the offset in it.
    #[must_use]
    pub fn map_coordinates(cell: u32, x: f32, y: f32) -> (f64, f64) {
        let block = |b: u32, at: f32| (f64::from(b) * 192.0 + f64::from(at)) / 240.0 - 101.95;
        (block(cell >> 24, x), block((cell >> 16) & 0xFF, y))
    }

    /// The screen point of a start's dot. The town's starts are laid in order round the town's
    /// dot: each toward its own map place, as far out as the map puts it but at least
    /// [`DOT_DISTANCE`], turned a step at a time either way, and failing that set a ring further
    /// out, until its `size`-pixel dot overlaps no start laid before it and does not cover the
    /// town's name (its centre and size). `starts` are all the town's starts, `start` among them.
    #[must_use]
    pub fn dot_at(
        town_dot: (f64, f64),
        town: Option<(f64, f64)>,
        start: (f64, f64),
        starts: &[(f64, f64)],
        scale: f64,
        size: f64,
        name: Option<((f64, f64), (f64, f64))>,
    ) -> (i32, i32) {
        use dereth_primitives::num::math;
        const STEP: f64 = std::f64::consts::PI / 12.0;
        // Screen y runs south.
        let offset = |at: (f64, f64)| {
            let (dx, dy) = town.map_or((0.0, 0.0), |town| {
                ((at.0 - town.0) * scale, (town.1 - at.1) * scale)
            });
            let length = math::hypot(dx, dy);
            let angle = if length > f64::EPSILON {
                math::atan2(dy, dx)
            } else {
                std::f64::consts::FRAC_PI_2
            };
            (angle, length.max(DOT_DISTANCE))
        };
        let covers_name = |p: (f64, f64)| {
            name.is_some_and(|((x, y), (w, h))| {
                (p.0 - x).abs() < (size + w) / 2.0 && (p.1 - y).abs() < (size + h) / 2.0
            })
        };
        let clear = |p: (f64, f64), laid: &[(f64, f64)]| {
            !covers_name(p)
                && laid
                    .iter()
                    .all(|q| (p.0 - q.0).abs() >= size || (p.1 - q.1).abs() >= size)
        };
        let lay = |at: (f64, f64), laid: &[(f64, f64)]| {
            let (angle, out) = offset(at);
            let point = |turn: f64, out: f64| {
                (
                    town_dot.0 + out * math::cos(angle + turn),
                    town_dot.1 + out * math::sin(angle + turn),
                )
            };
            (0..4)
                .flat_map(|ring| {
                    (0..=12).flat_map(move |k| {
                        let k = f64::from(k);
                        [(k * STEP, ring), (-k * STEP, ring)]
                    })
                })
                .map(|(turn, ring)| point(turn, out + f64::from(ring) * size / 4.0))
                .find(|&p| clear(p, laid))
                .unwrap_or_else(|| point(0.0, out))
        };
        let mut laid = Vec::new();
        let mut own = None;
        for &at in starts {
            let p = lay(at, &laid);
            if own.is_none() && at == start {
                own = Some(p);
            }
            laid.push(p);
        }
        let (x, y) = own.unwrap_or_else(|| lay(start, &laid));
        let round = |v: f64| dereth_primitives::num::to_i32_f64(v.round());
        (round(x), round(y))
    }
}

/// The client's five children and the three list templates.
pub mod summary_page {
    use dereth_ui::ElementId;
    /// The summary list box.
    pub const LIST: ElementId = ElementId(0x1000_0400);
    /// The list's scroll bar.
    pub const SCROLL: ElementId = ElementId(0x1000_0401);
    /// The how-to text.
    pub const HOW_TO: ElementId = ElementId(0x1000_0404);
    /// Template **0**'s one text child — one plain line.
    pub const LINE_TEXT: ElementId = ElementId(0x1000_02F9);
    /// Template **1**'s one text child — a category header.
    pub const HEADER_TEXT: ElementId = ElementId(0x1000_00FE);
    /// Template **2**'s left column.
    pub const PAIR_NAME: ElementId = ElementId(0x1000_02FC);
    /// Template **2**'s right column.
    pub const PAIR_VALUE: ElementId = ElementId(0x1000_02FD);
}

/// The client's ten `(label, value)` rows, in the order its ten-row loop emits them. The three
/// vitals are the client's own endurance/2, endurance, self — the
/// same three values the profession page prints.
pub const SUMMARY_ROWS: [&str; 10] = [
    "Strength",
    "Endurance",
    "Coordination",
    "Quickness",
    "Focus",
    "Self",
    "Health",
    "Stamina",
    "Mana",
    "Skill Credits",
];

/// The summary text's four skill sections: the header it writes and the [`SkillAdvancementClass`] a
/// row must be at to appear under it.
///
/// The last two both collect **untrained** skills and are separated by the `SkillBase`'s
/// `chargen_use`: `< 2` is useable, `> 1` is not (the client's two arms).
pub const SUMMARY_SKILL_SECTIONS: [(&str, SkillAdvancementClass); 4] = [
    ("Specialized Skills", SkillAdvancementClass::Specialized),
    ("Trained Skills", SkillAdvancementClass::Trained),
    ("Useable Untrained Skills", SkillAdvancementClass::Untrained),
    (
        "Unuseable Untrained Skills",
        SkillAdvancementClass::Untrained,
    ),
];

/// Seven wide strings, indexed by the template.
///
/// Built into the retail client, not taken from a string table: summary construction hard-codes
/// `L"Profession: "` and these four arrays, so the summary page is the one place in the wizard that
/// is not localised.
pub const PROFESSION_NAMES: [&str; 7] = [
    "Custom",
    "Bow Hunter",
    "Swashbuckler",
    "Life Caster",
    "War Mage",
    "Wayfarer",
    "Soldier",
];
/// The client's gender-name table, indexed by the gender. Index 0 really is `"?"` in retail.
pub const GENDER_NAMES: [&str; 3] = ["?", "Male", "Female"];
/// The client's heritage-name table, indexed by the heritage — **five entries**, so the summary
/// page names only the four original heritages and prints nothing for the other nine. That is the
/// client's own `if (heritage < 5)` and is kept.
pub const HERITAGE_NAMES: [&str; 5] = ["?", "Aluvian", "Gharu'ndim", "Sho", "Viamontian"];
/// The town names, indexed by the start area.
pub const TOWN_NAMES: [&str; 4] = ["Holtburg", "Shoushi", "Yaraq", "Sanamar"];

/// The client's three runs: this, the heritage-and-gender name-examples token, then
/// [`SUMMARY_HOW_TO_END`].
pub const SUMMARY_HOW_TO: &str = "ID_CharGen_SummaryHowTo";
/// See [`SUMMARY_HOW_TO`].
pub const SUMMARY_HOW_TO_END: &str = "ID_CharGen_SummaryHowToEnd";

/// The how-to text's switch — the naming-convention examples, `(heritage, male token, female
/// token)`.
///
/// Gender 2 is female here (the client's own gender enum calls 1 female, which disagrees with ACE's
/// naming; see [`CharGenScreen::choose_heritage`]), so the **`== 2`** arm is the female one, as the
/// client writes it. Heritages 5 and 10 share a pair and so do 12 and 13, which is the same
/// Shadowbound/Penumbraen and Olthoi/Olthoi-Acid pairing the heritage page has.
///
/// The switch has thirteen cases (heritages 1 to 13), each naming the female and the male token.
/// Heritages 5 and 10 share one arm and one pair, and 12 and 13 share another — the Olthoi pair
/// has no acid-specific names.
pub const SUMMARY_NAME_EXAMPLES: [(u32, &str, &str); 13] = [
    (1, "ID_CharGen_AluMaleNames", "ID_CharGen_AluFemaleNames"),
    (
        2,
        "ID_CharGen_GharuMaleNames",
        "ID_CharGen_GharuFemaleNames",
    ),
    (3, "ID_CharGen_ShoMaleNames", "ID_CharGen_ShoFemaleNames"),
    (4, "ID_CharGen_ViaMaleNames", "ID_CharGen_ViaFemaleNames"),
    (5, "ID_CharGen_ShadMaleNames", "ID_CharGen_ShadFemaleNames"),
    (6, "ID_CharGen_GearMaleNames", "ID_CharGen_GearFemaleNames"),
    (7, "ID_CharGen_AunTMaleNames", "ID_CharGen_AunTFemaleNames"),
    (8, "ID_CharGen_LugMaleNames", "ID_CharGen_LugFemaleNames"),
    (9, "ID_CharGen_EmpMaleNames", "ID_CharGen_EmpFemaleNames"),
    (10, "ID_CharGen_ShadMaleNames", "ID_CharGen_ShadFemaleNames"),
    (11, "ID_CharGen_UndMaleNames", "ID_CharGen_UndFemaleNames"),
    (
        HERITAGE_OLTHOI,
        "ID_CharGen_OlthoiMaleNames",
        "ID_CharGen_OlthoiFemaleNames",
    ),
    (
        HERITAGE_OLTHOI_ACID,
        "ID_CharGen_OlthoiMaleNames",
        "ID_CharGen_OlthoiFemaleNames",
    ),
];

/// The tab strip's two states, read off the shipped `chargen_master` layout: `0x100003EF`..`F4` each
/// declare 1, 3 and 6, with a dim caption colour in 1 and a bright one in 6.
///
/// The progress-state write sets **all six** to [`STATE_TAB_OFF`] and then the current page's to
/// [`STATE_TAB_ON`]; the 6 is the state id.
pub const STATE_TAB_OFF: dereth_ui::StateId = dereth_ui::StateId(1);
/// See [`STATE_TAB_OFF`].
pub const STATE_TAB_ON: dereth_ui::StateId = dereth_ui::StateId(6);

/// The summary page's name box (element messages `0x12` and `0x44`).
pub const NAME_FIELD: ElementId = ElementId(0x1000_0402);

/// `ID_CharGen_NamePrompt`, which the summary page puts in the empty name box.
pub const NAME_PROMPT: &str = "ID_CharGen_NamePrompt";

/// The string table the shipped char-gen layouts' own `StringInfo`s name.
///
/// Read out of the data, not assumed: element `0x100003AF` of layout `0x21000049`
/// (`chargen_appearance`) carries its caption as a `StringInfo` with
/// `table_id = 0x23000002`, and every other char-gen token in these layouts uses the same table.
/// The client reaches it as table **enum** `0x10000002` through enum-to-DataID lookup; this crate
/// has no data cache, so the resolved id is what crosses the seam.
pub const ERROR_STRING_TABLE: dereth_primitives::DataId = dereth_primitives::DataId(0x2300_0002);

/// The six literals the client puts in each slider's attribute-name text.
#[must_use]
pub const fn attribute_name(a: Attr) -> &'static str {
    match a {
        Attr::Strength => "Strength",
        Attr::Endurance => "Endurance",
        Attr::Quickness => "Quickness",
        Attr::Coordination => "Coordination",
        Attr::Focus => "Focus",
        Attr::Self_ => "Self",
    }
}

/// The profession page's floor, applied after converting the scrollbar value to an integer: a value
/// below 10 becomes 10. It is the same attribute floor the model carries and is duplicated on the
/// page.
const ATTR_FLOOR: i32 = 10;

// =================================================================================================
// The profession page — element type `0x1000003A`
// =================================================================================================

/// The client's switch, and the description token the profession update shows for each.
///
/// **The ids are not in template order and the pairs are the client's own, not a pattern**: the
/// Swashbuckler button `0x100003DF` is template **2** and sits after Soldier's `0x100003DE` in the
/// id space while sitting seventh down the page. The `(template, token)` half is the profession
/// update's own switch, case by case.
pub const PROFESSION_BUTTONS: [(ElementId, i32, &str); 7] = [
    (
        ElementId(0x1000_03D9),
        0,
        dereth_presentation::creation::profession_description_token(0).unwrap(),
    ),
    (
        ElementId(0x1000_03DA),
        1,
        dereth_presentation::creation::profession_description_token(1).unwrap(),
    ),
    (
        ElementId(0x1000_03DB),
        3,
        dereth_presentation::creation::profession_description_token(3).unwrap(),
    ),
    (
        ElementId(0x1000_03DC),
        4,
        dereth_presentation::creation::profession_description_token(4).unwrap(),
    ),
    (
        ElementId(0x1000_03DD),
        5,
        dereth_presentation::creation::profession_description_token(5).unwrap(),
    ),
    (
        ElementId(0x1000_03DE),
        6,
        dereth_presentation::creation::profession_description_token(6).unwrap(),
    ),
    (
        ElementId(0x1000_03DF),
        2,
        dereth_presentation::creation::profession_description_token(2).unwrap(),
    ),
];

/// The profession page's slider-index switch, and the page initialisation's six child lookups,
/// which agree: the six slider **fields** are contiguous in id and in *display* order, and the
/// attribute they carry is not.
///
/// `0x100003E8` is **coordination (4)** and `0x100003E9` is **quickness (3)** — the same crossing
/// [`Attr`] documents, seen here from the layout side.
pub const ATTRIBUTE_SLIDERS: [(ElementId, Attr); 6] = [
    (ElementId(0x1000_03E6), Attr::Strength),
    (ElementId(0x1000_03E7), Attr::Endurance),
    (ElementId(0x1000_03E8), Attr::Coordination),
    (ElementId(0x1000_03E9), Attr::Quickness),
    (ElementId(0x1000_03EA), Attr::Focus),
    (ElementId(0x1000_03EB), Attr::Self_),
];

/// The four children every slider field carries. All six fields carry the **same** four ids, which
/// is why the element-message handler resolves the attribute from the message's *parent* (the
/// slider index of the element's parent) and not from its own element id.
pub mod slider {
    use dereth_ui::ElementId;
    /// The slider's lock button.
    pub const LOCK: ElementId = ElementId(0x1000_02EC);
    /// The attribute's name, written once by the page initialisation.
    pub const NAME: ElementId = ElementId(0x1000_02ED);
    /// The attribute scrollbar; its position change is element message **10**.
    pub const SCROLL: ElementId = ElementId(0x1000_02EE);
    /// The editable number.
    pub const VALUE: ElementId = ElementId(0x1000_02EF);
}

/// The client's remaining four reads: a field, then one text child inside it.
pub const PROFESSION_READOUTS: [(ElementId, ElementId); 4] = [
    (ElementId(0x1000_03E2), ElementId(0x1000_02F1)), // credits available
    (ElementId(0x1000_03E3), ElementId(0x1000_02F3)), // health
    (ElementId(0x1000_03E4), ElementId(0x1000_02F3)), // stamina
    (ElementId(0x1000_03E5), ElementId(0x1000_02F3)), // mana
];

/// The profession blurb's text box.
pub const PROFESSION_DESC: ElementId = ElementId(0x1000_03E0);

/// The scrollbar's normalised position, written as attribute `0x86` with `value * 0.01`.
///
/// Not in [`dereth_ui::props::attr`] because no other screen writes it; the scrollbar's own
/// attribute block is the UI crate's and this is the only reader among the screens.
pub const ATTR_SCROLL_POSITION: u32 = 0x86;

/// The client's two lock states — locked is `0x10000019`, unlocked `0x10000018`.
pub const STATE_SLIDER_UNLOCKED: dereth_ui::StateId = dereth_ui::StateId(0x1000_0018);
/// See [`STATE_SLIDER_UNLOCKED`].
pub const STATE_SLIDER_LOCKED: dereth_ui::StateId = dereth_ui::StateId(0x1000_0019);
/// The client's two current-selection states — state 1 on the row that was selected and state 6 on
/// the newly selected row. The same `NORMAL`/`TOGGLED` pair the zoom halves use, and not the
/// `0x1000001x` layout states the profession and heritage bullets use.
pub const STATE_ROW_UNSELECTED: dereth_ui::StateId = dereth_ui::widgets::button::state::NORMAL;
/// See [`STATE_ROW_UNSELECTED`].
pub const STATE_ROW_SELECTED: dereth_ui::StateId = dereth_ui::widgets::button::state::TOGGLED;

/// The client's two profession-button states: the previously-current button is put back to
/// `0x10000016` and the new one to `0x10000017`.
pub const STATE_PROFESSION_OFF: dereth_ui::StateId = dereth_ui::StateId(0x1000_0016);
/// See [`STATE_PROFESSION_OFF`].
pub const STATE_PROFESSION_ON: dereth_ui::StateId = dereth_ui::StateId(0x1000_0017);

// =================================================================================================
// The skills page — element type `0x1000003B`
// =================================================================================================

/// The skills page's own three element ids and
/// the skill-record traversal step.
pub mod skills_page {
    use dereth_ui::ElementId;
    /// The skills list box. Selection change is element message **4**.
    pub const LIST: ElementId = ElementId(0x1000_03F7);
    /// The credits meter's field, whose `0x100002F3` child is the number.
    pub const CREDITS_FIELD: ElementId = ElementId(0x1000_03F9);
    /// The text child every `0x100003Ex` readout field carries.
    pub const READOUT_TEXT: ElementId = ElementId(0x1000_02F3);
    /// The show skills text's description pane.
    pub const DESCRIPTION: ElementId = ElementId(0x1000_03FD);

    /// The six children of skill-row template `0x100002FF`, in their observed store order.
    ///
    /// **Which id has which role follows the client's own stores, not a guess.** Each recursive
    /// child lookup was followed to the text or button operation it feeds:
    /// This yields the six roles in the exact order below.
    ///
    /// ```text
    /// 0x10000301 -> the skill's name
    /// 0x10000302 -> the "Skill Level" number
    /// 0x10000303 -> the credits the + arrow costs
    /// 0x10000306 -> the credits the - arrow returns
    /// 0x10000305 -> the decrease button
    /// 0x10000304 -> the increase button
    /// ```
    ///
    /// `0x10000306` is **not** a formula: it is the
    /// lower-cost number. The formula text belongs to the description pane, not to a row element.
    pub const ROW_NAME: ElementId = ElementId(0x1000_0301);
    /// The skill-level text; the current skill score is written here as a **number**. See
    /// [`ROW_NAME`].
    pub const ROW_LEVEL: ElementId = ElementId(0x1000_0302);
    /// The cost printed to the left of the **+** arrow. See
    /// [`ROW_NAME`].
    pub const ROW_UP_COST: ElementId = ElementId(0x1000_0303);
    /// The **+** button. The increase skill level.
    pub const ROW_INCREASE: ElementId = ElementId(0x1000_0304);
    /// The **-** button. The decrease skill level.
    pub const ROW_DECREASE: ElementId = ElementId(0x1000_0305);
    /// The cost printed to the right of the **-** arrow. See
    /// [`ROW_NAME`].
    pub const ROW_DOWN_COST: ElementId = ElementId(0x1000_0306);
    /// The category header template `0x100002F4`'s one text child.
    pub const HEADER_TEXT: ElementId = ElementId(0x1000_02F6);
}

/// Instance-id attribute `0x1000000A` = the skill id on the row — how a row says which skill it is,
/// and the only thing the skill-level increase reads off it.
pub const ATTR_ROW_SKILL_ID: u32 = 0x1000_000A;

/// The client's two states for a row's arrows: `0x1000001A` is the arrow that will refuse the click
/// and `0x1000001B` the one that will take it. Every one of the six state writes in that function
/// uses one of these two.
pub const STATE_ARROW_OFF: dereth_ui::StateId = dereth_ui::StateId(0x1000_001A);
/// See [`STATE_ARROW_OFF`].
pub const STATE_ARROW_ON: dereth_ui::StateId = dereth_ui::StateId(0x1000_001B);

/// The list's template list — element property **`0x64`**.
///
/// An `Array` of `Struct`s whose members are `0x63` (the template's layout, a `DataFile`) and
/// `0x62` (the template's element id, an `Enum`). Adding an item from template `n` indexes this
/// array. Read from the shipped `chargen_skills` layout
/// `0x21000048`: the skills list `0x100003F7` carries two entries, both in layout `0x2100004C` —
/// element `0x100002F4` (the category header) and `0x100002FF` (a skill row) — and `0x2100004C` is
/// named by no `DidMapper` enum at all, which is why the enum path cannot reach it.
pub const ATTR_TEMPLATE_LIST: u32 = 0x64;
/// The template struct's layout member.
pub const ATTR_TEMPLATE_LAYOUT: u32 = 0x63;
/// The template struct's element member.
pub const ATTR_TEMPLATE_ELEMENT: u32 = 0x62;

/// The client's four category rows, in the order it builds them, with the
/// `StringInfo` and tooltip tokens it gives each.
pub const SKILL_CATEGORIES: [(&str, &str); 4] = [
    ("ID_CharGen_Specialized", "ID_CharGen_Specialized_Tooltip"),
    ("ID_CharGen_Trained", "ID_CharGen_Trained_Tooltip"),
    (
        "ID_CharGen_UseableUntrained",
        "ID_CharGen_UseableUntrained_Tooltip",
    ),
    (
        "ID_CharGen_UnuseableUntrained",
        "ID_CharGen_UnuseableUntrained_Tooltip",
    ),
];

/// One skill record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillRecord {
    /// The skill id.
    pub skill: u32,
    /// The `SkillBase`'s name, which is the row's caption.
    pub name: String,
    /// The train cost after the heritage override.
    pub train_cost: i32,
    /// The specialize cost after the heritage override.
    pub spec_cost: i32,
    /// **False means the skill cannot be given up**.
    ///
    /// The skill-record build computes it as `trained cost != 0` from the `SkillTable` base and
    /// then clears it when the heritage's own normal cost for the skill is 0, so a heritage's free
    /// skills are exactly the ones a player cannot untrain. The client's field name reads the other
    /// way round and is kept.
    pub untrainable: bool,
    /// The same shape against the specialized cost / the heritage's primary cost.
    pub unspecializable: bool,
    /// The current advancement class.
    pub level: SkillAdvancementClass,
    /// **The number the *Skill Level* column shows**.
    ///
    /// The page recomputes it on every call, so it rises by 5 the moment a skill
    /// is trained and by another 5 when it is specialised. It is not the *section header's* string.
    pub score: i32,
    /// The `SkillBase`'s minimum level, copied in the skill-record build.
    ///
    /// The list split puts the untrained skills on `min_level < 2`: below that they go under
    /// *Useable Untrained Skills* and at or above it under *Unuseable Untrained Skills*. It is the
    /// same split `panels::skills::SkillGroup::of` makes in the in-game panel.
    pub min_level: u32,
    /// The row element, once one exists.
    pub element: Option<ElemHandle>,
}

/// Set the text of one named child of a skill row.
fn set_row_text(ui: &mut UiSystem, row: ElemHandle, id: ElementId, text: &str) {
    if let Some(t) = ui
        .get_child_recursive(row, id)
        .and_then(|c| ui.text_element_mut(c))
    {
        t.set_text(text);
    }
}

/// The client's switch: the index into [`SKILL_CATEGORIES`] — and so into
/// `self.skill_headers` — that a record's row belongs **under**.
///
/// Untrained skills go under *Useable Untrained* when the minimum level is below 2 and under
/// *Unuseable Untrained* otherwise; trained skills under *Trained*; specialized under
/// *Specialized*. Each is inserted sorted between its header and the next.
///
/// `None` is the client's own fall-through for an undefined class: the row has already been taken
/// out of the list by then and no arm puts it back, so an inactive skill has no row.
#[must_use]
pub const fn skill_group(level: SkillAdvancementClass, min_level: u32) -> Option<usize> {
    match level {
        SkillAdvancementClass::Specialized => Some(0),
        SkillAdvancementClass::Trained => Some(1),
        SkillAdvancementClass::Untrained if min_level < 2 => Some(2),
        SkillAdvancementClass::Untrained => Some(3),
        SkillAdvancementClass::Inactive => None,
    }
}

// =================================================================================================
// The appearance page — element type `0x1000003C`
// =================================================================================================

/// The appearance parts, in the client's enum order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum EParts {
    Invalid = 0,
    Hair = 1,
    Eyes = 2,
    Nose = 3,
    Mouth = 4,
    Skin = 5,
    Headgear = 6,
    Shirt = 7,
    Trousers = 8,
    Footwear = 9,
}

impl EParts {
    /// The choices array's index, which is **not** the enum value: the page fills `[0]` hair, `[1]`
    /// eyes, `[2]` nose, `[3]` mouth, `[5]` headgear, `[6]` shirt, `[7]` trousers and `[8]`
    /// footwear, and never touches `[4]` — skin has a shade and no strip.
    #[must_use]
    pub const fn choice_index(self) -> usize {
        match self {
            Self::Invalid => 0,
            Self::Hair => 0,
            Self::Eyes => 1,
            Self::Nose => 2,
            Self::Mouth => 3,
            Self::Skin => 4,
            Self::Headgear => 5,
            Self::Shirt => 6,
            Self::Trousers => 7,
            Self::Footwear => 8,
        }
    }
}

/// The nine part rows, from the part-selection switch and from the arrows' parent switch, which
/// name the same ids.
pub const APPEARANCE_ROWS: [(ElementId, EParts); 9] = [
    (ElementId(0x1000_03AF), EParts::Hair),
    (ElementId(0x1000_03B0), EParts::Eyes),
    (ElementId(0x1000_03B1), EParts::Nose),
    (ElementId(0x1000_03B2), EParts::Mouth),
    (ElementId(0x1000_03B3), EParts::Skin),
    (ElementId(0x1000_03B5), EParts::Headgear),
    (ElementId(0x1000_03B6), EParts::Shirt),
    (ElementId(0x1000_03B7), EParts::Trousers),
    (ElementId(0x1000_03B8), EParts::Footwear),
];

/// The client's three arms, as the row labels each puts on the
/// **Hair**, **Eyes** and **Skin** spinners, in that order.
///
/// Each arm writes three captions: the ordinary trio, Gear Knight's three replacements, and the
/// Olthoi's three replacements.
///
/// **It is three per arm and not four.** There are exactly three caption writes in each of the
/// three arms and no fourth string anywhere in the update. The fourth *visible* difference between
/// the arms is the skin row's move — see [`SKIN_ROW_Y_SPECIAL`].
pub const APPEARANCE_CAPTIONS_ORDINARY: [&str; 3] =
    ["ID_CharGen_HairStyle", "ID_CharGen_Eyes", "ID_CharGen_Skin"];
/// Heritage **6**, Gear Knight. See [`APPEARANCE_CAPTIONS_ORDINARY`].
pub const APPEARANCE_CAPTIONS_GEAR: [&str; 3] = [
    "ID_CharGen_GearText_HairButton",
    "ID_CharGen_GearText_EyesButton",
    "ID_CharGen_GearText_SkinButton",
];
/// Heritages **0x0C** and **0x0D**, the two Olthoi. See [`APPEARANCE_CAPTIONS_ORDINARY`].
pub const APPEARANCE_CAPTIONS_OLTHOI: [&str; 3] = [
    "ID_CharGen_OlthoiText_HairButton",
    "ID_CharGen_OlthoiText_EyesButton",
    "ID_CharGen_OlthoiText_SkinButton",
];

/// Move the skin row to `(0, 90)` through the element's position-changing operation.
/// The destination coordinates are measured directly.
///
/// The Skin row **slides up** on the three special heritages, into the space left by the hidden Nose and
/// Mouth rows. Olthoi uses y=90 rather than y=180 on the other ten. Without it the
/// page has a two-row gap above a Skin row that is still at its ordinary height, which is a
/// different picture from retail's.
pub const SKIN_ROW_Y_SPECIAL: i32 = 0x5A;
/// See [`SKIN_ROW_Y_SPECIAL`] — the ordinary ten heritages' Skin row height.
pub const SKIN_ROW_Y_ORDINARY: i32 = 0xB4;

/// The appearance page's fixed ids.
pub mod appearance {
    use dereth_ui::ElementId;
    /// The **previous** arrow inside every part row; the row is its parent.
    pub const ARROW_PREV: ElementId = ElementId(0x1000_030A);
    /// The **next** arrow.
    pub const ARROW_NEXT: ElementId = ElementId(0x1000_030B);
    /// The female button — the ♀ symbol on the left, which sets **gender 2**.
    ///
    /// **The constant names follow the button, not the client's gender enum.** The page binds the
    /// female button to `0x100003A7` and the male button to `0x100003A8`; the client's two arms are
    /// `0x100003A7 → gender 2` and `0x100003A8 → gender 1`; and the update lights the male button
    /// for gender **1**. The gender-name table agrees: index 1 is `"Male"`. The client's own gender
    /// enum calling 1 "female" is a misnomer, and the summary page and the wire both read 1 as male
    /// — which is also what ACE's `PlayerFactory.Create` does.
    pub const GENDER_FEMALE: ElementId = ElementId(0x1000_03A7);
    /// The male button — the ♂ symbol on the right, which sets **gender 1**. See [`GENDER_FEMALE`].
    pub const GENDER_MALE: ElementId = ElementId(0x1000_03A8);
    /// Choose the Face tab, then select the hair row.
    pub const TAB_FACE: ElementId = ElementId(0x1000_03A9);
    /// Choose the Clothes tab, then select the headgear row.
    pub const TAB_CLOTHES: ElementId = ElementId(0x1000_03AA);
    /// The face group, hidden while the clothes tab is up.
    pub const GROUP_FACE: ElementId = ElementId(0x1000_03AE);
    /// The clothes group.
    pub const GROUP_CLOTHES: ElementId = ElementId(0x1000_03B4);
    /// The nine colour spots, colours `0..8`.
    pub const COLOR_SPOTS: [ElementId; 9] = [
        ElementId(0x1000_030F),
        ElementId(0x1000_0310),
        ElementId(0x1000_0311),
        ElementId(0x1000_0312),
        ElementId(0x1000_0313),
        ElementId(0x1000_0314),
        ElementId(0x1000_0315),
        ElementId(0x1000_0316),
        ElementId(0x1000_0317),
    ];
    /// The colour-wheel pointer — the marker over the chosen spot, which hides on the old index and
    /// shows on the new. Contiguous, one per spot.
    pub const COLOR_POINTERS: [ElementId; 9] = [
        ElementId(0x1000_0318),
        ElementId(0x1000_0319),
        ElementId(0x1000_031A),
        ElementId(0x1000_031B),
        ElementId(0x1000_031C),
        ElementId(0x1000_031D),
        ElementId(0x1000_031E),
        ElementId(0x1000_031F),
        ElementId(0x1000_0320),
    ];
    /// The gradient circle — the shade disk under the nine spots, whose picture the gradient-disk
    /// builder generates.
    pub const GRAD_CIRCLE: ElementId = ElementId(0x1000_030E);
    /// The shade scroll bar, element message **10**.
    pub const SHADE_SCROLL: ElementId = ElementId(0x1000_0321);
    /// Rotate clockwise.
    pub const ROTATE_CW: ElementId = ElementId(0x1000_0323);
    /// Rotate counter-clockwise.
    pub const ROTATE_CCW: ElementId = ElementId(0x1000_0324);
    /// The *Hair Style* row, and the client's first swapped caption. The five ids are the client's
    /// own child-lookup arguments and are the same five [`super::APPEARANCE_ROWS`] carries.
    pub const HAIR_ROW: ElementId = ElementId(0x1000_03AF);
    /// The eyes row. See [`HAIR_ROW`].
    pub const EYES_ROW: ElementId = ElementId(0x1000_03B0);
    /// The nose row, hidden on the three special heritages. See [`HAIR_ROW`].
    pub const NOSE_ROW: ElementId = ElementId(0x1000_03B1);
    /// The mouth row, hidden with it. See [`HAIR_ROW`].
    pub const MOUTH_ROW: ElementId = ElementId(0x1000_03B2);
    /// The skin row, which the update also moves. See [`HAIR_ROW`].
    pub const SKIN_ROW: ElementId = ElementId(0x1000_03B3);
    /// The zoom-in step.
    pub const ZOOM_IN: ElementId = ElementId(0x1000_0325);
    /// The zoom-out step.
    pub const ZOOM_OUT: ElementId = ElementId(0x1000_0326);
    /// The viewport element (engine type `0x0D`) the preview draws into.
    pub const VIEWPORT: ElementId = ElementId(0x1000_03BB);
    /// The page's help pane.
    pub const HELP: ElementId = ElementId(0x1000_03AB);
}

/// The summary page's own viewport, which the summary preview drives.
pub const SUMMARY_VIEWPORT: ElementId = ElementId(0x1000_0406);

/// The turntable's rotation direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(missing_docs)]
pub enum ERotateDirection {
    Invalid = 0,
    Clockwise = 1,
    CounterClockwise = 2,
}

/// One part's choice state.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct PartChoice {
    /// The current choice.
    pub current: i32,
    /// How many choices there are.
    pub num: i32,
    /// The current colour.
    pub color: i32,
    /// How many colours there are.
    pub num_colors: i32,
    /// The shade.
    pub shade: f64,
}

// =================================================================================================
// The character-generation turntable
// =================================================================================================

/// The character-generation turntable as a **model**: what the preview space is asked to show.
///
/// The client builds a real physics body and hands it
/// to the viewport's creature-preview mode — the same preview-space mechanism
/// used by the portal space, which is why this is a description and not a renderer: **one**
/// host-side preview space serves both this and the teleport tunnel, and this crate may not
/// draw.
///
/// The host reads [`CharGenScreen::view3d`] each frame and renders `setup` at `heading`, seen from
/// `camera_position` looking at `camera_direction`, inside the viewport element's screen box.
#[derive(Debug, Clone, PartialEq)]
pub struct Cg3dView {
    /// The viewport element this frame's preview belongs in — the appearance page's
    /// [`appearance::VIEWPORT`] while that page is up, [`SUMMARY_VIEWPORT`] on the summary page,
    /// and `None` on the four pages that have no preview.
    pub viewport: Option<ElementId>,
    /// The setup id, falling back to `HUMAN_SETUP_ID` when the state has none. The client's
    /// initialisation does exactly that test.
    pub setup: dereth_primitives::DataId,
    /// The heading, degrees. → set_heading.
    ///
    /// **180, not 0.** Both pages that own a turntable set it during post-init --
    /// the appearance page stores 180 degrees and then applies it to the model.
    /// A heading of 0 faces
    /// **+Y**, which is the direction the camera looks *along*, so with 0 the model turns its back.
    ///
    pub heading: f32,
    /// The camera position — the camera write's first argument.
    pub camera_position: [f32; 3],
    /// The camera direction.
    pub camera_direction: [f32; 3],
    /// The running and rest animations, as the **enums** the initialisation resolves; the
    /// enum-to-data-id lookup is the host's, so the enum is what crosses the seam.
    pub animation_enum: u32,
    /// See [`Self::animation_enum`].
    pub rest_animation_enum: u32,
    /// The five-entry animation array.
    pub anim_array_enums: [u32; 5],
    /// True between the animation start and stop. The running animation plays at **30 fps**
    /// (`set_sequence_animation(did, 1, 0, 30.0)`); the rest one at **0**, i.e. held on its first
    /// frame.
    pub animating: bool,
    /// Whether the turntable is turning.
    pub rotating: bool,
    /// Which way it turns.
    pub rotate_dir: ERotateDirection,
    /// Whether the camera is zoomed in.
    pub zoomed_in: bool,
    /// The **background** object's setup id, taken from the heritage group's
    /// environment setup.
    ///
    /// The preview keeps a *second* physics body in the same creature-preview space: when the
    /// heritage's environment setup differs from the current one the old background object is
    /// deleted, the new setup is stored, and when it is valid a new object is made from it and
    /// added to the viewport.
    ///
    /// It is the room the recorded retail frame shows behind the head; without it the model stands
    /// in a void. `DataId(0)` is `INVALID_DID`, i.e. no background object at all.
    pub bg_setup: DataId,
}

/// The client's human setup id, which is also the reset's literal setup id.
pub const HUMAN_SETUP_ID: dereth_primitives::DataId = dereth_primitives::DataId(0x0200_0054);

/// The animation enums the preview uses for an ordinary heritage, then the Olthoi (`0x0C`) and
/// OlthoiAcid (`0x0D`) overrides. The pairs are `(running, rest)` followed by the five
/// animation-array entries.
pub const ANIM_ENUMS_DEFAULT: (u32, u32, [u32; 5]) = (
    0x1000_0006,
    0x1000_0005,
    [
        0x1000_0007,
        0x1000_0008,
        0x1000_0009,
        0x1000_000A,
        0x1000_000B,
    ],
);
/// Olthoi: one animation enum for both, and the **same** enum five times in the array.
pub const ANIM_ENUMS_OLTHOI: (u32, u32, [u32; 5]) = (0x1000_0011, 0x1000_0011, [0x1000_0012; 5]);
/// OlthoiAcid, the same shape.
pub const ANIM_ENUMS_OLTHOI_ACID: (u32, u32, [u32; 5]) =
    (0x1000_0013, 0x1000_0013, [0x1000_0014; 5]);

/// The client's target camera, per heritage.
///
/// The same three positions the summary preview uses, except that the summary's Olthoi case is `(0,
/// 0, 0)` where zoom-out's is `(0, -3.8, 1.15)`.
#[must_use]
pub fn zoomed_out_camera(heritage: u32) -> [f32; 3] {
    dereth_presentation::creation::camera(
        heritage,
        false,
        dereth_presentation::DisplayVariant::Modern,
    )
}

/// The appearance page's zoom-in step's target camera.
///
/// Tumerok (**7**) is the one ordinary heritage with a case of its own — a taller head, so the
/// camera backs off from -0.55 to -0.85. The height constant is **1.65** as a `float`, which the
/// OlthoiAcid zoom-out value confirms independently.
#[must_use]
pub fn zoomed_in_camera(heritage: u32) -> [f32; 3] {
    dereth_presentation::creation::camera(
        heritage,
        true,
        dereth_presentation::DisplayVariant::Modern,
    )
}

/// The rotation period — the wizard's constructor writes the double **3.0**, so a full turn of the
/// turntable takes three seconds.
pub const ROTATION_PER_SEC: f64 = 3.0;

/// The heading as both owning pages' post-init writes it: the appearance page and the summary page
/// each set **180** degrees and push it through the player-heading write. See
/// [`Cg3dView::heading`].
pub const INITIAL_HEADING: f32 = 180.0;

impl Default for Cg3dView {
    fn default() -> Self {
        Self {
            viewport: None,
            setup: HUMAN_SETUP_ID,
            heading: INITIAL_HEADING,
            camera_position: [0.0, -2.5, 0.95],
            camera_direction: [0.0, 0.0, 0.0],
            animation_enum: ANIM_ENUMS_DEFAULT.0,
            rest_animation_enum: ANIM_ENUMS_DEFAULT.1,
            anim_array_enums: ANIM_ENUMS_DEFAULT.2,
            animating: false,
            rotating: false,
            rotate_dir: ERotateDirection::Invalid,
            zoomed_in: false,
            bg_setup: dereth_primitives::DataId(0),
        }
    }
}

impl Cg3dView {
    /// Adopt the state's setup and the heritage's animations.
    pub fn initialize(&mut self, state: &CharGenState) {
        self.setup = if state.setup_id.0 == 0 {
            HUMAN_SETUP_ID
        } else {
            state.setup_id
        };
        let (a, r, arr) = match state.heritage_group {
            HERITAGE_OLTHOI => ANIM_ENUMS_OLTHOI,
            HERITAGE_OLTHOI_ACID => ANIM_ENUMS_OLTHOI_ACID,
            _ => ANIM_ENUMS_DEFAULT,
        };
        self.animation_enum = a;
        self.rest_animation_enum = r;
        self.anim_array_enums = arr;
    }

    /// The appearance page's rotation step, evaluated for a `dt` in seconds.
    ///
    /// `delta = dt / period * 360`, added for clockwise and subtracted otherwise; then 360 is added
    /// once if the heading is below 0 and subtracted once if it is above 360.
    ///
    /// The two wraps are the client's own, and they are **not** a modulo: a `dt` longer than the
    /// three-second period leaves the heading outside `0..360`, which is reproduced.
    pub fn do_rotation(&mut self, dt: f64) {
        if !self.rotating {
            return;
        }
        #[allow(clippy::cast_possible_truncation)] // the client narrows to f32 on the same line
        let delta = (dt / ROTATION_PER_SEC * 360.0) as f32;
        self.heading += if self.rotate_dir == ERotateDirection::Clockwise {
            delta
        } else {
            -delta
        };
        if self.heading < 0.0 {
            self.heading += 360.0;
        }
        if self.heading > 360.0 {
            self.heading -= 360.0;
        }
    }

    /// Pressing the button that is already spinning
    /// **stops** it; pressing the other one reverses and un-presses its opposite.
    pub fn rotate(&mut self, dir: ERotateDirection) {
        if self.rotating && dir == self.rotate_dir {
            self.rotating = false;
            return;
        }
        self.rotate_dir = dir;
        self.rotating = true;
    }
}

/// The two dat tables the wizard reads. The host loads them and hands them over, because this
/// crate has no data cache and the concrete DataIDs must not be hard-coded: they come from
/// `DidMapper 0x25000002` (`UNIQUEDB`) entries **0x0E** `CharGen_CharacterData` and **4**
/// `Weenie_SkillTable`.
#[derive(Debug)]
pub struct CharGenTables {
    /// The world rules and appearance resources shared by both creation interfaces.
    pub world: Rc<dereth_chargen::CreationTables>,
    /// The four `UIASSET` images the colour wheel is generated from.
    pub color_wheel_art: ColorWheelArt,
    /// The colour source across the host seam, or `None` where a caller has no dat behind it -- in
    /// which case the nine spots all draw `ColorEmpty`, which is what the client shows when the
    /// colour lookup finds nothing.
    pub colors: Option<std::rc::Rc<dyn CgColorSource>>,
    /// The creation texts the world's own files carry (a world from before Throne of Destiny):
    /// where the table names one, the page shows it in place of this interface's own string.
    pub texts: dereth_chargen::CreationTexts,
}

impl std::ops::Deref for CharGenTables {
    type Target = dereth_chargen::CreationTables;
    fn deref(&self) -> &Self::Target {
        &self.world
    }
}

/// What the wizard asks the host to do. Handed over as
/// [`UiRequest::CharGenAction`](crate::view::UiRequest::CharGenAction)s at the end of the
/// wizard's update; see `charmgmt::CharacterAction`. The type lives in `dereth_client_contract::pregame`.
pub use dereth_client_contract::pregame::CharGenAction;

/// The character-generation screen — mode `0x1000000B`.
#[derive(Debug)]
pub struct CharGenScreen {
    roots: Vec<ElemHandle>,
    bound: Bound,
    /// The progress state. The constructor ends by setting it to the heritage page.
    pub progress: EcgProgress,
    /// Which page element is visible; the progress-state write "shows one page, hides the rest".
    pub visible_page: Option<ElementId>,
    /// Whether the arrow buttons are enabled, which the progress-state write also updates.
    pub left_enabled: bool,
    /// See [`Self::left_enabled`].
    pub right_enabled: bool,
    /// The account flag character randomisation is handed and the expansion-warning dialog build
    /// tests.
    pub account_has_tod: bool,
    pub open_dialog: Option<CharGenDialog>,
    /// The exit-warning context.
    ///
    /// In the client this is the context the dialog factory returns, and the exit step first checks
    /// that it is 0: a second click while the confirmation is up builds nothing. Here it is the
    /// dialog *element* the wizard created, because that element is what has to be taken down again
    /// and what carries the answer.
    pub exit_dialog: Option<ElemHandle>,
    /// The randomize-warning context as a live element.
    pub randomize_warning_dialog: Option<ElemHandle>,
    /// The credit-warning context as a live element.
    pub credit_warning_dialog: Option<ElemHandle>,
    /// The error-message context as a live element.
    pub error_message_dialog: Option<ElemHandle>,
    /// The expansion-warning context as a live element.
    pub tod_warning_dialog: Option<ElemHandle>,
    /// The **factory** contexts behind those five elements. See [`DialogQueueSlots`].
    dialog_queue: DialogQueueSlots,
    /// The close-dialog notice was answered on a call with no `UiSystem` in reach,
    /// so the element and the factory's queue slot are freed on the next frame that has one --
    /// the same one-frame latch [`Self::pending_random`] is.
    ///
    /// **Not a heuristic.** Taking down every context that is not [`Self::open_dialog`] is
    /// indistinguishable from a *second* dialog having been recorded while the first is still up --
    /// and with a queue that happens: the second dialog's creation overwrites `open_dialog`, and
    /// such a sweep would delete the dialog the player is looking at.
    pending_close: Option<CharGenDialog>,
    /// The character-generation state — the one object all six pages are views over.
    pub state: CharGenState,
    /// The two dat tables, supplied by the host. Nothing can be chosen without them.
    pub tables: Option<Rc<CharGenTables>>,
    /// Whether a name has been entered.
    pub name_entered: bool,
    /// Whether the screen is waiting for the character set in order to log the new character in.
    pub awaiting_char_set_for_login: bool,
    /// The error text the dialog was given, as a string **id** in table enum `0x10000002`; the host
    /// resolves it.
    pub error_string_id: Option<&'static str>,
    /// What the host owes the session. See [`CharGenAction`].
    pub actions: Vec<CharGenAction>,
    /// The current profession button — the button currently in state `0x10000017`.
    pub current_profession_button: Option<ElementId>,
    /// The token last put in the profession text box.
    pub profession_text: Option<&'static str>,
    /// The skill records, in name order. **Not** the order the list draws in — [`Self::skill_list`]
    /// is, because the skill-entry update files each row under its own header.
    pub skill_rows: Vec<SkillRecord>,
    /// The specialized, trained, useable-untrained and unuseable-untrained headers: the four
    /// category rows the skill-record build makes from template 0, in that order. They are the
    /// *bounds* the sorted insert inserts between.
    pub skill_headers: Vec<ElemHandle>,
    /// The skills list as a live list-box widget, which is the only
    /// thing that can express this list: the list is one flat array holding headers and rows
    /// interleaved, and measures each row's own height.
    pub skill_list: Option<ListBoxWidget>,
    /// Every row placed in the summary list box, so that its own flush can take them down again.
    pub summary_rows: Vec<ElemHandle>,
    /// The nine parts' choice state.
    pub choices: [PartChoice; 9],
    /// The current part.
    pub current_part: EParts,
    /// The current selection — the part row currently latched lit, so that the selection write's
    /// state 1 on the **previous** one has something to name.
    pub current_selection_row: Option<ElementId>,
    /// The current colour.
    pub current_color: i32,
    /// Whether the Face tab (rather than Clothes) is up.
    pub face_tab: bool,
    /// The held headgear -- where the Face arm parks the headgear style while the **Face** tab is
    /// up, having set the live one to -1 so the head is bare.
    ///
    /// The Clothes arm restores it, guarded by the client's own `-2 < hold < headgear choice
    /// count`, which lets -1 ("no hat") through.
    pub hold_headgear: i32,
    /// The last heritage group -- the edge detector on the client's heritage-6 arm, which re-rolls
    /// the appearance and the clothing the first time a **Gear Knight** is shown and never again.
    pub last_heritage_group: u32,
    /// A *Random* answered on the summary page, waiting for a frame with a `UiSystem` in it.
    ///
    /// The randomize-warning close clears its stored context before it acts on the answer, then
    /// runs randomization on yes. [`Self::close_dialog`] -- called without a `UiSystem` -- cannot
    /// repaint a page. So the answer is latched here and [`Screen::update`] runs
    /// [`Self::do_random`] on the next frame, the same place the client's per-frame tick would
    /// have.
    pub pending_random: bool,
    /// The colour wheel's red/green/blue -- the nine spot colours, packed ARGB, for the part that
    /// is currently selected. `None` is a slot the part has no colour for **or** one whose palette
    /// would not resolve; both draw `ColorEmpty`.
    pub color_wheel: [Option<u32>; 9],
    /// The page has not yet been repainted since the character was rolled.
    ///
    /// In the client this cannot happen: screen construction runs character randomization
    /// **before** it builds a page, and its own move to the heritage page is the last line. Here
    /// the tables arrive a frame after `Screen::create`, so the first `heritage_page_update` saw
    /// heritage 0 and lit nothing; the repaint is latched and run from [`Screen::update`], which
    /// has a `UiSystem`.
    pub pending_refresh: bool,
    /// The client's one *state* write, latched for the same
    /// reason [`Self::pending_refresh`] is. See [`Self::initialize_appearance_page`].
    pub pending_initialize_appearance: bool,
    /// The turntable's model. See [`Cg3dView`].
    pub view3d: Cg3dView,
}

impl Default for CharGenScreen {
    fn default() -> Self {
        Self {
            roots: Vec::new(),
            bound: Bound::default(),
            progress: EcgProgress::Invalid,
            visible_page: None,
            left_enabled: false,
            right_enabled: false,
            account_has_tod: false,
            open_dialog: None,
            exit_dialog: None,
            randomize_warning_dialog: None,
            credit_warning_dialog: None,
            error_message_dialog: None,
            tod_warning_dialog: None,
            dialog_queue: DialogQueueSlots::default(),
            pending_close: None,
            state: CharGenState::default(),
            tables: None,
            name_entered: false,
            awaiting_char_set_for_login: false,
            error_string_id: None,
            actions: Vec::new(),
            current_profession_button: None,
            profession_text: None,
            skill_rows: Vec::new(),
            skill_headers: Vec::new(),
            skill_list: None,
            summary_rows: Vec::new(),
            choices: [PartChoice::default(); 9],
            current_part: EParts::Hair,
            current_selection_row: None,
            current_color: -1,
            face_tab: true,
            hold_headgear: -1,
            last_heritage_group: 0,
            pending_random: false,
            color_wheel: [None; 9],
            pending_refresh: false,
            pending_initialize_appearance: false,
            view3d: Cg3dView::default(),
        }
    }
}

impl CharGenScreen {
    /// The factory the screen registration is given.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self::default())
    }

    #[must_use]
    pub fn bound(&self) -> &Bound {
        &self.bound
    }

    /// The progress-state write: "Shows one page, hides the rest, updates the progress bar and the
    /// enabled state of the arrow buttons".
    pub fn set_progress_state(&mut self, ui: &mut UiSystem, state: EcgProgress) {
        let state = self.snap_for_olthoi(state);
        self.progress = state;
        self.visible_page = state.page().map(|(id, _)| id);
        let i = state.index();
        self.left_enabled = i.is_some_and(|i| i > 0);
        self.right_enabled = i.is_some_and(|i| i + 1 < EcgProgress::PAGES.len());
        if let Some(root) = self.roots.first().copied() {
            for p in EcgProgress::PAGES {
                let Some((id, _)) = p.page() else { continue };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, Some(id) == self.visible_page);
                }
            }
            // Step 2's other half: the three tabs an Olthoi character cannot reach.
            //
            // **Visibility, not the `Disabled` attribute**, as retail does it.
            // The client calls the *same* visibility operation on these three buttons that it calls
            // on the six page elements and on the right-arrow / Finish buttons, with 1 for the
            // eleven non-Olthoi heritages and 0 for the two Olthoi ones. Setting `attribute 0x0D`
            // instead would be a different picture (a greyed tab rather than no tab) and a
            // different hit test (a disabled element is still under the mouse).
            let olthoi = self.state.heritage_group == HERITAGE_OLTHOI
                || self.state.heritage_group == HERITAGE_OLTHOI_ACID;
            for p in [
                EcgProgress::Profession,
                EcgProgress::Skills,
                EcgProgress::Town,
            ] {
                let Some(id) = p.select_button() else {
                    continue;
                };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, !olthoi);
                }
            }
            // Step 1's tail, without which **no tab is ever highlighted** and **the FINISH button
            // never appears**:
            //
            // all six tabs go to state 1, the right arrow is shown and Finish hidden; then the
            // current tab goes to state 6, and on the summary page the right arrow is hidden and
            // Finish shown.
            for p in EcgProgress::PAGES {
                let Some(id) = p.select_button() else {
                    continue;
                };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_state(
                        h,
                        if p == state {
                            STATE_TAB_ON
                        } else {
                            STATE_TAB_OFF
                        },
                    );
                }
            }
            let summary = state == EcgProgress::Summary;
            for (id, visible) in [(RIGHT_BUTTON, !summary), (FINISH_BUTTON, summary)] {
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, visible);
                }
            }
        }
        self.world_page_help(ui);
        // Step 3: "show the selected page element, highlight its tab, and call that page's update".
        match state {
            EcgProgress::Hertage => self.heritage_page_update(ui),
            EcgProgress::Profession => self.profession_page_update(ui),
            EcgProgress::Skills => self.do_skill_records(ui),
            EcgProgress::Appearance => self.appearance_update(ui),
            EcgProgress::Town => self.town_page_update(ui),
            EcgProgress::Summary => self.summary_page_update(ui),
            EcgProgress::Invalid => self.refresh_view(ui),
        }
    }

    /// The progress-state write's step 2 — **the one place the wizard's graph is not linear**.
    ///
    /// For the two Olthoi heritages the Profession, Skills and Town tabs are disabled and the
    /// requested state is snapped, with the *direction* deciding where it lands: going backwards,
    /// Profession or Skills becomes Heritage and Town becomes Appearance; going forwards,
    /// Profession or Skills becomes Appearance and Town becomes Summary. So the Olthoi wizard is
    /// Heritage → Appearance → Summary in both directions.
    #[must_use]
    pub fn snap_for_olthoi(&self, want: EcgProgress) -> EcgProgress {
        let h = self.state.heritage_group;
        if h != HERITAGE_OLTHOI && h != HERITAGE_OLTHOI_ACID {
            return want;
        }
        let backwards = (want as i32) < (self.progress as i32);
        match (want, backwards) {
            (EcgProgress::Profession | EcgProgress::Skills, true) => EcgProgress::Hertage,
            (EcgProgress::Profession | EcgProgress::Skills, false) => EcgProgress::Appearance,
            (EcgProgress::Town, true) => EcgProgress::Appearance,
            (EcgProgress::Town, false) => EcgProgress::Summary,
            _ => want,
        }
    }

    /// The left arrow — the client's `0x100003C6` arm: past the heritage page it goes back one
    /// page, and on the heritage page it runs the exit step.
    ///
    /// **On the heritage page the left arrow is Exit.** It shares the exit step with the *Exit*
    /// button itself, which is why the wizard's first page still has a way out of it.
    pub fn previous_page(&mut self, ui: &mut UiSystem) {
        match self.progress.index() {
            Some(i) if i > 0 => {
                if let Some(p) = EcgProgress::from_index(i - 1) {
                    self.set_progress_state(ui, p);
                }
            }
            _ => {
                self.do_exit(ui);
            }
        }
    }

    /// The right arrow.
    pub fn next_page(&mut self, ui: &mut UiSystem) {
        if let Some(i) = self.progress.index() {
            if let Some(p) = EcgProgress::from_index(i + 1) {
                self.set_progress_state(ui, p);
            }
        }
    }

    /// The host hands over the two dat tables here. This is also when the wizard's constructor and
    /// its subsequent initialization can run.
    ///
    /// The screen is created before the tables exist — the mode change builds it from the layout
    /// and the host pushes state on the *next* frame — so the reset cannot run in
    /// [`Screen::create`] and runs here instead, once.
    ///
    /// **The second half of construction runs here too.** The screen binds the Random button, reads
    /// the account's expansion entitlement,
    /// and then randomizes the character **before** it builds a single page — so
    /// every page is a view over an already-rolled character. Without it the wizard opens on
    /// thirteen dark bullets, a black Town pane and a disabled Appearance page.
    ///
    /// [`Self::account_has_tod`] must already be set when this runs, which is why the host writes
    /// it before handing the tables over.
    pub fn set_tables(&mut self, t: Rc<CharGenTables>) {
        self.state.clothing = t.clothing.clone();
        self.state
            .randomize_character(&t.chargen, &t.skills, self.account_has_tod);
        self.tables = Some(t);
        self.setup_parts();
        self.pending_refresh = true;
        // Screen construction builds every page immediately after character
        // randomization, and the appearance page is the one that writes [`CharGenState`]. It needs
        // a `UiSystem` for the view half of choice selection, so it is latched here and run from
        // `Screen::update`, exactly as `pending_refresh` is — and **before** it, because the
        // constructor's move to the heritage page is its last line.
        self.pending_initialize_appearance = true;
    }

    /// The client's tail — the part that is not binding
    /// children.
    ///
    /// The function is mostly recursive child lookup into members this build resolves by id at
    /// use, plus three things that are state:
    ///
    /// | the page initialisation does | here |
    /// |---|---|
    /// | store heading 180 and apply it | already [`INITIAL_HEADING`] |
    /// | greys the **Skin** row's two arrows (`0x1000030A`/`0x1000030B`, attribute `0xD`) | done elsewhere |
    /// | clear zoom and choose the Face tab | **this** |
    /// | the gender statement at the end | **deliberately not**, see below |
    /// | when visible, update the view; then always set up parts and selection | the current update performs all three, without reproducing that visibility guard |
    ///
    /// # Why choosing Face at construction matters
    ///
    /// The client's Face arm parks the headgear style in the held headgear and stores **-1**; the
    /// update then skips the headgear object-description build entirely when the headgear style is
    /// -1, so no hat part or hooded head variant enters the description. Hair is added
    /// independently from the hair style, so what is left is an uncovered head with its own hair —
    /// which is exactly what retail shows on arrival.
    ///
    /// Without this only a **click** on the Face tab would run the model half
    /// ([`Self::apply_choice_view`] runs the view half from every update), so the page the player
    /// arrives on would be dressed, hood and all.
    ///
    /// **This is wire-visible and that is retail.** The headgear style stays -1 in the state that
    /// reaches the finish request unless the player clicks *Clothes*, whose arm restores held
    /// headgear. So a character created without opening the Clothes tab is bare-headed, and the ACE
    /// server reads `-1` as the unsigned maximum, meaning "no headgear". The evidence is the
    /// unconditional -1 store in the Face arm, and the absence of any other writer of the headgear
    /// style between construction and the finish.
    ///
    /// # The gender statement at the end — settled, and left unimplemented on purpose
    ///
    /// The page initialisation really does end with a **sex flip**: a state gender of 1 is set to
    /// 2 and a gender of 2 is set to 1, each followed by a page update; any other value is left
    /// alone and that update is skipped.
    ///
    /// **It is a one-argument gender write on the state.** The contrast that made the doubt
    /// reasonable is the client's gender arm, which has the same `1`/`2` shape but writes **two
    /// different button objects** with the state ids `0x10000016`/`0x10000017`. Here there is no
    /// second object.
    ///
    /// What it is: the page's own gender enum is `{invalid 0, female 1, male 2}` while the state's
    /// gender is the **inverse** (the female button `0x100003A7` sets gender 2 and the male
    /// `0x100003A8` sets gender 1, in the element-message handler). So this is a page-enum ⇄
    /// state-gender conversion written back into the *state* instead of into the page's own field —
    /// **a shipped bug**, reachable on every wizard open because character randomisation has
    /// already rolled the gender to 1 or 2 by then.
    ///
    /// **It is not implemented here, and the reason is measurable rather than squeamish.** The
    /// value it flips is a dice roll over `1..=2`, so flipping it leaves the distribution of the
    /// arrival state identical; the only observable is that the appearance indices were rolled
    /// against one sex's lists and then clamped by the gender constraint to the other's. That is a
    /// real fidelity difference with **no player-visible consequence**, and reproducing it moves
    /// every seeded-randomiser test oracle, which would then have to be corrected rather than
    /// relaxed. The description above is kept so that whoever reproduces it does not re-derive it.
    fn initialize_appearance_page(&mut self, ui: &mut UiSystem) {
        // Clear the zoom flag, then choose the Face tab. `set_choice`'s view half ends in `zoom(ui,
        // true)`, whose own guard returns when the zoom state already matches, so the order is the
        // client's and the zoom is not run twice.
        self.view3d.zoomed_in = false;
        self.set_choice(ui, true);
    }

    /// Take everything the host owes the session, oldest first.
    pub fn take_actions(&mut self) -> Vec<CharGenAction> {
        std::mem::take(&mut self.actions)
    }

    /// The heritage page's element-message handler's tail: the heritage write, then the page
    /// update.
    ///
    /// A heritage absent from the world, or Viamontian without expansion access,
    /// raises the expansion warning and leaves the current character unchanged.
    pub fn choose_heritage(&mut self, heritage: u32) {
        if heritage == TOD_HERITAGE && !self.account_has_tod {
            self.open_dialog = Some(CharGenDialog::ToDRequired);
            return;
        }
        let Some(t) = self.tables.clone() else { return };
        if !t.chargen.heritage_groups.contains_key(&heritage) {
            self.open_dialog = Some(CharGenDialog::ToDRequired);
            return;
        }
        self.state.clothing = t.clothing.clone();
        self.state
            .set_heritage_group(&t.chargen, &t.skills, heritage);
        self.after_heritage_change(&t);
    }

    /// The tail both heritage entry points share — the bullet click and *Random*'s
    /// random-heritage arm — so that the two cannot drift apart.
    ///
    /// None of this is in the heritage write; it is the shape this build needs around it, and two
    /// of the three lines are **belt and braces**: character randomization has already run in
    /// [`Self::set_tables`], so these are reached only when a heritage's own tables are narrower
    /// than the last one's.
    fn after_heritage_change(&mut self, t: &Rc<CharGenTables>) {
        // Gender 1. The heritage write sets no gender and the gender constraint drives every
        // appearance index to -1 while the gender is 0, so a heritage change still has to leave a
        // *usable* gender behind if one was somehow lost. ACE reads gender 1 as "Male"
        // (`PlayerFactory.Create`), which is a naming disagreement with the client's own gender
        // enum and not a wire disagreement.
        if self.state.gender == 0 {
            let g = t
                .chargen
                .heritage_groups
                .get(&self.state.heritage_group)
                .and_then(|h| h.sexes.keys().copied().next())
                .unwrap_or(1);
            self.state.set_gender(&t.chargen, g);
        }
        // The heritage constraint only resets the skill levels when the clamped budget has gone
        // negative, and the heritage write's own template application resets and re-trains the
        // skills whenever a template is selected. **So this runs only when there is no template**,
        // which the constraint leaves only for a heritage whose list the current index has fallen
        // off. Running it unconditionally would wipe the profession the roll had just applied --
        // applying the template trains its skills and this would undo them.
        if self.state.template < 0 {
            self.state.reset_skill_levels(&t.chargen, &t.skills);
        }
        // And the same for the appearance: the gender constraint clamps rather than re-rolls, and a
        // -1 that survives it is what the appearance randomiser would have replaced.
        self.state.default_appearance(&t.chargen);
        self.setup_parts();
    }

    /// **The *Random* button, all six of its arms.**
    ///
    /// Per page: Heritage randomises the heritage and updates the page; Profession randomises the
    /// template and resets the page to the template's attributes; Skills randomises the skills and
    /// updates; Appearance randomises the face (Face tab) or the clothing (Clothes tab) and
    /// updates; Town sets the start area to a random int over 3 (4 with Throne of Destiny) and
    /// updates; Summary randomises the whole character and updates.
    ///
    /// **The Town arm does not use the heritage's start-area list.** It is a flat draw over 3 — or
    /// 4 with a Throne of Destiny — straight into the start area, so *Random* on the Town page can
    /// hand an Aluvian a Yaraq start where its heritage page would never choose. That asymmetry is
    /// the client's.
    ///
    /// **The Summary arm is reached only through the warning dialog.** The button's own handler
    /// raises a warning on that page, and
    /// the client calls this routine with the answer; see
    /// [`Self::pending_random`].
    pub fn do_random(&mut self, ui: &mut UiSystem) {
        let Some(t) = self.tables.clone() else { return };
        let tod = self.account_has_tod;
        match self.progress {
            EcgProgress::Hertage => {
                self.state
                    .randomize_heritage_group(&t.chargen, &t.skills, tod);
                self.after_heritage_change(&t);
                self.heritage_page_update(ui);
            }
            EcgProgress::Profession => {
                self.state.randomize_template(&t.chargen, &t.skills);
                self.update_to_default_attributes(ui);
            }
            EcgProgress::Skills => {
                self.state.randomize_skills(&t.chargen, &t.skills);
                self.do_skill_records(ui);
            }
            EcgProgress::Appearance => {
                // Anything but the Clothes tab — the Face tab and the two heritages that have no
                // Clothes tab at all both take the appearance arm.
                if self.face_tab {
                    self.state.randomize_appearance(&t.chargen, false);
                } else {
                    self.state.randomize_clothing(&t.chargen, true);
                }
                self.appearance_update(ui);
            }
            EcgProgress::Town => {
                let n = if tod { 4 } else { 3 };
                let a = self.state.rng.rand_int(n);
                self.state.set_start_area(u32::try_from(a).unwrap_or(0));
                self.town_page_update(ui);
            }
            EcgProgress::Summary => {
                self.state.randomize_character(&t.chargen, &t.skills, tod);
                self.after_heritage_change(&t);
                self.summary_page_update(ui);
            }
            EcgProgress::Invalid => {}
        }
    }

    /// **The page's whole view**: without it no bullet ever fills and the description pane stays
    /// black.
    ///
    /// All thirteen buttons go to state `0x10000016`; the text is set to the starting-skills header
    /// (colour 1), then the starting skills, the bonus-skills header (colour 1) and the heritage's
    /// bonus skills are appended; the heritage's button goes to `0x10000017` and the background to
    /// the heritage's state; then a newline and the heritage's description are appended.
    ///
    /// The two header runs take **colour index 1** and the bodies index 0, which is why retail's
    /// pane has two green headings; see [`Self::append_run`].
    pub fn heritage_page_update(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (id, _) in HERITAGE_BUTTONS {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_state(h, STATE_PROFESSION_OFF);
            }
        }
        let Some(text) = ui.get_child_recursive(root, heritage_page::TEXT) else {
            return;
        };
        let row = HERITAGE_PAGE
            .iter()
            .find(|(h, _, _, _)| *h == self.state.heritage_group);
        if let Some(own) = self.world_heritage_text() {
            // The world's own description is the whole pane: its era's heritages had no starting
            // or bonus skills of their own to list.
            if let Some((id, _)) = HERITAGE_BUTTONS
                .iter()
                .find(|(_, h)| *h == self.state.heritage_group)
            {
                if let Some(h) = ui.get_child_recursive(root, *id) {
                    ui.set_state(h, STATE_PROFESSION_ON);
                }
            }
            if let Some((_, background, _, _)) = row.copied() {
                if let Some(h) = ui.get_child_recursive(root, heritage_page::BACKGROUND) {
                    ui.set_state(h, background);
                }
            }
            self.append_literal(ui, text, &own, 0, true);
            return;
        }
        self.append_run(ui, text, HERITAGE_SKILLS_HEADER, 1, true);
        self.append_run(ui, text, HERITAGE_SKILLS_BODY, 0, false);
        self.append_run(ui, text, HERITAGE_BONUS_HEADER, 1, false);
        if let Some((_, _, bonus, _)) = row {
            self.append_run(ui, text, bonus, 0, false);
        }
        if let Some((heritage, background, _, desc)) = row.copied() {
            if let Some((id, _)) = HERITAGE_BUTTONS.iter().find(|(_, h)| *h == heritage) {
                if let Some(h) = ui.get_child_recursive(root, *id) {
                    ui.set_state(h, STATE_PROFESSION_ON);
                }
            }
            if let Some(h) = ui.get_child_recursive(root, heritage_page::BACKGROUND) {
                ui.set_state(h, background);
            }
            if let Some(t) = ui.text_element_mut(text) {
                t.append_text("\n");
            }
            self.append_run(ui, text, desc, 0, false);
        }
    }

    /// The town page's town write → the char-gen state's start area write. The **model** half only;
    /// [`Self::town_page_update`] is the view half, and the element-message handler runs them in
    /// that order the way the client's town write does. They are separate because the refusal arm
    /// repaints too: the client's Sanamar branch re-writes the town from the *existing* area, so a
    /// refused click leaves the map consistent rather than untouched.
    pub fn choose_town(&mut self, start_area: u32) {
        let Some(tables) = &self.tables else { return };
        let Some(town) = usize::try_from(start_area)
            .ok()
            .and_then(|i| tables.chargen.starter_areas.get(i))
        else {
            return;
        };
        if town.name.eq_ignore_ascii_case("Sanamar") && !self.account_has_tod {
            self.open_dialog = Some(CharGenDialog::ToDRequired);
            return;
        }
        self.state.set_start_area(start_area);
    }

    fn town_map_index(name: &str) -> Option<usize> {
        let name = name.split_whitespace().next()?;
        TOWN_NAMES
            .iter()
            .position(|town| town.eq_ignore_ascii_case(name))
    }

    fn uses_world_town_list(&self) -> bool {
        self.tables.as_ref().is_some_and(|tables| {
            tables.chargen.starter_areas.len() < TOWN_NAMES.len()
                || tables
                    .chargen
                    .starter_areas
                    .iter()
                    .zip(TOWN_NAMES)
                    .any(|(area, name)| !area.name.eq_ignore_ascii_case(name))
        })
    }

    fn choose_map_town(&mut self, map_index: u32) {
        let area = self.tables.as_ref().and_then(|tables| {
            tables.chargen.starter_areas.iter().position(|area| {
                Self::town_map_index(&area.name) == usize::try_from(map_index).ok()
            })
        });
        if let Some(area) = area.and_then(|i| u32::try_from(i).ok()) {
            self.choose_town(area);
        }
    }

    /// Apply the selected starting town to the view and its text label.
    ///
    /// All four map pins go to state 1 and the chosen one to 6; the page takes its title state for
    /// the start area; and the text box gets the town text combined with `ID_CharGen_TownHowTo`.
    ///
    /// The town-string write builds the pane by substituting the town's own paragraph **into**
    /// `ID_CharGen_TownHowTo`, not by concatenating them — it takes the how-to as the format and
    /// the town text as the one argument, which is why retail's pane reads "Holtburg is a
    /// picturesque…" and *then* "Choose a starting town by clicking…".
    pub fn town_page_update(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        if self.uses_world_town_list() {
            self.update_world_towns(ui, root);
            return;
        }
        for (id, _) in TOWN_BUTTONS {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_state(h, town_page::PIN_OFF);
            }
        }
        let area = self.state.start_area;
        if let Some((id, _)) = TOWN_BUTTONS.iter().find(|(_, a)| *a == area) {
            if let Some(h) = ui.get_child_recursive(root, *id) {
                ui.set_state(h, town_page::PIN_ON);
            }
        }
        let i = usize::try_from(area).unwrap_or(0);
        if let Some(s) = town_page::TITLE_STATES.get(i).copied() {
            // The client sets the state on the page element; the title `0x10000408` carries the
            // same four states, and the page passes them down.
            for id in [
                EcgProgress::Town.page().map(|(id, _)| id),
                Some(town_page::TITLE),
            ]
            .into_iter()
            .flatten()
            {
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_state(h, s);
                }
            }
        }
        let Some(text) = ui.get_child_recursive(root, town_page::TEXT) else {
            return;
        };
        let Some(token) = town_page::TEXT_TOKENS.get(i).copied() else {
            return;
        };
        let body = self.string(ui, token);
        let frame = self.string(ui, town_page::HOW_TO);
        // The town-string format literal is `L"\n\n%s\n"`, and the town's own paragraph is what it
        // is appended *to*: the pane reads "Holtburg is a picturesque…", a blank line, then "Choose
        // a starting town by clicking…", which is the paired retail frame; the literal plus the
        // frame settle the order.
        let s = format!("{body}\n\n{frame}\n");
        if let Some(t) = ui.text_element_mut(text) {
            t.set_text(&s);
        }
    }

    /// A world whose starter areas are not the shipped map's four towns (the February 2005
    /// table's six outdoor starts, two by each of three towns): each town's pin stays as the map's
    /// name for it, its own dot giving way to a dot per start, made from the pin and set beside the
    /// town on the side the start lies. A dot chooses its start; the title names it and the pane
    /// reads as the town's own.
    fn update_world_towns(&self, ui: &mut UiSystem, root: ElemHandle) {
        let Some(tables) = &self.tables else { return };
        let areas = &tables.chargen.starter_areas;
        let chosen = usize::try_from(self.state.start_area).ok();
        let selected = chosen.and_then(|i| areas.get(i));
        let selected_town = selected.and_then(|area| Self::town_map_index(&area.name));
        let mut pins = [None; 4];
        for (id, index) in TOWN_BUTTONS {
            let Some(pin) = ui.get_child_recursive(root, id) else {
                continue;
            };
            let index = usize::try_from(index).unwrap_or(0);
            let has = areas
                .iter()
                .any(|area| Self::town_map_index(&area.name) == Some(index));
            ui.set_visible(pin, has);
            ui.set_state(
                pin,
                if selected_town == Some(index) {
                    town_page::PIN_ON
                } else {
                    town_page::PIN_OFF
                },
            );
            if let Some(dot) = ui.get_child_recursive(pin, town_page::PIN_DOT) {
                ui.set_visible(dot, false);
            }
            // The starts' dots take the clicks; the pin, a name now, would cover them.
            for part in std::iter::once(pin).chain(ui.children(pin)) {
                ui.set_mouse_visible(part, false);
            }
            if has {
                pins[index] = Some(pin);
            }
        }
        if let Some(title) = ui.get_child_recursive(root, town_page::TITLE) {
            if let Some(index) = selected_town {
                ui.set_state(title, town_page::TITLE_STATES[index]);
            }
            crate::options::keybinding::set_literal(
                ui,
                title,
                selected_town.map_or("Starting town", |index| TOWN_NAMES[index]),
            );
        }
        if let (Some(text), Some(index), Some(area)) = (
            ui.get_child_recursive(root, town_page::TEXT),
            selected_town,
            selected,
        ) {
            // The start's own name heads the town's text: the title holds only the town's.
            let body = self.string(ui, town_page::TEXT_TOKENS[index]);
            let frame = self.string(ui, town_page::HOW_TO);
            if let Some(t) = ui.text_element_mut(text) {
                t.set_text(&format!("{}\n\n{body}\n\n{frame}\n", area.name));
            }
        }
        let map = ui
            .get_child_recursive(root, town_page::MAP)
            .map(|m| ui.screen_box(m));
        let scale = map.map_or(DEFAULT_MAP_SCALE, |m| {
            f64::from(m.width()) / town_page::MAP_SPAN
        });
        for (index, area) in areas.iter().enumerate() {
            let Ok(index) = u32::try_from(index) else {
                continue;
            };
            let Some(town) = Self::town_map_index(&area.name) else {
                continue;
            };
            let (Some(pin), Some(at)) = (pins[town], area.locations.first()) else {
                continue;
            };
            let id = ElementId(WORLD_TOWN_BUTTON_BASE + index);
            let dot = ui.get_child_recursive(root, id).or_else(|| {
                let parent = ui.parent(pin)?;
                let pin_dot = ui.get_child_recursive(pin, town_page::PIN_DOT)?;
                // A copy of the pin, the button, with its dot and without its name, which stays
                // the town's.
                let mut desc = full_desc(ui, pin)?;
                desc.element_id = id;
                desc.children
                    .retain(|child, _| *child == town_page::PIN_DOT);
                let dot_box = ui.screen_box(pin_dot);
                let starts: Vec<(f64, f64)> = areas
                    .iter()
                    .filter(|a| Self::town_map_index(&a.name) == Some(town))
                    .filter_map(|a| a.locations.first())
                    .map(|p| {
                        town_page::map_coordinates(p.cell.0, p.frame.origin.x, p.frame.origin.y)
                    })
                    .collect();
                let centre = |b: dereth_ui::Box2D| {
                    (f64::from(b.x0 + b.x1) / 2.0, f64::from(b.y0 + b.y1) / 2.0)
                };
                let name = ui
                    .get_child_recursive(pin, town_page::PIN_NAME)
                    .map(|n| ui.screen_box(n))
                    .map(|b| (centre(b), (f64::from(b.width()), f64::from(b.height()))));
                let (x, y) = town_page::dot_at(
                    centre(dot_box),
                    town_page::TOWN_CENTRES[town],
                    town_page::map_coordinates(at.cell.0, at.frame.origin.x, at.frame.origin.y),
                    &starts,
                    scale,
                    f64::from(dot_box.width().max(dot_box.height())),
                    name,
                );
                let (w, h) = (dot_box.width(), dot_box.height());
                let parent_box = ui.screen_box(parent);
                let into = |d: &mut dereth_ui::ElementDesc, x: i32, y: i32| {
                    d.base.incorporation |= dereth_ui::desc::incorporation::LEGACY_ALL_GEOMETRY;
                    d.base.x = x;
                    d.base.y = y;
                    d.base.width = w;
                    d.base.height = h;
                };
                into(
                    &mut desc,
                    x - w / 2 - parent_box.x0,
                    y - h / 2 - parent_box.y0,
                );
                for child in desc.children.values_mut() {
                    into(child, 0, 0);
                }
                let layout = dereth_ui::LayoutDesc {
                    did: ui.node(pin)?.layout_did,
                    display_width: 800,
                    display_height: 600,
                    ..dereth_ui::LayoutDesc::default()
                };
                ui.register_for_element_message(
                    id,
                    dereth_ui::msg::element::id::BUTTON_CLICKED,
                    ME,
                );
                let handle = ui
                    .create_element_recursive_from_full_desc(&NoAssets, &layout, &desc)
                    .ok()??;
                ui.set_parent(handle, Some(parent));
                ui.initialize_tree(handle);
                // As on the pin, the click is the button's, not its picture's.
                for part in ui.children(handle) {
                    ui.set_mouse_visible(part, false);
                }
                Some(handle)
            });
            if let Some(dot) = dot {
                ui.set_visible(dot, true);
                ui.set_state(
                    dot,
                    if chosen == usize::try_from(index).ok() {
                        town_page::PIN_ON
                    } else {
                        town_page::PIN_OFF
                    },
                );
                ui.set_tooltip(dot, Some(format!("Start in {}.", area.name)));
            }
        }
    }

    /// The text element's "append string info with font" — set the font from property `0x1A`, the
    /// colour from property `0x1B`, and then the text.
    ///
    /// Only the colour index is honoured here: the two font arrays are the same array on every
    /// char-gen text element, and `TextElement` already carries element **0** of `0x1A`. `clear` is
    /// the "set string info with font", which is a text clear then this.
    fn append_run(
        &self,
        ui: &mut UiSystem,
        h: ElemHandle,
        token: &str,
        color_index: usize,
        clear: bool,
    ) {
        let s = self.string(ui, token);
        self.append_literal(ui, h, &s, color_index, clear);
    }

    /// [`Self::append_run`] with the text itself rather than its token.
    fn append_literal(
        &self,
        ui: &mut UiSystem,
        h: ElemHandle,
        s: &str,
        color_index: usize,
        clear: bool,
    ) {
        use dereth_assets::ui::PropertyValue;
        let color = ui.node(h).and_then(|n| {
            match n
                .merged_properties()
                .get(dereth_ui::props::attr::TEXT_FONT_COLOR)
            {
                Some(PropertyValue::Array(a)) => match a.get(color_index).map(|e| &e.value) {
                    Some(PropertyValue::Color(c)) => Some(*c),
                    _ => None,
                },
                _ => None,
            }
        });
        if let Some(t) = ui.text_element_mut(h) {
            if clear {
                t.set_text("");
            }
            if let Some(c) = color {
                t.font_color = c;
            }
            t.append_text(s);
        }
    }

    /// The summary page's element-message handler, the name box's two messages.
    ///
    /// It marks the name as entered and reads the box; an empty box does nothing; a length below
    /// `0x22` is stored as the name; anything longer puts the accepted name back in the box and
    /// shows the name-limit dialog.
    ///
    /// The over-length branch **reverts the box to the accepted name**, which is the whole of the
    /// client-side length rule.
    pub fn name_changed(&mut self, ui: &mut UiSystem, h: ElemHandle) {
        self.name_entered = true;
        let text = ui
            .text_element_mut(h)
            .map(|t| t.glyphs.inq_text(false))
            .unwrap_or_default();
        if text.is_empty() {
            return;
        }
        if !self.state.set_name(&text) {
            let accepted = self.state.name.clone();
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&accepted);
            }
            // The name limit dialog step — `ID_CharGen_NameTooLong`.
            self.open_dialog = Some(CharGenDialog::ErrorMessage);
            self.error_string_id = Some("ID_CharGen_NameTooLong");
        }
    }

    /// The finish step, `check_credits` deciding whether leftover attribute credits warn.
    ///
    /// The name is trimmed at both ends and stored. No name entered (or an empty one) raises the
    /// no-name dialog; leftover attribute credits with the check on raise the credit warning; a
    /// verification state other than undefined means it was already sent. Otherwise the state goes
    /// to pending and the char-gen result is sent.
    ///
    /// The already-sent test is the double-send guard and is why the credit warning's "yes" can
    /// safely re-enter with the check off.
    pub fn do_finish(&mut self, check_credits: bool) -> bool {
        let name = self.state.name.trim().to_string();
        self.state.name = name.clone();
        if !self.name_entered || name.is_empty() {
            self.open_dialog = Some(CharGenDialog::ErrorMessage);
            self.error_string_id = Some("ID_CharGen_NoNameWarning");
            return false;
        }
        if check_credits && self.state.remaining_atrb_credits > 0 {
            self.open_dialog = Some(CharGenDialog::CreditWarning);
            return false;
        }
        if self.state.verification != CgVerification::Undef {
            return false;
        }
        self.state.verification = CgVerification::Pending;
        self.open_dialog = Some(CharGenDialog::PleaseWait);
        self.actions.push(CharGenAction::SendCharGenResult(Box::new(
            self.state.get_char_gen_result(),
        )));
        true
    }

    /// The character-generation verification-response notice.
    ///
    /// On **Ok**: close the please-wait dialog and start waiting for the character set. On anything
    /// else: raise an error-message dialog with the code's own `StringInfo`; the wizard **stays on
    /// the summary page** so the name can be fixed — which also means clearing the verification
    /// latch so Finish can be pressed again.
    pub fn on_chargen_verification_response(&mut self, code: u32) {
        let v = CgVerification::from_code(code);
        self.state.verification = v;
        self.open_dialog = None;
        if v == CgVerification::Ok {
            self.awaiting_char_set_for_login = true;
            return;
        }
        self.state.verification = CgVerification::Undef;
        // Every refusal also sets the slot to -1, so a second Finish after fixing the name sends
        // slot -1.
        self.state.set_slot(-1);
        self.error_string_id = v.error_string_id();
        self.open_dialog = Some(CharGenDialog::ErrorMessage);
    }

    /// The client's awaiting-character-set branch, run when the fresh `0xF658` arrives.
    ///
    /// The wanted name is the state's name, with a `+` in front for create-as-admin. The first
    /// character in the set that is not greyed out, has a non-zero id and matches the name
    /// case-insensitively (`_stricmp`) is logged on; with no match the client queues UI mode
    /// `0x1000000A`.
    ///
    /// So a successful creation logs the new character in **without** a visit to character select.
    /// The `+` prefix is the client's own admin naming and matches ACE's plussed characters.
    pub fn character_set_arrived(&mut self, set: &CharacterSet) -> Option<UiMode> {
        if !self.awaiting_char_set_for_login {
            return None;
        }
        self.awaiting_char_set_for_login = false;
        let want = if self.state.create_as_admin {
            format!("+{}", self.state.name)
        } else {
            self.state.name.clone()
        };
        for (i, c) in set.set.iter().enumerate() {
            let greyed = crate::screens::charmgmt::greyed_out_for(set, i) != 0;
            if !greyed && c.id.0 != 0 && c.name.eq_ignore_ascii_case(&want) {
                self.actions.push(CharGenAction::LogOn(c.id));
                return None;
            }
        }
        // ACE plusses every character on an account whose access level is above Advocate, so the
        // name that comes back may carry a `+` the client did not ask for. Fall back to matching
        // without it rather than dropping through to character select.
        for (i, c) in set.set.iter().enumerate() {
            let greyed = crate::screens::charmgmt::greyed_out_for(set, i) != 0;
            let bare = c.name.strip_prefix('+').unwrap_or(&c.name);
            if !greyed && c.id.0 != 0 && bare.eq_ignore_ascii_case(self.state.name.trim()) {
                self.actions.push(CharGenAction::LogOn(c.id));
                return None;
            }
        }
        Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
    }

    // ---------------------------------------------------------------------------------------
    // The profession page
    // ---------------------------------------------------------------------------------------

    /// Put the previously-current profession button back to state `0x10000016`, put the one the
    /// template names into `0x10000017`, and show that template's blurb in the text box.
    ///
    /// A template outside 0..=6 falls into the switch's `default:`, which leaves the current
    /// profession button **null** and sets no text at all.
    pub fn update_profession(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        if let Some(prev) = self.current_profession_button {
            if let Some(h) = ui.get_child_recursive(root, prev) {
                ui.set_state(h, STATE_PROFESSION_OFF);
            }
        }
        let found = PROFESSION_BUTTONS
            .iter()
            .find(|(_, t, _)| *t == self.state.template);
        self.current_profession_button = found.map(|(id, _, _)| *id);
        self.profession_text = found.map(|(_, _, tok)| *tok);
        if let Some((id, _, _)) = found {
            if let Some(h) = ui.get_child_recursive(root, *id) {
                ui.set_state(h, STATE_PROFESSION_ON);
            }
        }
        if let (Some(tok), Some(h)) = (
            self.profession_text,
            ui.get_child_recursive(root, PROFESSION_DESC),
        ) {
            let text = self
                .world_profession_text()
                .unwrap_or_else(|| self.string(ui, tok));
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&text);
            }
        }
    }

    /// On a world whose table names its own page help, the Skills and Appearance panes read it:
    /// the skills page's, and the appearance page's then the clothing page's. The table's page
    /// help runs sex, appearance, clothing, heraldry, attributes, skills, spells and name.
    fn world_page_help(&self, ui: &mut UiSystem) {
        let (Some(tables), Some(root)) = (self.tables.clone(), self.roots.first().copied()) else {
            return;
        };
        let help = |i: usize| {
            tables
                .texts
                .get(tables.chargen.help_strings.get(i).copied())
        };
        let appearance = match (help(1), help(2)) {
            (Some(face), Some(clothes)) => Some(format!("{face}\n\n{clothes}")),
            (face, clothes) => face.or(clothes).map(str::to_owned),
        };
        for (id, text) in [
            (skills_page::DESCRIPTION, help(5).map(str::to_owned)),
            (appearance::HELP, appearance),
        ] {
            if let (Some(text), Some(h)) = (text, ui.get_child_recursive(root, id)) {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&text);
                }
            }
        }
    }

    /// The chosen heritage's description, when the world's own files carry one.
    fn world_heritage_text(&self) -> Option<String> {
        let tables = self.tables.as_ref()?;
        let heritage = tables
            .chargen
            .heritage_groups
            .get(&self.state.heritage_group)?;
        tables.texts.get(heritage.description).map(str::to_owned)
    }

    /// The chosen profession's description for the chosen sex, when the world's own files carry
    /// one.
    fn world_profession_text(&self) -> Option<String> {
        let tables = self.tables.as_ref()?;
        let heritage = tables
            .chargen
            .heritage_groups
            .get(&self.state.heritage_group)?;
        let template = usize::try_from(self.state.template).ok()?;
        let shown = heritage.template_presentation(self.state.gender, template)?;
        tables.texts.get(shown.description).map(str::to_owned)
    }

    /// The name page's help and the chosen sex's naming help, when the world's own files carry
    /// them.
    fn world_naming_text(&self) -> Option<String> {
        let tables = self.tables.as_ref()?;
        let sex = tables
            .chargen
            .heritage_groups
            .get(&self.state.heritage_group)?
            .sexes
            .get(&self.state.gender)?;
        let naming = tables.texts.get(sex.naming_help)?;
        // The page help's last entry is the name page's.
        Some(
            match tables
                .texts
                .get(tables.chargen.help_strings.last().copied())
            {
                Some(page) => format!("{page}\n\n{naming}"),
                None => naming.to_owned(),
            },
        )
    }

    /// The six sliders and the four read-outs, and then the template fit + profession update off
    /// its own tail.
    ///
    /// Each slider gets its value as text on the value field and as float attribute `0x86 = value *
    /// 0.01` on the scroll bar; the credits-available field gets the remaining attribute credits.
    pub fn update_attribute_values(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (field, attr) in ATTRIBUTE_SLIDERS {
            let Some(f) = ui.get_child_recursive(root, field) else {
                continue;
            };
            let v = self.state.get(attr);
            if let Some(h) = ui.get_child_recursive(f, slider::VALUE) {
                if let Some(t) = ui.text_element_mut(h) {
                    t.set_text(&v.to_string());
                }
            }
            if let Some(h) = ui.get_child_recursive(f, slider::SCROLL) {
                #[allow(clippy::cast_precision_loss)] // 10..=100 is exact in f32
                let pos = v as f32 * 0.01;
                // Setting the float attribute *stores* the value and then invokes attribute
                // handling. A bare handler call would leave the position the page wrote out of the
                // element's own property collection — and with the scrollbar reading attribute 0x86
                // back out of it every layout, a write that stores nothing puts the thumb back
                // where it was.
                ui.set_attribute_float(h, ATTR_SCROLL_POSITION, pos);
            }
        }
        // The credits available, then the three vitals.
        //
        // These readouts use endurance/2, endurance and self directly; this path does not
        // load the secondary-attribute table.
        let endurance = self.state.get(Attr::Endurance);
        let readouts = [
            self.state.remaining_atrb_credits,
            endurance / 2,
            endurance,
            self.state.get(Attr::Self_),
        ];
        for ((field, text), v) in PROFESSION_READOUTS.iter().zip(readouts) {
            let Some(f) = ui.get_child_recursive(root, *field) else {
                continue;
            };
            let Some(h) = ui.get_child_recursive(f, *text) else {
                continue;
            };
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(&v.to_string());
            }
        }
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.update_profession(ui);
    }

    /// The profession page's attribute-value write.
    ///
    /// The page clamps the request to what the budget can pay for — the current value plus the
    /// remaining credits with this attribute excluded — **before** calling the setter, which is why
    /// raising a slider past the budget lands on the highest legal value instead of being refused.
    pub fn set_attrib_value(&mut self, ui: &mut UiSystem, a: Attr, value: i32) {
        let cur = self.state.get(a);
        let head = self.state.get_abs_remaining_credits(Some(a));
        let value = if value - cur >= 0 && head < value - cur {
            cur + head
        } else {
            value
        };
        self.state.set_attribute(a, value);
        self.update_attribute_values(ui);
    }

    /// The toggle, its state writes on both the scrollbar and the lock button, and the attribute
    /// lock.
    pub fn set_lock(&mut self, ui: &mut UiSystem, a: Attr) {
        let now = !self.state.is_locked(a);
        self.state.set_locked(a, now);
        let state = if now {
            STATE_SLIDER_LOCKED
        } else {
            STATE_SLIDER_UNLOCKED
        };
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(field) = ATTRIBUTE_SLIDERS
            .iter()
            .find(|(_, at)| *at == a)
            .and_then(|(id, _)| ui.get_child_recursive(root, *id))
        else {
            return;
        };
        for id in [slider::SCROLL, slider::LOCK] {
            if let Some(h) = ui.get_child_recursive(field, id) {
                ui.set_state(h, state);
            }
        }
    }

    /// Reset the attribute locks first, then clear the lock on every slider that still *shows* as
    /// locked, which is what puts the states back.
    pub fn clear_locks(&mut self, ui: &mut UiSystem) {
        for a in Attr::ALL {
            self.state.set_locked(a, false);
        }
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (id, _) in ATTRIBUTE_SLIDERS {
            let Some(field) = ui.get_child_recursive(root, id) else {
                continue;
            };
            for c in [slider::SCROLL, slider::LOCK] {
                if let Some(h) = ui.get_child_recursive(field, c) {
                    ui.set_state(h, STATE_SLIDER_UNLOCKED);
                }
            }
        }
    }

    /// The profession page's "update to default attributes", which every profession button runs
    /// after setting and applying the template.
    pub fn update_to_default_attributes(&mut self, ui: &mut UiSystem) {
        self.clear_locks(ui);
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.update_profession(ui);
        self.update_attribute_values(ui);
    }

    /// One of the seven profession buttons: set and apply the template, then update to the default
    /// attributes.
    pub fn choose_profession(&mut self, ui: &mut UiSystem, template: i32) {
        let Some(t) = self.tables.clone() else { return };
        self.state
            .set_template(&t.chargen, &t.skills, template, true);
        self.update_to_default_attributes(ui);
        self.do_skill_records(ui);
    }

    /// The profession page's update — fit the template to the character, update the profession,
    /// update the attribute values.
    pub fn profession_page_update(&mut self, ui: &mut UiSystem) {
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.update_profession(ui);
        self.update_attribute_values(ui);
    }

    // ---------------------------------------------------------------------------------------
    // The skills page
    // ---------------------------------------------------------------------------------------

    /// The skills page's skill-records step.
    ///
    /// A flush, then the four category rows from template **0**, then one row from template **1**
    /// per entry of the `SkillTable`'s hash — each carrying its skill id in the instance property
    /// `0x1000000A`, its name as the row caption, and the two costs the heritage overrides.
    ///
    /// The heritage arm is reproduced as the client writes it and is **not** the trained-cost
    /// lookup: the page keeps the `SkillTable`'s base costs and only reacts when the heritage's own
    /// cost is **zero** — a normal cost of 0 makes the skill free *and* unremovable and takes its
    /// trained cost out of the specialised one; a primary cost of 0 does the same for
    /// specialisation. A heritage that merely made a skill *cheaper* would not be honoured here,
    /// and none of the thirteen shipped heritages does: every one overrides Arcane Lore (14) alone,
    /// with `(0, 2)`.
    pub fn do_skill_records(&mut self, ui: &mut UiSystem) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(list) = ui.get_child_recursive(root, skills_page::LIST) else {
            return;
        };
        // The previous build's rows and headers both go.
        if let Some(mut old) = self.skill_list.take() {
            old.flush(ui);
        }
        for r in std::mem::take(&mut self.skill_rows) {
            if let Some(h) = r.element {
                ui.remove_and_delete_root(h);
            }
        }
        for h in std::mem::take(&mut self.skill_headers) {
            ui.remove_and_delete_root(h);
        }
        let mut lb = ListBoxWidget::bind(ui, list);

        // The four category rows, **all four from template 0** and all four appended first — four
        // adds from template 0, appended. They are the group *bounds*, not a block of headings:
        // every skill row is inserted between two of them below.
        for (label, _tip) in SKILL_CATEGORIES {
            let Some(h) = lb.add_from_template(ui, 0, None) else {
                break;
            };
            let text = self.string(ui, label);
            if let Some(t) = ui
                .get_child_recursive(h, skills_page::HEADER_TEXT)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&text);
            }
            self.skill_headers.push(h);
        }

        let heritage = self.state.heritage_group;
        let over = tables
            .chargen
            .heritage_groups
            .get(&heritage)
            .map(|h| h.skills.clone())
            .unwrap_or_default();
        let mut rows: Vec<SkillRecord> = Vec::new();
        for (id, base) in &tables.skills.skills {
            let mut train = base.trained_cost;
            let mut spec = base.specialized_cost;
            let mut untrainable = train != 0;
            let mut unspecializable = spec != 0;
            if let Some((_, normal, primary)) = over.iter().find(|(s, _, _)| s == id) {
                if *normal == 0 {
                    spec -= train;
                    train = 0;
                    untrainable = false;
                }
                if *primary == 0 {
                    spec = 0;
                    unspecializable = false;
                }
            }
            rows.push(SkillRecord {
                skill: *id,
                name: base.name.clone(),
                train_cost: train,
                spec_cost: spec,
                untrainable,
                unspecializable,
                level: self.state.skill_level(*id),
                score: self.state.skill_score(&tables.skills, *id),
                min_level: base.min_level,
                element: None,
            });
        }
        // The client walks the char-gen state's own skill list, whose order it never relies on: the
        // skill-entry update re-files every row by name. Sorting here only makes `skill_rows` read
        // the way the page does.
        rows.sort_by(|a, b| a.name.cmp(&b.name).then(a.skill.cmp(&b.skill)));
        for r in &mut rows {
            // An add from template 1 — appended at the end, which is where the skill-entry update's
            // remove then takes it from.
            let Some(h) = lb.add_from_template(ui, 1, None) else {
                break;
            };
            if let Some(n) = ui.node_mut(h) {
                n.instance_properties.set(
                    ATTR_ROW_SKILL_ID,
                    dereth_assets::ui::PropertyValue::InstanceId(r.skill),
                );
            }
            ui.set_mouse_visible(h, true);
            if let Some(t) = ui
                .get_child_recursive(h, skills_page::ROW_NAME)
                .and_then(|c| ui.text_element_mut(c))
            {
                t.set_text(&r.name);
            }
            r.element = Some(h);
        }
        self.skill_rows = rows;
        self.skill_list = Some(lb);
        for i in 0..self.skill_rows.len() {
            self.update_skill_entry(ui, i);
        }
        self.update_credits_meter(ui);
    }

    /// Update and refile one skills-page entry.
    ///
    /// The row is removed from the list's items (not deleted), its text is set, and it is inserted
    /// sorted between its own header and the next one.
    ///
    /// The remove-then-insert is the whole point: **this is what puts a row under its own heading**
    /// in the list's items, and `Self::relayout_skill_list` is what turns that order into the
    /// rectangles the page draws.
    ///
    /// This alone does not move the row on screen: a re-file is **two** steps, the array and the
    /// geometry. If the list box's scrollable half caches each row's origin behind a key that
    /// cannot see a re-order, it puts every row back on the next tick and a trained skill stays
    /// under Useable Untrained Skills until the page is left and re-entered. An assertion that
    /// walks `skill_list.items` proves only the first step.
    pub fn update_skill_entry(&mut self, ui: &mut UiSystem, i: usize) {
        let Some((h, level, min_level, name)) = self
            .skill_rows
            .get(i)
            .and_then(|r| Some((r.element?, r.level, r.min_level, r.name.clone())))
        else {
            return;
        };
        // The client takes the row out of the list's items; it does **not** delete it, because the
        // insert below puts it straight back.
        if let Some(lb) = self.skill_list.as_mut() {
            lb.items.retain(|x| *x != h);
        }
        self.set_skill_text(ui, i);
        let Some(group) = skill_group(level, min_level) else {
            // The skill-text and skill-entry updates' `default:`: an undefined skill is removed and
            // never re-inserted, so it has no row at all.
            ui.remove_and_delete_root(h);
            if let Some(r) = self.skill_rows.get_mut(i) {
                r.element = None;
            }
            self.relayout_skill_list(ui);
            return;
        };
        let at = self.sorted_insert_index(ui, group, &name);
        if let Some(lb) = self.skill_list.as_mut() {
            let at = at.min(lb.items.len());
            lb.items.insert(at, h);
        }
        self.relayout_skill_list(ui);
    }

    /// Where in the list's items a row goes.
    ///
    /// Starting after the group's own header and stopping at the next header (or the end), it reads
    /// each item's `0x1000000A` skill id, skips anything that is not a skill row (a header has no
    /// such attribute), and stops at the first row whose name sorts after the new one (`wcscmp`);
    /// the row is inserted there. So it is an insertion sort by name, ascending, **inside one group
    /// only** — the same shape used by the in-game panel.
    fn sorted_insert_index(&self, ui: &UiSystem, group: usize, name: &str) -> usize {
        let Some(lb) = self.skill_list.as_ref() else {
            return 0;
        };
        let Some(header) = self.skill_headers.get(group).copied() else {
            return lb.items.len();
        };
        let Some(start) = lb.index_of(header) else {
            return lb.items.len();
        };
        let end = self
            .skill_headers
            .get(group + 1)
            .and_then(|h| lb.index_of(*h))
            .unwrap_or(lb.items.len());
        let mut i = start + 1;
        while i < end {
            let existing = lb.items[i];
            let other = ui
                .node(existing)
                .and_then(|n| match n.instance_properties.get(ATTR_ROW_SKILL_ID) {
                    Some(dereth_assets::ui::PropertyValue::InstanceId(v)) => Some(*v),
                    _ => None,
                })
                .and_then(|id| self.skill_rows.iter().find(|r| r.skill == id));
            if let Some(o) = other {
                if name < o.name.as_str() {
                    break;
                }
            }
            i += 1;
        }
        i
    }

    /// The list box's layout update, which the client runs itself whenever its items change —
    /// insert and remove both mark the layout dirty.
    ///
    /// Positioning the rows by hand at a fixed 21-pixel pitch cannot express a header row of a
    /// different height and leaves every row's `y` wrong the moment the list is re-ordered.
    fn relayout_skill_list(&mut self, ui: &mut UiSystem) {
        if let Some(lb) = self.skill_list.as_mut() {
            lb.update_layout(ui);
        }
    }

    /// The *Skill Level* number, the two credit costs
    /// and the two arrow states.
    ///
    /// The skill level is recomputed as the skill score and printed with `"%d"`. Then, by class:
    ///
    /// | class | up cost | down cost | + arrow on when | - arrow on when |
    /// |---|---|---|---|---|
    /// | untrained | train cost (blank if ≥ 999) | 0 | credits ≥ train cost | never |
    /// | trained | spec − train (blank if ≥ 999) | train cost | credits ≥ spec − train | untrainable |
    /// | specialized | 0 | spec − train | never | unspecializable |
    ///
    /// An undefined class writes nothing more. "On" is state `0x1000001B` and "off" `0x1000001A`.
    ///
    /// **Every constant here is the client's own**: the 999 is a `< 0x3E7` test, the empty string
    /// is `L""` and the format is `L"%d"`.
    fn set_skill_text(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some((h, skill, level, train, spec, untrainable, unspecializable)) =
            self.skill_rows.get(i).and_then(|r| {
                Some((
                    r.element?,
                    r.skill,
                    r.level,
                    r.train_cost,
                    r.spec_cost,
                    r.untrainable,
                    r.unspecializable,
                ))
            })
        else {
            return;
        };
        // The skill level is the skill score, recomputed on every call, which is how the number
        // rises by 5 when the skill is trained and by 10 when it is specialised.
        let score = self.state.skill_score(&tables.skills, skill);
        if let Some(r) = self.skill_rows.get_mut(i) {
            r.score = score;
        }
        set_row_text(ui, h, skills_page::ROW_LEVEL, &score.to_string());
        if level == SkillAdvancementClass::Inactive {
            return;
        }
        let credits = self.state.remaining_skill_credits;
        // 999 or more prints as the empty string `L""`.
        let printed = |v: i32| {
            if v < 999 {
                v.to_string()
            } else {
                String::new()
            }
        };
        let (up, down, up_on, down_on) = match level {
            SkillAdvancementClass::Untrained => {
                (printed(train), "0".to_string(), credits >= train, false)
            }
            SkillAdvancementClass::Trained => (
                printed(spec - train),
                train.to_string(),
                credits >= spec - train,
                untrainable,
            ),
            SkillAdvancementClass::Specialized => (
                "0".to_string(),
                (spec - train).to_string(),
                false,
                unspecializable,
            ),
            SkillAdvancementClass::Inactive => return,
        };
        set_row_text(ui, h, skills_page::ROW_UP_COST, &up);
        set_row_text(ui, h, skills_page::ROW_DOWN_COST, &down);
        for (id, on) in [
            (skills_page::ROW_INCREASE, up_on),
            (skills_page::ROW_DECREASE, down_on),
        ] {
            if let Some(c) = ui.get_child_recursive(h, id) {
                ui.set_state(c, if on { STATE_ARROW_ON } else { STATE_ARROW_OFF });
            }
        }
    }

    /// The skill-text update over every record.
    ///
    /// It is what both arrows call after a purchase, and it is not cosmetic: buying one skill
    /// changes the remaining skill credits, and every *other* row's `+` has to go dark the moment
    /// the credits can no longer pay for it.
    pub fn update_all_training_values(&mut self, ui: &mut UiSystem) {
        for i in 0..self.skill_rows.len() {
            self.set_skill_text(ui, i);
        }
    }

    /// The skills page's credits meter update.
    pub fn update_credits_meter(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let v = self.state.remaining_skill_credits;
        if let Some(t) = ui
            .get_child_recursive(root, skills_page::CREDITS_FIELD)
            .and_then(|f| ui.get_child_recursive(f, skills_page::READOUT_TEXT))
            .and_then(|c| ui.text_element_mut(c))
        {
            t.set_text(&v.to_string());
        }
    }

    /// Untrained → trained → specialised, each
    /// gated on the credits the step costs.
    ///
    /// The specialise step costs `spec - train`, not `spec`: the skill has already paid to be
    /// trained.
    pub fn increase_skill_level(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(r) = self.skill_rows.get(i) else {
            return;
        };
        let (skill, train, spec, level) = (r.skill, r.train_cost, r.spec_cost, r.level);
        let credits = self.state.remaining_skill_credits;
        let next = match level {
            SkillAdvancementClass::Untrained | SkillAdvancementClass::Inactive
                if credits >= train =>
            {
                SkillAdvancementClass::Trained
            }
            SkillAdvancementClass::Trained if credits >= spec - train => {
                SkillAdvancementClass::Specialized
            }
            _ => return,
        };
        self.state
            .set_skill_level(&tables.chargen, &tables.skills, skill, next);
        if let Some(r) = self.skill_rows.get_mut(i) {
            r.level = next;
        }
        self.update_skill_entry(ui, i);
        // Every arm of the skill-level increase falls into this tail.
        self.update_all_training_values(ui);
        self.update_credits_meter(ui);
    }

    /// The inverse, refused for a skill the heritage granted (unspecializable / untrainable false).
    pub fn decrease_skill_level(&mut self, ui: &mut UiSystem, i: usize) {
        let Some(tables) = self.tables.clone() else {
            return;
        };
        let Some(r) = self.skill_rows.get(i) else {
            return;
        };
        let (skill, level, untrainable, unspecializable) =
            (r.skill, r.level, r.untrainable, r.unspecializable);
        let next = match level {
            SkillAdvancementClass::Specialized if unspecializable => SkillAdvancementClass::Trained,
            SkillAdvancementClass::Trained if untrainable => SkillAdvancementClass::Untrained,
            _ => return,
        };
        self.state
            .set_skill_level(&tables.chargen, &tables.skills, skill, next);
        if let Some(r) = self.skill_rows.get_mut(i) {
            r.level = next;
        }
        self.update_skill_entry(ui, i);
        // The decrease path reaches the same tail.
        self.update_all_training_values(ui);
        self.update_credits_meter(ui);
    }

    // ---------------------------------------------------------------------------------------
    // The appearance page
    // ---------------------------------------------------------------------------------------

    /// The nine part choices, straight off the heritage's per-gender lists. Slot 4 (skin) is the
    /// one the client never fills.
    pub fn setup_parts(&mut self) {
        let Some(t) = self.tables.clone() else { return };
        let Some(sx) = self.state.sex(&t.chargen).cloned() else {
            return;
        };
        let n = |v: usize| i32::try_from(v).unwrap_or(0);
        let s = &self.state;
        self.choices[0] = PartChoice {
            current: s.hair_style,
            num: n(sx.hair_styles.len()),
            color: s.hair_color,
            num_colors: n(sx.hair_colors.len()),
            shade: s.hair_shade,
        };
        self.choices[1] = PartChoice {
            current: s.eyes_strip,
            num: n(sx.eye_strips.len()),
            color: s.eye_color,
            num_colors: n(sx.eye_colors.len()),
            shade: 0.0,
        };
        self.choices[2] = PartChoice {
            current: s.nose_strip,
            num: n(sx.nose_strips.len()),
            color: -1,
            num_colors: 0,
            shade: 0.0,
        };
        self.choices[3] = PartChoice {
            current: s.mouth_strip,
            num: n(sx.mouth_strips.len()),
            color: -1,
            num_colors: 0,
            shade: 0.0,
        };
        self.choices[5] = PartChoice {
            current: s.headgear_style,
            num: n(sx.headgear.len()),
            color: s.headgear_color,
            num_colors: s.num_headgear_colors,
            shade: s.headgear_shade,
        };
        self.choices[6] = PartChoice {
            current: s.shirt_style,
            num: n(sx.shirts.len()),
            color: s.shirt_color,
            num_colors: s.num_shirt_colors,
            shade: s.shirt_shade,
        };
        self.choices[7] = PartChoice {
            current: s.trousers_style,
            num: n(sx.pants.len()),
            color: s.trousers_color,
            num_colors: s.num_trousers_colors,
            shade: s.trousers_shade,
        };
        self.choices[8] = PartChoice {
            current: s.footwear_style,
            num: n(sx.footwear.len()),
            color: s.footwear_color,
            num_colors: s.num_footwear_colors,
            shade: s.footwear_shade,
        };
    }

    /// The two arrows inside a part row, `0x1000030A` (previous, `-1`) and `0x1000030B` (next,
    /// `+1`), whose *parent* names the part.
    ///
    /// The wrap is the client's, written out per part and identical for eight of the nine:
    /// `cur += d; if (cur < num) { if (cur < 0) cur = num - 1 } else { cur = 0 }`. **Headgear is
    /// the exception** — its floor is `-1`, "no hat", and it wraps off the top to `-1` rather than
    /// to 0, which is what makes bare-headed a reachable choice and is why ACE reads
    /// `HeadgearStyle == uint.MaxValue` as "no headgear".
    pub fn cycle_part(&mut self, ui: &mut UiSystem, part: EParts, d: i32) {
        let Some(t) = self.tables.clone() else { return };
        let i = part.choice_index();
        let (num, cur) = (self.choices[i].num, self.choices[i].current + d);
        let bare = part == EParts::Headgear;
        let cur = if cur < num {
            if cur < 0 && !(bare && cur == -1) {
                num - 1
            } else {
                cur
            }
        } else if bare {
            -1
        } else {
            0
        };
        self.choices[i].current = cur;
        match part {
            EParts::Hair => self.state.set_hair_style(cur),
            EParts::Eyes => self.state.eyes_strip = cur,
            EParts::Nose => self.state.nose_strip = cur,
            EParts::Mouth => self.state.mouth_strip = cur,
            EParts::Headgear => self.state.set_headgear_style(&t.chargen, cur),
            EParts::Shirt => self.state.set_shirt_style(&t.chargen, cur),
            EParts::Trousers => self.state.set_trousers_style(&t.chargen, cur),
            EParts::Footwear => self.state.set_footwear_style(&t.chargen, cur),
            EParts::Skin | EParts::Invalid => {}
        }
        self.set_selection(ui, part);
    }

    /// One of the nine part rows becomes the
    /// selected one, and the whole colour wheel is rebuilt for it.
    ///
    /// The tail takes the part's current colour and colour count, lights the current selection
    /// (state 6), builds the colour spots, then for Eyes draws the flat gradient disk and hides the
    /// shade slider, and for every other part draws the shaded disk, shows the slider and sets the
    /// part's shade; finally it writes the current colour.
    ///
    /// **Eyes is the one part with no shade**, which is why its disk is the flat `GradientPlug` and
    /// its slider is hidden.
    pub fn set_selection(&mut self, ui: &mut UiSystem, part: EParts) {
        self.current_part = part;
        self.setup_parts();
        // Selection writes the colour count; parts setup does not.
        self.fill_color_wheel();
        let i = part.choice_index();
        self.current_color = self.choices[i].color;
        // **The selection write's own head.** For each of the nine colour-wheel slots the client
        // shows the colour spot and hides its pointer; then it sets state 1 on the row that **was**
        // selected, and state 6 on the one that now is. Without the pointer half every one of the
        // nine markers the shipped layout starts visible stays visible for ever: **nine** gold
        // arrow heads around the wheel where retail shows exactly **one**, next to the chosen
        // colour. The colour write — called from this function's tail — is what puts that one back.
        if let Some(root) = self.roots.first().copied() {
            for (spot, pointer) in appearance::COLOR_SPOTS
                .iter()
                .zip(appearance::COLOR_POINTERS.iter())
            {
                if let Some(h) = ui.get_child_recursive(root, *spot) {
                    ui.set_visible(h, true);
                }
                if let Some(h) = ui.get_child_recursive(root, *pointer) {
                    ui.set_visible(h, false);
                }
            }
            for (row, state) in [
                (self.current_selection_row, STATE_ROW_UNSELECTED),
                (Self::selection_row(part), STATE_ROW_SELECTED),
            ] {
                let Some(id) = row else { continue };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_state(h, state);
                }
            }
        }
        self.current_selection_row = Self::selection_row(part);
        self.do_color_spots(ui);
        let eyes = part == EParts::Eyes;
        if let Some(root) = self.roots.first().copied() {
            if let Some(h) = ui.get_child_recursive(root, appearance::SHADE_SCROLL) {
                ui.set_visible(h, !eyes);
            }
        }
        self.set_color(ui, self.current_color);
        self.refresh_view(ui);
    }

    /// The Face / Clothes tabs — shows one of the two row groups.
    ///
    /// **The model half** is the whole of the held headgear:
    ///
    /// Face lights the Face radio (`0x10000017`, Clothes `0x10000016`), parks the headgear style in
    /// the held headgear and sets the live one to -1 (bare the head so the face is visible), shows
    /// the face rows and zooms in. Clothes saves the camera start, lights the Clothes radio,
    /// restores the held headgear when `-2 < held < headgear choice count`, shows the clothes rows
    /// and zooms out.
    ///
    /// The restore guard is `-2 <`, not `-1 <`: **-1 is a legal headgear style** ("no hat", the
    /// value ACE reads as `HeadgearStyle == uint.MaxValue`), so a player who took the hat off on
    /// the Clothes tab keeps it off. Choice slot 5 is [`EParts::Headgear`]'s row.
    pub fn set_choice(&mut self, ui: &mut UiSystem, face: bool) {
        if let Some(t) = self.tables.clone() {
            if face {
                self.hold_headgear = self.state.headgear_style;
                self.state.set_headgear_style(&t.chargen, -1);
            } else if self.hold_headgear > -2
                && self.hold_headgear < self.choices[EParts::Headgear.choice_index()].num
            {
                self.state
                    .set_headgear_style(&t.chargen, self.hold_headgear);
            }
        }
        self.apply_choice_view(ui, face);
    }

    /// The appearance choice's *view* half — the two row groups, the two radios, and the
    /// camera — with nothing that touches [`CharGenState`].
    ///
    /// It is separate because the per-frame update applies the **whole** Face choice on exactly
    /// three of the thirteen heritages — 6 (Gear Knight) and the two Olthoi, which have no Clothes
    /// tab — and on the other ten calls nothing of the sort. A rebuild that ran the full choice
    /// write from every update would park -1 in [`Self::hold_headgear`] the second time round and
    /// lose the hat for good.
    fn apply_choice_view(&mut self, ui: &mut UiSystem, face: bool) {
        self.face_tab = face;
        if let Some(root) = self.roots.first().copied() {
            for (id, want) in [
                (appearance::GROUP_FACE, face),
                (appearance::GROUP_CLOTHES, !face),
            ] {
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, want);
                }
            }
            // **The two radios**, which the choice write sets; without them no radio is selected.
            // The same `0x10000016`/`0x10000017` pair the heritage and profession buttons use.
            for (id, on) in [
                (appearance::TAB_FACE, face),
                (appearance::TAB_CLOTHES, !face),
            ] {
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_state(
                        h,
                        if on {
                            STATE_PROFESSION_ON
                        } else {
                            STATE_PROFESSION_OFF
                        },
                    );
                }
            }
        }
        // **The camera half.** The choice path's last statement is a zoom in on the
        // Face arm and a zoom out on the Clothes arm, so the Face tab is a head-and-shoulders view
        // and the Clothes tab a whole-body one -- and the page opens on Face, which is why retail
        // shows a face on arrival rather than a whole body. `zoom`'s own guard
        // is the client's — an unchanged zoom state re-latches the pressed half's button and moves
        // nothing else — so calling it from every repaint is what the update's tail does and not a
        // second zoom.
        self.zoom(ui, face);
    }

    /// The nine spots, gated on the part's colour count.
    ///
    /// Nose, mouth and skin have no colour arm at all: skin is a shade only.
    pub fn set_color(&mut self, ui: &mut UiSystem, index: i32) {
        let i = self.current_part.choice_index();
        if index >= self.choices[i].num_colors {
            return;
        }
        // The client's first two statements: the marker over the **old** spot goes away and the one
        // over the new spot appears, each guarded by `!= -1`. This is the wheel answering the
        // click. Both are done through the same element list the page binds
        // as the colour-wheel pointers.
        if let Some(root) = self.roots.first().copied() {
            for (slot, want) in [(self.current_color, false), (index, true)] {
                let Some(id) = usize::try_from(slot)
                    .ok()
                    .and_then(|s| appearance::COLOR_POINTERS.get(s).copied())
                else {
                    continue;
                };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, want);
                }
            }
        }
        self.current_color = index;
        self.choices[i].color = index;
        match self.current_part {
            EParts::Hair => self.state.set_hair_color(index),
            EParts::Eyes => self.state.set_eye_color(index),
            EParts::Headgear => self.state.headgear_color = index,
            EParts::Shirt => self.state.shirt_color = index,
            EParts::Trousers => self.state.trousers_color = index,
            EParts::Footwear => self.state.footwear_color = index,
            EParts::Nose | EParts::Mouth | EParts::Skin | EParts::Invalid => {}
        }
        // The gradient disk is drawn flat only for Eyes (the flag is set on that arm of the switch
        // above and nowhere else), so every other part re-tints the ring by the colour just chosen.
        //
        self.do_grad_disk(ui, self.current_part == EParts::Eyes);
        self.refresh_view(ui);
    }

    /// One slider, six destinations, and **nose,
    /// mouth and skin all write the skin shade**, which is the client's own fall-through.
    pub fn set_shade(&mut self, ui: &mut UiSystem, shade: f64) {
        let i = self.current_part.choice_index();
        self.choices[i].shade = shade;
        match self.current_part {
            EParts::Hair => self.state.set_hair_shade(shade),
            EParts::Nose | EParts::Mouth | EParts::Skin => self.state.set_skin_shade(shade),
            EParts::Headgear => self.state.headgear_shade = shade,
            EParts::Shirt => self.state.shirt_shade = shade,
            EParts::Trousers => self.state.trousers_shade = shade,
            EParts::Footwear => self.state.footwear_shade = shade,
            EParts::Eyes | EParts::Invalid => {}
        }
        self.refresh_view(ui);
    }

    /// Set the gender, then update the appearance state and
    /// preview.
    pub fn choose_gender(&mut self, ui: &mut UiSystem, gender: u32) {
        let Some(t) = self.tables.clone() else { return };
        self.state.set_gender(&t.chargen, gender);
        self.appearance_update(ui);
    }

    /// Set the zoom-in or zoom-out camera target. The client eases the camera
    /// through its zoom animation; the target is what the preview is asked for and the ease is the
    /// host's, so only the target crosses the seam.
    ///
    /// **The buttons' own view** is the whole of what those two functions
    /// do besides the camera. Both set button state through the same operation that the update's
    /// gender arm uses, on **both** halves, and they are not symmetrical.
    ///
    /// Zoom in, when not already zoomed in, sets the targets, puts the zoom-in button in state
    /// **6** and zoom-out in **1**, stops the animation and marks the view zoomed in; zoom out,
    /// when zoomed in, sets the targets, puts zoom-in in **1** and zoom-out in **6**, starts the
    /// animation and clears the flag. When the state already matches, each only re-latches its own
    /// button to 6.
    ///
    /// `6` is `widgets::button::state::TOGGLED` and `1` is `NORMAL`, so **exactly one half is ever lit**:
    /// the one last pressed. The `if` that the client's own guard falls through to still
    /// re-latches the pressed half, which is why a second `+` on an already-zoomed view is not a
    /// no-op even though nothing moves.
    ///
    /// The animation half is the same pair: the client's tail starts the animation when not zoomed
    /// in and stops it otherwise, so the Face tab holds a rest pose and the Clothes tab turns.
    pub fn zoom(&mut self, ui: &mut UiSystem, in_: bool) {
        if self.view3d.zoomed_in == in_ {
            // The client's fall-through, and it touches **one** button rather than the pair: zoom
            // in re-latches the zoom-in button to 6, and zoom out the mirror image. The half that
            // is already active is re-latched; the other is left exactly as it was.
            self.set_zoom_button_states(
                ui,
                if in_ { Some(true) } else { None },
                if in_ { None } else { Some(true) },
            );
            return;
        }
        self.view3d.zoomed_in = in_;
        self.view3d.camera_position = if in_ {
            zoomed_in_camera(self.state.heritage_group)
        } else {
            zoomed_out_camera(self.state.heritage_group)
        };
        self.view3d.camera_direction = [0.0, 0.0, 0.0];
        // Stop the animation on the way in, start it on the way out.
        self.view3d.animating = !in_;
        self.set_zoom_button_states(ui, Some(in_), Some(!in_));
        self.refresh_view(ui);
    }

    /// The two state writes the zoom in/out steps make, as one statement of which half is lit.
    ///
    /// Written as a pair rather than as "light the pressed one" because the changing path writes both,
    /// and lighting only the pressed half means that after `+` then `-` **both halves stay lit at
    /// once**, because nothing ever puts one
    /// back to `NORMAL`.
    /// `None` leaves that half alone, which is what the two fall-through paths do.
    fn set_zoom_button_states(
        &mut self,
        ui: &mut UiSystem,
        zoom_in_lit: Option<bool>,
        zoom_out_lit: Option<bool>,
    ) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        use dereth_ui::widgets::button::state::{NORMAL, TOGGLED};
        for (id, lit) in [
            (appearance::ZOOM_IN, zoom_in_lit),
            (appearance::ZOOM_OUT, zoom_out_lit),
        ] {
            let Some(lit) = lit else { continue };
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_state(h, if lit { TOGGLED } else { NORMAL });
            }
        }
    }

    /// The appearance page's update.
    pub fn appearance_update(&mut self, ui: &mut UiSystem) {
        // The client's head: the two gender buttons, `0x10000017` on the chosen one and
        // `0x10000016` on the other. A gender of 0 leaves both alone, which is the client's own
        // `goto`.
        if let Some(root) = self.roots.first().copied() {
            let female = match self.state.gender {
                1 => Some(false),
                2 => Some(true),
                _ => None,
            };
            if let Some(female) = female {
                for (id, on) in [
                    (appearance::GENDER_FEMALE, female),
                    (appearance::GENDER_MALE, !female),
                ] {
                    if let Some(h) = ui.get_child_recursive(root, id) {
                        ui.set_state(
                            h,
                            if on {
                                STATE_PROFESSION_ON
                            } else {
                                STATE_PROFESSION_OFF
                            },
                        );
                    }
                }
            }
        }
        // The client's heritage-6 arm, which is the **third** call site of the randomiser and an
        // edge, not a repaint: when the heritage is 6 and the last heritage was not, the appearance
        // and the clothing are randomised — a Gear Knight is re-rolled the first time the page
        // shows one and never again. The last heritage is written on that arm only, so leaving
        // heritage 6 and coming back re-rolls.
        if self.state.heritage_group == HERITAGE_GEAR_KNIGHT {
            if self.last_heritage_group != HERITAGE_GEAR_KNIGHT {
                if let Some(t) = self.tables.clone() {
                    self.state.randomize_appearance(&t.chargen, false);
                    self.state.randomize_clothing(&t.chargen, true);
                }
            }
            self.last_heritage_group = self.state.heritage_group;
        }
        // The client's three arms: the special heritages' chrome is in
        // [`Self::apply_special_heritage_chrome`], and the choice/selection third below.
        let special = matches!(
            self.state.heritage_group,
            HERITAGE_GEAR_KNIGHT | HERITAGE_OLTHOI | HERITAGE_OLTHOI_ACID
        );
        self.apply_special_heritage_chrome(ui);
        // **Not `set_choice`.** The update runs the full Face choice only on heritage 6 and the two
        // Olthoi, which have no Clothes tab; on the other ten it leaves the current tab alone.
        // Running the model half from here would park -1 in [`Self::hold_headgear`] on every
        // repaint. See [`Self::apply_choice_view`].
        if special {
            self.set_choice(ui, true);
            self.set_selection(ui, EParts::Hair);
        } else {
            // The view half only. Initialization already runs the Face model choice once for
            // ordinary heritages, so repeating it here would discard held headgear.
            self.apply_choice_view(ui, self.face_tab);
        }
        // The client's own tail, and the line that fills the colour wheel on the way in:
        // parts setup followed by selection of the current part. Without it the nine spots are
        // never generated at all until a part row or a tab is clicked -- nine black holes on a page
        // reached through its own tab.
        self.setup_parts();
        let part = self.current_part;
        self.set_selection(ui, part);
        self.refresh_view(ui);
        // The update's own tail: when visible, it updates the preview, sets the camera, and starts
        // the animation when not zoomed in (stops it otherwise). The Face tab holds a rest pose and
        // the Clothes tab turns.
        self.view3d.animating = !self.view3d.zoomed_in;
    }

    /// The client's **three arms**, everything but the choice-and-selection pair.
    ///
    /// Heritage **6** (Gear Knight) and the two Olthoi (`0x0C`, `0x0D`) do not get the ordinary
    /// page:
    ///
    /// | | ordinary (10) | Gear Knight (6) | Olthoi (0x0C, 0x0D) |
    /// |---|---|---|---|
    /// | Clothes button, Nose spinner, Mouth spinner | visible | hidden | hidden |
    /// | Hair / Eyes / Skin captions | [`APPEARANCE_CAPTIONS_ORDINARY`] | [`APPEARANCE_CAPTIONS_GEAR`] | [`APPEARANCE_CAPTIONS_OLTHOI`] |
    /// | the Eyes row's two arrows, attribute `0x0D` | `false`  | `true`  | `true`  |
    /// | Skin row y position | `0xB4`  | `0x5A`  | `0x5A`  |
    ///
    /// The client uses its visibility and position-change operations here, and the
    /// Boolean-attribute setter for the arrows. The resulting state changes establish those roles.
    ///
    /// **Why the whole page is a Face tab for these three.** They have no clothing at all, so
    /// hiding the Clothes button makes the Face choice the only reachable state; and with the Nose
    /// and Mouth rows gone the Skin row is the third of three rather than the fifth of five, which
    /// is the move.
    ///
    /// The camera each arm parks is [`zoomed_in_camera`]'s value for that heritage — the arms set
    /// both target and current camera position to it before applying the camera — so it is not
    /// repeated here; [`Self::refresh_view`] already asks for the same three positions.
    fn apply_special_heritage_chrome(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let captions = match self.state.heritage_group {
            HERITAGE_GEAR_KNIGHT => APPEARANCE_CAPTIONS_GEAR,
            HERITAGE_OLTHOI | HERITAGE_OLTHOI_ACID => APPEARANCE_CAPTIONS_OLTHOI,
            _ => APPEARANCE_CAPTIONS_ORDINARY,
        };
        let special = captions != APPEARANCE_CAPTIONS_ORDINARY;
        // The Clothes tab and the two facial spinners a heritage with no nose and no clothes has
        // nothing to put in.
        for id in [
            appearance::TAB_CLOTHES,
            appearance::NOSE_ROW,
            appearance::MOUTH_ROW,
        ] {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_visible(h, !special);
            }
        }
        // The three row labels.
        for (id, token) in [
            appearance::HAIR_ROW,
            appearance::EYES_ROW,
            appearance::SKIN_ROW,
        ]
        .iter()
        .zip(captions)
        {
            let text = self.string(ui, token);
            if let Some(t) = ui
                .get_child_recursive(root, *id)
                .and_then(|h| ui.text_element_mut(h))
            {
                t.set_text(&text);
            }
        }
        // The Eyes row's own two arrows: a Gear Knight and an Olthoi have one eye strip each, so
        // the arrows are greyed rather than hidden. The page initialisation does the same to the
        // **Skin** row's pair once at construction, on every heritage, because skin is a colour and
        // never a style.
        for (row, disabled) in [
            (appearance::EYES_ROW, special),
            (appearance::SKIN_ROW, true),
        ] {
            let Some(r) = ui.get_child_recursive(root, row) else {
                continue;
            };
            for arrow in [appearance::ARROW_PREV, appearance::ARROW_NEXT] {
                if let Some(h) = ui.get_child_recursive(r, arrow) {
                    ui.set_attribute_bool(h, dereth_ui::props::attr::DISABLED, disabled);
                }
            }
        }
        // And the Skin row slides up into the gap.
        if let Some(h) = ui.get_child_recursive(root, appearance::SKIN_ROW) {
            ui.move_to(
                h,
                0,
                if special {
                    SKIN_ROW_Y_SPECIAL
                } else {
                    SKIN_ROW_Y_ORDINARY
                },
            );
        }
    }

    // ---------------------------------------------------------------------------------------
    // The summary page
    // ---------------------------------------------------------------------------------------

    /// Update the character-generation summary page.
    ///
    /// Nothing while hidden. Otherwise it sets the per-heritage camera, fits the template to the
    /// character, writes the summary and how-to texts, updates the preview, gives the name box the
    /// focus, and — when no name has been entered — puts `ID_CharGen_NamePrompt` in the box and
    /// selects all of it.
    ///
    /// Taking focus is why a player can type a name the moment the page comes up without clicking
    /// the box first.
    ///
    /// **The update is a gesture, not a tick.** *"re-runs on every call"* is a claim about the
    /// body; how often it is **called** is a separate question, and the answer is not *"every
    /// frame"*. In retail it runs from exactly **three** places — the page initialisation (and only
    /// when visible), the progress-state write and the randomiser — and is never dispatched
    /// indirectly. The two live sites are the two below. A click into the box is none of the three,
    /// which is why retail leaves a plain caret there, as confirmed in a play check.
    pub fn summary_page_update(&mut self, ui: &mut UiSystem) {
        if let Some(t) = self.tables.clone() {
            self.state.fit_template_to_character(&t.chargen);
        }
        self.set_summary_text(ui);
        self.set_how_to_text(ui);
        self.refresh_view(ui);
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(name) = ui.get_child_recursive(root, NAME_FIELD) else {
            return;
        };
        ui.take_focus(name);
        if !self.name_entered {
            let prompt = self.string(ui, NAME_PROMPT);
            if let Some(t) = ui.text_element_mut(name) {
                t.set_text(&prompt);
                t.select_all();
            }
        }
    }

    /// The summary page's summary text write — the left-hand list.
    ///
    /// A flush, then, from the list box's own three templates:
    ///
    /// | template | element | what it is |
    /// |---|---|---|
    /// | 0 | `0x100002F8`, text `0x100002F9` | one plain line |
    /// | 1 | `0x100002FA`, text `0x100000FE` | a section header |
    /// | 2 | `0x100002FB`, text `0x100002FC` / `0x100002FD` | a name/value pair |
    ///
    /// four plain lines (profession, gender, heritage, starting town), an **Attributes** header,
    /// the ten pairs of [`SUMMARY_ROWS`], and then the four skill sections of
    /// [`SUMMARY_SKILL_SECTIONS`], each a header followed by one pair per skill at that level with
    /// its [`CharGenState::skill_score`].
    ///
    /// Skills are written in name order. This implementation keeps its own sorted list
    /// rather than reproducing a hash table's bucket traversal order.
    pub fn set_summary_text(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(list) = ui.get_child_recursive(root, summary_page::LIST) else {
            return;
        };
        let Some(tables) = self.tables.clone() else {
            return;
        };
        for h in std::mem::take(&mut self.summary_rows) {
            ui.remove_and_delete_root(h);
        }
        // The list's template list — the list-box reader, which is the same property `0x64` this
        // module documents in [`ATTR_TEMPLATE_LIST`]; the skills list is a [`ListBoxWidget`] and
        // uses the same reader.
        let templates = crate::panels::listbox::template_list(ui, list);
        let mut y = 0;
        let mut rows: Vec<ElemHandle> = Vec::new();
        let mut add = |ui: &mut UiSystem, template: usize, cells: &[(ElementId, String)]| {
            let Some((layout, element)) = templates.get(template).copied() else {
                return;
            };
            let Ok(h) = ui
                .require_env()
                .and_then(|e| e.create_child_element_by_data_id(ui, list, layout, element))
            else {
                return;
            };
            ui.move_to(h, 0, y);
            let b = ui.screen_box(h);
            y += b.y1 - b.y0 + 1;
            for (id, text) in cells {
                if let Some(t) = ui
                    .get_child_recursive(h, *id)
                    .and_then(|c| ui.text_element_mut(c))
                {
                    t.set_text(text);
                }
            }
            rows.push(h);
        };

        // The four plain lines, each with the client's own bounds check on the name array.
        let name = |arr: &[&str], i: i64| -> String {
            usize::try_from(i)
                .ok()
                .and_then(|i| arr.get(i))
                .map_or_else(String::new, |s| (*s).to_string())
        };
        let lines = [
            format!(
                "Profession: {}",
                name(&PROFESSION_NAMES, i64::from(self.state.template))
            ),
            format!(
                "Gender: {}",
                name(&GENDER_NAMES, i64::from(self.state.gender))
            ),
            format!(
                "Heritage: {}",
                name(&HERITAGE_NAMES, i64::from(self.state.heritage_group))
            ),
            format!(
                "Starting Town: {}",
                usize::try_from(self.state.start_area)
                    .ok()
                    .and_then(|i| tables.chargen.starter_areas.get(i))
                    .map_or("?", |area| area.name.as_str())
            ),
        ];
        for l in lines {
            add(ui, 0, &[(summary_page::LINE_TEXT, l)]);
        }
        add(
            ui,
            1,
            &[(summary_page::HEADER_TEXT, "Attributes".to_string())],
        );

        // The ten name/value pairs. The three vitals are the attribute-values update's own
        // endurance/2, endurance, self — attributes 2, 2 and 6.
        let endurance = self.state.get(Attr::Endurance);
        let values = [
            self.state.get(Attr::Strength),
            endurance,
            self.state.get(Attr::Coordination),
            self.state.get(Attr::Quickness),
            self.state.get(Attr::Focus),
            self.state.get(Attr::Self_),
            endurance / 2,
            endurance,
            self.state.get(Attr::Self_),
            self.state.remaining_skill_credits,
        ];
        for (label, v) in SUMMARY_ROWS.iter().zip(values) {
            add(
                ui,
                2,
                &[
                    (summary_page::PAIR_NAME, (*label).to_string()),
                    (summary_page::PAIR_VALUE, v.to_string()),
                ],
            );
        }

        // The four skill sections.
        let mut skills: Vec<(u32, String, i32)> = tables
            .skills
            .skills
            .iter()
            .map(|(id, b)| (*id, b.name.clone(), b.chargen_use))
            .collect();
        skills.sort_by(|a, b| a.1.cmp(&b.1));
        for (i, (header, level)) in SUMMARY_SKILL_SECTIONS.iter().enumerate() {
            add(ui, 1, &[(summary_page::HEADER_TEXT, (*header).to_string())]);
            for (id, sname, use_) in &skills {
                if self.state.skill_level(*id) != *level {
                    continue;
                }
                // The client's two arms: section 2 keeps `chargen_use < 2`, section 3 keeps the
                // rest. Sections 0 and 1 take everything at their level.
                if (i == 2 && *use_ >= 2) || (i == 3 && *use_ < 2) {
                    continue;
                }
                let score = self.state.skill_score(&tables.skills, *id);
                add(
                    ui,
                    2,
                    &[
                        (summary_page::PAIR_NAME, sname.clone()),
                        (summary_page::PAIR_VALUE, score.to_string()),
                    ],
                );
            }
        }
        self.summary_rows = rows;
    }

    /// The instruction pane, three runs concatenated:
    /// `ID_CharGen_SummaryHowTo`, the heritage-and-gender naming examples of
    /// [`SUMMARY_NAME_EXAMPLES`], and `ID_CharGen_SummaryHowToEnd`.
    ///
    /// The client appends each only if the string lookup succeeded **and** the string is not empty,
    /// and bails out of the whole chain on the first failure; the same order is kept here.
    pub fn set_how_to_text(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(h) = ui.get_child_recursive(root, summary_page::HOW_TO) else {
            return;
        };
        if let Some(own) = self.world_naming_text() {
            self.append_literal(ui, h, &own, 0, true);
            return;
        }
        let examples = SUMMARY_NAME_EXAMPLES
            .iter()
            .find(|(g, _, _)| *g == self.state.heritage_group)
            .map(|(_, male, female)| {
                if self.state.gender == 2 {
                    *female
                } else {
                    *male
                }
            });
        self.append_run(ui, h, SUMMARY_HOW_TO, 0, true);
        if let Some(t) = examples {
            self.append_run(ui, h, t, 0, false);
        }
        self.append_run(ui, h, SUMMARY_HOW_TO_END, 0, false);
    }

    /// The client's one write the page never repeats: the six attribute-name captions the char-gen
    /// state's attribute name read.
    ///
    /// The names are built into the client, not string-table tokens — "Strength", "Endurance",
    /// "Quickness", "Coordination", "Focus", "Self" — so the profession page is unlocalised in the
    /// same way the summary page is.
    pub fn initialize_profession_page(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        for (field, attr) in ATTRIBUTE_SLIDERS {
            let Some(f) = ui.get_child_recursive(root, field) else {
                continue;
            };
            let Some(h) = ui.get_child_recursive(f, slider::NAME) else {
                continue;
            };
            // **This is retail's own slip, reproduced rather than corrected.** The
            // page initialisation installs the number input filter on the attribute-name
            // **caption** and not on the editable number next to it:
            //
            // Find caption `0x100002ED`, install its numeric input filter, then find value
            // `0x100002EF` without installing a filter. The stored value-element member is
            // never read again. Finally write "Strength" on the same caption that got the filter.
            //
            // It looks like a mistake, and it is: the caption is not editable, so the filter never
            // fires. It is installed anyway, because a rebuild that "fixes" it would filter a field
            // retail does not filter, and because a layout change that made the caption editable
            // must diverge from retail here loudly rather than silently.
            dereth_ui::text::set_input_filter(ui, h, dereth_ui::text::number_input_filter);
            if let Some(t) = ui.text_element_mut(h) {
                t.set_text(attribute_name(attr));
            }
        }
    }

    /// The client's one line that is not a child binding: install the name input filter on the name
    /// box `0x10000402`.
    ///
    /// The filter is `isalpha(c)` for `c < 0x100` plus `'` (0x27), space (0x20) and
    /// `-` (0x2D) — so the character-name box refuses digits and every other punctuation mark,
    /// which is the one field in the whole client that refuses them. Without it a player could type
    /// `B0b!` into the box retail will not let them.
    pub fn initialize_summary_page(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(h) = ui.get_child_recursive(root, NAME_FIELD) else {
            return;
        };
        dereth_ui::text::set_input_filter(ui, h, dereth_ui::text::name_input_filter);
    }

    /// Re-read the setup and the animation set the current
    /// heritage names, and point the preview at whichever page's viewport is showing.
    ///
    /// **The update tail's object description is assembled host-side.**
    /// Two of its three stages need data resources (clothing table type `0x19` and
    /// palette-set type `0x18`), and the runtime descriptor the part array consumes is
    /// `dereth_animation::parts::ObjDesc`, which this crate does not depend on -- the same reason
    /// the colour wheel's [`CgColorSource`] is a seam. See
    /// `dereth_client::preview::chargen_objdesc`, which reproduces the block, and
    /// [`Cg3dView::bg_setup`], which is the half of it that does fit here.
    pub fn refresh_view(&mut self, _ui: &mut UiSystem) {
        self.view3d.initialize(&self.state);
        // The char-gen 3D view's initialise and update both take the setup from the char-gen
        // state's setup-id read, which is computed from the heritage's gender entry and the hair
        // style -- **not** from the stored setup id, which is only the heritage-or-gender-unchosen
        // fallback. `Cg3dView::initialize` has only the state, and `get_setup_id` needs the
        // char-gen table, so the setup is finished here. Without this, choosing Female leaves the
        // model male.
        if let Some(t) = self.tables.clone() {
            let s = self.state.get_setup_id(&t.chargen);
            self.view3d.setup = if s.0 == 0 { HUMAN_SETUP_ID } else { s };
            // The client's background-object block, whose id is the heritage's environment setup --
            // the room the model stands in. Without it the preview space holds one object and the
            // portrait stands in a void.
            self.view3d.bg_setup = t
                .chargen
                .heritage_groups
                .get(&self.state.heritage_group)
                .map_or(dereth_primitives::DataId(0), |hg| hg.environment_setup);
        }
        self.view3d.viewport = match self.progress {
            EcgProgress::Appearance => Some(appearance::VIEWPORT),
            EcgProgress::Summary => Some(SUMMARY_VIEWPORT),
            _ => None,
        };
        if self.progress == EcgProgress::Appearance {
            // The client's resting target for the current heritage: a heritage
            // change moves the camera even without a zoom press, because the three positions are
            // per heritage.
            self.view3d.camera_position = if self.view3d.zoomed_in {
                zoomed_in_camera(self.state.heritage_group)
            } else {
                zoomed_out_camera(self.state.heritage_group)
            };
        }
        if self.progress == EcgProgress::Summary {
            // The client's last call before the update starts the animation, so the summary model
            // is **animating** where the appearance page's Face tab holds its rest pose (Face ->
            // zoom in -> stop the animation). This is what the two pages actually ask the preview
            // for; the appearance page's own half of the pair -- the update's start-or-stop tail --
            // is [`Self::zoom`] and [`Self::appearance_update`]'s tail.
            self.view3d.animating = true;
            // The client's own three camera positions, which differ from the zoom-out ones only for
            // Olthoi.
            self.view3d.camera_position = match self.state.heritage_group {
                HERITAGE_OLTHOI => [0.0, 0.0, 0.0],
                HERITAGE_OLTHOI_ACID => [0.0, -5.7, 1.65],
                _ => [0.0, -2.5, 0.95],
            };
        }
    }

    /// The per-frame tick the client gives the preview.
    pub fn tick_preview(&mut self, dt: f64) {
        self.view3d.do_rotation(dt);
    }

    /// A `StringInfo` in table enum `0x10000002`, resolved if the host has the table.
    fn string(&self, ui: &UiSystem, token: &str) -> String {
        ui.resolve_string(
            ERROR_STRING_TABLE,
            dereth_primitives::num::hash::str_hash(token.as_bytes()),
        )
        .unwrap_or_else(|| token.to_string())
    }

    /// The slider index of the element's parent.
    ///
    /// **All six sliders carry the same four child ids**, so a message from `0x100002EC` or
    /// `0x100002EE` says nothing about which attribute it belongs to; the client resolves it from
    /// the element's parent, which is one of the six contiguous fields, and so does this.
    fn attribute_of_slider(&self, ui: &UiSystem, h: ElemHandle) -> Option<Attr> {
        let parent = ui.parent(h)?;
        let id = ui.node(parent)?.element_id();
        ATTRIBUTE_SLIDERS
            .iter()
            .find(|(f, _)| *f == id)
            .map(|(_, a)| *a)
    }

    /// The skill row a `+`/`-` click came from, by its parent's `0x1000000A` instance property —
    /// which is what the skill-level increase reads.
    fn row_of(&self, ui: &UiSystem, h: ElemHandle) -> Option<usize> {
        let parent = ui.parent(h)?;
        self.skill_rows
            .iter()
            .position(|r| r.element == Some(parent))
    }

    /// The current selection — the part row the selection write latches lit. It is the same element
    /// the switch assigns as the current selection, one per `EParts`, and [`APPEARANCE_ROWS`] is
    /// that mapping already.
    fn selection_row(part: EParts) -> Option<ElementId> {
        APPEARANCE_ROWS
            .iter()
            .find(|(_, p)| *p == part)
            .map(|(id, _)| *id)
    }

    /// The part a `0x1000030A` / `0x1000030B` arrow belongs to, by its parent row.
    fn part_of_arrow(&self, ui: &UiSystem, h: ElemHandle) -> Option<EParts> {
        let parent = ui.parent(h)?;
        let id = ui.node(parent)?.element_id();
        APPEARANCE_ROWS
            .iter()
            .find(|(r, _)| *r == id)
            .map(|(_, p)| *p)
    }

    /// The six dialog answers.
    pub fn close_dialog(&mut self, yes: bool) -> Option<UiMode> {
        let ctx = self.open_dialog.take()?;
        // The dialog factory has to be told, and the element deleted, and neither is possible from
        // here: the host answers the credit warning from `app.rs`'s `--create` drive with no
        // `UiSystem` at all. See [`Self::pending_close`].
        self.pending_close = Some(ctx);
        if !yes {
            return None;
        }
        match ctx {
            // The client's confirmation: back to character select.
            CharGenDialog::Exit => Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT),
            // "you still have unspent attribute credits — continue anyway?" re-enters the finish
            // with the credit check off.
            CharGenDialog::CreditWarning => {
                self.do_finish(false);
                None
            }
            // The randomize-warning close clears its context and rerolls on yes. There is no
            // `UiSystem` on this call, so the answer is latched and `Screen::update` runs
            // `do_random` on the next frame.
            CharGenDialog::RandomizeWarning => {
                self.pending_random = true;
                None
            }
            CharGenDialog::PleaseWait
            | CharGenDialog::ErrorMessage
            | CharGenDialog::ToDRequired => None,
        }
    }

    // -------------------------------------------------------------------------------------------
    // The wizard's dialogs, from the click to the answer
    //
    // The Exit confirmation and the other contexts the client can actually raise, using the same
    // dialog shape as character management. The moving parts are [`CharGenScreen::make_dialog`]
    // (the element), `service_dialogs` (raising recorded dialogs and taking down answered ones),
    // and `answer_dialog`, which handles the close-dialog notice.
    // -------------------------------------------------------------------------------------------

    /// The live element behind one of the six dialog contexts.
    #[must_use]
    pub const fn dialog_element(&self, ctx: CharGenDialog) -> Option<ElemHandle> {
        match ctx {
            CharGenDialog::Exit => self.exit_dialog,
            CharGenDialog::RandomizeWarning => self.randomize_warning_dialog,
            CharGenDialog::CreditWarning => self.credit_warning_dialog,
            CharGenDialog::ErrorMessage => self.error_message_dialog,
            CharGenDialog::ToDRequired => self.tod_warning_dialog,
            // The please-wait context is written only by the constructor and the destructor, both
            // with zero.
            CharGenDialog::PleaseWait => None,
        }
    }

    /// The factory context returned for a context, which is what the screen's context field
    /// actually holds. **`Some` with no [`Self::dialog_element`] is a dialog waiting its turn on
    /// queue 2** -- the state this screen could not represent at all before it used the factory.
    #[must_use]
    pub const fn dialog_context(&self, ctx: CharGenDialog) -> Option<u64> {
        self.dialog_queue.get(ctx)
    }

    fn dialog_slot(&mut self, ctx: CharGenDialog) -> Option<&mut Option<ElemHandle>> {
        Some(match ctx {
            CharGenDialog::Exit => &mut self.exit_dialog,
            CharGenDialog::RandomizeWarning => &mut self.randomize_warning_dialog,
            CharGenDialog::CreditWarning => &mut self.credit_warning_dialog,
            CharGenDialog::ErrorMessage => &mut self.error_message_dialog,
            CharGenDialog::ToDRequired => &mut self.tod_warning_dialog,
            CharGenDialog::PleaseWait => return None,
        })
    }

    /// The character-generation exit path:
    ///
    /// Refused when an exit warning is already recorded. Otherwise it builds a property set with
    /// `0x8E = 1` (confirmation), `0xAC = true` (modal) and `0xC5 =` the `ID_CharGen_ExitWarning`
    /// string from table `0x10000002`, creates the dialog in the current UI, and stores its
    /// context.
    ///
    /// Returns whether a dialog was raised, as the client's `bool` does.
    pub fn do_exit(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, EXIT_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::Exit, &text)
    }

    /// The randomize warning dialog build.
    ///
    /// The same shape as the exit warning: refused when already recorded, otherwise a modal
    /// confirmation (`0x8E = 1`, `0xAC = 1`) whose `0xC5` is `ID_CharGen_RandomizeWarning`, with
    /// its context stored.
    ///
    /// A yes answer runs character randomization.
    pub fn make_randomize_warning_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, RANDOMIZE_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::RandomizeWarning, &text)
    }

    /// The screen's credit warning dialog build — `0x8E = 1`, `0xAC = 1`, `0xC5 =
    /// ID_CharGen_CreditWarning`. *Yes* re-enters the finish with the credit check off.
    pub fn make_credit_warning_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, CREDIT_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::CreditWarning, &text)
    }

    /// The screen's error-message dialog build — `0x8E = 3` (`Message`, one button `0x26`). Its
    /// text is the `StringInfo` the caller supplied, which this build carries as
    /// [`CharGenScreen::error_string_id`] because none of the callers has a `UiSystem` to resolve
    /// one with.
    pub fn make_error_message_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let Some(token) = self.error_string_id else {
            return false;
        };
        let text = self.string(ui, token);
        self.make_dialog(ui, CharGenDialog::ErrorMessage, &text)
    }

    /// The screen's expansion warning dialog build — `0x8E = 3`, `0xC5 =
    /// ID_CharGen_ToDRequiredWarning`. Raised from element-message handling when an account without
    /// a *Throne of Destiny* takes a heritage or a start area that needs one.
    pub fn make_tod_warning_dialog(&mut self, ui: &mut UiSystem) -> bool {
        let text = self.string(ui, TOD_WARNING_STRING);
        self.make_dialog(ui, CharGenDialog::ToDRequired, &text)
    }

    /// Shared dialog creation: fill the `PropertyCollection` and hand it to
    /// the current UI's dialog factory, which allocates
    /// the global dialog context.
    ///
    /// **The `PropertyCollection` goes through the factory.** The factory lives on `UiSystem` --
    /// which is where the client keeps it, as statics on its own element-manager singleton -- so a
    /// [`Screen`] can reach it. Applying the properties straight to the element instead would make
    /// the five dialogs **stack** where retail queues them. See [`Self::service_dialog_queue`].
    ///
    /// Returns whether a dialog is **on screen** for this context afterwards, which is every dialog
    /// build's own `bool`: `false` for "one is already up" (the context-is-zero check each of them
    /// opens with), for a host with no dat behind it, and also for a dialog that was **queued**
    /// behind another on queue 2 -- it has a context and no element yet.
    fn make_dialog(&mut self, ui: &mut UiSystem, ctx: CharGenDialog, text: &str) -> bool {
        // The context-is-zero check, on the **context** and not on the element, which is what the
        // client tests: a dialog still waiting its turn on the queue has a context and no element,
        // and a second press must be refused then too.
        if self.dialog_queue.get(ctx).is_some() {
            return false;
        }
        let Some(kind) = ctx.kind() else { return false };
        // Recorded **before** the element exists, so a host with no environment installed still
        // sees the arm that was reached. Tests with no environment depend on this ordering.
        self.open_dialog = Some(ctx);
        // Each dialog path fills a `PropertyCollection` and hands it to the "make dialog in the
        // current UI" call. Property `0x8E` is the kind, `0xAC` is modality, and `0xC5` is the
        // prompt. **None of the five sets `0x8D` or `0xC3`**, so all five share queue **2** and a
        // second one waits behind the first rather than stacking on top of it, which is the whole
        // of what the queue is for.
        let mut data = dereth_ui::PropertyCollection::new();
        data.set(
            dereth_ui::props::attr::DIALOG_KIND,
            PropertyValue::Integer(kind.property()),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_MODAL,
            PropertyValue::Bool(true),
        );
        data.set(
            dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT,
            PropertyValue::String(text.to_string()),
        );
        let Some(context) = ui.dialogs.make_dialog(data, ui.now.0) else {
            return false;
        };
        if let Some(slot) = self.dialog_queue.slot(ctx) {
            *slot = Some(context);
        }
        // Dialog creation either built this context immediately or queued it behind the
        // one already open on queue 2. Building the element is this side's work either way; a
        // queued one is built on the frame its turn comes.
        self.service_dialog_queue(ui);
        self.dialog_element(ctx).is_some()
    }

    /// The client's **element** half, run for every context that the factory has opened for this
    /// screen and that has no element yet.
    ///
    /// `DialogController` lives on `UiSystem`, exactly as its statics live on the client's own
    /// singleton, so the queue is reachable from a [`Screen`] and the wizard's five dialogs queue
    /// as retail's do instead of stacking. This is the queue's second production caller. The order
    /// below is the client's: create the element for the kind's root, set its data, size and
    /// position it, bring it forward, then send the open-dialog notice -- the last two inside
    /// [`dereth_ui::UiSystem::bind_dialog_element`].
    fn service_dialog_queue(&mut self, ui: &mut UiSystem) {
        // The factory is asked which contexts are owed an element rather than being
        // second-guessed here; re-implementing the `element.is_none()` filter inline would be a
        // second copy of a rule that must not drift.
        let owed: Vec<u64> = ui
            .dialogs
            .pending_create()
            .into_iter()
            .map(|(c, _)| c)
            .collect();
        for ctx in CharGenDialog::RAISED {
            let Some(context) = self.dialog_queue.get(ctx) else {
                continue;
            };
            if !owed.contains(&context) {
                continue;
            }
            let Some(info) = ui.dialogs.info(context) else {
                continue;
            };
            let data = info.data.clone();
            let root = info.kind.root_element_id();
            let Ok(h) = ui
                .require_env()
                .and_then(|e| e.create_and_add_root_element(ui, DIALOG_LAYOUT, root))
            else {
                continue;
            };
            // The dialog's modal-property (`0xAC`) arm blocks clicks with mask `0x40`.
            ui.set_attribute_bool(
                h,
                dereth_ui::props::attr::DIALOG_MODAL,
                data.get_bool(dereth_ui::props::attr::DIALOG_MODAL)
                    .unwrap_or(false),
            );
            // The dialog's text update: property `0xC5` goes on child **`0x3E`**
            // (recursive lookup for `0x3E`); `0x3D` is the panel around it.
            if let Some(PropertyValue::String(text)) = data
                .get(dereth_ui::props::attr::DIALOG_COUNTDOWN_TEXT)
                .cloned()
            {
                if let Some(t) = ui
                    .get_child_recursive(h, dereth_ui::dialog::base::child::TEXT)
                    .and_then(|c| ui.text_element_mut(c))
                {
                    t.set_text(&text);
                }
            }
            // The per-dialog data application sets the button captions.
            dereth_ui::dialog::types::set_dialog_data(ui, h, &data);
            // The answer route, **by element id and not by bubbling**: `DialogElement`'s own
            // `listen_to_element_message` stops processing for the ids it recognises, so a listener
            // on the dialog root would never be reached. The same route character management and
            // the logout confirmation take.
            let (accept, cancel) = ctx.answer_children();
            for id in [accept, cancel].into_iter().flatten() {
                ui.register_for_element_message(id, MessageId(1), ME);
            }
            // Grow the shipped 400 x 95 panel to fit the prompt and centres it. Without it the
            // warning shows its first line in the top-left corner of the screen.
            dereth_ui::dialog::base::update_popup_size_and_position(ui, h);
            // Recorded as one of this screen's roots so use_new_mode deletes it with the screen,
            // which is the destructor calling the reset.
            self.roots.push(h);
            // Bring to front, then the open-dialog notice for the context.
            ui.bind_dialog_element(context, h);
            if let Some(slot) = self.dialog_slot(ctx) {
                *slot = Some(h);
            }
        }
    }

    /// The client's visible half: delete the element and forget it.
    fn close_dialog_element(&mut self, ui: &mut UiSystem, ctx: CharGenDialog) {
        let context = self.dialog_queue.slot(ctx).and_then(Option::take);
        let element = self.dialog_slot(ctx).and_then(Option::take);
        // The dialog factory's close frees the queue slot and, through
        // its "open next dialog", promotes whatever was waiting behind it. Told before the
        // element goes, so the promoted dialog is already current when the sweep below runs.
        if let Some(context) = context {
            ui.dialogs.close_dialog(context, ui.now.0);
        }
        if let Some(h) = element {
            for id in {
                let (a, c) = ctx.answer_children();
                [a, c]
            }
            .into_iter()
            .flatten()
            {
                ui.unregister_for_element_message(id, MessageId(1), ME);
            }
            self.roots.retain(|r| *r != h);
            ui.remove_and_delete_root(h);
        }
        // A dialog that was waiting behind this one is now the open one on queue 2 and is owed
        // an element. Without this a queued warning would never appear at all, which is the
        // failure mode a queue has and a stack does not.
        self.service_dialog_queue(ui);
    }

    /// Raise the element for a context that has been recorded and has none, and take down any
    /// element whose context is no longer the open one.
    ///
    /// The second half is what makes [`CharGenScreen::close_dialog`] safe to call **without** a
    /// `UiSystem`, which the host does from `app.rs`'s `--create` drive and which the
    /// verification-response handling does when a server reply dismisses the dialog that was
    /// waiting for it: `close_dialog` clears `open_dialog`, and the next frame with a `UiSystem` in
    /// it deletes the orphan.
    ///
    /// Called from both [`Screen::update`] and the tail of [`Screen::on_element_message`], for the
    /// same reason as on the character-management screen: the second is reached on a frame whose
    /// own element traffic recorded the context, and the first on a frame with no element traffic
    /// at all.
    fn service_dialogs(&mut self, ui: &mut UiSystem) {
        // An answer taken on a call that had no `UiSystem`, honoured on the first frame that
        // does. This also frees the factory's queue slot, so whatever was waiting behind it is
        // promoted and built below.
        if let Some(ctx) = self.pending_close.take() {
            self.close_dialog_element(ui, ctx);
        }
        // An element for a context the factory no longer has open -- a property-`0x8D` replacement,
        // or a close this screen did not make. The factory is the authority on which dialog is
        // current; guessing from `open_dialog` instead goes wrong the moment two are recorded at
        // once.
        for ctx in CharGenDialog::RAISED {
            if self.dialog_element(ctx).is_none() {
                continue;
            }
            let still_open = self
                .dialog_context(ctx)
                .is_some_and(|c| ui.dialogs.info(c).is_some());
            if !still_open {
                self.close_dialog_element(ui, ctx);
            }
        }
        // Anything the factory has made current and not yet given an element.
        self.service_dialog_queue(ui);
        let Some(ctx) = self.open_dialog else { return };
        if self.dialog_element(ctx).is_some() {
            return;
        }
        match ctx {
            CharGenDialog::Exit => self.do_exit(ui),
            CharGenDialog::RandomizeWarning => self.make_randomize_warning_dialog(ui),
            CharGenDialog::CreditWarning => self.make_credit_warning_dialog(ui),
            CharGenDialog::ErrorMessage => self.make_error_message_dialog(ui),
            CharGenDialog::ToDRequired => self.make_tod_warning_dialog(ui),
            // No character-generation dialog path raises one; see [`CharGenDialog::kind`].
            CharGenDialog::PleaseWait => false,
        };
    }

    /// Whether an element id is the *accept* half of a context's pair — the one statement of
    /// "which button is yes", taken from [`CharGenDialog::answer_children`] rather than repeated
    /// at the call site, so that transposing that table is a behaviour change.
    fn is_accept(ctx: CharGenDialog, id: ElementId) -> bool {
        ctx.answer_children().0 == Some(id)
    }

    /// The char-gen screen's close-dialog notice, reached from the dialog's own buttons
    /// rather than from a notice, because nothing in this build raises
    /// the UI close-dialog notice.
    ///
    /// The element goes first — that is the dialog close, which deletes it and *then* notifies —
    /// and the context is named explicitly rather than read off `open_dialog`, because the client
    /// demultiplexes on which of its six handles matched the incoming one.
    fn answer_dialog(&mut self, ui: &mut UiSystem, ctx: CharGenDialog, yes: bool) {
        self.close_dialog_element(ui, ctx);
        self.open_dialog = Some(ctx);
        if let Some(m) = self.close_dialog(yes) {
            ui.requests.emit(crate::view::UiRequest::QueueMode(m));
        }
        self.service_dialogs(ui);
    }
}

/// The char-gen state's "get colour from palette" and the averaging loop
/// the appearance page's selection change wraps it in — the seam that gets a colour out of
/// the dat and into the wheel.
///
/// The client asks for a `Palette` as resource type 10 and a `PalSet` as type `0x18`
/// wherever it needs one. This crate never reads the dat, so the host implements this and hands it
/// over on [`CharGenTables`]; it is **lazy** rather than a precomputed table because the set of
/// palettes a wheel needs depends on the style that is currently chosen.
pub trait CgColorSource: std::fmt::Debug {
    /// The ordered palette IDs in a `PalSet`. Barber Start needs the inverse of
    /// the native integral palette lookup: find the incoming concrete palette in this list and
    /// turn
    /// its one-based position back into the shade fraction.
    fn pal_set_palettes(&self, pal_set: DataId) -> Option<Vec<DataId>>;

    /// The colour of one palette entry — the palette's 32-bit colour read of one entry, ARGB.
    /// `None` when the palette is not in the dat, which the client answers with an error log line
    /// and a colour of **0**.
    fn palette_color(&self, palette: DataId, sample: PaletteSample) -> Option<u32>;

    /// The selection path's loop over a `PalSet`: for each palette in the set it reads the colour
    /// for `sample` and sums the red, green and blue bytes, then divides each sum by the palette
    /// count. So a spot is the **mean** of the whole set at one palette offset, per channel, with
    /// integer division. Alpha is not accumulated; the spot is opaque.
    fn pal_set_color(&self, pal_set: DataId, sample: PaletteSample) -> Option<u32>;
}

/// The four `UIASSET` images the colour wheel is drawn from, in enum order.
///
/// Enums `0x1000000D` … `0x10000010` in group **7**, the same group as
/// `portalspace_background` and `portalspace_animation`. Their names in the shipped mapper's own
/// `enum_to_name` table are `ColorBullet`, `ColorRing`, `ColorEmpty` and `GradientPlug`, and
/// **nothing here writes a DataID down**: the host resolves the four and hands them over.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ColorWheelArt {
    /// `0x1000000D` `ColorBullet` — the spot template the colour-spot step recolours.
    pub bullet: DataId,
    /// `0x1000000E` `ColorRing` — the shade disk the gradient-disk step multiplies by the current
    /// colour.
    pub ring: DataId,
    /// `0x1000000F` `ColorEmpty` — what a spot past the colour count shows instead.
    pub empty: DataId,
    /// `0x10000010` `GradientPlug` — what the disk shows while **Eyes** is the selected part,
    /// which is the one part with no shade.
    pub plug: DataId,
}

/// The nine colour spots and the shade disk — the colour-spot and gradient-disk steps.
impl CharGenScreen {
    /// The client's colour half: fill the colour wheel's red, green and blue for the part that has
    /// just been selected.
    ///
    /// Nine slots, of which only the first colour-count are filled; the rest keep whatever they had
    /// and are never shown, because the colour-spot step puts `ColorEmpty` in them.
    ///
    /// The colour count is written **here**, not during parts setup — which is why nose, mouth and
    /// skin end up with exactly **one** spot (the skin `PalSet`) although parts setup leaves them
    /// at zero.
    fn fill_color_wheel(&mut self) {
        self.color_wheel = [None; 9];
        let Some(t) = self.tables.clone() else { return };
        let Some(src) = t.colors.clone() else { return };
        let Some(sx) = self.state.sex(&t.chargen).cloned() else {
            return;
        };
        let part = self.current_part;
        let i = part.choice_index();
        let colors: Vec<Option<u32>> = match part {
            EParts::Hair => sx
                .hair_colors
                .iter()
                .map(|p| src.pal_set_color(DataId(*p), PaletteSample::Hair))
                .collect(),
            // Eyes read each palette directly, without averaging over a shade set.
            EParts::Eyes => sx
                .eye_colors
                .iter()
                .map(|p| src.palette_color(DataId(*p), PaletteSample::Eyes))
                .collect(),
            // All three arms set the count to 1: one spot, the heritage's own skin `PalSet`.
            EParts::Nose | EParts::Mouth | EParts::Skin => {
                vec![src.pal_set_color(sx.skin_palset, PaletteSample::Skin)]
            }
            EParts::Headgear => Self::gear_wheel(
                &*src,
                &self.state.headgear_pal_set_ids,
                PaletteSample::Headgear,
            ),
            EParts::Shirt => {
                Self::gear_wheel(&*src, &self.state.shirt_pal_set_ids, PaletteSample::Shirt)
            }
            EParts::Trousers => Self::gear_wheel(
                &*src,
                &self.state.trousers_pal_set_ids,
                PaletteSample::Trousers,
            ),
            EParts::Footwear => Self::gear_wheel(
                &*src,
                &self.state.footwear_pal_set_ids,
                PaletteSample::Footwear,
            ),
            EParts::Invalid => Vec::new(),
        };
        // The part's colour count is the list's length, and only the first nine can be shown.
        self.choices[i].num_colors = i32::try_from(colors.len()).unwrap_or(0);
        for (slot, c) in self.color_wheel.iter_mut().zip(colors) {
            *slot = c;
        }
    }

    fn gear_wheel(
        src: &dyn CgColorSource,
        ids: &[DataId],
        sample: PaletteSample,
    ) -> Vec<Option<u32>> {
        ids.iter().map(|p| src.pal_set_color(*p, sample)).collect()
    }

    /// The appearance page's color-spot step — **nine runtime-generated surfaces**.
    ///
    /// For each of the nine spots the client loads `ColorBullet` and `ColorEmpty`, sets the
    /// pointer's visibility, and makes a local surface the bullet's size the first time. A spot
    /// below the colour count gets the bullet blitted onto it with black replaced by the spot's
    /// colour; the others get `ColorEmpty`. The spot's image is then replaced with that surface.
    ///
    /// **It does not read a picture per spot out of the dat**, which is why the nine spots came up
    /// as nine black holes in this build: there is one template and nine derivations of it. See
    /// [`dereth_ui::region::SurfaceOp`].
    pub fn do_color_spots(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(t) = self.tables.clone() else { return };
        let art = t.color_wheel_art;
        let n = self.choices[self.current_part.choice_index()].num_colors;
        for (i, id) in appearance::COLOR_SPOTS.iter().enumerate() {
            let Some(h) = ui.get_child_recursive(root, *id) else {
                continue;
            };
            let lit = i32::try_from(i).is_ok_and(|i| i < n);
            let (did, op) = match (lit, self.color_wheel[i]) {
                (true, Some(c)) => (
                    art.bullet,
                    Some(dereth_ui::region::SurfaceOp::ReplaceColor {
                        from: dereth_ui::region::SurfaceOp::OPAQUE_BLACK,
                        to: c,
                    }),
                ),
                // A spot the part has no colour for -- and a lit spot whose palette would not
                // resolve, which the client also leaves un-recoloured rather than blank.
                _ => (art.empty, None),
            };
            set_derived_image(ui, h, did, op);
        }
    }

    /// The shade disk under the nine spots.
    ///
    /// For Eyes the gradient circle shows `GradientPlug`; otherwise it shows `ColorRing` multiplied
    /// by the current colour.
    ///
    /// `eyes` is true only when the selection write passes it, for **Eyes only** — the one part
    /// with no shade slider — and the plug is a flat cap rather than a gradient.
    pub fn do_grad_disk(&mut self, ui: &mut UiSystem, eyes: bool) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let Some(t) = self.tables.clone() else { return };
        let art = t.color_wheel_art;
        let Some(h) = ui.get_child_recursive(root, appearance::GRAD_CIRCLE) else {
            return;
        };
        if eyes {
            set_derived_image(ui, h, art.plug, None);
            return;
        }
        let c = usize::try_from(self.current_color)
            .ok()
            .and_then(|i| self.color_wheel.get(i).copied().flatten());
        let op = c.map(dereth_ui::region::SurfaceOp::Multiply);
        set_derived_image(ui, h, art.ring, op);
    }
}

/// Clear an element's image, then set the art `did` with the operation the picture is a derivation
/// under.
///
/// A `DataId` of 0 — an art id the host could not resolve — clears and puts nothing back, which is
/// the client's own missing-art guard around the whole body of the colour-spot step.
pub(crate) fn set_derived_image(
    ui: &mut UiSystem,
    h: ElemHandle,
    did: DataId,
    op: Option<dereth_ui::region::SurfaceOp>,
) {
    let Some(n) = ui.node_mut(h) else { return };
    if did.0 == 0 {
        n.region.image = None;
        return;
    }
    let mut g = dereth_ui::region::GraphicRef::opaque_surface(did, 0, 0);
    g.op = op;
    n.region.image = Some(g);
}
impl Screen for CharGenScreen {
    fn on_host_call(&mut self, _cx: &mut ScreenCx<'_>, call: &mut dyn std::any::Any) -> bool {
        use crate::screens::pregame_host::PregameCall;
        let Some(call) = call.downcast_mut::<PregameCall>() else {
            return false;
        };
        match call {
            PregameCall::CreditWarningOpen(out) => {
                *out = self.open_dialog == Some(CharGenDialog::CreditWarning);
            }
            PregameCall::CloseDialog(yes) => {
                let _ = self.close_dialog(*yes);
            }
            PregameCall::TickPreview(dt) => self.tick_preview(*dt),
            PregameCall::WizardPreview(out) => {
                *out = Some((self.view3d.clone(), self.state.clone(), self.tables.clone()));
            }
            PregameCall::PickCharacterRow { .. } => return false,
        }
        true
    }

    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        self.bound = bind_children(ui, root, CHROME);
        let mut pending = vec![root];
        while let Some(handle) = pending.pop() {
            pending.extend(ui.children(handle));
            let percentage = ui.node(handle).is_some_and(|node| {
                matches!(node.element_id(), slider::SCROLL | appearance::SHADE_SCROLL)
            });
            if let Some(bar) = ui
                .node_mut(handle)
                .and_then(|node| node.behaviour.as_mut())
                .and_then(|element| element.as_any_mut())
                .and_then(|element| {
                    element.downcast_mut::<dereth_ui::widgets::scrollbar::Scrollbar>()
                })
            {
                use dereth_ui::widgets::scrollbar::DirectWheel;
                bar.direct_wheel = Some(if percentage {
                    DirectWheel::Percentage
                } else {
                    DirectWheel::Content
                });
            }
        }
        // The client registers the framework on
        // the root, which is the only reason any of the wizard's buttons reach the switch below.
        ui.register_for_element_messages(root, ME);
        ui.register_for_global_message(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, ME);
        // The constructor's reset, which is what leaves the six attributes at fifty. Character
        // randomisation would follow it; see the module comment.
        if let Some(t) = self.tables.clone() {
            self.state.clothing = t.clothing.clone();
            self.state.reset(&t.chargen, &t.skills);
        }
        // The six slider captions, written once.
        self.initialize_profession_page(ui);
        // The name box's name input filter. The wizard's five pages are one screen
        // here, so the summary page's own page initialisation runs from the same place the
        // profession page's does.
        self.initialize_summary_page(ui);
        // The constructor ends by moving to the heritage page.
        self.set_progress_state(ui, EcgProgress::Hertage);
        Ok(())
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        let ui = &mut *cx.ui;
        // The name box reports **0x44** (text changed) and **0x12** (character typed), not 1.
        if m.source_id == NAME_FIELD
            && (m.id == MessageId(0x44) || m.id == dereth_ui::msg::element::id::CHARACTER)
        {
            self.name_changed(ui, m.source);
            return;
        }
        // The scrollbars: `0x100002EE` is an attribute slider and `0x10000321` the shade wheel,
        // and both report **10** (the scrollbar's position change) with the new position
        // in `p1`.
        if m.id == dereth_ui::msg::element::id::SCROLL_POSITION {
            if m.source_id == slider::SCROLL {
                if let Some(a) = self.attribute_of_slider(ui, m.source) {
                    // Truncate the scrollbar's float, then the client's own floor of 10.
                    //
                    // **The scale.** `p1` of message `0x0A` is `position x 1000`
                    // (the shade arm's `p1 * 0.001` is the oracle), and the attribute-values update
                    // writes the bar back with `value * 0.01` — so the value is `p1 * 0.1`, and an
                    // attribute runs 10..100 rather than 0..1000.
                    #[allow(clippy::cast_precision_loss)]
                    let v = to_i32(m.p1 as f32 * 0.1);
                    self.set_attrib_value(ui, a, v.max(ATTR_FLOOR));
                }
                return;
            }
            if m.source_id == appearance::SHADE_SCROLL {
                // The appearance page's element-message handler's own `0x10000321` arm: `clamp(p1 *
                // 0.001, 0.0, 1.0)`, with `p1` unsigned. Not `* 0.01`, which is off by a factor of
                // ten.
                self.set_shade(ui, (f64::from(m.p1) * 0.001).clamp(0.0, 1.0));
                return;
            }
        }
        if m.id != dereth_ui::msg::element::id::BUTTON_CLICKED {
            return;
        }
        if let Some((_, h)) = HERITAGE_BUTTONS.iter().find(|(id, _)| *id == m.source_id) {
            self.choose_heritage(*h);
            self.set_progress_state(ui, self.progress);
            return;
        }
        // The profession page.
        if let Some((_, t, _)) = PROFESSION_BUTTONS
            .iter()
            .find(|(id, _, _)| *id == m.source_id)
        {
            self.choose_profession(ui, *t);
            return;
        }
        if m.source_id == slider::LOCK {
            if let Some(a) = self.attribute_of_slider(ui, m.source) {
                self.set_lock(ui, a);
            }
            return;
        }
        // The skills page. Both buttons live inside a row and are identified by their parent, the
        // same way the profession page's lock is.
        if m.source_id == skills_page::ROW_INCREASE || m.source_id == skills_page::ROW_DECREASE {
            if let Some(i) = self.row_of(ui, m.source) {
                if m.source_id == skills_page::ROW_INCREASE {
                    self.increase_skill_level(ui, i);
                } else {
                    self.decrease_skill_level(ui, i);
                }
            }
            return;
        }
        // The appearance page.
        if m.source_id == appearance::ARROW_PREV || m.source_id == appearance::ARROW_NEXT {
            if let Some(part) = self.part_of_arrow(ui, m.source) {
                let d = if m.source_id == appearance::ARROW_PREV {
                    -1
                } else {
                    1
                };
                self.cycle_part(ui, part, d);
            }
            return;
        }
        if let Some((_, part)) = APPEARANCE_ROWS.iter().find(|(id, _)| *id == m.source_id) {
            self.set_selection(ui, *part);
            return;
        }
        if let Some(i) = appearance::COLOR_SPOTS
            .iter()
            .position(|id| *id == m.source_id)
        {
            self.set_color(ui, i32::try_from(i).unwrap_or(0));
            return;
        }
        match m.source_id {
            appearance::GENDER_FEMALE => {
                self.choose_gender(ui, 2);
                return;
            }
            appearance::GENDER_MALE => {
                self.choose_gender(ui, 1);
                return;
            }
            appearance::TAB_FACE => {
                self.set_choice(ui, true);
                self.set_selection(ui, EParts::Hair);
                return;
            }
            appearance::TAB_CLOTHES => {
                self.set_choice(ui, false);
                self.set_selection(ui, EParts::Headgear);
                return;
            }
            appearance::ROTATE_CW => {
                self.view3d.rotate(ERotateDirection::Clockwise);
                return;
            }
            appearance::ROTATE_CCW => {
                self.view3d.rotate(ERotateDirection::CounterClockwise);
                return;
            }
            appearance::ZOOM_IN => {
                self.zoom(ui, true);
                return;
            }
            appearance::ZOOM_OUT => {
                self.zoom(ui, false);
                return;
            }
            _ => {}
        }
        if let Some((_, a)) = TOWN_BUTTONS.iter().find(|(id, _)| *id == m.source_id) {
            self.choose_map_town(*a);
            self.town_page_update(ui);
            return;
        }
        if let Some(area) = m.source_id.0.checked_sub(WORLD_TOWN_BUTTON_BASE) {
            if self.uses_world_town_list()
                && self.tables.as_ref().is_some_and(|tables| {
                    usize::try_from(area).is_ok_and(|i| i < tables.chargen.starter_areas.len())
                })
            {
                self.choose_town(area);
                self.town_page_update(ui);
                return;
            }
        }
        match m.source_id {
            // `0x100003C6` — **and the left arrow on page 1 is Exit**, not "back": past page 1 it
            // goes back a page, on page 1 it runs the exit step.
            LEFT_BUTTON => self.previous_page(ui),
            RIGHT_BUTTON => self.next_page(ui),
            // `0x100003CA` — the exit step, the same call the left arrow makes on page 1.
            EXIT_BUTTON => {
                self.do_exit(ui);
            }
            // `0x100003CB` — the warning is the **summary page's** arm only; on the other five the
            // button re-rolls that page straight away.
            //
            // The summary arm builds
            // the warning element, and answering it yes is [`Self::pending_random`], which reaches
            // the sixth arm of `do_random`.
            RANDOM_BUTTON => {
                if self.progress == EcgProgress::Summary {
                    self.make_randomize_warning_dialog(ui);
                } else {
                    self.do_random(ui);
                }
            }
            // Finish: the finish with the credit check on, which puts up its own dialog on every
            // refusal. The client gates it on the summary page, where the button is the only thing
            // that is visible.
            FINISH_BUTTON => {
                if self.progress == EcgProgress::Summary {
                    self.do_finish(true);
                }
            }
            // ---- the dialogs' own buttons, registered by element id in `make_dialog` -------
            //
            // Each arm is guarded on the dialog that owns those ids actually being up, because the
            // registration is by **element id** and therefore global: a stale `0x17` from anywhere
            // else must not be read as an answer to a confirmation that is not on screen. The
            // accept/cancel decision comes from [`CharGenDialog::answer_children`] and is not
            // repeated here.
            //
            // The confirmation dialog: **button 1 is yes** (`id == 0x17` is stored into property
            // 0x92). Three of this screen's dialogs are confirmations, and at most one can be up at
            // a time because every one of them is modal.
            ElementId(0x17) | ElementId(0x19) => {
                if let Some(ctx) = [
                    CharGenDialog::Exit,
                    CharGenDialog::CreditWarning,
                    CharGenDialog::RandomizeWarning,
                ]
                .into_iter()
                .find(|c| self.dialog_element(*c).is_some())
                {
                    let yes = Self::is_accept(ctx, m.source_id);
                    self.answer_dialog(ui, ctx, yes);
                }
            }
            // The message dialog: its one button just closes. The dialog-close handler's
            // kind-3 arm clears the context and does nothing else, so the `yes` it is answered
            // with cannot matter — it is taken from the table anyway rather than assumed.
            MESSAGE_BUTTON => {
                if let Some(ctx) = [CharGenDialog::ErrorMessage, CharGenDialog::ToDRequired]
                    .into_iter()
                    .find(|c| self.dialog_element(*c).is_some())
                {
                    if ctx == CharGenDialog::ErrorMessage {
                        self.error_string_id = None;
                    }
                    let yes = Self::is_accept(ctx, m.source_id);
                    self.answer_dialog(ui, ctx, yes);
                }
            }
            other => {
                if let Some(p) = EcgProgress::PAGES
                    .iter()
                    .find(|p| p.select_button() == Some(other))
                {
                    self.set_progress_state(ui, *p);
                }
            }
        }
        // Dialog requests made from functions this build cannot hand a `UiSystem` to —
        // the finish credit warning, name-limit error, and two expansion warnings —
        // record their context and are raised here, on the same frame. See
        // [`Self::service_dialogs`] for why this and `update` are individually unfalsifiable and
        // jointly not.
        self.service_dialogs(ui);
    }

    /// The per-frame screen update. The client answers a dialog from the
    /// close-dialog notice, which the dialog factory raises and nothing in this build raises yet —
    /// so the wizard reads the answer off its own dialog element here instead, once per frame,
    /// which is the same place the client's per-frame tick would have run.
    fn update(
        &mut self,
        cx: &mut ScreenCx<'_>,
        _now: dereth_primitives::LocalTime,
    ) -> Option<UiMode> {
        let ui = &mut *cx.ui;
        self.service_dialogs(ui);
        // The client's tail, deferred one frame so that it has a
        // `UiSystem` to repaint the summary page with.
        if std::mem::take(&mut self.pending_random) {
            self.do_random(ui);
        }
        // The appearance page's initialisation, which the constructor runs while it builds the
        // pages -- i.e. **before** its own progress-state write, which is why this sits above
        // `pending_refresh` and not below it.
        if std::mem::take(&mut self.pending_initialize_appearance) {
            self.initialize_appearance_page(ui);
        }
        // The constructor's own move to the heritage page, re-run once the rolled character exists
        // -- see [`Self::pending_refresh`].
        if std::mem::take(&mut self.pending_refresh) {
            let p = self.progress;
            self.set_progress_state(ui, p);
        }
        // No re-measure here. The wizard's three description panes are filled by a text write long
        // after post-init measured them empty; retail's draw recalculates the glyph list on every
        // visible region, and `UiSystem::draw` runs that sweep from the root.
        None
    }

    /// What the wizard owes the session, in order, to the host's request drain.
    fn flush_to_host(&mut self, cx: &mut ScreenCx<'_>) {
        for a in self.take_actions() {
            cx.ui
                .requests
                .emit(crate::view::UiRequest::CharGenAction(a));
        }
    }

    fn on_pregame(
        &mut self,
        cx: &mut ScreenCx<'_>,
        p: &dereth_ui::framework::PregameCx<'_>,
    ) -> Option<UiMode> {
        let host = p.view;
        // **Before `set_tables`, not after.** The original constructor reads the expansion flag
        // and hands it straight to the heritage chooser, which rolls one die of `tod ? 4 : 3` for
        // the heritage; with the flag still false the Viamontian
        // heritage could never come up on the opening roll of a Throne of Destiny account.
        self.account_has_tod = host.account_has_tod;
        if self.tables.is_none() {
            // `set_tables` and not a field write: the wizard's constructor initializes its
            // first page from the tables, and the screen is
            // created before the host has handed them over.
            if let Some(t) = p
                .tables
                .clone()
                .and_then(|t| Rc::downcast::<CharGenTables>(t).ok())
            {
                // The client generator's seed and the C runtime's seed are the two
                // seeds the roll comes out of. See
                // [`dereth_chargen::CharGenRng`].
                if let Some((ran2, crt)) = host.chargen_seeds {
                    self.state.rng = dereth_chargen::CharGenRng::new(ran2, crt);
                }
                self.set_tables(t);
            }
        }
        // The slot the creation request carries is the char-gen state's, which lives with the
        // player session and not with this screen: the character screen's selection wrote it.
        self.state.set_slot(p.chargen_slot);
        // The char-gen verification response is a notice: applied on the edge.
        if let (Some(code), true) = (host.chargen_response, p.chargen_response_changed) {
            self.on_chargen_verification_response(code);
            // A refusal clears the slot in the session's state as well as in this copy.
            if self.state.slot != p.chargen_slot {
                cx.ui
                    .requests
                    .emit(crate::view::UiRequest::CharGenSlot(self.state.slot));
            }
        }
        // The persistent character-set notice followed by the UI-flow update reaches
        // the screen's awaiting-character-set-for-login branch, which logs
        // the newly created character straight in without a visit to character select.
        if p.received_set && p.char_set_changed {
            return self.character_set_arrived(p.char_set);
        }
        None
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }

    /// The wizard stops listening for unconsumed keys when it goes. The flow hands every queued
    /// external delivery to whichever screen is current, so a registration left behind here would
    /// give the next screen each key press twice: the game screen would fire a shortcut on the
    /// press and again on its echo.
    fn destroy(&mut self, cx: &mut ScreenCx<'_>) {
        cx.ui
            .unregister_for_global_message(dereth_ui::msg::global::KEY_DOWN_UNCONSUMED, ME);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the screen catalogue's four tables, and the layout-enum map (enum
    /// `0x10000039` → `0x21000038` `chargen_master`, root `0x100003CC`).
    #[test]
    fn the_wizard_names_the_documented_layout_chrome_pages_and_buttons() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0039));
        assert_eq!(ROOT, ElementId(0x1000_03CC));
        assert_eq!(CHROME.len(), 9);
        // The six pages: contiguous ids 0x100003D1..0x100003D6 against types 0x10000039..0x1000003E.
        for (i, p) in EcgProgress::PAGES.iter().enumerate() {
            let (id, ty) = p.page().unwrap();
            let i = u32::try_from(i).unwrap();
            assert_eq!(id, ElementId(0x1000_03D1 + i), "{p:?} element id");
            assert_eq!(ty, ElementType(0x1000_0039 + i), "{p:?} element type");
            assert_eq!(p.select_button(), Some(ElementId(0x1000_03EF + i)));
        }
        assert_eq!(EcgProgress::Invalid.page(), None);
        assert_eq!(EcgProgress::Invalid.select_button(), None);
    }

    /// the values, including the heritage typo the document
    /// flags with *(sic)*.
    #[test]
    fn the_progress_enum_has_the_documented_values() {
        assert_eq!(EcgProgress::Invalid as i32, 0);
        assert_eq!(EcgProgress::Hertage as i32, 1);
        assert_eq!(EcgProgress::Profession as i32, 2);
        assert_eq!(EcgProgress::Skills as i32, 3);
        assert_eq!(EcgProgress::Appearance as i32, 4);
        assert_eq!(EcgProgress::Town as i32, 5);
        assert_eq!(EcgProgress::Summary as i32, 6);
    }

    /// Oracle: the progress-state write — one page visible, the arrows enabled by position, and the
    /// constructor ending on the heritage page.
    #[test]
    fn walking_all_six_pages_shows_one_at_a_time_and_bounds_the_arrows() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CharGenScreen::default();
        s.set_progress_state(&mut ui, EcgProgress::Hertage);
        assert_eq!(s.visible_page, Some(ElementId(0x1000_03D1)));
        assert!(!s.left_enabled, "nothing before the first page");
        assert!(s.right_enabled);

        for expected in &EcgProgress::PAGES[1..] {
            s.next_page(&mut ui);
            assert_eq!(s.progress, *expected);
            assert_eq!(s.visible_page, expected.page().map(|(i, _)| i));
        }
        assert_eq!(s.progress, EcgProgress::Summary);
        assert!(s.left_enabled);
        assert!(!s.right_enabled, "nothing after the last page");
        s.next_page(&mut ui);
        assert_eq!(
            s.progress,
            EcgProgress::Summary,
            "the right arrow stops at the summary"
        );

        for expected in EcgProgress::PAGES.iter().rev().skip(1) {
            s.previous_page(&mut ui);
            assert_eq!(s.progress, *expected);
        }
        s.previous_page(&mut ui);
        assert_eq!(
            s.progress,
            EcgProgress::Hertage,
            "the left arrow stops at heritage"
        );
    }

    /// A stand-in for the two dat tables, with one heritage and two templates.
    ///
    /// The *shape* is the retail one — `attribute_credits` 330 and `skill_credits` 52 are what
    /// every non-Olthoi heritage carries — but the numbers here are this test's, so nothing below
    /// asserts fidelity against them. The fidelity assertions are in
    /// [`dereth_chargen`], against retail's behaviour.
    fn tables() -> Rc<CharGenTables> {
        use dereth_assets::tables::{HeritageGroup, SexCg};
        use std::collections::BTreeMap;
        let mut sexes = BTreeMap::new();
        sexes.insert(
            1u32,
            SexCg {
                naming_help: None,
                name: "Female".into(),
                scale: 100,
                setup: dereth_primitives::DataId(0),
                sound_table: dereth_primitives::DataId(0),
                icon: 0,
                base_palette: dereth_primitives::DataId(0),
                skin_palset: dereth_primitives::DataId(0),
                physics_table: dereth_primitives::DataId(0),
                motion_table: dereth_primitives::DataId(0),
                combat_table: dereth_primitives::DataId(0),
                base_objdesc: dereth_assets::tables::ObjDesc {
                    version: 11,
                    palette: None,
                    subpalettes: Vec::new(),
                    texture_changes: Vec::new(),
                    anim_part_changes: Vec::new(),
                },
                hair_colors: Vec::new(),
                hair_styles: Vec::new(),
                eye_colors: Vec::new(),
                eye_strips: Vec::new(),
                nose_strips: Vec::new(),
                mouth_strips: Vec::new(),
                headgear: Vec::new(),
                shirts: Vec::new(),
                pants: Vec::new(),
                footwear: Vec::new(),
                clothing_colors: Vec::new(),
            },
        );
        let mut heritage_groups = BTreeMap::new();
        for id in [1u32, TOD_HERITAGE, HERITAGE_OLTHOI] {
            heritage_groups.insert(
                id,
                HeritageGroup {
                    description: None,
                    name: format!("H{id}"),
                    icon: 0,
                    setup: dereth_primitives::DataId(0),
                    environment_setup: dereth_primitives::DataId(0),
                    attribute_credits: 330,
                    skill_credits: 52,
                    primary_start_areas: Vec::new(),
                    secondary_start_areas: Vec::new(),
                    skills: Vec::new(),
                    templates: Vec::new(),
                    sex_table_marker: 0,
                    sex_order: sexes.keys().copied().collect(),
                    template_presentations: BTreeMap::new(),
                    sexes: sexes.clone(),
                },
            );
        }
        Rc::new(CharGenTables {
            texts: dereth_chargen::CreationTexts::default(),
            world: Rc::new(dereth_chargen::CreationTables {
                chargen: CharGen {
                    help_strings: vec![],
                    id: dereth_primitives::DataId(0x0E00_0002),
                    second_data_id: dereth_primitives::DataId(0),
                    starter_areas: TOWN_NAMES
                        .iter()
                        .map(|name| dereth_assets::tables::StarterArea {
                            name: (*name).to_string(),
                            locations: Vec::new(),
                        })
                        .collect(),
                    hg_table_marker: 0,
                    heritage_order: heritage_groups.keys().copied().collect(),
                    heritage_groups,
                },
                skills: SkillTable {
                    id: dereth_primitives::DataId(0x0E00_0004),
                    buckets: 0,
                    skills: BTreeMap::new(),
                },
                clothing: Rc::new(BTreeMap::new()),
            }),
            // No dat behind this fixture, so the wheel has no art and no colours -- which is
            // exactly the Get returned null case the client also draws as nothing.
            color_wheel_art: ColorWheelArt::default(),
            colors: None,
        })
    }

    fn wizard() -> CharGenScreen {
        CharGenScreen {
            tables: Some(tables()),
            account_has_tod: true,
            ..CharGenScreen::default()
        }
    }

    /// Oracle: the client's switch, read case by case. The ids are neither contiguous nor ordered
    /// by heritage number, so this is a table copied case by case and not a pattern.
    #[test]
    fn the_thirteen_heritage_buttons_are_the_switch_cases() {
        let want: [(u32, u32); 13] = [
            (0x1000_03BF, 1),
            (0x1000_03C1, 2),
            (0x1000_03C2, 3),
            (0x1000_03C3, 4),
            (0x1000_0590, 5),
            (0x1000_05A9, 6),
            (0x1000_05E8, 7),
            (0x1000_05F1, 8),
            (0x1000_05C4, 9),
            (0x1000_0591, 10),
            (0x1000_05BF, 11),
            (0x1000_05C7, 12),
            (0x1000_05C8, 13),
        ];
        let got: Vec<(u32, u32)> = HERITAGE_BUTTONS.iter().map(|(e, h)| (e.0, *h)).collect();
        assert_eq!(got, want.to_vec());
        // Every heritage 1..=13 exactly once, which is what makes the table checkable.
        let mut hs: Vec<u32> = HERITAGE_BUTTONS.iter().map(|(_, h)| *h).collect();
        hs.sort_unstable();
        assert_eq!(hs, (1..=13).collect::<Vec<u32>>());
    }

    /// Oracle: the town write (`start area = town - Holtburg`, and the four town cases highlight
    /// Holt, Shoushi, Yaraq, Sanamar in that order) and the switch's literal start area 3 for
    /// Sanamar.
    #[test]
    fn the_four_town_buttons_map_to_the_documented_start_areas() {
        let got: Vec<(u32, u32)> = TOWN_BUTTONS.iter().map(|(e, a)| (e.0, *a)).collect();
        assert_eq!(
            got,
            vec![
                (0x1000_040D, 0),
                (0x1000_040F, 1),
                (0x1000_040E, 2),
                (0x1000_040B, 3)
            ]
        );
        assert_eq!(TOD_START_AREA, 3, "Sanamar is the Throne of Destiny town");
    }

    /// Oracle: the two expansion-entitlement guards — the heritage page's, on Viamontian
    /// only, and the town page's, on Sanamar only. Both raise a warning
    /// and leave state unchanged.
    #[test]
    fn the_expansion_gates_refuse_and_change_nothing() {
        let mut s = CharGenScreen {
            tables: Some(tables()),
            ..CharGenScreen::default()
        };
        assert!(!s.account_has_tod);
        s.choose_heritage(TOD_HERITAGE);
        assert_eq!(s.open_dialog, Some(CharGenDialog::ToDRequired));
        assert_eq!(s.state.heritage_group, 0, "the heritage was not taken");
        s.open_dialog = None;
        s.choose_town(TOD_START_AREA);
        assert_eq!(s.open_dialog, Some(CharGenDialog::ToDRequired));
        assert_eq!(s.state.start_area, 0);
        // The other twelve heritages and the other three towns fall straight through.
        s.open_dialog = None;
        s.choose_heritage(1);
        assert_eq!(s.state.heritage_group, 1);
        assert_eq!(s.state.gender, 1);
        assert_eq!(s.state.total_atrb_credits, 330);
        assert_eq!(s.state.total_skill_credits, 52);
        s.choose_town(2);
        assert_eq!(s.state.start_area, 2);
        assert_eq!(s.open_dialog, None);
        // With the expansion, Viamontian is allowed.
        s.account_has_tod = true;
        s.choose_heritage(TOD_HERITAGE);
        assert_eq!(s.state.heritage_group, TOD_HERITAGE);
    }

    /// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
    #[test]
    fn heritage_buttons_refuse_every_key_missing_from_the_world() {
        let mut t = tables();
        let world = Rc::get_mut(&mut Rc::get_mut(&mut t).unwrap().world).unwrap();
        let human = world.chargen.heritage_groups[&1].clone();
        world.chargen.heritage_groups.clear();
        for key in 1..=3 {
            world.chargen.heritage_groups.insert(key, human.clone());
        }
        world.chargen.heritage_order = vec![1, 2, 3];
        let mut s = CharGenScreen {
            tables: Some(t),
            account_has_tod: true,
            ..CharGenScreen::default()
        };
        for available in 1..=3 {
            s.open_dialog = None;
            s.choose_heritage(available);
            assert_eq!(s.state.heritage_group, available);
            assert_eq!(s.open_dialog, None);
            for unavailable in 4..=13 {
                s.choose_heritage(unavailable);
                assert_eq!(s.state.heritage_group, available);
                assert_eq!(s.state.gender, 1);
                assert_eq!(s.open_dialog, Some(CharGenDialog::ToDRequired));
                s.open_dialog = None;
            }
        }
    }

    /// Behaviour: chargen.tables.world-keys-and-costs-remain-authoritative
    #[test]
    fn town_buttons_map_geographic_names_to_the_world_area_indices() {
        let mut t = tables();
        let world = Rc::get_mut(&mut Rc::get_mut(&mut t).unwrap().world).unwrap();
        let names = [
            "Holtburg South",
            "Holtburg West",
            "Shoushi Southeast",
            "Shoushi West",
            "Yaraq North",
            "Yaraq East",
        ];
        world.chargen.starter_areas = names
            .iter()
            .map(|name| dereth_assets::tables::StarterArea {
                name: (*name).to_string(),
                locations: Vec::new(),
            })
            .collect();
        let mut s = CharGenScreen {
            tables: Some(t),
            ..CharGenScreen::default()
        };
        assert!(s.uses_world_town_list());
        for (map, area) in [(0, 0), (1, 2), (2, 4)] {
            s.choose_map_town(map);
            assert_eq!(s.state.start_area, area);
            assert_eq!(s.open_dialog, None);
        }
        for area in 0..6 {
            s.choose_town(area);
            assert_eq!(s.state.start_area, area);
            assert_eq!(
                s.open_dialog, None,
                "Shoushi West does not require expansion access"
            );
        }
        s.choose_map_town(3);
        s.choose_town(6);
        assert_eq!(
            s.state.start_area, 5,
            "absent towns never replace a valid selection"
        );
        assert!(!wizard().uses_world_town_list());
    }

    /// Oracle: the finish step, line for line — the no-name refusal, the credit warning and the
    /// already-sent double-send guard.
    #[test]
    fn finishing_refuses_without_a_name_warns_about_credits_and_only_sends_once() {
        let mut s = wizard();
        s.choose_heritage(1);
        s.choose_town(0);

        // No name: the dialog, and nothing sent.
        assert!(!s.do_finish(true));
        assert_eq!(s.error_string_id, Some("ID_CharGen_NoNameWarning"));
        assert!(s.take_actions().is_empty());

        // A name, but the default template leaves 30 of the 330 credits unspent.
        s.name_entered = true;
        assert!(s.state.set_name("Kupo"));
        assert!(s.state.remaining_atrb_credits > 0);
        assert!(!s.do_finish(true));
        assert_eq!(s.open_dialog, Some(CharGenDialog::CreditWarning));
        assert!(
            s.take_actions().is_empty(),
            "the warning is asked before anything is sent"
        );

        // Answering "yes" re-enters the finish with the credit check off, which sends.
        assert_eq!(s.close_dialog(true), None);
        let sent = s.take_actions();
        assert_eq!(sent.len(), 1);
        let CharGenAction::SendCharGenResult(r) = &sent[0] else {
            panic!("not a create")
        };
        assert_eq!(r.name, "Kupo");
        assert_eq!(r.heritage_group, 1);
        assert_eq!(r.start_area, 0);
        assert_eq!(r.skill_advancement_classes.len(), 55, "the wire contract");
        assert_eq!(s.state.verification, CgVerification::Pending);

        // Pressing Finish again while the answer is outstanding sends nothing.
        assert!(!s.do_finish(false));
        assert!(s.take_actions().is_empty());
    }

    /// Oracle: the char-gen verification-response notice's table, and the client's
    /// awaiting-character-set branch — "a successful creation logs the new character in without a
    /// visit to the character-select screen".
    #[test]
    fn a_successful_creation_logs_the_new_character_straight_in() {
        use dereth_ui::persist::CharacterIdentity;
        let mut s = wizard();
        s.choose_heritage(1);
        s.name_entered = true;
        s.state.set_name("Kupo");
        assert!(s.do_finish(false));
        s.take_actions();

        // A failure keeps the wizard on the summary page and re-arms Finish.
        s.on_chargen_verification_response(3);
        assert_eq!(s.open_dialog, Some(CharGenDialog::ErrorMessage));
        assert_eq!(s.error_string_id, Some("ID_Character_Err_NameReserved"));
        assert!(!s.awaiting_char_set_for_login);
        assert_eq!(
            s.state.verification,
            CgVerification::Undef,
            "Finish can be pressed again"
        );

        // Success arms the log-in, and the next character set carries it out.
        assert!(s.do_finish(false));
        s.take_actions();
        s.on_chargen_verification_response(1);
        assert!(s.awaiting_char_set_for_login);
        assert_eq!(s.open_dialog, None, "the please-wait dialog is closed");

        let set = CharacterSet {
            set: vec![
                CharacterIdentity {
                    id: dereth_primitives::ObjectId(0x5000_0001),
                    name: "Someone".into(),
                    seconds_grace_period: 0,
                },
                CharacterIdentity {
                    id: dereth_primitives::ObjectId(0x5000_0002),
                    // ACE plusses every character on a privileged account, so the name that comes
                    // back is not the name that was asked for.
                    name: "+Kupo".into(),
                    seconds_grace_period: 0,
                },
            ],
            num_allowed_characters: 11,
            ..CharacterSet::default()
        };
        assert_eq!(
            s.character_set_arrived(&set),
            None,
            "no mode change: it logs in instead"
        );
        assert_eq!(
            s.take_actions(),
            vec![CharGenAction::LogOn(dereth_primitives::ObjectId(
                0x5000_0002
            ))]
        );

        // And a set that does *not* contain the new name falls back to character select.
        s.awaiting_char_set_for_login = true;
        let empty = CharacterSet {
            set: Vec::new(),
            ..CharacterSet::default()
        };
        assert_eq!(
            s.character_set_arrived(&empty),
            Some(dereth_ui::framework::mode::CHARACTER_MANAGEMENT)
        );
        assert!(s.take_actions().is_empty());
    }

    /// The chrome buttons open the documented dialogs and the strip jumps pages.
    #[test]
    fn the_chrome_buttons_open_the_documented_dialogs_and_the_strip_jumps_pages() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = CharGenScreen::default();
        s.set_progress_state(&mut ui, EcgProgress::Hertage);
        let msg = |id: u32| ElementMessage {
            source_id: ElementId(id),
            source: ElemHandle::for_test(1),
            id: MessageId(1),
            p1: 0,
            p2: 0,
            point: dereth_ui::msg::MessagePoint::default(),
            serial: 1,
        };
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03CA),
        );
        assert_eq!(s.open_dialog, Some(CharGenDialog::Exit));
        s.open_dialog = None;
        // Off the summary page, Random rerolls the current page without raising a dialog.
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03CB),
        );
        assert_eq!(
            s.open_dialog, None,
            "Random does not warn on the heritage page"
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03F4),
        );
        assert_eq!(
            s.progress,
            EcgProgress::Summary,
            "the strip jumps straight to a page"
        );
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &msg(0x1000_03CB),
        );
        assert_eq!(
            s.open_dialog,
            Some(CharGenDialog::RandomizeWarning),
            "on the summary page Random warns first"
        );
    }
}
