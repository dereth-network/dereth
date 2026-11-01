//! Crafting as the skill window's crafting page and the ingredient panel ask the player's
//! scripts.
//!
//! - **The craft skills** the player has: the interface's list function fills a list of craft
//!   skill adaptors (each a craft skill's id, name, description, level as it stands, own level,
//!   the experience raising it costs, its icon and the factor of the experience its use earns); a
//!   craft skill's adaptor is asked on its own too. For each: its level (its own and as it stands),
//!   whether it may ever be raised, whether it may be raised now.
//! - **The recipes** of a craft skill: a list of recipe adaptors (each a recipe's id, name,
//!   description, charges, spinner bounds and its last value, whether its ingredients fill
//!   themselves in, how many it takes, its name's coloring, difficulty, the difficulty less the
//!   skill's cutoff when it has one, the time before it may be used again, its cost, whether it
//!   is shown, its icon, its craft skill and category, whether its spinner may be turned); a
//!   category's name; a recipe's adaptor on its own.
//! - **The experience** the player may spend on craft skills.
//!
//! [`Craft`] keeps them for the crafting page: asked again whenever the scripts tell the panels
//! the craft panel or a craft skill changed, and when the player arrives. What the player asks of
//! crafting (open a recipe, raise a craft skill, add an ingredient, make the recipe) is one
//! function of the player's scripts each.

use std::collections::BTreeMap;

use auberean_script::native_object::{native_type, Field, Item, NativeObject, TextInfo};

use crate::host::{CallResult, Scripts};
use crate::skills::{Asker, Slot};

/// The player's interface functions crafting calls, by id.
pub mod function {
    /// A craft skill's adaptor, into an out argument: 1, or 0.
    pub const CRAFT_SKILL: u32 = 0x00B4_011E;
    /// The craft skills, into a list.
    pub const CRAFT_SKILL_LIST: u32 = 0x00B4_011F;
    /// A craft skill's level, its own and as it stands, into two out arguments.
    pub const CRAFT_SKILL_LEVEL: u32 = 0x00B4_0120;
    /// A recipe's ingredients for a spinner value, into an out mask and a list.
    pub const RECIPE_INGREDIENTS: u32 = 0x00B4_0121;
    /// A recipe's difficulty and its name's color for a spinner value, into two out arguments.
    pub const RECIPE_DIFFICULTY: u32 = 0x00B4_0122;
    /// Open a recipe (the ingredient panel).
    pub const OPEN_RECIPE: u32 = 0x00B4_0123;
    /// The experience the player may spend on craft skills (64 bits).
    pub const AVAILABLE_CRAFT_EXPERIENCE: u32 = 0x00B4_0124;
    /// Whether an ingredient of a recipe fills itself in.
    pub const INGREDIENT_AUTO_POPULATES: u32 = 0x00B4_0125;
    /// Whether an item may be an ingredient of a recipe, into an out argument.
    pub const CHECK_INGREDIENT: u32 = 0x00B4_0126;
    /// A recipe's cost for a spinner value, into an out argument.
    pub const RECIPE_COST: u32 = 0x00B4_0127;
    /// Whether a craft skill may be raised now; ever.
    pub const CAN_CRAFT_SKILL_BE_RAISED: u32 = 0x00B4_0128;
    pub const IS_CRAFT_SKILL_RAISEABLE: u32 = 0x00B4_0129;
    /// Raise a craft skill.
    pub const RAISE_CRAFT_SKILL: u32 = 0x00B4_012A;
    /// A craft skill category's name, into an out argument.
    pub const CRAFT_SKILL_CATEGORY_NAME: u32 = 0x00B4_012B;
    /// A recipe's adaptor, into an out argument.
    pub const RECIPE: u32 = 0x00B4_009A;
    /// A craft skill's recipes, into a list.
    pub const RECIPE_LIST: u32 = 0x00B4_009B;
    /// Add an ingredient; make the recipe.
    pub const ADD_INGREDIENT: u32 = 0x00B4_009D;
    pub const EXECUTE_CRAFT: u32 = 0x00B4_009E;
}

