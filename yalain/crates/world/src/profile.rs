//! Examination profiles: what examining an object shows.
//!
//! A profile is lines of text and numbers on tabs, each drawn by a layout element the client
//! names (a label, a value, a group monster's name), kept in tab order (in the order added within
//! one tab), only for the tabs the client asked for and the general ones, with every tab any line
//! belongs to noted. It repeats the request it answers.

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::{NativeObject, TextInfo, Variable, VariableValue};
use auberean_script::wire::Out;
use yalain_content::Facts;

use crate::examine::Vitals;
use crate::graph::Builder;
use crate::session;

/// The client function an examination is answered with.
pub const EXAMINATION_PROFILE: u32 = 0x820A_0009;

/// The native types of an examination's request and profile.
pub const REQUEST_TYPE: u16 = 154;
pub const PROFILE_TYPE: u16 = 153;

/// Kinds of examination request.
pub mod request {
    /// Stop examining.
    pub const STOP: u32 = 0;
    /// An object.
    pub const OBJECT: u32 = 0x4000_0002;
    /// A target the examiner can attack.
    pub const TARGET: u32 = 0x4000_000D;
}

/// The tabs a profile's lines belong to: the general tab, and the requirements tab.
pub const GENERAL: u32 = 2;
pub const REQUIREMENTS: u32 = 32;

/// The tabs a profile always keeps lines of, whatever the client asked for.
pub const ALWAYS_KEPT: u32 = 0x203;

/// How a line is drawn: as a label, as a value, as a group monster's name.
pub mod look {
    pub const LABEL: u32 = 0x33BE_4035;
    pub const VALUE: u32 = 0xBE5B_6513;
    pub const GROUP_NAME: u32 = 0xE978_8992;
    /// A quest creature's name (a dye plant's).
    pub const QUEST_NAME: u32 = 0x3800_42FF;
    /// The line saying an item binds.
    pub const BINDS: u32 = 0xE9BC_E65B;
}

/// The strings of profiles (string ids) and their tables.
pub mod line {
    /// The general strings' table.
    pub const TABLE: u32 = 0x2500_0011;
    pub const LEVEL: u32 = 0x2EA0_10CC;
    pub const HEALTH: u32 = 0x186A_A4E6;
    pub const HEALTH_VALUE: u32 = 0x2B64_885C;
    pub const VIGOR: u32 = 0x2DF3_5274;
    pub const VIGOR_VALUE: u32 = 0xAAE5_0F6D;
    pub const CHANCE_TO_HIT: u32 = 0x9E78_D75C;
    pub const CHANCE_VALUE: u32 = 0xD819_12CB;
    pub const KINGDOM: u32 = 0x13E8_95B3;
    pub const KINGDOM_POINTS: u32 = 0x5C9A_09DA;
    pub const QUEST_RATING: u32 = 0x6DCD_9462;
    pub const PLAYER_NAME: u32 = 0xABA0_10CC;
    pub const CANNOT_RECALL_PORTAL: u32 = 0x0C68_13BE;
    pub const RECALL_PORTAL: u32 = 0x0416_FB0D;
    pub const RECALL_SUMMON_PORTAL: u32 = 0xD94A_AC63;
    pub const LIFESTONE_RECALL: u32 = 0x0453_EA93;
    pub const REQUIREMENTS: u32 = 0x4F39_B658;
    pub const MINIMUM_LEVEL: u32 = 0x4ED2_0478;
    pub const IN_AN_ALLEGIANCE: u32 = 0x9D7D_54C6;
    pub const A_MONARCH: u32 = 0x8ADC_E384;
    /// The kingdoms' table, and the neutral kingdom's name.
    pub const KINGDOMS: u32 = 0x2500_000F;
    pub const NEUTRAL: u32 = 0x1268_95B3;
    /// A craft title: the crafts' table, the first rank, the first craft.
    pub const CRAFTS: u32 = 0x2500_0CED;
    pub const INITIATE: u32 = 0xAE24_982D;
    pub const ARMORMASTER: u32 = 0xC865_0E1E;
    /// An item's name composed of a prefix, its name and a suffix.
    pub const ITEM_NAMES: u32 = 0x2500_035F;
    pub const ITEM_NAME: u32 = 0x7556_141B;
    pub const ARMOR_VALUE: u32 = 0x495D_F672;
}

/// The variables of those strings.
pub mod var {
    pub const LEVEL: u32 = 0x1916_B459;
    pub const HEALTH: u32 = 0x3730_C018;
    pub const MAX_HEALTH: u32 = 0x2B60_9018;
    pub const VIGOR: u32 = 0xB6B1_4769;
    pub const MAX_VIGOR: u32 = 0xAAE1_1769;
    pub const CHANCE: u32 = 0xF662_0CE8;
    pub const KINGDOM: u32 = 0xBEE1_1D49;
    pub const KINGDOM_POINTS: u32 = 0x74C6_A499;
    pub const QUEST_RATING: u32 = 0x12CB_1C93;
    pub const NAME: u32 = 0x9D06_6C50;
    pub const CRAFT: u32 = 0x0926_A4C8;
    pub const PREFIX: u32 = 0xA14A_2800;
    pub const SUFFIX: u32 = 0xA13E_9D40;
    /// The item's name inside an effect's suffix.
    pub const SUFFIX_ITEM: u32 = 0x9B26_2000;
}

/// The race and sex line of a player (race 1 human, 2 Lugian, 4 Tumerok, 16 Empyrean, 8
/// Drudge).
#[must_use]
pub const fn race_line(race: u32, sex: u32) -> u32 {
    let female = sex == crate::chargen::FEMALE;
    match (race, female) {
        (1, false) => 0x29D7_8353,
        (1, true) => 0x45AD_0311,
        (2, false) => 0xB4E2_1437,
        (2, true) => 0x303A_E137,
        (4, false) => 0xA93C_CB01,
        (4, true) => 0xA835_7AEB,
        (16, false) => 0xAC16_2DA7,
        (16, true) => 0x7814_3EC4,
        _ => 0x8055_3003,
    }
}

/// A kingdom's name in the kingdoms' table, by the faction a template belongs to.
#[must_use]
pub const fn kingdom(faction: u32) -> u32 {
    match faction {
        1 => 0x1260_95B3,
        2 => 0x1278_95B3,
        4 => 0x1270_95B3,
        _ => line::NEUTRAL,
    }
}

/// One line of a profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Node {
    Int {
        order: u32,
        look: u32,
        value: i32,
    },
    Text {
        order: u32,
        look: u32,
        text: TextInfo,
    },
    Break {
        order: u32,
    },
    Tab {
        order: u32,
        count: i32,
    },
    /// An icon: its layers (layer, picture, color).
    Icon {
        order: u32,
        layers: Vec<[u32; 3]>,
    },
    /// A picture.
    Image {
        order: u32,
        picture: u32,
    },
    /// A countdown to a time on the game clock (its bits).
    Countdown {
        order: u32,
        look: u32,
        at: u64,
    },
}

impl Node {
    const fn order(&self) -> u32 {
        match self {
            Self::Int { order, .. }
            | Self::Text { order, .. }
            | Self::Break { order }
            | Self::Tab { order, .. }
            | Self::Icon { order, .. }
            | Self::Image { order, .. }
            | Self::Countdown { order, .. } => *order,
        }
    }
}

/// A profile being built.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Profile {
    /// The tabs the client asked for.
    pub asked: u32,
    pub nodes: Vec<Node>,
    /// Every tab any line belongs to.
    pub orders: u32,
}

/// A string reference to a table's string.
#[must_use]
pub fn string(table: u32, id: u32) -> TextInfo {
    TextInfo {
        string_id: id,
        table,
        ..TextInfo::default()
    }
}

