use super::*;
// -------------------------------------------------------------------------------------------
// vitae.*
//
// The corpus has no vitae in it at all -- nobody died on camera -- so every message here is
// constructed and said to be; what is not constructed is the route.
// -------------------------------------------------------------------------------------------

/// A live vitae lights the lamp and fills the panel with the penalty and what it will cost.
pub fn a_live_vitae_lights_the_lamp_and_fills_the_panel() {
    use vitae::{Vitae, CP_POOL, LEVEL_OF_THE_CHARACTER, TICKS};

    let mut v = Vitae::new();
    v.describe(None);

    // A character with no vitae is a known full-strength character, not an unknown one, and the
    // lamp is dark.
    let full_strength = v.value() == Some(1.0) && v.lamp_is_dark();

    v.arrives(TICKS[0]);

    let in_the_registry = v.registry_vitae() == TICKS[0];
    // It joins neither enchantment counter: a vitae is not one of the three lists.
    let counts_untouched = v.enchantment_counts() == (0, 0);
    let lamp_is_lit = v.value() == Some(TICKS[0]) && !v.lamp_is_dark();

    v.open_the_panel();
    let text = v.panel_text();
    let says_the_penalty = text.contains('5') && !text.contains("full strength");
    // …and what earning it back will cost, computed here from the character's own numbers rather
    // than through the function the panel used.
    #[allow(clippy::cast_possible_truncation)]
    let threshold = ((math::pow(f64::from(LEVEL_OF_THE_CHARACTER), 2.5) * 2.5 + 20.0)
        * math::pow(f64::from(TICKS[0]), 5.0)
        + 0.5) as i32;
    let need = threshold - CP_POOL;
    let says_what_it_costs =
        text.contains(&dereth_ui_screens::panels::statmgmt::num(i64::from(need)));
    v.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitae.lamp.lights-for-a-live-penalty-and-the-panel-says-how-much",
        move |_| {
            full_strength
                && in_the_registry
                && counts_untouched
                && lamp_is_lit
                && says_the_penalty
                && says_what_it_costs
        },
    );
}

/// The panel follows every tick as the penalty wears off, and goes dark at full strength.
pub fn the_panel_follows_every_tick_and_goes_dark_at_full_strength() {
    use vitae::{Vitae, TICKS};

    let mut v = Vitae::new();
    v.describe(Some(TICKS[0]));
    v.open_the_panel();

    let mut seen: Vec<(String, bool, u32)> = Vec::new();
    for tick in TICKS {
        v.arrives(tick);
        seen.push((v.panel_text(), v.lamp_is_dark(), v.panel_updates()));
    }

    // Every arrival reached the panel, and each one wrote a different line.
    let rewritten_each_time = seen.windows(2).all(|w| w[1].2 > w[0].2 && w[0].0 != w[1].0);
    // The first three are penalties with the lamp lit; the last is full strength and dark.
    let penalties = seen
        .iter()
        .take(3)
        .all(|(t, dark, _)| !*dark && !t.contains("full strength"));
    let (last, dark, _) = &seen[3];
    let full_strength_is_dark = *dark && last.contains("full strength");
    // And the entry is still in the registry: the shard replaces it as it wears off rather than
    // taking it away.
    let still_there = (v.registry_vitae() - 1.0).abs() < f32::EPSILON;
    v.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitae.panel.follows-every-tick-and-goes-dark-at-full-strength",
        move |_| rewritten_each_time && penalties && full_strength_is_dark && still_there,
    );
}

/// The penalty takes the skill down, leaves the row drawn plain, and never touches an attribute.
pub fn the_penalty_takes_the_skill_down_but_leaves_the_row_plain() {
    use vitae::{Vitae, TICKS};

    let mut v = Vitae::new();
    v.describe(None);
    let (base, base_font) = v.skill_row();
    let plain_to_start = base_font == 0 && base >= 55;
    let strength = v.strength();

    // The same character, logged in again with the vitae his death left him.
    v.describe(Some(TICKS[0]));
    let (penalised, font) = v.skill_row();

    // The number, computed here rather than through the client's own multiply.
    #[allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]
    let want = ((base as f32 * TICKS[0]) + 0.5) as i32;
    let took_it_down = penalised == want && penalised < base;
    // **And the row still draws plain.** The page compares the raw skill against the enchanted one
    // with only the vitae added back, so the term cancels; a build that dropped it would draw a
    // vitae-carrying character's whole list as though every skill were debuffed.
    let still_plain = font == 0;
    let footer = v.skill_entry();
    let footer_shows_it = footer.vitae < 0
        && footer.effective - footer.vitae == base
        && footer.effective == penalised;

    // The discriminating case neither a buff alone nor a penalty alone can reach: a buff **on top
    // of** the penalty is drawn green, not red.
    v.buff_the_skill(3.0);
    let (buffed, buffed_font) = v.skill_row();
    let a_buff_on_top_is_green = buffed > penalised && buffed_font == 1;

    // And an attribute never moves: the multiply is on the skill path and not on that one.
    let attribute_untouched = v.strength() == strength;
    v.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "vitae.penalty.takes-the-skill-down-leaves-the-row-plain-and-spares-the-attributes",
        move |_| {
            plain_to_start
                && took_it_down
                && still_plain
                && footer_shows_it
                && a_buff_on_top_is_green
                && attribute_untouched
        },
    );
}
