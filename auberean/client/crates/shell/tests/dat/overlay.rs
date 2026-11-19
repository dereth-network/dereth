//! The interface's textured batches: what it draws over textures of its own reaches the frame a
//! host draws, the batches each frame afresh and the textures kept until the interface lets them
//! go.
//!
//! Behaviour: none (how an interface's drawing reaches the host, which no game behavior names)

use std::cell::Cell;
use std::rc::Rc;

use auberean_shell::host::TestHost;
use auberean_shell::interface::{
    Choice, Game, Interface, Overlay, OverlayDepth, OverlayItem, OverlayMaterial, OverlayReleased,
    OverlaySampler, OverlaySpace, OverlayTexture, OverlayVertex, Request, TextureData,
    TextureFormat,
};
use auberean_shell::Shell;

use crate::session::data;

const KEY: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 0x51,
};

/// An interface that uploads one texture once and draws one batch over it while `drawing`, and
/// lets the texture go when `letting_go`.
struct Batches {
    drawing: Rc<Cell<bool>>,
    letting_go: Rc<Cell<bool>>,
    uploaded: bool,
}

impl Interface for Batches {
    fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}

    fn draw(&mut self, _game: &Game<'_>, overlay: &mut Overlay) {
        if !self.uploaded {
            let texels = TextureData {
                width: 1,
                height: 1,
                format: TextureFormat::Bgra8,
                levels: vec![vec![255; 4]],
            };
            assert!(overlay.upload(KEY, &texels));
            self.uploaded = true;
        }
        if self.letting_go.get() {
            assert_eq!(overlay.release(KEY), OverlayReleased::Freed);
        }
        if self.drawing.get() {
            let corner = |x: f32, y: f32| OverlayVertex {
                position: [x, y, 0.0],
                color: 0xFFFF_FFFF,
                uv: [0.0, 0.0],
            };
            overlay.items.push(OverlayItem::Triangles {
                material: OverlayMaterial::Image,
                texture: KEY,
                sampler: OverlaySampler::POINT_CLAMP,
                depth: OverlayDepth::Over,
                vertices: vec![corner(-1.0, 1.0), corner(1.0, 1.0), corner(-1.0, -1.0)],
            });
        }
    }
}

/// A batch an interface draws over a texture it uploaded is in the frame the host draws, beside
/// that texture; the next frame holds only what the interface draws then, while the texture stays
/// until the interface lets it go.
#[test]
fn an_interface_s_textured_batch_reaches_the_frame_and_its_texture_stays_until_let_go() {
    let (world, data) = data();
    let drawing = Rc::new(Cell::new(true));
    let letting_go = Rc::new(Cell::new(false));
    let (d, l) = (Rc::clone(&drawing), Rc::clone(&letting_go));
    let maker = Box::new(
        move |_: Choice| -> Result<Box<dyn Interface + '_>, String> {
            Ok(Box::new(Batches {
                drawing: Rc::clone(&d),
                letting_go: Rc::clone(&l),
                uploaded: false,
            }))
        },
    );
    let mut s = Shell::with_interfaces(TestHost::default(), world, data.clone(), maker);
    let out = s.frame();
    assert_eq!(out.overlay.items.len(), 1, "the batch drawn");
    let OverlayItem::Triangles { texture, .. } = &out.overlay.items[0] else {
        panic!("a batch of triangles");
    };
    assert_eq!(*texture, KEY);
    assert!(out.overlay.textures.get(&KEY).is_some(), "its texture");
    drawing.set(false);
    let out = s.frame();
    assert!(out.overlay.items.is_empty(), "drawn afresh each frame");
    assert!(out.overlay.textures.get(&KEY).is_some(), "kept");
    letting_go.set(true);
    let out = s.frame();
    assert!(out.overlay.textures.is_empty(), "let go");
}
