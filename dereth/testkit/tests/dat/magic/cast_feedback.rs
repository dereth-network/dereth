use super::*;
/// The shard's sentence is drawn verbatim, on the channel the shard marked it with.
pub fn a_recorded_resist_is_drawn_verbatim_on_the_magic_channel() {
    use support::{chat_log, resists, textbox, UI_QUEUE};

    // The premise, read out of the recordings rather than written down: one recording carries
    // resists, every sentence in it is paired with a resist sound on the caster's own object, and
    // the two arrive together.
    let (session, pairs) = resists();
    let paired = !pairs.is_empty()
        && pairs.iter().all(|(dt, _, id, volume)| {
            (0.0..0.050).contains(dt) && *id == pairs[0].2 && (volume - 1.0).abs() < f32::EPSILON
        });
    // …and the object they are on is the session's own player, not the monster.
    let on_the_caster = pairs[0].2 .0 >> 28 == 5;
    // Two different monsters produced them, so the name in the sentence really is the target's.
    let names: std::collections::BTreeSet<&str> =
        pairs.iter().map(|(_, text, _, _)| text.as_str()).collect();
    let two_monsters = names.len() == 2;

    let (mut c, mut peer) = support::a_client_with_a_peer();
    let before = chat_log(&mut c).len();

    let blob = support::first_resist_blob(session);
    let expected = textbox(&blob).text;
    peer.send(&mut c, UI_QUEUE, blob);
    c.tick(6);

    let after = chat_log(&mut c);
    let drawn = after[before..]
        .iter()
        .find(|(_, t)| t.contains("resists"))
        .cloned();
    let verbatim = drawn
        .as_ref()
        .is_some_and(|(ty, t)| t.trim_end() == expected && *ty == MAGIC_CHANNEL);
    // And the colour is the wire type's, not one the client chose: a client that wrote a zero
    // here would draw a magic line in the broadcast colour.
    let coloured =
        dereth_ui_screens::chat::colors::color_for_type(MAGIC_CHANNEL).hex == 0x003F_BFFF;

    c.assert_behaviour(
        "magic.resist.the-shards-own-sentence-is-drawn-verbatim-on-the-magic-channel",
        move |_| paired && on_the_caster && two_monsters && verbatim && coloured,
    );
    c.shutdown();
}

/// A squelched magic channel drops the same recorded bytes, and counts that it did.
pub fn a_squelched_magic_channel_drops_the_same_bytes() {
    use support::{chat_log, resists, UI_QUEUE};

    let (session, _) = resists();
    let (mut c, mut peer) = support::a_client_with_a_peer();
    c.world_mut()
        .chat
        .squelch
        .global
        .types
        .insert(u32::from(MAGIC_CHANNEL));
    let before = chat_log(&mut c).len();

    peer.send(&mut c, UI_QUEUE, support::first_resist_blob(session));
    c.tick(6);

    let after = chat_log(&mut c);
    let nothing_drawn = after[before..].iter().all(|(_, t)| !t.contains("resists"));
    // The gate counted it, so the silence is the squelch and not a decode that failed.
    let counted = c.view().expect_app().hud().stats.textbox_lines_squelched == 1;

    c.assert_behaviour(
        "magic.resist.a-squelched-magic-channel-drops-the-same-recorded-bytes",
        move |_| nothing_drawn && counted,
    );
    c.shutdown();
}

/// The sound that comes with a resist reaches the client's own sound queue.
pub fn the_resist_sound_reaches_the_clients_queue() {
    use support::{resists, sound, SMARTBOX_QUEUE};

    let (session, _) = resists();
    let (mut c, mut peer) = support::a_client_with_a_peer();
    let before = c.view().objects().stats.sound_events;

    // The recording's own sound blob, re-addressed to this session's player and otherwise
    // untouched.
    let blob = support::first_resist_sound_blob(session);
    let m = sound(&blob);
    let the_recorded_body = m.sound_type == RESIST_SOUND
        && (m.volume - 1.0).abs() < f32::EPSILON
        && m.id == support::PLAYER;

    peer.send(&mut c, SMARTBOX_QUEUE, blob);
    c.tick(6);

    let accepted = c.view().objects().stats.sound_events - before == 1;
    let nothing_dropped = c.view().objects().stats.unhandled == 0;

    c.assert_behaviour(
        "magic.resist.the-sound-that-comes-with-it-reaches-the-clients-own-queue",
        move |_| the_recorded_body && accepted && nothing_dropped,
    );
    c.shutdown();
}

