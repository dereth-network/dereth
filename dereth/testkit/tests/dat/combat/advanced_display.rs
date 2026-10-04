use super::*;

// ---------------------------------------------------------------------------------------------
// combat.advanced.*
//
// **The bench is the shipped tree with the three pieces of a frame over it**, in the order the
// real frame runs them and with the delivery pass between them -- which is the whole mechanism by
// which a panel's visibility reaches the window it sits in, so a bench that skipped it would
// assert the cluster and never the window.
//
// **Two checks have no row of their own.** One is the calibration of the bench: that both windows
// can be seen moving in both directions, and that a level read back off a strip can be made
// non-zero -- which is what makes every absence below mean something, and which stands as a premise
// instead. The other is the recording's own frame -- that it is one clean session, reassembles
// completely and completes no blob twice. That is a claim about the corpus, the corpus property
// test owns it, and these two scenarios read the **locked decoded** corpus rather than reassembling
// a raw capture for themselves, so the reassembly is not theirs to check either.
//
// **No count and no word is pinned.** These walk the recording and assert the shape: the edge
// exists, the saves differ in one bit, the swings before it sit on notches and the ones after do
// not.
// ---------------------------------------------------------------------------------------------

pub(super) fn the_advanced_combat_option_suppresses_the_classic_combat_window() {
    use dereth_client_model::combat::CombatMode;

    // The calibration the absences below rest on: the window can be seen coming up and going down
    // again, through the client's own producer.
    let mut off = advanced::Bench::new();
    off.frame(vec![], 200.0);
    let starts_down = !off.combat_window() && off.strip() == vec![false, false];
    let off_by_default = !off.world().player_system.options.advanced_combat_ui();
    off.set_mode(CombatMode::Melee);
    off.frame(vec![], 200.1);
    let can_come_up = off.combat_page() && off.combat_window() && off.delivered() > 0;
    off.set_mode(CombatMode::NonCombat);
    off.frame(vec![], 200.2);
    let can_go_down = !off.combat_window();

    // ...and back up, which is the reading the suppressed one is compared against.
    off.set_mode(CombatMode::Melee);
    off.frame(vec![], 200.3);
    let up_with_the_option_off = off.combat_page() && off.combat_window();

    // The same character and the same mode change with the option on.
    let mut on = advanced::Bench::new();
    on.frame(vec![], 200.0);
    on.set_advanced(true);
    on.set_mode(CombatMode::Melee);
    on.frame(vec![], 200.1);
    let really_in_melee = on.world().combat.combat_mode == CombatMode::Melee;
    let suppressed = !on.combat_page() && !on.combat_window();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-suppresses-the-classic-combat-window",
        move |_| {
            starts_down
                && off_by_default
                && can_come_up
                && can_go_down
                && up_with_the_option_off
                && really_in_melee
                && suppressed
        },
    );
}

pub(super) fn turning_the_advanced_option_on_while_the_window_is_up_takes_it_down() {
    use dereth_client_model::combat::CombatMode;

    let mut b = advanced::Bench::new();
    b.set_mode(CombatMode::Melee);
    b.frame(vec![], 300.0);
    let up = b.combat_window();

    b.set_advanced(true);
    b.frame(vec![], 300.1);
    let down = !b.combat_page() && !b.combat_window();

    // And back: an edge that only ever hid would pass both stations above.
    b.set_advanced(false);
    b.frame(vec![], 300.2);
    let up_again = b.combat_window();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.turning-the-option-on-while-the-window-is-up-takes-it-down",
        move |_| up && down && up_again,
    );
}