/// A literal string.
#[must_use]
pub fn literal(text: &str) -> TextInfo {
    TextInfo {
        has_literal: 1,
        literal: Some(text.encode_utf16().collect()),
        ..TextInfo::default()
    }
}

/// A table's string with variables.
#[must_use]
pub fn with_vars(table: u32, id: u32, vars: Vec<Variable>) -> TextInfo {
    let mut t = string(table, id);
    for v in vars {
        t.set_variable(v);
    }
    t
}

/// A whole-number variable.
#[must_use]
pub fn number(id: u32, v: u32) -> Variable {
    Variable {
        id,
        kind: 2,
        value: VariableValue::Word(v),
    }
}

/// A string variable.
#[must_use]
pub fn nested(id: u32, t: TextInfo) -> Variable {
    Variable {
        id,
        kind: 9,
        value: VariableValue::Nested(Box::new(t)),
    }
}

impl Profile {
    #[must_use]
    pub fn new(asked: u32) -> Self {
        Self {
            asked,
            ..Self::default()
        }
    }

    fn add(&mut self, n: Node) {
        let order = n.order();
        self.orders |= order;
        if order & (self.asked | ALWAYS_KEPT) == 0 {
            return;
        }
        let at = self
            .nodes
            .iter()
            .position(|x| x.order() > order)
            .unwrap_or(self.nodes.len());
        self.nodes.insert(at, n);
    }

    pub fn text(&mut self, order: u32, look: u32, text: TextInfo) {
        self.add(Node::Text { order, look, text });
    }

    pub fn int(&mut self, order: u32, look: u32, value: i32) {
        self.add(Node::Int { order, look, value });
    }

    pub fn brk(&mut self, order: u32) {
        self.add(Node::Break { order });
    }

    pub fn tab(&mut self, order: u32, count: i32) {
        self.add(Node::Tab { order, count });
    }

    pub fn icon(&mut self, order: u32, layers: Vec<[u32; 3]>) {
        self.add(Node::Icon { order, layers });
    }

    pub fn image(&mut self, order: u32, picture: u32) {
        self.add(Node::Image { order, picture });
    }

    /// A countdown to a time on the game clock.
    pub fn countdown(&mut self, order: u32, look: u32, at: f64) {
        self.add(Node::Countdown {
            order,
            look,
            at: at.to_bits(),
        });
    }

    /// The profile's packed form, after the request it answers.
    #[must_use]
    pub fn packed(&self, request: &[u8]) -> Vec<u8> {
        let mut w = Out::new();
        w.bytes(request);
        w.u32(u32::try_from(self.nodes.len()).unwrap_or(0));
        for n in &self.nodes {
            match n {
                Node::Int { order, look, value } => {
                    w.u32(0x4000_0001);
                    w.u32(*order);
                    w.u32(*look);
                    w.u32(value.cast_unsigned());
                }
                Node::Text { order, look, text } => {
                    w.u32(0x4000_0004);
                    w.u32(*order);
                    w.u32(*look);
                    let _ = NativeObject::TextInfo(text.clone()).write(&mut w);
                }
                Node::Break { order } => {
                    w.u32(0x4000_0006);
                    w.u32(*order);
                }
                Node::Tab { order, count } => {
                    w.u32(0x4000_0007);
                    w.u32(*order);
                    w.u32(count.cast_unsigned());
                }
                Node::Image { order, picture } => {
                    w.u32(0x4000_0005);
                    w.u32(*order);
                    w.u32(*picture);
                }
                Node::Countdown { order, look, at } => {
                    w.u32(0x4000_0008);
                    w.u32(*order);
                    w.u32(*look);
                    w.u64(*at);
                }
                Node::Icon { order, layers } => {
                    w.u32(0x4000_0009);
                    w.u32(*order);
                    w.u32(u32::try_from(layers.len()).unwrap_or(0));
                    for l in layers {
                        for x in l {
                            w.u32(*x);
                        }
                    }
                }
            }
        }
        w.u32(self.orders);
        w.buf
    }
}

/// An examination request as the client packs it: its kind, the object, the tabs it asks for,
/// and the bytes as sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    pub kind: u32,
    pub object: u64,
    pub asked: u32,
    pub bytes: Vec<u8>,
}

fn word(b: &[u8], i: usize) -> u32 {
    b.get(i..i + 4)
        .map_or(0, |w| u32::from_le_bytes([w[0], w[1], w[2], w[3]]))
}

/// Read an examination request from its packed form.
#[must_use]
pub fn read_request(bytes: &[u8]) -> Request {
    let kind = word(bytes, 0);
    let object = u64::from(word(bytes, 12)) << 32 | u64::from(word(bytes, 8));
    let asked = match kind {
        request::OBJECT => word(bytes, 16),
        request::TARGET => word(bytes, 24),
        _ => 0,
    };
    Request {
        kind,
        object,
        asked,
        bytes: bytes.to_vec(),
    }
}

impl Request {
    /// The request as the profile repeats it: a request about a target carries the examiner's
    /// combat mode and its accuracy against it.
    #[must_use]
    pub fn answered(&self, mode: u32, skill: u32) -> Vec<u8> {
        let mut b = self.bytes.clone();
        if self.kind == request::TARGET && b.len() >= 24 {
            b[16..20].copy_from_slice(&mode.to_le_bytes());
            b[20..24].copy_from_slice(&skill.to_le_bytes());
        }
        b
    }
}

/// The answer to an examination: the profile after the request.
#[must_use]
pub fn message(request: &[u8], p: &Profile) -> ServerMessage {
    message_of(&p.packed(request))
}

/// The answer to an examination, from the profile's packed form (the request first).
#[must_use]
pub fn message_of(packed: &[u8]) -> ServerMessage {
    let mut b = Builder::new();
    b.native(
        PROFILE_TYPE,
        NativeObject::Packed {
            ty: PROFILE_TYPE.into(),
            bytes: packed.to_vec(),
        },
    );
    session::call(EXAMINATION_PROFILE, &[Arg::Object(b.graph())]).expect("a profile packs")
}

fn vitals_lines(p: &mut Profile, v: Vitals) {
    p.text(GENERAL, look::LABEL, string(line::TABLE, line::HEALTH));
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            line::HEALTH_VALUE,
            vec![
                number(var::HEALTH, v.health),
                number(var::MAX_HEALTH, v.max_health),
            ],
        ),
    );
    p.brk(GENERAL);
    p.text(GENERAL, look::LABEL, string(line::TABLE, line::VIGOR));
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            line::VIGOR_VALUE,
            vec![
                number(var::VIGOR, v.vigor),
                number(var::MAX_VIGOR, v.max_vigor),
            ],
        ),
    );
    p.brk(GENERAL);
    p.brk(GENERAL);
}

/// A summoned creature's lines below its own: its level, its kingdom (its summoner's, by faction),
/// its health and vigor, and on the stats tab its armor.
pub fn creature_lines(p: &mut Profile, level: i32, kingdom: u32, v: Vitals, armor: i32) {
    level_lines(p, level);
    kingdom_lines(p, kingdom);
    p.brk(GENERAL);
    p.text(GENERAL, look::LABEL, string(line::TABLE, line::HEALTH));
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            line::HEALTH_VALUE,
            vec![
                number(var::HEALTH, v.health),
                number(var::MAX_HEALTH, v.max_health),
            ],
        ),
    );
    p.brk(GENERAL);
    p.text(GENERAL, look::LABEL, string(line::TABLE, line::VIGOR));
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            line::VIGOR_VALUE,
            vec![
                number(var::VIGOR, v.vigor),
                number(var::MAX_VIGOR, v.max_vigor),
            ],
        ),
    );
    p.brk(GENERAL);
    p.brk(STATS);
    p.text(STATS, look::LABEL, string(line::TABLE, line::ARMOR_VALUE));
    p.int(STATS, look::VALUE, armor);
    p.brk(STATS);
}

