//! New playtest characters, and new bodies for a changed heritage or gender: made as character
//! creation makes a character, so no client has to go through the creation screens first.
//!
//! A character is made for an account that is not logged in, under a free name, with the
//! heritage and gender its recipe names, that heritage's first look (its first face, hair and
//! clothes), the recipe's innate split and the skills the heritage trains for nothing. It is
//! saved at once, then made its recipe's character as an apply makes it. A character whose
//! recipe names another heritage or gender is given a new body the same way, keeping its name,
//! its object id and where it stands.

use dereth_progression::order::Order;
use dereth_progression::{budget, Sac};
use empyrean_entity::enums::{AccessLevel, HeritageGroup, SkillAdvancementClass, WeenieType};
use empyrean_entity::{Appearance, Biota, ObjectGuid};
use empyrean_store::models::auth::Account;
use empyrean_world::factories::player_factory::{self, CreateResult, CreatedPlayer};
use empyrean_world::factories::player_factory_ex::create_character_create_info;
use empyrean_world::managers::{guid_manager, player_manager};
use empyrean_world::World;
use std::path::Path;

/// How many skills a character's creation names, the end of retail's list.
const CREATION_SKILLS: u32 = 55;

/// The character creation would make of `heritage` and `gender` for `account`, named `name`, with
/// object id `guid` and the innate split `innate`.
///
/// # Errors
/// A heritage or gender the data files or the world's era do not offer, or a creation that fails.
pub fn body(
    w: &mut World,
    account: &Account,
    guid: ObjectGuid,
    name: &str,
    (heritage, gender): (u32, u32),
    innate: [u32; 6],
) -> Result<CreatedPlayer, String> {
    let offered = super::with_tables(w, |t| t.heritages());
    let Some((_, heritage_name)) = offered.iter().find(|(id, _)| *id == heritage) else {
        return Err(format!("the data files offer no heritage {heritage}"));
    };
    if w.era.heritages.is_some_and(|h| !h.contains(&heritage)) {
        return Err(format!(
            "{heritage_name} is not a heritage of this world's era"
        ));
    }
    let has_gender = super::with_tables(w, |t| {
        t.chargen
            .heritage_groups
            .get(&heritage)
            .is_some_and(|h| h.sexes.contains_key(&gender))
    });
    if !has_gender {
        return Err(format!("{heritage_name} has no gender {gender}"));
    }
    let [strength, endurance, coordination, quickness, focus, self_] = innate;
    let mut info = create_character_create_info(
        w,
        name,
        strength,
        endurance,
        coordination,
        quickness,
        focus,
        self_,
        false,
    );
    info.heritage = HeritageGroup(i32::try_from(heritage).map_err(|_| "no such heritage")?);
    info.gender = gender;
    info.appearance = Appearance {
        headgear_style: u32::MAX,
        ..Appearance::default()
    };
    info.template_option = 0;
    info.start_area = 0;
    info.skill_advancement_classes = super::with_tables(w, |t| {
        (0..CREATION_SKILLS)
            .map(|id| match budget::starting_class(&t, heritage, id) {
                None => SkillAdvancementClass::Inactive,
                Some(Sac::Trained) => SkillAdvancementClass::Trained,
                Some(Sac::Specialized) => SkillAdvancementClass::Specialized,
                Some(_) => SkillAdvancementClass::Untrained,
            })
            .collect()
    });
    // The weenie creation takes for the account, as the character handler chooses it.
    let level = AccessLevel(i32::try_from(account.access_level).unwrap_or(0));
    let overriding = w
        .auth
        .lock()
        .accounts_config()
        .override_character_permissions;
    let staff = overriding && level >= AccessLevel::Developer && level <= AccessLevel::Admin;
    let weenie = staff
        .then(|| w.content.get_cached_weenie_by_class_name("admin"))
        .flatten()
        .or_else(|| w.content.get_cached_weenie_by_class_name("human"))
        .ok_or("the world database has no player weenie")?;
    let weenie_type = if staff {
        WeenieType::Admin
    } else {
        weenie.weenie_type
    };
    let (result, made) =
        player_factory::create(w, &info, weenie, guid, account.account_id, weenie_type);
    if result != CreateResult::Success {
        return Err(format!("character creation failed: {}", result.name()));
    }
    Ok(made)
}

/// What `made` carries and wears, as they are stored.
pub fn possessions(w: &mut World, made: &mut CreatedPlayer) -> Vec<Biota> {
    made.get_all_possessions(w)
        .into_iter()
        .map(|o| o.biota.clone())
        .collect()
}

/// Make the character `name` on account `account` from the recipe in `input`, and say what was
/// made in `result`.
///
/// # Errors
/// Why nothing was made.
pub fn create(
    w: &mut World,
    account: &str,
    name: &str,
    input: &Path,
    result: &Path,
) -> Result<dereth_progression::order::Made, String> {
    let order: Order = super::read_order(input)?;
    let account = w
        .auth
        .lock()
        .get_account_by_name(account)
        .ok_or_else(|| format!("there is no account {account}"))?;
    if w.net.find_by_account(&account.account_name).is_some() {
        return Err("the account is logged in".into());
    }
    let bare = name.trim_start_matches('+');
    // Creation's own limit: a name of 1 to 32 characters.
    if bare.trim().is_empty() || bare.chars().count() > 32 {
        return Err("the name is not one a character can have".into());
    }
    if player_manager::find_by_name(w, bare).0.is_some()
        || !w.shard.base_database().is_character_name_available(bare)
    {
        return Err("the name is taken".into());
    }
    let heritage = order
        .heritage
        .unwrap_or(dereth_progression::heritage::ALUVIAN);
    let gender = order.gender.unwrap_or(dereth_progression::gender::MALE);
    // Everything the apply would refuse, refused before anything is saved.
    super::checked_build(w, &order, heritage)?;
    let classes = order.items.iter().map(|i| i.class);
    for class in classes.chain(order.slots.values().filter_map(|s| s.class)) {
        if w.content.get_cached_weenie(class).is_none() {
            return Err(format!("the world has no class {class}"));
        }
    }
    let guid = guid_manager::new_player_guid(w);
    let mut made = body(
        w,
        &account,
        guid,
        bare,
        (heritage, gender),
        order.plan.innate,
    )?;
    let mut player = made.player.biota.clone();
    let mut carried = possessions(w, &mut made);
    let mut character = made.character().clone();
    // Counted as logged in once already: a character of a level is no newcomer, and the
    // training halls' welcome is for a first login.
    character.total_logins = character.total_logins.max(1);
    {
        let mut db = w.shard.base_database();
        db.begin_batch().map_err(|e| e.to_string())?;
        let saved = db.save_biota(&mut player, false)
            && carried.iter_mut().all(|b| db.save_biota(b, false))
            && db.save_character(&character);
        if !saved {
            db.rollback_batch();
            return Err("the new character would not save".into());
        }
        db.commit_batch().map_err(|e| e.to_string())?;
    }
    player_manager::add_offline_player_biota(w, player);
    super::apply_recipe(w, name, input, None, result, true)
}
