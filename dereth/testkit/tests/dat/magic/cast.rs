use dereth_client_model::{NullSink, World};
use dereth_primitives::ObjectId;
use dereth_testkit::HeadlessClient;

use super::support::{a_client_with_a_peer, chat_log, Peer, PLAYER};

/// The shard's answer when a spell fizzles.
pub const YOUR_SPELL_FIZZLED: u32 = 0x0402;
/// The line the client writes for it, without the line break the chat system trims.
pub const FIZZLED: &str = "Your spell fizzled.";
/// The channel it is written on.
pub const MAGIC_CHANNEL: u8 = 7;
/// The refusal the client makes when a spell cannot be cast on what is selected.
pub const CANNOT_CAST_ON: &str = "This spell cannot be cast on ";
/// And the line it writes when a cast really starts.
pub const CASTING: &str = "Casting ";

/// The subject: a war spell with an eight-slot formula.
const SUBJECT: u32 = 62;
pub const SPELL: &str = "Acid Stream V";

/// The legal target: an attackable creature.
pub const DRUDGE: ObjectId = ObjectId(0x6000_0001);
/// The illegal one: an object the client knows and that is not attackable.
pub const STATUE: ObjectId = ObjectId(0x6000_0002);
pub const STATUE_NAME: &str = "Ancient Statue";
/// Somewhere that is not the player's pack.
const ELSEWHERE: ObjectId = ObjectId(0x7000_0001);
/// One component object per formula slot.
const COMPONENTS: [ObjectId; 8] = [
    ObjectId(0x8000_0301),
    ObjectId(0x8000_0302),
    ObjectId(0x8000_0303),
    ObjectId(0x8000_0304),
    ObjectId(0x8000_0305),
    ObjectId(0x8000_0306),
    ObjectId(0x8000_0307),
    ObjectId(0x8000_0308),
];

/// The bit that says an object may be attacked.
const ATTACKABLE: u32 = 0x0010;
const TYPE_CREATURE: u32 = 0x0000_0010;
const TYPE_SPELL_COMPONENTS: u32 = 0x0000_1000;
/// The quality that says this character has to spend components at all.
const COMPONENTS_REQUIRED: u32 = 0x44;

/// The two cast messages, by ordered sub-type.
const CAST_TARGETED: u32 = 0x004A;
const CAST_UNTARGETED: u32 = 0x0048;

