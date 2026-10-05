//! The spell bar, the spell book, the vitae lamp, the spell-component tally and the purge of
//! timed enchantments.
//!
//! Every scenario drives a model-only client: messages delivered through `Inbound`, requests
//! made through `Player`, and what the client sent read back as its outbound requests. No data
//! file is opened.

use dereth_client_contract::UiRequest;
use dereth_client_model::Request;
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::qualities::{MagicRemoveSpell, MagicUpdateSpell};
use dereth_protocol::types::qualities::StatMod;
use dereth_testkit::{HeadlessClient, Inbound, Player};
use dereth_ui_screens::hud::indicators;
use dereth_ui_screens::view::GameView as _;

const PLAYER: ObjectId = ObjectId(0x5000_0001);

// =============================================================================================
// 1. spellbar.favorite.removal-reaches-the-copy-a-relog-reads
// =============================================================================================

/// A spell taken off the bar leaves the row the client draws, tells the shard, and reaches the
/// packed copy the client sends back at the next options save.
pub fn a_favorite_removal_reaches_the_packed_copy() {
    let mut c = HeadlessClient::model();
    {
        let w = c.world_mut();
        w.player_system.module = Some(dereth_protocol::login::PlayerModule {
            spell_bars: vec![Vec::new(); 8],
            ..dereth_protocol::login::PlayerModule::default()
        });
        w.player_system.spell_tabs[2] = vec![11];
    }

    c.when(Player::ui(UiRequest::AddSpellFavorite {
        spell_id: 12,
        index: 2,
        tab: 2,
    }));
    let packed_after_add = c
        .view()
        .world()
        .player_system
        .client_packed_module()
        .expect("a module to re-pack")
        .spell_bars[2]
        .clone();

    c.when(Player::ui(UiRequest::RemoveSpellFavorite {
        spell_id: 11,
        tab: 2,
    }));
    let drawn = c.view().world().player_system.spell_tabs[2].clone();
    let packed_after_remove = c
        .view()
        .world()
        .player_system
        .client_packed_module()
        .expect("a module to re-pack")
        .spell_bars[2]
        .clone();

    c.assert_behaviour(
        "spellbar.favorite.removal-reaches-the-copy-a-relog-reads",
        move |v| {
            let told = matches!(
                v.outbound().last(),
                Some(Request::RemoveSpellFavorite(m)) if (m.spell_id, m.spell_bank) == (11, 2)
            );
            packed_after_add == vec![11, 12]
                && drawn == vec![12]
                && packed_after_remove == vec![12]
                && told
        },
    );
}

// =============================================================================================
// 2. spellbook.delete.asks-the-shard-and-predicts-nothing
// =============================================================================================

/// Deleting a spell asks the shard and changes nothing locally.
pub fn deleting_a_spell_predicts_nothing() {
    let mut c = HeadlessClient::model();
    c.world_mut().player_system.spell_tabs[0] = vec![77];
    c.when(Player::ui(UiRequest::RemoveSpell { spell_id: 77 }));

    c.assert_behaviour(
        "spellbook.delete.asks-the-shard-and-predicts-nothing",
        |v| {
            let asked = matches!(
                v.outbound(),
                [Request::RemoveSpell(m)] if m.layered_spell_id == 77
            );
            // The bar the client draws is untouched: the shard's answer is what removes it.
            asked
                && v.world().player_system.spell_tabs[0] == vec![77]
                && v.interaction().stats.spells_deleted == 1
        },
    );
}

// =============================================================================================
// 3. spellbook.learned-spell.appears-without-a-relog
// =============================================================================================

const SPELL: u32 = 157;
const OTHER_SPELL: u32 = 2051;

/// A client whose character has a description, which is the gate the handler opens with.
fn a_described_player() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    c.world_mut()
        .seed_player_desc(PLAYER, dereth_client_model::Qualities::default());
    c.hud_mut().player_desc_received = true;
    c
}

