//! Character creation's model: the choices, the credits, the rules and the random draws every
//! creation screen is a view over.
//!
//! **Depends on** `dereth-primitives`, the decoded tables (`dereth-assets`), the shared rules'
//! creation arithmetic (`dereth-rules`) and the contract (`dereth-client-contract`), whose
//! `CharGenResultData` is what a finished creation hands the session. **Used by** the modern UI
//! (`dereth-ui-screens`), the drawn world's creation preview
//! (`dereth-scene`) and any other UI's creation screens.
//!
//! **Must never** draw, lay anything out or touch the wire: a UI shows this model, and the runtime
//! sends what it produces.
//!
//! The client keeps one character-creation state on the player system and the six pages are views
//! over it; the rules -- the attribute budget, the per-heritage skill costs, the name length -- are
//! all enforced here and then re-checked by the server. This crate is that object, minus the parts
//! that are a *view*: the colour wheels, the 3D preview and the palette resolution (the palette
//! lookups and the face palette update) belong to a UI's appearance page.
//!
//! # Two things retail does that the document does not say
//!
//! * **The attribute enum is not in display order.** The client numbers them `1 strength, 2
//!   endurance, 3 quickness, 4 coordination, 5 focus, 6 self` -- Quickness and Coordination are the
//!   other way round from the way every page lists them. See [`Attr`].
//! * **Attribute balancing walks in *display* order** -- strength, endurance,
//!   coordination, quickness, focus, self -- and its static rotating start index
//!   is left pointing at the attribute *after* the one that absorbed the last point. That is why
//!   consecutive edits spread the reduction around instead of emptying one attribute.

use std::collections::BTreeMap;
use std::rc::Rc;

use dereth_assets::motion::ClothingTable;
use dereth_assets::tables::{CharGen, SkillTable};
use dereth_primitives::num::math;
use dereth_primitives::DataId;

pub mod palette;
mod policy;
pub use policy::{
    CreationEntry, CreationPolicy, CreationRandom, CreationTables, CLASSIC_RANDOM_HERITAGES,
};
pub mod texts;
pub use texts::CreationTexts;

/// The client's two global pseudo-random streams, as character generation reaches them.
///
/// **They must never be merged, and character generation is the clearest example of why**: it draws
/// the heritage and the gender -- Turbine's Numerical Recipes `ran2` -- and the start area, the
/// appearance, the clothing and the profession template from the **C runtime's** `rand()`, while
/// heritage and gender use Turbine's `ran2` stream through the three character-state operations.
/// Two draws from one stream where the client takes one from each would change every value after
/// the first.
///
/// # Seeding, and why a headless run of this rebuild is exactly reproducible
///
/// The client seeds `ran2` from `time(NULL)` at startup, unconditionally. The CRT stream is seeded
/// by `srand(time(NULL))` in sound startup **only if DirectSound initialised**, so a silent client
/// keeps the CRT's own default `holdrand` of **1**. `dereth-client` already reproduces both rules --
/// `App::start_shell` passes `1` for the `ran2` seed on a headless run and `Audio::new` picks `1`
/// for the CRT seed when there is no device -- so a headless run rolls the same character every
/// time and its roll can be asserted exactly rather than statistically.
#[derive(Debug, Clone)]
pub struct CharGenRng {
    /// The client random generator's `ran2` stream. The heritage and the gender, and nothing else.
    pub ran2: dereth_primitives::num::rng::Ran2,
    /// The MSVC LCG behind `rand()`. Everything else.
    pub crt: dereth_primitives::num::rng::CrtRand,
}

impl Default for CharGenRng {
    /// A silent, headless client: `ran2` seeded 1 (`App::start_shell`) and the CRT stream at its
    /// own untouched `holdrand` of 1.
    fn default() -> Self {
        Self::new(1, 1)
    }
}

impl CharGenRng {
    /// `ran2_seed` is the startup's `time(NULL)` seed; `crt_seed` is `srand`'s, or 1 when sound
    /// startup never reached it.
    #[must_use]
    pub fn new(ran2_seed: i32, crt_seed: u32) -> Self {
        Self {
            ran2: dereth_primitives::num::rng::Ran2::new(ran2_seed),
            crt: dereth_primitives::num::rng::CrtRand::new(crt_seed),
        }
    }

    /// The dice roll -- **the `ran2` stream**, inclusive of both ends.
    pub fn roll_dice(&mut self, a: i32, b: i32) -> i32 {
        self.ran2.roll_i32(a, b)
    }

    /// A random int in `0..n` -- **the CRT stream**. One `rand()`, then `(r * n + ((r * n >> 31) &
    /// 0x7FFF)) >> 15`, which is a truncating `r * n / 32768`.
    ///
    /// The char-gen state's random-int is this function and nothing else.
    pub fn rand_int(&mut self, n: i32) -> i32 {
        let r = i32::from(self.crt.next_u16());
        let p = r.wrapping_mul(n);
        p.wrapping_add((p >> 31) & 0x7FFF) >> 15
    }

    /// A random int in `0..n` excluding one value -- the same draw repeated until it differs from
    /// `exclude`, which is how the client guarantees that a re-roll *changes* something. **`n <= 1`
    /// returns 0 without drawing at all**, and that early return is load-bearing: a one-entry list
    /// must not advance the stream, and every later value depends on how far the stream has
    /// advanced.
    ///
    /// The char-gen state's excluding random-int is this function and nothing else.
    pub fn rand_int_excluding(&mut self, n: i32, exclude: i32) -> i32 {
        if n <= 1 {
            return 0;
        }
        loop {
            let v = self.rand_int(n);
            if v != exclude {
                return v;
            }
        }
    }

    /// The char-gen state's random real read -- `(double)rand() * 3.051850947599719e-05`,
    /// using the same `1/32767` multiplier as the general random-double helper. **The CRT stream.**
    pub fn rand_real(&mut self) -> f64 {
        f64::from(self.crt.next_u16()) * 3.051_850_947_599_719e-5
    }
}

/// The six attribute ids, as the client numbers them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
#[allow(missing_docs)]
pub enum Attr {
    Strength = 1,
    Endurance = 2,
    Quickness = 3,
    Coordination = 4,
    Focus = 5,
    Self_ = 6,
}

impl Attr {
    /// The enumeration order, which is the order [`CharGenState::get_abs_remaining_credits`] walks.
    pub const ALL: [Self; 6] = [
        Self::Strength,
        Self::Endurance,
        Self::Quickness,
        Self::Coordination,
        Self::Focus,
        Self::Self_,
    ];

    /// The client's order, which is the order the profession page lists the
    /// sliders in: **coordination before quickness**.
    pub const BALANCE_ORDER: [Self; 6] = [
        Self::Strength,
        Self::Endurance,
        Self::Coordination,
        Self::Quickness,
        Self::Focus,
        Self::Self_,
    ];
}

// The skill-advancement enum, the attribute minimum, the skill-array length and the credit
// arithmetic live in `dereth_rules::chargen`, so the server can check a creation request with the
// client's own sums. They are re-exported here.
pub use dereth_rules::chargen::{SkillAdvancementClass, ATTR_MIN, TOTAL_NUM_SKILLS};

/// The character-creation verification response, the code `0xF643` carries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[allow(missing_docs)]
pub enum CgVerification {
    #[default]
    Undef = 0,
    Ok = 1,
    Pending = 2,
    NameInUse = 3,
    NameBanned = 4,
    Corrupt = 5,
    DatabaseDown = 6,
    AdminPrivilegeDenied = 7,
}

impl CgVerification {
    /// The value the wire carries.
    #[must_use]
    pub fn from_code(c: u32) -> Self {
        match c {
            1 => Self::Ok,
            2 => Self::Pending,
            3 => Self::NameInUse,
            4 => Self::NameBanned,
            5 => Self::Corrupt,
            6 => Self::DatabaseDown,
            7 => Self::AdminPrivilegeDenied,
            _ => Self::Undef,
        }
    }

    /// The `StringInfo` id the char gen main screen's char gen verification response notice
    /// shows for a failure, from table enum `0x10000002`.
    ///
    /// These are not the *character-select* screen's
    /// `ID_CharacterManagement_CG_VERIFICATION_RESPONSE_*` strings, which belong to a different
    /// handler on a different screen and say "your character can not be **restored**" — using them
    /// would show a player whose *creation* was refused for a taken name a sentence about
    /// restoring, and two of those five ids are not in the string table at all. The wizard's own
    /// four strings are the contiguous `ID_Character_Err_*` block:
    ///
    /// | code | string |
    /// |---|---|
    /// | 3 `NAME_IN_USE` | `ID_Character_Err_NameReserved` |
    /// | 4 `NAME_BANNED` | `ID_Character_Err_NameBanned` |
    /// | 7 `ADMIN_PRIVILEGE_DENIED` | `ID_Character_Err_NameAdminDenied` |
    /// | 0, 2, 5, 6, >7 (`default:`) | `ID_Character_Err_NameDBDown` |
    ///
    /// Subtracting one and comparing unsigned against six sends `UNDEF` and codes past 7 to the
    /// default arm: **only `OK` shows nothing**, so this returns `None` for exactly one value.
    /// Returning `None` for `Pending` or `Undef` too would make `make_error_message_dialog` refuse
    /// to build the dialog `on_chargen_verification_response` had just asked for — the please-wait
    /// would come down and nothing would replace it.
    #[must_use]
    pub fn error_string_id(self) -> Option<&'static str> {
        Some(match self {
            Self::NameInUse => "ID_Character_Err_NameReserved",
            Self::NameBanned => "ID_Character_Err_NameBanned",
            Self::AdminPrivilegeDenied => "ID_Character_Err_NameAdminDenied",
            Self::Undef | Self::Pending | Self::Corrupt | Self::DatabaseDown => {
                "ID_Character_Err_NameDBDown"
            }
            Self::Ok => return None,
        })
    }
}

/// The default template pushes every attribute towards this.
pub const DEFAULT_ATTR: i32 = 0x32;

/// The four clothing parts, which differ from each other only in which list they index.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EPart {
    Headgear,
    Shirt,
    Trousers,
    Footwear,
}

/// The two Olthoi heritages, for which the wizard skips three pages.
pub const HERITAGE_OLTHOI: u32 = 0x0C;
/// See [`HERITAGE_OLTHOI`].
pub const HERITAGE_OLTHOI_ACID: u32 = 0x0D;
/// Gear Knight -- the appearance page has an arm all of its own for it: no Clothes tab, no nose or
/// mouth spinner, its own four button captions, and a one-shot appearance + clothing randomisation
/// the first time the page shows one.
pub const HERITAGE_GEAR_KNIGHT: u32 = 6;

