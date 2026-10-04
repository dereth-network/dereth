use super::*;
// -------------------------------------------------------------------------------------------
// enchantments.duration / expiry / removal / dispel / pane
//
// These scenarios are about the **session clock**: the client rebases an enchantment's start on
// receipt, so a claim about a remaining time is only a claim when the clock is somewhere a real
// session's would be. That is why these five build a whole client with a socket-free endpoint
// under it and move the clock with the shard's own time-sync header rather than by writing a
// field -- the client has no setter for it, and inventing one would be asserting over a clock
// the client does not keep.
// -------------------------------------------------------------------------------------------

/// A thirty-minute buff that has been running for ten shows twenty minutes left, and still shows
/// the right number after a relog.
pub fn a_timed_buff_shows_its_real_remaining_time_across_a_relog() {
    use support::{a_beneficial_spell, cell_seconds, duration_cell, timed_buff_login};

    /// The buff's total length, as the spell's own data gives it: thirty minutes.
    const DURATION: f64 = 1800.0;
    /// Where the session clock is put before the first description, and where after it.
    ///
    /// **An hour, and the hour is load-bearing.** A clock smaller than the remaining time still
    /// prints a wrong-but-non-zero number; only a clock larger than it reaches the nothing-left
    /// display this claim rules out.
    const FIRST_LOGIN_AT: f64 = 3600.0;
    const RELOG_AT: f64 = 4200.0;

    let (mut c, mut peer) = support::a_client_with_a_peer();
    let spell = a_beneficial_spell(&c).0;

    peer.set_clock(&mut c, FIRST_LOGIN_AT);
    let clock = c.view().expect_app().clock().cur_time;
    assert!(
        (clock - FIRST_LOGIN_AT).abs() < 1.0,
        "the premise: the session clock is {clock:.3} and not near {FIRST_LOGIN_AT:.0}; without a \
         clock far from zero a rebased start and a raw one give the same answer"
    );

    peer.event(&mut c, &timed_buff_login(spell, DURATION, 600.0));
    c.tick(8);
    let arrived = c.view().expect_app().hud().player_desc_received;
    let cell = duration_cell(&c, spell).expect("the buff the description carried is in the pane");
    // Read back into seconds rather than compared with a literal: the frames between the packet
    // landing and this read advance the clock by tens of milliseconds and the cell truncates. The
    // tolerance is seconds and the defect is minutes.
    let first = cell_seconds(&cell);
    let twenty_minutes_left = cell != "0:00" && (first - 1200).abs() <= 5;

    // The relog: the same message again, at a later clock, with the shard's own decremented
    // start. It is the only place a fresh session learns the registry from.
    peer.set_clock(&mut c, RELOG_AT);
    peer.event(&mut c, &timed_buff_login(spell, DURATION, 1200.0));
    c.tick(8);
    let after = duration_cell(&c, spell).expect("the buff survives the second login");
    let second = cell_seconds(&after);
    let ten_minutes_left = after != "0:00" && (second - 600).abs() <= 5;
    // And it went **down** rather than being re-anchored: the shard's own decremented start is
    // what moved it.
    let it_went_down = second < first;

    // The control the shape of the number rests on: an item's permanent spell has no remaining
    // time and the pane blanks it rather than printing one, so the number above cannot be coming
    // from a path that prints something for everything.
    let blanks_a_permanent_one = {
        let app = c.view().expect_app();
        let view = app.hud().view(app.objects());
        let e = view
            .active_effects()
            .into_iter()
            .find(|e| e.spell == spell)
            .expect("the buff");
        !e.permanent && dereth_ui_screens::panels::effects::duration_cell(&e, false).is_empty()
    };

    c.assert_behaviour(
        "enchantments.duration.a-timed-buff-shows-its-real-remaining-time-across-a-relog",
        move |_| {
            arrived
                && twenty_minutes_left
                && ten_minutes_left
                && it_went_down
                && blanks_a_permanent_one
        },
    );
    c.shutdown();
}