fn book_holds(c: &HeadlessClient, spell: u32) -> bool {
    c.view()
        .world()
        .player_qualities()
        .and_then(|q| q.spell_book.as_ref())
        .is_some_and(|b| b.contains_key(&spell))
}

fn the_panel_knows(c: &HeadlessClient, spell: u32) -> bool {
    let v = c.view();
    v.hud().view(v.objects()).is_spell_known(spell)
}

/// A spell learned in a session is in the book straight away.
pub fn a_learned_spell_appears_without_a_relog() {
    let mut c = a_described_player();
    let unknown_before = !book_holds(&c, SPELL) && !the_panel_knows(&c, SPELL);
    let no_book_yet = c
        .view()
        .world()
        .player_qualities()
        .and_then(|q| q.spell_book.as_ref())
        .is_none();

    c.when(Inbound::message(&MagicUpdateSpell {
        layered_spell_id: SPELL,
    }));
    let learned =
        book_holds(&c, SPELL) && the_panel_knows(&c, SPELL) && !the_panel_knows(&c, OTHER_SPELL);

    // A repeat leaves the page already there alone, rather than resetting what the login gave.
    c.world_mut()
        .player_qualities_mut()
        .expect("held")
        .spell_book
        .as_mut()
        .expect("made")
        .get_mut(&SPELL)
        .expect("added")
        .casting_likelihood = 0.75;
    c.when(Inbound::message(&MagicUpdateSpell {
        layered_spell_id: SPELL,
    }));
    let repeat_kept = c
        .view()
        .world()
        .player_qualities()
        .and_then(|q| q.spell_book.as_ref())
        .is_some_and(|b| (b[&SPELL].casting_likelihood - 0.75).abs() < 1e-6);

    // Unlearning takes only that one back out.
    c.when(Inbound::message(&MagicUpdateSpell {
        layered_spell_id: OTHER_SPELL,
    }))
    .when(Inbound::message(&MagicRemoveSpell {
        layered_spell_id: SPELL,
    }));
    let unlearned = !book_holds(&c, SPELL)
        && book_holds(&c, OTHER_SPELL)
        && !the_panel_knows(&c, SPELL)
        && the_panel_knows(&c, OTHER_SPELL);

    // One learned before the character's own description has arrived is dropped, not stored.
    let mut early = HeadlessClient::model();
    early.when(Inbound::message(&MagicUpdateSpell {
        layered_spell_id: SPELL,
    }));
    let dropped = early.view().hud().stats.spells_added == 1 && !book_holds(&early, SPELL);

    c.assert_behaviour(
        "spellbook.learned-spell.appears-without-a-relog",
        move |v| {
            unknown_before
                && no_book_yet
                && learned
                && repeat_kept
                && unlearned
                && dropped
                && v.hud().stats.spells_removed == 1
        },
    );
}

// =============================================================================================
// 4. vitae.lamp.lights-only-for-a-real-penalty
// =============================================================================================

/// The lamp's place in the indicator row.
const VITAE_LAMP: usize = 5;
/// A one-death penalty.
const FIVE_PERCENT: f32 = 0.95;

fn vitae_enchantment(multiplier: f32) -> dereth_rules::enchant::Enchantment {
    dereth_rules::enchant::Enchantment {
        id: 666,
        spell_category: 0,
        power_level: 0,
        start_time: 0.0,
        duration: -1.0,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind: dereth_rules::enchant::ench_type::VITAE,
            key: 0,
            value: multiplier,
        },
        spell_set_id: None,
    }
}

fn lamp(c: &HeadlessClient) -> (Option<f32>, Option<u32>) {
    let v = c.view();
    let view = v.hud().view(v.objects());
    (view.vitae(), indicators::all_states(&view, 0)[VITAE_LAMP])
}

