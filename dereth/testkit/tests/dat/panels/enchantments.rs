use super::*;

// =============================================================================================
// skills.rows.*
//
// A spell cast *while playing* moves a skill row at once, not only after a relog: what these
// claims measure is the live arrival path, not the arithmetic.
//
// The character is the recording's, not one the scenario builds: what these claims need is a
// page with a row per skill and a real starting number, and the recorded description has both.
// =============================================================================================

/// The recorded character, **with a shard attached**, for the claims that are about a panel
/// reacting to a message.
///
/// The spells have to arrive as real datagrams. The decoded-event step hands a message straight
/// to the two halves of the client that read it, and the callback the *frame* passes -- the one
/// that tells a panel a notice happened -- is a no-op on that path, so a page that is a cached
/// join rather than a live read never rebuilds. That is exactly the shape of defect this family
/// is about, so a scenario driven that way would be measuring the harness.
///
/// The **description** still comes through the corpus step, because it is nearly two kilobytes
/// and the harness's shard frames one fragment per datagram: a blob that long does not fit in
/// one and the client's parser refuses it. It is a description and not a panel notice, so nothing
/// here turns on which way it arrived.
fn a_recorded_character_and_a_shard() -> (HeadlessClient, dereth_testkit::Peer) {
    let end = description_blob(SESSION).idx;
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = dereth_testkit::Peer::attach(&mut c, LOOKER);

    // **The character is made by the shard**, not written into the tables: an ordered game event
    // is addressed to an object, and the ordered queue only has somewhere to put one for an
    // object the client's own object stream has been told about. A character written straight
    // into the tables passes every visible check and then every spell is dropped in silence.
    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: LOOKER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut c,
        dereth_testkit::replay::OBJECT_QUEUE,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    assert!(
        c.world_mut().set_player(LOOKER),
        "the identity is adopted once"
    );

    c.when(Inbound::from_corpus(SESSION, 0..end + 1));
    c.tick(4);
    assert!(
        !c.view().expect_app().hud().skills.is_empty(),
        "the recorded description is this scenario's oracle and it carried no skill"
    );
    (c, peer)
}

/// Two skills far enough apart that culling by key is visible: one gets the spells, the other is
/// the control.
const HEAVY_WEAPONS: u32 = 0x2C;
const ARCANE_LORE: u32 = 0x0E;
const STRENGTH: u32 = 1;

/// What one skill row **draws** -- its number and its colour.
fn skill_row_value(c: &HeadlessClient, skill: u32) -> (i32, u32) {
    let r = c
        .view()
        .expect_app()
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.skill == skill)
        .unwrap_or_else(|| panic!("skill {skill:#X} has no row on the page"));
    (r.value, r.font)
}

/// What the attribute page reads for one attribute -- the control beside the skill rows, so that
/// "both broke" and "one broke" are different answers.
fn attribute_shown(c: &HeadlessClient, id: u32) -> i32 {
    let app = c.view().expect_app();
    dereth_ui_screens::view::GameView::attribute(&app.hud().view(app.objects()), id)
        .expect("the attribute page has this row")
}

/// One spell landing on the character, as a real datagram from the shard.
fn cast(
    c: &mut HeadlessClient,
    peer: &mut dereth_testkit::Peer,
    e: dereth_protocol::types::qualities::Enchantment,
) {
    peer.event(c, &dereth_protocol::qualities::MagicUpdateEnchantment(e));
    c.tick(4);
}

/// One spell on a skill, in its own duelling category.
pub(super) fn spell_on(
    spell: u16,
    category: u16,
    key: u32,
    delta: f32,
) -> dereth_protocol::types::qualities::Enchantment {
    let mut e = enchantment(
        spell,
        dereth_client_model::enchant::ench_type::SKILL,
        key,
        delta,
    );
    e.category_word = u32::from(category);
    e.power_level = 8;
    e.duration = 1800.0;
    e
}

/// Two spells on one skill in different categories, so neither replaces the other and both have
/// to be counted.
fn two_spells_on_the_same_skill() -> Vec<dereth_protocol::types::qualities::Enchantment> {
    vec![
        spell_on(4624, 100, HEAVY_WEAPONS, 45.0),
        spell_on(2000, 101, HEAVY_WEAPONS, 10.0),
    ]
}

