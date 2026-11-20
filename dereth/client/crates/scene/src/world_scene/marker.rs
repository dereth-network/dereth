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

impl super::WorldScene {
    /// The corners the ring `marker` is laid with under its object (see
    /// [`SceneDraw::ground_marker_corners`]); empty for an object not in the world.
    #[must_use]
    pub fn ground_marker_corners(
        &self,
        marker: &dereth_client_contract::overlay::GroundMarker,
    ) -> Vec<MarkerCorner> {
        self.draw
            .ground_marker_corners(&self.world, marker)
            .unwrap_or_default()
    }
}

impl SceneDraw {
    /// The corners `marker` is laid with under its object: on the terrain under it outdoors,
    /// following the land's shape (the remaster's ground as it is drawn, over the most its fine
    /// relief rises, while it draws one), or
    /// level with the object on a floor indoors. `None` for an object not in the world.
    pub(crate) fn ground_marker_corners(
        &self,
        ws: &WorldState,
        marker: &dereth_client_contract::overlay::GroundMarker,
    ) -> Option<Vec<MarkerCorner>> {
        let object = ws.objects.get(&marker.object)?;
        if !object.drawn {
            return None;
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
        Some(self.laid(
            ws,
            object.sim.position,
            object.frame.origin,
            radius,
            marker.turn,
            marker.uv,
        ))
    }

    /// The corners a ground mark is laid with at its place: on the terrain there outdoors,
    /// following the land's shape as a ground marker does, or level with the place on a floor
    /// indoors. `None` with no character in the world.
    pub(crate) fn ground_mark_corners(
        &self,
        ws: &WorldState,
        mark: &dereth_client_contract::overlay::GroundMark,
    ) -> Option<Vec<MarkerCorner>> {
        let centre = ws.character.as_ref()?.render_frame_of(mark.at).origin;
        Some(self.laid(ws, Some(mark.at), centre, mark.radius, mark.turn, mark.uv))
    }

    /// A picture of `radius` metres laid at `centre` (in the world's axes), for something at
    /// `place`: the ground under it by its own place in the world, outdoors the land's height
    /// around it relative to its own height (the remaster's ground as it is drawn, over the most
    /// its fine relief rises, while it draws one); indoors, or with no place, level with it.
    fn laid(
        &self,
        ws: &WorldState,
        place: Option<dereth_primitives::Position>,
        centre: Vec3,
        radius: f32,
        turn: f32,
        uv: [f32; 4],
    ) -> Vec<MarkerCorner> {
        let ground = |dx: f32, dy: f32| self.ground_under(ws, place.as_ref(), centre, dx, dy);
        marker_mesh(centre, radius, turn, uv, &ground)
    }

    /// How far the ground `(dx, dy)` metres from `centre` (in the world's axes, standing for
    /// `place`) stands above `centre`: outdoors the land's height there (the remaster's ground as
    /// it is drawn, over the most its fine relief rises, while it draws one); `None` indoors or with
    /// no place.
    fn ground_under(
        &self,
        ws: &WorldState,
        place: Option<&dereth_primitives::Position>,
        centre: Vec3,
        dx: f32,
        dy: f32,
    ) -> Option<f32> {
        let place = place.filter(|p| dereth_physics::landdefs::is_outdoors(p.cell))?;
        #[cfg(feature = "remaster")]
        if self.remaster_ground.active {
            return Some(
                self.remaster_ground
                    .ground_at(centre.x + dx, centre.y + dy)?
                    - centre.z
                    + self.remaster_ground.fine_relief(),
            );
        }
        let _ = centre;
        let world = &ws.character.as_ref()?.world;
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
    }

    /// The triangles `band` is drawn with for a camera at `eye` (in the world's axes): each point
    /// held its lift over the ground or floor under it (over its own place where that is higher,
    /// as on a bridge), and the band turned about its line to face the camera there; each
    /// corner with the tint of its point. `None` with no character in the world.
    pub(crate) fn band_corners(
        &self,
        ws: &WorldState,
        band: &dereth_client_contract::overlay::WorldBand,
        eye: Vec3,
    ) -> Option<Vec<(MarkerCorner, u32)>> {
        let character = ws.character.as_ref()?;
        let line: Vec<Vec3> = band
            .points
            .iter()
            .map(|p| {
                let c = character.render_frame_of(p.at).origin;
                let rise = self
                    .ground_under(ws, Some(&p.at), c, 0.0, 0.0)
                    .unwrap_or(0.0)
                    .max(0.0);
                Vec3::new(c.x, c.y, c.z + rise + p.lift)
            })
            .collect();
        let n = line.len();
        if n < 2 {
            return Some(Vec::new());
        }
        // Each point's side: across its line, square to the way the camera looks at it.
        let side = |i: usize| {
            let (a, b) = (line[i.saturating_sub(1)], line[(i + 1).min(n - 1)]);
            let along = b.sub(a);
            let to_eye = eye.sub(line[i]);
            let s = along.cross(to_eye);
            let len = s.magnitude();
            if len > f32::EPSILON {
                s.mul(1.0 / len)
            } else {
                Vec3::new(0.0, 0.0, 1.0)
            }
        };
        let edge = |i: usize, v: f32| {
            let p = &band.points[i];
            let off = side(i).mul(p.half_width * (2.0 * v - 1.0));
            (
                MarkerCorner {
                    position: line[i].add(off),
                    uv: [p.u, v],
                },
                p.tint,
            )
        };
        // Both windings, so the band shows whichever way round the camera sees it.
        let mut out = Vec::with_capacity((n - 1) * 12);
        for i in 0..n - 1 {
            let (a, b, c, d) = (
                edge(i, 0.0),
                edge(i, 1.0),
                edge(i + 1, 1.0),
                edge(i + 1, 0.0),
            );
            out.extend([a, b, c, a, c, d, a, c, b, a, d, c]);
        }
        Some(out)
    }

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
        let Some(corners) = self.ground_marker_corners(ws, marker) else {
            return Ok(());
        };
        self.draw_laid(ws, gpu, &corners, marker.tint, texture, key)
    }

