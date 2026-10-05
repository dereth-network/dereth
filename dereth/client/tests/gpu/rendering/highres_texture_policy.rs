//! The high-res dat is a server grant and the high-res texture level is a preference. The client
//! opens `client_highres.dat` only when bit `0x4` of the `0xF7E5` interrogation's product id is
//! set (ACE sends `0x1` unless `allow_highres_dat` is enabled,
//! `ACE.Server/Managers/PropertyManager.cs:508`), and even with the dat open, surface resolution
//! takes the second source level, the original art, whenever the environment texture detail
//! preference is not the highest. Fixture: the retail dats, the lit interior station in cell
//! `0x860201B1` on a headless software device, and a headless `App` fed a socket-free
//! interrogation. No datagram leaves the process.

#![cfg(gpu)]

use std::sync::Arc;
use {dereth_scene::world_scene::SceneReads, dereth_scene::world_scene::SceneWrites};

use dereth_assets::Decode;
use dereth_client_runtime::camera::CameraInput;
use dereth_client_runtime::character::CharacterInput;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::{CellId, DataId, Frame, LocalTime, Position, Quat, Vec3};
use dereth_render::device::{DeviceConfig, Gpu};
use dereth_scene::textures::TextureStore;
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};

/// The near-room floor of the interior station: surface row `0x08000246` selects surface-texture row
/// `0x050026A0`, whose two source levels are the high-res `0x06003AF6` (256x256, in
/// `client_highres.dat`) and the original `0x06003AF7` (128x128, in `client_portal.dat`).
const FLOOR: DataId = DataId(0x0500_26A0);
const FLOOR_HIGHRES: DataId = DataId(0x0600_3AF6);
const FLOOR_ORIGINAL: DataId = DataId(0x0600_3AF7);

/// The paired profile's `Render.EnvironmentTextureDetail=Medium`; the five authored choices
/// `[VeryLow, Low, Medium, High, VeryHigh]` map to `[4, 3, 2, 1, 0]`.
const MEDIUM: u32 = 2;
const HIGHEST: u32 = 0;

fn resolved(textures: &TextureStore<'_>, id: DataId) -> (DataId, u32, u32) {
    let (rid, rs, _) = textures.resolve(id).expect("the texture resolves");
    (rid, rs.width, rs.height)
}

/// **Rejecting.** A store opened the way the client opens one, with no grant, at the paired
/// preference (`Medium`) and at the registered default, resolves the floor to the original
/// 128x128 level, not the high-res `0x06003AF6` (256x256).
#[test]
fn without_a_grant_the_floor_resolves_to_the_original_level() {
    let store = dereth_dat::testing::open_store_or_fail();
    assert!(
        !store.highres_granted(),
        "open_dir must not open client_highres.dat on its own"
    );
    assert!(store.highres().is_none());
    let at_medium = TextureStore::with_environment_texture_detail(&store, MEDIUM);
    let (id, w, h) = resolved(&at_medium, FLOOR);
    assert_eq!(id, FLOOR_ORIGINAL, "resolved {:#010X}, {w}x{h}", id.0);
    assert_eq!((w, h), (128, 128));
    let default = TextureStore::new(&store);
    assert_eq!(
        resolved(&default, FLOOR).0,
        FLOOR_ORIGINAL,
        "the registered default is not the highest"
    );
    // The highest preference without the grant is still the original: the high-detail predicate
    // asks the cache for the `HiFi` file first.
    let highest = TextureStore::with_environment_texture_detail(&store, HIGHEST);
    assert!(!highest.keeps_high_detail());
    assert_eq!(resolved(&highest, FLOOR).0, FLOOR_ORIGINAL);
}

/// Behaviour: rendering.textures.the-highres-dat-is-a-server-grant-and-the-level-a-preference
/// With the dat granted, the preference decides: `0` keeps the high-res level, `Medium` does
/// not. The grant is idempotent and monotone.
#[test]
fn with_the_grant_only_the_highest_preference_keeps_the_high_res_level() {
    let store = dereth_dat::testing::open_store_or_fail();
    assert!(
        store.grant_highres().expect("client_highres.dat opens"),
        "the retail install has it"
    );
    assert!(store.highres_granted());
    assert!(store.grant_highres().expect("idempotent"));
    let hi = store.highres().expect("granted");
    assert!(hi.contains(FLOOR_HIGHRES) && !hi.contains(FLOOR_ORIGINAL));
    assert!(store.portal().contains(FLOOR_ORIGINAL) && !store.portal().contains(FLOOR_HIGHRES));

    let highest = TextureStore::with_environment_texture_detail(&store, HIGHEST);
    assert!(highest.keeps_high_detail());
    let (id, w, h) = resolved(&highest, FLOOR);
    assert_eq!(id, FLOOR_HIGHRES, "resolved {:#010X}, {w}x{h}", id.0);
    assert_eq!((w, h), (256, 256));

    let medium = TextureStore::with_environment_texture_detail(&store, MEDIUM);
    assert!(!medium.keeps_high_detail());
    assert_eq!(resolved(&medium, FLOOR).0, FLOOR_ORIGINAL);
}

