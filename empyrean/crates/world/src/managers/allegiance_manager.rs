// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Managers/AllegianceManager.cs
//! Port of `Source/ACE.Server/Managers/AllegianceManager.cs`.
//!
//! Allegiance helper methods. The loaded `Allegiance` objects live in `w.objects` (see
//! `world_objects/allegiance.rs`); `Allegiances` lists their guids and `Players` names each
//! player's node by the guid of the allegiance that holds it.

use empyrean_common::dotnet::DotNetDict;
use empyrean_entity::enums::{PropertyInstanceId, PropertyInt};
use empyrean_entity::ObjectGuid;
use empyrean_store::adapter::biota_converter::BiotaConverter;

use crate::dispatch::Class;
use crate::entity::actions::i_action::Action;
use crate::entity::allegiance_node::{self, NodeRef};
use crate::entity::i_player::{self, IPlayer};
use crate::managers::{player_manager, property_manager, world_manager};
use crate::network::game_messages::game_message;
use crate::network::game_messages::messages::game_message_private_update_property_int::game_message_private_update_property_int;
use crate::world_objects::allegiance;
use crate::world_objects::player_allegiance as pa;
use crate::world_objects::world_object::{CtorEnv, WorldObject};
use crate::World;

/// The mutable static state of ACE's `AllegianceManager`, held as a field of `World`.
#[derive(Debug, Default)]
pub struct AllegianceManagerState {
    /// A mapping of all loaded Allegiance GUIDs => their Allegiances (the objects are in
    /// `w.objects`).
    // ACE: AllegianceManager.Allegiances
    pub allegiances: DotNetDict<ObjectGuid, ()>,
    /// A mapping of all Players on the server => their AllegianceNodes (the guid of the
    /// allegiance whose tree holds the player's node).
    // ACE: AllegianceManager.Players
    pub players: DotNetDict<ObjectGuid, ObjectGuid>,
}

/// The maximum amount of leadership / loyalty
// ACE: AllegianceManager.SkillCap
pub const SKILL_CAP: f32 = 291.0;

/// The maximum amount of realtime hours sworn to patron
// ACE: AllegianceManager.RealCap
pub const REAL_CAP: f32 = 730.0;

/// The maximum amount of in-game hours sworn to patron
// ACE: AllegianceManager.GameCap
pub const GAME_CAP: f32 = 720.0;

// ACE: AllegianceManager.GetMonarch
/// Returns the monarch for a player
#[must_use]
pub fn get_monarch(w: &World, player: IPlayer) -> IPlayer {
    let Some(monarch_id) = i_player::monarch_id(w, player) else {
        return player;
    };

    let monarch = player_manager::find_by_guid(w, monarch_id).0;

    monarch.unwrap_or(player)
}

// ACE: AllegianceManager.GetAllegiance
/// Returns the full allegiance structure for any player (a player at any level of an
/// allegiance), or `None` for a player alone.
pub fn get_allegiance(w: &mut World, player: Option<IPlayer>) -> Option<ObjectGuid> {
    let player = player?;

    let monarch = get_monarch(w, player);

    // is this allegiance already loaded / cached?
    if let Some(&allegiance) = w.allegiance_manager.players.get(&monarch.guid()) {
        return Some(allegiance);
    }

    // try to load biota
    let allegiance_id = w
        .shard
        .base_database()
        .get_allegiance_id(monarch.guid().full());
    let biota = allegiance_id.and_then(|id| w.shard.base_database().get_biota(id, false));

    let from_database = biota.is_some();
    let mut allegiance = match biota {
        Some(biota) => {
            let entity_biota = BiotaConverter::convert_to_entity_biota(&biota, false);

            CtorEnv::with_world(w, |env| {
                WorldObject::from_biota(env, Class::Allegiance, entity_biota)
            })
        }
        None => allegiance::allegiance_from_monarch(w, monarch.guid()),
    };

    if allegiance::fields(&allegiance)
        .expect("an Allegiance")
        .total_members()
        == 1
    {
        return None;
    }

    if !from_database {
        // `WorldObjectFactory.CreateNewWorldObject("allegiance") as Allegiance`
        let created = w
            .content
            .get_cached_weenie_by_class_name("allegiance")
            .and_then(|weenie| w.content.get_cached_weenie(weenie.weenie_class_id))
            .and_then(|weenie| crate::factories::player_factory::create_new_world_object(w, weenie))
            .filter(WorldObject::is_allegiance);
        allegiance = created.expect("ACE: allegiance is null (NullReferenceException)");
        allegiance.set_monarch_id(Some(monarch.guid().full()));
        allegiance::init(w, &mut allegiance, monarch.guid());
    }

    let guid = allegiance.guid;
    store(w, allegiance);

    if !from_database {
        crate::dispatch::save_biota_to_database::save_biota_to_database(w, guid, true);
    }

    add_players(w, guid);

    //if (!Allegiances.ContainsKey(allegiance.Guid))
    //Allegiances.Add(allegiance.Guid, allegiance);
    w.allegiance_manager.allegiances.insert(guid, ());

    Some(guid)
}

