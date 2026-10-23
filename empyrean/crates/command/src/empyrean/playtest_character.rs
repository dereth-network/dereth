//! Offline playtest character templates, applied only by the server console.
//! DIVERGE: V453. World builders can reset a playtest character after keeping a restorable backup.

use empyrean_entity::enums::{
    PropertyAttribute, PropertyInt, PropertyInt64, Skill, SkillAdvancementClass,
};
use empyrean_entity::models::{PropertiesAttribute, PropertiesSkill};
use empyrean_entity::Biota;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// A portable starting character; identity and object ids remain server-owned.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Template {
    pub version: u32,
    pub level: u32,
    /// Strength, Endurance, Coordination, Quickness, Focus, Self.
    pub attributes: [u32; 6],
    #[serde(default)]
    pub skills: Vec<TemplateSkill>,
    #[serde(default)]
    pub equipment: Vec<Equipment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TemplateSkill {
    pub name: String,
    /// Untrained, Trained, or Specialized.
    pub training: String,
    /// Purchased ranks, before attribute bonuses.
    pub ranks: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Equipment {
    pub weenie: u32,
    /// Zero puts the item in inventory; otherwise a valid equipment location mask.
    #[serde(default)]
    pub location: u32,
}

impl Template {
    /// Validate the complete request before any character or inventory is changed.
    pub fn validate(&self, max_level: u32) -> Result<(), String> {
        if self.version != 1 {
            return Err("unsupported character template version".into());
        }
        if !(1..=max_level).contains(&self.level) {
            return Err(format!("level must be between 1 and {max_level}"));
        }
        if self.attributes.iter().any(|n| !(1..=10000).contains(n)) {
            return Err("attributes must be between 1 and 10000".into());
        }
        if self.equipment.len() > 64 {
            return Err("a template can carry at most 64 items".into());
        }
        let mut seen = BTreeSet::new();
        for row in &self.skills {
            let skill = Skill::from_name(&row.name)
                .filter(|s| *s != Skill::None)
                .ok_or_else(|| format!("unknown skill {}", row.name))?;
            if !seen.insert(skill.0) {
                return Err(format!("duplicate skill {}", row.name));
            }
            let training = training(&row.training)?;
            if training == SkillAdvancementClass::Untrained && row.ranks != 0 {
                return Err(format!(
                    "untrained skill {} cannot have purchased ranks",
                    row.name
                ));
            }
        }
        Ok(())
    }

    /// Replace the requested statistics while retaining character identity and other state.
    pub fn apply_statistics(&self, player: &mut Biota, level_xp: u64) -> Result<(), String> {
        self.validate(u32::MAX)?;
        let level = i32::try_from(self.level).map_err(|_| "level is too large")?;
        let xp = i64::try_from(level_xp).map_err(|_| "level experience is too large")?;
        player.set_property(PropertyInt::Level, level);
        player.set_property(PropertyInt64::TotalExperience, xp);
        player.set_property(PropertyInt64::AvailableExperience, 0);
        let attributes = player.properties_attribute.get_or_insert_default();
        for (key, value) in [
            PropertyAttribute::Strength,
            PropertyAttribute::Endurance,
            PropertyAttribute::Coordination,
            PropertyAttribute::Quickness,
            PropertyAttribute::Focus,
            PropertyAttribute::Self_,
        ]
        .into_iter()
        .zip(self.attributes)
        {
            attributes.insert(
                key,
                PropertiesAttribute {
                    init_level: value,
                    ..Default::default()
                },
            );
        }
        let skills = player.properties_skill.get_or_insert_default();
        for row in &self.skills {
            let skill = Skill::from_name(&row.name).ok_or("unknown skill")?;
            let sac = training(&row.training)?;
            skills.insert(
                skill,
                PropertiesSkill {
                    sac,
                    init_level: if sac == SkillAdvancementClass::Specialized {
                        10
                    } else {
                        0
                    },
                    level_from_pp: row.ranks,
                    ..Default::default()
                },
            );
        }
        Ok(())
    }
}

fn training(name: &str) -> Result<SkillAdvancementClass, String> {
    match name {
        "Untrained" => Ok(SkillAdvancementClass::Untrained),
        "Trained" => Ok(SkillAdvancementClass::Trained),
        "Specialized" => Ok(SkillAdvancementClass::Specialized),
        _ => Err(format!("unknown skill training {name}")),
    }
}

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::CommandHandlerInfo;
use crate::handlers::command_handler_helper::{write_output_error, write_output_info};
use empyrean_entity::enums::{AccessLevel, ChatMessageType, PropertyInstanceId};
use empyrean_net::SessionId;
use empyrean_store::adapter::BiotaConverter;
use empyrean_store::{ShardDatabase, SqliteShard};
use empyrean_world::{entity::i_player::IPlayer, managers::player_manager, World};
use std::path::Path;

/// Console-only character template operations.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let (handler, handler_name) = crate::handler!(handle);
    vec![CommandHandlerInfo { handler, handler_name, attribute: CommandHandlerAttribute::with_description(
        "playtest-character", AccessLevel::Admin, CommandHandlerFlag::ConsoleInvoke,
        "Apply an offline playtest template, keeping a character backup first.",
        "apply <character> <template.json> <backup.sqlite> | restore <character> <backup.sqlite> <before-restore.sqlite>",
    ) }]
}