/// The values the char-gen result writes, in that struct's own vocabulary: the hand-off type the
/// host copies into `dereth_protocol::login::CharGenResult`. It travels in a
/// [`dereth_client_contract::UiRequest::CharGenAction`], so it lives in
/// `dereth_client_contract::pregame`.
pub use dereth_client_contract::pregame::CharGenResultData;

/// Character-generation choices, credits, appearance, and randomization state.
#[derive(Debug, Clone)]
pub struct CharGenState {
    policy: CreationPolicy,
    heraldry_color: i32,
    /// The chosen heritage. 0 until a heritage is chosen.
    pub heritage_group: u32,
    /// The chosen gender. The keys of the heritage's `sexes` map; 0 until chosen.
    pub gender: u32,
    /// The index into the heritage's template list, or -1 for "Custom".
    pub template: i32,
    /// The attribute budget, seeded from the heritage's `AttributeCredits` (330 for most).
    pub total_atrb_credits: i32,
    /// The attribute credits left, displayed and warned about at Finish.
    pub remaining_atrb_credits: i32,
    /// The attribute floor and ceiling; the retail maximum is 100.
    pub atrb_min: i32,
    /// See [`Self::atrb_min`].
    pub atrb_max: i32,
    /// Strength … self, indexed by [`Attr`] minus one.
    values: [i32; 6],
    /// The seven attribute locks, index 0 unused as in the client.
    locked: [bool; 7],
    /// The skill budget, the heritage's `SkillCredits` (52 for most, 68 for Olthoi).
    pub total_skill_credits: i32,
    /// The skill credits left.
    pub remaining_skill_credits: i32,
    /// One level per skill, [`TOTAL_NUM_SKILLS`] long.
    pub skill_levels: Vec<SkillAdvancementClass>,
    /// The name, a 33-byte buffer in the client — so at most 32 characters.
    pub name: String,
    /// The slot, mirrored from character select.
    pub slot: i32,
    /// The start area — an index into the character-generation data's **global** starter-area list.
    /// ACE reads it that way in `PlayerFactory.Create` (`CharGen.StarterAreas[(int)startArea]`),
    /// not as an index into the heritage's own primary/secondary lists.
    pub start_area: u32,
    /// Create-as-admin / create-as-envoy, developer-build fields ACE still reads.
    pub create_as_admin: bool,
    /// See [`Self::create_as_admin`].
    pub create_as_envoy: bool,
    /// The verification state — the "already sent" latch.
    pub verification: CgVerification,
    /// The appearance fields, all left at the heritage's first option. See the module comment.
    pub eyes_strip: i32,
    pub nose_strip: i32,
    pub mouth_strip: i32,
    pub hair_color: i32,
    pub eye_color: i32,
    pub hair_style: i32,
    pub headgear_style: i32,
    pub headgear_color: i32,
    pub shirt_style: i32,
    pub shirt_color: i32,
    pub trousers_style: i32,
    pub trousers_color: i32,
    pub footwear_style: i32,
    pub footwear_color: i32,
    pub skin_shade: f64,
    pub hair_shade: f64,
    pub headgear_shade: f64,
    pub shirt_shade: f64,
    pub trousers_shade: f64,
    pub footwear_shade: f64,
    /// The headgear colour count — the length the colour-information store wrote.
    pub num_headgear_colors: i32,
    /// See [`Self::num_headgear_colors`].
    pub num_shirt_colors: i32,
    /// See [`Self::num_headgear_colors`].
    pub num_trousers_colors: i32,
    /// See [`Self::num_headgear_colors`].
    pub num_footwear_colors: i32,
    /// The headgear palette-template ids — the clothing palette-template keys the colour *index*
    /// selects between, and what the char-gen result actually puts on the wire.
    pub headgear_palette_template_ids: Vec<u32>,
    /// See [`Self::headgear_palette_template_ids`].
    pub shirt_palette_template_ids: Vec<u32>,
    /// See [`Self::headgear_palette_template_ids`].
    pub trousers_palette_template_ids: Vec<u32>,
    /// See [`Self::headgear_palette_template_ids`].
    pub footwear_palette_template_ids: Vec<u32>,
    /// The headgear … footwear pal-set ids — the palette set behind each kept palette template,
    /// which the appearance page's selection change reads back through the headgear pal-set read
    /// and its three siblings to colour the nine spots. The colour-information store writes it
    /// alongside the template ids, same length as them.
    pub headgear_pal_set_ids: Vec<DataId>,
    /// See [`Self::headgear_pal_set_ids`].
    pub shirt_pal_set_ids: Vec<DataId>,
    /// See [`Self::headgear_pal_set_ids`].
    pub trousers_pal_set_ids: Vec<DataId>,
    /// See [`Self::headgear_pal_set_ids`].
    pub footwear_pal_set_ids: Vec<DataId>,
    /// The setup id and its changed flag -- the **fallback** setup, and only that.
    ///
    /// The client returns this value **only when the heritage or the gender is still 0**; with both
    /// chosen it computes the setup live from the heritage's per-gender entry and the current hair
    /// style.
    /// See [`Self::get_setup_id`], which is what the 3D character preview actually reads. The
    /// client's literal is `0x02000054`.
    pub setup_id: DataId,
    /// See [`Self::setup_id`].
    pub setup_changed: bool,
    /// The animation id, the reset's literal `0x03000003`.
    pub anim_id: DataId,
    /// The `ClothingTable`s named by the heritage tables' gear items, which the host loads and
    /// hands over: the style writes read one per style to learn that style's palette-template list.
    /// Empty until the host supplies them, which is exactly the state the client is in before the
    /// table resolves — it writes a colour count of 0.
    pub clothing: Rc<BTreeMap<DataId, ClothingTable>>,
    /// [`Self::balance_attributes`]' rotating start index. This is static in the client, which is
    /// why "the order in which points are taken away depends on previous calls".
    balance_start: u32,
    /// The two streams character generation draws from. Globals in the client; a field here so a
    /// test can pin the roll, and so the host can seed them the way the client's startup does. See
    /// [`CharGenRng`].
    pub rng: CharGenRng,
    /// The heritage-, sex- and appearance-frozen flags -- set by [`Self::randomize_character`]'s
    /// last three lines and cleared on reset.
    ///
    /// The Classic refresh policy reads these before making any nested selections.
    pub frozen: [bool; 3],
}

impl Default for CharGenState {
    fn default() -> Self {
        Self {
            policy: CreationPolicy::Modern,
            heraldry_color: 7,
            heritage_group: 0,
            gender: 0,
            template: -1,
            total_atrb_credits: 0,
            remaining_atrb_credits: 0,
            atrb_min: ATTR_MIN,
            atrb_max: 100,
            values: [ATTR_MIN; 6],
            locked: [false; 7],
            total_skill_credits: 0,
            remaining_skill_credits: 0,
            skill_levels: vec![SkillAdvancementClass::Inactive; TOTAL_NUM_SKILLS],
            name: String::new(),
            slot: 0,
            start_area: 0,
            create_as_admin: false,
            create_as_envoy: false,
            verification: CgVerification::Undef,
            eyes_strip: 0,
            nose_strip: 0,
            mouth_strip: 0,
            hair_color: 0,
            eye_color: 0,
            hair_style: 0,
            headgear_style: 0,
            headgear_color: 0,
            shirt_style: 0,
            shirt_color: 0,
            trousers_style: 0,
            trousers_color: 0,
            footwear_style: 0,
            footwear_color: 0,
            skin_shade: 0.0,
            hair_shade: 0.0,
            headgear_shade: 0.0,
            shirt_shade: 0.0,
            trousers_shade: 0.0,
            footwear_shade: 0.0,
            num_headgear_colors: 0,
            num_shirt_colors: 0,
            num_trousers_colors: 0,
            num_footwear_colors: 0,
            headgear_palette_template_ids: Vec::new(),
            shirt_palette_template_ids: Vec::new(),
            trousers_palette_template_ids: Vec::new(),
            footwear_palette_template_ids: Vec::new(),
            headgear_pal_set_ids: Vec::new(),
            shirt_pal_set_ids: Vec::new(),
            trousers_pal_set_ids: Vec::new(),
            footwear_pal_set_ids: Vec::new(),
            setup_id: DataId(0x0200_0054),
            setup_changed: false,
            anim_id: DataId(0x0300_0003),
            clothing: Rc::new(BTreeMap::new()),
            balance_start: 1,
            rng: CharGenRng::default(),
            frozen: [false; 3],
        }
    }
}

/// A list's length, which every randomiser compares and divides by as a signed `long`. A list this
/// rebuild could not fit in an `i32` cannot exist in a dat record.
fn len_i32(n: usize) -> i32 {
    i32::try_from(n).unwrap_or(i32::MAX)
}

impl CharGenState {
    #[must_use]
    pub fn get(&self, a: Attr) -> i32 {
        self.values[a as usize - 1]
    }

    fn set_raw(&mut self, a: Attr, v: i32) {
        self.values[a as usize - 1] = v;
    }

    /// The lock attribute / the reset attribute lock.
    pub fn set_locked(&mut self, a: Attr, v: bool) {
        self.locked[a as usize] = v;
    }

    #[must_use]
    pub fn is_locked(&self, a: Attr) -> bool {
        self.locked[a as usize]
    }

    /// The heritage's `Skills` overrides layered on the `SkillTable` defaults — the trained and
    /// specialized costs, which ACE's `PlayerFactory.Create` reimplements one-for-one. See
    /// [`dereth_rules::chargen::skill_costs`], which this calls.
    #[must_use]
    pub fn skill_costs(cg: &CharGen, skills: &SkillTable, heritage: u32, skill: u32) -> (i32, i32) {
        dereth_rules::chargen::skill_costs(cg, skills, heritage, skill)
    }

    /// What the wizard's constructor runs before it randomises.
    ///
    /// The order matters and produces the six fifties this build has always opened on: setting
    /// heritage 0 and gender 0 clears the budgets, every appearance index goes to **-1** and every
    /// shade to **-1.0**, and the last step before the setup ids is
    /// [`Self::apply_default_template`], which with an attribute budget of 0 finds the remaining
    /// credits non-zero for every attribute and so raises all six to **50**.
    pub fn reset(&mut self, cg: &CharGen, skills: &SkillTable) {
        self.verification = CgVerification::Undef;
        self.locked = [false; 7];
        self.set_heritage_group(cg, skills, 0);
        self.set_gender(cg, 0);
        self.eyes_strip = -1;
        self.nose_strip = -1;
        self.mouth_strip = -1;
        self.skin_shade = -1.0;
        self.hair_shade = -1.0;
        self.hair_color = -1;
        self.eye_color = -1;
        self.hair_style = -1;
        self.set_headgear_style(cg, -1);
        self.headgear_color = -1;
        self.headgear_shade = -1.0;
        self.set_shirt_style(cg, -1);
        self.shirt_color = -1;
        self.shirt_shade = -1.0;
        self.set_trousers_style(cg, -1);
        self.trousers_color = -1;
        self.trousers_shade = -1.0;
        self.set_footwear_style(cg, -1);
        self.footwear_color = -1;
        self.footwear_shade = -1.0;
        self.template = -1;
        self.atrb_min = ATTR_MIN;
        self.atrb_max = 100;
        self.total_atrb_credits = 0;
        self.remaining_atrb_credits = 0;
        self.total_skill_credits = 0;
        self.remaining_skill_credits = 0;
        self.name.clear();
        self.start_area = u32::MAX;
        self.create_as_admin = false;
        self.create_as_envoy = false;
        // The reset clears the three flags; `randomize_character` sets them again on the way out.
        self.frozen = [false; 3];
        self.apply_default_template(cg, skills);
        self.setup_id = dereth_primitives::DataId(0x0200_0054);
        self.anim_id = dereth_primitives::DataId(0x0300_0003);
    }