// -------------------------------------------------------------------------------------------
// magic.cast.*
//
// A fizzle, the shard's own recorded fizzle, and the refusals the client makes itself. Which
// recordings carry a fizzle is the recorded-fizzle scenario's premise, read at run time.
// -------------------------------------------------------------------------------------------

/// A fizzle ends the cast, prints its line, and burns nothing the player is carrying.
pub fn a_fizzle_ends_the_cast_prints_its_line_and_burns_nothing() {
    use cast::{CastBench, CASTING, FIZZLED, MAGIC_CHANNEL};

    let mut b = CastBench::new();
    let idle_before = b.busy_count() == 0;
    b.cast_at(Some(cast::DRUDGE));

    // The cast really went out and the client really went busy, or every silence below is a
    // silence after nothing.
    let went_out = b.wire_has_targeted_cast() && b.spells_cast() == 1 && b.busy_count() == 1;
    let announced = b
        .spew()
        .iter()
        .any(|s| s.trim_end() == format!("{CASTING}{}", cast::SPELL));
    let lines_before = b.chat().len();
    let carried: Vec<i64> = b.formula_slots().iter().map(|s| b.held(*s)).collect();

    b.use_done(cast::YOUR_SPELL_FIZZLED);

    let cast_is_idle_again = b.busy_count() == 0 && b.uses_done() == 1;
    let after = b.chat();
    let printed = after[lines_before..]
        .iter()
        .find(|(_, t)| t.contains("fizzl"))
        .is_some_and(|(ty, t)| t.trim_end() == FIZZLED && *ty == MAGIC_CHANNEL);

    // The client burned nothing of its own accord: the shard burns, and the client learns of it
    // from the stack update it sends afterwards.
    let now: Vec<i64> = b.formula_slots().iter().map(|s| b.held(*s)).collect();
    let burned_nothing = now == carried && now.iter().all(|n| *n >= 10);
    // …and the other half of the same rule: when the shard's own burn arrives, the count follows.
    let slots = b.formula_slots();
    let was = b.held(slots[0]);
    b.restock(0, slots[0], 9);
    let follows_the_shard = b.held(slots[0]) == was - 1;
    b.shutdown();

    // The control that separates "the client prints for this answer" from "for every answer":
    // a clean acknowledgement takes the busy count down and writes nothing.
    let mut b = CastBench::new();
    b.cast_at(Some(cast::DRUDGE));
    let busy = b.busy_count() == 1;
    let lines_before = b.chat().len();
    b.use_done(0);
    let after = b.chat();
    let clean_is_silent = b.busy_count() == 0
        && after[lines_before..]
            .iter()
            .all(|(_, t)| !t.contains("fizzl"));
    b.shutdown();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "magic.cast.a-fizzle-ends-the-cast-prints-its-line-and-burns-nothing",
        move |_| {
            idle_before
                && went_out
                && announced
                && cast_is_idle_again
                && printed
                && burned_nothing
                && follows_the_shard
                && busy
                && clean_is_silent
        },
    );
}