/// The scripts' notices to the crafting panels.
pub mod notice {
    /// The craft panel changed.
    pub const UPDATE_CRAFT_PANEL: u32 = 0x0088 << 16 | 0x0001;
    /// An ingredient was added (the recipe, the ordinal, the item, whether it went).
    pub const ADD_INGREDIENT_DONE: u32 = 0x0088 << 16 | 0x0002;
    /// The recipe was made (or not).
    pub const EXECUTE_CRAFT_DONE: u32 = 0x0088 << 16 | 0x0003;
    /// A recipe's cost changed.
    pub const UPDATE_COST: u32 = 0x0088 << 16 | 0x0004;
    /// Open the ingredient panel on a recipe.
    pub const OPEN_INGREDIENT_PANEL: u32 = 0x0088 << 16 | 0x0005;
    /// A craft skill changed.
    pub const UPDATE_CRAFT_SKILL: u32 = 0x0088 << 16 | 0x0006;
}

/// The scripts' notices after which the recipe the ingredient panel has open is asked again: an
/// ingredient put in, the recipe made, a craft skill changed, the player's items changed.
pub const RECIPE_NOTICED: [u32; 6] = [
    notice::ADD_INGREDIENT_DONE,
    notice::EXECUTE_CRAFT_DONE,
    notice::UPDATE_CRAFT_SKILL,
    0x008B << 16 | 0x0001,
    0x008B << 16 | 0x0002,
    0x008B << 16 | 0x0004,
];

/// The native types of the crafting adaptors, and of a list of references.
pub const CRAFT_SKILL_ADAPTOR: u32 = 0x1AD4;
pub const RECIPE_ADAPTOR: u32 = 0xAA;
pub const INGREDIENT_ADAPTOR: u32 = 0xA5;
const REF_LIST: u32 = 0x65;

/// The fields an adaptor's setters wrote.
fn fields(o: &NativeObject) -> Option<&BTreeMap<String, Field>> {
    match o {
        NativeObject::Adaptor { fields, .. } => Some(fields),
        _ => None,
    }
}

/// A word field (a new adaptor's when never set).
fn word(f: &BTreeMap<String, Field>, ty: u32, name: &str) -> u32 {
    match f.get(name) {
        Some(Field::Word(v)) => *v,
        _ => auberean_glue::adaptors::new_word(ty, name),
    }
}

fn long(f: &BTreeMap<String, Field>, name: &str) -> u64 {
    match f.get(name) {
        Some(Field::Long(v)) => *v,
        _ => 0,
    }
}

fn text(f: &BTreeMap<String, Field>, name: &str) -> TextInfo {
    match f.get(name) {
        Some(Field::Value(v)) => match &**v {
            NativeObject::TextInfo(t) => t.clone(),
            _ => TextInfo::default(),
        },
        _ => TextInfo::default(),
    }
}

/// A craft skill as its adaptor gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CraftSkill {
    pub did: u32,
    pub name: TextInfo,
    pub description: TextInfo,
    /// Its level as it stands, and its own.
    pub level: u32,
    pub raw_level: u32,
    pub cost_to_raise: u64,
    pub icon: u32,
    /// The factor of the experience its use earns.
    pub usage_xp_mod: f32,
}

impl CraftSkill {
    /// From its adaptor.
    // D2-12352, D2-12353
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        let f = fields(o).cloned().unwrap_or_default();
        if !matches!(
            o,
            NativeObject::Adaptor { .. } | NativeObject::Opaque { .. }
        ) {
            return None;
        }
        let t = CRAFT_SKILL_ADAPTOR;
        Some(Self {
            did: word(&f, t, "DID"),
            name: text(&f, "Name"),
            description: text(&f, "Description"),
            level: word(&f, t, "Level"),
            raw_level: word(&f, t, "RawLevel"),
            cost_to_raise: long(&f, "CostToRaise"),
            icon: word(&f, t, "IconDID"),
            usage_xp_mod: f32::from_bits(word(&f, t, "UsageXPMod")),
        })
    }
}

/// A recipe as its adaptor gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Recipe {
    pub did: u32,
    pub name: TextInfo,
    pub description: TextInfo,
    /// -1 for a recipe without charges.
    pub charges: i32,
    pub min_spinner: i32,
    pub max_spinner: i32,
    pub last_spinner: i32,
    pub auto_populating: bool,
    pub max_ingredients: u32,
    pub name_coloring: bool,
    pub name_color: u32,
    pub difficulty: u32,
    pub dynamic_difficulty: bool,
    /// The difficulty less the skill's cutoff, when it has one.
    pub difficulty_minus_cutoff: Option<u32>,
    /// Seconds before it may be used again.
    pub time_use_again: f64,
    pub cost: i32,
    pub shown: bool,
    pub dynamic_cost: bool,
    pub icon: u32,
    pub craft_skill: u32,
    pub category: u32,
    pub read_only_spinner: bool,
}

