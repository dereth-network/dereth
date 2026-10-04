use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::types::qualities::{
    AcQualities, Attribute, AttributeCache, Enchantment as ProtocolEnchantment, Skill as WireSkill,
    StatMod,
};
use dereth_testkit::{ClientSpec, HeadlessClient};
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::GameView as _;

/// The character these scenarios drive.
pub const PLAYER: ObjectId = ObjectId(0x5000_0001);

/// The skill the buff below raises, and the one the Skills page row is read off.
const A_SKILL: u32 = 0x2C;
/// The four bits the shard writes for an ordinary beneficial self-buff.
const SKILL_ADD: u32 = 0x0000_0010 | 0x0000_1000 | 0x0000_8000 | 0x0200_0000;
/// The character's own ranks and starting level in that skill.
const SKILL_RANKS: u32 = 50;
const SKILL_INIT: u32 = 5;
const BASE_ATTRIBUTE: u32 = 100;
/// The buff's value, as the spell's own row in the world data carries it.
pub const BUFF_VALUE: i32 = 45;
/// Short enough that a client-side timer, if one existed, would fire inside the clock moves
/// these scenarios make.
pub const BUFF_DURATION: f64 = 20.0;
/// Where the clock is when the buff arrives, and where it is moved to afterwards -- five
/// times the duration later, so nothing can be passing by accident.
const ARMED_AT: f64 = 1000.0;
const LONG_AFTER: f64 = 1120.0;

/// The sentence the client appends when the shard takes an enchantment away, without the
/// newline the chat system trims.
pub const EXPIRED: &str = " has expired.";
/// The chat channel it is written on.
pub const MAGIC: u8 = 7;

/// The shard these scenarios play: `dereth_testkit::replay::Peer`.
pub use dereth_testkit::Peer;

/// A whole client in the gameplay screen with the endpoint attached and the player's own body
/// created, so that an ordered event addressed to him can be delivered.
pub fn a_client_with_a_peer() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    let mut peer = Peer::attach(&mut c, PLAYER);

    let mut p = dereth_protocol::objects::ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut c,
        10,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("the create encodes"),
    );
    c.tick(1);
    c.world_mut().player = Some(PLAYER);
    (c, peer)
}

/// A beneficial spell the shipped table really has a row for, chosen at run time.
///
/// The pane joins every enchantment against that table and drops the ones it misses, so an
/// invented id would make a scenario green by measuring nothing.
pub fn a_beneficial_spell(c: &HeadlessClient) -> (u32, String) {
    let app = c.view().expect_app();
    let table = app
        .hud()
        .spell_table
        .as_ref()
        .expect("the shipped spell table loads");
    table
        .spells
        .iter()
        .find(|(_, b)| b.bitfield & 4 != 0 && !b.name.is_empty())
        .map(|(id, b)| (*id, b.name.clone()))
        .expect("the shipped table has at least one beneficial spell")
}

fn attribute(v: u32) -> Attribute {
    Attribute {
        level_from_cp: 0,
        init_level: v,
        cp_spent: 0,
    }
}

/// A **specialised** skill, so the shipped table's own minimum cannot zero its base level.
fn specialised(ranks: u32) -> WireSkill {
    WireSkill {
        level_from_pp: u16::try_from(ranks).expect("the rank count is small"),
        format_version: 1,
        sac: 3,
        pp: 0,
        init_level: SKILL_INIT,
        resistance_of_last_check: 0,
        last_used_time: 0.0,
    }
}

fn player_module() -> dereth_protocol::login::PlayerModule {
    dereth_protocol::login::PlayerModule {
        spell_bars: vec![Vec::new()],
        spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
        options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
        ..dereth_protocol::login::PlayerModule::default()
    }
}

fn description(qualities: AcQualities) -> dereth_protocol::login::LoginPlayerDescription {
    dereth_protocol::login::LoginPlayerDescription {
        qualities,
        player_module: player_module(),
        content_profiles: Vec::new(),
        inventory_placements: Vec::new(),
    }
}

