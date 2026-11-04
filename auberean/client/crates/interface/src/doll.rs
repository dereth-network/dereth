//! The player's figure in a panel's viewport, as the player looks: the inventory's paper doll (the
//! whole figure) and a consignment's portrait (the head and shoulders).
//!
//! The figure stands still in its first pose, facing its scene's camera, which sees it straight on
//! (without perspective). The paper doll's camera sees as much height as the figure draws (less for
//! the races drawn shorter), the figure's own place half that height below the camera; a
//! portrait's camera sees a quarter of it, the figure lower, so the head fills the picture.

use auberean_client_sdk::protocol::types::VisualDesc as WireVisual;
use auberean_client_sdk::runtime::description;
use auberean_client_sdk::Client;
use auberean_scene::view::{figure_box, scene_camera, ViewFigure, ViewLight, ViewScene};
use auberean_ui::{Handle, Ui};
use auberean_ui_game::inventory::paper_doll::{doll_scale, RACE};
use auberean_world::look::Desc;
use auberean_world::records::WorldRecords;

/// The player's look as the world draws it: its own record's when the description names one, else
/// the description sent.
#[must_use]
pub fn look_of(records: &WorldRecords, v: &WireVisual) -> Desc {
    if v.bits & WireVisual::DID != 0 && v.did != 0 {
        if let Some(id) = auberean_scene::visual_of(records, v) {
            return Desc {
                parent: id,
                scale: auberean_primitives::Vec3::new(1.0, 1.0, 1.0),
                child_scale: 1.0,
                particle_scale: 1.0,
                ..Desc::default()
            };
        }
    }
    auberean_scene::desc_of(records, v)
}

/// The race whose portrait stands higher.
const DRUDGE: i32 = 8;

/// Where the figure stands below the camera (its place's height) and the height the camera sees,
/// for a figure that draws `height` metres: the whole figure, or a portrait.
// D2-02900, D2-02322
#[must_use]
pub fn framing(height: f32, race: Option<i32>, portrait: bool) -> (f32, f32) {
    let h = height * doll_scale(race);
    if portrait {
        let below = if race.unwrap_or(0) == DRUDGE {
            -0.625
        } else {
            -0.875
        };
        (h * below, h * 0.25)
    } else {
        (h * -0.5, h)
    }
}

/// The viewports showing the player's figure, and the figure's drawn height for the look it was
/// last worked out for.
#[derive(Debug, Default)]
pub struct Dolls {
    shown: Vec<(Handle, bool)>,
    framed: Option<(Desc, f32)>,
}

impl Dolls {
    /// A viewport shows the player's figure from now on (`portrait`: its head and shoulders).
    // D2-02906, D2-02329
    pub fn show(&mut self, viewport: Handle, portrait: bool) {
        if !self.shown.iter().any(|(v, _)| *v == viewport) {
            self.shown.push((viewport, portrait));
        }
    }

    /// The scene of each such viewport on the screen now: its own scene and camera, the camera
    /// seeing straight on as much as the figure's framing says, the figure as the player looks.
    /// A search of the paper doll's figure under `pick` (window pixels) goes with the whole
    /// figure's scene, the pointer taken from its viewport's corner: that scene's place in `out`.
    // D2-02900, D2-02322, D2-02907, D2-06211, D2-06212, D2-05962, D2-10483, D2-10490, D2-10492, D2-02902, D2-02330, D2-02331
    pub fn scenes(
        &mut self,
        ui: &Ui,
        records: &WorldRecords,
        client: &Client,
        pick: Option<(f32, f32)>,
        out: &mut Vec<ViewScene>,
    ) -> Option<usize> {
        let player = client.model.player.and_then(|p| client.model.object(p))?;
        let race = description::int(&player.weenie, RACE)
            .or_else(|| player.qualities.ints.get(&RACE).copied());
        let look = look_of(records, &player.visual);
        if self.framed.as_ref().is_none_or(|(d, _)| *d != look) {
            let height = figure_box(records, &look).map_or(0.0, |(lo, hi)| hi.z - lo.z);
            self.framed = Some((look.clone(), height));
        }
        let height = self.framed.as_ref().map_or(0.0, |f| f.1);
        if height <= 0.0 {
            return None;
        }
        let mut asked = None;
        self.shown.retain(|(v, _)| ui.node(*v).is_some());
        for &(v, portrait) in &self.shown {
            if !ui.shown(v) {
                continue;
            }
            let (Some(rect), Some(name)) = (ui.screen_rect(v), ui.viewport_camera(v)) else {
                continue;
            };
            let scene_id = ui.viewport_scene(v);
            let Some(scene) = records.get::<auberean_assets::ui::UiScene>(scene_id) else {
                continue;
            };
            let Some(mut camera) = scene_camera(&scene, &name) else {
                continue;
            };
            let (below, seen) = framing(height, race, portrait);
            camera.orthographic = true;
            camera.set_vertical_scale(seen);
            #[allow(clippy::cast_precision_loss)]
            let rect = [
                rect.x0 as f32,
                rect.y0 as f32,
                rect.x1 as f32,
                rect.y1 as f32,
            ];
            let pick = pick
                .filter(|_| !portrait && asked.is_none())
                .map(|(x, y)| (x - rect[0], y - rect[1]));
            if pick.is_some() {
                asked = Some(out.len());
            }
            out.push(ViewScene {
                rect,
                scene: scene_id,
                camera,
                light: ViewLight::Scene,
                figures: vec![ViewFigure {
                    look: look.clone(),
                    origin: auberean_primitives::Vec3::new(0.0, 0.0, below),
                    heading: 0.0,
                    mode: 0,
                    still: true,
                }],
                pick,
                behaviors: Vec::new(),
                under: false,
            });
        }
        asked
    }
}
