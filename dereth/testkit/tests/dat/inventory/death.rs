use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::ObjectId;
use dereth_protocol::login::{player_module_flags, LoginPlayerDescription, ShortCutData};
use dereth_testkit::replay::{Peer, OBJECT_QUEUE, ORDERED_QUEUE};
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound};

const ME: ObjectId = ObjectId(0x5700_0001);
/// The wielded thing. It drops to the corpse.
const SWORD: ObjectId = ObjectId(0x5700_0010);
/// A side pack, and the thing in it whose tile must survive.
const PACK: ObjectId = ObjectId(0x5700_0020);
const ROCK: ObjectId = ObjectId(0x5700_0021);
const CORPSE: ObjectId = ObjectId(0x5700_0030);

const SWORD_TILE: i32 = 3;
const ROCK_TILE: i32 = 5;

/// The shipped body a wielded thing hangs off; it is here because the thing needs a real
/// physical body before the shard's delete will be accepted for it, and not as a claim about
/// what a sword looks like.
const SETUP: u32 = 0x0200_0001;

struct Death {
    c: HeadlessClient,
    peer: Peer,
}

impl Death {
    /// A character wielding a sword, carrying a side pack with a rock in it, with the sword
    /// on one shortcut tile and the rock on another.
    fn new() -> Self {
        let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
        {
            let w = &mut c.app_mut().probe_mut().objects_mut().world;
            w.player = Some(ME);
            for id in [ME, PACK, ROCK, CORPSE] {
                let mut wn = dereth_client_model::Weenie::new(id);
                wn.valid = true;
                wn.pwd.name = format!("decor-{:04X}", id.0 & 0xFFFF);
                w.tables.weenies.insert(id, wn);
            }
            {
                let me = w.tables.weenies.get_mut(ME).expect("seeded");
                me.pwd.items_capacity = Some(102);
                me.pwd.containers_capacity = Some(7);
                me.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
            }
            {
                let p = w.tables.weenies.get_mut(PACK).expect("seeded");
                p.pwd.items_capacity = Some(24);
                p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
                p.pwd.container_id = Some(ME);
            }
            {
                let corpse = w.tables.weenies.get_mut(CORPSE).expect("seeded");
                corpse.pwd.items_capacity = Some(24);
                corpse.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE
                    | dereth_rules::weenie::bitfield::CORPSE;
            }
            w.tables
                .weenies
                .get_mut(ROCK)
                .expect("seeded")
                .pwd
                .container_id = Some(PACK);
            w.tables
                .inventories
                .insert(ME, dereth_client_model::objects::ObjectInventory::new(ME));
            w.tables.inventories.insert(
                PACK,
                dereth_client_model::objects::ObjectInventory::new(PACK),
            );
            w.tables.inventories.insert(
                CORPSE,
                dereth_client_model::objects::ObjectInventory::new(CORPSE),
            );
            w.tables
                .inventories
                .get_mut(ME)
                .expect("seeded")
                .add_content(PACK, true, 0);
            w.tables
                .inventories
                .get_mut(PACK)
                .expect("seeded")
                .add_content(ROCK, false, 0);
            w.remake_character_inventory();
        }
        let peer = Peer::attach(&mut c, ME);
        let mut d = Self { c, peer };
        d.c.tick(1);

        d.create(wielded_sword(), 0);
        d.c.tick(2);
        assert!(
            d.c.view().objects().presence(SWORD).is_some(),
            "the premise: the shard's description gave the wielded thing a body of its own"
        );
        assert!(
            d.c.view().world().is_owned_by_player(SWORD),
            "the premise: a wielded thing belongs to whoever is wielding it"
        );

        d.describe(&[(SWORD_TILE, SWORD), (ROCK_TILE, ROCK)]);
        assert_eq!(
            d.tile_of(SWORD),
            Some(SWORD_TILE as usize),
            "the premise: the bar starts with both tiles filled"
        );
        assert_eq!(
            d.tile_of(ROCK),
            Some(ROCK_TILE as usize),
            "the premise: and the other"
        );
        assert!(
            d.given_up().is_empty(),
            "the premise: nothing in the bring-up asked the shard to forget a tile"
        );
        d
    }

    fn create(&mut self, wdesc: dereth_protocol::types::PublicWeenieDesc, instance: u16) {
        let m = dereth_protocol::objects::ItemCreateObject(
            dereth_protocol::objects::ObjectCreatePayload {
                id: SWORD,
                objdesc: dereth_protocol::types::ObjDesc::default(),
                physicsdesc: dereth_protocol::types::PhysicsDesc {
                    bitfield: dereth_protocol::types::physicsdesc::flags::SETUP,
                    state: 0x0040_0408,
                    setup_id: Some(SETUP),
                    timestamps: dereth_protocol::types::PhysicsTimestamps {
                        instance,
                        ..dereth_protocol::types::PhysicsTimestamps::default()
                    },
                    ..dereth_protocol::types::PhysicsDesc::default()
                },
                wdesc,
            },
        );
        let blob = dereth_protocol::write_blob(&m).expect("the description encodes");
        self.peer.send(&mut self.c, OBJECT_QUEUE, blob);
    }

