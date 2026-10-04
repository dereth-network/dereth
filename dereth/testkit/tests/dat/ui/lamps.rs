//! UI fixtures and scenarios for lamps.

use super::*;
// =============================================================================================
// hud.lamp-row.* -- the six lamps and the way out, top left
//
// Every top-left lamp can be pressed, the way out raises its question, and buffs, a death
// penalty and an over-full pack each light their lamp. Seven scenarios, seven rows.
//
// **Every lamp claim is made unlit first and lit second on one client**, because a lamp lighting
// is an edge and a scenario that looks only once cannot see one.
//
// The character here is built by the shard stand-in the harness owns, and its strength is this
// scenario's own. Nothing the row claims depends on which strength it is: the lamp is read against
// whatever that character can carry, and the scenario works it out the way the client does.
// =============================================================================================

/// The six lamps and the button at the end of the row.
const ROW_LINK_LAMP: ElementId = ElementId(0x1000_00F8);
const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);
const DEBUFF_LAMP: ElementId = ElementId(0x1000_00F6);
pub(super) const VITAE_LAMP: ElementId = ElementId(0x1000_00F4);
pub(super) const BURDEN_LAMP: ElementId = ElementId(0x1000_00F7);
const EXIT_BUTTON: ElementId = ElementId(0x1000_00FA);

/// The panels the lamps' actions open, and the stack window that holds them.
const CHARACTER_INFO_PANEL: ElementId = ElementId(0x1000_0183);
const POSITIVE_MAGIC_PANEL: ElementId = ElementId(0x1000_0184);
const NEGATIVE_MAGIC_PANEL: ElementId = ElementId(0x1000_0185);
const LINK_STATUS_PANEL: ElementId = ElementId(0x1000_0187);
const VITAE_PANEL: ElementId = ElementId(0x1000_018A);
const PANEL_STACK: ElementId = ElementId(0x1000_05FF);

/// The character the lamp scenarios are about.
const LAMP_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);
/// What the character is carrying, and how many augmentations they have -- the two the client
/// works a carrying capacity out of.
const ENCUMB_VAL: u32 = 5;
const NUM_AUGMENTATIONS: u32 = 230;

/// The state an element of the strip is in.
fn strip_state(c: &HeadlessClient, id: ElementId) -> u32 {
    hud_state(c, hud_find(c, id)).0
}

/// Press something on the strip, through the channel a real press ends in.
///
/// **Two frames, and the second is not padding**: the press fires the action on the first, and the
/// showing it asks for reaches the panel stack on the second. That one-frame seam is the message
/// bus's, declared and pre-existing.
pub(super) fn press_the_strip(c: &mut HeadlessClient, id: ElementId) {
    let h = hud_find(c, id);
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    c.tick(2);
}

/// Put a lamp in a lit state, the way the lamp's own update does the moment the player gains
/// something to show.
///
/// This is not decoration: a lamp with nothing to show *is* a disabled button, and a disabled
/// button swallows its own press. So a scenario about pressing a lamp has to light it first, and
/// that is the client's behaviour rather than this scenario's convenience.
pub(super) fn light_the_lamp(c: &mut HeadlessClient, id: ElementId) {
    let h = hud_find(c, id);
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    shell.ui.set_state(h, StateId(1));
}

/// A gameplay client with a character the shard has described, so the lamps have qualities to
/// read.
pub(super) fn a_described_character() -> (HeadlessClient, dereth_testkit::Peer) {
    use dereth_protocol::types::qualities::{
        attribute_cache_mask as m, quality_flags, AcQualities, Attribute, AttributeCache,
    };

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut peer = dereth_testkit::Peer::attach_creating(&mut c, LAMP_PLAYER);
    c.world_mut().player = Some(LAMP_PLAYER);
    let cache = AttributeCache {
        flags: m::STRENGTH,
        strength: Some(Attribute {
            level_from_cp: 0,
            init_level: 100,
            cp_spent: 0,
        }),
        ..AttributeCache::default()
    };
    let d = dereth_protocol::login::LoginPlayerDescription {
        qualities: AcQualities {
            flags: quality_flags::ATTRIBUTE_CACHE,
            attribute_cache: Some(cache),
            ..AcQualities::default()
        },
        player_module: dereth_protocol::login::PlayerModule {
            spell_bars: vec![Vec::new()],
            spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
            ..dereth_protocol::login::PlayerModule::default()
        },
        content_profiles: Vec::new(),
        inventory_placements: Vec::new(),
    };
    peer.event(&mut c, &d);
    c.tick(4);
    (c, peer)
}

