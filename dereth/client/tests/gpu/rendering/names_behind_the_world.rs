//! Horizon's names over people and creatures stand in the world: on every renderer of the
//! ordinary look, a name is drawn only where the world drawn this frame is farther away than the
//! top of the body it names, pixel by pixel, and the rest of the interface is drawn over
//! everything as it always was. Under Better Lighting, whose world's depth is its own, a name is
//! drawn over everything as before.
//!
//! Fixtures, on each renderer this build has, on a hardware device: a wall eight meters off across
//! the left half of the frame, written into the frame's depth as the world's own draws write
//! theirs; and the world itself at Holtburg from the retail dats (`DERETH_TEST_DAT_DIR`), with a
//! big body standing in front of others. The names are the interface's own draw list, given the
//! depth of the point each stands on and drawn through the client's presentation.
//!
//! Behaviour: none (experimental Horizon interface)

#![cfg(gpu)]

use dereth_client_runtime::present::Presentation;
use dereth_client_shell::gpu::Renderer;
use dereth_horizon::art::Art;
use dereth_horizon::draw::{Anchor, Argb, DrawList, Overlay, Rect, Spot};
use dereth_primitives::ObjectId;
use dereth_render::device::{AdapterKind, Backend, PerDrawConstants, PerFrameConstants};
use dereth_render::pso::{PipelineKey, SurfaceContext};
use dereth_render::surface::{surface_type, Surface};
use dereth_render::vertex::VertexFormat;
use dereth_render::{DrawConstants, ViewParams, ZFunc};

const W: u32 = 320;
const H: u32 = 200;
/// The wall covers every column left of this one, from top to bottom.
const WALL_RIGHT: u32 = 160;
const WALL_METERS: f32 = 8.0;
/// How far off the bodies stand whose names are behind the wall.
const BEHIND_METERS: f32 = 10.0;
/// How far off the bodies stand whose names are in front of it.
const BEFORE_METERS: f32 = 6.0;

const RED: Argb = 0xFFFF_0000;
const GREEN: Argb = 0xFF00_FF00;
const BLUE: Argb = 0xFF00_00FF;
const WHITE: Argb = 0xFFFF_FFFF;
/// The wall's colour.
const GREY: Argb = 0xFF60_6060;

/// The depth a point `meters` ahead of the camera is drawn at, by the world's own projection.
fn depth(meters: f32) -> f32 {
    let view = ViewParams::default();
    let p = dereth_render::camera::projection(&view) * glam::Vec4::new(0.0, 0.0, meters, 1.0);
    p.z / p.w
}

/// A renderer on `backend`, off screen, on a real GPU.
fn renderer(backend: Backend) -> Renderer {
    let r = Renderer::new_on(backend, None, W, H)
        .unwrap_or_else(|e| panic!("{backend:?}: these frames need a device and none opened: {e}"));
    assert_eq!(
        r.gpu.adapter_kind(),
        AdapterKind::Hardware,
        "{backend:?}: {} is a software rasteriser",
        r.gpu.adapter_name()
    );
    r
}

/// One opaque white texel, which the wall is drawn with.
const WALL_TEXTURE: dereth_client_contract::overlay::OverlayTexture =
    dereth_client_contract::overlay::OverlayTexture {
        space: dereth_client_contract::overlay::OverlaySpace::Local,
        key: 0x7761_6C6C,
    };

/// The pipeline state an opaque world surface is drawn with: tested against the depth and
/// writing its own.
fn world_surface() -> PipelineKey {
    let surface = Surface {
        r#type: surface_type::BASE1_IMAGE,
        ..Surface::default()
    };
    let ctx = SurfaceContext {
        vertex_format: VertexFormat::XyzDiffuseTex1,
        ..SurfaceContext::default()
    };
    let (mut key, _) = PipelineKey::from_surface(&surface, ctx);
    key.z_func = ZFunc::LessEqual;
    key.z_write = true;
    key
}