    /// The char-gen state's randomize character -- **what retail's character-generation main screen
    /// constructor runs immediately after the reset**, and what this build never had.
    ///
    /// In order: reset; set the heritage to a `ran2` dice roll over `1..=3` (`1..=4` on a Throne of
    /// Destiny account), which itself re-rolls the start area; set the gender to a `ran2` roll over
    /// `1..=2`; then, all from the CRT stream, randomise the appearance, the headgear, shirt,
    /// trousers and footwear, the template, and the start area a second time; finally set the three
    /// frozen flags.
    ///
    /// **The two start-area rolls are not a slip.** The heritage write contains one and the tail
    /// contains another, so the CRT stream is advanced twice and the second draw is the one the
    /// Town page opens on. Removing either changes every later value.
    ///
    /// **A roll over `1..=3` -- three, not thirteen.** The wizard shows thirteen heritage bullets
    /// and the randomiser only ever rolls Aluvian, Gharu'ndim or Sho, plus Viamontian on a Throne
    /// of Destiny account. That is the client's own range, and it is why the paired retail frame
    /// opens on Aluvian.
    pub fn randomize_character(&mut self, cg: &CharGen, skills: &SkillTable, tod: bool) {
        self.reset(cg, skills);
        let heritage = self.roll_heritage(cg, tod);
        self.set_heritage_group(cg, skills, heritage);
        let gender = self.rng.roll_dice(1, 2);
        self.set_gender(cg, u32::try_from(gender).unwrap_or(1));
        self.randomize_appearance(cg, false);
        self.randomize_headgear(cg, false);
        self.randomize_shirt(cg);
        self.randomize_trousers(cg);
        self.randomize_footwear(cg);
        self.randomize_template(cg, skills);
        self.randomize_start_area(cg);
        self.frozen = [true; 3];
    }

    /// Randomize the heritage group — the Heritage page's *Random* arm.
    ///
    /// A dice roll over `1..=3` (`1..=4` with Throne of Destiny) then the heritage write, and
    /// nothing else: **the `ran2` stream**. Because the heritage write re-rolls the start area from
    /// the CRT stream, one click here moves both generators.
    pub fn randomize_heritage_group(&mut self, cg: &CharGen, skills: &SkillTable, tod: bool) {
        let heritage = self.roll_heritage(cg, tod);
        self.set_heritage_group(cg, skills, heritage);
    }

    /// The heritage roll both random heritage arms share: one `ran2` die over Aluvian, Gharu'ndim
    /// and Sho, plus Viamontian with Throne of Destiny.
    ///
    /// The die covers only the ones the connected world defines, so a world without Viamontian
    /// never rolls it whatever the account holds. With every candidate present, as on the final
    /// world, the roll and its result are the client's own; with none, the whole range is rolled.
    fn roll_heritage(&mut self, cg: &CharGen, tod: bool) -> u32 {
        let hi: u32 = if tod { 4 } else { 3 };
        let keys: Vec<u32> = (1..=hi)
            .filter(|k| cg.heritage_groups.contains_key(k))
            .collect();
        if keys.is_empty() {
            let roll = self.rng.roll_dice(1, i32::try_from(hi).unwrap_or(3));
            return u32::try_from(roll).unwrap_or(1);
        }
        let roll = self.rng.roll_dice(1, len_i32(keys.len()));
        usize::try_from(roll - 1)
            .ok()
            .and_then(|i| keys.get(i))
            .copied()
            .unwrap_or(keys[0])
    }

    /// The char-gen state's randomize start area -- **the CRT stream**.
    ///
    /// With no heritage, or a heritage with no primary start areas, the start area is left
    /// untouched. Otherwise it draws an index into the heritage's primary start-area list
    /// (exclusion -1) and takes that entry if it is a valid index into the global starter-area
    /// list, else -1.
    ///
    /// The exclusion argument is a literal **-1**, which no index can equal, so the loop always
    /// terminates on its first draw -- but the `n <= 1` early return in the CRT helper still
    /// applies, and every heritage but the two Olthoi ships exactly **one** primary start area, so
    /// for those the client draws **nothing at all** and takes entry 0.
    pub fn randomize_start_area(&mut self, cg: &CharGen) {
        if self.heritage_group == 0 {
            return;
        }
        let Some(hg) = cg.heritage_groups.get(&self.heritage_group) else {
            return;
        };
        let areas = hg.primary_start_areas.clone();
        let n_all = cg.starter_areas.len();
        let Ok(n) = i32::try_from(areas.len()) else {
            return;
        };
        if n == 0 {
            return;
        }
        let i = self.rng.rand_int_excluding(n, -1);
        let v = usize::try_from(i).ok().and_then(|i| areas.get(i).copied());
        self.start_area = match v {
            Some(v) if usize::try_from(v).is_ok_and(|v| v < n_all) => v,
            // -1 -- the same sentinel the reset writes.
            _ => u32::MAX,
        };
    }

    /// Randomize appearance — **eight CRT draws, in this order**.
    ///
    /// Every index goes through [`CharGenRng::rand_int_excluding`] with the value it already holds
    /// as the exclusion, so a re-roll on a list of two or more always *changes* the part; the two
    /// shades are unrestricted. The order is eyes, nose, mouth, **skin shade, hair shade**, hair
    /// colour, eye colour, hair style, and each list is skipped -- without drawing -- when it is
    /// empty.
    ///
    /// `alt` is used only by the barber screen; on that path the
    /// hair style is drawn from a fixed count rather than from the list. Whole-character randomization and
    /// the Appearance page's random arm both pass `false`.
    pub fn randomize_appearance(&mut self, cg: &CharGen, alt: bool) {
        let Some(sx) = self.sex(cg).cloned() else {
            return;
        };
        let counts = [
            len_i32(sx.eye_strips.len()),
            len_i32(sx.nose_strips.len()),
            len_i32(sx.mouth_strips.len()),
        ];
        let mut strips = [self.eyes_strip, self.nose_strip, self.mouth_strip];
        for (i, n) in counts.into_iter().enumerate() {
            if n != 0 {
                strips[i] = self.rng.rand_int_excluding(n, strips[i]);
            }
        }
        [self.eyes_strip, self.nose_strip, self.mouth_strip] = strips;
        self.skin_shade = self.rng.rand_real();
        self.hair_shade = self.rng.rand_real();
        let n_hair_colors = len_i32(sx.hair_colors.len());
        if n_hair_colors != 0 {
            self.hair_color = self.rng.rand_int_excluding(n_hair_colors, self.hair_color);
        }
        let n_eye_colors = len_i32(sx.eye_colors.len());
        if n_eye_colors != 0 {
            self.eye_color = self.rng.rand_int_excluding(n_eye_colors, self.eye_color);
        }
        let n_hair_styles = len_i32(sx.hair_styles.len());
        if n_hair_styles != 0 {
            self.hair_style = if alt {
                self.rng.rand_int(7)
            } else {
                self.rng.rand_int_excluding(n_hair_styles, self.hair_style)
            };
        }
    }

    /// Randomize headgear — **three CRT draws**, and the only part whose
    /// style can legally come out **-1**.
    ///
    /// A draw over `(n + 1)` followed by [`Self::set_headgear_style`] with `result - 1`: the extra
    /// slot is "no hat", which is the value ACE reads as `HeadgearStyle == uint.MaxValue`. The
    /// colour draw is over the headgear colour count, excluding the current colour, and happens
    /// **after** the style, so it sees the colour count the new style's `ClothingTable` has just
    /// installed. The shade is a raw `rand() * 3.051850947599719e-05`, the same arithmetic the
    /// real-number draw uses.
    pub fn randomize_headgear(&mut self, cg: &CharGen, alt: bool) {
        let Some(sx) = self.sex(cg).cloned() else {
            return;
        };
        let n = len_i32(sx.headgear.len());
        if n != 0 {
            let pick = if alt {
                self.rng.rand_int_excluding(n + 1, self.headgear_style + 1)
            } else {
                self.rng.rand_int(n + 1)
            };
            self.set_headgear_style(cg, pick - 1);
        }
        if self.num_headgear_colors != 0 {
            self.headgear_color = self
                .rng
                .rand_int_excluding(self.num_headgear_colors, self.headgear_color);
        }
        self.headgear_shade = self.rng.rand_real();
    }

    /// Randomize shirt style, colour, then shade using **the CRT stream**.
    pub fn randomize_shirt(&mut self, cg: &CharGen) {
        let Some(sx) = self.sex(cg).cloned() else {
            return;
        };
        let n = len_i32(sx.shirts.len());
        if n != 0 {
            let pick = self.rng.rand_int_excluding(n, self.shirt_style);
            self.set_shirt_style(cg, pick);
        }
        if self.num_shirt_colors != 0 {
            self.shirt_color = self
                .rng
                .rand_int_excluding(self.num_shirt_colors, self.shirt_color);
        }
        self.shirt_shade = self.rng.rand_real();
    }

    /// The char-gen state's randomize trousers.
    pub fn randomize_trousers(&mut self, cg: &CharGen) {
        let Some(sx) = self.sex(cg).cloned() else {
            return;
        };
        let n = len_i32(sx.pants.len());
        if n != 0 {
            let pick = self.rng.rand_int_excluding(n, self.trousers_style);
            self.set_trousers_style(cg, pick);
        }
        if self.num_trousers_colors != 0 {
            self.trousers_color = self
                .rng
                .rand_int_excluding(self.num_trousers_colors, self.trousers_color);
        }
        self.trousers_shade = self.rng.rand_real();
    }

