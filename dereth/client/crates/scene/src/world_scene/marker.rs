//! A picture laid on the ground under a world object: a front end's selection ring.

use super::*;

/// How many cells a marker's square is cut into on each side. Each corner of a cell takes the
/// height of the ground under it, so the picture follows a slope or a dip as it lies.
pub const MARKER_GRID: usize = 8;

/// How far over the ground a marker lies, in metres: enough to stay in front of the ground it lies
/// on, too little to be seen floating.
pub const MARKER_LIFT: f32 = 0.04;

/// The least radius a marker is drawn at, in metres, so a small object's still shows.
pub const MARKER_MIN_RADIUS: f32 = 0.5;

/// One corner of a marker's mesh: where it is, in the world's own axes (east, north, up), and the
/// picture's texture coordinate there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MarkerCorner {
    pub position: Vec3,
    pub uv: [f32; 2],
}

/// The triangles of a marker of `radius` metres laid at `centre` (the object's origin, in the
/// world's axes), turned by `turn` radians, showing the `uv` part (`[left, top, right, bottom]`)
/// of its picture. `ground(dx, dy)` is how far the ground `(dx, dy)` metres from the centre
/// stands above the centre; a point with no ground found lies level with it. Six corners a cell,
/// two triangles.
#[must_use]
pub fn marker_mesh(
    centre: Vec3,
    radius: f32,
    turn: f32,
    uv: [f32; 4],
    ground: &dyn Fn(f32, f32) -> Option<f32>,
) -> Vec<MarkerCorner> {
    let n = MARKER_GRID;
    let (sin, cos) = dereth_primitives::num::math::sin_cosf(turn);
    #[allow(clippy::cast_precision_loss)]
    let step = 2.0 * radius / n as f32;
    let corner = |i: usize, j: usize| {
        #[allow(clippy::cast_precision_loss)]
        let (dx, dy) = (-radius + i as f32 * step, -radius + j as f32 * step);
        let dz = ground(dx, dy).unwrap_or(0.0);
        // The picture turns about the centre; the mesh under it stays square to the world.
        let (u, v) = (dx / radius, dy / radius);
        let (u, v) = (u * cos + v * sin, -u * sin + v * cos);
        let (u, v) = (u * 0.5 + 0.5, 0.5 - v * 0.5);
        MarkerCorner {
            position: Vec3::new(centre.x + dx, centre.y + dy, centre.z + dz + MARKER_LIFT),
            uv: [uv[0] + u * (uv[2] - uv[0]), uv[1] + v * (uv[3] - uv[1])],
        }
    };
    let mut out = Vec::with_capacity(n * n * 6);
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, d) = (
                corner(i, j),
                corner(i + 1, j),
                corner(i + 1, j + 1),
                corner(i, j + 1),
            );
            out.extend([a, b, c, a, c, d]);
        }
    }
    out
}

impl SceneDraw {
    /// Lay `marker` on the ground under its object, after the world and before the overlay: on
    /// the terrain under it outdoors, following the land's shape, or level with the object on a
    /// floor indoors. Hidden where the world stands in front of it, and written into no depth.
    /// Nothing for an object not in the world.
    ///
    /// # Errors
    /// The device's own.
    pub fn draw_ground_marker(
        &self,
        ws: &WorldState,
        gpu: &mut Gpu,
        marker: &dereth_client_contract::overlay::GroundMarker,
        texture: TextureSlot,
        key: &PipelineKey,
    ) -> Result<(), RenderError> {
        let Some(object) = ws.objects.get(&marker.object) else {
            return Ok(());
        };
        if !object.drawn {
            return Ok(());
        }
        let radius = {
            let driver = object.sim.driver.borrow();
            let parts = &driver.part_array;
            parts
                .setup
                .as_ref()
                .map_or(0.0, |s| s.selection_sphere.radius * parts.scale.z)
        };
        let radius = (radius * marker.scale).max(MARKER_MIN_RADIUS);
        // The ground under the object, by its own place in the world: outdoors, the land's
        // height around it, relative to its own height.
        let place = object
            .sim
            .position
            .filter(|p| dereth_physics::landdefs::is_outdoors(p.cell));
        let world = ws.character.as_ref().map(|c| &c.world);
        let ground = |dx: f32, dy: f32| -> Option<f32> {
            let (place, world) = (place.as_ref()?, world?);
            let mut cell = place.cell;
            let mut local = Vec3::new(
                place.frame.origin.x + dx,
                place.frame.origin.y + dy,
                place.frame.origin.z,
            );
            dereth_physics::landdefs::adjust_to_outside(&mut cell, &mut local);
            let mut at = *place;
            at.cell = cell;
            at.frame.origin = local;
            Some(world.terrain_height_at(&at)? - place.frame.origin.z)
        };
        let corners = marker_mesh(object.frame.origin, radius, marker.turn, marker.uv, &ground);
        let mut bytes = Vec::with_capacity(corners.len() * 24);
        for c in &corners {
            // The device's axes put up second.
            for v in [c.position.x, c.position.z, c.position.y] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes.extend_from_slice(&marker.tint.to_le_bytes());
            bytes.extend_from_slice(&c.uv[0].to_le_bytes());
            bytes.extend_from_slice(&c.uv[1].to_le_bytes());
        }
        let (w, h) = gpu.size();
        gpu.set_viewport(self.effective_viewport(ws, w, h));
        let view = self.view_params(ws, w, h);
        let per_frame = per_frame_constants(&view);
        gpu.bind_texture(texture, 1);
        let drawn = gpu.draw_dynamic(
            key,
            &DrawConstants::default(),
            &per_frame,
            &PerDrawConstants::identity(),
            &bytes,
        );
        gpu.reset_viewport();
        drawn
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a picture a front end lays under its selection; not a behaviour of the retail client)
    use super::*;