impl Recipe {
    /// From its adaptor.
    // D2-12362, D2-12363
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        if !matches!(
            o,
            NativeObject::Adaptor { .. } | NativeObject::Opaque { .. }
        ) {
            return None;
        }
        let f = fields(o).cloned().unwrap_or_default();
        let t = RECIPE_ADAPTOR;
        let w = |n: &str| word(&f, t, n);
        Some(Self {
            did: w("DID"),
            name: text(&f, "Name"),
            description: text(&f, "Description"),
            charges: w("Charges").cast_signed(),
            min_spinner: w("MinSpinnerValue").cast_signed(),
            max_spinner: w("MaxSpinnerValue").cast_signed(),
            last_spinner: w("LastSpinnerValue").cast_signed(),
            auto_populating: w("HasAutoPopulatingIngredients") != 0,
            max_ingredients: w("MaximumNumberOfIngredients"),
            name_coloring: w("HasNameColoringTable") != 0,
            name_color: w("NameColor"),
            difficulty: w("Difficulty"),
            dynamic_difficulty: w("DynamicDifficulty") != 0,
            difficulty_minus_cutoff: match f.get("DifficultyMinusSkillCutoff") {
                Some(Field::Word(v)) => Some(*v),
                _ => None,
            },
            time_use_again: f64::from_bits(long(&f, "TimeUseAgain")),
            cost: w("Cost").cast_signed(),
            shown: w("Shown") != 0,
            dynamic_cost: w("DynamicCost") != 0,
            icon: w("IconDID"),
            craft_skill: w("CraftSkillDID"),
            category: w("CraftSkillCategory"),
            read_only_spinner: w("ReadOnlySpinner") != 0,
        })
    }
}

/// An ingredient as its adaptor gives it: its ordinal in the recipe, the recipe, what it is, and
/// whether it fills itself in.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Ingredient {
    pub id: u32,
    pub recipe: u32,
    pub description: TextInfo,
    pub auto_populate: bool,
}

impl Ingredient {
    /// From its adaptor.
    // D2-12366, D2-12367
    #[must_use]
    pub fn of(o: &NativeObject) -> Option<Self> {
        if !matches!(
            o,
            NativeObject::Adaptor { .. } | NativeObject::Opaque { .. }
        ) {
            return None;
        }
        let f = fields(o).cloned().unwrap_or_default();
        let t = INGREDIENT_ADAPTOR;
        Some(Self {
            id: word(&f, t, "ID"),
            recipe: word(&f, t, "RecipeDID"),
            description: text(&f, "Description"),
            auto_populate: word(&f, t, "AutoPopulate") & 0xFF != 0,
        })
    }
}

/// A craft skill with what the page asks of it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SkillFacts {
    /// As the list gives it, and as asked on its own (`None` when the scripts said no).
    pub listed: CraftSkill,
    pub own: Option<CraftSkill>,
    /// Its level: its own, and as it stands.
    pub raw_level: u32,
    pub level: u32,
    /// Whether it may ever be raised; whether it may be raised now.
    pub raiseable: bool,
    pub can_raise: bool,
    /// Its recipes, in the scripts' order.
    pub recipes: Vec<Recipe>,
}

/// What the crafting page shows, as last asked of the scripts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Craft {
    /// Counts each asking.
    pub version: u64,
    /// The craft skills, in the scripts' order.
    pub skills: Vec<SkillFacts>,
    /// The experience the player may spend on craft skills.
    pub experience: u64,
    /// Every recipe category's name (those the scripts named).
    pub categories: BTreeMap<u32, TextInfo>,
    /// The calls of the last asking that did not run to their end.
    pub incomplete: Vec<(u32, CallResult)>,
    /// The recipe the ingredient panel reads, at a spinner value, as last asked.
    pub opened: Option<RecipeAt>,
}

