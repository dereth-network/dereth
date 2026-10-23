//! Where a world was left, kept beside it in `.lyceum-place.toml` and taken up when it opens again
//! (unless the command line names a landblock): the camera, the tool, and the dungeon gone into
//! with where Leave goes back to.
//!
//! ```toml
//! camera = [41570.5, 34650.0, 112.0, 30.0, -20.0, 3]   # world meters, yaw and pitch, speed
//! mode = "select"
//! dungeon = 0x0163
//! back = [41570.5, 34650.0, 112.0, 30.0, -20.0, 3]
//! ```

use std::path::{Path, PathBuf};

use dereth_primitives::Vec3;

use crate::camera::FlyCamera;

/// The file a world's place is kept in, beside its `world.toml`.
#[must_use]
pub fn file(world: &Path) -> PathBuf {
    world
        .parent()
        .unwrap_or(Path::new("."))
        .join(".lyceum-place.toml")
}

/// Where a world was left.
#[derive(Debug, Clone, PartialEq)]
pub struct Left {
    pub camera: FlyCamera,
    /// The tool, as a script names it: `select`, `place`, `terrain`, `spawn`.
    pub mode: String,
    /// The dungeon gone into, and where the camera was before.
    pub dungeon: Option<(u16, FlyCamera)>,
}

impl Left {
    /// The landblock the camera is over.
    #[must_use]
    pub fn landblock(&self) -> Option<u16> {
        crate::camera::landblock_of(self.camera.position).map(|(lb, ..)| lb)
    }
}

fn camera_value(c: &FlyCamera) -> String {
    format!(
        "[{:.3}, {:.3}, {:.3}, {:.3}, {:.3}, {}]",
        c.position.x,
        c.position.y,
        c.position.z,
        c.yaw.to_degrees(),
        c.pitch.to_degrees(),
        c.speed
    )
}

fn camera_of(v: &toml::Value) -> Option<FlyCamera> {
    let a = v.as_array()?;
    let n: Vec<f64> = a
        .iter()
        .filter_map(|x| x.as_float().or_else(|| x.as_integer().map(|i| i as f64)))
        .collect();
    let [x, y, z, yaw, pitch, speed] = n[..] else {
        return None;
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // meters, degrees, a step
    let mut c = FlyCamera::new(
        Vec3::new(x as f32, y as f32, z as f32),
        (yaw as f32).to_radians(),
        (pitch as f32).to_radians(),
    );
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    {
        c.speed = speed.max(0.0) as usize;
    }
    Some(c)
}

/// The text `left` is kept as.
#[must_use]
pub fn text(left: &Left) -> String {
    let mut s =
        String::from("# Where Lyceum left this world: the camera, the tool and the dungeon.\n");
    s.push_str(&format!("camera = {}\n", camera_value(&left.camera)));
    s.push_str(&format!("mode = \"{}\"\n", left.mode));
    if let Some((lb, back)) = &left.dungeon {
        s.push_str(&format!("dungeon = 0x{lb:04X}\n"));
        s.push_str(&format!("back = {}\n", camera_value(back)));
    }
    s
}

/// Where world `text` says it was left; `None` for a file that does not read.
#[must_use]
pub fn read_text(text: &str) -> Option<Left> {
    let t: toml::Table = toml::from_str(text).ok()?;
    let camera = camera_of(t.get("camera")?)?;
    let mode = t
        .get("mode")
        .and_then(toml::Value::as_str)
        .unwrap_or("select")
        .to_owned();
    let dungeon = t
        .get("dungeon")
        .and_then(toml::Value::as_integer)
        .and_then(|n| u16::try_from(n).ok())
        .map(|lb| {
            let back = t.get("back").and_then(camera_of).unwrap_or(camera);
            (lb, back)
        });
    Some(Left {
        camera,
        mode,
        dungeon,
    })
}

/// Where world `world` was left, when it was.
#[must_use]
pub fn read(world: &Path) -> Option<Left> {
    read_text(&std::fs::read_to_string(file(world)).ok()?)
}

/// Keep where world `world` is left.
///
/// # Errors
/// The file would not write.
pub fn write(world: &Path, left: &Left) -> Result<(), String> {
    let path = file(world);
    std::fs::write(&path, text(left)).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's place in a world, kept)
    use super::*;

    #[test]
    fn a_world_left_in_a_dungeon_reads_back_its_camera_tool_and_the_way_out() {
        let mut camera = FlyCamera::new(Vec3::new(41570.5, 34650.25, 112.0), 0.5, -0.25);
        camera.speed = 4;
        let back = FlyCamera::new(Vec3::new(100.0, 200.0, 50.0), 1.0, 0.0);
        let left = Left {
            camera,
            mode: "spawn".into(),
            dungeon: Some((0x0163, back)),
        };
        let again = read_text(&text(&left)).expect("it reads");
        assert_eq!(again.mode, "spawn");
        assert_eq!(again.dungeon.map(|(lb, _)| lb), Some(0x0163));
        let close = |a: &FlyCamera, b: &FlyCamera| {
            (a.position.x - b.position.x).abs() < 0.01
                && (a.position.z - b.position.z).abs() < 0.01
                && (a.yaw - b.yaw).abs() < 1e-3
                && (a.pitch - b.pitch).abs() < 1e-3
                && a.speed == b.speed
        };
        assert!(close(&again.camera, &left.camera), "{again:?}");
        assert!(close(&again.dungeon.unwrap().1, &back));
        // Outdoors, nothing of a dungeon.
        let out = Left {
            dungeon: None,
            ..left
        };
        assert_eq!(read_text(&text(&out)).unwrap().dungeon, None);
        assert_eq!(read_text("not = [toml"), None);
    }
}