    fn flat(_: f32, _: f32) -> Option<f32> {
        None
    }

    #[test]
    fn a_marker_covers_its_radius_about_the_object_and_lies_just_over_level_ground() {
        let mesh = marker_mesh(
            Vec3::new(10.0, 20.0, 5.0),
            2.0,
            0.0,
            [0.0, 0.0, 1.0, 1.0],
            &flat,
        );
        assert_eq!(mesh.len(), MARKER_GRID * MARKER_GRID * 6);
        let xs: Vec<f32> = mesh.iter().map(|c| c.position.x).collect();
        let ys: Vec<f32> = mesh.iter().map(|c| c.position.y).collect();
        assert_eq!(xs.iter().copied().fold(f32::MAX, f32::min), 8.0);
        assert_eq!(xs.iter().copied().fold(f32::MIN, f32::max), 12.0);
        assert_eq!(ys.iter().copied().fold(f32::MAX, f32::min), 18.0);
        assert_eq!(ys.iter().copied().fold(f32::MIN, f32::max), 22.0);
        assert!(mesh
            .iter()
            .all(|c| (c.position.z - (5.0 + MARKER_LIFT)).abs() < 1e-6));
    }

    #[test]
    fn a_marker_follows_the_ground_under_each_corner() {
        // A slope rising half a metre a metre to the east.
        let slope = |dx: f32, _: f32| Some(dx * 0.5);
        let mesh = marker_mesh(
            Vec3::new(0.0, 0.0, 0.0),
            2.0,
            0.0,
            [0.0, 0.0, 1.0, 1.0],
            &slope,
        );
        for c in &mesh {
            assert!((c.position.z - (c.position.x * 0.5 + MARKER_LIFT)).abs() < 1e-5);
        }
    }

    #[test]
    fn the_picture_is_centred_on_the_object_and_turns_about_it() {
        let uv = [0.25, 0.5, 0.75, 1.0];
        let centre_uv = |turn: f32| {
            let mesh = marker_mesh(Vec3::ZERO, 1.0, turn, uv, &flat);
            // The corner at the centre of the square.
            mesh.iter()
                .find(|c| c.position.x.abs() < 1e-6 && c.position.y.abs() < 1e-6)
                .map(|c| c.uv)
                .expect("a corner at the centre")
        };
        for turn in [0.0, 1.0, 2.5] {
            let [u, v] = centre_uv(turn);
            assert!(
                (u - 0.5).abs() < 1e-6 && (v - 0.75).abs() < 1e-6,
                "{turn}: {u} {v}"
            );
        }
        // East of the centre, a quarter turn moves the picture's right edge to its top.
        let east = |turn: f32| {
            marker_mesh(Vec3::ZERO, 1.0, turn, [0.0, 0.0, 1.0, 1.0], &flat)
                .into_iter()
                .find(|c| (c.position.x - 1.0).abs() < 1e-6 && c.position.y.abs() < 1e-6)
                .map(|c| c.uv)
                .expect("a corner east of the centre")
        };
        let [u, v] = east(0.0);
        assert!((u - 1.0).abs() < 1e-6 && (v - 0.5).abs() < 1e-6);
        let [u, v] = east(std::f32::consts::FRAC_PI_2);
        assert!((u - 0.5).abs() < 1e-5 && (v - 1.0).abs() < 1e-5, "{u} {v}");
    }
}