/// `Allegiances[allegiance.Guid] = allegiance`'s object half: the new instance replaces any
/// loaded one with its guid.
///
/// Not ACE's (a fix, V261): the allegiance-chat boots and gags of the instance
/// being replaced carry over to the new one, with their expiry times, so a rebuild of the same
/// allegiance does not lift them.
fn store(w: &mut World, mut allegiance: WorldObject) {
    let guid = allegiance.guid;
    let previous_filters = w
        .objects
        .get_mut(guid)
        .and_then(allegiance::fields_mut)
        .and_then(|f| f.chat_filters.take());
    if let (Some(previous), Some(f)) = (previous_filters, allegiance::fields_mut(&mut allegiance)) {
        f.chat_filters = Some(previous);
    }
    w.objects.remove(guid);
    w.objects.insert(allegiance).expect("just removed");
}

// ACE: AllegianceManager.GetAllegianceNode
/// Returns the AllegianceNode for a Player
#[must_use]
pub fn get_allegiance_node(w: &World, player: IPlayer) -> Option<NodeRef> {
    w.allegiance_manager
        .players
        .get(&player.guid())
        .map(|&allegiance| NodeRef {
            allegiance,
            player: player.guid(),
        })
}

// ACE: AllegianceManager.FindAllPlayers
/// Returns a list of all players under a monarch
#[must_use]
pub fn find_all_players(w: &World, monarch_guid: ObjectGuid) -> Vec<IPlayer> {
    player_manager::find_all_by_monarch(w, monarch_guid)
}

// ACE: AllegianceManager.LoadPlayer
/// Loads the Allegiance and AllegianceNode for a Player
pub fn load_player(w: &mut World, player: Option<IPlayer>) {
    let Some(player) = player else { return };

    let allegiance = get_allegiance(w, Some(player));
    pa::set_i_player_allegiance(w, player, allegiance);
    let node = get_allegiance_node(w, player);
    pa::set_i_player_allegiance_node(w, player, node);

    // TODO: update chat channels for online players here?
}

// ACE: AllegianceManager.Rebuild
/// Called when a player joins/exits an Allegiance
pub fn rebuild(w: &mut World, allegiance: Option<ObjectGuid>) {
    let Some(allegiance) = allegiance else { return };

    remove_cache(w, allegiance);

    // rebuild allegiance
    let monarch = allegiance_node::player(w, allegiance::monarch_player_guid(w, allegiance));
    let allegiance =
        get_allegiance(w, monarch).expect("ACE: allegiance is null (NullReferenceException)");

    // relink players
    for (member, _) in allegiance::members(w, allegiance) {
        let Some(player) = player_manager::find_by_guid(w, member.full()).0 else {
            continue;
        };

        load_player(w, Some(player));
    }

    // update dynamic properties
    allegiance::update_properties(w, allegiance);
}

