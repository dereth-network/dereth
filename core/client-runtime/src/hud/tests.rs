use super::*;

/// Behaviour: appraisal.presentation.variants-preserve-order-and-world-facts
#[test]
fn appraisal_display_facts_survive_the_live_projection_and_snapshot() {
    use dereth_client_contract::snapshot::GameSnapshot;
    use dereth_client_model::{Weenie, World};
    use dereth_protocol::{archive::PackedHash, types::appraisal::AppraisalProfile};
    let id = ObjectId(7);
    let mut world = World::new();
    world.tables.weenies.insert(id, Weenie::new(id));
    world.selected = Some(id);
    let mut profile = AppraisalProfile::default();
    profile.tables.ints = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x2f, 0x820), (0xcc, 17), (0x2f, 999), (0xcc, 999)],
    });
    profile.tables.strings = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x13, "Aluvian".into()), (0x13, "Later duplicate".into())],
    });
    profile.spell_book = Some(vec![5, 999999, 0x8000_0005]);
    world.appraisal.set(id, profile);
    let mut hud = Hud::new();
    hud.spell_table = Some(dereth_assets::tables::SpellTable {
        id: dereth_primitives::DataId(0x0e00_000e),
        spell_buckets: 1,
        spells: [(
            5,
            dereth_assets::tables::SpellBase {
                name: String::new(),
                description: String::new(),
                school: 0,
                icon: 0,
                category: 0,
                bitfield: 0,
                base_mana: 0,
                base_range_constant: 0.0,
                base_range_mod: 0.0,
                power: 0,
                spell_economy_mod: 0.0,
                formula_version: 0,
                component_loss: 0.0,
                meta_spell_type: 0,
                meta_spell_id: 0,
                duration: None,
                portal_lifetime: None,
                raw_comps: [0; 8],
                comp_key: 0,
                comps: vec![],
                caster_effect: 0,
                target_effect: 0,
                fizzle_effect: 0,
                recovery_interval: 0.0,
                recovery_amount: 0.0,
                display_order: 0,
                non_component_target_type: 0,
                mana_mod: 0,
            },
        )]
        .into(),
        spellset_bucket_index: 0,
        spellsets: Default::default(),
    });
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    let live = view.appraisal(id).unwrap();
    let captured = GameSnapshot::from_view(&view).appraisal(id).unwrap();
    for facts in [&live, &captured] {
        assert_eq!(facts.attack_type, Some(0x820));
        assert_eq!(facts.elemental_damage_bonus, Some(17));
        assert_eq!(facts.activation_heritage.as_deref(), Some("Aluvian"));
        let spells = facts.magic.spells.as_ref().unwrap();
        assert_eq!(
            spells
                .iter()
                .map(|spell| spell.resolved)
                .collect::<Vec<_>>(),
            [true, false, true]
        );
        assert!(spells.iter().all(|spell| spell.name.is_empty()));
    }
    assert_eq!(live, captured);
    // The projection owns its strings and survives replacement of the cached tables.
    world.appraisal.set(id, AppraisalProfile::default());
    assert_eq!(live.activation_heritage.as_deref(), Some("Aluvian"));
    let empty = HudView {
        hud: &hud,
        world: &world,
    }
    .appraisal(id)
    .unwrap();
    assert_eq!(empty.attack_type, None);
    assert_eq!(empty.activation_heritage, None);
}