/// A backup contains the character and every nested possession, in the shard's own format.
fn rows(
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
pub fn replace(db: &mut dyn ShardDatabase, id: u32, new: &[Biota]) -> Result<(), String> {
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
        db.commit_batch().map_err(|e| e.to_string())
    })();
    if result.is_err() {
        db.rollback_batch();
    }
    result
}

/// Save a recoverable character snapshot without replacing an existing recovery file.
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
    let mut copy = SqliteShard::open(path).map_err(|e| e.to_string())?;
    copy.begin_batch().map_err(|e| e.to_string())?;
    for row in &mut stored {
        copy.write_biota(row).map_err(|e| e.to_string())?;
    }
    copy.commit_batch().map_err(|e| e.to_string())
}

fn apply(
    w: &mut World,
    name: &str,
    input: &Path,
    recovery: &Path,
    restore: bool,
) -> Result<(), String> {
    let (found, _) = player_manager::find_by_name(w, name);
    let Some(IPlayer::Offline(guid)) = found else {
        return Err("character must exist and be logged out".into());
    };
    if w.shard.queue_count() != 0 || w.shard.in_transaction() {
        return Err("character saves are still pending; retry after they finish".into());
    }
    let id = guid.full();
    let old = w
        .player_manager
        .offline_players
        .get(&id)
        .ok_or("character is unavailable")?
        .biota
        .clone();
    let new = if restore {
        if !input.is_file() {
            return Err("backup does not exist".into());
        }
        let mut copy = SqliteShard::open(input).map_err(|e| e.to_string())?;
        rows(&mut copy, id)?
            .iter()
            .map(|b| BiotaConverter::convert_to_entity_biota(b, false))
            .collect::<Vec<_>>()
    } else {
        let template: Template =
            serde_json::from_slice(&std::fs::read(input).map_err(|e| e.to_string())?)
                .map_err(|e| e.to_string())?;
        template.validate(empyrean_world::world_objects::player_xp::get_max_level(w))?;
        let xp = w.dats.portal_dat().xp_table().level_xp[template.level as usize];
        let mut player = old;
        template.apply_statistics(&mut player, xp)?;
        for row in &template.skills {
            let skill = Skill::from_name(&row.name).ok_or("unknown skill")?;
            if !w
                .dats
                .portal_dat()
                .skill_table()
                .skills
                .contains_key(&(skill.0 as u32))
            {
                return Err(format!("skill {} is unavailable in this world", row.name));
            }
            let sac = training(&row.training)?;
            if let Some(table) =
                empyrean_world::world_objects::player_skills::get_skill_xp_table(w, sac)
            {
                let spent = *table
                    .get(usize::from(row.ranks))
                    .ok_or_else(|| format!("too many purchased ranks for {}", row.name))?;
                player
                    .properties_skill
                    .as_mut()
                    .unwrap()
                    .get_mut(&skill)
                    .unwrap()
                    .pp = spent;
            }
        }
        let mut new = vec![player];
        let mut worn = 0_u32;
        // Check every class and equipment location before allocating any object ids.
        for item in &template.equipment {
            let weenie = w
                .content
                .get_cached_weenie(item.weenie)
                .ok_or_else(|| format!("unknown equipment {}", item.weenie))?;
            use empyrean_entity::enums::WeenieType as T;
            if !matches!(
                weenie.weenie_type,
                T::Generic
                    | T::Clothing
                    | T::MissileLauncher
                    | T::Ammunition
                    | T::MeleeWeapon
                    | T::Book
                    | T::Coin
                    | T::Food
                    | T::Container
                    | T::Key
                    | T::Lockpick
                    | T::Healer
                    | T::LightSource
                    | T::SpellComponent
                    | T::Scroll
                    | T::Caster
                    | T::ManaStone
                    | T::Gem
                    | T::CraftTool
                    | T::Stackable
                    | T::PetDevice
            ) {
                return Err(format!("{} is not inventory equipment", item.weenie));
            }
            let valid = weenie
                .get_property(PropertyInt::ValidLocations)
                .unwrap_or(0) as u32;
            if item.location != 0
                && (item.location & valid != item.location || item.location & worn != 0)
            {
                return Err(format!(
                    "equipment {} has an invalid or occupied location",
                    item.weenie
                ));
            }
            worn |= item.location;
        }
        for (index, item) in template.equipment.iter().enumerate() {
            let object = empyrean_world::factories::world_object_factory::create_new_world_object_by_wcid_in_world(w, item.weenie).ok_or("equipment cannot be created")?;
            let mut biota = object.biota;
            if item.location == 0 {
                biota.set_property(PropertyInstanceId::Container, id);
                biota.try_remove_property(PropertyInstanceId::Wielder);
                biota.try_remove_property(PropertyInt::CurrentWieldedLocation);
                biota.set_property(
                    PropertyInt::PlacementPosition,
                    i32::try_from(index).unwrap(),
                );
            } else {
                biota.set_property(PropertyInstanceId::Wielder, id);
                biota.try_remove_property(PropertyInstanceId::Container);
                biota.set_property(
                    PropertyInt::CurrentWieldedLocation,
                    i32::try_from(item.location).map_err(|_| "equipment location is too large")?,
                );
            }
            new.push(biota);
        }
        new
    };
    let mut db = w.shard.base_database();
    backup(&mut **db, id, recovery)?;
    replace(&mut **db, id, &new)?;
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
    if let [op, path] = parameters {
        if op == "account" {
            let busy = w.net.find_by_account(path).is_some();
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
        if op == "schema" {
            let skills: Vec<_> = Skill::ALL
                .iter()
                .filter(|s| {
                    **s != Skill::None
                        && w.dats
                            .portal_dat()
                            .skill_table()
                            .skills
                            .contains_key(&(s.0 as u32))
                })
                .filter_map(|s| s.name())
                .collect();
            let locations: Vec<_> = empyrean_entity::enums::EquipMask::ALL
                .iter()
                .filter_map(|v| {
                    v.name()
                        .map(|name| serde_json::json!({"name":name,"value":v.0}))
                })
                .collect();
            let schema = serde_json::json!({"version":1,"max_level":empyrean_world::world_objects::player_xp::get_max_level(w),"skills":skills,"locations":locations});
            match std::fs::write(path, serde_json::to_vec_pretty(&schema).unwrap()) {
                Ok(()) => write_output_info(
                    w,
                    None,
                    "Playtest character schema written.",
                    ChatMessageType::Broadcast,
                ),
                Err(e) => write_output_error(w, None, &e.to_string(), ChatMessageType::Broadcast),
            }
            return;
        }
    }
    let result = match parameters {
        [op, name, input, recovery] if op == "apply" || op == "restore" => apply(
            w,
            name,
            Path::new(input),
            Path::new(recovery),
            op == "restore",
        ),
        _ => Err(
            "use playtest-character apply|restore <character> <input> <new-backup.sqlite>".into(),
        ),
    };
    match result {
        Ok(()) => write_output_info(
            w,
            None,
            "Playtest character updated; the previous state is backed up.",
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