    /// One login description, carrying the bar the shard has stored for this character.
    fn describe(&mut self, bar: &[(i32, ObjectId)]) {
        let mut d = LoginPlayerDescription::default();
        let o = dereth_client_model::player::options::Options::default();
        d.player_module.options = o.options;
        d.player_module.options2 = o.options2;
        d.player_module.option_flags |= player_module_flags::SHORTCUT;
        d.player_module.shortcuts = Some(
            bar.iter()
                .map(|(index, object_id)| ShortCutData {
                    index: *index,
                    object_id: *object_id,
                    spell_id: 0,
                })
                .collect(),
        );
        self.c
            .when(Inbound::event(SessionEvent::PlayerDescription(Box::new(d))))
            .tick(2);
    }

    /// The messages the shard sends when a wielded thing is taken out of the hand on death.
    /// None of them is the one that takes the thing away.
    fn unhanded(&mut self) {
        let wielder = dereth_protocol::qualities::QualitiesUpdateInstanceId(
            dereth_protocol::qualities::PublicUpdate {
                sequence: 1,
                object: SWORD,
                property_id: 3,
                value: ObjectId(0),
            },
        );
        let blob = dereth_protocol::write_blob(&wielder).expect("encodes");
        self.peer.send(&mut self.c, ORDERED_QUEUE, blob);
        let where_it_was = dereth_protocol::qualities::QualitiesUpdateInt(
            dereth_protocol::qualities::PublicUpdate {
                sequence: 1,
                object: SWORD,
                property_id: 10,
                value: 0,
            },
        );
        let blob = dereth_protocol::write_blob(&where_it_was).expect("encodes");
        self.peer.send(&mut self.c, ORDERED_QUEUE, blob);
        let picked_up = dereth_protocol::objects::InventoryPickupEvent {
            id: SWORD,
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 0,
                event: 1,
            },
        };
        let blob = dereth_protocol::write_blob(&picked_up).expect("encodes");
        self.peer.send(&mut self.c, OBJECT_QUEUE, blob);
        self.c.tick(2);
    }

    /// And the one that does: the thing is gone.
    fn taken_away(&mut self) {
        let gone = dereth_protocol::objects::ItemDeleteObject {
            id: SWORD,
            instance_sequence: 0,
        };
        let blob = dereth_protocol::write_blob(&gone).expect("encodes");
        self.peer.send(&mut self.c, OBJECT_QUEUE, blob);
        self.c.tick(4);
    }

    fn tile_of(&self, item: ObjectId) -> Option<usize> {
        self.c.view().world().player_system.shortcut_slot_of(item)
    }

    /// Which tiles the client has asked the shard to forget, in order.
    fn given_up(&self) -> Vec<i32> {
        self.c
            .outbound()
            .iter()
            .filter_map(|r| match r {
                dereth_client_model::Request::RemoveShortCut(m) => i32::try_from(m.index).ok(),
                _ => None,
            })
            .collect()
    }

    /// The number any tile showing `item` would draw.
    fn number_on(&self, item: ObjectId) -> Option<u32> {
        use dereth_ui_screens::view::GameView as _;
        let app = self.c.view().expect_app();
        app.hud()
            .view(app.objects())
            .slot_decoration(item)
            .and_then(|d| d.shortcut_num)
    }
}

fn wielded_sword() -> dereth_protocol::types::PublicWeenieDesc {
    use dereth_protocol::types::weeniedesc::header;
    dereth_protocol::types::PublicWeenieDesc {
        header: header::WIELDER_ID | header::LOCATION | header::VALID_LOCATIONS,
        name: "decor sword".into(),
        icon_id: dereth_protocol::types::weeniedesc::ICON_BASE,
        obj_type: dereth_rules::weenie::item_type::MELEE_WEAPON,
        wielder_id: Some(ME),
        location: Some(dereth_rules::slots::loc::MELEE_WEAPON),
        valid_locations: Some(dereth_rules::slots::loc::MELEE_WEAPON),
        ..dereth_protocol::types::PublicWeenieDesc::default()
    }
}