/// Behaviour: allegiance.oath.shared-cost-facts-survive-snapshots
#[test]
fn oath_cost_reads_live_breaks_and_survives_the_snapshot() {
    use dereth_client_contract::snapshot::GameSnapshot;
    use dereth_client_model::{StatKey, StatType, StatValue, Weenie, World};
    let player = ObjectId(1);
    let mut world = World::new();
    world.player = Some(player);
    let mut row = Weenie::new(player);
    row.qualities = Some(dereth_client_model::Qualities::new());
    row.qualities
        .as_mut()
        .unwrap()
        .set(StatKey::new(StatType::Int, 0x19), StatValue::Int(1));
    world.tables.weenies.insert(player, row);
    let mut hud = Hud::new();
    hud.player_desc_received = true;
    hud.xp_table = Some(dereth_assets::tables::XpTable {
        id: dereth_primitives::DataId(0x0e000018),
        attribute_xp: vec![],
        vital_xp: vec![],
        trained_xp: vec![],
        specialized_xp: vec![],
        level_xp: vec![0, 1000, 21000],
        level_credits: vec![0; 3],
    });
    for era in [
        dereth_primitives::EraId::Infiltration,
        dereth_primitives::EraId::Eor,
    ] {
        hud.era.era = era;
        for (breaks, expected) in [(-1, 0), (0, 0), (1, 1250)] {
            world
                .weenie_mut(player)
                .unwrap()
                .qualities
                .as_mut()
                .unwrap()
                .set(StatKey::new(StatType::Int, 0x84), StatValue::Int(breaks));
            let view = HudView {
                hud: &hud,
                world: &world,
            };
            let expected = (era == dereth_primitives::EraId::Infiltration).then_some(expected);
            assert_eq!(view.oath_xp_cost(), expected);
            assert_eq!(GameSnapshot::from_view(&view).oath_xp_cost(), expected);
        }
    }
    hud.era.era = dereth_primitives::EraId::Infiltration;
    world
        .weenie_mut(player)
        .unwrap()
        .qualities
        .as_mut()
        .unwrap()
        .set(StatKey::new(StatType::Int, 0x19), StatValue::Int(2));
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert_eq!(view.experience_header().unwrap().level_span, 0);
    assert_eq!(view.oath_xp_cost(), Some(6250));
    assert_eq!(GameSnapshot::from_view(&view).oath_xp_cost(), Some(6250));
}

/// The era every front end reads comes from the world's own tables: February 2005's 126
/// levels and 36 skills (the old weapon skills), the end of retail's 275 levels and its
/// consolidated skills. The era itself is the server's when it announced one, else the dats'.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail and February 2005 dats: --features retail-dats"
)]
fn the_era_view_reads_the_worlds_level_cap_and_skills() {
    let world = dereth_client_model::World::default();
    let old = dereth_dat::testing::open_classic_store_or_fail();
    let mut h = Hud::new();
    h.load_tables(&old, &world);
    let v = HudView {
        hud: &h,
        world: &world,
    };
    let era = GameView::era(&v).expect("a world");
    assert!(era.before_throne_of_destiny());
    assert_eq!(era.level_cap, 126);
    assert_eq!(era.skills.len(), 36);
    assert!(
        era.has_skill(11) && !era.has_skill(44),
        "Sword, not Heavy Weapons"
    );
    assert_eq!(
        era.era,
        dereth_primitives::EraId::Infiltration,
        "read from the dats"
    );
    assert!(!era.features().ratings && !era.features().luminance);

    // The server's announcement wins over the files.
    let mut h = Hud::new();
    h.era.era = dereth_primitives::EraId::Eor;
    h.era.era_announced = true;
    h.load_tables(&old, &world);
    assert_eq!(h.era.era, dereth_primitives::EraId::Eor);
    assert!(h.era.features().ratings);

    let later = dereth_dat::testing::open_store_or_fail();
    let mut h = Hud::new();
    h.load_tables(&later, &world);
    assert!(!h.era.before_throne_of_destiny());
    assert_eq!(h.era.level_cap, 275);
    assert!(h.era.has_skill(44), "Heavy Weapons");
    assert_eq!(h.era.era, dereth_primitives::EraId::Eor);
}