fn level_lines(p: &mut Profile, level: i32) {
    p.text(
        GENERAL,
        look::LABEL,
        with_vars(
            line::TABLE,
            line::LEVEL,
            vec![number(var::LEVEL, level.cast_unsigned())],
        ),
    );
    p.int(GENERAL, look::VALUE, level);
    p.brk(GENERAL);
}

fn kingdom_lines(p: &mut Profile, faction: u32) {
    let k = string(line::KINGDOMS, kingdom(faction));
    p.text(
        GENERAL,
        look::LABEL,
        with_vars(
            line::TABLE,
            line::KINGDOM,
            vec![nested(var::KINGDOM, k.clone())],
        ),
    );
    p.text(GENERAL, look::LABEL, k);
    p.brk(GENERAL);
}

/// A player's profile as another player (or itself) sees it: its name, its first craft title,
/// its race and sex, its level, its kingdom (the neutral one for a player in none), its health
/// and vigor, its kingdom points and quest rating.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn player(
    asked: u32,
    world_name: &str,
    race: u32,
    sex: u32,
    level: i32,
    kingdom: Option<u32>,
    v: Vitals,
    quest_rating: u32,
) -> Profile {
    let mut p = Profile::new(asked);
    let name = || literal(world_name);
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            line::PLAYER_NAME,
            vec![nested(var::NAME, name())],
        ),
    );
    p.brk(GENERAL);
    let craft = with_vars(
        line::CRAFTS,
        line::ARMORMASTER,
        vec![nested(var::NAME, name())],
    );
    p.text(
        GENERAL,
        look::LABEL,
        with_vars(
            line::CRAFTS,
            line::INITIATE,
            vec![nested(var::CRAFT, craft), nested(var::NAME, name())],
        ),
    );
    p.brk(GENERAL);
    p.text(
        GENERAL,
        look::LABEL,
        string(line::TABLE, race_line(race, sex)),
    );
    p.brk(GENERAL);
    p.text(GENERAL, look::LABEL, literal(""));
    p.brk(GENERAL);
    level_lines(&mut p, level);
    kingdom_lines(&mut p, kingdom.unwrap_or(0));
    p.brk(GENERAL);
    vitals_lines(&mut p, v);
    p.text(
        GENERAL,
        look::LABEL,
        with_vars(
            line::TABLE,
            line::KINGDOM_POINTS,
            vec![number(var::KINGDOM_POINTS, 500)],
        ),
    );
    p.int(GENERAL, look::VALUE, 500);
    p.brk(GENERAL);
    p.text(
        GENERAL,
        look::LABEL,
        with_vars(
            line::TABLE,
            line::QUEST_RATING,
            vec![number(var::QUEST_RATING, quest_rating)],
        ),
    );
    p.int(GENERAL, look::VALUE, quest_rating.cast_signed());
    p.brk(GENERAL);
    p.brk(GENERAL);
    p
}

/// A monster's profile as a player who can attack it sees it: its name, description, level,
/// kingdom (if it belongs to one), health and vigor, and the chance to hit it.
#[must_use]
pub fn monster(asked: u32, f: &Facts, v: Vitals, chance: u32) -> Profile {
    let mut p = Profile::new(asked);
    let name = f.name.map_or_else(|| literal(""), |(s, t)| string(t, s));
    let name_look = if f.group {
        look::GROUP_NAME
    } else {
        look::VALUE
    };
    p.text(GENERAL, name_look, name);
    p.brk(GENERAL);
    let desc = f
        .description
        .map_or_else(|| literal(""), |(s, t)| string(t, s));
    p.text(GENERAL, look::LABEL, desc);
    p.brk(GENERAL);
    level_lines(&mut p, f.level.unwrap_or(1));
    if let Some(faction) = f.faction.filter(|x| [1, 2, 4].contains(x)) {
        kingdom_lines(&mut p, faction);
    }
    p.brk(GENERAL);
    vitals_lines(&mut p, v);
    p.text(
        GENERAL,
        look::LABEL,
        string(line::TABLE, line::CHANCE_TO_HIT),
    );
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            line::CHANCE_VALUE,
            vec![number(var::CHANCE, chance)],
        ),
    );
    p.brk(GENERAL);
    p
}

/// A person's profile: its name and its description.
#[must_use]
pub fn npc(asked: u32, name: TextInfo, f: &Facts) -> Profile {
    let mut p = Profile::new(asked);
    p.text(GENERAL, look::VALUE, name);
    p.brk(GENERAL);
    let desc = f
        .description
        .map_or_else(|| literal(""), |(s, t)| string(t, s));
    p.text(GENERAL, look::LABEL, desc);
    p.brk(GENERAL);
    p
}

/// What the requirements tab says of an object for monarchs alone.
pub fn monarch_lines(p: &mut Profile) {
    p.brk(REQUIREMENTS);
    p.text(
        REQUIREMENTS,
        look::LABEL,
        string(line::TABLE, line::REQUIREMENTS),
    );
    p.brk(REQUIREMENTS);
    p.tab(REQUIREMENTS, 0);
    p.text(
        REQUIREMENTS,
        look::LABEL,
        string(line::TABLE, line::IN_AN_ALLEGIANCE),
    );
    p.brk(REQUIREMENTS);
    p.tab(REQUIREMENTS, 0);
    p.tab(REQUIREMENTS, 0);
    p.text(
        REQUIREMENTS,
        look::VALUE,
        string(line::TABLE, line::A_MONARCH),
    );
    p.brk(REQUIREMENTS);
}

/// An item's name composed of its template's name with no prefix or suffix.
#[must_use]
pub fn item_name(f: &Facts) -> TextInfo {
    let name = f.name.map_or_else(|| literal(""), |(s, t)| string(t, s));
    with_vars(
        line::ITEM_NAMES,
        line::ITEM_NAME,
        vec![
            nested(var::PREFIX, literal("")),
            nested(var::SUFFIX, literal("")),
            nested(var::NAME, name),
        ],
    )
}

/// A made item's name: its template's name after the word its level gives it ("Crude Vine Shako").
#[must_use]
pub fn made_name(f: &Facts, level_word: u32) -> TextInfo {
    let name = || f.name.map_or_else(|| literal(""), |(s, t)| string(t, s));
    with_vars(
        line::ITEM_NAMES,
        line::ITEM_NAME,
        vec![
            nested(
                var::PREFIX,
                with_vars(
                    line::ITEM_NAMES,
                    level_word,
                    vec![nested(var::SUFFIX_ITEM, name())],
                ),
            ),
            nested(var::SUFFIX, literal("")),
            nested(var::NAME, name()),
        ],
    )
}

/// A treasure potion's or piece of jewelry's name: its template's name with its effect's suffix
/// ("Potion of Revival"), the effect's name table holding the suffix.
#[must_use]
pub fn potion_name(f: &Facts, effect_table: u32) -> TextInfo {
    named_for(f, literal(""), effect_table)
}

/// A treasure item's name: the word treasure gives it, its template's name and the suffix of the
/// effect it is named for.
#[must_use]
pub fn treasure_name(f: &Facts, effect_table: u32) -> TextInfo {
    use crate::inventory::treasure_name::PREFIX_TEXT;
    treasure_word_name(f, effect_table, PREFIX_TEXT.0)
}

/// A treasure item's name with the word it drew before it.
#[must_use]
pub fn treasure_word_name(f: &Facts, effect_table: u32, word: u32) -> TextInfo {
    use crate::inventory::treasure_name::PREFIX_TEXT;
    let name = f.name.map_or_else(|| literal(""), |(s, t)| string(t, s));
    let word = with_vars(PREFIX_TEXT.1, word, vec![nested(var::SUFFIX_ITEM, name)]);
    named_for(f, word, effect_table)
}

