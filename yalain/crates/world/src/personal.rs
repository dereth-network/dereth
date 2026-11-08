//! A player's own small requests: its aliases, inscribing an item, and the list of the skills it
//! has trained.
//!
//! **Aliases.** The client keeps the player's aliases itself and tells the server each one set or
//! removed; the server answers nothing and hands the table back with the options at the next
//! entry.
//!
//! **Inscribing.** A weapon, armor, a shield, jewelry or a tool the player holds takes an
//! inscription: the player is told it is inscribed; anything else is refused with a text. Nothing
//! else is sent.
//!
//! **Trained skills.** Asked, the server lists each skill the player trained (not those its race
//! gives it), one line each with the skill's experience and what it cost.

use auberean_script::args::Arg;
use auberean_script::library::Elem;
use auberean_script::native_object::{Item, NativeObject};
use yalain_net::{ConnId, ServerNet};
use yalain_store::Store;

use crate::calls::{Node, Value};
use crate::graph::{list, Builder};
use crate::progress::{self, long, nested, text_with, word};
use crate::{session, Server};

/// The requests, and the list's answer.
pub mod function {
    pub const SET_ALIAS: u32 = 0x8212_0011;
    pub const INSCRIBE: u32 = 0x8212_0002;
    pub const LIST_SKILLS: u32 = 0x8212_0052;
    pub const LIST_SKILLS_DONE: u32 = 0x8212_005E;
}

/// The script packages an item derives from to take an inscription.
pub const INSCRIBABLE: [&str; 9] = [
    "WeaponTemplate",
    "ArmorTemplate",
    "RareArmorTemplate",
    "ShieldTemplate",
    "JewelryTemplate",
    "ToolTemplate",
    "ButcheryToolTemplate",
    "MiningToolTemplate",
    "MapTemplate",
];

/// The longest inscription an item takes, in units of its text, the scripts' figure for every kind
/// that may be inscribed, unless its template names its own (2017: 26 asked for, the longest 39).
pub const MAX_INSCRIPTION: usize = 200;

/// The texts.
pub mod text {
    /// "You successfully inscribe the item." and "You cannot inscribe that.".
    pub const INSCRIBED: (u32, u32) = (0x764F_43B5, 0x2500_000B);
    pub const CANNOT_INSCRIBE: (u32, u32) = (0x8669_67B4, 0x2500_000B);
    /// "Your inscription is too long."
    pub const TOO_LONG: (u32, u32) = (0x72F8_3432, 0x2500_000B);
    /// The kind of text they are shown as.
    pub const SHOWN_AS: u32 = 8;
    /// One line of the trained skills: the skill, its experience, its credits, its hero credits.
    pub const SKILL_LINE: (u32, u32) = (0x44DD_C668, 0x2500_07C7);
    pub const SKILL: u32 = 0x9D06_6C50;
    pub const XP: u32 = 0xB140_0000;
    pub const CREDITS: u32 = 0xB263_AC49;
    pub const HERO_CREDITS: u32 = 0xB72C_506B;
}

/// A string value of a call.
fn string_of(v: &Value) -> Option<String> {
    match v.root() {
        Some(Node::Text(t)) => Some(t.clone()),
        _ => None,
    }
}

/// A player sets or removes an alias. The values come last parameter first: whether it is added,
/// the text, the alias.
pub fn set_alias<S: Store>(s: &mut Server<S>, world: ConnId, v: &[Value]) {
    let (Some(Value::Int(add)), Some(text), Some(alias)) = (
        v.first(),
        v.get(1).and_then(string_of),
        v.get(2).and_then(string_of),
    ) else {
        s.unhandled.note("an alias that does not read");
        return;
    };
    let kept = s.players.get_mut(&world).and_then(|p| {
        p.options
            .set_alias(&alias, &text, *add != 0)
            .then_some(p.character)
    });
    if let Some(character) = kept {
        s.saves.option_changed(character);
    } else {
        s.unhandled.note("an alias past what a character keeps");
    }
}

/// May an item of a template take an inscription?
#[must_use]
pub fn inscribable<S: Store>(s: &Server<S>, template: u32) -> bool {
    s.catalog
        .get(template)
        .is_some_and(|t| s.inscribable_packages.contains(&t.description.package))
}

/// A player inscribes an item. The values come last parameter first: the inscription, the item.
pub fn inscribe<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId, v: &[Value]) {
    let (Some(words), Some(Value::Long(item))) = (
        v.first().and_then(|x| match x.root() {
            Some(Node::TextInfo(t)) => Some(t.clone()),
            _ => None,
        }),
        v.get(1),
    ) else {
        s.unhandled.note("an inscription that does not read");
        return;
    };
    let template = s.things.get(*item).map_or(0, |t| t.template);
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let held = p.play.items.iter().any(|h| h.id == *item);
    let name = crate::player::world_name(&p.name, p.sex);
    let literal = words.literal.as_deref().unwrap_or_default();
    let longest = s
        .catalog
        .get(template)
        .and_then(|t| t.props.int("Item_MaxInscriptionLength"))
        .and_then(|n| usize::try_from(n).ok())
        .unwrap_or(MAX_INSCRIPTION);
    let said = if !held || !inscribable(s, template) {
        text::CANNOT_INSCRIBE
    } else if literal.len() > longest {
        text::TOO_LONG
    } else {
        let literal = String::from_utf16_lossy(literal);
        if let Some(t) = s.things.get_mut(*item) {
            t.inscription = Some((literal, name));
        }
        crate::world::keep_player(s, world);
        text::INSCRIBED
    };
    progress::private(
        net,
        world,
        &crate::chat::show_text(text::SHOWN_AS, progress::text(said)),
    );
}

/// A player asks for the skills it has trained: one line each, in the order it holds them.
pub fn list_skills<S: Store>(s: &mut Server<S>, net: &mut ServerNet, world: ConnId) {
    let Some(p) = s.players.get(&world) else {
        return;
    };
    let starting = s.start.skills.get(&p.race).cloned().unwrap_or_default();
    let skills = crate::skills::held(&starting, &p.play.skills);
    let lines: Vec<_> = skills
        .iter()
        .filter(|h| h.granted == 0)
        .map(|h| {
            let name = s
                .skill_table
                .skills
                .get(&h.skill)
                .map_or_else(Default::default, |d| progress::text(d.name));
            text_with(
                text::SKILL_LINE,
                vec![
                    nested(text::SKILL, name),
                    long(text::XP, 12, h.xp),
                    word(text::CREDITS, 6, h.cost),
                    word(text::HERO_CREDITS, 6, 0),
                ],
            )
        })
        .collect();
    // The list first (the root), its lines after it.
    let mut b = Builder::new();
    let first = b.next_id() + 1;
    let refs = (first..).take(lines.len()).map(Item::Ref).collect();
    b.native(session::native::REF_LIST, list(Elem::Ref, refs));
    for line in lines {
        b.native(session::native::TEXT, NativeObject::TextInfo(line));
    }
    let m = session::call(function::LIST_SKILLS_DONE, &[Arg::Object(b.graph())])
        .expect("a list of lines packs");
    progress::private(net, world, &m);
}