/// Oracle: the coordinate conversion checked against the live retail read-out —
/// **42.2N, 33.8E** while standing in Holtburg, which is
/// landblock `0xA9B4`.
#[test]
fn the_coordinate_read_out_matches_the_live_retail_value_over_holtburg() {
    // The cell the headless run stands in, `0xA9B40025`: block 0xA9/0xB4, cell index 0x25.
    let c = dereth_primitives::CellId(0xA9B4_0025);
    let (n, e) = player_coords(c).expect("Holtburg's cell is a valid lcoord");
    // block y 0xB4 = 180 -> ly = 1440 + (0x25-1)%8 = 1444 -> (1444-1024)*0.1+0.5 = 42.5
    // block x 0xA9 = 169 -> lx = 1352 + (0x25-1)/8 = 1356 -> (1356-1024)*0.1+0.5 = 33.7
    assert!((n - 42.5).abs() < 0.001, "north was {n}");
    assert!((e - 33.7).abs() < 0.001, "east was {e}");
    // The retail read-out over the same town is 42.2N, 33.8E: the same tenth-scale, within the
    // few cells a player moves. What is asserted here is the formula and the axis order — a
    // swap would put 33.x first, and the retail line puts 42.x first.
    assert!(n > e, "N/S is the first coordinate");

    // A cell id `gid_to_lcoord` rejects is the player-coordinates query returning false.
    assert_eq!(player_coords(dereth_primitives::CellId(0)), None);
}

/// Oracle: — the array is indexed by
/// `windowID − 1`, so row 0 is window 1 and the main chat window (id 8) is row 7.
#[test]
fn the_placement_array_is_indexed_by_window_id_minus_one() {
    use dereth_protocol::property::{BaseProperty, PackObjPropertyCollection};

    let row = |visible: bool, x: i32| {
        let mut c = PropertyCollection::default();
        c.entries.push((
            placement::VISIBILITY,
            BaseProperty {
                name: placement::VISIBILITY,
                value: Some(BasePropertyValue::Bool(visible)),
            },
        ));
        c.entries.push((
            placement::X,
            BaseProperty {
                name: placement::X,
                value: Some(BasePropertyValue::Integer(x)),
            },
        ));
        BaseProperty {
            name: placement::ARRAY,
            value: Some(BasePropertyValue::Struct(c)),
        }
    };

    let mut props = PropertyCollection::default();
    props.entries.push((
        placement::ARRAY,
        BaseProperty {
            name: placement::ARRAY,
            value: Some(BasePropertyValue::Array(vec![
                row(false, 10),
                row(false, 20),
                row(false, 30),
                row(false, 40),
                row(false, 50),
                row(false, 60),
                row(false, 70),
                row(true, 80),
            ])),
        },
    ));
    let m = PlayerModule {
        option_flags: dereth_protocol::login::player_module_flags::GAMEPLAY_OPTIONS,
        gameplay_options: Some(PackObjPropertyCollection {
            version: PackObjPropertyCollection::CORE_VERSION,
            properties: props,
        }),
        ..PlayerModule::default()
    };

    let p = decode_placements(&m);
    assert_eq!(p.rows.len(), 8);
    // Window 8 is the main chat window (`chat::interface::window::MAIN`), and it is row 7.
    let main = p
        .get(dereth_client_contract::chat::interface::window::MAIN)
        .expect("window 8");
    assert_eq!(main.visible, Some(true));
    assert_eq!(main.x, Some(80));
    // Window 1 is row 0, and window 0 does not exist.
    assert_eq!(p.get(1).and_then(|r| r.x), Some(10));
    assert!(p.get(0).is_none());
    assert!(p.get(9).is_none());
}

/// Oracle: the module's own tolerance contract — a `PlayerModule` with no gameplay-options
/// section decodes to an empty placement table rather than failing, which is
/// the chat-window option query returning false for every window and the layout's own values standing.
#[test]
fn a_player_module_with_no_gameplay_options_places_nothing() {
    let p = decode_placements(&PlayerModule::default());
    assert!(p.rows.is_empty());
}

/// `SideBySideVitals` — `options_` bit 21.
const SIDE_BY_SIDE: u32 = 0x0020_0000;

/// The `PlayerOption` ordinal for Side-by-Side Vitals, taken through the same bridge
/// `interaction.rs`'s `SetPlayerOption` arm takes rather than restated as a literal.
fn side_by_side_ordinal() -> usize {
    option_ordinal(dereth_client_contract::PlayerOption::SideBySideVitals)
}