// ACE: AllegianceManager.AddPlayers
/// Appends the Players lookup table with the members of an Allegiance
pub fn add_players(w: &mut World, allegiance: ObjectGuid) {
    for (player, _node) in allegiance::members(w, allegiance) {
        // `Players[player] = allegianceNode` (added, or replaced in place)
        w.allegiance_manager.players.insert(player, allegiance);
    }
}

// ACE: AllegianceManager.RemoveCache
/// Removes an Allegiance from the Players lookup table cache
pub fn remove_cache(w: &mut World, allegiance: ObjectGuid) {
    for (member, _) in allegiance::members(w, allegiance) {
        w.allegiance_manager.players.remove(&member);
    }
}

// ACE: AllegianceManager.PassXP
/// Passes `amount` up from the vassal's node, on the world queue (ACE: "this function can be
/// called from multi-threaded operations").
pub fn pass_xp(w: &mut World, vassal_node: NodeRef, amount: u64, direct: bool) {
    world_manager::enqueue_action(
        w,
        Action::delegate(move |w: &mut World| do_pass_xp(w, vassal_node, amount, direct)),
    );
}

/// The pass-up percentages `DoPassXP` computes, in `float` as ACE does.
///
/// Not ACE's (retail, V299; the retail captures): ACE also keeps `passup = generated *
/// received` and applies it to the vassal's XP in one step. Retail applies the two in turn, each
/// rounded ([`PassupFactors::amounts`]), so there is no single pass-up fraction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PassupFactors {
    /// `generated`: the fraction of the vassal's XP it generates (its tithe).
    pub generated: f32,
    /// `received`: the fraction of the vassal's tithe the patron receives.
    pub received: f32,
}

impl PassupFactors {
    /// The vassal's tithe of `amount` and the patron's gain from it: `round(amount * generated)`,
    /// then `round(tithe * received)`, each at double precision and rounded to nearest.
    ///
    /// Not ACE's (retail, V299; the retail captures): ACE truncates `amount * generated`
    /// and `amount * (generated * received)` in `float`.
    #[must_use]
    pub fn amounts(&self, amount: u64) -> (u64, u64) {
        let tithe = passup_amount(amount, self.generated);
        (tithe, passup_amount(tithe, self.received))
    }
}

/// A time sworn, in the units of the pass-up formulas: real days and in-game hours.
///
/// Not ACE's (retail, V285): ACE has no sworn times.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SwornDays {
    /// Real days sworn (RT).
    pub real_days: f64,
    /// In-game hours sworn (IG).
    pub game_hours: f64,
}

impl From<pa::SwornTime> for SwornDays {
    fn from(t: pa::SwornTime) -> Self {
        Self {
            real_days: t.real_days(),
            game_hours: t.game_hours(),
        }
    }
}

/// The average time a patron's direct vassals have been sworn to it (RT2, IG2), from each
/// vassal's whole seconds; zero for no vassals.
///
/// Not ACE's (retail, V285): ACE has no sworn times.
#[must_use]
pub fn sworn_average(times: &[pa::SwornTime]) -> SwornDays {
    if times.is_empty() {
        return SwornDays::default();
    }
    #[allow(clippy::cast_precision_loss)] // a vassal count
    let n = times.len() as f64;
    let real: f64 = times.iter().map(|t| t.real_days()).sum();
    let game: f64 = times.iter().map(|t| t.game_hours()).sum();
    SwornDays {
        real_days: real / n,
        game_hours: game / n,
    }
}