/// The premise the policy relies on, over the whole portal dat: every `SurfaceTexture` lists one
/// or two levels (any other count is a surface-resolution data error), and every two-level one has
/// its **second** level in the portal dat -- so "drop the high detail" never names a record an
/// ungranted client cannot read. The first level is *usually* the high-res partition; the scan
/// counts the pairs whose first level is also portal art (e.g. `0x0500009B` -> `0x0600511C`),
/// which surface resolution treats no differently.
#[test]
fn every_surface_texture_has_one_or_two_levels_and_the_second_is_always_portal_art() {
    let store = dereth_dat::testing::open_store_or_fail();
    assert!(store.grant_highres().expect("client_highres.dat opens"));
    let hi = store.highres().expect("granted");
    let (mut one, mut two_hi, mut two_portal, mut two_missing, mut other) =
        (0usize, 0usize, 0usize, 0usize, 0usize);
    for id in store.ids_of(DbType::SurfaceTexture) {
        if !store.portal().contains(id) {
            continue;
        }
        let bytes = store.read_typed(DbType::SurfaceTexture, id).expect("reads");
        let t = dereth_assets::SurfaceTexture::decode_payload(id, &bytes).expect("decodes");
        match t.source_levels.as_slice() {
            [_] => one += 1,
            [high, original] => {
                assert!(
                    store.portal().contains(*original),
                    "{:#010X}: level[1] {:#010X} not in portal",
                    id.0,
                    original.0
                );
                if hi.contains(*high) {
                    two_hi += 1;
                } else if store.portal().contains(*high) {
                    two_portal += 1;
                } else {
                    two_missing += 1;
                }
            }
            _ => other += 1,
        }
    }
    eprintln!(
        "surface textures: {one} single-level; two-level with level[0] in highres {two_hi}, in portal {two_portal}, in neither {two_missing}; other {other}"
    );
    assert_eq!(
        other, 0,
        "a surface texture has a level count rejected by the surface-data lookup"
    );
    assert!(
        two_hi >= 2_000,
        "the two-level pairs carry the 2,294 high-res records: {two_hi}"
    );
}

// ---- the station: the bake follows the policy, and the means do not move ------------------

const W: u32 = 800;
const H: u32 = 600;
const STATION_CELL: CellId = CellId(0x8602_01B1);
const STATION_ORIGIN: Vec3 = Vec3::new(18.061_283, -30.180_618, 0.005);
const STATION_HEADING: Quat = Quat::new(0.115_688, 0.0, 0.0, -0.993_286);

/// The station's regions, viewport pixels.
const REGIONS: &[(&str, (usize, usize, usize, usize))] = &[
    ("near-left-wall", (120, 113, 200, 263)),
    ("near-floor-left", (130, 393, 330, 483)),
    ("near-floor-right", (480, 393, 640, 483)),
    ("near-ceiling", (500, 63, 680, 103)),
    ("far-back-wall", (340, 173, 480, 213)),
    ("far-floor-by-fire", (330, 253, 470, 288)),
    ("far-table", (320, 218, 380, 248)),
    ("potted-plant", (245, 208, 290, 263)),
];

/// Two rasterisations of the same station from the two art sets sample different texels, so a
/// region's mean moves by the noise of that resampling; the two levels' whole means measure
/// within 0.04..1 %. A shift past this is a brightness change, which the policy must not
/// cause.
const MEAN_TOLERANCE: f64 = 3.0;

fn luma_mean(rgba: &[u8], r: (usize, usize, usize, usize)) -> f64 {
    let (x0, y0, x1, y1) = r;
    let mut sum = 0.0;
    let mut n = 0.0;
    for y in y0..y1 {
        for x in x0..x1 {
            let at = (y * W as usize + x) * 4;
            sum += 0.299 * f64::from(rgba[at])
                + 0.587 * f64::from(rgba[at + 1])
                + 0.114 * f64::from(rgba[at + 2]);
            n += 1.0;
        }
    }
    sum / n
}