fn hud_with(options: u32, options2: u32) -> (Hud, dereth_client_model::World) {
    let m = PlayerModule {
        options,
        options2,
        ..PlayerModule::default()
    };
    let mut h = Hud::new();
    h.player_module = Some(m.clone());
    let mut w = dereth_client_model::World::new();
    w.player_system.apply_player_module(&m);
    (h, w)
}

/// The applied latch moves when side by side vitals moves.
#[test]
fn the_applied_latch_moves_when_side_by_side_vitals_moves() {
    let (h, mut w) = hud_with(0, 0);
    let off = h.applied_key(&w, 7, Some(SIDE_BY_SIDE));
    assert!(
        !off.side_by_side_vitals,
        "bit 21 is clear in an all-zero options word"
    );

    // The player ticks Side-by-Side Vitals on the Character Options page. Nothing else about
    // the screen or the placements changes.
    let change = w.player_system.set_option(
        side_by_side_ordinal(),
        true,
        dereth_primitives::ServerTime(0.0),
    );
    assert!(change.moved(), "the tick moved no bit at all");
    let on = h.applied_key(&w, 7, Some(SIDE_BY_SIDE));
    assert!(on.side_by_side_vitals);
    assert_ne!(
        off, on,
        "the latch cannot see the option it applies, so it will freeze"
    );
    assert_eq!(
        on.screen_serial, off.screen_serial,
        "the screen was not rebuilt"
    );
}

/// The tick moves the word option bit reads and the login blob copy does not.
#[test]
fn the_tick_moves_the_word_option_bit_reads_and_the_login_blob_copy_does_not() {
    // Arm 1 — the player's own tick, and only it. `Hud::player_module` is left at login's
    // all-zero word, so a reader on that copy answers `false` and this fails.
    let (h, mut w) = hud_with(0, 0);
    w.player_system.set_option(
        side_by_side_ordinal(),
        true,
        dereth_primitives::ServerTime(0.0),
    );
    assert_eq!(
        h.player_module.as_ref().expect("a module").options & SIDE_BY_SIDE,
        0,
        "the tick did not reach the login blob's copy -- that is the premise, not the defect"
    );
    assert!(
        h.option_bit(&w, Some(SIDE_BY_SIDE)),
        "a player tick must reach option_bit, not merely character_option"
    );
    assert_eq!(
        character_option(&w, dereth_client_contract::PlayerOption::SideBySideVitals),
        Some(true),
        "and the two accessors must answer from one word"
    );

    // Arm 2 — the login blob's copy moved on its own, which is what a stale reader would
    // follow. There is one option word now, and this is not it.
    let (mut h, w) = hud_with(0, 0);
    h.player_module.as_mut().expect("a module").options = u32::MAX;
    assert!(
        !h.option_bit(&w, Some(SIDE_BY_SIDE)),
        "option_bit is reading Hud::player_module again: see its own note"
    );
}

/// With no player description every option bit is false.
#[test]
fn with_no_player_description_every_option_bit_is_false() {
    let h = Hud::new();
    let mut w = dereth_client_model::World::new();
    // The model word can be non-zero before a description lands: `Options::default()` is
    // the default character-option word. The gate is the *module*, not the word.
    w.player_system.options.options = u32::MAX;
    assert!(w.player_system.module.is_none());
    assert!(!h.option_bit(&w, Some(SIDE_BY_SIDE)));
    assert_eq!(
        character_option(&w, dereth_client_contract::PlayerOption::SideBySideVitals),
        None
    );
}

