//! A figure in a viewport: the player's in the inventory window, a character's before the world.
//! A scene of its own, the figure seen straight on (without perspective). In the inventory window
//! it stands still in its first pose, framed to fill the viewport's height whatever the race;
//! before the world it plays its idle, and in creation it is framed for the tallest the race may
//! be made, so the height chosen shows.

use auberean_primitives::Vec3;
use auberean_scene::view::figure_box_and_scale;
use auberean_scene::WorldData;
use auberean_shell::interface::{Looks, ViewCamera, ViewFigure, ViewLight, ViewScene};

use crate::draw::Rect;

/// How much taller than the figure the viewport sees, so it stands clear of the frame.
pub const MARGIN: f32 = 1.08;

/// How far in front of the figure the camera stands, in meters.
const DISTANCE: f32 = 5.0;

/// The look a character whose own look does not put together is shown with, as the game's own
/// character list shows one.
pub const STAND_IN: u32 = 0x1F00_0065;

/// `look`, or the stand-in when it names nothing or does not put together.
#[must_use]
pub fn or_stand_in(
    world: &WorldData,
    look: &auberean_world::look::Desc,
) -> auberean_world::look::Desc {
    let empty = look.parent.is_none() && look.groups.is_empty();
    if empty || auberean_world::look::Look::of(&world.records, look).is_err() {
        auberean_world::look::Desc {
            parent: auberean_primitives::DataId(STAND_IN),
            scale: Vec3::new(1.0, 1.0, 1.0),
            child_scale: 1.0,
            particle_scale: 1.0,
            ..auberean_world::look::Desc::default()
        }
    } else {
        look.clone()
    }
}

/// The figure's height for the look it was last worked out for: before its scale, and as drawn.
#[derive(Debug, Default)]
pub struct Figure {
    framed: Option<(auberean_world::look::Desc, f32, f32)>,
}

/// How a figure stands in its viewport.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Stand {
    /// Degrees from facing the camera.
    pub heading: f32,
    /// It plays its idle, rather than standing still in its first pose.
    pub idle: bool,
    /// The tallest the figure's height scale may be: the viewport is framed for a figure that
    /// tall, so a shorter one stands shorter in it. None frames the figure as it is drawn.
    pub tallest: Option<f32>,
}

impl Stand {
    /// Still, facing the camera, framed as drawn: the inventory window's.
    pub const STILL: Self = Self {
        heading: 0.0,
        idle: false,
        tallest: None,
    };
}

/// The height a viewport is framed for, meters: the figure as drawn (`unscaled` times its height
/// scale `scale`), or as it would be at the tallest scale.
#[must_use]
pub fn framed_height(unscaled: f32, scale: f32, tallest: Option<f32>) -> f32 {
    unscaled * tallest.map_or(scale, |t| t.max(scale))
}

/// The camera that sees a figure `height` meters tall straight on, from in front of it.
#[must_use]
pub fn camera(height: f32) -> ViewCamera {
    let mut c = ViewCamera {
        position: Vec3::new(0.0, DISTANCE, 0.0),
        heading: 180.0,
        orthographic: true,
        ..ViewCamera::default()
    };
    c.set_vertical_scale(height * MARGIN);
    c
}

impl Figure {
    /// The scene showing the player's figure in `rect` (window pixels), as `looks` has the
    /// player; none before the player's look is known or when it draws nothing.
    pub fn scene(&mut self, world: &WorldData, looks: &Looks, rect: Rect) -> Option<ViewScene> {
        let (_, look) = looks.player.as_ref()?;
        self.scene_of(world, look, Stand::STILL, rect)
    }

    /// The scene showing a figure of `look` in `rect` (window pixels), standing as `stand` says;
    /// none when it draws nothing. Its feet are at the foot of the height it is framed for.
    pub fn scene_of(
        &mut self,
        world: &WorldData,
        look: &auberean_world::look::Desc,
        stand: Stand,
        rect: Rect,
    ) -> Option<ViewScene> {
        if self.framed.as_ref().is_none_or(|(d, _, _)| d != look) {
            let (unscaled, scale) = figure_box_and_scale(&world.records, look)
                .map_or((0.0, 1.0), |(lo, hi, s)| (hi.z - lo.z, s.z));
            self.framed = Some((look.clone(), unscaled, scale));
        }
        let (unscaled, scale) = self.framed.as_ref().map_or((0.0, 1.0), |f| (f.1, f.2));
        let height = framed_height(unscaled, scale, stand.tallest);
        if height <= 0.0 || unscaled * scale <= 0.0 {
            return None;
        }
        Some(ViewScene {
            rect: [rect.x, rect.y, rect.right(), rect.bottom()],
            scene: 0,
            camera: camera(height),
            light: ViewLight::Camera,
            figures: vec![ViewFigure {
                look: look.clone(),
                origin: Vec3::new(0.0, 0.0, -height / 2.0),
                heading: stand.heading,
                mode: 0,
                still: !stand.idle,
            }],
            pick: None,
            behaviors: Vec::new(),
            under: false,
        })
    }
}