/// The row stays until the shard takes it away -- the client keeps no expiry clock of its own.
pub fn the_row_survives_its_own_duration_running_out() {
    use support::{chat_log, pane_rows, remaining, skill_row, Station, EXPIRED};

    let mut s = Station::arm();
    let base = s.base_skill;
    let drawn = pane_rows(&s.c) == vec![s.spell] && skill_row(&s.c).0 == base + support::BUFF_VALUE;
    let before = remaining(&s.c, s.spell).expect("the entry");
    let just_cast = before > 0.0 && before <= support::BUFF_DURATION;
    let lines_before = chat_log(&mut s.c).len();

    s.clock_far_past_the_duration();

    // The remaining time is allowed to go past; what is not allowed is the row going away.
    let after = remaining(&s.c, s.spell).unwrap_or_else(|| {
        panic!("the client expired the enchantment on a clock it does not keep")
    });
    let gone_past = after < 0.0;
    let still_drawn = pane_rows(&s.c) == vec![s.spell];
    let still_buffed = skill_row(&s.c).0 == base + support::BUFF_VALUE;
    let said_nothing = chat_log(&mut s.c)[lines_before..]
        .iter()
        .all(|(_, t)| !t.contains(EXPIRED));

    s.c.assert_behaviour(
        "enchantments.expiry.the-row-stays-until-the-shard-takes-it-and-not-when-its-time-runs-out",
        move |_| drawn && just_cast && gone_past && still_drawn && still_buffed && said_nothing,
    );
    s.c.shutdown();
}

/// The shard's removal takes the row out of the pane, the skill back to base, and prints the line.
pub fn the_shards_removal_empties_the_pane_and_the_skill() {
    use support::{chat_log, helpful_panel_is_open, pane_rows, skill_row, Station, EXPIRED};

    let mut s = Station::arm();
    let base = s.base_skill;
    let buffed =
        pane_rows(&s.c) == vec![s.spell] && skill_row(&s.c) == (base + support::BUFF_VALUE, 1);
    let lines_before = chat_log(&mut s.c).len();

    // Let its own clock run out first, so the removal is an expiry and not a dispel of a running
    // buff -- which is the sequence the claim names.
    s.clock_far_past_the_duration();
    s.remove();

    let took_it_out =
        s.c.view()
            .expect_app()
            .interaction()
            .stats
            .enchantments_removed
            == 1;
    let counts_followed = {
        let app = s.c.view().expect_app();
        app.hud().view(app.objects()).enchantment_counts() == (0, 0)
    };
    let pane_emptied = pane_rows(&s.c).is_empty() && helpful_panel_is_open(&mut s.c);
    let skill_back_to_base = skill_row(&s.c) == (base, 0);
    let one_line =
        s.c.view()
            .expect_app()
            .interaction()
            .stats
            .enchantment_expiry_lines
            == 1;
    let name = s.name.clone();
    let line_is_the_spells_own = chat_log(&mut s.c)[lines_before..]
        .iter()
        .find(|(_, t)| t.contains(EXPIRED))
        .is_some_and(|(ty, t)| t.trim_end() == format!("{name}{EXPIRED}") && *ty == support::MAGIC);

    s.c.assert_behaviour(
        "enchantments.removal.takes-the-row-out-of-the-pane-the-skill-back-to-base-and-says-so",
        move |_| {
            buffed
                && took_it_out
                && counts_followed
                && pane_emptied
                && skill_back_to_base
                && one_line
                && line_is_the_spells_own
        },
    );
    s.c.shutdown();
}

/// A dispel empties the same two panels and says nothing at all.
pub fn a_dispel_empties_the_same_panels_without_a_line() {
    use support::{chat_log, pane_rows, skill_row, Station, EXPIRED};

    let mut s = Station::arm();
    let base = s.base_skill;
    let drawn = pane_rows(&s.c) == vec![s.spell];
    let lines_before = chat_log(&mut s.c).len();

    s.dispel();

    let pane_emptied = pane_rows(&s.c).is_empty();
    let skill_back_to_base = skill_row(&s.c) == (base, 0);
    let no_line =
        s.c.view()
            .expect_app()
            .interaction()
            .stats
            .enchantment_expiry_lines
            == 0
            && chat_log(&mut s.c)[lines_before..]
                .iter()
                .all(|(_, t)| !t.contains(EXPIRED));

    s.c.assert_behaviour(
        "enchantments.dispel.empties-the-same-two-panels-and-says-nothing",
        move |_| drawn && pane_emptied && skill_back_to_base && no_line,
    );
    s.c.shutdown();
}

/// A second buff reaches a pane that is already open, with no second click.
pub fn a_second_buff_reaches_an_already_open_pane() {
    use support::{pane_rows, Station};

    let mut s = Station::arm();
    let first = s.spell;
    let second = s.arm_a_second_buff();

    let mut rows = pane_rows(&s.c);
    rows.sort_unstable();
    let mut want = vec![first, second];
    want.sort_unstable();
    let both_drawn = rows == want;
    let counts_followed = {
        let app = s.c.view().expect_app();
        app.hud().view(app.objects()).enchantment_counts() == (2, 0)
    };

    s.c.assert_behaviour(
        "enchantments.pane.a-second-buff-reaches-a-pane-that-is-already-open",
        move |_| both_drawn && counts_followed,
    );
    s.c.shutdown();
}