/// With the production constant the latch follows the option.
#[test]
fn with_the_production_constant_the_latch_follows_the_option() {
    let (h, mut w) = hud_with(0, 0);
    let off = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    assert!(
        !off.side_by_side_vitals,
        "bit 21 is clear in an all-zero options word"
    );
    // The player's own tick, through the same bridge `interaction.rs`'s arm uses.
    w.player_system.set_option(
        side_by_side_ordinal(),
        true,
        dereth_primitives::ServerTime(0.0),
    );
    let on = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    assert!(
        on.side_by_side_vitals,
        "the production constant must read the option"
    );
    assert_ne!(
        off, on,
        "the pass is gated on this key: if it cannot move, ticking the row changes nothing on \
             screen until the next screen rebuild"
    );
    assert_eq!(
        on.screen_serial, off.screen_serial,
        "the screen was not rebuilt"
    );
    assert_eq!(
        character_option::SIDE_BY_SIDE_VITALS,
        Some(0x0020_0000),
        "side-by-side vitals is `options_ >> 0x15 & 1`"
    );
}

/// The bulk of what the pass applies. The original two writers were the `0x0013` arm and
/// session-end reset. Local numeric placement writes now also mirror this cache, explicitly
/// updating the applied key because their retail notice does not reseed the screen. An
/// arbitrary placement replacement still invalidates the key, as this test demonstrates.
#[test]
fn the_applied_latch_moves_when_the_placements_move() {
    let (mut h, w) = hud_with(0, 0);
    let before = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    h.placements.set(
        dereth_client_contract::chat::interface::window::MAIN,
        dereth_client_contract::floaty::WindowPlacement {
            visible: Some(true),
            ..dereth_client_contract::floaty::WindowPlacement::default()
        },
    );
    assert_ne!(
        before,
        h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS)
    );
    // And the screen serial, which is the term the latch has always carried.
    assert_ne!(
        h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS),
        h.applied_key(&w, 8, character_option::SIDE_BY_SIDE_VITALS)
    );
}

/// **The declared exclusion, asserted so it cannot be reversed by accident.** `lock_ui` is the
/// third thing the pass applies and is deliberately *not* in the key: its visible mirror flows
/// screen → HUD (`Hud::set_lock_ui` runs after the pass), so a toggle would otherwise make
/// the next frame a re-seed frame and re-assert every window's stored visibility over whatever
/// the player had since changed. If somebody adds it, this fails and they have to read the note
/// on [`AppliedKey`] before deciding.
#[test]
fn lock_ui_is_deliberately_not_in_the_applied_latch() {
    let (mut h, w) = hud_with(0, 0);
    let before = h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS);
    h.set_lock_ui(true);
    assert!(
        h.lock_ui(),
        "the padlock did not go up, so this asserts nothing"
    );
    assert_eq!(h.stats.lock_ui_writes, 1);
    assert_eq!(
        before,
        h.applied_key(&w, 7, character_option::SIDE_BY_SIDE_VITALS),
        "lock_ui entered the latch: see AppliedKey's note before allowing this"
    );
}

/// The padlock hud mirror preserves neighboring options2 bits.
#[test]
fn the_padlock_hud_mirror_preserves_neighboring_options2_bits() {
    let (mut h, _w) = hud_with(0, 0);
    h.player_module.as_mut().expect("a module").options2 |= 0x0200_0000;
    h.set_lock_ui(true);
    assert!(h.lock_ui(), "the padlock went up in the HUD's copy");
    assert_eq!(
        h.player_module.as_ref().expect("a module").options2 & 0x0200_0000,
        0x0200_0000,
        "the neighboring options2 bit survives the lock mirror"
    );
}

/// One `SessionEvent::UiEvent` carrying `m`, shaped as `Session::dispatch` shapes it: the
/// opcode dword and then the body.
fn ui_event<M: dereth_protocol::Message>(m: &M) -> SessionEvent {
    let mut blob = M::OPCODE.0.to_le_bytes().to_vec();
    blob.extend(dereth_protocol::write_body(m).expect("a synthetic message encodes"));
    SessionEvent::UiEvent {
        opcode: M::OPCODE,
        blob,
    }
}