/// The lamp is lit only while there is a penalty, and reads the same as it always did when
/// there is not.
pub fn the_vitae_lamp_lights_only_for_a_penalty() {
    let mut c = HeadlessClient::model();
    // Nothing is known yet: there is no description to ask, and the lamp is dark.
    let unasked = lamp(&c) == (None, Some(indicators::STATE_NOTHING));

    c.world_mut().seed_player_desc(
        ObjectId(0x5000_000A),
        dereth_client_model::Qualities::default(),
    );
    c.hud_mut().player_desc_received = true;
    // A character with no vitae: the lamp draws exactly what it drew before it had a source.
    let healthy = lamp(&c) == (Some(1.0), Some(indicators::STATE_NOTHING));

    c.world_mut()
        .player_qualities_mut()
        .expect("seeded")
        .enchantments
        .vitae = Some(vitae_enchantment(FIVE_PERCENT));
    let penalised = lamp(&c) == (Some(FIVE_PERCENT), Some(1));
    let registry_agrees = c
        .view()
        .world()
        .player_qualities()
        .is_some_and(|q| (q.enchantments.vitae_value() - FIVE_PERCENT).abs() < f32::EPSILON);

    c.world_mut()
        .player_qualities_mut()
        .expect("seeded")
        .enchantments
        .vitae = None;
    let removed = lamp(&c) == (Some(1.0), Some(indicators::STATE_NOTHING));

    c.assert_behaviour("vitae.lamp.lights-only-for-a-real-penalty", move |_| {
        unasked && healthy && penalised && registry_agrees && removed
    });
}

// =============================================================================================
// 5. spell-components.the-pack-is-counted-as-the-player-fills-it
// 6. spell-components.what-the-player-asks-to-keep-reaches-the-shard-and-the-vendor
// 7. enchantments.a-purge-takes-the-timed-ones-and-leaves-the-permanent
// =============================================================================================
//
// The components are synthetic and said to be: the catalogue the client joins a component to is a
// host-side read of the shipped tables, and this tier opens no dat. What is **not** synthetic is
// the route -- every component below reaches the tally through the client's own "the shard moved an
// item" seam or through the sweep the character's own description runs, and never by writing the
// tally.

/// The two kinds of item the tally's gate asks about.
const TYPE_SPELL_COMPONENTS: u32 = 0x0000_1000;
const TYPE_CONTAINER: u32 = 0x0000_0200;
/// The largest number of a component the panel will accept, and the first it will not.
const MOST_THE_PANEL_TAKES: i32 = 5000;
const ONE_TOO_MANY: i32 = 5001;

/// A three-component catalogue: two of one category, so the pair shares a list, and one of
/// another.
fn a_catalogue() -> dereth_client_model::magic::ComponentCatalogue {
    use dereth_client_model::magic::ComponentBase;
    let base = ComponentBase::default;
    dereth_client_model::magic::ComponentCatalogue::new(
        [(1_u32, 100_u32), (2, 200), (3, 300)],
        [
            (
                1_u32,
                ComponentBase {
                    name: "Lead Scarab".into(),
                    category: 0,
                    ..base()
                },
            ),
            (
                2,
                ComponentBase {
                    name: "Iron Scarab".into(),
                    category: 0,
                    ..base()
                },
            ),
            (
                3,
                ComponentBase {
                    name: "Red Taper".into(),
                    category: 2,
                    ..base()
                },
            ),
        ],
    )
}

fn put(
    w: &mut dereth_client_model::World,
    id: ObjectId,
    pwd: dereth_protocol::types::PublicWeenieDesc,
) {
    let mut wn = dereth_client_model::Weenie::new(id);
    wn.pwd = pwd;
    w.tables.weenies.insert(id, wn);
}

