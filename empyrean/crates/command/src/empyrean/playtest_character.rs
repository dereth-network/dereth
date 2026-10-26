//! Playtest characters made from a recipe, applied only by the server console.
//! DIVERGE: V453. World builders can make a playtest character a believable player of a chosen
//! level, and put it back from a backup.
//!
//! A recipe is a level and what a player of it chose (an archetype's innate split, its skills,
//! how it shared its experience). The character is built from it with the data files' own
//! budget for the level (`dereth_progression`), so its experience and credits are those a player
//! of that level earned and every rank is one that experience bought. Before anything is written
//! the character is checked as the `verify-xp` and `verify-skill-credits` console commands check
//! a player, without skipping administrators, and refused if anything fails. Its gear is loot of
//! the tier it would hunt at (or the tier where it plays, when that is higher), each slot rolled
//! on its own until it can wear and wield it, or filled with a chosen item. What it carries must
//! fit its packs and be no more than it could pick up. Beyond its build it can hold augmentations
//! (their experience spent from its budget), luminance and auras, a summoning mastery, titles and
//! solved quests; the titles and quests are on its character record, which every backup keeps and
//! every restore puts back with the rest.
//! A new character is made by the server, as character creation makes one, of the heritage and
//! gender its recipe names. A recipe can be previewed: its gear rolled and its items made as an
//! apply makes them, and what it would make said, with no character made or changed (the same
//! recipe rolls the same gear). A character can be exported to a character file, which another
//! world can import (`character_file`).

pub mod create;
pub mod extras;
pub mod gear;
pub mod pack;

pub use gear::{tier_for_level, Wielder, ARMOR_SLOTS};

use dereth_progression::gear::{Slot, AMMUNITION};
use dereth_progression::order::{Gear, Made, MadeItem, Order, MADE_VERSION, ORDER_VERSION};
use dereth_progression::{check, Build, Sac, Sheet, SheetSkill, Tables};
use empyrean_entity::enums::{
    PropertyAttribute, PropertyAttribute2nd, PropertyInt, PropertyInt64, Skill,
    SkillAdvancementClass,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesAttribute2nd, PropertiesSkill};
use empyrean_entity::{Biota, ObjectGuid};
use std::collections::BTreeSet;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::{write_output_error, write_output_info};
use crate::handlers::developer_fix_commands::{
    get_additional_credits, get_heritage_aug, AUGMENTATION_DEVICES,
};
use empyrean_entity::enums::{AccessLevel, ChatMessageType, HeritageGroup, PropertyInstanceId};
use empyrean_net::SessionId;
use empyrean_store::adapter::BiotaConverter;
use empyrean_store::models::shard::Character;
use empyrean_store::{ShardDatabase, SqliteShard};
use empyrean_world::world_objects::world_object::WorldObject;
use empyrean_world::{entity::i_player::IPlayer, managers::player_manager, World};
use std::path::Path;

/// Console-only playtest character operations.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle);
    vec![CommandHandlerInfo { handler, handler_name, attribute: CommandHandlerAttribute::with_description(
        "playtest-character", AccessLevel::Admin, CommandHandlerFlag::ConsoleInvoke,
        "Make a playtest character from a recipe, keeping a character backup first unless told not to.",
        "create <account> <character> <recipe.json> <result.json> | apply <character> <recipe.json> <backup.sqlite or -> <result.json> [replace] | preview <recipe.json> <result.json> | restore <character> <backup.sqlite> <before-restore.sqlite> | export <character> <file.empyrean-character> [<note.json>]",
    ) }]
}