/// The character, with an **empty** registry, so every enchantment is one that arrived live.
pub fn a_character() -> dereth_protocol::login::LoginPlayerDescription {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::qualities::{attribute_cache_mask as m, quality_flags};
    let cache = AttributeCache {
        flags: m::STRENGTH | m::COORDINATION,
        strength: Some(attribute(BASE_ATTRIBUTE)),
        coordination: Some(attribute(BASE_ATTRIBUTE)),
        ..AttributeCache::default()
    };
    description(AcQualities {
        flags: quality_flags::ATTRIBUTE_CACHE | quality_flags::SKILLS,
        attribute_cache: Some(cache),
        skills: Some(PackedHash {
            table_size: 8,
            entries: vec![(A_SKILL, specialised(SKILL_RANKS))],
        }),
        ..AcQualities::default()
    })
}

/// A description carrying one timed enchantment, in the shard's own writer order: the length
/// is the spell's own and the start is **negative** elapsed, which is how the shard counts it
/// down.
pub fn timed_buff_login(
    spell: u32,
    duration: f64,
    elapsed: f64,
) -> dereth_protocol::login::LoginPlayerDescription {
    use dereth_protocol::types::qualities::{quality_flags, EnchantmentRegistry};
    let e = ProtocolEnchantment {
        id: spell,
        category_word: 1,
        power_level: 6,
        start_time: -elapsed,
        duration,
        caster: PLAYER,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: StatMod {
            kind: SKILL_ADD,
            key: 6,
            value: 40.0,
        },
        spell_set_id: None,
    };
    description(AcQualities {
        flags: quality_flags::ENCHANTMENT_REGISTRY,
        enchantments: Some(EnchantmentRegistry {
            flags: EnchantmentRegistry::ADDITIVE,
            additive: Some(vec![e]),
            ..EnchantmentRegistry::default()
        }),
        ..AcQualities::default()
    })
}

/// One live enchantment message body. Its start is **relative to receipt**, so a buff that has
/// just been cast carries zero.
fn buff(spell: u32, category: u32) -> ProtocolEnchantment {
    ProtocolEnchantment {
        id: spell,
        category_word: category,
        power_level: 8,
        start_time: 0.0,
        duration: BUFF_DURATION,
        caster: PLAYER,
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        #[allow(clippy::cast_precision_loss)]
        smod: StatMod {
            kind: SKILL_ADD,
            key: A_SKILL,
            value: BUFF_VALUE as f32,
        },
        spell_set_id: None,
    }
}

// -----------------------------------------------------------------------------------------
// Reading what the client drew
// -----------------------------------------------------------------------------------------

fn gameplay(c: &mut HeadlessClient) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = c.app_mut().ui_mut().expect("the shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is up");
    (ui, screen)
}

fn find(c: &mut HeadlessClient, id: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay(c);
    let root = screen.root().expect("the gameplay root");
    ui.get_child_recursive(root, id)
        .unwrap_or_else(|| panic!("{:#010X} is not in the shipped tree", id.0))
}

/// Every chat line any window has taken, with the channel it was written on.
pub fn chat_log(c: &mut HeadlessClient) -> Vec<(u8, String)> {
    gameplay(c)
        .1
        .chat
        .iter()
        .flat_map(|w| w.log.iter().cloned())
        .collect()
}

/// The spells the effects pane is drawing, in the order it drew them.
pub fn pane_rows(c: &HeadlessClient) -> Vec<u32> {
    c.view()
        .expect_app()
        .hud()
        .panels
        .effects_helpful
        .rows
        .iter()
        .map(|r| r.spell)
        .collect()
}

/// The drawn number on the Skills page's row for the buffed skill, with its colour.
pub fn skill_row(c: &HeadlessClient) -> (i32, u32) {
    let app = c.view().expect_app();
    let r = app
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.skill == A_SKILL)
        .expect("the buffed skill has a row on the page");
    (r.value, r.font)
}