    /// Lay each of `marks` on the ground at its place, in order, with the texture slot it is
    /// drawn from, after the world and before the overlay, as [`Self::draw_ground_marker`] lays a
    /// marker. Nothing with no character in the world.
    ///
    /// # Errors
    /// The device's own; the rest are not drawn.
    pub fn draw_ground_marks(
        &self,
        ws: &WorldState,
        gpu: &mut Gpu,
        marks: &[(dereth_client_contract::overlay::GroundMark, TextureSlot)],
        key: &PipelineKey,
    ) -> Result<(), RenderError> {
        for (mark, texture) in marks {
            let Some(corners) = self.ground_mark_corners(ws, mark) else {
                return Ok(());
            };
            self.draw_laid(ws, gpu, &corners, mark.tint, *texture, key)?;
        }
        Ok(())
    }

    /// Draw each of `bands` with the texture slot it is drawn from, in order, after the world and
    /// the ground marks and before the overlay: turned to face the camera, hidden where the world
    /// stands in front of it, written into no depth. Nothing with no character in the world.
    ///
    /// # Errors
    /// The device's own; the rest are not drawn.
    pub fn draw_world_bands(
        &self,
        ws: &WorldState,
        gpu: &mut Gpu,
        bands: &[(&dereth_client_contract::overlay::WorldBand, TextureSlot)],
        key: &PipelineKey,
    ) -> Result<(), RenderError> {
        let (w, h) = gpu.size();
        let view = self.view_params(ws, w, h);
        // The camera, in the device's axes (up second), then in the world's.
        let eye = view.view.inverse().w_axis;
        let eye = Vec3::new(eye.x, eye.z, eye.y);
        for (band, texture) in bands {
            let Some(corners) = self.band_corners(ws, band, eye) else {
                return Ok(());
            };
            let mut bytes = Vec::with_capacity(corners.len() * 24);
            for (c, tint) in &corners {
                for v in [c.position.x, c.position.z, c.position.y] {
                    bytes.extend_from_slice(&v.to_le_bytes());
                }
                bytes.extend_from_slice(&tint.to_le_bytes());
                bytes.extend_from_slice(&c.uv[0].to_le_bytes());
                bytes.extend_from_slice(&c.uv[1].to_le_bytes());
            }
            gpu.set_viewport(self.effective_viewport(ws, w, h));
            let per_frame = per_frame_constants(&view);
            gpu.bind_texture(*texture, 1);
            let drawn = gpu.draw_dynamic(
                key,
                &DrawConstants::default(),
                &per_frame,
                &PerDrawConstants::identity(),
                &bytes,
            );
            gpu.reset_viewport();
            drawn?;
        }
        Ok(())
    }

    /// Draw a laid picture's `corners`, tinted `tint`, from `texture`.
    fn draw_laid(
        &self,
        ws: &WorldState,
        gpu: &mut Gpu,
        corners: &[MarkerCorner],
        tint: u32,
        texture: TextureSlot,
        key: &PipelineKey,
    ) -> Result<(), RenderError> {
        let mut bytes = Vec::with_capacity(corners.len() * 24);
        for c in corners {
            // The device's axes put up second.
            for v in [c.position.x, c.position.z, c.position.y] {
                bytes.extend_from_slice(&v.to_le_bytes());
            }
            bytes.extend_from_slice(&tint.to_le_bytes());
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

    /// A picture turned by minus a heading's angle has its top that way: turned for east, the
    /// corner east of its centre shows the top of the picture's middle.
    #[test]
    fn a_picture_turned_for_a_heading_has_its_top_that_way() {
        let east = -std::f32::consts::FRAC_PI_2;
        let mesh = marker_mesh(Vec3::ZERO, 1.0, east, [0.0, 0.0, 1.0, 1.0], &flat);
        let at_east = mesh
            .iter()
            .find(|c| (c.position.x - 1.0).abs() < 1e-4 && c.position.y.abs() < 1e-4)
            .expect("a corner east of the centre");
        assert!((at_east.uv[0] - 0.5).abs() < 1e-4, "{:?}", at_east.uv);
        assert!(at_east.uv[1].abs() < 1e-4, "{:?}", at_east.uv);
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