fn named_for(f: &Facts, prefix: TextInfo, effect_table: u32) -> TextInfo {
    let name = || f.name.map_or_else(|| literal(""), |(s, t)| string(t, s));
    with_vars(
        line::ITEM_NAMES,
        line::ITEM_NAME,
        vec![
            nested(var::PREFIX, prefix),
            nested(
                var::SUFFIX,
                with_vars(
                    effect_table,
                    crate::inventory::treasure_name::EFFECT_SUFFIX,
                    vec![nested(var::SUFFIX_ITEM, name())],
                ),
            ),
            nested(var::NAME, name()),
        ],
    )
}

/// A portal's recall line, by its flags: "You can recall and summon this portal.", "You can recall
/// this portal." or "You cannot recall or summon this portal." (2017: 500 portals examined, each
/// with the one line throughout).
#[must_use]
pub fn portal_line(flags: u32) -> u32 {
    use yalain_content::portal::{RECALL, SUMMON};
    if flags & RECALL == 0 {
        line::CANNOT_RECALL_PORTAL
    } else if flags & SUMMON != 0 {
        line::RECALL_SUMMON_PORTAL
    } else {
        line::RECALL_PORTAL
    }
}

/// A portal's or a lifestone's profile: its name, what may be recalled to it, its description,
/// and the level a player needs to use it.
#[must_use]
pub fn place(asked: u32, name: TextInfo, recall: u32, f: &Facts) -> Profile {
    let mut p = Profile::new(asked);
    p.text(GENERAL, look::VALUE, name);
    p.brk(GENERAL);
    p.brk(GENERAL);
    p.text(GENERAL, look::LABEL, string(line::TABLE, recall));
    p.brk(GENERAL);
    let desc = f
        .description
        .map_or_else(|| literal(""), |(s, t)| string(t, s));
    p.text(GENERAL, look::LABEL, desc);
    p.brk(GENERAL);
    match f.min_level {
        Some(level) => {
            p.brk(REQUIREMENTS);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                string(line::TABLE, line::REQUIREMENTS),
            );
            p.brk(REQUIREMENTS);
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                string(line::TABLE, line::MINIMUM_LEVEL),
            );
            p.int(REQUIREMENTS, look::VALUE, level);
            p.brk(REQUIREMENTS);
        }
        None => p.brk(GENERAL),
    }
    p
}

/// The chance a player of one level has to hit a monster of another, as this server rules it
/// (the 2017 servers' rule is not known): half, five more each level above the monster, five
/// less each level below, between 5 and 95.
#[must_use]
pub fn chance_to_hit(attacker: i32, target: i32) -> u32 {
    (50 + 5 * (attacker - target)).clamp(5, 95).cast_unsigned()
}

/// The strings of item profiles (in the general table unless said).
pub mod item_line {
    pub const VALUE_LABEL: u32 = 0x2F3E_4575;
    /// "Inscription by: ", and the player.
    pub const INSCRIPTION_BY: u32 = 0x8BB1_ECE2;
    pub const VALUE: u32 = 0xA828_186C;
    pub const WEAPON_STATS: u32 = 0xA4CC_8C3D;
    pub const SHIELD_STATS: u32 = 0x06C6_0F68;
    pub const TOOL_STATS: u32 = 0xEF43_9EA4;
    /// "Increases yields for each successful attempt by {}."
    pub const TOOL_YIELD: u32 = 0xC506_00E6;
    /// "Requires a minimum craft skill level of {} in:", and each craft skill's line.
    pub const CRAFT_LEVEL: u32 = 0x7386_AE0C;
    pub const CRAFT_SKILL: u32 = 0x2651_2F1C;
    pub const CRAFT_SKILL_MORE: u32 = 0x4357_AF0D;
    pub const CRAFT_SKILL_LAST: u32 = 0xC197_AF2D;
    pub const SHIELD_RATING: u32 = 0x71EF_5CCF;
    /// "There is {} available enchantment slot on this item.", and more than one.
    pub const SLOT: u32 = 0xD8FC_509F;
    pub const SLOTS: u32 = 0x90AA_F593;
    /// "This item can become harmonic."
    pub const HARMONIC: u32 = 0xA0CE_495E;
    /// "This item will bind itself to the first player that uses it in combat. ..."
    pub const BINDS: u32 = 0x1BB0_CDB6;
    /// "{The} {totem} has been imbued by a spellbinder and can be used to enchant {a} {kind}."
    pub const TOTEM_IMBUED: u32 = 0xD937_4C67;
    /// "The target item must have a minimum level of at least {}."
    pub const TOTEM_LEVEL: u32 = 0xFAC7_2EC9;
    /// "The target item must have an arcane lore of at least {}."
    pub const TOTEM_LORE: u32 = 0xB873_5FB6;
    /// "Permanent Enchantment Effect(s):"
    pub const EFFECTS_PERMANENT: u32 = 0xF397_0BCA;
    /// "Enchantment Effect(s):"
    pub const EFFECTS_ENCHANTMENT: u32 = 0xC4E5_ED4C;
    /// "Amount: ", and its value.
    pub const AMOUNT: u32 = 0x0CA6_14AD;
    pub const AMOUNT_VALUE: u32 = 0xCB4D_6AD4;
    pub const AMOUNT_POINTS: u32 = 0x8B6B_C049;
    pub const DAMAGE: u32 = 0x9B6C_373C;
    pub const DAMAGE_VALUE: u32 = 0xA862_1B86;
    pub const COMBAT_DELAY: u32 = 0x3B4D_51EA;
    pub const VIGOR_COST: u32 = 0x157A_4FEA;
    pub const ARMOR_STATS: u32 = 0x1B6A_40D9;
    pub const ARMOR_RATING: u32 = 0x8FC3_9362;
    pub const WORN_ON: u32 = 0x67C3_AEF9;
    pub const WHEN_USED: u32 = 0x1D45_A89A;
    pub const DESTROYED_ON_USE: u32 = 0x6235_0D68;
    pub const MAXIMUM_LEVEL: u32 = 0x127D_2F5D;
    /// "Must have skills:", then each skill by name.
    pub const SKILLS_NEEDED: u32 = 0xA83D_97BB;
    /// "Arcane Lore: " and the rating needed.
    pub const ARCANE_LORE: u32 = 0x378E_61A2;
    pub const RACES: u32 = 0xE25D_922D;
    pub const ONE_RACE: u32 = 0x9923_CC4A;
    pub const RACE_LIST: u32 = 0xD819_20B6;
    /// A stack's name: its plural name and how many (in the plurals' table).
    pub const PLURALS: u32 = 0x2500_0012;
    pub const STACK: u32 = 0xFE4D_81DA;
    /// The plural name a table holds when the item names none.
    pub const PLURAL_NAME: u32 = 0x3565_4074;
    /// The name and description of an item in its own table (an effect's in its own too).
    pub const NAME: u32 = 0xA860_7CEC;
    pub const DESCRIPTION: u32 = 0x6C45_9364;
    /// Who made an item.
    pub const CRAFTED_BY: u32 = 0x5976_18F5;
    /// An item's durability: now, of at most.
    pub const DURABILITY: u32 = 0xAC0F_9A1E;
    pub const DURABILITY_VALUE: u32 = 0x9CED_51BD;
    /// The headings of an item's effects: when used, when wielded, when worn, on a hit.
    pub const EFFECTS_USED: u32 = 0x5FD5_5D2F;
    pub const EFFECTS_WIELDED: u32 = 0x1ED0_DC0C;
    pub const EFFECTS_WORN: u32 = 0x3587_45A7;
    pub const EFFECTS_ON_HIT: u32 = 0xBF4E_91A0;
    /// An effect's name, and the spellcraft it is cast with.
    pub const EFFECT_NAME: u32 = 0x414C_08B6;
    pub const SPELLCRAFT: u32 = 0x416A_FF79;
}