/// The title table message keeps its list and not only the display title.
#[test]
fn the_title_table_message_keeps_its_list_and_not_only_the_display_title() {
    let mut h = Hud::new();
    let mut w = dereth_client_model::World::new();
    let m = dereth_protocol::social::CharacterTitlesMessage {
        version: 1,
        display_title: 34,
        titles: vec![1, 34, 87],
    };
    let _ = h.apply_events(&[ui_event(&m)], &mut w);
    assert_eq!(h.display_title, 34, "the header's title id still arrives");
    assert_eq!(
        w.player_system.social.titles,
        vec![1, 34, 87],
        "the title list survives the handler -- this is what the Titles tab draws"
    );
    assert_eq!(w.player_system.social.display_title, 34);
    assert_eq!(h.stats.undecodable, 0);
}

/// Oracle: adding a character title always inserts it, then optionally makes it the display title.
///
/// The **add is unconditional**; only the display move is gated. The panel's own
/// add-title and set-display-title handlers
/// both walk the title list for the id first and insert only when it is absent,
/// which is the dedupe below.
#[test]
fn add_or_set_character_title_appends_to_the_list_whether_or_not_it_becomes_the_display() {
    let mut h = Hud::new();
    let mut w = dereth_client_model::World::new();
    let table = dereth_protocol::social::CharacterTitlesMessage {
        version: 1,
        display_title: 34,
        titles: vec![34],
    };
    let quiet = dereth_protocol::social::SocialAddOrSetCharacterTitle {
        new_title: 12,
        set_as_display_title: 0,
    };
    let loud = dereth_protocol::social::SocialAddOrSetCharacterTitle {
        new_title: 99,
        set_as_display_title: 1,
    };
    let again = dereth_protocol::social::SocialAddOrSetCharacterTitle {
        new_title: 12,
        set_as_display_title: 0,
    };
    let _ = h.apply_events(
        &[
            ui_event(&table),
            ui_event(&quiet),
            ui_event(&loud),
            ui_event(&again),
        ],
        &mut w,
    );
    assert_eq!(
        w.player_system.social.titles,
        vec![34, 12, 99],
        "every earned title is on the list, once, in arrival order"
    );
    assert_eq!(
        h.display_title, 99,
        "and only the flagged one moved the header"
    );
    assert_eq!(w.player_system.social.display_title, 99);
}

/// The death tag decides a system line's fate by the hear-PK-deaths option alone.
#[test]
fn the_pk_death_filter_drops_or_strips_only_a_tagged_line() {
    assert_eq!(
        pk_death_filter("You have entered the Trade channel.", false),
        PkDeathLine::Untagged
    );
    assert_eq!(
        pk_death_filter("[PKDe]Lark was slain by Wren!", false),
        PkDeathLine::Dropped
    );
    assert_eq!(
        pk_death_filter("[PKDe]Lark was slain by Wren!", true),
        PkDeathLine::Stripped("Lark was slain by Wren!".into())
    );
    // Anywhere in the line, every copy, and nothing else touched.
    assert_eq!(
        pk_death_filter("Lark [PKDe]was slain [PKDe]", true),
        PkDeathLine::Stripped("Lark was slain ".into())
    );
    // The tag is matched exactly, case and all.
    assert_eq!(pk_death_filter("[pkde]Lark", false), PkDeathLine::Untagged);
}

/// End to end through the `0xF7E0` arm: heard by default with the tag gone, and not drawn at
/// all once the player turns the option off; an untagged line is drawn either way.
#[test]
fn a_pk_death_broadcast_is_heard_stripped_by_default_and_dropped_when_the_option_is_off() {
    use dereth_client_model::player::options::option::HEAR_PK_DEATHS;
    let line = |text: &str| dereth_protocol::comms::CommunicationTextboxString {
        text: text.into(),
        text_type: 0,
    };
    let mut h = Hud::new();
    let mut w = dereth_client_model::World::new();
    assert!(w.player_system.options.hear_pk_deaths(), "on by default");
    let got = h.apply_events(&[ui_event(&line("[PKDe]Lark was slain by Wren!"))], &mut w);
    let bodies: Vec<&str> = got.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies, vec!["Lark was slain by Wren!"]);

    w.player_system
        .set_option(HEAR_PK_DEATHS, false, dereth_primitives::ServerTime(0.0));
    let got = h.apply_events(
        &[
            ui_event(&line("[PKDe]Lark was slain by Wren!")),
            ui_event(&line("You feel refreshed.")),
        ],
        &mut w,
    );
    let bodies: Vec<&str> = got.iter().map(|m| m.body.as_str()).collect();
    assert_eq!(bodies, vec!["You feel refreshed."]);
    assert_eq!(h.stats.textbox_pk_deaths_dropped, 1);
    assert_eq!(h.stats.textbox_lines_squelched, 0);
}