/// The wall, `WALL_METERS` off over the columns left of `WALL_RIGHT`, drawn into the open frame
/// as a world surface is.
fn wall(r: &mut Renderer) {
    let slot = r
        .overlay_slot(WALL_TEXTURE)
        .expect("the wall's texel is uploaded");
    #[allow(clippy::cast_precision_loss)]
    let right = WALL_RIGHT as f32 / W as f32 * 2.0 - 1.0;
    let z = depth(WALL_METERS);
    let v = |x: f32, y: f32| dereth_client_contract::overlay::OverlayVertex {
        position: [x, y, z],
        color: GREY,
        uv: [0.0, 0.0],
    };
    let vertices = [
        v(-1.0, 1.0),
        v(-1.0, -1.0),
        v(right, -1.0),
        v(right, -1.0),
        v(right, 1.0),
        v(-1.0, 1.0),
    ];
    let mut per_frame = PerFrameConstants::from_view(&ViewParams::default());
    per_frame.view_proj = dereth_scene::gpu::glam_identity();
    per_frame.view = dereth_scene::gpu::glam_identity();
    r.gpu.bind_texture(slot, 3);
    r.gpu
        .draw_dynamic(
            &world_surface(),
            &DrawConstants::default(),
            &per_frame,
            &PerDrawConstants::identity(),
            &dereth_scene::gpu::overlay_vertex_bytes(&vertices),
        )
        .expect("the wall is drawn");
}

/// A name: a flat rectangle of its own colour standing on the top of the body `id`, `meters` off.
struct Name {
    id: ObjectId,
    rect: Rect,
    colour: Argb,
    meters: f32,
}

/// The interface's draw list: `names` first, standing in the world, then `rest`, the rest of the
/// interface, drawn over it; the names followed to the depth of the point each stands on, as the
/// client follows them once the world has moved.
fn interface(names: &[Name], rest: &[(Rect, Argb)]) -> DrawList {
    let mut list = DrawList::default();
    for n in names {
        let from = list.mark();
        list.fill(n.rect, n.colour);
        let top = (n.rect.x + n.rect.w / 2.0, n.rect.bottom());
        list.anchor_in_world(n.id, Anchor::Top, from, top, (n.rect.x, n.rect.y));
    }
    for (r, c) in rest {
        list.fill(*r, *c);
    }
    list.follow(|id, _| {
        let n = names.iter().find(|n| n.id == id)?;
        Some(Spot {
            x: n.rect.x + n.rect.w / 2.0,
            y: n.rect.bottom(),
            depth: Some(depth(n.meters)),
        })
    });
    list
}

/// The frame `list` draws over the wall, as BGRA.
fn frame(r: &mut Renderer, list: &DrawList) -> Vec<u8> {
    let art = Art::empty();
    let mut overlay = Overlay::default();
    overlay.compose(r, &art, list, false, &[]);
    Presentation::start_frame(r).expect("the frame starts");
    wall(r);
    overlay.draw(r).expect("the interface is drawn");
    Presentation::end_frame(r).expect("the frame ends");
    let (w, h, bgra) = r.capture_bgra().expect("the frame is read back");
    assert_eq!((w, h), (W, H));
    overlay.release(r);
    bgra
}

/// The renderer on `backend` with the wall's texel uploaded.
fn ready(backend: Backend) -> Renderer {
    let mut r = renderer(backend);
    let white = dereth_primitives::TextureData {
        width: 1,
        height: 1,
        format: dereth_primitives::TextureFormat::Bgra8,
        levels: vec![vec![255, 255, 255, 255]],
    };
    Presentation::overlay_upload(&mut r, WALL_TEXTURE, &white).expect("the wall's texel uploads");
    r
}

/// Whether the BGRA pixel `p` is `colour`, give or take rounding.
fn is(p: &[u8; 4], colour: Argb) -> bool {
    let [b, g, r] = [colour & 0xFF, (colour >> 8) & 0xFF, (colour >> 16) & 0xFF];
    [p[0], p[1], p[2]]
        .iter()
        .zip([b, g, r])
        .all(|(&got, want)| u32::from(got).abs_diff(want) <= 2)
}