/// The formulas of `DoPassXP` for a vassal's current loyalty and time sworn, the patron's current
/// leadership, its number of direct vassals and their average time sworn (ACE computes them
/// inline).
///
/// Not ACE's (retail, V285): the retail captures fit, to the unit,
/// a vassal side that folds the time term into a whole effective Loyalty,
/// `E = round(Loyalty * (1 + min(RT,730)/730 * min(IG,720)/720))` (round half away from zero;
/// Loyalty is the live buffed value; neither it nor E is capped at 291), with
/// `Generated % = 50 + 22.5 * E / 291` on a vassal's own XP and `5.5 * E / 291` on the XP its
/// own vassals passed to it (the pass-through). ACE pins the time term at its maximum, caps
/// Loyalty at 291, and passes through `16 + 8 * ...`.
///
/// Not ACE's (retail, V299; the retail captures): the patron side folds its terms into
/// a whole effective Leadership the same way,
/// `E = round(Leadership * (1 + 0.3 * V + 0.7 * min(RT2,730)/730 * min(IG2,720)/720))` (round
/// half away from zero; Leadership is the live buffed value, neither it nor E capped at 291;
/// V = min(direct vassals, 4)/4; RT2 and IG2 the vassals' average times), with
/// `Received % = 50 + 22.5 * E / 291` on both steps. ACE uses the 2004 form
/// `1 + V * RT2/730 * IG2/720` with the time term pinned at its maximum, caps Leadership at 291,
/// and receives `16 + 8 * ...` on the pass-through step, where retail keeps the ordinary
/// Received % (V299, the retail captures). The percentages are computed at double precision and
/// kept as `float`.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // C# `(float)` of the double percentages
pub fn passup_factors(
    current_loyalty: u32,
    current_leadership: u32,
    patron_total_vassals: i32,
    direct: bool,
    vassal_time: SwornDays,
    vassals_avg_time: SwornDays,
) -> PassupFactors {
    let skill_cap = f64::from(SKILL_CAP);
    let real_cap = f64::from(REAL_CAP);
    let game_cap = f64::from(GAME_CAP);

    let loyalty = f64::from(current_loyalty);
    let leadership = f64::from(current_leadership);

    let time_real = real_cap.min(vassal_time.real_days);
    let time_game = game_cap.min(vassal_time.game_hours);

    let time_real_avg = real_cap.min(vassals_avg_time.real_days);
    let time_game_avg = game_cap.min(vassals_avg_time.game_hours);

    let total_vassals = f64::from(patron_total_vassals);
    let vassal_factor = (0.25 * total_vassals).min(1.0);

    let effective_loyalty =
        (loyalty * (1.0 + (time_real / real_cap) * (time_game / game_cap))).round();
    let effective_leadership = (leadership
        * (1.0
            + 0.3 * vassal_factor
            + 0.7 * (time_real_avg / real_cap) * (time_game_avg / game_cap)))
        .round();

    // generated: 50 + 22.5 * E/291 on own XP, 5.5 * E/291 passed through
    let (factor1, factor2) = if direct {
        (50.0f64, 22.5f64)
    } else {
        (0.0, 5.5)
    };
    // received: 50 + 22.5 * E/291 on both steps
    let (received1, received2) = (50.0f64, 22.5f64);

    let generated = ((factor1 + factor2 * (effective_loyalty / skill_cap)) * 0.01) as f32;
    let received = ((received1 + received2 * (effective_leadership / skill_cap)) * 0.01) as f32;

    PassupFactors {
        generated,
        received,
    }
}

/// `amount * fraction`, at double precision and rounded to the nearest whole XP.
///
/// Not ACE's (retail, V285): ACE multiplies in `float` and truncates.
/// The retail captures' tithes fit the double product of the `float` percentage, rounded (2 x
/// 130,000,000 + 8 x 13,000,000 XP at E 160 tithe 227,030,934, where truncation gives
/// 227,030,926).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]
pub fn passup_amount(amount: u64, fraction: f32) -> u64 {
    (amount as f64 * f64::from(fraction)).round() as u64
}