impl Craft {
    /// Everything asked again of the scripts: the craft skills, each one's own adaptor, level
    /// and whether it may be raised, its recipes and their categories' names; the experience.
    // D2-00440, D2-01183, D2-00439, D2-01074, D2-00445, D2-01110
    pub fn ask(&mut self, scripts: &mut Scripts) {
        self.version += 1;
        let mut a = Asker {
            s: scripts,
            incomplete: Vec::new(),
        };
        let listed = a.craft_skill_list();
        let mut skills = Vec::with_capacity(listed.len());
        let mut categories = BTreeMap::new();
        for skill in listed {
            let did = skill.did;
            let own = a.craft_skill(did);
            let (raw_level, level) = a.craft_skill_level(did);
            let raiseable = a.word_of(function::IS_CRAFT_SKILL_RAISEABLE, Some(did)) != 0;
            let can_raise = a.word_of(function::CAN_CRAFT_SKILL_BE_RAISED, Some(did)) != 0;
            let recipes = a.recipe_list(did);
            for r in &recipes {
                if let std::collections::btree_map::Entry::Vacant(e) = categories.entry(r.category)
                {
                    if let Some(n) = a.category_name(r.category) {
                        e.insert(n);
                    }
                }
            }
            skills.push(SkillFacts {
                listed: skill,
                own,
                raw_level,
                level,
                raiseable,
                can_raise,
                recipes,
            });
        }
        self.experience = a.long_of(function::AVAILABLE_CRAFT_EXPERIENCE, None);
        self.skills = skills;
        self.categories = categories;
        self.incomplete = a.incomplete;
    }
}

/// The adaptors a list the scripts filled holds, each read.
fn listed<T>(
    s: &Scripts,
    o: Option<NativeObject>,
    read: impl Fn(&NativeObject) -> Option<T>,
) -> Vec<T> {
    let Some(NativeObject::Sequence { items, .. }) = o else {
        return Vec::new();
    };
    items
        .iter()
        .filter_map(Item::reference)
        .filter_map(|r| s.vm.heap.native(r).ok().and_then(&read))
        .collect()
}

impl Asker<'_> {
    /// The craft skills (a new list in the slot; each adaptor it then holds read).
    // D2-00429, D2-01163, D2-01014, D2-01050
    fn craft_skill_list(&mut self) -> Vec<CraftSkill> {
        let a = self.ask_with(
            None,
            function::CRAFT_SKILL_LIST,
            &[Slot::New(REF_LIST)],
            &[],
            false,
        );
        let o = a.and_then(|a| a.objects.into_iter().next().flatten());
        listed(self.s, o, CraftSkill::of)
    }

    /// A craft skill's adaptor, when the scripts fill it.
    // D2-00428, D2-01160
    fn craft_skill(&mut self, did: u32) -> Option<CraftSkill> {
        let a = self.ask_with(
            None,
            function::CRAFT_SKILL,
            &[Slot::New(CRAFT_SKILL_ADAPTOR)],
            &[did],
            false,
        )?;
        if a.value == 0 {
            return None;
        }
        a.objects
            .into_iter()
            .next()
            .flatten()
            .and_then(|o| CraftSkill::of(&o))
    }

    /// A craft skill's level: its own, and as it stands.
    // D2-00430, D2-01162
    fn craft_skill_level(&mut self, did: u32) -> (u32, u32) {
        let Some(a) = self.ask_with(
            None,
            function::CRAFT_SKILL_LEVEL,
            &[Slot::Word(0), Slot::Word(0)],
            &[did],
            false,
        ) else {
            return (0, 0);
        };
        (a.outs[1], a.outs[0])
    }

    /// A craft skill's recipes.
    // D2-00432, D2-01175, D2-01016, D2-01052
    pub(crate) fn recipe_list(&mut self, skill: u32) -> Vec<Recipe> {
        let a = self.ask_with(
            None,
            function::RECIPE_LIST,
            &[Slot::New(REF_LIST)],
            &[skill],
            false,
        );
        let o = a.and_then(|a| a.objects.into_iter().next().flatten());
        listed(self.s, o, Recipe::of)
    }

    /// A recipe's adaptor, when the scripts fill it.
    // D2-00431, D2-01171
    pub(crate) fn recipe(&mut self, did: u32) -> Option<Recipe> {
        let a = self.ask_with(
            None,
            function::RECIPE,
            &[Slot::New(RECIPE_ADAPTOR)],
            &[did],
            false,
        )?;
        if a.value == 0 {
            return None;
        }
        a.objects
            .into_iter()
            .next()
            .flatten()
            .and_then(|o| Recipe::of(&o))
    }

    /// A category's name, when the scripts give one.
    // D2-00442, D2-01161
    fn category_name(&mut self, category: u32) -> Option<TextInfo> {
        let a = self.ask_with(
            None,
            function::CRAFT_SKILL_CATEGORY_NAME,
            &[Slot::New(native_type::STRING_INFO)],
            &[category],
            false,
        )?;
        if a.value == 0 {
            return None;
        }
        match a.objects.into_iter().next().flatten() {
            Some(NativeObject::TextInfo(t)) => Some(t),
            _ => Some(TextInfo::default()),
        }
    }
}