/// A described player with the catalogue loaded and nothing in his pack.
fn a_player_with_a_catalogue() -> HeadlessClient {
    let mut c = HeadlessClient::model();
    let w = c.world_mut();
    w.player = Some(PLAYER);
    put(
        w,
        PLAYER,
        dereth_protocol::types::PublicWeenieDesc {
            name: "Lark".to_owned(),
            obj_type: dereth_rules::weenie::item_type::CREATURE,
            ..dereth_protocol::types::PublicWeenieDesc::default()
        },
    );
    if let Some(x) = w.tables.weenies.get_mut(PLAYER) {
        x.qualities = Some(dereth_client_model::qualities::Qualities::default());
    }
    w.magic.catalogue = a_catalogue();
    c
}

/// Put an item in a container and let the container's own list know about it, which is what the
/// shard's contents message does.
fn a_component(
    w: &mut dereth_client_model::World,
    container: ObjectId,
    id: ObjectId,
    wcid: u32,
    name: &str,
    stack: Option<u16>,
) {
    put(
        w,
        id,
        dereth_protocol::types::PublicWeenieDesc {
            name: name.to_owned(),
            wcid,
            obj_type: TYPE_SPELL_COMPONENTS,
            stack_size: stack,
            container_id: Some(container),
            ..dereth_protocol::types::PublicWeenieDesc::default()
        },
    );
    refresh_contents(w, container);
}

fn refresh_contents(w: &mut dereth_client_model::World, container: ObjectId) {
    let held: Vec<dereth_protocol::types::ContentProfile> = w
        .tables
        .weenies
        .iter()
        .filter(|(_, x)| x.pwd.container_id == Some(container))
        .map(|(k, x)| dereth_protocol::types::ContentProfile {
            iid: k,
            container_properties: u32::from(x.is_container()),
        })
        .collect();
    w.view_object_contents(
        container,
        &held,
        &mut dereth_client_model::RecordingSink::default(),
    );
}

/// The shard moving an item into a container, which is the seam the tally is filled from.
fn moved_into(w: &mut dereth_client_model::World, id: ObjectId, container: ObjectId) {
    w.server_says_move_item(
        id,
        container,
        0,
        ObjectId(0),
        0,
        true,
        &mut dereth_client_model::RecordingSink::default(),
    );
}