/// The variables of item lines.
pub mod item_var {
    /// What a totem's enchantment goes on.
    pub const TOTEM_KIND: u32 = 0x9352_2CD0;
    /// The least a totem's target may have.
    pub const LEAST: u32 = 0xA516_8800;
    pub const VALUE: u32 = 0x0D06_6558;
    /// The player who inscribed it.
    pub const INSCRIBER: u32 = 0x77D6_A489;
    pub const DAMAGE_LOW: u32 = 0x9B26_7000;
    pub const DAMAGE_HIGH: u32 = 0x9B06_C000;
    pub const COUNT: u32 = 0x073E_ACEA;
    pub const RACE: u32 = 0x9D06_6C50;
    pub const REST: u32 = 0x9D16_C540;
    pub const PLURAL: u32 = 0x8615_ECB3;
    pub const QUANTITY: u32 = 0x9D56_6800;
    /// A durability now (its most is [`DAMAGE_HIGH`]'s variable).
    pub const DURABILITY: u32 = 0x34FC_9528;
    /// How many enchantment slots.
    pub const SLOTS: u32 = 0xC732_7D4A;
    /// A tool's yield, the craft skill it asks of.
    pub const YIELD: u32 = 0x5636_7D59;
    pub const CRAFT_LEVEL: u32 = 0x1916_B459;
    pub const CRAFT_SKILL: u32 = 0x272E_4CC9;
}

/// The looks an item's name is drawn with besides a plain value's: a rare item's, an item someone
/// made, an item with effects.
pub mod name_look {
    pub const RARE: u32 = 0x35DB_789D;
    pub const CRAFTED: u32 = 0x2264_9835;
    pub const ENCHANTED: u32 = 0x6005_FF6B;
}

/// The tab of an item's effects.
pub const EFFECTS: u32 = 0x10;

/// When an item's effect applies.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum When {
    /// On whoever uses the item.
    Used,
    /// On whoever wields or wears it.
    Equipped,
    /// On what it hits.
    Hit,
    /// On the item itself, always.
    Permanent,
    /// On the item itself for now: what its holder's auras hold on it.
    Enchantment,
}

/// One effect an item's profile lists: when it applies, its name's table (the effect's own, which
/// holds its name and description), and the spellcraft it is cast with when the profile shows one.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ItemEffect {
    pub when: When,
    pub table: u32,
    pub spellcraft: Option<i32>,
    /// Its amount, when it shows one.
    pub amount: Option<Amount>,
}

/// How little above a whole number a percentage is still shown as one: (0.3 of a float reads 30.000002
/// and shows 30; 0.8 taken from 1 reads 19.999998 and shows so.)
const WHOLE_AMOUNT: f32 = 0.001;

/// An effect's amount as a profile shows it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Amount {
    /// In hundredths of what it changes.
    Percent(f32),
    /// A whole number of points.
    Points(u32),
}

/// The template properties naming an item's effects and their spellcraft, by when they apply.
const EFFECT_PROPS: [(When, &str, &str); 3] = [
    (When::Used, "Usage_Effect", "_Spellcraft"),
    (When::Equipped, "Item_EquipperEffect", "Stat"),
    (When::Hit, "Item_TargetEffect", "Stat"),
];

/// What a profile needs of an effect: its name's table, whether a player is ever shown it, and
/// whether it shows the spellcraft it is cast with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EffectShown {
    pub table: u32,
    pub visible: bool,
    pub spellcraft_shown: bool,
}

impl EffectShown {
    /// From an effect's definition: shown to players when its flags say the client sees it (the
    /// lowest bit of its internal flags' high word); its spellcraft shown unless it happens at once
    /// or it is a change to qualities by the same amount whatever the spellcraft. None for an effect
    /// with no name.
    #[must_use]
    pub fn of(d: &crate::effect_defs::EffectDef) -> Option<Self> {
        use crate::effect_defs::Kind;
        const CLIENT_VISIBLE: u32 = 0x1;
        const INSTANTANEOUS: u32 = 0x4000_0000;
        let (_, table) = d.name?;
        let constant = match &d.kind {
            Kind::Numeric { magnitude, .. } => magnitude
                .0
                .windows(2)
                .all(|w| (w[0].1 - w[1].1).abs() < f32::EPSILON),
            _ => false,
        };
        Some(Self {
            table,
            visible: d.internal_flags & CLIENT_VISIBLE != 0,
            spellcraft_shown: d.internal_flags & INSTANTANEOUS == 0 && !constant,
        })
    }
}

/// The effects a template's properties give an item, in the order a profile lists them, each
/// named by `effect` (the effect's name in the template to how a profile shows it; none for an
/// effect the server does not know); effects players are never shown are left out.
#[must_use]
pub fn item_effects(
    props: &yalain_content::Props,
    effect: &mut dyn FnMut(&str) -> Option<EffectShown>,
) -> Vec<ItemEffect> {
    let mut out = Vec::new();
    for (when, name, spellcraft) in EFFECT_PROPS {
        for n in 1..=8 {
            let key = format!("{name}{n}");
            let Some(yalain_content::Prop::Text(e)) = props.get(&key) else {
                continue;
            };
            let Some(shown) = effect(e).filter(|x| x.visible) else {
                continue;
            };
            #[allow(clippy::cast_possible_truncation)]
            let sc = props
                .float(&format!("{key}{spellcraft}"))
                .map(|x| x as i32)
                .unwrap_or(0);
            out.push(ItemEffect {
                when,
                table: shown.table,
                spellcraft: shown.spellcraft_shown.then_some(sc),
                amount: None,
            });
        }
    }
    out
}

/// The tab of an item's stats.
pub const STATS: u32 = 8;

/// The tab an item's inscription is on.
pub const INSCRIPTION: u32 = 0x80;

/// The script classes whose start-up makes their items destroyed on use (`true`) or not
/// (`false`): an item is destroyed on use when the nearest of these its class derives from says
/// so. Keys, glyphs, potions, saddles, totems and some trophies are; coins and two trophies are
/// not.
pub const DESTROYED_ON_USE_KINDS: [(&str, bool); 15] = [
    ("KeyTemplate", true),
    ("GlyphTemplate", true),
    ("PotionTemplate", true),
    ("PotionTutorialTemplate", true),
    ("SaddleTemplate", true),
    ("Totem", true),
    ("CharmRatSmasher", true),
    ("ElixirBeetleSlayer", true),
    ("ElixirWaspBane", true),
    ("SealAcademy", true),
    ("MedallionOpening", true),
    ("Coin", false),
    ("HeartTyrantLiving", false),
    ("HeroSkillCreditToken", false),
    // A forager's food wears with each use instead (2017: 65 of 65 uses answered as not spent).
    ("FoodForaged", false),
];

/// Whether items of a package are destroyed on use by their kind: the nearest of
/// [`DESTROYED_ON_USE_KINDS`] the package derives from says so (`id_of` gives a class's package,
/// `derives(a, b)` whether package `a` is `b` or derives from it).
pub fn destroyed_on_use_kind(
    package: u32,
    id_of: impl Fn(&str) -> Option<u32>,
    derives: impl Fn(u32, u32) -> bool,
) -> bool {
    let of: Vec<(u32, bool)> = DESTROYED_ON_USE_KINDS
        .iter()
        .filter_map(|(n, on)| Some((id_of(n)?, *on)))
        .filter(|(k, _)| derives(package, *k))
        .collect();
    of.iter()
        .find(|(k, _)| of.iter().all(|(o, _)| derives(*k, *o)))
        .is_some_and(|(_, on)| *on)
}