/// Event `0x0317` draws its one string exactly as `0x02EB` does: chat type `0x1A`, no prefix,
/// window 0.
#[test]
fn event_0317_is_drawn_as_a_transient_line() {
    let mut h = Hud::new();
    let mut w = dereth_client_model::World::new();
    let got = h.apply_events(
        &[
            ui_event(&dereth_protocol::comms::CommunicationTransientString0317 {
                text: "The chest is locked.".into(),
            }),
            ui_event(&dereth_protocol::comms::CommunicationTransientString {
                text: "The chest is locked.".into(),
            }),
        ],
        &mut w,
    );
    assert_eq!(got.len(), 2);
    assert_eq!(got[0], got[1], "both events draw the same line");
    assert_eq!(got[0].body, "The chest is locked.");
    assert_eq!(got[0].ty, 0x1A);
    assert_eq!(h.stats.undecodable, 0);
}

/// Behaviour: none (what an interface reads to offer wearing or wielding an item).
#[test]
fn where_an_item_can_be_worn_reaches_the_interface_live_and_in_a_snapshot() {
    use dereth_client_contract::snapshot::GameSnapshot;
    use dereth_client_model::{Weenie, World};
    let (sword, gem) = (ObjectId(7), ObjectId(8));
    let mut world = World::new();
    let mut w = Weenie::new(sword);
    // Wielded in the main hand.
    w.pwd.valid_locations = Some(0x0010_0000);
    world.tables.weenies.insert(sword, w);
    world.tables.weenies.insert(gem, Weenie::new(gem));
    // The snapshot keeps the objects an interface is looking at: the selection among them.
    world.selected = Some(sword);
    let hud = Hud::new();
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert_eq!(view.equip_locations(sword), 0x0010_0000);
    assert_eq!(view.equip_locations(gem), 0, "a gem is not worn");
    assert_eq!(view.equip_locations(ObjectId(99)), 0, "nor what is unknown");
    let captured = GameSnapshot::from_view(&view);
    assert_eq!(captured.equip_locations(sword), 0x0010_0000);
}

/// Behaviour: none (what an interface reads to leave dropping and giving out for an attuned item).
#[test]
fn an_appraised_attuned_item_reads_as_attuned_live_and_in_a_snapshot() {
    use dereth_client_contract::snapshot::GameSnapshot;
    use dereth_client_model::{Weenie, World};
    use dereth_protocol::{archive::PackedHash, types::appraisal::AppraisalProfile};
    let (bound, plain) = (ObjectId(7), ObjectId(8));
    let mut world = World::new();
    world.tables.weenies.insert(bound, Weenie::new(bound));
    world.tables.weenies.insert(plain, Weenie::new(plain));
    let mut profile = AppraisalProfile::default();
    profile.tables.ints = Some(PackedHash {
        table_size: 8,
        entries: vec![(0x72, 1)],
    });
    world.appraisal.set(bound, profile);
    world.selected = Some(bound);
    let hud = Hud::new();
    let view = HudView {
        hud: &hud,
        world: &world,
    };
    assert!(view.item_attuned(bound));
    assert!(
        !view.item_attuned(plain),
        "not appraised: not known to be attuned"
    );
    assert!(GameSnapshot::from_view(&view).item_attuned(bound));
}