/// What the player is carrying is counted as it reaches him.
pub fn the_component_tally_follows_the_pack() {
    let mut c = a_player_with_a_catalogue();
    let nothing_yet = {
        let w = c.view().world();
        w.magic.components.tracked_objects() == 0 && !w.magic.components.component_is_owned(100)
    };

    // Two of one kind: a stack of fifty, and one that carries no stack size at all.
    let a = ObjectId(0x8000_0001);
    let b = ObjectId(0x8000_0002);
    {
        let w = c.world_mut();
        a_component(w, PLAYER, a, 100, "Lead Scarab", Some(50));
        a_component(w, PLAYER, b, 100, "Lead Scarab", None);
        moved_into(w, a, PLAYER);
        moved_into(w, b, PLAYER);
    }
    let counted = {
        let w = c.view().world();
        w.magic.components.tracked_objects() == 2
            && w.magic.components.component_is_owned(100)
            // fifty from the stack, and one for the object that carries no stack size.
            && w.magic.components.num_component(&w.magic.catalogue, 100) == 51
            && w.magic.components.object_is_owned_component(a) == Some(100)
    };

    // Something that is not a component, in the same pack, is not counted.
    let sceptre = ObjectId(0x8000_0003);
    {
        let w = c.world_mut();
        put(
            w,
            sceptre,
            dereth_protocol::types::PublicWeenieDesc {
                name: "Sandstone Sceptre".to_owned(),
                wcid: 999,
                obj_type: dereth_rules::weenie::item_type::MISC,
                container_id: Some(PLAYER),
                ..dereth_protocol::types::PublicWeenieDesc::default()
            },
        );
        refresh_contents(w, PLAYER);
        moved_into(w, sceptre, PLAYER);
    }
    let gated = c.view().world().magic.components.tracked_objects() == 2;

    // One of the two goes elsewhere: the kind is still owned, because the other one holds it.
    let elsewhere = ObjectId(0x7000_0001);
    {
        let w = c.world_mut();
        put(
            w,
            elsewhere,
            dereth_protocol::types::PublicWeenieDesc::default(),
        );
        if let Some(x) = w.tables.weenies.get_mut(a) {
            x.pwd.container_id = Some(elsewhere);
        }
        refresh_contents(w, PLAYER);
        moved_into(w, a, elsewhere);
    }
    let still_owned = {
        let w = c.view().world();
        w.magic.components.tracked_objects() == 1
            && w.magic.components.component_is_owned(100)
            && w.magic.components.num_component(&w.magic.catalogue, 100) == 1
    };

    // **The side pack**, walked by the sweep the character's own description runs.
    let mut p = a_player_with_a_catalogue();
    let pack = ObjectId(0x8000_0100);
    {
        let w = p.world_mut();
        put(
            w,
            pack,
            dereth_protocol::types::PublicWeenieDesc {
                name: "Pack".to_owned(),
                obj_type: TYPE_CONTAINER,
                items_capacity: Some(24),
                container_id: Some(PLAYER),
                ..dereth_protocol::types::PublicWeenieDesc::default()
            },
        );
        refresh_contents(w, PLAYER);
        for i in 0..3_u32 {
            a_component(
                w,
                pack,
                ObjectId(0x8000_0200 + i),
                200,
                "Iron Scarab",
                Some(u16::try_from(10 + i).expect("small")),
            );
        }
    }
    let first_sweep = p.world_mut().initialize_spell_components();
    let walked = first_sweep == (3, 3) && {
        let w = p.view().world();
        w.magic.components.num_component(&w.magic.catalogue, 200) == 10 + 11 + 12
    };
    // Sweeping again offers the same three and changes nothing, which is what stops the count
    // doubling every time the character's description arrives.
    let second_sweep = p.world_mut().initialize_spell_components();
    let idempotent = second_sweep == (3, 0)
        && p.view()
            .world()
            .magic
            .components
            .num_component(&p.view().world().magic.catalogue, 200)
            == 33;

    // **A stack that changes size** moves the count by the difference and can never un-own a kind.
    let mut s = a_player_with_a_catalogue();
    let taper = ObjectId(0x8000_0001);
    {
        let w = s.world_mut();
        a_component(w, PLAYER, taper, 300, "Red Taper", Some(40));
        moved_into(w, taper, PLAYER);
    }
    let before = {
        let w = s.view().world();
        w.magic.components.num_component(&w.magic.catalogue, 300) == 40
    };
    {
        let w = s.world_mut();
        if let Some(x) = w.tables.weenies.get_mut(taper) {
            x.pwd.stack_size = Some(5);
        }
        w.update_spell_component(taper);
    }
    let by_the_difference = {
        let w = s.view().world();
        w.magic.components.num_component(&w.magic.catalogue, 300) == 5
            && w.magic.components.component_is_owned(300)
    };

    c.assert_behaviour(
        "spell-components.the-pack-is-counted-as-the-player-fills-it",
        move |_| {
            nothing_yet
                && counted
                && gated
                && still_owned
                && walked
                && idempotent
                && before
                && by_the_difference
        },
    );
}

