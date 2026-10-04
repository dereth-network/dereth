//! The barber window: an ordered `CharacterStartBarber 0x0075` opens the shipped barber modal with
//! the incoming appearance inverted into row choices, the authored rows, colour spots and shade bar
//! edit a private preview, heritage-specific options (no crown, no flame, earthbound) apply their
//! local effect, Apply sends one `Character_FinishBarber 0x0311` with the sixteen appearance fields
//! and never mutates the player optimistically, and Cancel closes the modal without sending.
//! Fixture: the retail dats (char-gen table, palette sets, string tables), a headless 800x600 App in
//! gameplay mode, and a socket-free replay endpoint that feeds the notices; no live barber is used.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::{app::App, config::Config, net::ClientNetwork};
use dereth_primitives::{AssetSource, DataId, LocalTime, ObjectId};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::Message;
use dereth_ui::framework::{DidMapperResolver, LayoutEnum, LayoutEnumResolver as _};
use dereth_ui::ElementId;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
use dereth_ui_screens::panels::barber::{APPLY, CANCEL, NEXT, OPTION1, PART_ROWS};
use dereth_ui_screens::screens::chargen::appearance::{COLOR_SPOTS, SHADE_SCROLL};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const BARBER: ElementId = dereth_ui_screens::panels::barber::PANEL;
const HAIR_ROW: ElementId = ElementId(0x1000_059E);
const EYES_ROW: ElementId = ElementId(0x1000_059F);
const SKIN_ROW: ElementId = ElementId(0x1000_05A2);

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19100", 7304, "barber", "unused", 0)
            .expect("a socket-free replay net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19100".parse().expect("addr")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
                stamp: 0,
            },
            net,
        )
    }

    fn event<M: Message>(&mut self, app: &mut App, message: &M) {
        self.stamp += 1;
        let blob = dereth_protocol::events::pack_event(PLAYER, self.stamp, message).expect("event");
        self.send(app, 9, blob);
    }

    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .expect("one fragment");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("datagram");
        app.replay_network_mut()
            .expect("replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("fed");
    }
}

fn setup() -> (App, Peer) {
    assert!(dereth_dat::testing::have_dats(), "retail dats are required");
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir().join("dereth-barber-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("app");
    app.start_shell().expect("shell");
    app.load_static_scene(dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..Default::default()
    })
    .expect("scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    settle(&mut app);

    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).expect("replay endpoint");
    let mut player = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    player.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    player.physicsdesc.setup_id = Some(0x0200_0001);
    player.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(player)).expect("player"),
    );
    app.frame();
    app.objects_mut().world.player = Some(PLAYER);
    app.objects_mut()
        .world
        .weenie_mut(PLAYER)
        .expect("created player")
        .pwd
        .name = "Larktest".into();

    let mut q = dereth_client_model::Qualities::new();
    q.ints = Some(
        [
            (dereth_client::hud::HERITAGE_GROUP, 1),
            (dereth_client::hud::GENDER, 1),
        ]
        .into(),
    );
    app.objects_mut().world.seed_player_desc(PLAYER, q);
    app.hud_mut().player_desc_received = true;
    settle(&mut app);
    let _ = actions_sent(&mut app);
    (app, peer)
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        app.frame();
    }
}

fn visible(app: &App, id: ElementId) -> bool {
    let ui = &app.ui().expect("shell").ui;
    let h = ui.get_element(id).expect("shipped element");
    ui.node(h).expect("live element").region.flags.visible
}

fn screen(app: &App) -> &GamePlayScreen {
    let any: &dyn std::any::Any = app.ui().expect("shell").flow.current().expect("screen");
    any.downcast_ref().expect("gameplay")
}

fn caption(app: &mut App, id: ElementId) -> String {
    let h = child(app, BARBER, id);
    let ui = &mut app.ui_mut().expect("shell").ui;
    ui.text_element_mut(h).map_or_else(String::new, |text| {
        String::from_utf16_lossy(
            &text
                .glyphs
                .glyphs
                .iter()
                .map(|glyph| glyph.data)
                .collect::<Vec<_>>(),
        )
    })
}

fn from_the_dat(app: &App, token: &str) -> String {
    app.ui()
        .expect("shell")
        .ui
        .resolve_string(
            dereth_ui_screens::screens::chargen::ERROR_STRING_TABLE,
            dereth_primitives::num::hash::str_hash(token.as_bytes()),
        )
        .unwrap_or_else(|| panic!("{token} is in the shipped string table"))
}

fn from_dat_table(app: &App, table: DataId, token: &str) -> String {
    app.ui()
        .expect("shell")
        .ui
        .resolve_string(
            table,
            dereth_primitives::num::hash::str_hash(token.as_bytes()),
        )
        .unwrap_or_else(|| panic!("{token} is in shipped table {table:?}"))
}

fn child(app: &App, parent: ElementId, id: ElementId) -> dereth_ui::ElemHandle {
    let ui = &app.ui().expect("shell").ui;
    let root = screen(app).root().expect("gameplay root");
    let parent = ui.get_child_recursive(root, parent).expect("parent");
    ui.get_child_recursive(parent, id).expect("child")
}

/// Opening the barber page binds all five ordinary rows. Hair, Eyes, and Skin are the three
/// captions resolved from string-table enum `0x10000002` with their fonts; Nose and Mouth retain
/// their authored layout captions. Each physical row remains selectable without erasing its text.
#[test]
fn opened_ordinary_barber_has_localized_captions_on_all_five_physical_rows() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(appearance_oracle().settings),
    );
    settle(&mut app);

    let expected = [
        from_the_dat(&app, "ID_CharGen_HairStyle"),
        from_the_dat(&app, "ID_CharGen_Eyes"),
        String::new(),
        String::new(),
        from_the_dat(&app, "ID_CharGen_Skin"),
    ];
    for ((row, _), expected) in PART_ROWS.into_iter().zip(expected) {
        let control = child(&app, BARBER, row);
        click_handle(&mut app, control);
        let actual = caption(&mut app, row);
        if expected.is_empty() {
            assert!(
                !actual.trim().is_empty(),
                "the authored {row:?} caption remains visible"
            );
        } else {
            assert_eq!(
                actual, expected,
                "the native localized {row:?} caption is visible"
            );
        }
    }
}