/// Where armor is worn (its valid places' bits) and each place's name.
pub const WORN_PLACES: [(u32, u32); 5] = [
    (0x1, 0x11E3_EE99),
    (0x2, 0xE683_4CFC),
    (0x8, 0xD1C3_EE5C),
    (0x10, 0x1193_EE1D),
    (0x20, 0x2863_AF79),
];

/// The races' names, by their bits.
pub const RACE_NAMES: [(u32, u32); 5] = [
    (1, 0x7C8E_3F7D),
    (2, 0xFCDE_BE8E),
    (4, 0x890E_7CDE),
    (8, 0x204E_6E5C),
    (16, 0x6C89_BCFE),
];

/// What a tool's profile shows: what it adds to each use, the craft skill level it asks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToolView {
    pub yields: Option<u32>,
    pub craft: u32,
}

/// What an item's profile shows of it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ItemView {
    /// Its icon's layers (layer, picture, color); none: its picture.
    pub icon: Vec<[u32; 3]>,
    pub picture: Option<u32>,
    /// Its name as shown (a stack's with its count).
    pub name: Option<TextInfo>,
    /// Its value (the stack's).
    pub value: u32,
    pub description: Option<TextInfo>,
    /// A weapon's damage, its variance, its combat delay and vigor cost.
    pub weapon: Option<(i32, f32, i32, i32)>,
    /// Armor's rating, combat delay and the places it is worn on.
    pub armor: Option<(i32, i32, u32)>,
    pub destroyed_on_use: bool,
    /// It is treasure jewelry: its effects are worn.
    pub jewelry: bool,
    pub min_level: i32,
    pub max_level: i32,
    /// The races that may use it (bits; 0 for all).
    pub races: u32,
    /// The skills it takes to use it (their names), and the Arcane Lore.
    pub skills: Vec<TextInfo>,
    pub arcane_lore: i32,
    /// A rare item.
    pub rare: bool,
    /// The name of the player who made it.
    pub crafter: Option<String>,
    /// Its durability now and at most.
    pub durability: Option<(i32, i32)>,
    /// Its effects, in the order listed.
    pub effects: Vec<ItemEffect>,
    /// What a player inscribed on it, and the player's name.
    pub inscription: Option<(String, String)>,
    /// Its enchantment slots.
    pub slots: u32,
    /// It can be made harmonic.
    pub harmonic: bool,
    /// What it is as a tool.
    pub tool: Option<ToolView>,
    /// The names of the craft skills a tool's level asks of.
    pub tool_skills: Vec<TextInfo>,
    /// Its weapon stats are a shield's, with this rating.
    pub shield: Option<i32>,
    /// It binds to the first player that uses it in combat.
    pub binds: bool,
    /// Its own effects change its damage, its vigor cost: those lines are drawn as enchanted.
    pub damage_changed: bool,
    pub vigor_changed: bool,
    /// It is a totem: what it goes on, and the least level and Arcane Lore its target must have.
    pub totem: Option<TotemView>,
}

/// What a totem says of what it may enchant.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TotemView {
    pub kind: u32,
    pub least_level: Option<i32>,
    pub least_lore: Option<i32>,
}

impl ItemView {
    /// The look its name is drawn with: a rare item's; a crafted item's, unless it has effects
    /// when worn; an item with effects; else a plain value's.
    #[must_use]
    pub fn name_look(&self) -> u32 {
        let worn = self.armor.is_some() && self.weapon.is_none();
        if self.rare {
            name_look::RARE
        } else if self.binds {
            look::QUEST_NAME
        } else if self.crafter.is_some()
            && !(worn && self.effects.iter().any(|e| e.when == When::Equipped))
        {
            name_look::CRAFTED
        } else if !self.effects.is_empty() {
            name_look::ENCHANTED
        } else {
            look::VALUE
        }
    }
}

/// The facts the human starting weapons' classes give them: their names and descriptions in
/// their own tables (the bow's "Sapling Bow", the stave's "Apprentice Staff", the sword's "Rusty
/// Sword", as the client names them), value 1, damage 9 with variance 0.24, delay 70, vigor 4,
/// and the races that may use them.
#[must_use]
pub fn starting_weapon(template: u32) -> Option<ItemView> {
    let (table, races) = match template {
        0x4700_051B => (Some(0x2500_0184), 1),
        0x4700_051D => (Some(0x2500_0C88), 3),
        0x4700_051C => (Some(0x2500_1A66), 0),
        _ => return None,
    };
    Some(ItemView {
        name: table.map(|t| string(t, item_line::NAME)),
        value: 1,
        description: table.map(|t| string(t, item_line::DESCRIPTION)),
        weapon: Some((9, 0.24, 70, 4)),
        races,
        ..ItemView::default()
    })
}

impl ItemView {
    /// What a template's properties say of an item of it, with its count and value (an item's
    /// own value, else the template's times the count).
    #[must_use]
    pub fn of(props: &yalain_content::Props, quantity: u32, value: Option<u32>) -> Self {
        let text = |n: &str| match props.get(n) {
            Some(yalain_content::Prop::Str(id, table)) => Some(string(*table, *id)),
            _ => None,
        };
        #[allow(clippy::cast_possible_truncation)]
        let int = |n: &str| props.int(n).map(|v| v as i32);
        let unit = int("Item_Value").unwrap_or(0).max(0).cast_unsigned();
        let name = if quantity > 1 {
            let plural = text("PluralName").or_else(|| match props.get("Name") {
                Some(yalain_content::Prop::Str(_, table)) => {
                    Some(string(*table, item_line::PLURAL_NAME))
                }
                _ => None,
            });
            plural.map(|pl| {
                with_vars(
                    item_line::PLURALS,
                    item_line::STACK,
                    vec![
                        nested(item_var::PLURAL, pl),
                        number(item_var::QUANTITY, quantity),
                    ],
                )
            })
        } else {
            text("Name")
        };
        let weapon = int("Weapon_Damage").map(|d| {
            (
                d,
                props.float("Weapon_Variance").unwrap_or(0.0),
                int("Weapon_Speed").unwrap_or(0),
                int("Weapon_VigorCost").unwrap_or(0),
            )
        });
        let armor = int("Item_ArmorLevel").map(|a| {
            (
                a,
                int("Item_CombatDelay").unwrap_or(0),
                props.word("Item_ValidInventoryLocations").unwrap_or(0),
            )
        });
        let durability = int("Item_Durability_MaxLevel")
            .filter(|m| *m > 0)
            .map(|m| (int("Item_Durability_CurrentLevel").unwrap_or(m), m));
        Self {
            rare: props.flag("Item_IsRareItem"),
            durability,
            icon: Vec::new(),
            picture: props.word("Icon"),
            name,
            value: value.unwrap_or(unit.saturating_mul(quantity.max(1))),
            description: text("Description"),
            weapon,
            armor,
            destroyed_on_use: props.flag("Item_IsDestroyOnUse"),
            min_level: int("Usage_MinLevel").unwrap_or(0),
            max_level: int("Usage_MaxLevel").unwrap_or(0),
            races: props.word("Usage_RequiredRace").unwrap_or(0),
            arcane_lore: int("Usage_RequiredArcaneLore").unwrap_or(0),
            ..Self::default()
        }
    }
}

/// The races' list as a requirement line shows it: one race by name; more, the first and the
/// rest's list nested.
fn race_list(races: &[u32]) -> TextInfo {
    let name = |r: u32| string(line::TABLE, r);
    match races {
        [one] => with_vars(
            line::TABLE,
            item_line::ONE_RACE,
            vec![nested(item_var::RACE, name(*one))],
        ),
        [first, rest @ ..] => with_vars(
            line::TABLE,
            item_line::RACE_LIST,
            vec![
                nested(item_var::REST, race_list(rest)),
                nested(item_var::RACE, name(*first)),
            ],
        ),
        [] => literal(""),
    }
}

