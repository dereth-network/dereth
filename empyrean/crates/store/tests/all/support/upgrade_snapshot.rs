//! What an upgrade fixture holds, read back through the store: its accounts, their characters,
//! and each character's biota and possessions, as JSON. The fixture generator writes it beside the
//! databases (`expected.json`) and the harness compares the upgraded databases against it.
//! Fixture: none (shared by the upgrade-fixture generator and the harness).
//!
//! The fields are the ones a player would notice missing; keep the shape stable, since every
//! committed fixture's `expected.json` was written with it.

use empyrean_store::authentication_database::AccountQuery;
use empyrean_store::models::shard::Biota;
use empyrean_store::{AuthDatabase, ShardDatabase};
use serde_json::{json, Value};

/// The access levels an account can have.
const ACCESS_LEVELS: std::ops::RangeInclusive<u32> = 0..=5;

fn pairs<T, K: Ord + Copy>(rows: &[T], key: impl Fn(&T) -> K, value: impl Fn(&T) -> Value) -> Value
where
    Value: From<K>,
{
    let mut v: Vec<&T> = rows.iter().collect();
    v.sort_by_key(|r| key(r));
    Value::Array(
        v.into_iter()
            .map(|r| Value::Array(vec![Value::from(key(r)), value(r)]))
            .collect(),
    )
}

/// One biota: its identity and the property rows a character's things are made of.
#[must_use]
pub fn biota(b: &Biota) -> Value {
    json!({
        "id": b.id,
        "weenie_class_id": b.weenie_class_id,
        "weenie_type": b.weenie_type,
        "int": pairs(&b.biota_properties_int, |r| r.r#type, |r| json!(r.value)),
        "int64": pairs(&b.biota_properties_int64, |r| r.r#type, |r| json!(r.value)),
        "bool": pairs(&b.biota_properties_bool, |r| r.r#type, |r| json!(r.value)),
        "float": pairs(&b.biota_properties_float, |r| r.r#type, |r| json!(r.value)),
        "string": pairs(&b.biota_properties_string, |r| r.r#type, |r| json!(r.value)),
        "did": pairs(&b.biota_properties_did, |r| r.r#type, |r| json!(r.value)),
        "iid": pairs(&b.biota_properties_iid, |r| r.r#type, |r| json!(r.value)),
        "attribute": pairs(&b.biota_properties_attribute, |r| r.r#type, |r| json!([r.init_level, r.level_from_cp, r.cp_spent])),
        "skill": pairs(&b.biota_properties_skill, |r| r.r#type, |r| json!([r.level_from_pp, r.sac, r.pp, r.init_level])),
        "spell_book": pairs(&b.biota_properties_spell_book, |r| r.spell, |r| json!(f64::from(r.probability))),
        "position": pairs(&b.biota_properties_position, |r| r.position_type, |r| json!([
            r.obj_cell_id,
            f64::from(r.origin_x), f64::from(r.origin_y), f64::from(r.origin_z),
            f64::from(r.angles_w), f64::from(r.angles_x), f64::from(r.angles_y), f64::from(r.angles_z),
        ])),
    })
}

/// Everything the fixture holds, read through `auth` and `shard`.
///
/// # Panics
/// When a read fails.
#[must_use]
pub fn snapshot(auth: &mut dyn AuthDatabase, shard: &mut dyn ShardDatabase) -> Value {
    let mut accounts = Vec::new();
    for level in ACCESS_LEVELS {
        accounts.extend(
            auth.select_accounts(AccountQuery::AccessLevel(level))
                .expect("accounts"),
        );
    }
    accounts.sort_by_key(|a| a.account_id);
    let accounts: Vec<Value> = accounts
        .iter()
        .map(|a| {
            let mut characters = shard.get_characters(a.account_id, true);
            characters.sort_by_key(|c| c.id);
            let characters: Vec<Value> = characters
                .iter()
                .map(|c| {
                    let player = shard.get_biota(c.id, true).as_ref().map(biota);
                    let possessed = shard.get_possessed_biotas_in_parallel(c.id);
                    let things = |mut v: Vec<Biota>| {
                        v.sort_by_key(|b| b.id);
                        Value::Array(v.iter().map(biota).collect())
                    };
                    let mut quests: Vec<_> = c
                        .character_properties_quest_registry
                        .iter()
                        .map(|q| json!([q.quest_name, q.num_times_completed, q.last_time_completed]))
                        .collect();
                    quests.sort_by_key(ToString::to_string);
                    json!({
                        "id": c.id,
                        "name": c.name,
                        "is_deleted": c.is_deleted,
                        "delete_time": c.delete_time,
                        "total_logins": c.total_logins,
                        "character_options_1": c.character_options_1,
                        "character_options_2": c.character_options_2,
                        "spellbook_filters": c.spellbook_filters,
                        "friends": c.character_properties_friend_list.iter().map(|f| f.friend_id).collect::<Vec<_>>(),
                        "quests": quests,
                        "shortcuts": c.character_properties_shortcut_bar.iter().map(|s| json!([s.shortcut_bar_index, s.shortcut_object_id])).collect::<Vec<_>>(),
                        "spell_bars": c.character_properties_spell_bar.iter().map(|s| json!([s.spell_bar_number, s.spell_bar_index, s.spell_id])).collect::<Vec<_>>(),
                        "biota": player,
                        "inventory": things(possessed.inventory),
                        "wielded": things(possessed.wielded_items),
                    })
                })
                .collect();
            json!({
                "id": a.account_id,
                "name": a.account_name,
                "access_level": a.access_level,
                "password_hash": a.password_hash,
                "create_time": a.create_time.ticks(),
                "characters": characters,
            })
        })
        .collect();
    json!({ "accounts": accounts })
}