/// Heritage 5 is Umbraen/Shadowbound in the shipped CharGen table. Its only barber option is not
/// a wire boolean: the client maps the physical No Crown toggle to a sex-specific setup and PES,
/// runs that local player effect, then sends both wire options as literal zero.
#[test]
fn shadowbound_no_crown_runs_the_local_effect_then_sends_the_exact_finish() {
    use dereth_assets::Decode;

    const MALE_CROWN: u32 = 0x0200_196F;
    const MALE_NO_CROWN: u32 = 0x0200_1A5F;

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let mut player = dereth_client_model::Qualities::new();
    player.ints = Some(
        [
            (dereth_client::hud::HERITAGE_GROUP, 5),
            (dereth_client::hud::GENDER, 1),
        ]
        .into(),
    );
    app.objects_mut().world.seed_player_desc(PLAYER, player);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::movement::MovementPositionEvent {
            id: PLAYER,
            position: dereth_protocol::movement::PositionPack {
                origin: dereth_protocol::types::Origin {
                    objcell_id: 0xA9B4_0029,
                    origin: dereth_protocol::types::Vec3 {
                        x: 133.0,
                        y: 5.15,
                        z: 94.2,
                    },
                },
                orientation: dereth_protocol::types::Quat {
                    w: 1.0,
                    ..Default::default()
                },
                position_timestamp: 1,
                ..Default::default()
            },
        })
        .expect("placed-player F748"),
    );
    settle(&mut app);
    {
        let driver = app
            .world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver();
        assert!(
            driver.env.in_cell,
            "the local PES receiver is genuinely in a physics cell"
        );
        assert!(
            driver.assets.script(DataId(0x3300_12BB)).is_some(),
            "the exact male no-crown PES is registered in the local player's asset source"
        );
    }
    let oracle = appearance_oracle_for(5, 1, Some(MALE_CROWN));
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(oracle.settings),
    );
    settle(&mut app);

    let option = child(&app, BARBER, OPTION1);
    assert_eq!(
        screen(&app).barber.state.heritage_group,
        5,
        "the 0075 heritage reached the modal"
    );
    let ui = &app.ui().expect("shell").ui;
    assert!(
        ui.node(option).expect("option").region.flags.visible && ui.is_visible(option),
        "heritage 5 exposes its one authored option (state {:?}, local {}, effective {})",
        ui.node(option).expect("option").state,
        ui.node(option).expect("option").region.flags.visible,
        ui.is_visible(option),
    );
    assert_eq!(
        caption(&mut app, OPTION1),
        from_dat_table(&app, DataId(0x2300_0001), "ID_Barber_Shadow_NoCrown"),
        "the option caption comes from the retail string table"
    );
    assert_eq!(screen(&app).barber.view3d.setup, DataId(MALE_CROWN));

    click_handle(&mut app, option);
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(6),
        "the physical checkbox reaches its toggled state"
    );
    assert_eq!(
        screen(&app).barber.view3d.setup,
        DataId(MALE_NO_CROWN),
        "the private preview adopts the sex-specific no-crown setup"
    );
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "the option remains private before Apply"
    );

    let before_player = app.objects().world.player_objdesc.clone();
    let before_effects = app.world_scene().expect("scene").draw.stats.scripts_played;
    let store = dereth_dat::testing::open_store_or_fail();
    let resolver = DidMapperResolver::load_group(&store, 2).expect("UNIQUEDB mapper");
    let cg_id = resolver
        .resolve(LayoutEnum(0x0E))
        .expect("CharGen_CharacterData");
    let cg = dereth_assets::tables::CharGen::decode_payload(
        cg_id,
        &store.read(cg_id).expect("char-gen bytes"),
    )
    .expect("char-gen table");
    let sx = &cg.heritage_groups[&5].sexes[&1];
    let hair =
        &sx.hair_styles[usize::try_from(screen(&app).barber.state.hair_style).expect("hair")];
    let (_, head_object) = hair
        .objdesc
        .anim_part_changes
        .first()
        .copied()
        .expect("hair object");
    let (_, default_head_texture, head_texture) = hair
        .objdesc
        .texture_changes
        .first()
        .copied()
        .unwrap_or((0, DataId(0), DataId(0)));
    let eyes = &sx.eye_strips[usize::try_from(screen(&app).barber.state.eyes_strip).expect("eyes")];
    let eyes_desc = if hair.bald == 0 {
        &eyes.objdesc
    } else {
        &eyes.objdesc_bald
    };
    let (_, default_eyes_texture, eyes_texture) = eyes_desc.texture_changes[0];
    let (_, default_nose_texture, nose_texture) = sx.nose_strips
        [usize::try_from(screen(&app).barber.state.nose_strip).expect("nose")]
    .1
    .texture_changes[0];
    let (_, default_mouth_texture, mouth_texture) = sx.mouth_strips
        [usize::try_from(screen(&app).barber.state.mouth_strip).expect("mouth")]
    .1
    .texture_changes[0];
    let expected = vec![
        0x0311,
        sx.base_palette.0,
        head_object.0,
        head_texture.0,
        default_head_texture.0,
        eyes_texture.0,
        default_eyes_texture.0,
        nose_texture.0,
        default_nose_texture.0,
        mouth_texture.0,
        default_mouth_texture.0,
        oracle.settings.skin_palette,
        oracle.settings.hair_palette,
        oracle.settings.eyes_palette,
        MALE_NO_CROWN,
        0,
        0,
    ];

    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    assert_eq!(
        app.world_scene().expect("scene").draw.stats.scripts_played,
        before_effects + 1,
        "the sex/no-crown PES reaches the local player before the request leaves"
    );
    {
        let driver = app
            .world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver();
        assert!(
            !driver.scripts.is_empty() || !driver.particles.is_empty(),
            "the exact PES is queued or has materialized particles on the local player's driver"
        );
    }
    settle(&mut app);
    let finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(finish.len(), 1, "physical Apply sends one FinishBarber");
    assert_eq!(
        dwords(&finish[0]),
        expected,
        "the setup changes while both wire options stay zero"
    );
    assert_eq!(
        app.objects().world.player_objdesc,
        before_player,
        "Apply is not optimistic"
    );

    let authoritative = dereth_protocol::types::ObjDesc {
        anim_part_changes: vec![dereth_protocol::types::AnimPartChange {
            part_index: 0,
            part_id: head_object.0,
        }],
        ..Default::default()
    };
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemObjDescEvent {
            id: PLAYER,
            objdesc: authoritative.clone(),
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        })
        .expect("authoritative ObjDesc event"),
    );
    settle(&mut app);
    assert_eq!(
        app.objects()
            .presence(PLAYER)
            .expect("player presence")
            .objdesc,
        authoritative,
        "the authoritative appearance response still owns the world copy"
    );
    app.shutdown();
}

/// Heritage 10 follows the same one-checkbox UI shape as heritage 5, but owns a distinct set of
/// sex-specific setups. Its crown arm has a real PES; the no-crown PES ids are zero,
/// so Apply destroys the old particle manager without scheduling or counting a script for zero.
#[test]
fn penumbraen_female_crown_and_no_crown_take_their_exact_apply_paths() {
    const FEMALE_CROWN: u32 = 0x0200_196D;
    const FEMALE_NO_CROWN: u32 = 0x0200_1A5C;
    const FEMALE_CROWN_PES: u32 = 0x3300_1254;

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let mut player = dereth_client_model::Qualities::new();
    player.ints = Some(
        [
            (dereth_client::hud::HERITAGE_GROUP, 10),
            (dereth_client::hud::GENDER, 2),
        ]
        .into(),
    );
    app.objects_mut().world.seed_player_desc(PLAYER, player);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::movement::MovementPositionEvent {
            id: PLAYER,
            position: dereth_protocol::movement::PositionPack {
                origin: dereth_protocol::types::Origin {
                    objcell_id: 0xA9B4_0029,
                    origin: dereth_protocol::types::Vec3 {
                        x: 133.0,
                        y: 5.15,
                        z: 94.2,
                    },
                },
                orientation: dereth_protocol::types::Quat {
                    w: 1.0,
                    ..Default::default()
                },
                position_timestamp: 1,
                ..Default::default()
            },
        })
        .expect("placed-player F748"),
    );
    settle(&mut app);
    {
        let driver = app
            .world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver();
        assert!(
            driver.env.in_cell,
            "the local PES receiver is in a physics cell"
        );
        assert!(
            driver.assets.script(DataId(FEMALE_CROWN_PES)).is_some(),
            "the exact female crown PES is registered"
        );
    }

    let crown = appearance_oracle_for(10, 2, Some(FEMALE_CROWN));
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(crown.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    assert!(
        app.ui().expect("shell").ui.is_visible(option),
        "heritage 10 exposes option one"
    );
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(1),
        "the crown setup starts unchecked"
    );
    assert_eq!(screen(&app).barber.view3d.setup, DataId(FEMALE_CROWN));
    let crown_wire = finish_oracle(&app, &crown, Some(FEMALE_CROWN), 0);
    let before_crown = app.world_scene().expect("scene").draw.stats.scripts_played;
    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    assert_eq!(
        app.world_scene().expect("scene").draw.stats.scripts_played,
        before_crown + 1,
        "crown Apply plays its sex-specific local PES"
    );
    settle(&mut app);
    let crown_finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(crown_finish.len(), 1, "crown Apply sends one FinishBarber");
    assert_eq!(dwords(&crown_finish[0]), crown_wire);
    assert!(
        !app.world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver()
            .particles
            .is_empty(),
        "the actual crown PES materializes a local particle effect"
    );

    let no_crown = appearance_oracle_for(10, 2, Some(FEMALE_CROWN));
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(no_crown.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    click_handle(&mut app, option);
    assert_eq!(screen(&app).barber.view3d.setup, DataId(FEMALE_NO_CROWN));
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "the toggle remains private"
    );
    let no_crown_wire = finish_oracle(&app, &no_crown, Some(FEMALE_NO_CROWN), 0);
    let before_zero = app.world_scene().expect("scene").draw.stats.clone();
    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    let after_zero = &app.world_scene().expect("scene").draw.stats;
    assert_eq!(after_zero.scripts_played, before_zero.scripts_played);
    assert_eq!(after_zero.scripts_unplayed, before_zero.scripts_unplayed);
    assert!(
        app.world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver()
            .particles
            .is_empty(),
        "zero no-crown PES destroys the previous particle manager without queueing a replacement"
    );
    settle(&mut app);
    let no_crown_finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(
        no_crown_finish.len(),
        1,
        "no-crown Apply still sends FinishBarber"
    );
    assert_eq!(dwords(&no_crown_finish[0]), no_crown_wire);
    assert_eq!(
        &dwords(&no_crown_finish[0])[15..],
        &[0, 0],
        "both option fields are literal zero"
    );

    let authoritative = dereth_protocol::types::ObjDesc {
        anim_part_changes: vec![dereth_protocol::types::AnimPartChange {
            part_index: 0,
            part_id: no_crown_wire[2],
        }],
        ..Default::default()
    };
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemObjDescEvent {
            id: PLAYER,
            objdesc: authoritative.clone(),
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        })
        .expect("authoritative ObjDesc event"),
    );
    settle(&mut app);
    assert_eq!(
        app.objects()
            .presence(PLAYER)
            .expect("player presence")
            .objdesc,
        authoritative,
        "the authoritative appearance reply still owns the world copy"
    );
    app.shutdown();
}