// ACE: AllegianceManager.DoPassXP
/// The pass-up itself, recursive up the chain. A node that has left its allegiance by the time
/// this runs passes nothing (ACE's delegate holds the old node; see `allegiance.rs`).
///
/// Not ACE's (retail, V285; the retail captures): the vassal side uses the real time
/// sworn and a whole effective Loyalty, the pass-through step generates `5.5% * E/291`, and the
/// amounts are rounded at double precision ([`passup_factors`], [`passup_amount`]). The patron
/// side (V299) uses a whole effective Leadership from its vassal count and their average time
/// sworn, receives `50 + 22.5 * E/291` of the vassal's rounded tithe on every step, and rounds
/// its gain again ([`PassupFactors::amounts`]).
pub fn do_pass_xp(w: &mut World, vassal_node: NodeRef, amount: u64, direct: bool) {
    // http://asheron.wikia.com/wiki/Allegiance_Experience

    // Pre-patch:
    // Vassal-to-patron pass-up has no effective cap, but patron-to-grandpatron pass-up caps with an effective Loyalty of 175.
    // If you're sworn for 10 days to the same patron, the base Loyalty required to maximize the pass-up from your vassal through you to your patron is only 88.
    // Take into account level 7 enchantments, and you can see how there's practically no need to spend XP on the skill, due to the ease in reaching the cap.
    // Leadership is arguably worse off. In theory, you need to train Leadership and spend XP on it in order to get maximum results.
    // However, effective Leadership is multiplied by two factors: how many vassals you have, and how long they have been sworn to you, with the emphasis on the number of vassals you have.
    // The effect of Leadership on pass-up caps at around 165 effective Leadership, or 83 base Leadership before the modifier.
    // The end result of this is that if you have two active vassals and you can get 10 mules sworn underneath you for an average of 5 in-game days,
    // you never need to raise your Leadership beyond 83. Again, take into account level 7 enchantments and you can see why few people even bother training the skill. It's just too easy to reach the cap.

    // Post-patch:
    // - Leadership and Loyalty are not based on Self attribute
    // - Effective Loyalty is still modified by the time you have spent sworn to your patron
    // - Effective Leadership is still modified by number of vassals and average time that they have been sworn to you,
    //   but the emphasis is on the "time sworn" side and not on the "number of vassals" side. In fact, the vassals
    //   required to achieve the maximum benefit has been decreased from 12 to 4. This is to reduce the incentive of having non-playing vassals.
    // - For both Loyalty and Leadership, the time sworn modifier will now be a factor of both in-game time and real time.
    // - Most importantly, Leadership and Loyalty no longer "cap"

    // XP pass-up:
    // - New minimums and maximums
    // - Vassal-to-patron pass-up will have a minimum of 25% of earned XP, and a maximum of 90% of earned XP.
    //   Under the old system, the minimum was about 9% of earned XP, and the effective maximum was somewhere near 44% of earned XP.
    // - Patron-to-grandpatron pass-up will have a minimum of 0% of XP passed-up by the patron's vassal, and a maximum of 10% of passed-up XP.
    //   Under the old system, the minimum was about 30% and the maximum was about 94%.

    // Original system: up to January 12, 2004
    // Follow-up: all XP instead of just kill XP: October 2009

    // Formulas:
    // http://asheron.wikia.com/wiki/XP_Passup

    // Thanks for Xerxes of Thistledown, who verified accuracy over four months of testing and research!

    // Generated % - Percentage of XP passed to the patron through the vassal's earned XP (hunting and most quests).
    // Received % - Percentage of XP that patron will receive from his vassal's Generated XP.
    // Passup % -  Percentage of XP actually received by patron from vassal's earned XP (hunting and most quests).

    // Generated % = 50.0 + 22.5 * (loyalty / 291) * (1.0 + (RT/730) * (IG/720))
    // Received % = 50.0 + 22.5 * (leadership / 291) * (1.0 + V * (RT2/730) * (IG2/720))
    // Passup % = Generated% * Received% / 100.0

    // Where:
    // Loyalty = Buffed Loyalty (291 max)
    // Leadership = Buffed Leadership (291 max)
    // RT = actual real time sworn to patron in days (730 max)
    // IG = actual in-game time sworn to patron in hours (720 max)
    // RT2 = average real time sworn to patron for all vassals in days (730 max)
    // IG2 = average in-game time sworn to patron for all vassals in hours (720 max)
    // V = vassal factor(1 = 0.25, 2 = 0.50, 3 = 0.75, 4 + = 1.00) (1.0 max)

    let Some(tree) = allegiance::tree(w, vassal_node.allegiance) else {
        return;
    };
    let Some(id) = allegiance::member_node(w, vassal_node) else {
        return;
    };
    let Some(patron_id) = tree.node(id).patron else {
        return;
    };
    let patron_guid = tree.node(patron_id).player_guid;
    let patron_total_vassals = tree.total_vassals(patron_id);
    let patron_vassals: Vec<ObjectGuid> = tree
        .vassals(patron_id)
        .into_iter()
        .map(|v| tree.node(v).player_guid)
        .collect();
    let patron_node = NodeRef {
        allegiance: vassal_node.allegiance,
        player: patron_guid,
    };

    let vassal = allegiance_node::player(w, vassal_node.player)
        .expect("ACE: vassalNode.Player is null (NullReferenceException)");
    let patron = allegiance_node::player(w, patron_guid)
        .expect("ACE: patronNode.Player is null (NullReferenceException)");

    if !i_player::existed_before_allegiance_xp_changes(w, vassal) {
        return;
    }

    let current_loyalty = pa::i_player_get_current_loyalty(w, vassal);
    let current_leadership = pa::i_player_get_current_leadership(w, patron);

    // Not ACE's (retail, V285; the retail captures): the vassal's own time sworn, and
    // the average of the patron's direct vassals', in place of ACE's pinned maximum.
    let vassal_time = SwornDays::from(pa::i_player_sworn_time(w, vassal));
    let vassals_times: Vec<pa::SwornTime> = patron_vassals
        .into_iter()
        .filter_map(|g| allegiance_node::player(w, g))
        .map(|p| pa::i_player_sworn_time(w, p))
        .collect();
    let vassals_avg_time = sworn_average(&vassals_times);

    let factors = passup_factors(
        current_loyalty,
        current_leadership,
        patron_total_vassals,
        direct,
        vassal_time,
        vassals_avg_time,
    );

    // Not ACE's (retail, V285, V299): the tithe, then the patron's share of it, each at double
    // precision and rounded (see `PassupFactors::amounts`).
    let (generated_amount, passup_amount) = factors.amounts(amount);

    /*Console.WriteLine("---");
    Console.WriteLine("AllegianceManager.PassXP(" + amount + ")");
    Console.WriteLine("Vassal: " + vassal.Name);
    Console.WriteLine("Patron: " + patron.Name);

    Console.WriteLine("Generated: " + Math.Round(generated * 100, 2) + "%");
    Console.WriteLine("Received: " + Math.Round(received * 100, 2) + "%");
    Console.WriteLine("Passup: " + Math.Round(passup * 100, 2) + "%");

    Console.WriteLine("Generated amount: " + generatedAmount);
    Console.WriteLine("Passup amount: " + passupAmount);*/

    if passup_amount > 0 {
        //vassal.CPTithed += generatedAmount;
        //patron.CPCached += passupAmount;
        //patron.CPPoolToUnload += passupAmount;

        let generated_total =
            pa::i_player_allegiance_xp_generated(w, vassal).wrapping_add(generated_amount);
        pa::i_player_set_allegiance_xp_generated(w, vassal, generated_total);

        let cached = i_player::allegiance_xp_cached(w, patron).wrapping_add(passup_amount);
        if property_manager::get_bool(w, "offline_xp_passup_limit", false, true).item {
            pa::i_player_set_allegiance_xp_cached(w, patron, cached.min(u64::from(u32::MAX)));
        } else {
            pa::i_player_set_allegiance_xp_cached(w, patron, cached);
        }

        if let Some(online_patron) = player_manager::get_online_player(w, patron.guid().full()) {
            pa::add_allegiance_xp(w, online_patron);
        }

        // call recursively
        do_pass_xp(w, patron_node, passup_amount, false);
    }
}