/// The two recorded fizzles, and the recording that carries them.
///
/// The recordings are the ones the corpus index names. That the fizzles arrive as the shard's
/// error message is read here rather than written down.
pub fn recorded_fizzles() -> (&'static str, Vec<Vec<u8>>) {
    /// The acknowledgement, and the error message.
    const USE_DONE: u32 = 0x01C7;
    const WEENIE_ERROR: u32 = 0x028A;

    let is_fizzle = |b: &Vec<u8>| {
        b.len() >= 20
            && u32::from_le_bytes(b[16..20].try_into().expect("four bytes")) == YOUR_SPELL_FIZZLED
    };
    let mut carrying: Vec<(&'static str, Vec<Vec<u8>>)> = Vec::new();
    let mut acknowledgements = 0usize;
    for (id, _slug) in dereth_client_net::client_session::testing::session_index() {
        // The acknowledgement is read only as the reader's own control. Over every recording
        // the index names, the fizzle is neither confined to one recording nor absent from the
        // acknowledgement, so this scenario claims neither.
        acknowledgements += super::support::recorded_event(id, USE_DONE).len();
        let errors: Vec<Vec<u8>> = super::support::recorded_event(id, WEENIE_ERROR)
            .into_iter()
            .filter(is_fizzle)
            .collect();
        if !errors.is_empty() {
            carrying.push((id, errors));
        }
    }
    assert!(
        acknowledgements > 0,
        "the reader has to be able to look: no acknowledgement anywhere means it is wrong"
    );
    assert!(
        !carrying.is_empty(),
        "no recording carries a fizzle on the shard's error message at all"
    );
    carrying.remove(0)
}

pub struct CastBench {
    c: HeadlessClient,
    peer: Peer,
}

impl CastBench {
    pub fn new() -> Self {
        let (mut c, mut peer) = a_client_with_a_peer();
        {
            let w = c.world_mut();
            let me = w.weenie_mut(PLAYER).expect("the player was created");
            me.pwd.name = "Aldis".into();
            me.pwd.obj_type = TYPE_CREATURE;
        }
        // The description is what pushes the shipped spell table, the component table and the
        // components-required quality into the game model.
        peer.event(&mut c, &a_caster());
        c.tick(8);
        assert!(
            c.view().expect_app().hud().player_desc_received,
            "the description reached the player-description arm"
        );
        assert!(
            c.view().world().magic.spell_table.is_some(),
            "without the shipped spell table the cast takes its silent no-table arm"
        );
        assert!(
            !c.view().world().magic.catalogue.is_empty(),
            "and without the component table no component resolves to a class"
        );
        assert!(
            c.view().world().are_spell_components_required(),
            "the premise: this character has to spend components, so the check runs"
        );

        for (id, name, bitfield) in [
            (DRUDGE, "Drudge Slave", ATTACKABLE),
            (STATUE, STATUE_NAME, 0),
        ] {
            let w = c.world_mut();
            put(
                w,
                id,
                dereth_protocol::types::PublicWeenieDesc {
                    name: name.to_owned(),
                    obj_type: TYPE_CREATURE,
                    bitfield,
                    ..Default::default()
                },
            );
        }

        let mut b = Self { c, peer };
        let slots = b.formula_slots();
        assert_eq!(
            slots.len(),
            8,
            "the subject spell has an eight-slot formula"
        );
        for (i, scid) in slots.iter().enumerate() {
            b.restock(i, *scid, 10);
        }
        b.c.tick(2);
        let _ = b.wire();
        b
    }

    /// The formula's components, out of the cast path's own source.
    pub fn formula_slots(&self) -> Vec<u32> {
        let w = self.c.view().world();
        let table = w
            .magic
            .spell_table
            .clone()
            .expect("the description pushed the table");
        let base = table.spells.get(&SUBJECT).expect("the subject spell");
        assert_eq!(base.name, SPELL);
        let f = w.spell_formula(base);
        let n = dereth_rules::magic::num_spell_components(&f);
        f[..n].to_vec()
    }

    /// Put `stack` of one component class in the pack, through the seam the shard's own move
    /// arrives on -- never by a hand call into the tracker.
    pub fn restock(&mut self, slot: usize, scid: u32, stack: u16) {
        let (wcid, name) = {
            let cat = &self.c.view().world().magic.catalogue;
            let wcid = cat.scid_to_wcid(scid);
            assert_ne!(wcid, 0, "the component resolves to a class");
            (
                wcid,
                cat.inq_spell_component_base(scid)
                    .expect("a base")
                    .name
                    .clone(),
            )
        };
        let id = COMPONENTS[slot];
        let w = self.c.world_mut();
        put(
            w,
            id,
            dereth_protocol::types::PublicWeenieDesc {
                name,
                wcid,
                obj_type: TYPE_SPELL_COMPONENTS,
                stack_size: Some(stack),
                container_id: Some(PLAYER),
                ..Default::default()
            },
        );
        refresh_contents(w, PLAYER);
        w.server_says_move_item(id, PLAYER, 0, ObjectId(0), 0, true, &mut NullSink);
        self.c.tick(2);
    }

    /// Take every object of the first slot's class out of the pack. Owning is membership of
    /// the class, so one stack left behind would leave the cast legal.
    pub fn drop_the_first_component_class(&mut self) {
        let slots = self.formula_slots();
        let dropped: Vec<ObjectId> = slots
            .iter()
            .enumerate()
            .filter(|(_, s)| **s == slots[0])
            .map(|(i, _)| COMPONENTS[i])
            .collect();
        let w = self.c.world_mut();
        put(
            w,
            ELSEWHERE,
            dereth_protocol::types::PublicWeenieDesc::default(),
        );
        for id in &dropped {
            if let Some(x) = w.tables.weenies.get_mut(*id) {
                x.pwd.container_id = Some(ELSEWHERE);
            }
        }
        refresh_contents(w, PLAYER);
        for id in &dropped {
            w.server_says_move_item(*id, ELSEWHERE, 0, ObjectId(0), 0, true, &mut NullSink);
        }
        self.c.tick(2);
    }

    /// How many of the class the client believes the player holds.
    pub fn held(&self, scid: u32) -> i64 {
        let w = self.c.view().world();
        let wcid = w.magic.catalogue.scid_to_wcid(scid);
        w.magic.components.num_component(&w.magic.catalogue, wcid)
    }

    /// Select a target the way the client's own selection does, then press Cast -- which is
    /// the only route into the cast at all.
    pub fn cast_at(&mut self, target: Option<ObjectId>) {
        self.c
            .world_mut()
            .set_selected_object(target, false, &mut NullSink);
        let mut panels = std::mem::take(&mut self.c.app_mut().probe_mut().hud_mut().panels);
        {
            let shell = self.c.app_mut().ui_mut().expect("the shell");
            let ui = &mut shell.ui;
            let tab = panels.spellcasting.open_sub_menu_index(ui);
            panels.spellcasting.set_selected(ui, tab, SUBJECT);
            panels.spellcasting.cast(ui);
        }
        self.c.app_mut().probe_mut().hud_mut().panels = panels;
        self.c.tick(3);
    }

    /// The shard's acknowledgement of the cast.
    pub fn use_done(&mut self, failure_type: u32) {
        self.peer.event(
            &mut self.c,
            &dereth_protocol::objects::ItemUseDone { failure_type },
        );
        self.c.tick(6);
    }

    /// Replay one recorded blob, re-addressed and re-stamped for this session: the recorded
    /// sequence numbers are the ones that session had reached, and replaying them into a
    /// session whose counter starts at zero would stall every one of them.
    pub fn replay(&mut self, blob: Vec<u8>) {
        self.peer.replay_blob(&mut self.c, blob);
        self.c.tick(6);
    }

    /// The ordered game actions this client has built into a datagram since the last look.
    fn wire(&mut self) -> Vec<u32> {
        let out = self
            .c
            .app_mut()
            .replay_network_mut()
            .expect("the replay endpoint")
            .take_outgoing();
        let mut ops = Vec::new();
        for (raw, _) in &out {
            let Ok(p) = dereth_transport::ParsedPacket::parse(raw) else {
                continue;
            };
            for f in &p.fragments {
                if f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4 {
                    continue;
                }
                if u32::from_le_bytes(f.payload[0..4].try_into().expect("four bytes"))
                    != dereth_protocol::OrderedActionHeader::MAGIC
                {
                    continue;
                }
                ops.push(u32::from_le_bytes(
                    f.payload[8..12].try_into().expect("four bytes"),
                ));
            }
        }
        ops
    }

    pub fn wire_has_targeted_cast(&mut self) -> bool {
        self.wire().contains(&CAST_TARGETED)
    }

    pub fn wire_has_any_cast(&mut self) -> bool {
        let w = self.wire();
        w.contains(&CAST_TARGETED) || w.contains(&CAST_UNTARGETED)
    }

    /// The strip the client's own refusals are drawn in. They are on a channel the shipped
    /// chat windows filter out, so this is where they are read.
    pub fn spew(&self) -> Vec<String> {
        let m = &self.c.view().expect_app().hud().panels.spew.model;
        m.items.iter().chain(m.pending.iter()).cloned().collect()
    }

    pub fn spew_lines(&self) -> u64 {
        self.c.view().expect_app().hud().stats.spew_lines
    }

    pub fn chat(&mut self) -> Vec<(u8, String)> {
        chat_log(&mut self.c)
    }

    pub fn busy_count(&self) -> u32 {
        self.c.view().world().magic.busy_count
    }

    pub fn spells_cast(&self) -> u64 {
        self.c.view().expect_app().interaction().stats.spells_cast
    }

    pub fn uses_done(&self) -> u64 {
        self.c.view().expect_app().interaction().stats.uses_done
    }

    pub fn shutdown(self) {
        self.c.shutdown();
    }
}

fn put(w: &mut World, id: ObjectId, pwd: dereth_protocol::types::PublicWeenieDesc) {
    let mut wn = dereth_client_model::weenie::Weenie::new(id);
    wn.pwd = pwd;
    w.tables.weenies.insert(id, wn);
}

fn refresh_contents(w: &mut World, container: ObjectId) {
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
    w.view_object_contents(container, &held, &mut NullSink);
}

/// The description a character who has to spend components arrives with.
fn a_caster() -> dereth_protocol::login::LoginPlayerDescription {
    use dereth_protocol::archive::PackedHash;
    use dereth_protocol::types::qualities::{
        base_flags, quality_flags, AcBaseQualities, AcQualities, PropertyTables,
    };
    let qualities = AcQualities {
        base: AcBaseQualities {
            flags: base_flags::BOOL,
            weenie_type: 0x0A,
            tables: PropertyTables {
                bools: Some(PackedHash {
                    table_size: 8,
                    entries: vec![(COMPONENTS_REQUIRED, 1)],
                }),
                ..PropertyTables::default()
            },
        },
        flags: quality_flags::SKILLS,
        skills: Some(PackedHash {
            table_size: 8,
            entries: Vec::new(),
        }),
        ..AcQualities::default()
    };
    dereth_protocol::login::LoginPlayerDescription {
        qualities,
        player_module: dereth_protocol::login::PlayerModule {
            spell_bars: vec![Vec::new()],
            spell_filters: dereth_protocol::login::PlayerModule::DEFAULT_SPELL_FILTERS,
            options2: dereth_protocol::login::PlayerModule::DEFAULT_OPTIONS2,
            ..dereth_protocol::login::PlayerModule::default()
        },
        content_profiles: Vec::new(),
        inventory_placements: Vec::new(),
    }
}