/// The remaining time the pane would draw for `spell`, or nothing when the row has gone.
pub fn remaining(c: &HeadlessClient, spell: u32) -> Option<f64> {
    let app = c.view().expect_app();
    let view = app.hud().view(app.objects());
    view.active_effects()
        .into_iter()
        .find(|e| e.spell == spell)
        .map(|e| e.remaining)
}

/// The pane's own value cell for `spell`.
pub fn duration_cell(c: &HeadlessClient, spell: u32) -> Option<String> {
    let app = c.view().expect_app();
    let view = app.hud().view(app.objects());
    let e = view
        .active_effects()
        .into_iter()
        .find(|e| e.spell == spell)?;
    Some(dereth_ui_screens::panels::effects::duration_cell(&e, true))
}

/// A cell in either of the two shapes the pane writes, back into seconds, so an assertion can
/// carry a tolerance.
pub fn cell_seconds(cell: &str) -> i64 {
    let parts: Vec<i64> = cell
        .split(':')
        .map(|p| p.parse::<i64>().expect("a numeric field"))
        .collect();
    match parts.as_slice() {
        [m, s] => m * 60 + s,
        [h, m, s] => h * 3600 + m * 60 + s,
        _ => panic!("neither of the two shapes the cell is written in: {cell:?}"),
    }
}

pub fn helpful_panel_is_open(c: &mut HeadlessClient) -> bool {
    let panel = find(c, dereth_ui_screens::panels::effects::HELPFUL_PANEL);
    gameplay(c)
        .0
        .node(panel)
        .expect("a live node")
        .region
        .flags
        .visible
}

// -----------------------------------------------------------------------------------------
// The fixture the four enchantment-life scenarios share
// -----------------------------------------------------------------------------------------

/// The buff lamp the player clicks to open the pane.
const BUFF_LAMP: ElementId = ElementId(0x1000_00F5);

/// A character with one live buff and the helpful pane open, reached the way a player reaches
/// it: the enchantment lights the lamp and a pointer click on the lamp opens the pane.
pub struct Station {
    pub c: HeadlessClient,
    peer: Peer,
    pub spell: u32,
    pub name: String,
    pub base_skill: i32,
}

impl Station {
    pub fn arm() -> Self {
        let (mut c, mut peer) = a_client_with_a_peer();
        peer.set_clock(&mut c, ARMED_AT);
        peer.event(&mut c, &a_character());
        c.tick(8);
        assert!(
            c.view().expect_app().hud().player_desc_received,
            "the description reached the player-description arm"
        );
        assert!(
            c.view().world().magic.spell_table.is_some(),
            "and the shipped spell table is loaded, or every pane row is dropped"
        );
        let base_skill = skill_row(&c).0;
        assert!(
            base_skill >= i32::try_from(SKILL_RANKS + SKILL_INIT).expect("small"),
            "the unbuffed row carries at least the character's own ranks: {base_skill}"
        );
        assert_eq!(
            skill_row(&c).1,
            0,
            "no enchantment yet, so the row is drawn plain"
        );

        let (spell, name) = a_beneficial_spell(&c);
        peer.event(
            &mut c,
            &dereth_protocol::qualities::MagicUpdateEnchantment(buff(spell, 100)),
        );
        c.tick(6);
        assert_eq!(
            {
                let app = c.view().expect_app();
                app.hud().view(app.objects()).enchantment_counts()
            },
            (1, 0),
            "one helpful enchantment was counted, which is what lights the lamp"
        );

        // The three events a player produces, and nothing else.
        let lamp = find(&mut c, BUFF_LAMP);
        {
            let (ui, _) = gameplay(&mut c);
            let b = ui.screen_box(lamp);
            assert!(b.is_valid(), "the lamp has no box to point at");
            let (x, y) = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
            ui.mouse_move(LocalTime(0.0), x, y);
            ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
            ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
        }
        c.tick(4);
        assert!(
            helpful_panel_is_open(&mut c),
            "the click has to open the pane before its contents can matter"
        );
        Self {
            c,
            peer,
            spell,
            name,
            base_skill,
        }
    }