/// How many pixels of `bgra` are `colour`, in the columns `cols`.
fn count(bgra: &[u8], colour: Argb, cols: std::ops::Range<u32>) -> usize {
    bgra.as_chunks::<4>()
        .0
        .iter()
        .enumerate()
        .filter(|(i, p)| {
            #[allow(clippy::cast_possible_truncation)]
            let x = (*i as u32) % W;
            cols.contains(&x) && is(p, colour)
        })
        .count()
}

/// How many pixels `r` covers.
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn area(r: Rect) -> usize {
    (r.w as usize) * (r.h as usize)
}

/// A name hidden behind the wall draws not one pixel; one standing in front of the wall, over the
/// same pixels, draws all of its own.
#[test]
fn a_name_behind_a_wall_draws_nothing_where_the_wall_stands_in_front() {
    let _gpu = crate::common::gpu_lock();
    for backend in Backend::compiled() {
        let mut r = ready(backend);
        let behind = Name {
            id: ObjectId(0x8300_0701),
            rect: Rect::new(30.0, 40.0, 60.0, 14.0),
            colour: RED,
            meters: BEHIND_METERS,
        };
        let before = Name {
            id: ObjectId(0x8300_0702),
            rect: Rect::new(30.0, 100.0, 60.0, 14.0),
            colour: GREEN,
            meters: BEFORE_METERS,
        };
        let bgra = frame(&mut r, &interface(&[behind, before], &[]));
        assert_eq!(
            count(&bgra, RED, 0..W),
            0,
            "{backend:?}: the name behind the wall"
        );
        assert_eq!(
            count(&bgra, GREEN, 0..W),
            area(Rect::new(30.0, 100.0, 60.0, 14.0)),
            "{backend:?}: the name in front of the wall"
        );
        let lit = bgra
            .as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| (i % (W as usize)) < (WALL_RIGHT as usize) && p[..3] != [0, 0, 0])
            .count();
        assert!(lit > 0, "{backend:?}: the wall is drawn");
    }
}

/// A name where nothing of the world stands in front of it draws every pixel it covers.
#[test]
fn a_name_in_the_open_draws_in_full() {
    let _gpu = crate::common::gpu_lock();
    for backend in Backend::compiled() {
        let mut r = ready(backend);
        let open = Rect::new(200.0, 60.0, 80.0, 16.0);
        let name = Name {
            id: ObjectId(0x8300_0703),
            rect: open,
            colour: BLUE,
            meters: BEHIND_METERS,
        };
        let bgra = frame(&mut r, &interface(&[name], &[]));
        assert_eq!(count(&bgra, BLUE, 0..W), area(open), "{backend:?}");
    }
}

/// A name half behind the wall's edge draws the half beside the wall and none of the half behind
/// it.
#[test]
fn a_name_partly_behind_a_wall_draws_only_the_part_beside_it() {
    let _gpu = crate::common::gpu_lock();
    for backend in Backend::compiled() {
        let mut r = ready(backend);
        let across = Rect::new(120.0, 80.0, 80.0, 16.0);
        let name = Name {
            id: ObjectId(0x8300_0704),
            rect: across,
            colour: RED,
            meters: BEHIND_METERS,
        };
        let bgra = frame(&mut r, &interface(&[name], &[]));
        assert_eq!(
            count(&bgra, RED, 0..WALL_RIGHT),
            0,
            "{backend:?}: behind the wall"
        );
        assert_eq!(
            count(&bgra, RED, WALL_RIGHT..W),
            area(Rect::new(160.0, 80.0, 40.0, 16.0)),
            "{backend:?}: beside the wall"
        );
    }
}