pub(super) fn the_strip_comes_up_while_an_attack_key_is_held_and_goes_on_release() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};

    let mut b = advanced::Bench::new();
    b.set_advanced(true);
    b.set_mode(CombatMode::Melee);
    // Deliberately nothing to swing at: the advanced arm starts a build anyway, so the release
    // sends nothing and the hide comes out of the release itself rather than out of a fixture
    // standing in for the shard.
    b.nothing_selected();
    b.frame(vec![], 400.0);
    let starts_down = b.strip() == vec![false, false];

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 400.1);
    let building = b.world().combat.power_bar_mode == PowerBarMode::AdvancedCombat
        && b.world().combat.build_in_progress;
    b.frame(vec![], 400.2);
    let shown = b.strip() == vec![false, true]
        && !b.combat_window()
        // The charge beginning zeroes the one it reached, and the other is not written at all.
        && b.strip_levels() == vec![None, Some(0.0)];
    b.frame(vec![], 400.4);
    let held = b.strip_levels();
    let charging = held[1].is_some_and(|v| v > 0.0) && held[0].is_none();

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, false)], 400.5);
    let did_not_send = !b.world().combat.attack_server_response_pending
        && b.world().combat.power_bar_mode == PowerBarMode::Undef;
    b.frame(vec![], 400.6);
    let gone = b.strip() == vec![false, false]
        && b.strip_levels() == vec![None, Some(0.0)]
        && !b.combat_window();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-strip-comes-up-while-an-attack-key-is-held-and-goes-on-release",
        move |_| starts_down && building && shown && charging && did_not_send && gone,
    );
}

pub(super) fn a_release_that_reaches_the_shard_holds_the_strip_until_the_answer() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};
    use dereth_primitives::LocalTime;

    let mut b = advanced::Bench::new();
    b.set_advanced(true);
    b.set_mode(CombatMode::Melee);
    // The repeat is off, so the answer's other arm -- the one that hides -- is what this is about.
    b.set_auto_repeat(false);
    b.frame(vec![], 500.0);

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 500.1);
    b.frame(vec![], 500.5);
    let held = b.strip() == vec![false, true];

    b.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, false)], 500.6);
    let sent =
        b.world().combat.attack_server_response_pending && !b.world().combat.build_in_progress;
    b.frame(vec![], 500.7);
    let still_up = b.strip() == vec![false, true];

    b.the_shard_answers_the_swing(LocalTime(500.8));
    let cleared = b.world().combat.power_bar_mode == PowerBarMode::Undef;
    b.frame(vec![], 500.9);
    let gone = b.strip() == vec![false, false];

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.a-release-that-reaches-the-shard-holds-the-strip-until-the-answer",
        move |_| held && sent && still_up && cleared && gone,
    );
}

pub(super) fn the_advanced_option_chooses_which_display_is_live() {
    use dereth_client::interaction::action as ia;
    use dereth_client_model::combat::{CombatMode, PowerBarMode};

    let mut classic = advanced::Bench::new();
    classic.set_mode(CombatMode::Melee);
    classic.frame(vec![], 600.0);
    classic.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 600.1);
    for i in 1..=3 {
        classic.frame(vec![], 600.1 + f64::from(i) * 0.1);
    }
    let classic_holds = classic.world().combat.power_bar_mode == PowerBarMode::Combat
        && classic.strip() == vec![false, false]
        && classic.strip_levels() == vec![None, None]
        && classic.window_meter().is_some_and(|v| v > 0.0);

    let mut adv = advanced::Bench::new();
    adv.set_advanced(true);
    adv.set_mode(CombatMode::Melee);
    adv.frame(vec![], 600.0);
    adv.frame(vec![advanced::key(ia::COMBAT_MEDIUM_ATTACK, true)], 600.1);
    for i in 1..=3 {
        adv.frame(vec![], 600.1 + f64::from(i) * 0.1);
    }
    let levels = adv.strip_levels();
    let advanced_holds = adv.world().combat.power_bar_mode == PowerBarMode::AdvancedCombat
        && adv.strip() == vec![false, true]
        && levels[0].is_none()
        && levels[1].is_some_and(|v| v > 0.0)
        && adv.window_meter().is_none();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-chooses-which-display-is-live-and-only-one-ever-is",
        move |_| classic_holds && advanced_holds,
    );
}