    /// Move the session clock far past the buff's own duration.
    pub fn clock_far_past_the_duration(&mut self) {
        self.peer.set_clock(&mut self.c, LONG_AFTER);
        self.c.tick(8);
        let clock = self.c.view().expect_app().clock().cur_time;
        assert!(
            clock - ARMED_AT > BUFF_DURATION * 5.0,
            "the premise: the clock really moved past the duration ({clock:.1})"
        );
    }

    pub fn remove(&mut self) {
        let spell = self.spell;
        self.peer.event(
            &mut self.c,
            &dereth_protocol::qualities::MagicRemoveEnchantment {
                layered_spell_id: spell,
            },
        );
        self.c.tick(8);
    }

    pub fn dispel(&mut self) {
        let spell = self.spell;
        self.peer.event(
            &mut self.c,
            &dereth_protocol::qualities::MagicDispelEnchantment {
                layered_spell_id: spell,
            },
        );
        self.c.tick(8);
    }

    /// A second beneficial spell, in a different category so the duel keeps both.
    pub fn arm_a_second_buff(&mut self) -> u32 {
        let first = self.spell;
        let second = {
            let app = self.c.view().expect_app();
            let table = app.hud().spell_table.as_ref().expect("the shipped table");
            *table
                .spells
                .iter()
                .find(|(id, b)| **id != first && b.bitfield & 4 != 0 && !b.name.is_empty())
                .map(|(id, _)| id)
                .expect("a second beneficial spell")
        };
        self.peer.event(
            &mut self.c,
            &dereth_protocol::qualities::MagicUpdateEnchantment(buff(second, 101)),
        );
        self.c.tick(8);
        second
    }
}
// -----------------------------------------------------------------------------------------
// The recorded resists
// -----------------------------------------------------------------------------------------

/// The queue the shard puts a chat sentence on.
pub const UI_QUEUE: u16 = 9;
/// And the one it puts an effect on.
pub const SMARTBOX_QUEUE: u16 = 10;

/// The message a chat sentence arrives as, and the one an effect's sound arrives as.
const TEXTBOX: u32 = 0xF7E0;
const SOUND_EVENT: u32 = 0xF750;
/// The sound the shard plays when a spell is resisted.
const RESIST_SOUND: i32 = 0x91;
/// The fragment of the shard's own sentence that marks one.
const RESIST: &str = "resists your spell";
/// The channel the shard marks it with.
const MAGIC_CHANNEL: u32 = 7;

/// Every server-to-client blob of `opcode` in `session`, with the time it arrived.
///
/// `opcode` is the decoded corpus's own label, which for a message inside the ordered
/// envelope is that message's **sub-type**; the payload it hands back still carries the whole
/// envelope, so a body starts sixteen bytes in.
pub fn recorded(session: &str, opcode: u32) -> Vec<(f64, Vec<u8>)> {
    let c = dereth_client_net::client_session::testing::Corpus::load(session)
        .expect("the decoded corpus parses")
        .unwrap_or_else(|| panic!("the decoded corpus has no session {session}"));
    let mut out = Vec::new();
    for b in c.blobs {
        if b.dir != dereth_client_net::client_session::testing::Direction::ServerToClient
            || b.opcode != opcode
        {
            continue;
        }
        let t = std::time::Duration::from_micros(b.t_rel_micros).as_secs_f64();
        out.push((t, b.payload));
    }
    out
}

/// Decode one recorded chat sentence with the production reader.
pub fn textbox(blob: &[u8]) -> dereth_protocol::comms::CommunicationTextboxString {
    dereth_protocol::read_body_padded::<dereth_protocol::comms::CommunicationTextboxString>(
        &blob[4..],
    )
    .expect("the recorded body decodes")
}