/// An inscription's lines, on their own tab: who wrote it, then its words (2017: 493 of 493 profiles
/// holding one, on 23 items).
pub fn inscription_lines(p: &mut Profile, said: &str, by: &str) {
    p.brk(INSCRIPTION);
    p.text(
        INSCRIPTION,
        look::LABEL,
        with_vars(
            line::TABLE,
            item_line::INSCRIPTION_BY,
            vec![nested(item_var::INSCRIBER, literal(by))],
        ),
    );
    p.text(INSCRIPTION, look::VALUE, literal(by));
    p.brk(INSCRIPTION);
    p.tab(INSCRIPTION, 0);
    p.text(INSCRIPTION, look::LABEL, literal(said));
    p.brk(INSCRIPTION);
}

/// An item's profile: its icon (or picture), its name, its value, its description; a weapon's or
/// armor's stats; what using it does to it, and what it takes to use; what is inscribed on it.
#[must_use]
pub fn item(asked: u32, v: &ItemView) -> Profile {
    use item_line as l;
    let mut p = Profile::new(asked);
    if !v.icon.is_empty() {
        p.icon(GENERAL, v.icon.clone());
    } else if let Some(pic) = v.picture {
        p.image(GENERAL, pic);
    }
    p.text(GENERAL, look::LABEL, literal(" "));
    if let Some(name) = &v.name {
        p.text(GENERAL, v.name_look(), name.clone());
    }
    p.brk(GENERAL);
    p.text(GENERAL, look::LABEL, string(line::TABLE, l::VALUE_LABEL));
    p.text(
        GENERAL,
        look::VALUE,
        with_vars(
            line::TABLE,
            l::VALUE,
            vec![number(item_var::VALUE, v.value)],
        ),
    );
    p.brk(GENERAL);
    p.text(
        GENERAL,
        look::LABEL,
        v.description.clone().unwrap_or_else(|| literal("")),
    );
    p.brk(GENERAL);
    if let Some(who) = &v.crafter {
        p.text(
            GENERAL,
            look::LABEL,
            with_vars(
                line::TABLE,
                l::CRAFTED_BY,
                vec![nested(var::NAME, literal(who))],
            ),
        );
        p.text(GENERAL, look::LABEL, literal(who));
        p.brk(GENERAL);
    }
    if v.slots > 0 {
        let line_of = if v.slots == 1 { l::SLOT } else { l::SLOTS };
        p.brk(GENERAL);
        p.text(
            GENERAL,
            look::LABEL,
            with_vars(line::TABLE, line_of, vec![number(item_var::SLOTS, v.slots)]),
        );
        p.brk(GENERAL);
        p.brk(GENERAL);
    }
    if let Some((now, most)) = v.durability {
        p.tab(STATS, 0);
        p.text(STATS, look::LABEL, string(line::TABLE, l::DURABILITY));
        p.text(
            STATS,
            look::VALUE,
            with_vars(
                line::TABLE,
                l::DURABILITY_VALUE,
                vec![
                    number(item_var::DURABILITY, now.max(0).cast_unsigned()),
                    number(item_var::DAMAGE_HIGH, most.max(0).cast_unsigned()),
                ],
            ),
        );
        p.brk(STATS);
    }
    let stat = |p: &mut Profile, label: u32| {
        p.tab(STATS, 0);
        p.text(STATS, look::LABEL, string(line::TABLE, label));
    };
    if let Some(tool) = v.tool {
        p.text(STATS, look::LABEL, string(line::TABLE, l::TOOL_STATS));
        p.brk(STATS);
        if let Some(n) = tool.yields {
            p.brk(STATS);
            p.tab(STATS, 0);
            p.text(
                STATS,
                look::LABEL,
                with_vars(line::TABLE, l::TOOL_YIELD, vec![number(item_var::YIELD, n)]),
            );
            p.brk(STATS);
        }
    }
    if let Some((damage, variance, delay, vigor)) = v.weapon {
        let heading = if v.shield.is_some() {
            l::SHIELD_STATS
        } else {
            l::WEAPON_STATS
        };
        p.text(STATS, look::LABEL, string(line::TABLE, heading));
        p.brk(STATS);
        let damage_line = |p: &mut Profile| {
            p.tab(STATS, 0);
            p.text(STATS, look::LABEL, string(line::TABLE, l::DAMAGE));
            #[allow(clippy::cast_precision_loss)]
            let low = dereth_primitives::num::floor_to_i32(damage as f32 * (1.0 - variance));
            p.text(
                STATS,
                if v.damage_changed {
                    name_look::ENCHANTED
                } else {
                    look::VALUE
                },
                with_vars(
                    line::TABLE,
                    l::DAMAGE_VALUE,
                    vec![
                        number(item_var::DAMAGE_LOW, low.max(0).cast_unsigned()),
                        number(item_var::DAMAGE_HIGH, damage.max(0).cast_unsigned()),
                    ],
                ),
            );
            p.brk(STATS);
        };
        // A shield's rating leads, and its damage comes last.
        if let Some(rating) = v.shield {
            stat(&mut p, l::SHIELD_RATING);
            p.int(STATS, look::VALUE, rating);
            p.brk(STATS);
        } else {
            damage_line(&mut p);
        }
        stat(&mut p, l::COMBAT_DELAY);
        p.int(STATS, look::VALUE, delay);
        p.brk(STATS);
        stat(&mut p, l::VIGOR_COST);
        p.int(
            STATS,
            if v.vigor_changed {
                name_look::ENCHANTED
            } else {
                look::VALUE
            },
            vigor,
        );
        p.brk(STATS);
        if v.shield.is_some() {
            damage_line(&mut p);
        }
        if v.harmonic {
            p.brk(STATS);
            p.text(STATS, look::LABEL, string(line::TABLE, l::HARMONIC));
            p.brk(STATS);
            p.brk(STATS);
        }
    }
    if let Some((rating, delay, places)) = v.armor {
        p.text(STATS, look::LABEL, string(line::TABLE, l::ARMOR_STATS));
        p.brk(STATS);
        stat(&mut p, l::ARMOR_RATING);
        p.int(STATS, look::VALUE, rating);
        p.brk(STATS);
        stat(&mut p, l::COMBAT_DELAY);
        p.int(STATS, look::VALUE, delay);
        p.brk(STATS);
        stat(&mut p, l::WORN_ON);
        p.brk(STATS);
        for (bit, name) in WORN_PLACES {
            if places & bit != 0 {
                p.tab(STATS, 0);
                p.tab(STATS, 0);
                p.text(STATS, look::VALUE, string(line::TABLE, name));
                p.brk(STATS);
            }
        }
    }
    totem_lines(&mut p, v);
    effect_lines(&mut p, v);
    if v.destroyed_on_use {
        p.brk(REQUIREMENTS);
        p.text(REQUIREMENTS, look::LABEL, string(line::TABLE, l::WHEN_USED));
        p.brk(REQUIREMENTS);
        p.brk(REQUIREMENTS);
        p.tab(REQUIREMENTS, 0);
        p.text(
            REQUIREMENTS,
            look::LABEL,
            string(line::TABLE, l::DESTROYED_ON_USE),
        );
        p.brk(REQUIREMENTS);
    }
    let races: Vec<u32> = RACE_NAMES
        .iter()
        .filter(|(bit, _)| v.races & bit != 0)
        .map(|(_, n)| *n)
        .collect();
    if v.min_level > 1
        || v.max_level > 0
        || !races.is_empty()
        || !v.skills.is_empty()
        || v.arcane_lore > 0
    {
        p.brk(REQUIREMENTS);
        p.text(
            REQUIREMENTS,
            look::LABEL,
            string(line::TABLE, line::REQUIREMENTS),
        );
        p.brk(REQUIREMENTS);
        if v.min_level > 1 {
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                string(line::TABLE, line::MINIMUM_LEVEL),
            );
            p.int(REQUIREMENTS, look::VALUE, v.min_level);
            p.brk(REQUIREMENTS);
        }
        if v.max_level > 0 {
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                string(line::TABLE, l::MAXIMUM_LEVEL),
            );
            p.int(REQUIREMENTS, look::VALUE, v.max_level);
            p.brk(REQUIREMENTS);
        }
        if !races.is_empty() {
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                with_vars(
                    line::TABLE,
                    l::RACES,
                    vec![number(
                        item_var::COUNT,
                        u32::try_from(races.len()).unwrap_or(0),
                    )],
                ),
            );
            p.text(REQUIREMENTS, look::VALUE, race_list(&races));
            p.brk(REQUIREMENTS);
        }
        if !v.skills.is_empty() {
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                string(line::TABLE, l::SKILLS_NEEDED),
            );
            p.brk(REQUIREMENTS);
            for name in &v.skills {
                p.tab(REQUIREMENTS, 0);
                p.tab(REQUIREMENTS, 0);
                p.text(REQUIREMENTS, look::VALUE, name.clone());
                p.brk(REQUIREMENTS);
            }
        }
        if v.arcane_lore > 0 {
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                string(line::TABLE, l::ARCANE_LORE),
            );
            p.int(REQUIREMENTS, look::VALUE, v.arcane_lore);
            p.brk(REQUIREMENTS);
        }
    }
    if let Some(tool) = v.tool {
        p.brk(REQUIREMENTS);
        p.text(
            REQUIREMENTS,
            look::LABEL,
            with_vars(
                line::TABLE,
                l::CRAFT_LEVEL,
                vec![number(item_var::CRAFT_LEVEL, tool.craft)],
            ),
        );
        p.brk(REQUIREMENTS);
        let many = v.tool_skills.len() > 1;
        for (i, name) in v.tool_skills.iter().enumerate() {
            let form = if !many {
                l::CRAFT_SKILL
            } else if i + 1 == v.tool_skills.len() {
                l::CRAFT_SKILL_LAST
            } else {
                l::CRAFT_SKILL_MORE
            };
            p.tab(REQUIREMENTS, 0);
            p.text(
                REQUIREMENTS,
                look::LABEL,
                with_vars(
                    line::TABLE,
                    form,
                    vec![nested(item_var::CRAFT_SKILL, name.clone())],
                ),
            );
            if i + 1 < v.tool_skills.len() {
                p.brk(REQUIREMENTS);
            }
        }
    }
    if v.binds {
        p.tab(REQUIREMENTS, 0);
        p.tab(REQUIREMENTS, 0);
        p.text(REQUIREMENTS, look::BINDS, string(line::TABLE, l::BINDS));
        p.brk(REQUIREMENTS);
    }
    if let Some((said, by)) = &v.inscription {
        inscription_lines(&mut p, said, by);
    }
    p
}