/// **The gate.** A spell that lands while the player is playing moves the skill row it is about,
/// two of them add up, a spell that lowers a skill lowers it and colours it, and a spell on an
/// attribute lifts the skills that are worked out from that attribute -- while leaving every
/// other row alone.
pub(super) fn a_spell_cast_while_playing_moves_the_skill_row_it_is_about() {
    let (mut c, mut peer) = a_recorded_character_and_a_shard();
    // The instrument has to be able to look: an empty page reports "no spell" exactly the way a
    // stale one does, so the denominator comes first.
    assert!(
        c.view().expect_app().hud().skills.len() > 30,
        "the premise: the page holds a row per skill, and it holds {}",
        c.view().expect_app().hud().skills.len()
    );
    // **The recorded character is not a blank one**: it carries something that raises every
    // skill a little, so the starting colour is the raised one and what is measured below is the
    // movement rather than the absolute.
    let (base, base_font) = skill_row_value(&c, HEAVY_WEAPONS);
    assert_ne!(base_font, 2, "the premise: nothing has lowered this skill");

    let spells = two_spells_on_the_same_skill();
    cast(&mut c, &mut peer, spells[0]);
    // The arrival, proved before anything is concluded about the sum.
    let arrived = {
        let w = c.view().world();
        w.player
            .and_then(|p| w.weenie(p))
            .and_then(|we| we.qualities.as_ref())
            .map(|q| {
                q.enchantments.add_list.len() == 1
                    && q.enchantments.add_list[0].smod.key == HEAVY_WEAPONS
                    && (q.enchantments.add_list[0].smod.value - 45.0).abs() < 1e-6
            })
            .unwrap_or(false)
    };
    let (one, one_font) = skill_row_value(&c, HEAVY_WEAPONS);

    cast(&mut c, &mut peer, spells[1]);
    let (two, two_font) = skill_row_value(&c, HEAVY_WEAPONS);

    // The control: an attribute still reads its own total, and the skills worked out from that
    // attribute move with it.
    let strength = attribute_shown(&c, STRENGTH);
    let mut on_strength = enchantment(
        4325,
        dereth_client_model::enchant::ench_type::ATTRIBUTE,
        STRENGTH,
        45.0,
    );
    on_strength.category_word = 200;
    cast(&mut c, &mut peer, on_strength);
    let attribute_moved = attribute_shown(&c, STRENGTH) == strength + 45;
    let through_the_formula = skill_row_value(&c, HEAVY_WEAPONS).0 > two;
    let after_the_attribute = skill_row_value(&c, HEAVY_WEAPONS).0;

    // A spell that lowers one skill lowers that one, colours it, and leaves the other alone.
    let (lore_base, lore_font) = skill_row_value(&c, ARCANE_LORE);
    assert_ne!(
        lore_font, 2,
        "the premise: nothing has lowered this one either"
    );
    cast(&mut c, &mut peer, spell_on(3000, 102, ARCANE_LORE, -20.0));
    let (lore, lore_after_font) = skill_row_value(&c, ARCANE_LORE);
    // And the other skill is untouched by it: the spells are sorted by what they are about before
    // any of them is applied.
    let the_other_is_untouched = skill_row_value(&c, HEAVY_WEAPONS).0 == after_the_attribute;

    c.assert_behaviour(
        "skills.rows.a-spell-cast-while-playing-moves-the-skill-row-it-is-about",
        move |_| {
            arrived
                && one == base + 45
                && one_font == 1
                && two == base + 55
                && two_font == 1
                && attribute_moved
                && through_the_formula
                && lore == lore_base - 20
                && lore_after_font == 2
                && the_other_is_untouched
        },
    );
    c.shutdown();
}

/// A character who logs in **already** carrying both spells gets the same total the live path
/// produces: the login path and the live path share one arithmetic.
pub(super) fn a_character_who_logs_in_already_enchanted_reads_the_same_total() {
    use dereth_protocol::types::qualities::{quality_flags, EnchantmentRegistry};

    let mut c = a_recorded_character(SESSION);
    let (base, _) = skill_row_value(&c, HEAVY_WEAPONS);

    // The recording's own description, delivered a second time with the two spells already in the
    // registry it carries -- which is what a character who logs in enchanted receives, because
    // the live message is only sent when something *changes*.
    let mut d = recorded_description(SESSION);
    d.qualities.flags |= quality_flags::ENCHANTMENT_REGISTRY;
    d.qualities.enchantments = Some(EnchantmentRegistry {
        flags: EnchantmentRegistry::ADDITIVE,
        additive: Some(two_spells_on_the_same_skill()),
        ..EnchantmentRegistry::default()
    });
    // The description goes in as a decoded event: it is the same two kilobytes the harness's
    // shard cannot frame in one datagram, and the arm that unpacks it rebuilds the page itself
    // rather than through the notice a panel would hear.
    c.when(Inbound::event(
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(d)),
    ));
    c.tick(8);

    let took_both = {
        let w = c.view().world();
        w.player
            .and_then(|p| w.weenie(p))
            .and_then(|we| we.qualities.as_ref())
            .is_some_and(|q| q.enchantments.add_list.len() == 2)
    };
    let (after, font) = skill_row_value(&c, HEAVY_WEAPONS);

    c.assert_behaviour(
        "skills.rows.a-character-who-logs-in-already-enchanted-reads-the-same-total",
        move |_| took_both && after == base + 55 && font == 1,
    );
    c.shutdown();
}