// ACE: AllegianceManager.OnSwearAllegiance
/// Updates the Allegiance tree structure when a new player joins (`vassal`: the vassal swearing
/// into the Allegiance).
///
/// Not ACE's (a fix, V262): the approved-vassal cleanup runs on the live
/// allegiance after `Rebuild`. ACE runs it on the instance fetched before `Rebuild` replaced it,
/// so the live allegiance kept the vassal as approved and its next save wrote the entry back.
pub fn on_swear_allegiance(w: &mut World, vassal: ObjectGuid) {
    let vassal_player = IPlayer::Online(vassal);

    // was this vassal previously a Monarch?
    if let Some(previous) = pa::i_player_allegiance(w, vassal_player) {
        remove_cache(w, previous);
    }

    // rebuild the new combined structure
    let allegiance = get_allegiance(w, Some(vassal_player));
    rebuild(w, allegiance);

    load_player(w, Some(vassal_player));

    // maintain approved vassals list (on the allegiance the rebuild left the vassal in)
    let live = pa::i_player_allegiance(w, vassal_player);
    if let Some(live) = allegiance.and(live) {
        let vassal_id = vassal.full();
        if w.objects
            .get(live)
            .is_some_and(|o| allegiance::has_approved_vassal(o, vassal_id))
        {
            allegiance::remove_approved_vassal(w, live, vassal_id);
        }
    }
}