/// The shard's own recorded fizzle prints the same line, and leaves the cast where it was.
pub fn a_recorded_fizzle_prints_the_line_without_ending_the_cast() {
    use cast::{recorded_fizzles, CastBench, FIZZLED, MAGIC_CHANNEL};

    // The premise, read out of the recordings: at least one of them carries a fizzle on the
    // shard's error message, and every body it carries is the bare code.
    let (session, bodies) = recorded_fizzles();
    let the_corpus_carries_them = !bodies.is_empty()
        && bodies.iter().all(|b| {
            b.len() == 20
                && u32::from_le_bytes(b[16..20].try_into().expect("four bytes"))
                    == cast::YOUR_SPELL_FIZZLED
        });

    let mut b = CastBench::new();
    b.cast_at(Some(cast::DRUDGE));
    let busy = b.busy_count() == 1;
    let lines_before = b.chat().len();

    b.replay(
        bodies
            .into_iter()
            .next()
            .expect("the recording carries one"),
    );

    let after = b.chat();
    let printed = after[lines_before..]
        .iter()
        .find(|(_, t)| t.contains("fizzl"))
        .is_some_and(|(ty, t)| t.trim_end() == FIZZLED && *ty == MAGIC_CHANNEL);
    // The shard's error message says what went wrong; it does not end the cast. That belongs to
    // the acknowledgement, and a client that did both would end the cast twice.
    let cast_untouched = b.busy_count() == 1;
    b.shutdown();

    let _ = session;
    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "magic.cast.the-shards-recorded-fizzle-prints-the-same-line-and-does-not-end-the-cast",
        move |_| the_corpus_carries_them && busy && printed && cast_untouched,
    );
}

/// Every refusal the client makes itself is said in its own words and never reaches the shard.
pub fn a_refusal_the_client_makes_itself_never_reaches_the_shard() {
    use cast::{CastBench, CANNOT_CAST_ON, CASTING};

    // 1. A target the spell cannot be cast on: the refusal names the target and nothing goes out.
    let wrong_target = {
        let mut b = CastBench::new();
        let before = b.spew_lines();
        b.cast_at(Some(cast::STATUE));
        let nothing_sent = !b.wire_has_any_cast() && b.spells_cast() == 0 && b.busy_count() == 0;
        let bubbles = b.spew();
        let named = bubbles
            .iter()
            .find(|s| s.starts_with(CANNOT_CAST_ON))
            .is_some_and(|s| s.trim_end() == format!("{CANNOT_CAST_ON}{}", cast::STATUE_NAME));
        // Exactly one refusal, and the success line is past it.
        let once = b.spew_lines() - before == 1 && bubbles.iter().all(|s| !s.starts_with(CASTING));
        b.shutdown();
        nothing_sent && named && once
    };

    // 2. Nothing selected at all: a different sentence, reached before the target is looked at.
    let no_target = {
        let mut b = CastBench::new();
        b.cast_at(None);
        let nothing_sent = !b.wire_has_any_cast();
        let said = b
            .spew()
            .iter()
            .find(|s| s.contains("suitable target"))
            .is_some_and(|s| s.trim_end() == dereth_client_model::magic::messages::NEED_TARGET);
        b.shutdown();
        nothing_sent && said
    };

    // 3. A component missing: the third local exit, and it is taken before the target is looked
    // at at all.
    let missing_component = {
        let mut b = CastBench::new();
        b.drop_the_first_component_class();
        let out_of_the_pack = b.held(b.formula_slots()[0]) == 0;
        b.cast_at(Some(cast::DRUDGE));
        let nothing_sent = !b.wire_has_targeted_cast();
        let bubbles = b.spew();
        let said = bubbles
            .iter()
            .find(|s| s.contains("components"))
            .is_some_and(|s| {
                s.trim_end() == dereth_client_model::magic::messages::MISSING_COMPONENTS
            });
        let never_looked = bubbles.iter().all(|s| !s.starts_with(CASTING));
        b.shutdown();
        out_of_the_pack && nothing_sent && said && never_looked
    };

    // The positive control for all three: the same gesture at a legal target sends and announces,
    // so the silences above are refusals and not a bench that never casts anything.
    let control = {
        let mut b = CastBench::new();
        b.cast_at(Some(cast::DRUDGE));
        let sent = b.wire_has_targeted_cast();
        let bubbles = b.spew();
        let announced = bubbles
            .iter()
            .any(|s| s.trim_end() == format!("{CASTING}{}", cast::SPELL))
            && bubbles.iter().all(|s| !s.starts_with(CANNOT_CAST_ON));
        b.shutdown();
        sent && announced
    };

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent",
        move |_| wrong_target && no_target && missing_component && control,
    );
}