/// The native type of the table of a recipe's ingredients made for the scripts: item ids by
/// ordinal.
pub const INGREDIENT_TABLE: u32 = 0x08;

/// The packed form of the table of the items in a recipe's slots, by ordinal, as the interface
/// hands it to the scripts: a new table (47 buckets), each entry put at the head of its bucket in
/// the order given.
// D2-00443, D2-01207
#[must_use]
pub fn ingredient_table(ingredients: &[(u32, u64)]) -> Vec<u8> {
    use auberean_script::library::Elem;
    use auberean_script::native_object::{empty_buckets, Hash, NEW_TABLE_SIZE};
    let mut buckets: Vec<Vec<(Item, Item)>> = empty_buckets(NEW_TABLE_SIZE);
    let n = buckets.len();
    for (ordinal, item) in ingredients {
        let k = Item::Word(*ordinal);
        let b = k.bucket(n);
        buckets[b].insert(0, (k, Item::Long(*item)));
    }
    let table = NativeObject::Hash(Hash {
        key: Elem::Word,
        value: Elem::Long,
        size_index: NEW_TABLE_SIZE,
        buckets,
    });
    let mut w = auberean_script::wire::Out::new();
    // A table of words and 64-bit values always packs.
    let _ = table.write(&mut w);
    w.buf
}

/// The native type of a list of 64-bit values (the player's items).
const LONG_LIST: u32 = 0x3B;

/// A recipe as the ingredient panel reads it at a spinner value: the recipe, its craft skill's
/// level, the ingredients it takes there (a bit per slot number), its cost and its difficulty and
/// name's color there, whether each ingredient fills itself in, the player's items and which fit
/// which ingredient's slot (the player has the item and the recipe takes it there).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecipeAt {
    pub recipe: Recipe,
    pub spinner: i32,
    pub skill_level: u32,
    pub mask: u32,
    pub ingredients: Vec<Ingredient>,
    pub cost: Option<i32>,
    pub difficulty: Option<(u32, u32)>,
    pub auto_populate: BTreeMap<u32, bool>,
    pub items: Vec<u64>,
    pub fits: BTreeMap<(u32, u64), bool>,
}

impl Scripts {
    /// A recipe at a spinner value (its last one when none is given), every answer asked of the
    /// scripts as the ingredient panel asks; `None` when the scripts give no recipe.
    // D2-02224, D2-02228, D2-02231, D2-02232, D2-02218
    pub fn recipe_at(
        &mut self,
        did: u32,
        spinner: Option<i32>,
    ) -> Option<(RecipeAt, Vec<(u32, CallResult)>)> {
        let mut a = Asker {
            s: self,
            incomplete: Vec::new(),
        };
        let recipe = a.recipe(did)?;
        let spinner = spinner.unwrap_or(recipe.last_spinner);
        let skill_level = a.craft_skill(recipe.craft_skill).map_or(0, |s| s.level);
        let (mask, ingredients) = a.recipe_ingredients(did, spinner);
        let cost = a.recipe_cost(did, spinner);
        let difficulty = a.recipe_difficulty(did, spinner);
        let auto_populate = ingredients
            .iter()
            .map(|i| (i.id, a.auto_populates(did, i.id)))
            .collect();
        let items = a.inventory();
        let mut fits = BTreeMap::new();
        for item in &items {
            let have = a.have(*item);
            for i in &ingredients {
                let ok = have && a.check_ingredient(did, i.id, *item, spinner);
                fits.insert((i.id, *item), ok);
            }
        }
        let incomplete = a.incomplete;
        Some((
            RecipeAt {
                recipe,
                spinner,
                skill_level,
                mask,
                ingredients,
                cost,
                difficulty,
                auto_populate,
                items,
                fits,
            },
            incomplete,
        ))
    }
}