/// Decode one recorded sound event with the production reader.
pub fn sound(blob: &[u8]) -> dereth_protocol::objects::EffectsSoundEvent {
    dereth_protocol::read_body_padded::<dereth_protocol::objects::EffectsSoundEvent>(&blob[4..])
        .expect("the recorded body decodes")
}

/// The one recording that carries resists, and each resist paired with the sound that came
/// with it: how long after the sentence the sound arrived, the sentence, the object the sound
/// was played on, and its volume.
///
/// The recordings are the ones the corpus index names, so a recording added or renamed changes
/// what this reads rather than what it asserts.
pub fn resists() -> (&'static str, Vec<(f64, String, ObjectId, f32)>) {
    let mut carrying: Vec<&'static str> = Vec::new();
    for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
        let n = recorded(id, TEXTBOX)
            .iter()
            .filter(|(_, b)| textbox(b).text.contains(RESIST))
            .count();
        if n > 0 {
            carrying.push(id);
        }
    }
    assert_eq!(
        carrying.len(),
        1,
        "exactly one recording carries a resist -- the corpus has one run with any magic in \
             it at all: {carrying:?}"
    );
    let session = carrying[0];

    let texts: Vec<(f64, String)> = recorded(session, TEXTBOX)
        .iter()
        .map(|(t, b)| (*t, textbox(b)))
        .filter(|(_, m)| m.text.contains(RESIST))
        .map(|(t, m)| {
            assert_eq!(
                m.text_type, MAGIC_CHANNEL,
                "the channel is the shard's own and the client does not choose it"
            );
            (t, m.text)
        })
        .collect();
    let sounds: Vec<(f64, ObjectId, f32)> = recorded(session, SOUND_EVENT)
        .iter()
        .map(|(t, b)| (*t, sound(b)))
        .filter(|(_, m)| m.sound_type == RESIST_SOUND)
        .map(|(t, m)| (t, m.id, m.volume))
        .collect();
    assert_eq!(
        texts.len(),
        sounds.len(),
        "each resist the shard announces is paired with the sound it plays"
    );
    (
        session,
        texts
            .into_iter()
            .zip(sounds)
            .map(|((tt, text), (ts, id, volume))| (ts - tt, text, id, volume))
            .collect(),
    )
}

/// Every server-to-client ordered blob in `session` whose sub-type is `sub`.
///
/// The decoded corpus labels an ordered blob with the **envelope's** opcode and hands back the
/// whole envelope, so the sub-type is twelve bytes in and a body sixteen.
pub fn recorded_event(session: &str, sub: u32) -> Vec<Vec<u8>> {
    const ORDERED_EVENT: u32 = 0xF7B0;
    recorded(session, ORDERED_EVENT)
        .into_iter()
        .map(|(_, b)| b)
        .filter(|b| {
            b.get(12..16)
                .is_some_and(|w| u32::from_le_bytes(w.try_into().expect("four bytes")) == sub)
        })
        .collect()
}

/// The first recorded resist sentence, as the shard framed it.
pub fn first_resist_blob(session: &str) -> Vec<u8> {
    recorded(session, TEXTBOX)
        .into_iter()
        .map(|(_, b)| b)
        .find(|b| textbox(b).text.contains(RESIST))
        .expect("the recording carries one")
}

/// The first recorded resist sound, re-addressed to this scenario's player and otherwise
/// untouched.
pub fn first_resist_sound_blob(session: &str) -> Vec<u8> {
    let mut b = recorded(session, SOUND_EVENT)
        .into_iter()
        .map(|(_, b)| b)
        .find(|b| sound(b).sound_type == RESIST_SOUND)
        .expect("the recording carries one");
    b[4..8].copy_from_slice(&PLAYER.0.to_le_bytes());
    b
}