pub(super) fn the_advanced_option_is_off_at_login_and_the_toggle_is_one_bit() {
    let recorded = advanced::the_recorded_session();
    let default_word = dereth_client_model::player::options::DEFAULT_OPTIONS;
    let on_word = default_word | advanced::THE_OPTION_BIT;

    // The character's own description, as the shard sent it.
    let carries = |b: &[u8], w: u32| {
        b.windows(4)
            .any(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) == w)
    };
    let described: Vec<&dereth_client_net::client_session::testing::CorpusBlob> = recorded
        .blobs
        .iter()
        .filter(|b| {
            b.dir == dereth_client_net::client_session::testing::Direction::ServerToClient
                && advanced::is_a_description(b)
        })
        .collect();
    let arrives_off = described.len() == 1
        && carries(&described[0].payload, default_word)
        && !carries(&described[0].payload, on_word);

    // Every option save the recorded client sent, in order.
    let saves: Vec<u32> = advanced::the_recorded_option_saves(&recorded);
    let flags: Vec<bool> = saves
        .iter()
        .map(|w| w & advanced::THE_OPTION_BIT != 0)
        .collect();
    let goes_on_and_off = flags.first() == Some(&true) && flags.last() == Some(&false);
    let one_bit = saves
        .iter()
        .all(|w| (w ^ default_word) & !advanced::THE_OPTION_BIT == 0);
    let off_is_the_default = saves.last() == Some(&default_word);

    // And the bit the table names is the one the wire moved.
    let (name, word, mask) = dereth_client_model::player::options::PLAYER_OPTIONS
        [dereth_client_model::player::options::option::ADVANCED_COMBAT_UI];
    let named = name == "AdvancedCombatUI"
        && word == dereth_client_model::player::options::OptionWord::One
        && mask == advanced::THE_OPTION_BIT
        && !dereth_client_model::player::options::default_option_value(
            dereth_client_model::player::options::option::ADVANCED_COMBAT_UI,
        );

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-option-is-off-at-login-and-the-toggle-is-one-bit-on-the-wire",
        move |_| {
            arrives_off
                && saves.len() >= 2
                && goes_on_and_off
                && one_bit
                && off_is_the_default
                && named
        },
    );
}

pub(super) fn the_recorded_attacks_are_charges_after_the_toggle_and_notches_before() {
    let recorded = advanced::the_recorded_session();
    let on_at = advanced::when_the_option_went_on(&recorded)
        .expect("the recorded session turns the option on");
    let attacks = advanced::the_recorded_swings(&recorded);
    assert!(!attacks.is_empty(), "the recorded session carries swings");

    let (before, after): (Vec<_>, Vec<_>) = attacks.iter().partition(|(t, _)| *t < on_at);
    let both_halves = !before.is_empty() && !after.is_empty();

    // The gauge offers seven positions; a hair of tolerance, because the value crosses the wire
    // as a float and the client's own is the product of an integer step.
    let on_a_notch = |p: f32| (0u8..=6).any(|k| (p - f32::from(k) / 6.0).abs() < 1e-3);
    let off_the_notches: Vec<(f64, f32)> = before
        .iter()
        .filter(|(_, p)| !on_a_notch(*p))
        .copied()
        .collect();
    // The ones that are off the notches are the documented pairs: the release sends twice at one
    // instant, once at the power it was let go at and once capped.
    let every_odd_one_is_a_pair = off_the_notches
        .iter()
        .all(|(t, _)| before.iter().filter(|(u, _)| (u - t).abs() < 1e-6).count() == 2);
    let most_are_notches = off_the_notches.len() < before.len();

    let none_after_is = after.iter().all(|(_, p)| !on_a_notch(*p));
    let each_is_a_partial_hold = after.iter().all(|(_, p)| *p > 0.0 && *p < 1.0);
    // ...which is what a release before the bar fills looks like, because the bar is a second.
    let a_second = (dereth_client_model::combat::POWER_BAR_SECONDS - 1.0).abs() < f64::EPSILON;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "combat.advanced.the-recorded-attacks-are-charges-after-the-toggle-and-notches-before",
        move |_| {
            both_halves
                && most_are_notches
                && every_odd_one_is_a_pair
                && none_after_is
                && each_is_a_partial_hold
                && a_second
        },
    );
}