/// Heritage 11's authored hair alternate setup selects skeleton versus zombie; its one checkbox
/// selects the matching flame/no-flame setup. Apply resets the local particle manager, plays the
/// sex-and-shape-specific flame PES when enabled, and carries the choice in setup rather than the
/// two trailing wire options.
#[test]
fn undead_female_zombie_flame_and_no_flame_take_their_exact_apply_paths() {
    const FEMALE_ZOMBIE: u32 = 0x0200_1AA1;
    const FEMALE_ZOMBIE_NO_FLAME: u32 = 0x0200_1AA2;
    const FEMALE_ZOMBIE_FLAME_PES: u32 = 0x3300_12D6;

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let mut player = dereth_client_model::Qualities::new();
    player.ints = Some(
        [
            (dereth_client::hud::HERITAGE_GROUP, 11),
            (dereth_client::hud::GENDER, 2),
        ]
        .into(),
    );
    app.objects_mut().world.seed_player_desc(PLAYER, player);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::movement::MovementPositionEvent {
            id: PLAYER,
            position: dereth_protocol::movement::PositionPack {
                origin: dereth_protocol::types::Origin {
                    objcell_id: 0xA9B4_0029,
                    origin: dereth_protocol::types::Vec3 {
                        x: 133.0,
                        y: 5.15,
                        z: 94.2,
                    },
                },
                orientation: dereth_protocol::types::Quat {
                    w: 1.0,
                    ..Default::default()
                },
                position_timestamp: 1,
                ..Default::default()
            },
        })
        .expect("placed-player F748"),
    );
    settle(&mut app);
    {
        let driver = app
            .world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver();
        assert!(
            driver.env.in_cell,
            "the local PES receiver is in a physics cell"
        );
        assert!(
            driver
                .assets
                .script(DataId(FEMALE_ZOMBIE_FLAME_PES))
                .is_some(),
            "the exact female zombie flame PES is registered"
        );
    }

    // The incoming no-flame setup selects the option. Physically clear it and Apply the generated
    // female zombie flame setup/PES.
    let no_flame = undead_zombie_oracle(2, true);
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(no_flame.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    assert!(
        app.ui().expect("shell").ui.is_visible(option),
        "heritage 11 exposes option one"
    );
    assert_eq!(
        caption(&mut app, OPTION1),
        from_dat_table(&app, DataId(0x2300_0001), "ID_Barber_Undead_NoFlame")
    );
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(6),
        "an incoming no-flame setup selects No Flame"
    );
    assert_eq!(
        screen(&app).barber.view3d.setup,
        DataId(FEMALE_ZOMBIE_NO_FLAME)
    );

    // Hair owns the skeleton/zombie split for Apply, but the common preview path leaves its
    // selected alternate setup latched until another option gesture.
    let hair_next = child(&app, HAIR_ROW, NEXT);
    click_handle(&mut app, hair_next);
    assert_eq!(
        screen(&app).barber.view3d.setup,
        DataId(FEMALE_ZOMBIE_NO_FLAME),
        "the common Hair/Next path leaves the selected special alternate setup latched"
    );

    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(no_flame.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    click_handle(&mut app, option);
    assert_eq!(screen(&app).barber.view3d.setup, DataId(FEMALE_ZOMBIE));
    let flame_wire = finish_oracle(&app, &no_flame, Some(FEMALE_ZOMBIE), 0);
    let before_flame = app.world_scene().expect("scene").draw.stats.scripts_played;
    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    assert_eq!(
        app.world_scene().expect("scene").draw.stats.scripts_played,
        before_flame + 1,
        "flame Apply plays the female zombie PES before transport settles"
    );
    settle(&mut app);
    let flame_finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(flame_finish.len(), 1);
    assert_eq!(dwords(&flame_finish[0]), flame_wire);
    assert_eq!(&dwords(&flame_finish[0])[15..], &[0, 0]);
    assert!(
        !app.world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver()
            .particles
            .is_empty(),
        "the actual flame PES materializes a local particle effect"
    );

    // Flame arrives unchecked. Select No Flame and prove its zero PES clears the effect
    // without recording a failed script before sending the no-flame setup.
    let flame = undead_zombie_oracle(2, false);
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(flame.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(1)
    );
    click_handle(&mut app, option);
    assert_eq!(
        screen(&app).barber.view3d.setup,
        DataId(FEMALE_ZOMBIE_NO_FLAME)
    );
    let no_flame_wire = finish_oracle(&app, &flame, Some(FEMALE_ZOMBIE_NO_FLAME), 0);
    let before_zero = app.world_scene().expect("scene").draw.stats.clone();
    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    let after_zero = &app.world_scene().expect("scene").draw.stats;
    assert_eq!(after_zero.scripts_played, before_zero.scripts_played);
    assert_eq!(after_zero.scripts_unplayed, before_zero.scripts_unplayed);
    assert!(
        app.world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver()
            .particles
            .is_empty(),
        "zero no-flame PES destroys the prior particle manager without queueing a replacement"
    );
    settle(&mut app);
    let no_flame_finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(no_flame_finish.len(), 1);
    assert_eq!(dwords(&no_flame_finish[0]), no_flame_wire);
    assert_eq!(&dwords(&no_flame_finish[0])[15..], &[0, 0]);

    let authoritative = dereth_protocol::types::ObjDesc {
        anim_part_changes: vec![dereth_protocol::types::AnimPartChange {
            part_index: 0,
            part_id: no_flame_wire[2],
        }],
        ..Default::default()
    };
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemObjDescEvent {
            id: PLAYER,
            objdesc: authoritative.clone(),
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        })
        .expect("authoritative ObjDesc event"),
    );
    settle(&mut app);
    assert_eq!(
        app.objects()
            .presence(PLAYER)
            .expect("player presence")
            .objdesc,
        authoritative,
        "the authoritative appearance reply still owns the world copy"
    );
    app.shutdown();
}