/// The rest of the interface is drawn over the world as it always was: a frame with a name
/// hidden behind the wall is the very frame drawn without it, and the interface's own pictures
/// over the wall draw every pixel they cover.
#[test]
fn the_rest_of_the_interface_is_drawn_over_the_world_unchanged() {
    let _gpu = crate::common::gpu_lock();
    for backend in Backend::compiled() {
        let mut r = ready(backend);
        let bar = Rect::new(20.0, 150.0, 260.0, 20.0);
        let rest = [(bar, WHITE), (Rect::new(40.0, 30.0, 40.0, 40.0), BLUE)];
        let hidden = Name {
            id: ObjectId(0x8300_0705),
            rect: Rect::new(30.0, 40.0, 60.0, 14.0),
            colour: RED,
            meters: BEHIND_METERS,
        };
        let without = frame(&mut r, &interface(&[], &rest));
        let with = frame(&mut r, &interface(&[hidden], &rest));
        assert!(
            without == with,
            "{backend:?}: a hidden name changed {} pixels",
            without
                .as_chunks::<4>()
                .0
                .iter()
                .zip(with.as_chunks::<4>().0)
                .filter(|(a, b)| a != b)
                .count()
        );
        assert_eq!(count(&with, WHITE, 0..W), area(bar), "{backend:?}: the bar");
        assert_eq!(
            count(&with, BLUE, 0..W),
            area(Rect::new(40.0, 30.0, 40.0, 40.0)),
            "{backend:?}: the picture over the wall"
        );
    }
}

/// Under Better Lighting the world's depth is its own, not the one names are placed by, so a
/// name is drawn over everything, as it was before names stood in the world.
#[cfg(feature = "hifi")]
#[test]
fn under_better_lighting_a_name_is_drawn_over_the_world() {
    use dereth_render::wgpu::sidecar::{
        wgpu, FrameSidecar, SidecarContext, SidecarError, SidecarFrame,
    };

    /// Better Lighting with nothing of its own to draw this frame: the frame is drawn the
    /// ordinary way, as it is when no world stands in it.
    struct Plain;

    impl FrameSidecar for Plain {
        fn prepare(&mut self, _cx: &mut SidecarContext<'_>) -> Result<SidecarFrame, SidecarError> {
            Ok(SidecarFrame::Plain)
        }

        fn encode(
            &mut self,
            _cx: &mut SidecarContext<'_>,
            _encoder: &mut wgpu::CommandEncoder,
            _target: &wgpu::TextureView,
        ) -> Result<(), SidecarError> {
            Ok(())
        }

        fn as_any_mut(&mut self) -> &mut dyn std::any::Any {
            self
        }
    }

    let _gpu = crate::common::gpu_lock();
    let mut r = Renderer::new_on_for_hifi(Backend::Wgpu, None, W, H, true)
        .unwrap_or_else(|e| panic!("this frame needs a wgpu device and none opened: {e}"));
    let white = dereth_primitives::TextureData {
        width: 1,
        height: 1,
        format: dereth_primitives::TextureFormat::Bgra8,
        levels: vec![vec![255, 255, 255, 255]],
    };
    Presentation::overlay_upload(&mut r, WALL_TEXTURE, &white).expect("the wall's texel uploads");
    let behind = Rect::new(30.0, 40.0, 60.0, 14.0);
    let name = || Name {
        id: ObjectId(0x8300_0706),
        rect: behind,
        colour: RED,
        meters: BEHIND_METERS,
    };
    let bgra = frame(&mut r, &interface(&[name()], &[]));
    assert_eq!(count(&bgra, RED, 0..W), 0, "the ordinary look hides it");
    r.gpu
        .hifi_install(Box::new(Plain))
        .unwrap_or_else(|e| panic!("Better Lighting is not taken here: {e}"));
    let bgra = frame(&mut r, &interface(&[name()], &[]));
    assert_eq!(
        count(&bgra, RED, 0..W),
        area(behind),
        "under Better Lighting it is drawn over the world"
    );
    drop(r.gpu.hifi_take());
}

/// The same in the world itself: bodies standing on the ground at Holtburg, a big one in front of
/// the others, drawn by the world's own draw; each name placed and given its depth by the point
/// over its body the presentation projects, as the client places it.
mod in_the_world {
    use super::*;
    use crate::instruments::nameplate_frames::{
        along, create, place_camera, scene, step, turned, Subject, VIEW,
    };
    use dereth_client_runtime::objects::ObjectStream;
    use dereth_horizon::ui::game::BlipKind;
    use dereth_scene::world_scene::{SceneReads, SceneWrites, WorldScene};