// ACE: AllegianceManager.OnBreakAllegiance
/// Updates the Allegiance tree structure when a member leaves. `self_` is the player initiating
/// the break request, `target` the patron or vassal of the self player.
///
/// Not ACE's (a fix, V263): when `self`'s `Allegiance` was never linked (an
/// offline member of an allegiance not rebuilt since start-up, booted by
/// `HandleActionBreakAllegianceBoot`), the previous structure is the allegiance whose tree holds
/// `self`'s node in the `Players` lookup table. ACE throws `NullReferenceException` here, after
/// the member's patron and monarch ids were cleared and saved: the tree was not rebuilt and the
/// booter was not told.
pub fn on_break_allegiance(w: &mut World, self_: Option<IPlayer>, target: Option<IPlayer>) {
    // remove the previous allegiance structure
    if let Some(s) = self_ {
        // ??
        let previous = pa::i_player_allegiance(w, s)
            .or_else(|| w.allegiance_manager.players.get(&s.guid()).copied());
        if let Some(previous) = previous {
            remove_cache(w, previous);
        }
    }

    // rebuild for self and target
    let self_allegiance = get_allegiance(w, self_);
    let target_allegiance = get_allegiance(w, target);

    rebuild(w, self_allegiance);
    rebuild(w, target_allegiance);

    load_player(w, self_);
    load_player(w, target);

    handle_no_allegiance(w, self_);
    handle_no_allegiance(w, target);
}

// ACE: AllegianceManager.HandleNoAllegiance
pub fn handle_no_allegiance(w: &mut World, player: Option<IPlayer>) {
    let Some(player) = player else { return };
    if pa::i_player_allegiance(w, player).is_some() {
        return;
    }

    let online_player = player_manager::get_online_player(w, player.guid().full());

    let mut updated = false;

    if i_player::monarch_id(w, player).is_some() {
        pa::i_player_update_property_iid(w, player, PropertyInstanceId::Monarch, None, true);

        updated = true;
    }

    if i_player::allegiance_rank(w, player).is_some() {
        i_player::set_allegiance_rank(w, player, None);

        if let Some(online_player) = online_player {
            let session = player_manager::player_session(w, online_player)
                .expect("ACE: Player.Session is null (NullReferenceException)");
            let msg = game_message_private_update_property_int(
                w.objects.get_mut(online_player).expect("online"),
                PropertyInt::AllegianceRank,
                0,
            );
            game_message::enqueue_send(w, session, msg);
        }

        updated = true;
    }

    if updated {
        i_player::save_biota_to_database(w, player, true);
    }

    if let Some(online_player) = online_player {
        let allegiance = pa::i_player_allegiance(w, IPlayer::Online(online_player));
        let node = pa::i_player_allegiance_node(w, IPlayer::Online(online_player));
        allegiance::send_allegiance_update(w, online_player, allegiance, node);
    }
}