/// What a totem says of what it may enchant: the line naming it, each least its target must have.
fn totem_lines(p: &mut Profile, v: &ItemView) {
    use item_line as l;
    let Some(t) = v.totem else {
        return;
    };
    let mut vars = vec![nested(item_var::TOTEM_KIND, string(line::TABLE, t.kind))];
    if let Some(name) = &v.name {
        vars.push(nested(item_var::RACE, name.clone()));
    }
    p.text(
        STATS,
        look::LABEL,
        with_vars(line::TABLE, l::TOTEM_IMBUED, vars),
    );
    p.brk(STATS);
    for (id, least) in [
        (l::TOTEM_LEVEL, t.least_level),
        (l::TOTEM_LORE, t.least_lore),
    ] {
        if let Some(least) = least {
            let least = number(item_var::LEAST, least.max(0).cast_unsigned());
            p.text(STATS, look::LABEL, with_vars(line::TABLE, id, vec![least]));
            p.brk(STATS);
        }
    }
    p.brk(STATS);
}

/// An item's effects on their tab, by when they apply: a heading with how many, then each
/// effect's name, description and the spellcraft it is cast with when shown. A weapon's
/// equipped and hit effects head the tab after a break, unindented; armor's and jewelry's are worn,
/// after a break; anything else's are indented once, with no break.
fn effect_lines(p: &mut Profile, v: &ItemView) {
    use item_line as l;
    let weapon = v.weapon.is_some();
    let armor = v.armor.is_some();
    let mut first = true;
    for when in [
        When::Used,
        When::Permanent,
        When::Enchantment,
        When::Equipped,
        When::Hit,
    ] {
        let list: Vec<&ItemEffect> = v.effects.iter().filter(|e| e.when == when).collect();
        if list.is_empty() {
            continue;
        }
        let (heading, indent, after_break) = match when {
            When::Used => (l::EFFECTS_USED, 1, false),
            When::Equipped if (armor || v.jewelry) && !weapon => (l::EFFECTS_WORN, 1, true),
            When::Equipped if weapon && !armor => (l::EFFECTS_WIELDED, 0, true),
            When::Equipped if armor => (l::EFFECTS_WIELDED, 1, true),
            When::Equipped => (l::EFFECTS_WIELDED, 1, false),
            When::Hit => (l::EFFECTS_ON_HIT, 0, true),
            When::Permanent => (l::EFFECTS_PERMANENT, 0, true),
            When::Enchantment => (l::EFFECTS_ENCHANTMENT, 0, true),
        };
        if after_break && first {
            p.brk(EFFECTS);
        }
        first = false;
        p.tab(EFFECTS, indent);
        p.text(
            EFFECTS,
            look::LABEL,
            with_vars(
                line::TABLE,
                heading,
                vec![number(
                    item_var::COUNT,
                    u32::try_from(list.len()).unwrap_or(0),
                )],
            ),
        );
        p.brk(EFFECTS);
        for e in list {
            p.tab(EFFECTS, indent + 1);
            p.text(
                EFFECTS,
                look::VALUE,
                with_vars(
                    line::TABLE,
                    l::EFFECT_NAME,
                    vec![nested(var::NAME, string(e.table, l::NAME))],
                ),
            );
            p.brk(EFFECTS);
            p.tab(EFFECTS, indent + 1);
            p.text(EFFECTS, look::LABEL, string(e.table, l::DESCRIPTION));
            p.brk(EFFECTS);
            if let Some(sc) = e.spellcraft {
                p.tab(EFFECTS, indent + 1);
                p.text(EFFECTS, look::LABEL, string(line::TABLE, l::SPELLCRAFT));
                p.int(EFFECTS, look::VALUE, sc);
                p.brk(EFFECTS);
            }
            if let Some(amount) = e.amount {
                p.tab(EFFECTS, indent + 1);
                p.text(EFFECTS, look::LABEL, string(line::TABLE, l::AMOUNT));
                let (form, shown) = match amount {
                    Amount::Points(n) => (l::AMOUNT_POINTS, number(item_var::QUANTITY, n)),
                    Amount::Percent(n) if n - n.floor() < WHOLE_AMOUNT => (
                        l::AMOUNT_VALUE,
                        number(
                            item_var::QUANTITY,
                            dereth_primitives::num::to_i32(n).cast_unsigned(),
                        ),
                    ),
                    Amount::Percent(n) => (
                        l::AMOUNT_VALUE,
                        Variable {
                            id: item_var::QUANTITY,
                            kind: 4,
                            value: VariableValue::Double(f64::from(n).to_bits(), 2),
                        },
                    ),
                };
                p.text(
                    EFFECTS,
                    look::VALUE,
                    with_vars(line::TABLE, form, vec![shown]),
                );
                p.brk(EFFECTS);
            }
        }
    }
}