/// Heritage 9's checkbox is carried as wire option one, while its immediate local effect changes
/// the player's motion table: checked is earthbound/standing and unchecked is floating.
#[test]
fn empyrean_option_changes_the_local_motion_table_before_exact_finish() {
    const FEMALE_FLOAT: DataId = DataId(0x0900_020A);
    const FEMALE_STANDING: DataId = DataId(0x0900_020D);

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let mut player = dereth_client_model::Qualities::new();
    player.ints = Some(
        [
            (dereth_client::hud::HERITAGE_GROUP, 9),
            (dereth_client::hud::GENDER, 2),
        ]
        .into(),
    );
    app.objects_mut().world.seed_player_desc(PLAYER, player);
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::movement::MovementPositionEvent {
            id: PLAYER,
            position: dereth_protocol::movement::PositionPack {
                origin: dereth_protocol::types::Origin {
                    objcell_id: 0xA9B4_0029,
                    origin: dereth_protocol::types::Vec3 {
                        x: 133.0,
                        y: 5.15,
                        z: 94.2,
                    },
                },
                orientation: dereth_protocol::types::Quat {
                    w: 1.0,
                    ..Default::default()
                },
                position_timestamp: 1,
                ..Default::default()
            },
        })
        .expect("placed-player F748"),
    );
    settle(&mut app);
    {
        let driver = app
            .world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .driver();
        assert!(driver.assets.motion_table(FEMALE_FLOAT).is_some());
        assert!(driver.assets.motion_table(FEMALE_STANDING).is_some());
    }

    // A nonzero inbound option-one field selects Earthbound. Physically clear it and Apply the
    // female floating table with option one zero.
    let mut earthbound = appearance_oracle_for(9, 2, None);
    earthbound.settings.option1 = 1;
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(earthbound.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    assert!(
        app.ui().expect("shell").ui.is_visible(option),
        "heritage 9 exposes option one"
    );
    assert_eq!(
        caption(&mut app, OPTION1),
        from_dat_table(&app, DataId(0x2300_0001), "ID_Barber_Empyrean_Earthbound")
    );
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(6),
        "nonzero inbound option one selects Earthbound"
    );
    click_handle(&mut app, option);
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(1),
        "the physical option is now unchecked"
    );
    let floating_wire = finish_oracle(&app, &earthbound, None, 0);
    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    assert_eq!(
        app.world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .motion_table_id(),
        FEMALE_FLOAT,
        "unchecked Apply installs the female floating motion table before transport settles"
    );
    settle(&mut app);
    let floating_finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(floating_finish.len(), 1);
    assert_eq!(dwords(&floating_finish[0]), floating_wire);

    // Zero arrives unchecked. Physically select Earthbound and Apply the standing table with
    // option one exactly one.
    let floating = appearance_oracle_for(9, 2, None);
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(floating.settings),
    );
    settle(&mut app);
    let option = child(&app, BARBER, OPTION1);
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .node(option)
            .expect("option")
            .state,
        dereth_ui::StateId(1),
        "zero inbound option one clears Earthbound"
    );
    click_handle(&mut app, option);
    let standing_wire = finish_oracle(&app, &floating, None, 1);
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "the option remains private before Apply"
    );
    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    assert_eq!(
        app.world_state()
            .expect("scene")
            .character
            .as_ref()
            .expect("local player")
            .motion_table_id(),
        FEMALE_STANDING,
        "checked Apply installs the female standing motion table before transport settles"
    );
    settle(&mut app);
    let standing_finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(standing_finish.len(), 1);
    assert_eq!(dwords(&standing_finish[0]), standing_wire);
    assert_eq!(&dwords(&standing_finish[0])[15..], &[1, 0]);

    let authoritative = dereth_protocol::types::ObjDesc {
        anim_part_changes: vec![dereth_protocol::types::AnimPartChange {
            part_index: 0,
            part_id: standing_wire[2],
        }],
        ..Default::default()
    };
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemObjDescEvent {
            id: PLAYER,
            objdesc: authoritative.clone(),
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        })
        .expect("authoritative ObjDesc event"),
    );
    settle(&mut app);
    assert_eq!(
        app.objects()
            .presence(PLAYER)
            .expect("player presence")
            .objdesc,
        authoritative,
        "the authoritative appearance reply still owns the world copy"
    );
    app.shutdown();
}

/// Initialization checks both no-crown setup IDs for each of heritages 5 and 10; the incoming
/// setup does not have to agree with the gender before the checkbox is selected.
#[test]
fn both_special_heritages_accept_either_incoming_no_crown_setup_id() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    for (heritage, gender, incoming, expected) in [
        (5_u32, 1_i32, 0x0200_1A5E_u32, 0x0200_1A5F_u32),
        (5, 2, 0x0200_1A5F, 0x0200_1A5E),
        (10, 1, 0x0200_1A5C, 0x0200_1A5D),
        (10, 2, 0x0200_1A5D, 0x0200_1A5C),
    ] {
        let mut player = dereth_client_model::Qualities::new();
        player.ints = Some(
            [
                (
                    dereth_client::hud::HERITAGE_GROUP,
                    i32::try_from(heritage).expect("heritage"),
                ),
                (dereth_client::hud::GENDER, gender),
            ]
            .into(),
        );
        app.objects_mut().world.seed_player_desc(PLAYER, player);
        let oracle = appearance_oracle_for(
            heritage,
            u32::try_from(gender).expect("gender"),
            Some(incoming),
        );
        peer.event(
            &mut app,
            &dereth_protocol::trade::CharacterStartBarber(oracle.settings),
        );
        settle(&mut app);
        let option = child(&app, BARBER, OPTION1);
        assert!(app.ui().expect("shell").ui.is_visible(option));
        assert_eq!(
            app.ui().expect("shell").ui.node(option).expect("option").state,
            dereth_ui::StateId(6),
            "heritage {heritage} gender {gender} must accept opposite-sex no-crown ID {incoming:#010X}"
        );
        assert_eq!(
            screen(&app).barber.view3d.setup,
            DataId(expected),
            "generated preview setup remains sex-specific"
        );
    }
    app.shutdown();
}

/// Hair/Next changes the private hair choice, but does not rewrite the character-creation 3D view's latched
/// alternate setup. A selected No Crown preview therefore remains No Crown for both shadow
/// heritages while the changed preview continues to submit frames.
#[test]
fn shadow_no_crown_preview_survives_physical_hair_next() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    for (heritage, crown, no_crown) in [
        (5_u32, 0x0200_1970_u32, 0x0200_1A5E_u32),
        (10, 0x0200_196D, 0x0200_1A5C),
    ] {
        let mut player = dereth_client_model::Qualities::new();
        player.ints = Some(
            [
                (
                    dereth_client::hud::HERITAGE_GROUP,
                    i32::try_from(heritage).expect("heritage"),
                ),
                (dereth_client::hud::GENDER, 2),
            ]
            .into(),
        );
        app.objects_mut().world.seed_player_desc(PLAYER, player);
        let oracle = appearance_oracle_for(heritage, 2, Some(crown));
        peer.event(
            &mut app,
            &dereth_protocol::trade::CharacterStartBarber(oracle.settings),
        );
        settle(&mut app);

        let option = child(&app, BARBER, OPTION1);
        assert_eq!(
            app.ui()
                .expect("shell")
                .ui
                .node(option)
                .expect("option")
                .state,
            dereth_ui::StateId(1),
            "heritage {heritage} crown starts unchecked"
        );
        click_handle(&mut app, option);
        assert_eq!(screen(&app).barber.view3d.setup, DataId(no_crown));

        let before_style = screen(&app).barber.state.hair_style;
        let before_frames = app.renderer().ui_stats.previews_drawn;
        let hair_next = child(&app, HAIR_ROW, NEXT);
        click_handle(&mut app, hair_next);
        settle(&mut app);
        assert_ne!(screen(&app).barber.state.hair_style, before_style);
        assert!(
            app.renderer().ui_stats.previews_drawn > before_frames,
            "heritage {heritage} physically changed preview submits a later frame"
        );
        assert_eq!(
            screen(&app).barber.view3d.setup,
            DataId(no_crown),
            "heritage {heritage} Hair/Next preserves the selected No Crown preview setup"
        );
        assert_eq!(
            app.ui()
                .expect("shell")
                .ui
                .node(option)
                .expect("option")
                .state,
            dereth_ui::StateId(6),
            "heritage {heritage} No Crown checkbox stays selected"
        );
        assert!(!actions_sent(&mut app).contains(&0x0311));
    }
    app.shutdown();
}