/// What the player asks to keep in stock reaches the shard, and the shop buys the gap.
pub fn the_desired_level_reaches_the_shard_and_the_vendor_buys_the_shortfall() {
    let mut c = a_player_with_a_catalogue();

    // A number the panel accepts: told to the shard, and remembered here.
    let mut asked = dereth_client_model::RecordingRequests::default();
    let answer = c
        .world_mut()
        .set_desired_component_level(&mut asked, 100, 12);
    let told = answer == 12
        && matches!(
            asked.0.as_slice(),
            [Request::SetDesiredComponentLevel(m)] if (m.component_did, m.level) == (100, 12)
        )
        && c.view().world().player_system.desired_comp_level(100) == 12;
    asked.0.clear();

    // The top of what it accepts still goes.
    let at_the_top =
        c.world_mut()
            .set_desired_component_level(&mut asked, 100, MOST_THE_PANEL_TAKES)
            == MOST_THE_PANEL_TAKES
            && asked.0.len() == 1;
    asked.0.clear();

    // One past it, and a negative, are neither sent nor remembered -- and the panel is handed the
    // stored number back rather than the one it was refused.
    let refused = c
        .world_mut()
        .set_desired_component_level(&mut asked, 100, ONE_TOO_MANY)
        == MOST_THE_PANEL_TAKES
        && asked.0.is_empty()
        && c.world_mut()
            .set_desired_component_level(&mut asked, 100, -1)
            == MOST_THE_PANEL_TAKES
        && asked.0.is_empty()
        && c.view().world().player_system.desired_comp_level(100) == MOST_THE_PANEL_TAKES;

    // **The shop.** The player wants ten Lead Scarabs and five Red Tapers and already holds four
    // Lead Scarabs; the shop stocks Lead Scarabs and no Red Tapers at all.
    let mut v = a_player_with_a_catalogue();
    let held = ObjectId(0x8000_0001);
    {
        let w = v.world_mut();
        a_component(w, PLAYER, held, 100, "Lead Scarab", Some(4));
        moved_into(w, held, PLAYER);
        w.player_system.set_desired_comp_level(100, 10);
        w.player_system.set_desired_comp_level(300, 5);
    }
    let stock = ObjectId(0x6000_0001);
    let info = dereth_protocol::trade::VendorInfo {
        merchant_id: ObjectId(0x7000_0001),
        profile: dereth_protocol::trade::VendorProfile {
            item_types: 0xFFFF_FFFF,
            sell_price: 1.0,
            ..dereth_protocol::trade::VendorProfile::default()
        },
        items: vec![dereth_protocol::trade::ItemProfile {
            iid: stock,
            // "unlimited", which is what almost every recorded stock row carries.
            amount: -1,
            pwd: Some(dereth_protocol::types::PublicWeenieDesc {
                name: "Lead Scarab".to_owned(),
                wcid: 100,
                obj_type: TYPE_SPELL_COMPONENTS,
                stack_size: Some(20),
                value: Some(400),
                ..dereth_protocol::types::PublicWeenieDesc::default()
            }),
        }],
    };
    let opened = {
        let w = v.world_mut();
        let mut notices = dereth_client_model::RecordingSink::default();
        let mut req = dereth_client_model::RecordingRequests::default();
        w.handle_vendor_info(&info, &mut notices, &mut req, ServerTime(0.0)) == 1
    };

    let mut said = dereth_client_model::RecordingSink::default();
    let filled = v.world_mut().fill_component_list(None, 0, &mut said);
    let basket = {
        let w = v.view().world();
        let pwd = w
            .shop
            .stock
            .iter()
            .find(|r| r.iid == stock)
            .expect("the stock row")
            .pwd
            .clone();
        // Six: ten wanted, less the four already held. A tally that had never been filled would
        // have asked for ten.
        w.shop.buy_list == vec![(stock, 6)]
            // and the running total is the shop's own price, not a second price chain here.
            && filled.total == w.shop.profile.vendor_sell_price(&pwd, 6)
    };
    let counted = filled.desired_rows == 2
        && filled.short_rows == 2
        && filled.not_stocked == 1
        && filled.added == 1
        && !filled.aborted;
    let named = said.0.iter().any(|n| {
        matches!(n, dereth_client_model::Notice::DisplayString { text, .. } if text.contains("Red Taper"))
    });

    c.assert_behaviour(
        "spell-components.what-the-player-asks-to-keep-reaches-the-shard-and-the-vendor",
        move |_| told && at_the_top && refused && opened && basket && counted && named,
    );
}