fn station_frame(store: &Arc<RetailDatStore>, detail: u32) -> Vec<u8> {
    let cfg = DeviceConfig {
        width: W,
        height: H,
        ..DeviceConfig::default()
    };
    let mut gpu = Gpu::new(None, &cfg).expect("the station requires a real D3D12/WARP device");
    let mut scene_cfg = SceneConfig {
        landblock: 0x8602,
        start_cell: Some(STATION_CELL),
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    scene_cfg.render.environment_texture_detail = detail;
    let region = dereth_client_runtime::landblock::load_region(store).expect("the region decodes");
    let mut scene = WorldScene::load(store, &mut gpu, scene_cfg).expect("the scene loads");
    scene
        .attach_character(store, &region, &mut gpu)
        .expect("the body is created");
    scene
        .character
        .as_mut()
        .expect("a body")
        .teleport(Position::new(
            STATION_CELL,
            Frame::new(STATION_ORIGIN, STATION_HEADING),
        ));
    scene.follow_character_now();
    let mut stream = ObjectStream::new();
    let step = dereth_physics::globals::MIN_QUANTUM;
    #[allow(clippy::cast_possible_truncation)] // LINT-OK: a fixed simulation step in seconds
    let dtf = step as f32;
    let mut now = 0.0f64;
    for _ in 0..120 {
        now += step;
        scene
            .sync_objects(store, &mut gpu, &mut stream)
            .expect("sync_objects");
        scene.update(
            CameraInput::default(),
            CharacterInput::default(),
            LocalTime(now),
            dtf,
        );
        dereth_client_runtime::camera::update_viewer(
            &mut scene,
            CameraInput::default(),
            LocalTime(now),
            step,
        );
        scene.stream(store, &mut gpu).expect("stream");
        scene
            .reserve_upload_arena(&mut gpu)
            .expect("reserve the arena");
        gpu.begin_frame().expect("begin");
        scene.draw(&mut gpu).expect("draw");
        gpu.end_frame().expect("end");
    }
    gpu.capture().expect("capture").to_rgba()
}

/// The station pose, rendered from the original art (no grant, `Medium`) and from the high-res
/// art (granted, highest detail): the frames differ -- the bake took the policy -- and no region's
/// mean luma moves by more than [`MEAN_TOLERANCE`].
#[test]
fn the_station_bakes_the_original_art_and_its_region_means_do_not_move() {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let original = station_frame(&store, MEDIUM);
    assert!(store.grant_highres().expect("client_highres.dat opens"));
    let high = station_frame(&store, HIGHEST);
    let changed = original
        .chunks_exact(4)
        .zip(high.chunks_exact(4))
        .filter(|(a, b)| a != b)
        .count();
    eprintln!("highres station: {changed} pixels differ between the original and the high-res art");
    assert!(
        changed > 10_000,
        "the two art sets must rasterise differently; the bake ignored the policy"
    );
    let mut failures = Vec::new();
    for (name, r) in REGIONS {
        let (o, h) = (luma_mean(&original, *r), luma_mean(&high, *r));
        eprintln!(
            "highres {name:<18} original {o:>6.2} high-res {h:>6.2} delta {:>+6.2}",
            o - h
        );
        if (o - h).abs() > MEAN_TOLERANCE {
            failures.push(format!("{name}: {o:.2} vs {h:.2}"));
        }
    }
    assert!(
        failures.is_empty(),
        "region means moved with the art set: {failures:?}"
    );
}

// ---- the DDD arm: the product id is the grant ---------------------------------------------

/// Through the client's own transport and `App::frame`, a `0xF7E5` interrogation with product id
/// `0x1` (ACE's default) leaves the store without the high-res DAT; setting bit `0x4` by sending
/// product id `0x5` opens it.
#[test]
fn the_interrogation_product_id_grants_the_high_res_dat() {
    use dereth_client::app::App;
    use dereth_client_runtime::config::Config;
    use dereth_client_runtime::net::ClientNetwork;
    use dereth_protocol::admin::DddInterrogation;

    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "PREFLIGHT: {} has no client_highres.dat",
        dir.display()
    );

    let run = |product_id: u32| -> bool {
        let mut app = App::new(Config {
            headless: true,
            sound: false,
            ui: true,
            preferences_file: std::env::temp_dir().join("dereth-highres-not-created/prefs.ini"),
            dat_dir: dir.clone(),
            ..Default::default()
        })
        .expect("the application comes up headless on WARP");
        app.start_shell().expect("the UI shell comes up");
        assert!(
            !app.probe().dat_store().highres_granted(),
            "a fresh store has not been granted the dat"
        );
        let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "highres", "unused", 0)
            .expect("a socket-free network client");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("a literal address")),
        );
        let mut crypto = dereth_transport::CryptoSystem::new(0xDEAD_BEEF);
        app.attach_replay_network(net)
            .map_err(|_| ())
            .expect("the endpoint attaches");
        let bytes = dereth_protocol::write_blob(&DddInterrogation {
            servers_region: 1,
            name_rule_language: 1,
            product_id,
            supported_languages: vec![0, 1],
        })
        .expect("0xF7E5");
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: 2,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: 1,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: 5,
                },
                bytes,
            ))
            .expect("one fragment fits");
        let raw = packet
            .serialize(Some(crypto.next()))
            .expect("the envelope serialises");
        app.replay_network_mut()
            .expect("an explicitly socket-free endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("the transport accepts its own envelope");
        for i in 0..3 {
            assert!(app.frame(), "frame {i} does not end the client");
        }
        app.probe().dat_store().highres_granted()
    };

    assert!(
        !run(0x1),
        "ACE's product id 0x1 must not open client_highres.dat"
    );
    assert!(
        run(0x1 | DddInterrogation::PRODUCT_HIGHRES),
        "product id bit 0x4 opens client_highres.dat"
    );
}