fn finish_oracle(
    app: &App,
    oracle: &AppearanceOracle,
    setup: Option<u32>,
    option1: i32,
) -> Vec<u32> {
    use dereth_assets::Decode;

    let store = dereth_dat::testing::open_store_or_fail();
    let resolver = DidMapperResolver::load_group(&store, 2).expect("UNIQUEDB mapper");
    let cg_id = resolver
        .resolve(LayoutEnum(0x0E))
        .expect("CharGen_CharacterData");
    let cg = dereth_assets::tables::CharGen::decode_payload(
        cg_id,
        &store.read(cg_id).expect("char-gen bytes"),
    )
    .expect("char-gen table");
    let state = &screen(app).barber.state;
    let sx = &cg.heritage_groups[&state.heritage_group].sexes[&state.gender];
    let hair = &sx.hair_styles[usize::try_from(state.hair_style).expect("hair")];
    let (_, head_object) = hair
        .objdesc
        .anim_part_changes
        .first()
        .copied()
        .expect("hair object");
    let (_, default_head_texture, head_texture) = hair
        .objdesc
        .texture_changes
        .first()
        .copied()
        .unwrap_or((0, DataId(0), DataId(0)));
    let eyes = &sx.eye_strips[usize::try_from(state.eyes_strip).expect("eyes")];
    let eyes_desc = if hair.bald == 0 {
        &eyes.objdesc
    } else {
        &eyes.objdesc_bald
    };
    let (_, default_eyes_texture, eyes_texture) = eyes_desc.texture_changes[0];
    let (_, default_nose_texture, nose_texture) = sx.nose_strips
        [usize::try_from(state.nose_strip).expect("nose")]
    .1
    .texture_changes[0];
    let (_, default_mouth_texture, mouth_texture) = sx.mouth_strips
        [usize::try_from(state.mouth_strip).expect("mouth")]
    .1
    .texture_changes[0];
    let generated_setup = if hair.alternate_setup.0 == 0 {
        sx.setup
    } else {
        hair.alternate_setup
    };
    vec![
        0x0311,
        sx.base_palette.0,
        head_object.0,
        head_texture.0,
        default_head_texture.0,
        eyes_texture.0,
        default_eyes_texture.0,
        nose_texture.0,
        default_nose_texture.0,
        mouth_texture.0,
        default_mouth_texture.0,
        oracle.settings.skin_palette,
        oracle.settings.hair_palette,
        oracle.settings.eyes_palette,
        setup.unwrap_or(generated_setup.0),
        u32::try_from(option1).expect("nonnegative option one"),
        0,
    ]
}

fn click_handle(app: &mut App, h: dereth_ui::ElemHandle) {
    let at = {
        let ui = &app.ui().expect("shell").ui;
        let b = ui.screen_clip_box(h);
        assert!(
            b.is_valid(),
            "the shipped barber control has a visible clip box"
        );
        let center = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
        let at = if ui.hit_test_screen(center.0, center.1) == Some(h) {
            Some(center)
        } else {
            (b.y0..=b.y1)
                .flat_map(|y| (b.x0..=b.x1).map(move |x| (x, y)))
                .find(|(x, y)| ui.hit_test_screen(*x, *y) == Some(h))
        };
        let ancestry = |mut at: Option<dereth_ui::ElemHandle>| {
            let mut out = Vec::new();
            while let Some(handle) = at {
                let Some(node) = ui.node(handle) else { break };
                out.push((node.element_id(), node.region.z_level));
                at = ui.parent(handle);
            }
            out
        };
        at.unwrap_or_else(|| {
            panic!(
                "the shipped barber control at {b:?} has no direct-hit pixel; wanted={:?}, center={:?}",
                ancestry(Some(h)),
                ancestry(ui.hit_test_screen(center.0, center.1))
            )
        })
    };
    let ui = &mut app.ui_mut().expect("shell").ui;
    ui.mouse_move(LocalTime(1.0), at.0, at.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    app.frame();
}

fn drag_scrollbar_to_start(app: &mut App, bar: dereth_ui::ElemHandle) {
    let (start, end) = {
        let ui = &app.ui().expect("shell").ui;
        let thumb = ui
            .get_child(bar, ElementId(1))
            .expect("the scrollbar thumb");
        let thumb_box = ui.screen_clip_box(thumb);
        let bar_box = ui.screen_clip_box(bar);
        assert!(
            thumb_box.is_valid() && bar_box.is_valid(),
            "the visible authored shade control"
        );
        let start = (
            (thumb_box.x0 + thumb_box.x1) / 2,
            (thumb_box.y0 + thumb_box.y1) / 2,
        );
        let end = if bar_box.width() >= bar_box.height() {
            (bar_box.x0 - 100, start.1)
        } else {
            (start.0, bar_box.y0 - 100)
        };
        (start, end)
    };
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        ui.mouse_move(LocalTime(2.0), start.0, start.1);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, start.0, start.1);
    }
    app.frame();
    app.ui_mut()
        .expect("shell")
        .ui
        .mouse_move(LocalTime(2.1), end.0, end.1);
    app.frame();
    app.ui_mut().expect("shell").ui.mouse_up(
        dereth_ui::focus::action::PRIMARY_CLICK,
        end.0,
        end.1,
        false,
    );
    app.frame();
}

fn actions_sent(app: &mut App) -> Vec<u32> {
    game_actions(app)
        .into_iter()
        .filter_map(|body| body.get(..4)?.try_into().ok().map(u32::from_le_bytes))
        .collect()
}

fn game_actions(app: &mut App) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for (bytes, _) in app.replay_network_mut().expect("endpoint").take_outgoing() {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for fragment in packet.fragments {
            let body = fragment.payload;
            if fragment.header.queue_id == 3
                && body.len() >= 12
                && u32::from_le_bytes(body[0..4].try_into().expect("four bytes")) == 0xF7B1
            {
                out.push(body[8..].to_vec());
            }
        }
    }
    out
}

fn dwords(bytes: &[u8]) -> Vec<u32> {
    assert_eq!(
        bytes.len() % 4,
        0,
        "the ordered action body is dword aligned"
    );
    bytes
        .chunks_exact(4)
        .map(|b| u32::from_le_bytes(b.try_into().expect("four bytes")))
        .collect()
}

fn changed_in(a: &[u8], b: &[u8], width: u32, box_: dereth_ui::Box2D) -> usize {
    let width = usize::try_from(width).expect("frame width");
    (box_.y0..box_.y1)
        .flat_map(|y| (box_.x0..box_.x1).map(move |x| (x, y)))
        .filter(|(x, y)| {
            let i = (usize::try_from(*y).expect("screen y") * width
                + usize::try_from(*x).expect("screen x"))
                * 4;
            a.get(i..i + 4) != b.get(i..i + 4)
        })
        .count()
}

struct AppearanceOracle {
    settings: dereth_protocol::trade::BarberSettings,
    hair: i32,
    eyes: i32,
    nose: i32,
    mouth: i32,
    skin_shade: f64,
    hair_color: i32,
    hair_shade: f64,
    eye_color: i32,
    skin_palettes: Vec<u32>,
    hair_palettes: Vec<Vec<u32>>,
    eye_palettes: Vec<u32>,
}

fn appearance_oracle() -> AppearanceOracle {
    appearance_oracle_for(1, 1, None)
}

fn appearance_oracle_for(heritage: u32, gender: u32, setup: Option<u32>) -> AppearanceOracle {
    use dereth_assets::Decode;

    let store = dereth_dat::testing::open_store_or_fail();
    let resolver = DidMapperResolver::load_group(&store, 2).expect("UNIQUEDB mapper");
    let cg_id = resolver
        .resolve(LayoutEnum(0x0E))
        .expect("CharGen_CharacterData");
    let cg = dereth_assets::tables::CharGen::decode_payload(
        cg_id,
        &store.read(cg_id).expect("char-gen bytes"),
    )
    .expect("char-gen table");
    let sx = &cg.heritage_groups.get(&heritage).expect("heritage").sexes[&gender];

    let (hair, hair_style) = sx
        .hair_styles
        .iter()
        .enumerate()
        .skip(1)
        .find(|(_, h)| !h.objdesc.anim_part_changes.is_empty())
        .expect("a non-first hair style with a part change");
    let (eyes, eye_strip) = sx
        .eye_strips
        .iter()
        .enumerate()
        .find(|(_, e)| !e.objdesc.texture_changes.is_empty())
        .expect("an eye strip texture");
    let (nose, nose_strip) = sx
        .nose_strips
        .iter()
        .enumerate()
        .find(|(_, (_, d))| !d.texture_changes.is_empty())
        .expect("a nose strip texture");
    let (mouth, mouth_strip) = sx
        .mouth_strips
        .iter()
        .enumerate()
        .find(|(_, (_, d))| !d.texture_changes.is_empty())
        .expect("a mouth strip texture");

    let palettes = |id: dereth_primitives::DataId| {
        dereth_assets::material::PaletteSet::decode_payload(
            id,
            &store.read(id).expect("palette-set bytes"),
        )
        .expect("palette set")
        .palette_ids
    };
    let skin = palettes(sx.skin_palset);
    let skin_index = usize::from(skin.len() > 1);
    let hair_color = sx
        .hair_colors
        .iter()
        .enumerate()
        .find_map(|(i, id)| {
            let p = palettes(dereth_primitives::DataId(*id));
            (!p.is_empty()).then_some((i, p))
        })
        .expect("a hair palette set");
    let hair_palette_index = hair_color.1.len() - 1;
    let eye_color = usize::from(sx.eye_colors.len() > 1);

    AppearanceOracle {
        settings: dereth_protocol::trade::BarberSettings {
            base_palette: sx.base_palette.0,
            head_object: hair_style.objdesc.anim_part_changes[0].1 .0,
            head_texture: 0,
            default_head_texture: 0,
            eyes_texture: eye_strip.objdesc.texture_changes[0].2 .0,
            default_eyes_texture: 0,
            nose_texture: nose_strip.1.texture_changes[0].2 .0,
            default_nose_texture: 0,
            mouth_texture: mouth_strip.1.texture_changes[0].2 .0,
            default_mouth_texture: 0,
            skin_palette: skin[skin_index].0,
            hair_palette: hair_color.1[hair_palette_index].0,
            eyes_palette: sx.eye_colors[eye_color],
            setup_id: setup.unwrap_or(sx.setup.0),
            option1: 0,
            option2: 0,
        },
        hair: i32::try_from(hair).expect("hair index"),
        eyes: i32::try_from(eyes).expect("eyes index"),
        nose: i32::try_from(nose).expect("nose index"),
        mouth: i32::try_from(mouth).expect("mouth index"),
        skin_shade: f64::from(u32::try_from(skin_index + 1).expect("skin index"))
            / f64::from(u32::try_from(skin.len()).expect("skin count")),
        hair_color: i32::try_from(hair_color.0).expect("hair color"),
        hair_shade: f64::from(u32::try_from(hair_palette_index + 1).expect("hair index"))
            / f64::from(u32::try_from(hair_color.1.len()).expect("hair count")),
        eye_color: i32::try_from(eye_color).expect("eye color"),
        skin_palettes: skin.into_iter().map(|id| id.0).collect(),
        hair_palettes: sx
            .hair_colors
            .iter()
            .map(|id| {
                palettes(dereth_primitives::DataId(*id))
                    .into_iter()
                    .map(|id| id.0)
                    .collect()
            })
            .collect(),
        eye_palettes: sx.eye_colors.clone(),
    }
}