/// An enchantment the shard sent, through the production dispatch.
fn an_enchantment(spell_id: u16, duration: f64, kind: u32, category: u16) -> Inbound {
    let e = dereth_protocol::types::qualities::Enchantment {
        id: u32::from(spell_id),
        category_word: u32::from(category),
        power_level: 1,
        start_time: 0.0,
        duration,
        caster: ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind,
            key: 1,
            value: 1.0,
        },
        spell_set_id: None,
    };
    Inbound::message(&dereth_protocol::qualities::MagicUpdateEnchantment(e))
}

/// A purge, which carries no body at all.
fn a_purge(opcode: u32) -> Inbound {
    Inbound::event(dereth_client_net::client_session::SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode(opcode),
        blob: opcode.to_le_bytes().to_vec(),
    })
}

/// `Magic_PurgeEnchantments` and `Magic_PurgeBadEnchantments`, both body-less.
const PURGE: u32 = 0x02C6;
const PURGE_BAD: u32 = 0x0312;
/// A duration the client reads as "for ever".
const FOR_EVER: f64 = -1.0;
/// The last spell id that counts toward the two tallies, and the first that does not.
const LAST_COUNTED: u16 = 0x7FFF;
const FIRST_IGNORED: u16 = 0x8000;

fn an_enchanted_player(beneficial: &[(u32, bool)]) -> HeadlessClient {
    let mut c = a_player_with_a_catalogue();
    c.world_mut().magic.spell_beneficial = beneficial.iter().copied().collect();
    c
}

fn in_effect(c: &HeadlessClient) -> Vec<u16> {
    c.view()
        .world()
        .player_qualities()
        .map(|q| {
            q.enchantments
                .enchantments_in_effect()
                .iter()
                .map(|e| e.spell_id())
                .collect()
        })
        .unwrap_or_default()
}

fn tally(c: &HeadlessClient) -> (u32, u32) {
    c.view().world().player_qualities().map_or((0, 0), |q| {
        (q.enchantments.helpful_count, q.enchantments.harmful_count)
    })
}

/// A purge takes the timed enchantments and leaves the permanent ones.
pub fn a_purge_leaves_the_permanent_enchantments() {
    use dereth_rules::enchant::ench_type;

    // Spell 10 helps, spell 20 harms, spell 30 helps and is permanent.
    let mut c = an_enchanted_player(&[(10, true), (20, false), (30, true)]);
    for (id, duration, kind, cat) in [
        (
            10_u16,
            60.0_f64,
            ench_type::ADDITIVE | ench_type::BENEFICIAL,
            1_u16,
        ),
        (20, 60.0, ench_type::ADDITIVE, 2),
        (30, FOR_EVER, ench_type::ADDITIVE | ench_type::BENEFICIAL, 3),
    ] {
        c.when(an_enchantment(id, duration, kind, cat));
    }
    let all_three = in_effect(&c).len() == 3 && tally(&c) == (2, 1);

    c.when(a_purge(PURGE));
    let only_the_permanent = in_effect(&c) == [30]
        // the tally ends at the survivor, not at zero
        && tally(&c) == (1, 0)
        && c.view().interaction().stats.enchantments_purged == 1;

    // **The purge that takes only the harmful ones**: the buff and the permanent debuff both stay.
    let mut b = an_enchanted_player(&[(10, true), (20, false), (40, false)]);
    for (id, duration, kind, cat) in [
        (
            10_u16,
            60.0_f64,
            ench_type::ADDITIVE | ench_type::BENEFICIAL,
            1_u16,
        ),
        (20, 60.0, ench_type::ADDITIVE, 2),
        (40, FOR_EVER, ench_type::ADDITIVE, 4),
    ] {
        b.when(an_enchantment(id, duration, kind, cat));
    }
    b.when(a_purge(PURGE_BAD));
    let mut left = in_effect(&b);
    left.sort_unstable();
    let buffs_kept = left == [10, 40] && b.view().interaction().stats.bad_enchantments_purged == 1;

    // **An item's cooldown** is in force and counts toward neither tally. Both sides of the
    // boundary in one run, because one that only ever used the first ignored id could not tell
    // which of the two neighbouring numbers the line is drawn at.
    let mut d = an_enchanted_player(&[
        (10, true),
        (u32::from(LAST_COUNTED), true),
        (u32::from(FIRST_IGNORED), true),
    ]);
    for id in [10_u16, LAST_COUNTED, FIRST_IGNORED] {
        d.when(an_enchantment(
            id,
            60.0,
            ench_type::ADDITIVE | ench_type::BENEFICIAL,
            9,
        ));
    }
    let ceiling = tally(&d) == (2, 0) && in_effect(&d).len() == 3;

    c.assert_behaviour(
        "enchantments.a-purge-takes-the-timed-ones-and-leaves-the-permanent",
        move |_| all_three && only_the_permanent && buffs_kept && ceiling,
    );
}