    /// The char-gen state's randomize footwear.
    pub fn randomize_footwear(&mut self, cg: &CharGen) {
        let Some(sx) = self.sex(cg).cloned() else {
            return;
        };
        let n = len_i32(sx.footwear.len());
        if n != 0 {
            let pick = self.rng.rand_int_excluding(n, self.footwear_style);
            self.set_footwear_style(cg, pick);
        }
        if self.num_footwear_colors != 0 {
            self.footwear_color = self
                .rng
                .rand_int_excluding(self.num_footwear_colors, self.footwear_color);
        }
        self.footwear_shade = self.rng.rand_real();
    }

    /// Randomize clothing — headgear, shirt, trousers, footwear, in that
    /// order. The Appearance page's *Random* arm on the **Clothes** tab, which passes `alt = 1`.
    pub fn randomize_clothing(&mut self, cg: &CharGen, alt: bool) {
        self.randomize_headgear(cg, alt);
        self.randomize_shirt(cg);
        self.randomize_trousers(cg);
        self.randomize_footwear(cg);
    }

    /// Randomize the profession template using **the CRT stream**; it never picks
    /// template 0.
    ///
    /// Nothing without a heritage and a gender. The two Olthoi heritages (`0x0C`, `0x0D`) get
    /// template 1 applied directly. Otherwise, when the heritage has more than one template, it
    /// draws over `n - 1` excluding `template - 1`, adds one, and applies the result.
    ///
    /// Index 0 is the "Custom" fallback used when fitting a template to the character, so the
    /// roll is over `1..n-1` -- expressed as a draw over `n - 1` shifted up by one, with the
    /// *current* template shifted down by one as the exclusion. The two Olthoi heritages have no
    /// Profession page at all and are pinned to template 1.
    pub fn randomize_template(&mut self, cg: &CharGen, skills: &SkillTable) {
        if self.heritage_group == 0 || self.gender == 0 {
            return;
        }
        if self.heritage_group == HERITAGE_OLTHOI || self.heritage_group == HERITAGE_OLTHOI_ACID {
            self.template = 1;
            self.apply_template(cg, skills);
            return;
        }
        let n = match cg.heritage_groups.get(&self.heritage_group) {
            Some(hg) => len_i32(hg.templates.len()),
            None => return,
        };
        if n > 1 {
            self.template = self.rng.rand_int_excluding(n - 1, self.template - 1) + 1;
            if self.template != -1 {
                self.apply_template(cg, skills);
            }
        }
    }

    /// Randomize skills — the Skills page's *Random* arm. **The CRT
    /// stream**, and it is a two-phase spend rather than a single pick.
    ///
    /// Phase one, after [`Self::reset_skill_levels`]: up to **100** draws over
    /// [`TOTAL_NUM_SKILLS`]. Each drawn index is bounds-checked before indexing. An `UNTRAINED`
    /// slot is trained **only on an odd pass number**, while a `TRAINED` slot is specialised on any
    /// pass; the loop stops as soon as the remaining skill credits reach exactly 0.
    ///
    /// Phase two, only if credits are still positive: a straight walk from skill 0 upwards buying
    /// whatever the remaining budget affords, which is why a random roll usually ends at 0 credits.
    ///
    /// **The odd-pass gate is not decoration.** Without it phase one trains twice as fast, the walk
    /// in phase two never runs, and a normal 52-credit heritage lands on a different skill set.
    pub fn randomize_skills(&mut self, cg: &CharGen, skills: &SkillTable) {
        self.randomize_skills_in_range(cg, skills, TOTAL_NUM_SKILLS);
    }

    fn randomize_skills_in_range(&mut self, cg: &CharGen, skills: &SkillTable, draw_range: usize) {
        if self.heritage_group == 0 || self.gender == 0 || self.sex(cg).is_none() {
            return;
        }
        self.reset_skill_levels(cg, skills);
        let total = len_i32(draw_range);
        let mut pass: u32 = 0;
        loop {
            let draw = self.rng.rand_int(total);
            if let Some(i) = usize::try_from(draw).ok().filter(|i| *i < TOTAL_NUM_SKILLS) {
                let id = u32::try_from(i).unwrap_or(0);
                match self.skill_levels[i] {
                    SkillAdvancementClass::Untrained if pass & 1 != 0 => {
                        self.set_skill_level(cg, skills, id, SkillAdvancementClass::Trained);
                    }
                    SkillAdvancementClass::Trained => {
                        self.set_skill_level(cg, skills, id, SkillAdvancementClass::Specialized);
                    }
                    _ => {}
                }
            }
            if self.remaining_skill_credits == 0 {
                break;
            }
            pass += 1;
            if pass >= 100 {
                break;
            }
        }
        if self.remaining_skill_credits <= 0 {
            return;
        }
        for i in 0..TOTAL_NUM_SKILLS {
            let id = u32::try_from(i).unwrap_or(0);
            let (trained, specialized) = Self::skill_costs(cg, skills, self.heritage_group, id);
            let left = self.remaining_skill_credits;
            match self.skill_levels[i] {
                SkillAdvancementClass::Untrained if left - trained >= 0 => {
                    self.remaining_skill_credits = left - trained;
                    self.skill_levels[i] = SkillAdvancementClass::Trained;
                }
                SkillAdvancementClass::Trained if left + trained - specialized >= 0 => {
                    self.remaining_skill_credits = left + trained - specialized;
                    self.skill_levels[i] = SkillAdvancementClass::Specialized;
                }
                _ => {}
            }
            if self.remaining_skill_credits == 0 {
                break;
            }
        }
    }

    /// The char-gen state's heritage-group write, as the heritage page's element-message handler
    /// calls it.
    ///
    /// Selecting a heritage does **not** reset the skill levels and re-apply the default template.
    /// The function it adopts the group's setup id and its two budgets, runs
    /// [`Self::apply_template`] on the template that is *already* selected, picks a start area, and
    /// then clamps everything the new heritage cannot support — and [`Self::reset_skill_levels`]
    /// runs there only when the clamped budget has gone **negative**.
    ///
    /// **The start-area randomiser is called from inside it**, which is the whole reason retail's
    /// Town page opens on a lit pin. It draws from the C-runtime `rand()` stream. See
    /// [`Self::randomize_start_area`].
    pub fn set_heritage_group(&mut self, cg: &CharGen, skills: &SkillTable, heritage: u32) {
        self.heritage_group = heritage;
        if heritage != 0 {
            let Some(hg) = cg.heritage_groups.get(&heritage) else {
                return;
            };
            let (setup, attr, skill) = (hg.setup, hg.attribute_credits, hg.skill_credits);
            if setup != self.setup_id {
                self.setup_changed = true;
                self.setup_id = setup;
            }
            self.total_atrb_credits = i32::try_from(attr).unwrap_or(0);
            self.recompute_remaining();
            self.total_skill_credits = i32::try_from(skill).unwrap_or(0);
            self.remaining_skill_credits = self.total_skill_credits;
            self.apply_template(cg, skills);
            // The client re-rolls the start area here, which is why retail's Town page opens on a
            // lit pin the moment a heritage exists rather than on a black pane.
            self.randomize_start_area(cg);
        }
        self.constrain_all_by_heritage(cg, skills);
        let (hg, sh, tr, fw) = (
            self.headgear_style,
            self.shirt_style,
            self.trousers_style,
            self.footwear_style,
        );
        self.set_headgear_style(cg, hg);
        self.set_shirt_style(cg, sh);
        self.set_trousers_style(cg, tr);
        self.set_footwear_style(cg, fw);
    }

    /// The char-gen state's constrain all by heritage.
    pub fn constrain_all_by_heritage(&mut self, cg: &CharGen, skills: &SkillTable) {
        let Some(hg) = cg.heritage_groups.get(&self.heritage_group).cloned() else {
            self.template = -1;
            self.total_atrb_credits = 0;
            self.recompute_remaining();
            self.total_skill_credits = 0;
            self.remaining_skill_credits = 0;
            self.name.clear();
            self.start_area = u32::MAX;
            self.apply_default_template(cg, skills);
            return;
        };
        self.constrain_all_by_gender(cg);
        if i32::try_from(hg.templates.len()).unwrap_or(0) <= self.template {
            self.template = -1;
        }
        let attr = i32::try_from(hg.attribute_credits).unwrap_or(0);
        if attr < self.total_atrb_credits {
            // The client's own arm, reproduced as it stands: it zeroes the attribute budget
            // *before* dividing it by six, so all six attributes are driven to 0 and the floor of
            // 10 is not applied here. Unreachable in the shipped data -- every heritage carries 330
            // credits except Olthoi's 60, and nothing lowers the budget mid-session -- and so
            // reproduced rather than repaired.
            self.total_atrb_credits = 0;
            self.recompute_remaining();
            for a in Attr::BALANCE_ORDER {
                let v = if a == Attr::Strength {
                    0
                } else {
                    self.total_atrb_credits / 6
                };
                self.set_attribute_balanced(a, v, false);
            }
        }
        let skill = i32::try_from(hg.skill_credits).unwrap_or(0);
        if skill < self.total_skill_credits {
            self.total_skill_credits = skill;
        }
        self.update_remaining_skill_credits(cg, skills);
        if self.remaining_skill_credits < 0 {
            self.reset_skill_levels(cg, skills);
        }
    }

    /// The char-gen state's gender write.
    pub fn set_gender(&mut self, cg: &CharGen, gender: u32) {
        self.gender = gender;
        self.constrain_all_by_gender(cg);
        let (hg, sh, tr, fw) = (
            self.headgear_style,
            self.shirt_style,
            self.trousers_style,
            self.footwear_style,
        );
        self.set_headgear_style(cg, hg);
        self.set_shirt_style(cg, sh);
        self.set_trousers_style(cg, tr);
        self.set_footwear_style(cg, fw);
    }

    /// The live setup id -- **the setup the preview is actually built from.**
    ///
    /// With a heritage and a gender chosen it is the per-gender record's setup, unless a hair style
    /// is chosen and that style carries a non-zero alternate setup, which wins. With either still 0
    /// it is the stored fallback [`Self::setup_id`].
    ///
    /// **This is not the heritage group's own `setup` field**, and the difference is visible on
    /// every character: the heritage group's setup is `0x02000054` for all thirteen heritages --
    /// the generic human -- while the per-gender setup is `0x02000001` for a human male and
    /// `0x0200004E` for a human female, and the shipped table holds **18 distinct** sex setups.
    /// Seeding [`Self::setup_id`] from the heritage's field alone would mean **choosing Female
    /// changes nothing about the model**.
    ///
    /// The hair arm matters too: **124 of the 869 shipped hair styles carry an alternate setup**,
    /// on heritages **7** (Tumerok) and **11** (Undead) only, so a hair choice there swaps the
    /// whole body setup.
    #[must_use]
    pub fn get_setup_id(&self, cg: &CharGen) -> DataId {
        let Some(sx) = self.sex(cg) else {
            return self.setup_id;
        };
        if self.heritage_group == 0 || self.gender == 0 {
            return self.setup_id;
        }
        if self.hair_style != -1 {
            if let Some(h) = usize::try_from(self.hair_style)
                .ok()
                .and_then(|i| sx.hair_styles.get(i))
            {
                if h.alternate_setup.0 != 0 {
                    return h.alternate_setup;
                }
            }
        }
        sx.setup
    }