fn undead_zombie_oracle(gender: u32, no_flame: bool) -> AppearanceOracle {
    use dereth_assets::Decode;

    let (zombie, setup) = match (gender == 2, no_flame) {
        (false, false) => (DataId(0x0200_1A9D), 0x0200_1A9D),
        (false, true) => (DataId(0x0200_1A9D), 0x0200_1A96),
        (true, false) => (DataId(0x0200_1AA1), 0x0200_1AA1),
        (true, true) => (DataId(0x0200_1AA1), 0x0200_1AA2),
    };
    let mut oracle = appearance_oracle_for(11, gender, Some(setup));
    let store = dereth_dat::testing::open_store_or_fail();
    let resolver = DidMapperResolver::load_group(&store, 2).expect("UNIQUEDB mapper");
    let cg_id = resolver
        .resolve(LayoutEnum(0x0E))
        .expect("CharGen_CharacterData");
    let cg = dereth_assets::tables::CharGen::decode_payload(
        cg_id,
        &store.read(cg_id).expect("char-gen bytes"),
    )
    .expect("char-gen table");
    let sx = &cg.heritage_groups[&11].sexes[&gender];
    let (hair, style) = sx
        .hair_styles
        .iter()
        .enumerate()
        .find(|(_, hair)| hair.alternate_setup == zombie)
        .expect("a shipped zombie hair style");
    oracle.hair = i32::try_from(hair).expect("hair index");
    oracle.settings.head_object = style
        .objdesc
        .anim_part_changes
        .first()
        .expect("zombie hair object")
        .1
         .0;
    oracle
}

/// The ordered start notice reads sixteen appearance fields at offsets 4 through 0x40, then
/// initializes and raises this exact shipped window.
#[test]
fn ordered_start_opens_an_editable_preview_and_cancel_discards_it() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    assert!(
        !visible(&app, BARBER),
        "the notice-driven modal starts hidden"
    );

    // `RemainingPanels` hands `SpeechBubblePanel::post_init` the whole gameplay root. Prove the
    // two-node mouse reset resolves child 0x10000046 first and cannot mutate that caller root.
    let gameplay_root = screen(&app).root().expect("gameplay root");
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        let original_root_mouse = ui
            .node(gameplay_root)
            .expect("gameplay root")
            .should_be_mouse_visible;
        ui.set_mouse_visible(gameplay_root, true);
        let mut probe = dereth_ui_screens::hud::speech_bubbles::SpeechBubblePanel::default();
        probe.post_init(ui, gameplay_root);
        assert!(
            ui.node(gameplay_root).expect("gameplay root").should_be_mouse_visible,
            "message-window post-initialization must not clear mouse visibility on the gameplay screen"
        );
        ui.set_mouse_visible(gameplay_root, original_root_mouse);
    }

    // Message-window initialization clears mouse visibility on its empty panel and list. The strip
    // overlaps Hair Next, so measure the pointer falling through to the game view before the barber
    // is shown.
    let hidden_hair_next = child(&app, HAIR_ROW, NEXT);
    let hair_box = app
        .ui()
        .expect("shell")
        .ui
        .screen_clip_box(hidden_hair_next);
    let hair_point = (
        (hair_box.x0 + hair_box.x1) / 2,
        (hair_box.y0 + hair_box.y1) / 2,
    );
    let sbox = app
        .ui()
        .expect("shell")
        .ui
        .get_element(dereth_ui_screens::hud::world_view::SMART_BOX)
        .expect("shipped smart box");
    assert_eq!(
        app.ui()
            .expect("shell")
            .ui
            .hit_test_screen(hair_point.0, hair_point.1),
        Some(sbox),
        "an empty spew strip passes a real pointer through to the ordinary game view"
    );

    let oracle = appearance_oracle();
    let start = dereth_protocol::trade::CharacterStartBarber(oracle.settings);
    peer.event(&mut app, &start);
    settle(&mut app);
    assert!(
        visible(&app, BARBER),
        "the actual 0x0075 producer must raise the barber panel"
    );

    let state = &screen(&app).barber.state;
    assert_eq!(
        (
            state.hair_style,
            state.eyes_strip,
            state.nose_strip,
            state.mouth_strip
        ),
        (oracle.hair, oracle.eyes, oracle.nose, oracle.mouth),
        "Start must invert the incoming object/texture IDs instead of showing random defaults"
    );
    assert_eq!(
        (state.hair_color, state.eye_color),
        (oracle.hair_color, oracle.eye_color)
    );
    assert_eq!(
        state.skin_shade, oracle.skin_shade,
        "incoming skin palette -> native shade"
    );
    assert_eq!(
        state.hair_shade, oracle.hair_shade,
        "incoming hair palette -> native shade"
    );

    let before_style = screen(&app).barber.state.hair_style;
    let before = app.renderer().ui_stats.previews_drawn;
    let hair_next = child(&app, HAIR_ROW, NEXT);
    click_handle(&mut app, hair_next);
    settle(&mut app);
    assert_ne!(
        screen(&app).barber.state.hair_style,
        before_style,
        "the Hair row's own shared Next child must change the private barber model"
    );
    assert!(
        app.renderer().ui_stats.previews_drawn > before,
        "the barber viewport remains active and submits preview frames after the edit"
    );
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "preview edits are local until Apply"
    );

    let cancel = child(&app, BARBER, CANCEL);
    click_handle(&mut app, cancel);
    settle(&mut app);
    assert!(!visible(&app, BARBER), "Cancel destroys the modal");
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "Cancel must not send FinishBarber"
    );
    app.shutdown();
}