/// An argument's two slots of a 64-bit value.
fn long_args(v: u64) -> [(u32, u8); 2] {
    use auberean_script::value::tag;
    #[allow(clippy::cast_possible_truncation)]
    let (lo, hi) = (v as u32, (v >> 32) as u32);
    [(lo, tag::LONG | tag::MIXED), (hi, tag::LONG | tag::MIXED)]
}

impl Asker<'_> {
    /// A recipe's ingredients at a spinner value: the mask of their slots and each.
    // D2-00433, D2-01174, D2-01015, D2-01051
    fn recipe_ingredients(&mut self, did: u32, spinner: i32) -> (u32, Vec<Ingredient>) {
        let Some(a) = self.ask_with(
            None,
            function::RECIPE_INGREDIENTS,
            &[Slot::New(REF_LIST), Slot::Word(0)],
            &[spinner.cast_unsigned(), did],
            false,
        ) else {
            return (0, Vec::new());
        };
        let mask = a.outs[1];
        let o = a.objects.into_iter().next().flatten();
        (mask, listed(self.s, o, Ingredient::of))
    }

    /// A recipe's cost at a spinner value, when the scripts answer.
    // D2-00444, D2-01172
    #[allow(clippy::cast_possible_wrap)]
    fn recipe_cost(&mut self, did: u32, spinner: i32) -> Option<i32> {
        let a = self.ask_with(
            None,
            function::RECIPE_COST,
            &[Slot::Word(0)],
            &[spinner.cast_unsigned(), did],
            false,
        )?;
        (a.value != 0).then_some(a.outs[0] as i32)
    }

    /// A recipe's difficulty and its name's color at a spinner value, when the scripts answer.
    // D2-00434, D2-01173
    fn recipe_difficulty(&mut self, did: u32, spinner: i32) -> Option<(u32, u32)> {
        let a = self.ask_with(
            None,
            function::RECIPE_DIFFICULTY,
            &[Slot::Word(0), Slot::Word(0)],
            &[spinner.cast_unsigned(), did],
            false,
        )?;
        (a.value != 0).then_some((a.outs[1], a.outs[0]))
    }

    /// Whether an ingredient of a recipe fills itself in.
    // D2-00435, D2-01098
    fn auto_populates(&mut self, did: u32, ordinal: u32) -> bool {
        self.word_of_args(function::INGREDIENT_AUTO_POPULATES, &[ordinal, did]) != 0
    }

    /// The player's items, in the scripts' order.
    // D2-01159
    fn inventory(&mut self) -> Vec<u64> {
        let a = self.ask_with(None, INVENTORY_LIST, &[Slot::New(LONG_LIST)], &[], false);
        match a.and_then(|a| a.objects.into_iter().next().flatten()) {
            Some(NativeObject::Sequence { items, .. }) => items
                .iter()
                .filter_map(|i| match i {
                    Item::Long(v) => Some(*v),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Whether the player has an item.
    // D2-01096
    fn have(&mut self, item: u64) -> bool {
        self.ask_tagged(
            None,
            crate::interface::function::DO_I_HAVE_ITEM,
            &[],
            &long_args(item),
            false,
        )
        .is_some_and(|a| a.value & 0xFFFF_FFFF != 0)
    }

    /// Whether an item may go in a recipe's ingredient slot at a spinner value.
    // D2-00438, D2-01079
    fn check_ingredient(&mut self, did: u32, ordinal: u32, item: u64, spinner: i32) -> bool {
        use auberean_script::value::tag;
        let [lo, hi] = long_args(item);
        let args = [
            (spinner.cast_unsigned(), tag::MIXED),
            lo,
            hi,
            (ordinal, tag::MIXED),
            (did, tag::MIXED),
        ];
        self.ask_tagged(
            None,
            function::CHECK_INGREDIENT,
            &[Slot::Word(0)],
            &args,
            false,
        )
        .is_some_and(|a| a.outs[0] & 0xFF != 0)
    }

    /// A word a player's function answers, given its arguments in the order they are pushed.
    fn word_of_args(&mut self, function: u32, args: &[u32]) -> u32 {
        #[allow(clippy::cast_possible_truncation)]
        self.ask_with(None, function, &[], args, false)
            .map_or(0, |a| a.value as u32)
    }
}

/// The interface's function that lists the player's items.
pub const INVENTORY_LIST: u32 = 0x00B4_00E9;