    /// The heritage's per-gender record, which every appearance rule is bounded by.
    #[must_use]
    pub fn sex<'a>(&self, cg: &'a CharGen) -> Option<&'a dereth_assets::tables::SexCg> {
        cg.heritage_groups
            .get(&self.heritage_group)?
            .sexes
            .get(&self.gender)
    }

    /// Every appearance index clamped to the
    /// **last** entry of the list it indexes, and the whole lot set to -1 when there is no
    /// heritage or no gender.
    pub fn constrain_all_by_gender(&mut self, cg: &CharGen) {
        let Some(sx) = self.sex(cg).cloned() else {
            self.eyes_strip = -1;
            self.nose_strip = -1;
            self.mouth_strip = -1;
            self.skin_shade = -1.0;
            self.hair_shade = -1.0;
            self.hair_color = -1;
            self.eye_color = -1;
            self.hair_style = -1;
            self.set_headgear_style(cg, -1);
            self.headgear_color = -1;
            self.headgear_shade = -1.0;
            self.set_shirt_style(cg, -1);
            self.shirt_color = -1;
            self.shirt_shade = -1.0;
            self.set_trousers_style(cg, -1);
            self.trousers_color = -1;
            self.trousers_shade = -1.0;
            self.set_footwear_style(cg, -1);
            self.footwear_color = -1;
            self.footwear_shade = -1.0;
            return;
        };
        let last = |n: usize| i32::try_from(n).unwrap_or(0) - 1;
        if last(sx.eye_strips.len()) < self.eyes_strip {
            self.eyes_strip = last(sx.eye_strips.len());
        }
        if last(sx.nose_strips.len()) < self.nose_strip {
            self.nose_strip = last(sx.nose_strips.len());
        }
        if last(sx.mouth_strips.len()) < self.mouth_strip {
            self.mouth_strip = last(sx.mouth_strips.len());
        }
        if last(sx.hair_styles.len()) < self.hair_style {
            self.hair_style = last(sx.hair_styles.len());
        }
        if last(sx.eye_colors.len()) < self.eye_color {
            self.eye_color = last(sx.eye_colors.len());
        }
        if last(sx.hair_colors.len()) < self.hair_color {
            self.hair_color = last(sx.hair_colors.len());
        }
        if last(sx.headgear.len()) < self.headgear_style {
            self.set_headgear_style(cg, last(sx.headgear.len()));
        }
        if self.num_headgear_colors <= self.headgear_color {
            self.headgear_color = self.num_headgear_colors - 1;
        }
        if last(sx.shirts.len()) < self.shirt_style {
            self.set_shirt_style(cg, last(sx.shirts.len()));
        }
        if self.num_shirt_colors <= self.shirt_color {
            self.shirt_color = self.num_shirt_colors - 1;
        }
        if last(sx.pants.len()) < self.trousers_style {
            self.set_trousers_style(cg, last(sx.pants.len()));
        }
        if self.num_trousers_colors <= self.trousers_color {
            self.trousers_color = self.num_trousers_colors - 1;
        }
        if last(sx.footwear.len()) < self.footwear_style {
            self.set_footwear_style(cg, last(sx.footwear.len()));
        }
        if self.num_footwear_colors <= self.footwear_color {
            self.footwear_color = self.num_footwear_colors - 1;
        }
    }

    /// The char-gen state's reset skill levels.
    ///
    /// This is **not** "set every skill to untrained", and the difference is a wire contract. The
    /// loop runs from **1**, and it only writes the level of an id the skill table actually
    /// contains *and* whose two per-heritage costs are both `>= 0`: a trained cost below 1 makes it
    /// specialized (if the specialized cost is also below 1) or trained, and otherwise it is
    /// untrained.
    ///
    /// So a heritage's free skills — the ones the heritage page lists as "starting skills" and
    /// "bonus skills, trained" — are exactly the ones whose cost is zero, and every id the skill
    /// table does not name is left at the array's zero, `INVALID` / **`Inactive`**. That matters:
    /// ACE's `PlayerFactory.Create` `continue`s on `Inactive` and looks every other value up in
    /// `SkillBaseHash`, so sending `Untrained` for an id the table has no row for is
    /// `InvalidSkillRequested` → `Corrupt`.
    pub fn reset_skill_levels(&mut self, cg: &CharGen, skills: &SkillTable) {
        self.skill_levels = vec![SkillAdvancementClass::Inactive; TOTAL_NUM_SKILLS];
        for i in 1..TOTAL_NUM_SKILLS {
            let id = u32::try_from(i).unwrap_or(0);
            self.skill_levels[i] =
                dereth_rules::chargen::default_skill_class(cg, skills, self.heritage_group, id);
        }
        self.update_remaining_skill_credits(cg, skills);
    }

    /// The absolute remaining attribute credits, excluding `exclude`.
    ///
    /// The budget minus, for attributes 1 to 6, the current value when the attribute is locked or
    /// is `exclude`, and 10 otherwise. Locked attributes and the one being edited count at their
    /// real value; every other counts as if it were at the minimum. That is what makes "raise one
    /// and watch the others fall" work.
    #[must_use]
    pub fn get_abs_remaining_credits(&self, exclude: Option<Attr>) -> i32 {
        let mut remaining = self.total_atrb_credits;
        for a in Attr::ALL {
            let charge = if self.is_locked(a) || Some(a) == exclude {
                self.get(a)
            } else {
                ATTR_MIN
            };
            remaining -= charge;
        }
        remaining
    }

    /// The char-gen state's strength write and its five identical siblings.
    ///
    /// A raise is refused (the current value is kept) when no credits remain with this attribute
    /// excluded; otherwise the value is stored and the others are balanced against it.
    ///
    /// Returns the value that ended up stored, which is what the client returns.
    ///
    /// This is the profession page's call, which passes `balance = 1`. Applying a template passes
    /// **0**; see [`Self::set_attribute_balanced`].
    pub fn set_attribute(&mut self, a: Attr, value: i32) -> i32 {
        self.set_attribute_balanced(a, value, true)
    }

    /// The attribute write with its `balance` argument, which only [`Self::balance_attributes`]
    /// reads.
    ///
    /// The clamp to the attribute floor and ceiling is **this build's**, not the client's: the
    /// client clamps on the *page*, in the `0x100002EF` text box's `to_uint32` arm (`< 0x65` else
    /// 100, `< 10` else 10) and in the attribute-value setter's credit headroom. Keeping it here as
    /// well cannot change a legal build and stops an out-of-range template from reaching the wire.
    pub fn set_attribute_balanced(&mut self, a: Attr, value: i32, balance: bool) -> i32 {
        let value = value.clamp(self.atrb_min, self.atrb_max);
        if value > self.get(a) && self.get_abs_remaining_credits(Some(a)) == 0 {
            return self.get(a);
        }
        self.set_raw(a, value);
        if balance {
            self.balance_attributes(Some(a));
        }
        self.recompute_remaining();
        self.get(a)
    }

    fn recompute_remaining(&mut self) {
        self.remaining_atrb_credits =
            self.total_atrb_credits - Attr::ALL.iter().map(|a| self.get(*a)).sum::<i32>();
    }

    /// Balance the attributes against the budget, never touching `exclude`.
    ///
    /// Runs only when the six attributes total more than the budget. It walks
    /// [`Attr::BALANCE_ORDER`] starting at whichever attribute the *previous* call left the static
    /// index pointing at, skipping the excluded attribute, any locked one and any already
    /// at [`ATTR_MIN`], decrementing one point at a time. When the overspend is gone it leaves the
    /// static pointing at the attribute after the one that paid, and returns.
    pub fn balance_attributes(&mut self, exclude: Option<Attr>) {
        let mut over =
            Attr::ALL.iter().map(|a| self.get(*a)).sum::<i32>() - self.total_atrb_credits;
        if over <= 0 {
            return;
        }
        let order = Attr::BALANCE_ORDER;
        let start = order
            .iter()
            .position(|a| *a as u32 == self.balance_start)
            .unwrap_or(0);
        // At most one point per attribute per pass, and the loop terminates because every pass that
        // changes nothing breaks out.
        let mut i = start;
        loop {
            let mut moved = false;
            for _ in 0..order.len() {
                let a = order[i];
                i = (i + 1) % order.len();
                if Some(a) == exclude || self.is_locked(a) || self.get(a) <= ATTR_MIN {
                    continue;
                }
                self.set_raw(a, self.get(a) - 1);
                moved = true;
                over -= 1;
                if over < 1 {
                    self.balance_start = order[i] as u32;
                    self.recompute_remaining();
                    return;
                }
            }
            if !moved {
                break;
            }
        }
        self.recompute_remaining();
    }

    /// The char-gen state's apply default template — the "Custom" profession.
    ///
    /// Each attribute in turn is pushed to **50** when it is already there or when the remaining
    /// credits with it excluded are non-zero, and the remaining attribute credits are recomputed
    /// after every one. Attributes are visited in [`Attr::BALANCE_ORDER`] (the order retail tests
    /// them: strength, endurance, coordination, quickness, …). The client does **not** balance the
    /// attributes at the end, and its last step **is** resetting the skill levels.
    pub fn apply_default_template(&mut self, cg: &CharGen, skills: &SkillTable) {
        for a in Attr::BALANCE_ORDER {
            if self.get(a) >= DEFAULT_ATTR || self.get_abs_remaining_credits(Some(a)) != 0 {
                self.set_raw(a, DEFAULT_ATTR);
                self.recompute_remaining();
            }
        }
        self.reset_skill_levels(cg, skills);
    }

    /// The profession page's own entry point.
    ///
    /// Stores `n`, applies it when `apply` is set and `n != -1`, and returns the stored template.
    pub fn set_template(&mut self, cg: &CharGen, skills: &SkillTable, n: i32, apply: bool) -> i32 {
        self.template = n;
        if apply && n != -1 {
            self.apply_template(cg, skills);
        }
        self.template
    }

    /// **The function that decides the template number the server is sent**, which resolves how the
    /// client handles that field.
    ///
    /// The profession page's update and the summary page's update both call it, so by the time
    /// *Finish* is pressed the template is whichever of the heritage's templates the character most
    /// resembles — never the constructor's -1, as long as a heritage and a gender are set. It
    /// scores templates **1..n-1** only; index 0 is the fallback.
    ///
    /// The three terms and their exponents (2.5, 3.0 and 3.5, each scaled by 1/3):
    ///
    /// * attributes: `pow(sum(min(char, tmpl)) / sum(tmpl), 2.5) / 3`, or a flat **0.25** when the
    ///   template's own attributes sum to less than 1 (which is every heritage's entry 0);
    /// * normal skills: `pow(trained-or-better / count, 3.0) / 3`, or **+0.3** for an empty list;
    /// * primary skills: `pow((1 per trained + 2 per specialised) / (2 * count), 3.5) / 3`, or
    ///   **+0.45** for an empty list.
    ///
    /// Strictly-greater wins, so the lowest index wins a tie, and a best score below **0.75**
    ///  falls back to template **0**.
    pub fn fit_template_to_character(&mut self, cg: &CharGen) {
        if self.heritage_group == 0 || self.gender == 0 {
            return;
        }
        let Some(hg) = cg.heritage_groups.get(&self.heritage_group) else {
            return;
        };
        let mut best = -1i32;
        let mut best_score = 0.0f64;
        for (i, t) in hg.templates.iter().enumerate().skip(1) {
            let tmpl: [i32; 6] = [
                i32::try_from(t.attributes[0]).unwrap_or(0),
                i32::try_from(t.attributes[1]).unwrap_or(0),
                i32::try_from(t.attributes[2]).unwrap_or(0),
                i32::try_from(t.attributes[3]).unwrap_or(0),
                i32::try_from(t.attributes[4]).unwrap_or(0),
                i32::try_from(t.attributes[5]).unwrap_or(0),
            ];
            // `BALANCE_ORDER` is the struct's own field order: strength, endurance, coordination,
            // quickness, focus, self.
            let mine = Attr::BALANCE_ORDER.map(|a| self.get(a));
            let total: i32 = tmpl.iter().sum();
            let mut score = if total < 1 {
                0.25
            } else {
                let matched: i32 = (0..6)
                    .map(|k| if tmpl[k] <= mine[k] { tmpl[k] } else { mine[k] })
                    .sum();
                math::pow(f64::from(matched) / f64::from(total), 2.5) / 3.0
            };
            let normals = i32::try_from(t.normal_skills.len()).unwrap_or(0);
            if normals > 0 {
                let hit = t
                    .normal_skills
                    .iter()
                    .filter(|s| self.skill_level(**s) as i32 >= 2)
                    .count();
                let hit = i32::try_from(hit).unwrap_or(0);
                score += math::pow(f64::from(hit) / f64::from(normals), 3.0) / 3.0;
            } else {
                score += 0.3;
            }
            let primaries = i32::try_from(t.primary_skills.len()).unwrap_or(0);
            if primaries > 0 {
                let mut hit = 0i32;
                for s in &t.primary_skills {
                    match self.skill_level(*s) {
                        SkillAdvancementClass::Trained => hit += 1,
                        SkillAdvancementClass::Specialized => hit += 2,
                        _ => {}
                    }
                }
                score += math::pow(f64::from(hit) / f64::from(primaries * 2), 3.5) / 3.0;
            } else {
                score += 0.45;
            }
            if best_score < score {
                best_score = score;
                best = i32::try_from(i).unwrap_or(0);
            }
        }
        if best_score < 0.75 {
            best = 0;
        }
        self.template = best;
    }

    /// The char-gen state's skill-level read, bounds-checked as the client's callers are.
    #[must_use]
    pub fn skill_level(&self, skill: u32) -> SkillAdvancementClass {
        usize::try_from(skill)
            .ok()
            .and_then(|i| self.skill_levels.get(i).copied())
            .unwrap_or(SkillAdvancementClass::Inactive)
    }

    /// The six attributes by their **enum** number, and 0 for anything else, which is the client's
    /// own `default:` arm.
    ///
    /// The summary page is the caller that needs the number rather than the [`Attr`]:
    /// The client reads the two attribute ids out of a `SkillFormula`.
    #[must_use]
    pub fn get_attribute(&self, n: u32) -> i32 {
        match n {
            1 => self.get(Attr::Strength),
            2 => self.get(Attr::Endurance),
            3 => self.get(Attr::Quickness),
            4 => self.get(Attr::Coordination),
            5 => self.get(Attr::Focus),
            6 => self.get(Attr::Self_),
            _ => 0,
        }
    }

    /// What the **summary page** prints beside a skill.
    ///
    /// 0 when the skill is not in the table, when its level is below the skill's minimum level, or
    /// when the formula fails; otherwise the formula over the skill's two attributes, plus 5 when
    /// trained or 10 when specialized.
    ///
    /// The skill formula is `z == 0 ? fail : floor((x*a1 + y*a2 + w) / z + 0.5)`, in floating
    /// point. `x`, `y`, `w` and `z` are unsigned in the table and none of the shipped rows is
    /// negative.
    #[must_use]
    pub fn skill_score(&self, skills: &SkillTable, skill: u32) -> i32 {
        dereth_rules::chargen::skill_score(skills, skill, self.skill_level(skill), |n| {
            self.get_attribute(n)
        })
    }

    /// The char-gen state's apply template — a profession.
    ///
    /// Clears every lock, sets the six attributes from the template, resets the skill levels,
    /// trains every `NormalSkill` and then specialises every `PrimarySkill` it can still afford.
    /// **Olthoi heritages force template 0**, which is why their profession page is skipped.
    ///
    /// It takes **no index**: the caller has already written the template. Taking one would let a
    /// caller apply one template while the stored template named another, and the stored template
    /// is what goes on the wire.
    pub fn apply_template(&mut self, cg: &CharGen, skills: &SkillTable) {
        self.locked = [false; 7];
        if self.heritage_group == HERITAGE_OLTHOI || self.heritage_group == HERITAGE_OLTHOI_ACID {
            self.template = 0;
        }
        if self.heritage_group == 0 || self.gender == 0 || self.template < 0 {
            return;
        }
        let Some(hg) = cg.heritage_groups.get(&self.heritage_group) else {
            return;
        };
        let Some(t) = usize::try_from(self.template)
            .ok()
            .and_then(|i| hg.templates.get(i))
        else {
            return;
        };
        let t = t.clone();
        // Strength … self, in retail's order, and with **balance off** — no balancing between the
        // six writes. Each still carries its own "no credits left" refusal, which is why `set_raw`
        // is wrong here.
        for (a, v) in [
            (Attr::Strength, t.attributes[0]),
            (Attr::Endurance, t.attributes[1]),
            (Attr::Coordination, t.attributes[2]),
            (Attr::Quickness, t.attributes[3]),
            (Attr::Focus, t.attributes[4]),
            (Attr::Self_, t.attributes[5]),
        ] {
            self.set_attribute_balanced(a, i32::try_from(v).unwrap_or(ATTR_MIN), false);
        }
        self.reset_skill_levels(cg, skills);
        for s in &t.normal_skills {
            self.set_skill_level(cg, skills, *s, SkillAdvancementClass::Trained);
        }
        // The primary list is specialised *directly*, refunding whatever the skill already cost —
        // which is why a primary skill that is also a normal skill does not pay twice.
        for s in &t.primary_skills {
            let Some(idx) = usize::try_from(*s).ok().filter(|i| *i < TOTAL_NUM_SKILLS) else {
                continue;
            };
            let (trained, specialized) = Self::skill_costs(cg, skills, self.heritage_group, *s);
            if self.skill_levels[idx] == SkillAdvancementClass::Inactive {
                continue;
            }
            let refund = match self.skill_levels[idx] {
                SkillAdvancementClass::Trained => trained,
                SkillAdvancementClass::Specialized => specialized,
                _ => 0,
            };
            let credits = self.remaining_skill_credits + refund;
            if credits - specialized >= 0 {
                self.remaining_skill_credits = credits - specialized;
                self.skill_levels[idx] = SkillAdvancementClass::Specialized;
            }
        }
    }

    /// The char-gen state's store color information.
    ///
    /// The style's `ClothingTable` (database-object type `0x19`) is walked in **hash-bucket order**
    /// and a clothing palette-template key is kept only when the heritage's per-gender
    /// `clothing_colors` list also names it. The kept keys are the palette-template ids the colour
    /// spots index, and the count is the colour count the spots are enabled against.
    ///
    /// The bucket order is the observable part: it is the *index* the wire carries, so a sorted
    /// walk would send a different colour. The hash iterator starts at bucket 0 and follows each
    /// chain, so the order is `key % buckets` and then insertion order within a bucket, which for a
    /// packed hash table read back from the dat is the order the file lists them.
    ///
    /// The third return is the pal-set id array the same loop fills: for each kept key, the
    /// clothing palette template's **first** sub-palette's `PaletteSet`. The appearance page's
    /// selection change reads it back through the four pal-set reads to colour the nine spots,
    /// which is the only consumer in the client.
    fn store_color_information(
        &self,
        table: Option<&ClothingTable>,
        colors: &[u32],
    ) -> (i32, Vec<u32>, Vec<DataId>) {
        let Some(t) = table else {
            return (0, Vec::new(), Vec::new());
        };
        let buckets = t.palette_template_buckets.max(1);
        let mut by_bucket: Vec<Vec<u32>> = vec![Vec::new(); buckets as usize];
        for key in t.palette_templates.keys() {
            by_bucket[(key % buckets) as usize].push(*key);
        }
        let mut kept = Vec::new();
        let mut pal_sets = Vec::new();
        for bucket in by_bucket {
            for key in bucket {
                if colors.contains(&key) {
                    kept.push(key);
                    pal_sets.push(
                        t.palette_templates
                            .get(&key)
                            .and_then(|p| p.subpalette_effects.first())
                            .map_or(DataId(0), |e| e.palette_set),
                    );
                }
            }
        }
        (i32::try_from(kept.len()).unwrap_or(0), kept, pal_sets)
    }

    /// The char-gen state's headgear-style write and its three identical siblings
    /// (the shirt, trousers and footwear style writes).
    ///
    /// The colour count is cleared first and stays 0 for a style of **-1** ("nothing"), which is
    /// what the reset leaves headgear at and what ACE reads as `HeadgearStyle == uint.MaxValue`.
    /// After a real style the colour index is pulled into `0 ..= count - 1`.
    pub fn set_headgear_style(&mut self, cg: &CharGen, style: i32) {
        let (n, ids, pal_sets) = self.gear_colors(cg, EPart::Headgear, style);
        self.num_headgear_colors = n;
        self.headgear_palette_template_ids = ids;
        self.headgear_pal_set_ids = pal_sets;
        if n > 0 {
            self.headgear_color = self.headgear_color.clamp(0, n - 1);
        }
        self.headgear_style = style;
    }

    /// See [`Self::set_headgear_style`].
    pub fn set_shirt_style(&mut self, cg: &CharGen, style: i32) {
        let (n, ids, pal_sets) = self.gear_colors(cg, EPart::Shirt, style);
        self.num_shirt_colors = n;
        self.shirt_palette_template_ids = ids;
        self.shirt_pal_set_ids = pal_sets;
        if n > 0 {
            self.shirt_color = if self.policy == CreationPolicy::Classic {
                self.shirt_color.min(n - 1)
            } else {
                self.shirt_color.clamp(0, n - 1)
            };
        }
        self.shirt_style = style;
    }

    /// See [`Self::set_headgear_style`].
    pub fn set_trousers_style(&mut self, cg: &CharGen, style: i32) {
        let (n, ids, pal_sets) = self.gear_colors(cg, EPart::Trousers, style);
        self.num_trousers_colors = n;
        self.trousers_palette_template_ids = ids;
        self.trousers_pal_set_ids = pal_sets;
        if n > 0 {
            self.trousers_color = if self.policy == CreationPolicy::Classic {
                self.trousers_color.min(n - 1)
            } else {
                self.trousers_color.clamp(0, n - 1)
            };
        }
        self.trousers_style = style;
    }

    /// See [`Self::set_headgear_style`].
    pub fn set_footwear_style(&mut self, cg: &CharGen, style: i32) {
        let (n, ids, pal_sets) = self.gear_colors(cg, EPart::Footwear, style);
        self.num_footwear_colors = n;
        self.footwear_palette_template_ids = ids;
        self.footwear_pal_set_ids = pal_sets;
        if n > 0 {
            self.footwear_color = if self.policy == CreationPolicy::Classic {
                self.footwear_color.min(n - 1)
            } else {
                self.footwear_color.clamp(0, n - 1)
            };
        }
        self.footwear_style = style;
    }

    fn gear_colors(&self, cg: &CharGen, part: EPart, style: i32) -> (i32, Vec<u32>, Vec<DataId>) {
        if self.heritage_group == 0 || self.gender == 0 || style == -1 {
            return (0, Vec::new(), Vec::new());
        }
        let Some(sx) = self.sex(cg) else {
            return (0, Vec::new(), Vec::new());
        };
        let list = match part {
            EPart::Headgear => &sx.headgear,
            EPart::Shirt => &sx.shirts,
            EPart::Trousers => &sx.pants,
            EPart::Footwear => &sx.footwear,
        };
        let Some(item) = usize::try_from(style).ok().and_then(|i| list.get(i)) else {
            return (0, Vec::new(), Vec::new());
        };
        let colors = sx.clothing_colors.clone();
        self.store_color_information(self.clothing.get(&item.clothing_table), &colors)
    }

    /// The deterministic **floor** under the appearance randomiser,
    /// [`Self::randomize_appearance`].
    ///
    /// The reset and the gender constraint both drive every appearance index
    /// to **-1** and every shade to **-1.0**, and the retail client only ever leaves that state
    /// through the constructor's character randomisation. That runs here too, so this
    /// function is not the thing that fills the page -- it is the guard that catches an
    /// index a *narrower* heritage clamped back to -1, and it takes the **first** option rather
    /// than drawing, so that a repaint never advances a generator.
    ///
    /// It has to end on *something*: ACE dereferences `HairStyleList[HairStyle]`,
    /// `HairColorList[HairColor]`, `EyeColorList[EyeColor]`, `EyeStripList`, `NoseStripList`,
    /// `MouthStripList`, `ShirtList`, `PantsList` and `FootwearList` **unguarded**, so a -1 throws
    /// server-side and is answered with silence exactly as a template number of -1 is rather than
    /// sending an invalid template. Headgear keeps its -1: that value is "no hat" and is the one
    /// ACE reads as `HeadgearStyle == uint.MaxValue`.
    ///
    /// which option the retail client lands on is a draw from
    /// the client's own random-int, so index 0 is not what retail shows — it is a legal, reproducible
    /// choice, and every one of these fields is a *player* choice the appearance page can change.
    pub fn default_appearance(&mut self, cg: &CharGen) {
        if self.sex(cg).is_none() {
            return;
        }
        for (v, floor) in [
            (&mut self.hair_style, 0),
            (&mut self.eyes_strip, 0),
            (&mut self.nose_strip, 0),
            (&mut self.mouth_strip, 0),
            (&mut self.hair_color, 0),
            (&mut self.eye_color, 0),
        ] {
            if *v < floor {
                *v = floor;
            }
        }
        for shade in [
            &mut self.skin_shade,
            &mut self.hair_shade,
            &mut self.headgear_shade,
            &mut self.shirt_shade,
            &mut self.trousers_shade,
            &mut self.footwear_shade,
        ] {
            if !(0.0..=1.0).contains(shade) {
                *shade = 0.0;
            }
        }
        // The three that go through their own setters, so the colour count and the palette-template
        // list follow. Headgear is deliberately not in this list.
        for part in [EPart::Shirt, EPart::Trousers, EPart::Footwear] {
            let cur = match part {
                EPart::Shirt => self.shirt_style,
                EPart::Trousers => self.trousers_style,
                EPart::Footwear => self.footwear_style,
                EPart::Headgear => continue,
            };
            let v = if cur < 0 { 0 } else { cur };
            match part {
                EPart::Shirt => self.set_shirt_style(cg, v),
                EPart::Trousers => self.set_trousers_style(cg, v),
                EPart::Footwear => self.set_footwear_style(cg, v),
                EPart::Headgear => {}
            }
        }
        for (c, n) in [
            (&mut self.headgear_color, self.num_headgear_colors),
            (&mut self.shirt_color, self.num_shirt_colors),
            (&mut self.trousers_color, self.num_trousers_colors),
            (&mut self.footwear_color, self.num_footwear_colors),
        ] {
            if *c < 0 && n > 0 {
                *c = 0;
            }
        }
    }

    /// The appearance page's own writes, which are plain field stores in the client, with the wrap
    /// the arrows do.
    pub fn set_hair_style(&mut self, style: i32) {
        self.hair_style = style;
    }

    /// The store, then the face-palette update.
    pub fn set_hair_color(&mut self, color: i32) {
        self.hair_color = color;
    }

    /// The char-gen state's eye color write.
    pub fn set_eye_color(&mut self, color: i32) {
        self.eye_color = color;
    }

    /// The char-gen state's skin shade write.
    pub fn set_skin_shade(&mut self, shade: f64) {
        self.skin_shade = shade;
    }

    /// The char-gen state's hair shade write.
    pub fn set_hair_shade(&mut self, shade: f64) {
        self.hair_shade = shade;
    }

    /// Change an available skill's class, refunding its prior cost before charging the new class.
    /// Unavailable skills and unaffordable changes leave both the class and credits unchanged.
    pub fn set_skill_level(
        &mut self,
        cg: &CharGen,
        skills: &SkillTable,
        skill: u32,
        level: SkillAdvancementClass,
    ) {
        let Some(idx) = usize::try_from(skill)
            .ok()
            .filter(|i| *i < TOTAL_NUM_SKILLS)
        else {
            return;
        };
        if dereth_rules::chargen::default_skill_class(cg, skills, self.heritage_group, skill)
            == SkillAdvancementClass::Inactive
            || level == SkillAdvancementClass::Inactive
        {
            return;
        }
        let (trained, specialized) = Self::skill_costs(cg, skills, self.heritage_group, skill);
        let cost = |class| match class {
            SkillAdvancementClass::Trained => trained,
            SkillAdvancementClass::Specialized => specialized,
            _ => 0,
        };
        let remaining = self.remaining_skill_credits + cost(self.skill_levels[idx]) - cost(level);
        if remaining < 0 {
            return;
        }
        self.skill_levels[idx] = level;
        self.remaining_skill_credits = remaining;
    }

    /// The char-gen state's remaining-skill-credits update. Note the loop starts at **1**.
    pub fn update_remaining_skill_credits(&mut self, cg: &CharGen, skills: &SkillTable) {
        let used = dereth_rules::chargen::skill_credits_used(
            cg,
            skills,
            self.heritage_group,
            &self.skill_levels,
        );
        self.remaining_skill_credits = self.total_skill_credits - used;
    }

    /// Accept the summary's bounded nonempty edit, then format it through the supported narrow
    /// text conversion. The UI-facing length guard remains separate from name formatting.
    pub fn set_name(&mut self, s: &str) -> bool {
        if !dereth_rules::chargen::name_length_ok(s) {
            return false;
        }
        let units: Vec<u16> = s.encode_utf16().collect();
        let Ok(narrow) = dereth_primitives::text::ChatConversion::default().to_spstring(&units)
        else {
            return false;
        };
        self.name = dereth_rules::names::format_name(&narrow.bytes);
        true
    }

    /// The char-gen state's slot write.
    pub fn set_slot(&mut self, slot: i32) {
        self.slot = slot;
    }

    /// The char-gen state's start area write.
    pub fn set_start_area(&mut self, area: u32) {
        self.start_area = area;
    }

    /// The client's in-range colour check (`-1 < color < count`), four times.
    fn palette_template(color: i32, num: i32, ids: &[u32]) -> u32 {
        if color < num && color > -1 {
            usize::try_from(color)
                .ok()
                .and_then(|i| ids.get(i).copied())
                .unwrap_or(0)
        } else {
            0
        }
    }

    /// The char-gen state's char gen result read — everything above, packed.
    ///
    /// The name is trimmed at both ends by the summary page before it gets here.
    #[must_use]
    pub fn get_char_gen_result(&self) -> CharGenResultData {
        CharGenResultData {
            heritage_group: self.heritage_group,
            gender: self.gender,
            eyes_strip: self.eyes_strip,
            nose_strip: self.nose_strip,
            mouth_strip: self.mouth_strip,
            hair_color: self.hair_color,
            eye_color: self.eye_color,
            hair_style: self.hair_style,
            headgear_style: self.headgear_style,
            // The char-gen result maps each colour *index* through the per-part palette-template
            // array and writes **0** when the index is out of `0 .. count`. The four arrays are the
            // client's; without them every one of these would be the literal 0 the out-of-range
            // branch writes.
            headgear_color: Self::palette_template(
                self.headgear_color,
                self.num_headgear_colors,
                &self.headgear_palette_template_ids,
            ),
            shirt_style: self.shirt_style,
            shirt_color: Self::palette_template(
                self.shirt_color,
                self.num_shirt_colors,
                &self.shirt_palette_template_ids,
            ),
            trousers_style: self.trousers_style,
            trousers_color: Self::palette_template(
                self.trousers_color,
                self.num_trousers_colors,
                &self.trousers_palette_template_ids,
            ),
            footwear_style: self.footwear_style,
            footwear_color: Self::palette_template(
                self.footwear_color,
                self.num_footwear_colors,
                &self.footwear_palette_template_ids,
            ),
            skin_shade: self.skin_shade,
            hair_shade: self.hair_shade,
            headgear_shade: self.headgear_shade,
            shirt_shade: self.shirt_shade,
            trousers_shade: self.trousers_shade,
            footwear_shade: self.footwear_shade,
            template_num: self.template,
            strength: self.get(Attr::Strength),
            endurance: self.get(Attr::Endurance),
            coordination: self.get(Attr::Coordination),
            quickness: self.get(Attr::Quickness),
            focus: self.get(Attr::Focus),
            self_: self.get(Attr::Self_),
            slot: self.slot,
            // The client resolves this through a data-id-by-enum lookup — enum `0x10000003` for an
            // ordinary character, `0x10000090`/`0x10000091` for the two Olthoi and
            // `0x10000004`/`0x10000092`/`0x10000093` for the admin variants. It stays 0 here
            // because ACE ignores it outright ("//characterCreateInfo.ClassId;") and nothing in
            // this build has a reader; recorded rather than resolved.
            class_id: 0,
            skill_advancement_classes: self.skill_levels.iter().map(|s| *s as i32).collect(),
            name: self.name.clone(),
            start_area: self.start_area,
            is_admin: i32::from(self.create_as_admin),
            is_envoy: i32::from(self.create_as_envoy),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the "absolute remaining credits" switch, whose case labels are
    /// `1 strength, 2 endurance, 3 quickness, 4 coordination, 5 focus, 6 self`, and
    /// and the attribute balance, whose body tests them in the order strength, endurance,
    /// coordination, quickness, focus, self.
    #[test]
    fn the_attribute_enumeration_and_the_balance_order_are_not_the_same_order() {
        assert_eq!(Attr::Strength as i32, 1);
        assert_eq!(Attr::Endurance as i32, 2);
        assert_eq!(Attr::Quickness as i32, 3, "quickness is 3, not 4");
        assert_eq!(Attr::Coordination as i32, 4);
        assert_eq!(Attr::Focus as i32, 5);
        assert_eq!(Attr::Self_ as i32, 6);
        assert_ne!(Attr::ALL, Attr::BALANCE_ORDER);
        assert_eq!(Attr::BALANCE_ORDER[2], Attr::Coordination);
        assert_eq!(Attr::BALANCE_ORDER[3], Attr::Quickness);
        // Both orders are permutations of the same six.
        let mut a = Attr::ALL;
        let mut b = Attr::BALANCE_ORDER;
        a.sort_unstable();
        b.sort_unstable();
        assert_eq!(a, b);
    }

    fn budgeted(total: i32) -> CharGenState {
        CharGenState {
            total_atrb_credits: total,
            ..CharGenState::default()
        }
    }

    /// The default template ends in a skill-level reset, which needs the two tables. These are
    /// empty, so that tail is a no-op and only the attribute arm is under test.
    fn no_tables() -> (CharGen, SkillTable) {
        (
            CharGen {
                help_strings: Vec::new(),
                id: DataId(0),
                second_data_id: DataId(0),
                starter_areas: Vec::new(),
                hg_table_marker: 0,
                heritage_order: Vec::new(),
                heritage_groups: BTreeMap::new(),
            },
            SkillTable {
                id: DataId(0),
                buckets: 0,
                skills: BTreeMap::new(),
            },
        )
    }

    /// Oracle: the remaining-credits count, evaluated. With every attribute at the minimum and
    /// nothing locked, the answer is the budget minus six tens.
    #[test]
    fn remaining_credits_charge_the_minimum_for_every_attribute_but_the_excluded_and_the_locked() {
        let mut s = budgeted(330);
        assert_eq!(s.get_abs_remaining_credits(None), 330 - 60);
        // Raising one attribute only costs when it is the excluded one or is locked.
        s.set_raw(Attr::Strength, 100);
        assert_eq!(
            s.get_abs_remaining_credits(None),
            330 - 60,
            "not excluded, charged as 10"
        );
        assert_eq!(
            s.get_abs_remaining_credits(Some(Attr::Strength)),
            330 - 50 - 100
        );
        s.set_locked(Attr::Strength, true);
        assert_eq!(
            s.get_abs_remaining_credits(None),
            330 - 50 - 100,
            "locked, charged in full"
        );
    }

    /// Oracle: the client's attribute write and balance. Evaluated, not restated: the totals below
    /// are computed by running the code.
    #[test]
    fn raising_an_attribute_takes_the_points_from_the_others_and_never_from_the_locked() {
        let mut s = budgeted(330);
        {
            let (cg, sk) = no_tables();
            s.apply_default_template(&cg, &sk);
        }
        // Six attributes of 50 is exactly 300 of the 330; the default template stops there.
        assert_eq!(Attr::ALL.map(|a| s.get(a)), [50; 6]);
        assert_eq!(s.remaining_atrb_credits, 30);

        // Raise strength past the budget and the others pay for it.
        s.set_attribute(Attr::Strength, 100);
        assert_eq!(s.get(Attr::Strength), 100);
        let total: i32 = Attr::ALL.iter().map(|a| s.get(*a)).sum();
        assert!(total <= 330, "the budget is never exceeded, got {total}");
        assert!(Attr::ALL.iter().all(|a| s.get(*a) >= ATTR_MIN));

        // A locked attribute is never the one that pays.
        let mut s = budgeted(330);
        {
            let (cg, sk) = no_tables();
            s.apply_default_template(&cg, &sk);
        }
        s.set_locked(Attr::Self_, true);
        s.set_attribute(Attr::Strength, 100);
        assert_eq!(
            s.get(Attr::Self_),
            50,
            "a locked attribute keeps its points"
        );
        assert!(Attr::ALL.iter().map(|a| s.get(*a)).sum::<i32>() <= 330);

        // And an attribute cannot be raised at all with no credits left.
        let mut s = budgeted(60);
        assert_eq!(s.get_abs_remaining_credits(Some(Attr::Strength)), 0);
        assert_eq!(s.set_attribute(Attr::Strength, 40), ATTR_MIN, "refused");
    }

    /// The attribute balance uses a rotating static index, so consecutive calls spread the
    /// reduction". Two successive one-point overspends must not both come out of the same
    /// attribute.
    #[test]
    fn the_balance_start_index_rotates_between_calls() {
        let mut s = budgeted(330);
        {
            let (cg, sk) = no_tables();
            s.apply_default_template(&cg, &sk);
        }
        let before = Attr::ALL.map(|a| s.get(a));
        s.set_attribute(Attr::Focus, 81); // 30 free + 1 => exactly one point must come from elsewhere
        let first = Attr::ALL.map(|a| s.get(a));
        s.set_attribute(Attr::Focus, 82);
        let second = Attr::ALL.map(|a| s.get(a));
        let paid_first: Vec<usize> = (0..6).filter(|i| first[*i] < before[*i]).collect();
        let paid_second: Vec<usize> = (0..6).filter(|i| second[*i] < first[*i]).collect();
        assert_eq!(paid_first.len(), 1, "exactly one point was taken");
        assert_eq!(paid_second.len(), 1);
        assert_ne!(paid_first, paid_second, "the start index rotated");
    }

    /// Oracle: the summary page's name guard — a length below `0x22` is accepted — against a
    /// 33-byte buffer.
    #[test]
    fn the_name_limit_is_thirty_two_characters_and_empty_is_refused() {
        let mut s = CharGenState::default();
        assert!(
            !s.set_name(""),
            "empty is `text.length == 1`, which returns early"
        );
        assert!(s.set_name(&"a".repeat(32)));
        assert_eq!(s.name.len(), 32);
        assert!(!s.set_name(&"a".repeat(33)), "33 is `length >= 0x22`");
        assert_eq!(
            s.name.len(),
            32,
            "the rejected edit does not overwrite the accepted name"
        );
    }

    /// Oracle: the client's skill count ([`TOTAL_NUM_SKILLS`]) and ACE's `PlayerFactory.Create`
    /// (`if (SkillAdvancementClasses.Count != 55) return ClientServerSkillsMismatch`). The count is
    /// a wire contract and a mismatch **boots the account**.
    #[test]
    fn the_result_always_carries_exactly_fifty_five_skill_entries() {
        let s = CharGenState::default();
        let r = s.get_char_gen_result();
        assert_eq!(r.skill_advancement_classes.len(), 55);
        assert_eq!(TOTAL_NUM_SKILLS, 55);
        // A state with no heritage has run no skill-level reset, so every entry is the array's zero
        // — `INVALID` / `Inactive`, which is the value ACE `continue`s on.
        assert!(r.skill_advancement_classes.iter().all(|c| *c == 0));
    }

    /// Behaviour: chargen.name.accepted-edits-use-shared-narrow-formatting
    #[test]
    fn accepted_ascii_names_use_shared_formatting_without_overwriting_refused_edits() {
        let mut state = CharGenState::default();
        for (input, expected) in [
            ("probe walker", "Probe walker"),
            ("probe Walker", "Probe Walker"),
            ("  tEST123  IV  ", "Test IV"),
            ("mAcDonald", "MacDonald"),
            ("o'BRIEN-smith", "O'Brien-smith"),
        ] {
            assert!(state.set_name(input));
            assert_eq!(state.name, expected);
            assert_eq!(state.get_char_gen_result().name, expected);
        }
        assert!(state.set_name(&"a".repeat(32)));
        let accepted = state.name.clone();
        assert!(!state.set_name(""));
        assert!(!state.set_name(&"b".repeat(33)));
        assert_eq!(state.name, accepted);
    }

    /// Oracle: the verification-response table in the recovered character-creation behavior.
    #[test]
    fn the_verification_codes_are_the_documented_values() {
        assert_eq!(CgVerification::from_code(1), CgVerification::Ok);
        assert_eq!(CgVerification::from_code(3), CgVerification::NameInUse);
        assert_eq!(CgVerification::from_code(5), CgVerification::Corrupt);
        assert_eq!(CgVerification::from_code(6), CgVerification::DatabaseDown);
        assert_eq!(CgVerification::from_code(99), CgVerification::Undef);
        assert_eq!(
            CgVerification::Ok.error_string_id(),
            None,
            "success shows no dialog"
        );
        assert_eq!(
            CgVerification::Pending.error_string_id(),
            Some("ID_Character_Err_NameDBDown")
        );
        assert_eq!(
            CgVerification::Undef.error_string_id(),
            Some("ID_Character_Err_NameDBDown"),
            "code 0 is outside the verification table after unsigned subtraction of one"
        );
        assert_eq!(
            CgVerification::NameInUse.error_string_id(),
            Some("ID_Character_Err_NameReserved"),
            "the wizard's string, not the character-management screen's"
        );
    }
}