/// Run `f` over the tables a build is made from, as the server's data files hold them.
pub fn with_tables<R>(w: &World, f: impl FnOnce(Tables<'_>) -> R) -> R {
    let dats = std::sync::Arc::clone(&w.dats);
    let p = dats.portal_dat();
    f(Tables {
        xp: p.xp_table(),
        skills: p.skill_table(),
        chargen: p.char_gen(),
        vitals: p.secondary_attribute_table(),
    })
}

fn sac_of(s: Sac) -> SkillAdvancementClass {
    match s {
        Sac::Trained => SkillAdvancementClass::Trained,
        Sac::Specialized => SkillAdvancementClass::Specialized,
        Sac::Untrained => SkillAdvancementClass::Untrained,
        Sac::Undef => SkillAdvancementClass::Inactive,
    }
}

fn sac_from(s: SkillAdvancementClass) -> Sac {
    Sac::from_raw(s.0)
}

/// Replace `player`'s level, experience, credits, attributes, vitals and skills with `b`'s, and
/// its augmentations with its heritage's free one (the rest `extras::write` sets). Its vitals are
/// full.
///
/// # Errors
/// A value too large to store.
pub fn write_build(player: &mut Biota, t: &Tables<'_>, b: &Build) -> Result<(), String> {
    fn too_large<E>(_: E) -> String {
        "a value is too large to store".to_owned()
    }
    player.set_property(
        PropertyInt::Level,
        i32::try_from(b.budget.level).map_err(too_large)?,
    );
    player.set_property(
        PropertyInt64::TotalExperience,
        i64::try_from(b.budget.experience).map_err(too_large)?,
    );
    player.set_property(
        PropertyInt64::AvailableExperience,
        i64::try_from(b.unspent_xp).map_err(too_large)?,
    );
    player.try_remove_property(PropertyInt64::VerifyXp);
    player.set_property(
        PropertyInt::TotalSkillCredits,
        b.budget.credits() + b.quest_credits,
    );
    player.set_property(PropertyInt::AvailableSkillCredits, b.unspent_credits);
    let attributes = player.properties_attribute.get_or_insert_default();
    for a in &b.attributes {
        let key = PropertyAttribute(u16::try_from(a.id).map_err(too_large)?);
        attributes.insert(
            key,
            PropertiesAttribute {
                init_level: a.innate,
                level_from_cp: a.ranks,
                cp_spent: a.xp,
            },
        );
    }
    let vitals = player.properties_attribute_2nd.get_or_insert_default();
    for v in &b.vitals {
        let key = PropertyAttribute2nd(u16::try_from(v.id).map_err(too_large)?);
        let start = vitals.get(&key).map_or(0, |o| o.init_level);
        vitals.insert(
            key,
            PropertiesAttribute2nd {
                init_level: start,
                level_from_cp: v.ranks,
                cp_spent: v.xp,
                current_level: start + b.vital(t, v.id),
            },
        );
    }
    let skills = player.properties_skill.get_or_insert_default();
    let built: BTreeSet<u32> = b.skills.iter().map(|s| s.id).collect();
    let others: Vec<Skill> = skills
        .iter()
        .map(|(k, _)| *k)
        .filter(|k| u32::try_from(k.0).is_ok_and(|id| !built.contains(&id)))
        .collect();
    for k in others {
        skills.insert(
            k,
            PropertiesSkill {
                sac: SkillAdvancementClass::Untrained,
                ..Default::default()
            },
        );
    }
    for s in &b.skills {
        skills.insert(
            Skill(i32::try_from(s.id).map_err(too_large)?),
            PropertiesSkill {
                sac: sac_of(s.sac),
                init_level: s.start,
                level_from_pp: u16::try_from(s.ranks).map_err(too_large)?,
                pp: s.xp,
                ..Default::default()
            },
        );
    }
    for (aug, _) in AUGMENTATION_DEVICES.iter() {
        player.try_remove_property(*aug);
    }
    if let Some(aug) = b.heritage_augmentation {
        player.set_property(PropertyInt(u16::try_from(aug).map_err(too_large)?), 1);
    }
    Ok(())
}

/// `player` as the checks read a character: its stored numbers, and the experience its
/// augmentations cost (each one's gem weenie's cost, the heritage's free one aside), as
/// `verify-xp` adds them up.
#[must_use]
pub fn sheet_of(w: &World, player: &Biota) -> Sheet {
    let heritage = player.get_property(PropertyInt::HeritageGroup).unwrap_or(0);
    let free = get_heritage_aug(HeritageGroup(heritage));
    let mut augmentation_xp = 0_i64;
    for (aug, class) in AUGMENTATION_DEVICES.iter() {
        let mut n = player.get_property(*aug).unwrap_or(0);
        if Some(*aug) == free {
            n -= 1;
        }
        if n <= 0 {
            continue;
        }
        let cost = w
            .content
            .get_cached_weenie_by_class_name(class)
            .and_then(|g| {
                g.properties_int64
                    .as_ref()
                    .and_then(|p| p.get(&PropertyInt64::AugmentationCost).copied())
            })
            .unwrap_or(0);
        augmentation_xp += cost.wrapping_mul(i64::from(n));
    }
    let attributes = player
        .properties_attribute
        .as_ref()
        .map(|d| {
            dereth_progression::ATTRIBUTES
                .iter()
                .filter_map(|id| {
                    let a = d.get(&PropertyAttribute(u16::try_from(*id).ok()?))?;
                    Some((*id, a.init_level, a.level_from_cp, a.cp_spent))
                })
                .collect()
        })
        .unwrap_or_default();
    let vitals = player
        .properties_attribute_2nd
        .as_ref()
        .map(|d| {
            d.iter()
                .map(|(k, v)| (u32::from(k.0), v.level_from_cp, v.cp_spent))
                .collect()
        })
        .unwrap_or_default();
    let skills = player
        .properties_skill
        .as_ref()
        .map(|d| {
            d.iter()
                .filter_map(|(k, v)| {
                    Some(SheetSkill {
                        id: u32::try_from(k.0).ok()?,
                        sac: sac_from(v.sac),
                        start: v.init_level,
                        ranks: u32::from(v.level_from_pp),
                        xp: v.pp,
                    })
                })
                .collect()
        })
        .unwrap_or_default();
    Sheet {
        level: player
            .get_property(PropertyInt::Level)
            .and_then(|l| u32::try_from(l).ok())
            .unwrap_or(0),
        heritage: u32::try_from(heritage).unwrap_or(0),
        total_xp: player
            .get_property(PropertyInt64::TotalExperience)
            .unwrap_or(0),
        available_xp: player
            .get_property(PropertyInt64::AvailableExperience)
            .unwrap_or(0),
        total_credits: player
            .get_property(PropertyInt::TotalSkillCredits)
            .unwrap_or(0),
        available_credits: player
            .get_property(PropertyInt::AvailableSkillCredits)
            .unwrap_or(0),
        quest_credits: 0,
        attributes,
        vitals,
        skills,
        augmentation_xp,
        heritage_augmentation: free.map(|a| player.get_property(a).unwrap_or(0)),
        // A stored character's creation is not known: `verify-xp`'s own reading of it.
        creation_xp: None,
        augmentations: extras::held(player),
    }
}

/// Check `player` as a legitimate character: everything [`dereth_progression::check()`] finds,
/// and the skill credits `verify-skill-credits` expects of its level, with what its character
/// record's quests add (as the database holds the record).
///
/// # Errors
/// What was found.
pub fn verify(w: &World, player: &Biota) -> Result<(), String> {
    let record = w.shard.base_database().get_character(player.id);
    verify_sheet(w, sheet_with(w, player, record.as_ref()))
}

/// [`verify`], for a character made from `built` with the character record `record`: the
/// experience it holds beyond what it earned must be exactly its creation's free ranks.
///
/// # Errors
/// What was found.
pub fn verify_made(
    w: &World,
    player: &Biota,
    built: &Build,
    record: Option<&Character>,
) -> Result<(), String> {
    let mut sheet = sheet_with(w, player, record);
    sheet.creation_xp = Some(i64::try_from(built.creation_xp).map_err(|_| "too much experience")?);
    verify_sheet(w, sheet)
}

/// [`verify`], for a character whose character record is `record` (one read from a file, not
/// this world's database).
///
/// # Errors
/// What was found.
pub fn verify_with(w: &World, player: &Biota, record: Option<&Character>) -> Result<(), String> {
    verify_sheet(w, sheet_with(w, player, record))
}

/// `player`'s sheet with what its character record's quests add: the skill credits they granted,
/// and the experience of an augmentation held as a quest.
pub fn sheet_with(w: &World, player: &Biota, record: Option<&Character>) -> Sheet {
    let mut sheet = sheet_of(w, player);
    if let Some(r) = record {
        let (credits, xp) = extras::from_record(r);
        sheet.quest_credits = credits;
        sheet.augmentation_xp += xp;
    }
    sheet
}

fn verify_sheet(w: &World, sheet: Sheet) -> Result<(), String> {
    let mut found: Vec<String> = with_tables(w, |t| check(&t, &sheet))
        .into_iter()
        .filter(|f| !f.warning)
        .map(|f| f.text)
        .collect();
    let creation = with_tables(w, |t| {
        t.chargen
            .heritage_groups
            .get(&sheet.heritage)
            .map_or(0, |h| i32::try_from(h.skill_credits).unwrap_or(0))
    });
    let level = i32::try_from(sheet.level).unwrap_or(0);
    let expected = creation + get_additional_credits(level) + sheet.quest_credits;
    if sheet.total_credits != expected {
        found.push(format!(
            "It should have {expected} total skill credits, but has {}.",
            sheet.total_credits
        ));
    }
    if found.is_empty() {
        Ok(())
    } else {
        Err(found.join(" "))
    }
}

/// A backup contains the character and every nested possession, in the shard's own format.
pub(crate) fn rows(
    db: &mut dyn ShardDatabase,
    id: u32,
) -> Result<Vec<empyrean_store::models::shard::Biota>, String> {
    let player = db
        .get_biota(id, false)
        .ok_or("character has no stored state")?;
    let items = db.get_possessed_biotas_in_parallel(id);
    Ok(std::iter::once(player)
        .chain(items.inventory)
        .chain(items.wielded_items)
        .collect())
}

/// Atomically replace this character's possessions and statistics.
///
/// # Errors
/// A backup of another character, an object id another character holds, or a failed write;
/// nothing is changed then.
pub fn replace(db: &mut dyn ShardDatabase, id: u32, new: &[Biota]) -> Result<(), String> {
    replace_with(db, id, new, None)
}

/// [`replace`], and its character record (quests, titles, spell bars) with `record`, in the same
/// write.
///
/// # Errors
/// As [`replace`], or a record of another character; nothing is changed then.
pub fn replace_with(
    db: &mut dyn ShardDatabase,
    id: u32,
    new: &[Biota],
    record: Option<&Character>,
) -> Result<(), String> {
    if record.is_some_and(|r| r.id != id) {
        return Err("backup belongs to another character".into());
    }
    if new.first().is_none_or(|p| p.id != id) {
        return Err("backup belongs to another character".into());
    }
    let old = rows(db, id)?;
    let owned: BTreeSet<_> = old.iter().map(|b| b.id).collect();
    let mut ids = BTreeSet::new();
    for item in new {
        if !ids.insert(item.id)
            || (!owned.contains(&item.id) && db.get_biota(item.id, false).is_some())
        {
            return Err("a restored object id is already in use".into());
        }
    }
    db.begin_batch().map_err(|e| e.to_string())?;
    let result = (|| {
        for item in old.iter().skip(1) {
            if !db.remove_biota(item.id) {
                return Err("could not remove previous equipment".to_owned());
            }
        }
        for item in new {
            if !db.save_biota(&mut item.clone(), false) {
                return Err("could not save character equipment".to_owned());
            }
        }
        if let Some(r) = record {
            db.write_character(r).map_err(|e| e.to_string())?;
        }
        db.commit_batch().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        db.rollback_batch();
    }
    result
}

/// Save a recoverable character snapshot, its character record among it, without replacing an
/// existing recovery file.
///
/// # Errors
/// A file already there, or one that would not write.
pub fn backup(db: &mut dyn ShardDatabase, id: u32, path: &Path) -> Result<(), String> {
    let mut stored = rows(db, id)?;
    // Never replace a prior recovery file, even if a caller repeats its request.
    drop(
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?,
    );
    let record = db.get_character(id);
    let mut copy = SqliteShard::open(path).map_err(|e| e.to_string())?;
    copy.begin_batch().map_err(|e| e.to_string())?;
    for row in &mut stored {
        copy.write_biota(row).map_err(|e| e.to_string())?;
    }
    if let Some(r) = &record {
        copy.write_character(r).map_err(|e| e.to_string())?;
    }
    copy.commit_batch().map_err(|e| e.to_string())
}

/// The underclothes a character keeps under a new suit: worn items that cover only the shirt
/// and trouser slots.
const UNDERCLOTHES: u32 = 0x0000_00DE;

/// The character `name`, if it is offline with nothing waiting to be saved: its object id and
/// its stored state.
pub(crate) fn offline(w: &World, name: &str) -> Result<(u32, Biota), String> {
    let (found, _) = player_manager::find_by_name(w, name);
    let Some(IPlayer::Offline(guid)) = found else {
        return Err("character must exist and be logged out".into());
    };
    if w.shard.queue_count() != 0 || w.shard.in_transaction() {
        return Err("character saves are still pending; retry after they finish".into());
    }
    let id = guid.full();
    let biota = w
        .player_manager
        .offline_players
        .get(&id)
        .ok_or("character is unavailable")?
        .biota
        .clone();
    Ok((id, biota))
}

/// Write the new state: a backup of the old first (unless there is nothing to keep, a character
/// just made), then everything at once (the character record too, when there is a new one), then
/// the offline character the world holds.
fn commit(
    w: &mut World,
    id: u32,
    new: &[Biota],
    record: Option<&Character>,
    recovery: Option<&Path>,
) -> Result<(), String> {
    let mut db = w.shard.base_database();
    if let Some(recovery) = recovery {
        backup(&mut **db, id, recovery)?;
    }
    replace_with(&mut **db, id, new, record)?;
    drop(db);
    let offline = w
        .player_manager
        .offline_players
        .get_mut(&id)
        .ok_or("character is unavailable")?;
    offline.biota = new[0].clone();
    offline.changes_detected = false;
    Ok(())
}

fn restore(w: &mut World, name: &str, input: &Path, recovery: &Path) -> Result<(), String> {
    let (id, _) = offline(w, name)?;
    if !input.is_file() {
        return Err("backup does not exist".into());
    }
    let mut copy = SqliteShard::open(input).map_err(|e| e.to_string())?;
    let new = rows(&mut copy, id)?
        .iter()
        .map(|b| BiotaConverter::convert_to_entity_biota(b, false))
        .collect::<Vec<_>>();
    // A backup made before backups kept the record leaves the record as it is.
    let record = copy.get_character(id);
    drop(copy);
    commit(w, id, &new, record.as_ref(), Some(recovery))
}

/// How many of `item` there are: its stack's size, else one.
fn count_of(item: &Biota) -> u32 {
    item.get_property(PropertyInt::StackSize)
        .and_then(|n| u32::try_from(n).ok())
        .unwrap_or(1)
}

/// Put `item` in container `owner` at `index`, or worn by character `owner` at `slot`.
fn place(item: &mut Biota, owner: u32, slot: u32, index: usize) -> Result<(), String> {
    if slot == 0 {
        item.set_property(PropertyInstanceId::Container, owner);
        item.try_remove_property(PropertyInstanceId::Wielder);
        item.try_remove_property(PropertyInt::CurrentWieldedLocation);
        item.set_property(
            PropertyInt::PlacementPosition,
            i32::try_from(index).map_err(|_| "too many items")?,
        );
    } else {
        item.set_property(PropertyInstanceId::Wielder, owner);
        item.try_remove_property(PropertyInstanceId::Container);
        item.set_property(
            PropertyInt::CurrentWieldedLocation,
            i32::try_from(slot).map_err(|_| "an equipment location is too large")?,
        );
    }
    Ok(())
}

/// The recipe in `input`.
///
/// # Errors
/// A file that does not read, an old template, or a recipe of another version or too many items.
pub fn read_order(input: &Path) -> Result<Order, String> {
    let text = std::fs::read(input).map_err(|e| e.to_string())?;
    let order: Order = serde_json::from_slice(&text).map_err(|e| {
        if text.windows(12).any(|w| w == b"\"attributes\"") {
            "the recipe is an old template; open the world in Lyceum to upgrade it".to_owned()
        } else {
            e.to_string()
        }
    })?;
    if order.version != ORDER_VERSION {
        return Err(format!("unsupported recipe version {}", order.version));
    }
    if order.items.len() > 64 {
        return Err("a recipe can carry at most 64 items".into());
    }
    Ok(order)
}

/// The build `order` makes of a character of `heritage`, when the world can hold it: its level
/// within the world's, every skill it trains one of the world's era, and its augmentations, auras
/// and quests ones it can hold, their experience spent from the budget.
///
/// # Errors
/// What the world cannot hold.
pub fn checked_build(w: &World, order: &Order, heritage: u32) -> Result<Build, String> {
    let max_level = empyrean_world::world_objects::player_xp::get_max_level(w);
    if order.level > max_level {
        return Err(format!("the level must be from 1 to {max_level}"));
    }
    let costs = extras::costs(w);
    let (built, found) = with_tables(w, |t| {
        dereth_progression::extras::build(
            &t,
            order.level,
            heritage,
            &order.plan,
            &order.extras,
            &costs,
        )
    })?;
    let refused: Vec<String> = found
        .into_iter()
        .filter(|f| !f.warning)
        .map(|f| f.text)
        .collect();
    if !refused.is_empty() {
        return Err(refused.join(" "));
    }
    if let Some(allowed) = w.era.creation_skills {
        if let Some(s) = built
            .chosen_skills()
            .iter()
            .find(|s| !allowed.contains(&s.id))
        {
            return Err(format!("{} is not a skill of this world's era", s.name));
        }
    }
    Ok(built)
}

/// Make the character `name` from the recipe in `input`, keeping a backup of it in `recovery`
/// first (none for a character just made), and say what was made in `result`. Unless
/// `replacing`, the character must be one a recipe made, as `result` says. A recipe of another
/// heritage or gender gives the character a new body, as creation makes one; the clothes and
/// things that body is made with are what it keeps.
///
/// # Errors
/// Why nothing was changed.
pub fn apply_recipe(
    w: &mut World,
    name: &str,
    input: &Path,
    recovery: Option<&Path>,
    result: &Path,
    replacing: bool,
) -> Result<Made, String> {
    let order = read_order(input)?;
    let (id, old) = offline(w, name)?;
    if !replacing {
        let made: Option<Made> = std::fs::read(result)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok());
        if made.is_none_or(|m| m.id != id) {
            return Err(
                "this character was not made from a recipe; say replace to overwrite it".into(),
            );
        }
    }
    let of = |p: PropertyInt| old.get_property(p).and_then(|h| u32::try_from(h).ok());
    let was = (
        of(PropertyInt::HeritageGroup).unwrap_or(dereth_progression::heritage::ALUVIAN),
        of(PropertyInt::Gender).unwrap_or(dereth_progression::gender::MALE),
    );
    let body = (
        order.heritage.unwrap_or(was.0),
        order.gender.unwrap_or(was.1),
    );
    let built = checked_build(w, &order, body.0)?;
    // Another heritage or gender: the body creation makes of it, where the old one stood.
    let mut remade = None;
    let mut player = old;
    if body != was {
        let account = w
            .player_manager
            .offline_players
            .get(&id)
            .and_then(|o| o.account.clone())
            .ok_or("the character has no account")?;
        let bare = name.trim_start_matches('+');
        let mut made = create::body(
            w,
            &account,
            ObjectGuid::new(id),
            bare,
            body,
            order.plan.innate,
        )?;
        let mut fresh = made.player.biota.clone();
        fresh
            .properties_position
            .clone_from(&player.properties_position);
        player = fresh;
        let character = made.character().clone();
        remade = Some((create::possessions(w, &mut made), character));
    }
    with_tables(w, |t| write_build(&mut player, &t, &built))?;
    let mut record = w.shard.base_database().get_character(id);
    if let Some((_, character)) = &remade {
        // The new body's hair, which the character record holds.
        let r = record.as_mut().ok_or("the character record is gone")?;
        r.hair_texture = character.hair_texture;
        r.default_hair_texture = character.default_hair_texture;
    }
    extras::write(
        w,
        &mut player,
        record.as_mut(),
        order.level,
        &built,
        &order.extras,
    )?;
    verify_made(w, &player, &built, record.as_ref())?;
    let Given {
        tier,
        worn,
        ammunition,
        carried,
    } = given(w, &order, &built)?;

    // With no gear given, what it has it keeps; with gear, only the underclothes it wore.
    let kept: Vec<Biota> = {
        let all: Vec<Biota> = if let Some((made, _)) = &remade {
            made.clone()
        } else {
            let mut db = w.shard.base_database();
            let possessed = db.get_possessed_biotas_in_parallel(id);
            let all = possessed.inventory.iter().chain(&possessed.wielded_items);
            all.map(|b| BiotaConverter::convert_to_entity_biota(b, false))
                .collect()
        };
        all.into_iter()
            .filter(|b| {
                let at = b
                    .get_property(PropertyInt::CurrentWieldedLocation)
                    .map_or(0, i32::cast_unsigned);
                order.gear == Gear::None || (at != 0 && at & !UNDERCLOTHES == 0)
            })
            .collect()
    };
    let on: Vec<&Biota> = worn
        .iter()
        .map(|x| &x.item.biota)
        .chain(ammunition.iter().map(|a| &a.biota))
        .collect();
    let load = pack::load_of(&player, &built, &on, &kept, &carried);
    let refused: Vec<String> = dereth_progression::load::findings(&load)
        .into_iter()
        .filter(|f| !f.warning)
        .map(|f| f.text)
        .collect();
    if !refused.is_empty() {
        return Err(refused.join(" "));
    }
    let mut made = Made {
        version: MADE_VERSION,
        hash: order.hash.clone(),
        name: name.trim_start_matches('+').to_owned(),
        id,
        level: order.level,
        tier,
        items: Vec::new(),
        burden: load.burden,
        capacity: load.capacity,
    };
    let in_pack = u32::try_from(
        kept.iter()
            .filter(|b| {
                b.get_property(PropertyInt::CurrentWieldedLocation)
                    .is_none()
            })
            .count(),
    )
    .unwrap_or(u32::MAX);
    let mut new = vec![player];
    new.extend(kept);
    let wielded = worn
        .into_iter()
        .map(|x| (x.item, x.at, Some(x.slot)))
        .chain(ammunition.map(|a| (a, AMMUNITION, None)));
    for (item, at, slot) in wielded {
        let mut item = item.biota;
        place(&mut item, id, at, 0)?;
        made.items
            .push(with_tables(w, |t| made_item(&t, &item, at, slot)));
        new.push(item);
    }
    for (mut item, container, index) in pack::pack(id, in_pack, carried) {
        place(&mut item, container, 0, index)?;
        made.items
            .push(with_tables(w, |t| made_item(&t, &item, 0, None)));
        new.push(item);
    }
    commit(w, id, &new, record.as_ref(), recovery)?;
    let text = serde_json::to_vec_pretty(&made).map_err(|e| e.to_string())?;
    std::fs::write(result, text).map_err(|e| e.to_string())?;
    Ok(made)
}

/// What a character built as `built` is given by `order`: the tier its gear is rolled at, what it
/// wears and wields, the ammunition its missile weapon shoots (worn ready), and what it carries.
struct Given {
    tier: u32,
    worn: Vec<gear::Worn>,
    ammunition: Option<WorldObject>,
    carried: Vec<WorldObject>,
}

/// Roll and make what `order` gives a character built as `built`: an apply and a preview of the
/// same order make the same things.
///
/// # Errors
/// A tier that is not one, a chosen item that cannot be worn where it is asked for, or an item
/// the world lacks.
fn given(w: &mut World, order: &Order, built: &Build) -> Result<Given, String> {
    let tier = match order.gear {
        Gear::Tier(t) if !(1..=8).contains(&t) => {
            return Err(format!("tier {t} is not from 1 to 8"))
        }
        g => dereth_progression::gear::tier(
            g,
            order.place_tier.filter(|t| (1..=8).contains(t)),
            tier_for_level(w, order.level),
        ),
    };
    let dats = std::sync::Arc::clone(&w.dats);
    let portal = dats.portal_dat();
    let who = Wielder {
        build: built,
        tables: Tables {
            xp: portal.xp_table(),
            skills: portal.skill_table(),
            chargen: portal.char_gen(),
            vitals: portal.secondary_attribute_table(),
        },
    };
    let worn = gear::outfit(w, &who, order, tier)?;
    let mut carried = pack::given(w, order)?;
    if order.gear == Gear::Starter {
        let mut starter = pack::starter(w, built);
        starter.append(&mut carried);
        carried = starter;
    }
    let ammunition = pack::ammunition(&worn, &mut carried);
    Ok(Given {
        tier,
        worn,
        ammunition,
        carried,
    })
}

/// What a server says it made of `item`, worn at `at` (0 in the pack) to fill `slot`.
fn made_item(t: &Tables<'_>, item: &Biota, at: u32, slot: Option<Slot>) -> MadeItem {
    MadeItem {
        name: gear::name(item),
        class: item.weenie_class_id,
        slot: at,
        wield: if at == 0 {
            String::new()
        } else {
            gear::wield_text(t, item)
        },
        count: count_of(item),
        gear: slot,
    }
}

/// What the recipe in `input` would make of a character, said in `result` as an apply says what
/// it made, with no character made or changed: the build it is checked as, its gear rolled and its
/// items made as an apply makes them, and what they weigh against what it can carry. No character
/// is named, so the name is empty and the id 0; the clothes a body is made with are not counted.
///
/// # Errors
/// What an apply of it would refuse.
pub fn preview(w: &mut World, input: &Path, result: &Path) -> Result<Made, String> {
    let order = read_order(input)?;
    let heritage = order
        .heritage
        .unwrap_or(dereth_progression::heritage::ALUVIAN);
    let built = checked_build(w, &order, heritage)?;
    let Given {
        tier,
        worn,
        ammunition,
        carried,
    } = given(w, &order, &built)?;
    // A body holding what the build holds: the augmentations the load reads.
    let mut body = Biota::default();
    for (p, n) in &built.held {
        body.set_property(
            PropertyInt(u16::try_from(*p).map_err(|_| "no such property")?),
            *n,
        );
    }
    let on: Vec<&Biota> = worn
        .iter()
        .map(|x| &x.item.biota)
        .chain(ammunition.iter().map(|a| &a.biota))
        .collect();
    let load = pack::load_of(&body, &built, &on, &[], &carried);
    let refused: Vec<String> = dereth_progression::load::findings(&load)
        .into_iter()
        .filter(|f| !f.warning)
        .map(|f| f.text)
        .collect();
    if !refused.is_empty() {
        return Err(refused.join(" "));
    }
    let mut made = Made {
        version: MADE_VERSION,
        hash: order.hash.clone(),
        name: String::new(),
        id: 0,
        level: order.level,
        tier,
        items: Vec::new(),
        burden: load.burden,
        capacity: load.capacity,
    };
    with_tables(w, |t| {
        for x in &worn {
            made.items
                .push(made_item(&t, &x.item.biota, x.at, Some(x.slot)));
        }
        if let Some(a) = &ammunition {
            made.items.push(made_item(&t, &a.biota, AMMUNITION, None));
        }
        for (item, _, _) in pack::pack(0, 0, carried) {
            made.items.push(made_item(&t, &item, 0, None));
        }
    });
    let text = serde_json::to_vec_pretty(&made).map_err(|e| e.to_string())?;
    std::fs::write(result, text).map_err(|e| e.to_string())?;
    Ok(made)
}

/// The backup an apply keeps: at `arg`, or none for `-`.
fn backup_arg(arg: &str) -> Option<&Path> {
    (arg != "-").then(|| Path::new(arg))
}

/// Handle only the local console; network callers cannot name host files.
pub fn handle(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    if session.is_some() {
        write_output_error(
            w,
            session,
            "Playtest characters can only be changed on the console.",
            ChatMessageType::Broadcast,
        );
        return;
    }
    if let [op, account] = parameters {
        if op == "account" {
            let busy = w.net.find_by_account(account).is_some();
            write_output_info(
                w,
                None,
                if busy {
                    "Playtest account: busy"
                } else {
                    "Playtest account: ready"
                },
                ChatMessageType::Broadcast,
            );
            return;
        }
    }
    let path = Path::new;
    if let [op, input, result] = parameters {
        if op == "preview" {
            match preview(w, path(input), path(result)) {
                Ok(_) => write_output_info(
                    w,
                    None,
                    "Playtest character previewed.",
                    ChatMessageType::Broadcast,
                ),
                Err(e) => write_output_error(
                    w,
                    None,
                    &format!("Playtest character not previewed: {e}"),
                    ChatMessageType::Broadcast,
                ),
            }
            return;
        }
    }
    if let Some((op, rest)) = parameters.split_first() {
        if op == "export" {
            let answer = match rest {
                [name, out] => super::character_file::export(w, name, path(out), None),
                [name, out, note] => super::character_file::export(w, name, path(out), Some(path(note))),
                _ => Err("use playtest-character export <character> <file.empyrean-character> [<note.json>]".into()),
            };
            match answer {
                Ok(None) => write_output_info(
                    w,
                    None,
                    "Playtest character exported.",
                    ChatMessageType::Broadcast,
                ),
                Ok(Some(found)) => write_output_info(
                    w,
                    None,
                    &format!("Playtest character exported; it fails the checks, so no world will import it: {found}"),
                    ChatMessageType::Broadcast,
                ),
                Err(e) => write_output_error(
                    w,
                    None,
                    &format!("Playtest character not exported: {e}"),
                    ChatMessageType::Broadcast,
                ),
            }
            return;
        }
    }
    let result = match parameters {
        [op, account, name, input, result] if op == "create" => {
            if let Err(e) = create::create(w, account, name, path(input), path(result)) {
                Err(e)
            } else {
                write_output_info(
                    w,
                    None,
                    "Playtest character created.",
                    ChatMessageType::Broadcast,
                );
                return;
            }
        }
        [op, name, input, recovery, result] if op == "apply" => {
            apply_recipe(w, name, path(input), backup_arg(recovery), path(result), false)
                .map(|_| ())
        }
        [op, name, input, recovery, result, how] if op == "apply" && how == "replace" => {
            apply_recipe(w, name, path(input), backup_arg(recovery), path(result), true)
                .map(|_| ())
        }
        [op, name, input, recovery] if op == "restore" => {
            restore(w, name, path(input), path(recovery))
        }
        _ => Err("use playtest-character create <account> <character> <recipe.json> <result.json>, apply <character> <recipe.json> <new-backup.sqlite or -> <result.json> [replace], preview <recipe.json> <result.json>, restore <character> <backup.sqlite> <new-backup.sqlite>, or export <character> <file.empyrean-character> [<note.json>]".into()),
    };
    // An apply told to keep no backup kept none.
    let kept = !matches!(parameters, [op, _, _, b, ..] if op == "apply" && b == "-");
    match result {
        Ok(()) => write_output_info(
            w,
            None,
            if kept {
                "Playtest character updated; the previous state is backed up."
            } else {
                "Playtest character updated."
            },
            ChatMessageType::Broadcast,
        ),
        Err(e) => write_output_error(
            w,
            None,
            &format!("Playtest character unchanged: {e}"),
            ChatMessageType::Broadcast,
        ),
    }
}