// ACE: AllegianceManager.FindAllegiance
#[must_use]
pub fn find_allegiance(w: &World, allegiance_id: u32) -> Option<ObjectGuid> {
    let guid = ObjectGuid::new(allegiance_id);
    w.allegiance_manager
        .allegiances
        .contains_key(&guid)
        .then_some(guid)
}

// ACE: AllegianceManager.HandlePlayerDelete
/// Called from a database callback: the work runs on the world queue.
pub fn handle_player_delete(w: &mut World, player_guid: u32) {
    world_manager::enqueue_action(
        w,
        Action::delegate(move |w: &mut World| do_handle_player_delete(w, player_guid)),
    );
}

// ACE: AllegianceManager.DoHandlePlayerDelete
pub fn do_handle_player_delete(w: &mut World, player_guid: u32) {
    let Some(player) = player_manager::find_by_guid(w, player_guid).0 else {
        // DIVERGE: ACE writes this line to the console.
        log::warn!(
            "AllegianceManager.HandlePlayerDelete({player_guid:08X}): couldn't find player guid"
        );
        return;
    };
    let Some(allegiance) = get_allegiance(w, Some(player)) else {
        return;
    };

    let tree = allegiance::tree(w, allegiance).expect("loaded").clone();
    let allegiance_node = allegiance::member_node(
        w,
        NodeRef {
            allegiance,
            player: player.guid(),
        },
    );

    let mut players = vec![player];

    if let Some(patron_id) = i_player::patron_id(w, player) {
        if let Some(patron) = player_manager::find_by_guid(w, patron_id).0 {
            players.push(patron);
        }
    }

    i_player::set_patron_id(w, player, None);
    pa::i_player_update_property_iid(w, player, PropertyInstanceId::Monarch, None, true);

    // vassals now become monarchs...
    let allegiance_node =
        allegiance_node.expect("ACE: allegianceNode is null (NullReferenceException)");
    for vassal_node in tree.vassals(allegiance_node) {
        let vassal_guid = tree.node(vassal_node).player_guid;
        let Some(vassal) = player_manager::find_by_guid(w, vassal_guid.full()).0 else {
            continue;
        };

        i_player::set_patron_id(w, vassal, None);
        pa::i_player_update_property_iid(w, vassal, PropertyInstanceId::Monarch, None, true);

        // walk the allegiance tree from this node, update monarch ids
        for node in tree.walk(vassal_node, false) {
            let node_player = allegiance_node::player(w, tree.node(node).player_guid)
                .expect("ACE: node.Player is null (NullReferenceException)");
            pa::i_player_update_property_iid(
                w,
                node_player,
                PropertyInstanceId::Monarch,
                Some(vassal_guid.full()),
                true,
            );

            i_player::save_biota_to_database(w, node_player, true);
        }

        players.push(vassal);
    }

    remove_cache(w, allegiance);

    // rebuild for those directly involved
    for &p in &players {
        let a = get_allegiance(w, Some(p));
        rebuild(w, a);
    }

    for &p in &players {
        load_player(w, Some(p));
    }

    for &p in &players {
        handle_no_allegiance(w, Some(p));
    }

    // save immediately?
    for &p in &players {
        i_player::save_biota_to_database(w, p, true);
    }

    for &p in &players {
        pa::check_allegiance_house(w, p.guid());

        let new_allegiance = get_allegiance(w, Some(p));
        if let Some(new_allegiance) = new_allegiance {
            walk_check_allegiance_house(w, new_allegiance);
        }
    }
}

/// `allegiance.Monarch.Walk((node) => Player.CheckAllegianceHouse(node.PlayerGuid), false)`.
pub(crate) fn walk_check_allegiance_house(w: &mut World, allegiance: ObjectGuid) {
    let tree = allegiance::tree(w, allegiance).expect("loaded").clone();
    let monarch = allegiance::fields_of(w, allegiance)
        .expect("loaded")
        .monarch_node();
    for node in tree.walk(monarch, false) {
        pa::check_allegiance_house(w, tree.node(node).player_guid);
    }
}