fn on_the_corpse() -> dereth_protocol::types::PublicWeenieDesc {
    use dereth_protocol::types::weeniedesc::header;
    dereth_protocol::types::PublicWeenieDesc {
        header: header::CONTAINER_ID,
        name: "decor sword".into(),
        icon_id: dereth_protocol::types::weeniedesc::ICON_BASE,
        obj_type: dereth_rules::weenie::item_type::MELEE_WEAPON,
        container_id: Some(CORPSE),
        ..dereth_protocol::types::PublicWeenieDesc::default()
    }
}

/// The wielded thing dying with the character empties its tile and asks the shard once.
pub fn a_wielded_thing_that_dies_empties_its_tile_and_tells_the_shard_once() {
    let mut d = Death::new();

    d.unhanded();
    let unmoved = d.tile_of(SWORD) == Some(SWORD_TILE as usize) && d.given_up().is_empty();

    d.taken_away();
    let accepted = d.c.view().objects().stats.removes == 1;
    let emptied = d.tile_of(SWORD).is_none();
    let survivor = d.tile_of(ROCK) == Some(ROCK_TILE as usize);
    d.c.tick(8);
    let asked_once = d.given_up() == vec![SWORD_TILE];

    d.c.assert_behaviour(
            "shortcut.bar.a-wielded-thing-that-dies-with-the-player-empties-its-tile-and-tells-the-shard-once",
            move |_| unmoved && accepted && emptied && survivor && asked_once,
        );
    d.c.shutdown();
}

/// The corpse's copy of the same thing draws no number and asks for nothing again.
pub fn the_corpses_copy_draws_no_number_and_asks_for_nothing_again() {
    let mut d = Death::new();
    let drawn_before = d.number_on(SWORD) == Some(SWORD_TILE as u32);

    d.unhanded();
    d.taken_away();
    d.c.tick(8);
    let asked_once = d.given_up() == vec![SWORD_TILE];

    // The corpse is opened: the same thing under the same name, and the shard listing it.
    d.create(on_the_corpse(), 1);
    d.c.tick(2);
    let listed = dereth_protocol::objects::ItemOnViewContents {
        container: CORPSE,
        contents: vec![dereth_protocol::types::ContentProfile {
            iid: SWORD,
            container_properties: 0,
        }],
    };
    let blob = dereth_protocol::write_blob(&listed).expect("encodes");
    d.peer.send(&mut d.c, ORDERED_QUEUE, blob);
    d.c.tick(3);

    let back_on_screen = {
        use dereth_ui_screens::view::GameView as _;
        let app = d.c.view().expect_app();
        app.hud()
            .view(app.objects())
            .container_contents(CORPSE)
            .contains(&SWORD)
    };
    let no_number = d.number_on(SWORD).is_none();
    let survivor_keeps_its = d.number_on(ROCK) == Some(ROCK_TILE as u32);
    d.c.tick(8);
    let nothing_more = d.given_up() == vec![SWORD_TILE];

    d.c.assert_behaviour(
            "shortcut.bar.the-corpses-copy-of-a-dropped-thing-draws-no-number-and-asks-for-nothing-again",
            move |_| {
                drawn_before
                    && asked_once
                    && back_on_screen
                    && no_number
                    && survivor_keeps_its
                    && nothing_more
            },
        );
    d.c.shutdown();
}

/// The next login, with the shard's own answer, leaves that tile empty.
pub fn a_later_login_without_the_dropped_row_leaves_that_tile_empty() {
    let mut d = Death::new();
    d.unhanded();
    d.taken_away();
    d.c.tick(8);
    let asked = d.given_up() == vec![SWORD_TILE];

    d.describe(&[(ROCK_TILE, ROCK)]);
    let still_empty = d.tile_of(SWORD).is_none();
    let survivor = d.tile_of(ROCK) == Some(ROCK_TILE as usize);
    d.c.tick(4);
    let asked_nothing_more = d.given_up() == vec![SWORD_TILE];

    d.c.assert_behaviour(
        "shortcut.bar.a-later-login-without-the-dropped-row-leaves-that-tile-empty",
        move |_| asked && still_empty && survivor && asked_nothing_more,
    );
    d.c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_a_wielded_thing_that_dies_empties_its_tile_and_tells_the_shard_once => a_wielded_thing_that_dies_empties_its_tile_and_tells_the_shard_once ["shortcut.bar.a-wielded-thing-that-dies-with-the-player-empties-its-tile-and-tells-the-shard-once"],
    scenario_the_corpses_copy_draws_no_number_and_asks_for_nothing_again => the_corpses_copy_draws_no_number_and_asks_for_nothing_again ["shortcut.bar.the-corpses-copy-of-a-dropped-thing-draws-no-number-and-asks-for-nothing-again"],
    scenario_a_later_login_without_the_dropped_row_leaves_that_tile_empty => a_later_login_without_the_dropped_row_leaves_that_tile_empty ["shortcut.bar.a-later-login-without-the-dropped-row-leaves-that-tile-empty"],
}