/// Behaviour: panels.barber.apply-sends-one-exact-finish-without-mutating-the-player
///
/// Clicking Apply builds the sixteen appearance fields, then sends them in the literal order
/// asserted below. This oracle builds the expected values directly from the retail character-
/// generation rows; it does not use `BarberSettings`'s encoder or the production panel's generator.
#[test]
fn physical_apply_sends_one_exact_finish_without_optimistic_player_mutation() {
    use dereth_assets::Decode;

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let oracle = appearance_oracle();
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(oracle.settings),
    );
    settle(&mut app);

    let before_player = app.objects().world.player_objdesc.as_ref().cloned();
    let before_presence = app
        .objects()
        .presence(PLAYER)
        .expect("player presence")
        .objdesc
        .clone();
    let hair_next = child(&app, HAIR_ROW, NEXT);
    click_handle(&mut app, hair_next);
    settle(&mut app);
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "the private Hair edit must not send before the authored Apply button"
    );

    let store = dereth_dat::testing::open_store_or_fail();
    let resolver = DidMapperResolver::load_group(&store, 2).expect("UNIQUEDB mapper");
    let cg_id = resolver
        .resolve(LayoutEnum(0x0E))
        .expect("CharGen_CharacterData");
    let cg = dereth_assets::tables::CharGen::decode_payload(
        cg_id,
        &store.read(cg_id).expect("char-gen bytes"),
    )
    .expect("char-gen table");
    let sx = &cg.heritage_groups[&1].sexes[&1];
    let next_hair = &sx.hair_styles
        [usize::try_from(screen(&app).barber.state.hair_style).expect("selected hair")];
    let (_, head_object) = next_hair
        .objdesc
        .anim_part_changes
        .first()
        .copied()
        .expect("selected hair object");
    let (_, default_head_texture, head_texture) = next_hair
        .objdesc
        .texture_changes
        .first()
        .copied()
        .unwrap_or((
            0,
            dereth_primitives::DataId(0),
            dereth_primitives::DataId(0),
        ));
    let eyes_row = &sx.eye_strips[usize::try_from(oracle.eyes).expect("eyes")];
    let eyes_desc = if next_hair.bald == 0 {
        &eyes_row.objdesc
    } else {
        &eyes_row.objdesc_bald
    };
    let (_, default_eyes_texture, eyes_texture) = eyes_desc
        .texture_changes
        .first()
        .copied()
        .expect("selected eyes texture");
    let (_, default_nose_texture, nose_texture) = sx.nose_strips
        [usize::try_from(oracle.nose).expect("nose")]
    .1
    .texture_changes[0];
    let (_, default_mouth_texture, mouth_texture) = sx.mouth_strips
        [usize::try_from(oracle.mouth).expect("mouth")]
    .1
    .texture_changes[0];
    let setup = if next_hair.alternate_setup.0 == 0 {
        sx.setup
    } else {
        next_hair.alternate_setup
    };
    let expected = vec![
        0x0311,
        sx.base_palette.0,
        head_object.0,
        head_texture.0,
        default_head_texture.0,
        eyes_texture.0,
        default_eyes_texture.0,
        nose_texture.0,
        default_nose_texture.0,
        mouth_texture.0,
        default_mouth_texture.0,
        oracle.settings.skin_palette,
        oracle.settings.hair_palette,
        oracle.settings.eyes_palette,
        setup.0,
        0,
        0,
    ];

    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    settle(&mut app);
    let finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(
        finish.len(),
        1,
        "Apply emits exactly one literal 0x0311 action"
    );
    assert_eq!(
        dwords(&finish[0]),
        expected,
        "all sixteen native fields and their order"
    );
    assert!(
        !visible(&app, BARBER),
        "native Apply closes the barber modal"
    );
    assert_eq!(
        app.objects().world.player_objdesc.as_ref(),
        before_player.as_ref(),
        "the local request must not optimistically mutate the authoritative player appearance"
    );
    assert_eq!(
        &app.objects()
            .presence(PLAYER)
            .expect("player presence")
            .objdesc,
        &before_presence,
        "the ordered request must not optimistically mutate the authoritative object stream"
    );

    // The ordinary authoritative route is still `0xF625 Item_ObjDescEvent`, not `0x0311`'s local
    // preview. A valid server reply must therefore be the first thing that changes the world copy.
    let authoritative = dereth_protocol::types::ObjDesc {
        anim_part_changes: vec![dereth_protocol::types::AnimPartChange {
            part_index: 0,
            part_id: head_object.0,
        }],
        ..Default::default()
    };
    let authoritative_blob =
        dereth_protocol::write_blob(&dereth_protocol::objects::ItemObjDescEvent {
            id: PLAYER,
            objdesc: authoritative.clone(),
            timestamps: dereth_protocol::types::PhysicsEventStamp {
                instance: 1,
                event: 1,
            },
        })
        .expect("authoritative ObjDesc event");
    let decoded = dereth_protocol::read_body_padded::<dereth_protocol::objects::ItemObjDescEvent>(
        &authoritative_blob[4..],
    )
    .expect("the independent authoritative marker round-trips before replay");
    assert_eq!(
        decoded.objdesc, authoritative,
        "the F625 marker survives ObjDesc packing"
    );
    peer.send(&mut app, 10, authoritative_blob);
    settle(&mut app);
    assert_eq!(
        &app.objects()
            .presence(PLAYER)
            .expect("player presence")
            .objdesc,
        &authoritative,
        "the existing authoritative appearance reply reaches the player presence after Apply"
    );
    app.shutdown();
}

/// The shared authored colour spots and shade scrollbar route through the currently selected
/// Hair/Eyes/Skin row. Colour selection changes only hair or eye colour, while shade selection
/// changes hair shade or the shared skin shade. The final palette IDs below come directly from the
/// retail palette sets, not from the production panel's shade resolver.
#[test]
fn physical_palette_controls_send_the_selected_native_palettes_and_preserve_other_fields() {
    use dereth_assets::Decode;

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let oracle = appearance_oracle();
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(oracle.settings),
    );
    settle(&mut app);

    let inbound_hair = usize::try_from(oracle.hair_color).expect("inbound hair colour");
    let (hair_color, hair_palettes) = oracle
        .hair_palettes
        .iter()
        .enumerate()
        .take(COLOR_SPOTS.len())
        .find(|(i, palettes)| *i != inbound_hair && !palettes.is_empty())
        .expect("a second authored hair colour");
    let eye_color = oracle
        .eye_palettes
        .iter()
        .enumerate()
        .take(COLOR_SPOTS.len())
        .find(|(i, _)| i32::try_from(*i).ok() != Some(oracle.eye_color))
        .map(|(i, _)| i)
        .expect("a second authored eye colour");

    // Page initialization selects Hair. The real colour button and real scrollbar thumb must therefore
    // edit the private preview without any network request.
    let hair_spot = child(&app, BARBER, COLOR_SPOTS[hair_color]);
    click_handle(&mut app, hair_spot);
    let shade = child(&app, BARBER, SHADE_SCROLL);
    drag_scrollbar_to_start(&mut app, shade);
    assert_eq!(
        (screen(&app).barber.state.hair_color, screen(&app).barber.state.hair_shade),
        (i32::try_from(hair_color).expect("hair index"), 0.0),
        "Hair selection maps spot ordinal to a hair palette set and scrollbar position to its shade"
    );

    // The same spot ordinal means an eye Palette after the actual Eyes row becomes current; Eyes
    // deliberately hides and ignores the shade control.
    let eyes_row = child(&app, BARBER, EYES_ROW);
    click_handle(&mut app, eyes_row);
    let eye_spot = child(&app, BARBER, COLOR_SPOTS[eye_color]);
    click_handle(&mut app, eye_spot);
    assert_eq!(
        screen(&app).barber.state.eye_color,
        i32::try_from(eye_color).expect("eye index"),
        "Eyes selection maps the authored spot ordinal to the eye palette list"
    );

    // Skin owns one colour spot but shares the shade destination with Nose and Mouth. Drag its
    // real thumb independently of Hair so Apply must resolve the skin PalSet at index zero.
    let skin_row = child(&app, BARBER, SKIN_ROW);
    click_handle(&mut app, skin_row);
    drag_scrollbar_to_start(&mut app, shade);
    assert_eq!(
        screen(&app).barber.state.skin_shade,
        0.0,
        "Skin owns the shared skin shade"
    );
    assert!(
        !actions_sent(&mut app).contains(&0x0311),
        "palette edits remain private until Apply"
    );

    let apply = child(&app, BARBER, APPLY);
    click_handle(&mut app, apply);
    settle(&mut app);
    let finish: Vec<_> = game_actions(&mut app)
        .into_iter()
        .filter(|body| body.get(..4) == Some(0x0311_u32.to_le_bytes().as_slice()))
        .collect();
    assert_eq!(
        finish.len(),
        1,
        "the physical Apply sends exactly one FinishBarber"
    );
    // The notice fixture uses zero for old/default textures because Start inverts only its new
    // IDs. Base-appearance generation, however, regenerates both halves from the still-selected
    // retail rows. Read those rows independently, as the other Finish oracles do.
    let store = dereth_dat::testing::open_store_or_fail();
    let resolver = DidMapperResolver::load_group(&store, 2).expect("UNIQUEDB mapper");
    let cg_id = resolver
        .resolve(LayoutEnum(0x0E))
        .expect("CharGen_CharacterData");
    let cg = dereth_assets::tables::CharGen::decode_payload(
        cg_id,
        &store.read(cg_id).expect("char-gen bytes"),
    )
    .expect("char-gen table");
    let sx = &cg.heritage_groups[&1].sexes[&1];
    let hair = &sx.hair_styles[usize::try_from(oracle.hair).expect("hair")];
    let (_, head_object) = hair.objdesc.anim_part_changes[0];
    let (_, default_head_texture, head_texture) =
        hair.objdesc.texture_changes.first().copied().unwrap_or((
            0,
            dereth_primitives::DataId(0),
            dereth_primitives::DataId(0),
        ));
    let eye_row = &sx.eye_strips[usize::try_from(oracle.eyes).expect("eyes")];
    let eye_desc = if hair.bald == 0 {
        &eye_row.objdesc
    } else {
        &eye_row.objdesc_bald
    };
    let (_, default_eyes_texture, eyes_texture) = eye_desc.texture_changes[0];
    let (_, default_nose_texture, nose_texture) = sx.nose_strips
        [usize::try_from(oracle.nose).expect("nose")]
    .1
    .texture_changes[0];
    let (_, default_mouth_texture, mouth_texture) = sx.mouth_strips
        [usize::try_from(oracle.mouth).expect("mouth")]
    .1
    .texture_changes[0];
    let setup = if hair.alternate_setup.0 == 0 {
        sx.setup
    } else {
        hair.alternate_setup
    };
    let expected = vec![
        0x0311,
        sx.base_palette.0,
        head_object.0,
        head_texture.0,
        default_head_texture.0,
        eyes_texture.0,
        default_eyes_texture.0,
        nose_texture.0,
        default_nose_texture.0,
        mouth_texture.0,
        default_mouth_texture.0,
        *oracle.skin_palettes.first().expect("skin palette zero"),
        *hair_palettes.first().expect("hair palette zero"),
        oracle.eye_palettes[eye_color],
        setup.0,
        0,
        0,
    ];
    assert_eq!(
        dwords(&finish[0]),
        expected,
        "only the selected native palette IDs change; all other appearance fields are preserved"
    );
    app.shutdown();
}