    const WW: u32 = 640;
    const WH: u32 = 400;
    const MAGENTA: Argb = 0xFFFF_00FF;

    /// A chest made four times its size, standing between the camera and the drudges.
    const IN_FRONT: Subject = Subject {
        id: ObjectId(0x8300_0711),
        name: "Chest",
        kind: BlipKind::Portal,
        setup: 0x0200_007C,
        mtable: None,
        scale: Some(4.0),
        at: (0.0, 6.0),
    };

    /// A drudge, `at` from the player.
    const fn drudge(id: u32, at: (f32, f32)) -> Subject {
        Subject {
            id: ObjectId(id),
            name: "Drudge Skulker",
            kind: BlipKind::Creature,
            setup: 0x0200_07DD,
            mtable: Some(0x0900_0008),
            scale: None,
            at,
        }
    }

    /// Behind the chest, well in from its edge.
    const BEHIND: Subject = drudge(0x8300_0712, (-1.5, 15.0));
    /// Well clear of it.
    const OPEN: Subject = drudge(0x8300_0713, (9.0, 15.0));
    /// Behind its edge.
    const EDGE: Subject = drudge(0x8300_0714, (1.2, 15.0));

    /// The station: the bodies made, settled on the ground, and the camera behind the chest,
    /// looking past it at the drudges.
    fn station(store: &std::sync::Arc<dereth_dat::RetailDatStore>, r: &mut Renderer) -> WorldScene {
        let mut scene = scene(store, &mut r.gpu);
        let mut stream = ObjectStream::new();
        let mut t = 1.0f64;
        let (made, settled) = (120, 240);
        let mut at = None;
        for i in 0..=settled {
            t += 1.0 / 60.0;
            if i == made {
                let here = turned(scene.character.as_ref().expect("a body").position(), VIEW);
                for s in [&IN_FRONT, &BEHIND, &OPEN, &EDGE] {
                    create(&mut stream, s, along(&here, s.at, std::f32::consts::PI), t);
                }
                at = Some(here);
            }
            step(store, &mut r.gpu, &mut scene, &mut stream, t, |scene| {
                if let Some(at) = at {
                    // At a person's eye height over the player's own ground.
                    place_camera(scene, &at, (0.0, 1.0, 1.8), 0.0, ObjectId(0));
                }
            });
        }
        scene
    }

    /// A name over each of `ids`: a flat rectangle standing on the point over its body, given that
    /// point's depth when `in_world` says so, as the client follows it; and where each is.
    fn names(scene: &WorldScene, ids: &[ObjectId], in_world: bool) -> (DrawList, Vec<Rect>) {
        let mut list = DrawList::default();
        let mut rects = Vec::new();
        for &id in ids {
            let (x, y, _) = scene.target_top(id, (WW, WH)).expect("on screen");
            let r = Rect::new((x - 24.0).round(), (y - 16.0).round(), 48.0, 10.0);
            let from = list.mark();
            list.fill(r, MAGENTA);
            list.anchor_in_world(id, Anchor::Top, from, (x, y), (r.x, r.y));
            rects.push(r);
        }
        list.follow(|id, _| {
            let (x, y, depth) = scene.target_top(id, (WW, WH))?;
            Some(Spot {
                x,
                y,
                depth: depth.filter(|_| in_world),
            })
        });
        (list, rects)
    }

    /// The world drawn with `list` over it, as BGRA.
    fn drawn(r: &mut Renderer, scene: &mut WorldScene, list: &DrawList) -> Vec<u8> {
        let art = Art::empty();
        let mut overlay = Overlay::default();
        overlay.compose(r, &art, list, false, &[]);
        scene
            .reserve_upload_arena(&mut r.gpu)
            .expect("reserve the arena");
        Presentation::start_frame(r).expect("the frame starts");
        scene.draw(&mut r.gpu).expect("the world is drawn");
        overlay.draw(r).expect("the interface is drawn");
        Presentation::end_frame(r).expect("the frame ends");
        let (_, _, bgra) = r.capture_bgra().expect("the frame is read back");
        overlay.release(r);
        bgra
    }