// -------------------------------------------------------------------------------------------

dereth_testkit::scenarios! {
    scenario_a_favorite_removal_reaches_the_packed_copy => a_favorite_removal_reaches_the_packed_copy ["spellbar.favorite.removal-reaches-the-copy-a-relog-reads"],
    scenario_deleting_a_spell_predicts_nothing => deleting_a_spell_predicts_nothing ["spellbook.delete.asks-the-shard-and-predicts-nothing"],
    scenario_a_learned_spell_appears_without_a_relog => a_learned_spell_appears_without_a_relog ["spellbook.learned-spell.appears-without-a-relog"],
    scenario_the_vitae_lamp_lights_only_for_a_penalty => the_vitae_lamp_lights_only_for_a_penalty ["vitae.lamp.lights-only-for-a-real-penalty"],
    scenario_the_component_tally_follows_the_pack => the_component_tally_follows_the_pack ["spell-components.the-pack-is-counted-as-the-player-fills-it"],
    scenario_the_desired_level_reaches_the_shard_and_the_vendor_buys_the_shortfall => the_desired_level_reaches_the_shard_and_the_vendor_buys_the_shortfall ["spell-components.what-the-player-asks-to-keep-reaches-the-shard-and-the-vendor"],
    scenario_a_purge_leaves_the_permanent_enchantments => a_purge_leaves_the_permanent_enchantments ["enchantments.a-purge-takes-the-timed-ones-and-leaves-the-permanent"],
    scenario_spell_favorite_moves_model_and_wire => spell_favorite_moves_model_and_wire ["spellbar.favorite.model-and-wire-move-together"],
}

// -------------------------------------------------------------------------------------------
// 8. spellbar.favorite.model-and-wire-move-together
// -------------------------------------------------------------------------------------------

/// A spell dropped on the bar goes into the model the client draws and on to the wire together.
pub fn spell_favorite_moves_model_and_wire() {
    let mut c = HeadlessClient::model();
    c.world_mut().player_system.spell_tabs[0] = vec![5, 6];

    c.when(Player::ui(UiRequest::AddSpellFavorite {
        spell_id: 3,
        index: 3,
        tab: 0,
    }))
    .when(Player::ui(UiRequest::AddSpellFavorite {
        spell_id: 9,
        index: 1,
        tab: 0,
    }))
    // There is no ninth bank for the shard to put it in, so nothing is written and nothing is
    // sent. The whole of the guard, and the only case that separates "wrote it" from "sent it".
    .when(Player::ui(UiRequest::AddSpellFavorite {
        spell_id: 4,
        index: 0,
        tab: 8,
    }))
    .assert_behaviour("spellbar.favorite.model-and-wire-move-together", |v| {
        let drawn = v.world().player_system.spell_tabs[0] == vec![5, 9, 6, 3];
        let sent = matches!(
            v.outbound(),
            [Request::AddSpellFavorite(a), Request::AddSpellFavorite(b)]
                if (a.spell_id, a.index, a.spell_bank) == (3, 3, 0)
                    && (b.spell_id, b.index, b.spell_bank) == (9, 1, 0)
        );
        drawn && sent
    });
}