/// Colour-spot drawing creates recoloured bullet surfaces from palette-set channel averages (or
/// the eye palette's exact entry `0x103`) and puts `ColorEmpty` in every unused slot. Gradient-disk
/// drawing multiplies the ring by the selected colour, except Eyes uses the flat `GradientPlug`.
/// The different authored selections must change actual presented pixels.
#[test]
fn opened_barber_draws_native_colour_spots_empty_slots_and_gradient_modes() {
    use dereth_ui::region::SurfaceOp;
    use dereth_ui_screens::screens::chargen::PaletteSample;

    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let oracle = appearance_oracle();
    peer.event(
        &mut app,
        &dereth_protocol::trade::CharacterStartBarber(oracle.settings),
    );
    settle(&mut app);

    let tables = screen(&app).barber.tables().expect("barber tables");
    let colors = tables.colors.as_ref().expect("retail palette source");
    let state = &screen(&app).barber.state;
    let sx = state
        .sex(&tables.chargen)
        .expect("ordinary Aluvian sex row");
    let hair_color = usize::try_from(state.hair_color).expect("hair colour");
    let hair_rgb = colors
        .pal_set_color(DataId(sx.hair_colors[hair_color]), PaletteSample::Hair)
        .expect("Hair palette set channel mean");
    let spot0 = child(&app, BARBER, COLOR_SPOTS[0]);
    let grad = child(
        &app,
        BARBER,
        dereth_ui_screens::screens::chargen::appearance::GRAD_CIRCLE,
    );
    let (spot_box, grad_box) = {
        let ui = &app.ui().expect("shell").ui;
        let image = ui
            .node(spot0)
            .expect("spot")
            .region
            .image
            .as_ref()
            .expect("Hair spot image");
        assert_eq!(
            image.did, tables.color_wheel_art.bullet,
            "Hair uses ColorBullet"
        );
        assert_eq!(
            image.op,
            Some(SurfaceOp::ReplaceColor {
                from: SurfaceOp::OPAQUE_BLACK,
                to: hair_rgb
            }),
            "Hair spot is the exact averaged palette set colour"
        );
        let image = ui
            .node(grad)
            .expect("gradient")
            .region
            .image
            .as_ref()
            .expect("Hair ring image");
        assert_eq!(
            image.did, tables.color_wheel_art.ring,
            "Hair uses ColorRing"
        );
        assert_eq!(
            image.op,
            Some(SurfaceOp::Multiply(hair_rgb)),
            "the ring uses Hair's tint"
        );
        (ui.screen_clip_box(spot0), ui.screen_clip_box(grad))
    };
    let (w, _, hair_pixels) = app.renderer_mut().capture_bgra().expect("Hair wheel frame");

    let eyes_row = child(&app, BARBER, EYES_ROW);
    click_handle(&mut app, eyes_row);
    settle(&mut app);
    let eye_rgb = colors
        .palette_color(DataId(sx.eye_colors[0]), PaletteSample::Eyes)
        .expect("Eye colour spot zero's Palette entry 0x103");
    {
        let ui = &app.ui().expect("shell").ui;
        let image = ui
            .node(spot0)
            .expect("spot")
            .region
            .image
            .as_ref()
            .expect("Eye spot image");
        assert_eq!(
            image.op,
            Some(SurfaceOp::ReplaceColor {
                from: SurfaceOp::OPAQUE_BLACK,
                to: eye_rgb
            }),
            "Eyes uses the exact Palette entry rather than a palette set average"
        );
        let image = ui
            .node(grad)
            .expect("gradient")
            .region
            .image
            .as_ref()
            .expect("eye plug image");
        assert_eq!(
            image.did, tables.color_wheel_art.plug,
            "Eyes replaces the ring with GradientPlug"
        );
        assert_eq!(image.op, None, "the eye plug is not tinted");
    }
    let (_, _, eye_pixels) = app.renderer_mut().capture_bgra().expect("Eye wheel frame");
    assert!(
        changed_in(&hair_pixels, &eye_pixels, w, grad_box) > 20,
        "Hair's tinted ring and Eyes' flat plug must differ in presented pixels"
    );

    let skin_row = child(&app, BARBER, SKIN_ROW);
    click_handle(&mut app, skin_row);
    settle(&mut app);
    let skin_rgb = colors
        .pal_set_color(sx.skin_palset, PaletteSample::Skin)
        .expect("skin palette set channel mean");
    let unused = child(&app, BARBER, COLOR_SPOTS[1]);
    let unused_box = {
        let ui = &app.ui().expect("shell").ui;
        let image = ui
            .node(spot0)
            .expect("spot")
            .region
            .image
            .as_ref()
            .expect("Skin spot image");
        assert_eq!(
            image.op,
            Some(SurfaceOp::ReplaceColor {
                from: SurfaceOp::OPAQUE_BLACK,
                to: skin_rgb
            }),
            "Skin's one spot uses its palette set average"
        );
        let image = ui
            .node(unused)
            .expect("unused spot")
            .region
            .image
            .as_ref()
            .expect("empty image");
        assert_eq!(
            image.did, tables.color_wheel_art.empty,
            "Skin slots one through eight use ColorEmpty"
        );
        assert_eq!(
            image.op, None,
            "ColorEmpty is copied without a palette operation"
        );
        ui.screen_clip_box(unused)
    };
    let (_, _, skin_pixels) = app.renderer_mut().capture_bgra().expect("Skin wheel frame");
    assert!(
        changed_in(&hair_pixels, &skin_pixels, w, spot_box) > 10,
        "Hair and Skin generated spot colours must differ in presented pixels"
    );
    app.ui_mut().expect("shell").ui.set_visible(unused, false);
    app.frame();
    let (_, _, without_empty) = app
        .renderer_mut()
        .capture_bgra()
        .expect("empty slot hidden");
    assert!(
        changed_in(&skin_pixels, &without_empty, w, unused_box) > 10,
        "ColorEmpty itself must reach visible output, not only the retained surface recipe"
    );
    app.shutdown();
}