    /// How many pixels of `bgra` inside `rect` are `colour`.
    fn inside(bgra: &[u8], rect: Rect, colour: Argb) -> usize {
        bgra.as_chunks::<4>()
            .0
            .iter()
            .enumerate()
            .filter(|(i, p)| {
                #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
                let (x, y) = ((*i as u32 % WW) as f32 + 0.5, (*i as u32 / WW) as f32 + 0.5);
                rect.contains(x, y) && is(p, colour)
            })
            .count()
    }

    /// A drudge's name behind the chest draws nothing, one well clear of it draws in full, and
    /// one behind its edge draws only the part beside it.
    #[test]
    fn a_name_behind_a_body_in_front_of_it_draws_only_where_the_body_is_not() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        for backend in Backend::compiled() {
            let mut r = Renderer::new_on(backend, None, WW, WH)
                .unwrap_or_else(|e| panic!("{backend:?}: no device opened: {e}"));
            let mut scene = station(&store, &mut r);
            let ids = [BEHIND.id, OPEN.id, EDGE.id];
            // Drawn over everything, every name is whole: each stands on screen, on nothing of
            // its own colour.
            let (over, rects) = names(&scene, &ids, false);
            let all = drawn(&mut r, &mut scene, &over);
            for (rect, id) in rects.iter().zip(&ids) {
                assert_eq!(
                    inside(&all, *rect, MAGENTA),
                    area(*rect),
                    "{backend:?} {id:?}: drawn over everything"
                );
            }
            let (in_world, rects) = names(&scene, &ids, true);
            let bgra = drawn(&mut r, &mut scene, &in_world);
            let shown: Vec<usize> = rects.iter().map(|r| inside(&bgra, *r, MAGENTA)).collect();
            assert_eq!(shown[0], 0, "{backend:?}: behind the chest");
            assert_eq!(shown[1], area(rects[1]), "{backend:?}: in the open");
            assert!(
                shown[2] > 0 && shown[2] < area(rects[2]),
                "{backend:?}: behind its edge, {} of {}",
                shown[2],
                area(rects[2])
            );
            // The part drawn is the part beside the chest: on every row, from where the chest's
            // edge leaves off to the name's end.
            let edge = rects[2];
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            for y in edge.y as u32..edge.bottom() as u32 {
                let row: Vec<bool> = (edge.x as u32..edge.right() as u32)
                    .map(|x| {
                        let i = ((y * WW + x) * 4) as usize;
                        is(&[bgra[i], bgra[i + 1], bgra[i + 2], bgra[i + 3]], MAGENTA)
                    })
                    .collect();
                let first = row.iter().position(|&m| m).unwrap_or(row.len());
                assert!(
                    row[first..].iter().all(|&m| m),
                    "{backend:?}: row {y} of the name behind the edge: {row:?}"
                );
            }
            scene.release_textures(&mut r.gpu);
        }
    }

    /// Seen from a room, a body outdoors keeps no depth the world drew for it: the outdoors is
    /// drawn before the room's depth is cleared and its openings are stamped back nearer, so its
    /// name, the selection's, is drawn over the world as before rather than hidden by the opening
    /// it is seen through.
    #[test]
    fn a_name_over_a_body_outdoors_seen_from_a_room_has_no_depth() {
        let _gpu = crate::common::gpu_lock();
        let store = crate::common::dats();
        let mut r = Renderer::new(None, WW, WH)
            .unwrap_or_else(|e| panic!("this station needs a device and none opened: {e}"));
        let mut scene = station(&store, &mut r);
        let depth = |scene: &WorldScene| scene.target_top(BEHIND.id, (WW, WH)).map(|t| t.2);
        assert!(
            matches!(depth(&scene), Some(Some(_))),
            "seen from outdoors, it has the depth the world drew it at"
        );
        let camera = &mut scene.character.as_mut().expect("a body").camera;
        // Holtburg's windowed house.
        camera.viewer_cell = Some(dereth_primitives::CellId(0xA9B4_0146));
        assert_eq!(depth(&scene), Some(None), "seen from a room");
        scene.release_textures(&mut r.gpu);
    }
}