/// One int quality, through the production writer. The sequence has to advance: a stale one is
/// dropped, which is the gate that would make a silent no-op look like a lamp that will not light.
pub(super) fn set_int_quality(c: &mut HeadlessClient, sequence: u8, property: u32, value: i32) {
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::qualities::QualitiesPrivateUpdateInt(
            dereth_protocol::qualities::PrivateUpdate {
                sequence,
                property_id: property,
                value,
            },
        ),
    ));
    c.tick(1);
}

/// One enchantment, the way the only producer of one writes it.
pub(super) fn an_enchantment(
    spell: u16,
    category: u16,
    kind: u32,
    value: f32,
) -> dereth_protocol::types::qualities::Enchantment {
    dereth_protocol::types::qualities::Enchantment {
        id: u32::from(spell),
        category_word: u32::from(category),
        power_level: 1,
        start_time: 0.0,
        duration: 60.0,
        caster: dereth_primitives::ObjectId(0),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind,
            key: 1,
            value,
        },
        spell_set_id: None,
    }
}

pub(super) fn enchant(c: &mut HeadlessClient, e: dereth_protocol::types::qualities::Enchantment) {
    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::qualities::MagicUpdateEnchantment(e),
    ));
    c.tick(1);
}

/// One spell the shipped table really marks helpful and one it does not. A spell the table does
/// not carry moves neither counter, which is why an invented id would light no lamp and measure
/// nothing.
fn a_helpful_and_a_harmful_spell(c: &HeadlessClient) -> (u16, u16) {
    let app = c.view().expect_app();
    let t = app
        .hud()
        .spell_table
        .as_ref()
        .expect("the shipped spell table loads");
    let pick = |helpful: bool| {
        t.spells
            .iter()
            .find(|(_, s)| (s.bitfield & 4 != 0) == helpful)
            .map(|(id, _)| u16::try_from(*id).expect("a spell id fits"))
            .expect("the shipped table has one of each")
    };
    (pick(true), pick(false))
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session
// ---------------------------------------------------------------------------------------------

/// Two checkpoints: nothing up before the press, the question up after it -- a dialog that was
/// always up could not pass.
pub(super) fn the_way_out_of_the_strip_raises_the_question_about_ending_the_session() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));

    // It is a plain button and carries no action of its own, which is why its press reaches the
    // strip's own listener at all rather than being consumed on the way.
    let a_plain_button = {
        let h = hud_find(&c, EXIT_BUTTON);
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let n = ui.node(h).expect("a live node");
        n.ty().0 == 1
            && n.merged_properties()
                .get_enum(dereth_ui_screens::screens::gameplay::BUTTON_INPUT_ACTION)
                .is_none()
    };
    let nothing_up_first = {
        let (_, s) = hud_gameplay(&mut c);
        s.logout_dialog().is_none()
    };

    press_the_strip(&mut c, EXIT_BUTTON);

    let (the_question_is_up, the_asking_form, and_it_does_not_close_the_client) = {
        let (_, s) = hud_gameplay(&mut c);
        (
            s.logout_dialog().is_some(),
            s.logout_prompt.as_deref()
                == Some(dereth_ui_screens::screens::gameplay::logout::END_SESSION_CONFIRM),
            !s.should_quit_on_logout,
        )
    };

    c.assert_behaviour(
        "hud.lamp-row.the-way-out-of-the-strip-raises-the-question-about-ending-the-session",
        move |_| {
            a_plain_button
                && nothing_up_first
                && the_question_is_up
                && the_asking_form
                && and_it_does_not_close_the_client
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down
// ---------------------------------------------------------------------------------------------

/// Two checkpoints per lamp -- down, then up -- and the four panels that were not asked for are
/// read at the same moment, which is what stops "everything is visible" passing.
pub(super) fn every_lamp_opens_its_own_panel() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    let table = [
        (ROW_LINK_LAMP, 0x1000_0009_u32, LINK_STATUS_PANEL),
        (BUFF_LAMP, 0x1000_0006, POSITIVE_MAGIC_PANEL),
        (DEBUFF_LAMP, 0x1000_0007, NEGATIVE_MAGIC_PANEL),
        (VITAE_LAMP, 0x1000_000C, VITAE_PANEL),
        (BURDEN_LAMP, 0x1000_0005, CHARACTER_INFO_PANEL),
    ];

    // The layout half: each lamp carries the action the shipped layout gives it.
    let mut the_actions_are_authored = true;
    for (lamp, action, _) in table {
        let h = hud_find(&c, lamp);
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        the_actions_are_authored &= ui
            .node(h)
            .expect("a live node")
            .merged_properties()
            .get_enum(dereth_ui_screens::screens::gameplay::BUTTON_INPUT_ACTION)
            == Some(action);
    }

    // Checkpoint 1 -- every page is down, and the stack window with them.
    let mut all_down_first = !hud_visible(&c, hud_find(&c, PANEL_STACK));
    for (_, _, panel) in table {
        all_down_first &= !hud_visible(&c, hud_find(&c, panel));
    }

    // Checkpoint 2 -- one lamp at a time.
    let mut each_opens_its_own = true;
    for (lamp, _, panel) in table {
        light_the_lamp(&mut c, lamp);
        press_the_strip(&mut c, lamp);
        each_opens_its_own &=
            hud_visible(&c, hud_find(&c, panel)) && hud_visible(&c, hud_find(&c, PANEL_STACK));
        for (_, _, other) in table {
            if other != panel {
                each_opens_its_own &= !hud_visible(&c, hud_find(&c, other));
            }
        }
    }

    c.assert_behaviour(
        "hud.lamp-row.every-lamp-opens-its-own-panel-and-leaves-the-other-lamps-panels-down",
        move |_| the_actions_are_authored && all_down_first && each_opens_its_own,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing
// ---------------------------------------------------------------------------------------------

/// The negative half of the same arm, which is what stops "every press opens every panel" passing.
pub(super) fn a_button_of_the_strip_with_no_action_of_its_own_opens_nothing() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    let none_to_start = hud_gameplay(&mut c).1.button_actions_fired == 0;
    press_the_strip(&mut c, EXIT_BUTTON);
    let still_none = hud_gameplay(&mut c).1.button_actions_fired == 0;
    let and_no_panel = !hud_visible(&c, hud_find(&c, PANEL_STACK));

    c.assert_behaviour(
        "hud.lamp-row.a-button-of-the-strip-with-no-action-of-its-own-opens-nothing",
        move |_| none_to_start && still_none && and_no_panel,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity
// ---------------------------------------------------------------------------------------------

/// Five checkpoints on one client, driven by real quality updates against what this character can
/// actually carry, worked out the way the client works it out.
pub(super) fn the_burden_lamp_crosses_both_thresholds_on_the_characters_own_capacity() {
    let (mut c, _peer) = a_described_character();

    let (strength, augs) = {
        let world = c.view().world();
        let q = world
            .player_qualities()
            .expect("the character has been described");
        let s = q
            .attributes
            .and_then(|a| a.strength)
            .map(|a| i32::try_from(a.init_level + a.level_from_cp).expect("fits"))
            .expect("the character has a strength");
        (s, q.inq_int(NUM_AUGMENTATIONS))
    };
    let capacity = dereth_client_model::inventory::burden::encumbrance_capacity(strength, augs);
    let a_real_capacity = capacity > 0;

    // Checkpoint 1 -- as described, carrying nothing.
    let under_to_start = strip_state(&c, BURDEN_LAMP) == indicators::burden::UNDER;

    // Checkpoint 2 -- under, but deliberately not nothing, so the reading is proven to be running
    // rather than merely agreeing with the layout.
    set_int_quality(&mut c, 1, NUM_AUGMENTATIONS, augs);
    set_int_quality(&mut c, 2, ENCUMB_VAL, capacity / 2);
    let half_is_under = strip_state(&c, BURDEN_LAMP) == indicators::burden::UNDER;

    // Checkpoint 3 -- exactly all of it, which is over and not under.
    set_int_quality(&mut c, 3, ENCUMB_VAL, capacity);
    let all_of_it_is_over = strip_state(&c, BURDEN_LAMP) == indicators::burden::OVER;

    // Checkpoint 4 -- twice it.
    set_int_quality(&mut c, 4, ENCUMB_VAL, capacity * 2);
    let twice_is_further = strip_state(&c, BURDEN_LAMP) == indicators::burden::WAY_OVER;

    // ...and it goes out again, so the lamp is seen going out as well as coming on.
    set_int_quality(&mut c, 5, ENCUMB_VAL, 0);
    let it_goes_out = strip_state(&c, BURDEN_LAMP) == indicators::burden::UNDER;

    c.assert_behaviour(
        "hud.lamp-row.the-burden-lamp-crosses-both-thresholds-on-the-characters-own-capacity",
        move |_| {
            a_real_capacity
                && under_to_start
                && half_is_under
                && all_of_it_is_over
                && twice_is_further
                && it_goes_out
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out
// ---------------------------------------------------------------------------------------------

/// The asymmetry is the whole scenario: a pair of lamps driven by one shared count would pass a
/// "something is lit" assertion and be wrong.
pub(super) fn a_buff_lights_one_lamp_and_a_debuff_the_other() {
    use dereth_client_model::enchant::ench_type;
    let (mut c, _peer) = a_described_character();

    let both_dark_first = strip_state(&c, BUFF_LAMP) == indicators::STATE_NOTHING
        && strip_state(&c, DEBUFF_LAMP) == indicators::STATE_NOTHING;
    let (good, bad) = a_helpful_and_a_harmful_spell(&c);

    enchant(
        &mut c,
        an_enchantment(good, 1, ench_type::ADDITIVE | ench_type::BENEFICIAL, 1.0),
    );
    let one_lamp_alone = strip_state(&c, BUFF_LAMP) == 1
        && strip_state(&c, DEBUFF_LAMP) == indicators::STATE_NOTHING;

    enchant(&mut c, an_enchantment(bad, 2, ench_type::ADDITIVE, 1.0));
    let and_then_the_other = strip_state(&c, BUFF_LAMP) == 1 && strip_state(&c, DEBUFF_LAMP) == 1;

    c.when(dereth_testkit::Inbound::message(
        &dereth_protocol::qualities::MagicPurgeEnchantments,
    ));
    c.tick(1);
    let a_purge_puts_both_out = strip_state(&c, BUFF_LAMP) == indicators::STATE_NOTHING
        && strip_state(&c, DEBUFF_LAMP) == indicators::STATE_NOTHING;

    c.assert_behaviour(
        "hud.lamp-row.a-buff-lights-one-lamp-a-debuff-the-other-and-a-purge-puts-both-out",
        move |_| both_dark_first && one_lamp_alone && and_then_the_other && a_purge_puts_both_out,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one
// ---------------------------------------------------------------------------------------------

/// **No recording witnesses this**: nothing in the corpus installs a death penalty. The body is
/// this scenario's own, and the arm it goes in through is the client's only door for one.
pub(super) fn the_vitae_lamp_lights_for_a_penalty_and_not_for_a_multiplier_of_one() {
    use dereth_client_model::enchant::ench_type;
    let (mut c, _peer) = a_described_character();

    let dark_with_no_penalty = strip_state(&c, VITAE_LAMP) == indicators::STATE_NOTHING;

    enchant(
        &mut c,
        an_enchantment(666, 0, ench_type::VITAE | ench_type::MULTIPLICATIVE, 0.95),
    );
    let it_reached_the_character = (c
        .view()
        .world()
        .player_qualities()
        .expect("the character has been described")
        .enchantments
        .vitae_value()
        - 0.95)
        .abs()
        < 1e-6;
    let a_penalty_lights_it = strip_state(&c, VITAE_LAMP) == 1;

    // Present and yet no penalty at all: the case a lit-only scenario cannot see.
    enchant(
        &mut c,
        an_enchantment(666, 0, ench_type::VITAE | ench_type::MULTIPLICATIVE, 1.0),
    );
    let none_at_all_puts_it_out = strip_state(&c, VITAE_LAMP) == indicators::STATE_NOTHING;

    c.assert_behaviour(
        "hud.lamp-row.the-vitae-lamp-lights-for-a-penalty-and-not-for-a-multiplier-of-one",
        move |_| {
            dark_with_no_penalty
                && it_reached_the_character
                && a_penalty_lights_it
                && none_at_all_puts_it_out
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all
// ---------------------------------------------------------------------------------------------

/// The one lamp no message drives. The strip's table would otherwise carry a citation for it
/// rather than a measurement.
pub(super) fn the_link_lamp_comes_up_good_and_falls_to_lost() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(6));
    // The layout gives this lamp nothing to draw at rest, so it is put straight into "good".
    let good_to_start = strip_state(&c, ROW_LINK_LAMP) == indicators::link_status::media::GOOD;

    // There is no link at all here, so every reading falls through; run frames until the lamp's
    // own cadence has come round.
    for _ in 0..600 {
        c.tick(1);
        if strip_state(&c, ROW_LINK_LAMP) == indicators::link_status::media::LOST {
            break;
        }
    }
    let it_falls_to_lost = strip_state(&c, ROW_LINK_LAMP) == indicators::link_status::media::LOST;

    c.assert_behaviour(
        "hud.link-lamp.it-comes-up-good-and-falls-to-lost-when-nothing-is-heard-at-all",
        move |_| good_to_start && it_falls_to_lost,
    );
    c.shutdown();
}
