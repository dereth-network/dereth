//! The views behind the screens before the world: where the camera stands, which way it looks
//! and at what hour of a clear day. A world names its own in its overlay folder
//! ([`FILE`](crate::vista::FILE)), one or several; with several, each run of the client shows one
//! of them, picked at random when it starts ([`pick`]). A world that names none is shown from the
//! interface's own block, as before.
//!
//! The file is TOML, one view:
//!
//! ```toml
//! version = 1
//! # Where the camera stands and which way it looks, in any form explore mode reads a location:
//! # its camera field, a report line, or `@loc`'s text.
//! camera = "0xA9B4 [96 40 140] yaw 0.5 pitch -0.2"
//! # Where in the day, 0 to 1 (0.5 is noon); mid-morning without it.
//! time_of_day = 0.4
//! ```
//!
//! or several, each a `[[vista]]` table of the same two keys:
//!
//! ```toml
//! version = 1
//!
//! [[vista]]
//! camera = "0xA9B4 [96 40 140] yaw 0.5 pitch -0.2"
//!
//! [[vista]]
//! camera = "0xA8B3 [60 150 120] yaw 2.5 pitch -0.3"
//! time_of_day = 0.7
//! ```

use dereth_dat::RetailDatStore;

use crate::report::ReportCamera;

/// The file in a world's overlay folder that names the views behind its character select.
pub const FILE: &str = "vista.toml";

/// A view a world names behind its character select.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vista {
    /// Where the camera stands and which way it looks, measured from its block.
    pub camera: ReportCamera,
    /// Where in a clear day the view stands, `[0, 1)`; `None`: the interface's own hour.
    pub time_of_day: Option<f32>,
}

/// The views a vista file's `text` names, in its order: one or more.
///
/// # Errors
/// The text is not TOML or is not version 1; it names no view, or both one view at its top and a
/// list; a view names no camera or one that does not read, or a time of day outside 0 to 1; or it
/// has a key it does not know.
pub fn parse(text: &str) -> Result<Vec<Vista>, String> {
    let doc: toml::Table = toml::from_str(text).map_err(|e| format!("the vista: {e}"))?;
    if let Some(key) = doc
        .keys()
        .find(|k| !matches!(k.as_str(), "version" | "camera" | "time_of_day" | "vista"))
    {
        return Err(format!("the vista has a key it does not know: {key}"));
    }
    match doc.get("version").and_then(toml::Value::as_integer) {
        Some(1) => {}
        Some(v) => return Err(format!("the vista is version {v}, not 1")),
        None => return Err("the vista names no version".to_owned()),
    }
    let Some(list) = doc.get("vista") else {
        return Ok(vec![view(&doc)?]);
    };
    if doc.contains_key("camera") || doc.contains_key("time_of_day") {
        return Err("the vista names one view at its top and a list of them".to_owned());
    }
    let list = list
        .as_array()
        .filter(|a| !a.is_empty())
        .ok_or("the vista's list is not one or more [[vista]] tables")?;
    list.iter()
        .map(|v| {
            let table = v.as_table().ok_or("a vista in the list is not a table")?;
            if let Some(key) = table
                .keys()
                .find(|k| !matches!(k.as_str(), "camera" | "time_of_day"))
            {
                return Err(format!(
                    "a vista in the list has a key it does not know: {key}"
                ));
            }
            view(table)
        })
        .collect()
}

/// One view: its `camera` and its `time_of_day`.
fn view(table: &toml::Table) -> Result<Vista, String> {
    let camera = table
        .get("camera")
        .and_then(toml::Value::as_str)
        .ok_or("the vista names no camera")?;
    // The camera field may come without its name in front; a location of another form is read
    // as explore mode reads it.
    let camera = crate::explore::parse_start(&format!("camera {camera}"))
        .or_else(|_| crate::explore::parse_start(camera))
        .map_err(|e| format!("the vista: {e}"))?;
    let time_of_day = match table.get("time_of_day") {
        None => None,
        Some(v) => {
            // LINT-OK: a fraction of a day, 0 to 1.
            #[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
            let t = v
                .as_float()
                .or_else(|| v.as_integer().map(|i| i as f64))
                .map(|t| t as f32)
                .filter(|t| (0.0..1.0).contains(t))
                .ok_or("the vista's time_of_day is not a number from 0 up to 1")?;
            Some(t)
        }
    };
    Ok(Vista {
        camera,
        time_of_day,
    })
}

/// The views the world `store` holds names behind its character select: its overlay folder's
/// [`FILE`]. Empty when it has no overlay, no such file, or one that does not read (which is
/// logged).
#[must_use]
pub fn of_world(store: &RetailDatStore) -> Vec<Vista> {
    let Some(text) = store
        .overlay_dir()
        .and_then(|dir| dir.folder().open(FILE).ok())
        .and_then(|file| crate::map_places::read_whole(&*file))
    else {
        return Vec::new();
    };
    parse(&text).unwrap_or_else(|e| {
        tracing::warn!("the world's view behind character select is not used: {e}");
        Vec::new()
    })
}

/// The view of `views` a run whose seed is `seed` shows: the same seed always picks the same one,
/// and seeds spread over all of them. `None` when there are none.
#[must_use]
pub fn pick(views: &[Vista], seed: u64) -> Option<Vista> {
    // The seed's bits mixed (SplitMix64's finish), so seeds that differ a little pick apart.
    let mut z = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    let n = u64::try_from(views.len()).ok().filter(|n| *n > 0)?;
    views.get(usize::try_from(z % n).ok()?).copied()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the view behind character select is this client's own interface)
    use super::*;

    /// A vista reads its camera in explore mode's form and its hour; without an hour it leaves
    /// the interface's.
    #[test]
    fn a_vista_reads_its_camera_and_its_hour() {
        let [v] = parse(
            "version = 1\ncamera = \"0x1D2C [96 40 140.5] yaw 0.5 pitch -0.2\"\ntime_of_day = 0.7\n",
        )
        .expect("reads")[..] else {
            panic!("one view")
        };
        assert_eq!(v.camera.block, 0x1D2C);
        assert_eq!(
            (v.camera.origin.x, v.camera.origin.y, v.camera.origin.z),
            (96.0, 40.0, 140.5)
        );
        assert_eq!((v.camera.yaw, v.camera.pitch), (0.5, -0.2));
        assert_eq!(v.time_of_day, Some(0.7));
        let v = parse("version = 1\ncamera = \"camera 0x1D2C [96 40 140] yaw 0 pitch 0\"\n")
            .expect("reads");
        assert_eq!(v[0].time_of_day, None);
    }

    /// A camera measured past its block's edge is moved into the block it stands in.
    #[test]
    fn a_vista_camera_past_its_block_is_measured_from_the_block_it_stands_in() {
        let v =
            parse("version = 1\ncamera = \"0x1D2C [200 -10 140] yaw 0 pitch 0\"\n").expect("reads");
        assert_eq!(v[0].camera.block, 0x1E2B);
        assert_eq!((v[0].camera.origin.x, v[0].camera.origin.y), (8.0, 182.0));
    }

    /// A vista with no camera, another version, an hour past the day or a key of another kind is
    /// refused rather than half read.
    #[test]
    fn a_vista_that_does_not_read_whole_is_refused() {
        for text in [
            "version = 1\n",
            "version = 2\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\n",
            "camera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\n",
            "version = 1\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\ntime_of_day = 1.5\n",
            "version = 1\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\nheight = 3\n",
            "version = 1\ncamera = \"over there\"\n",
            "version = 1\nvista = []\n",
            "version = 1\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\n[[vista]]\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\n",
            "version = 1\n[[vista]]\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\n[[vista]]\ntime_of_day = 0.5\n",
            "version = 1\n[[vista]]\ncamera = \"0x1D2C [96 40 140] yaw 0 pitch 0\"\nheight = 3\n",
        ] {
            assert!(parse(text).is_err(), "{text:?} read");
        }
    }

    /// Three views in a list, in their order.
    const THREE: &str = "version = 1
[[vista]]
camera = \"0x1D2C [10 20 30] yaw 0 pitch 0\"
[[vista]]
camera = \"0x1D2D [40 50 60] yaw 1 pitch 0\"
time_of_day = 0.7
[[vista]]
camera = \"0x1D2E [70 80 90] yaw 2 pitch 0\"
";

    /// A list reads each view, in its order, each with its own hour or none.
    #[test]
    fn a_vista_list_reads_each_view_in_its_order() {
        let views = parse(THREE).expect("reads");
        let blocks: Vec<u16> = views.iter().map(|v| v.camera.block).collect();
        assert_eq!(blocks, [0x1D2C, 0x1D2D, 0x1D2E]);
        let hours: Vec<Option<f32>> = views.iter().map(|v| v.time_of_day).collect();
        assert_eq!(hours, [None, Some(0.7), None]);
    }

    /// Each view of a list is picked for some seed, the same seed picks the same view, a lone
    /// view is always picked, and with none there is nothing to pick.
    #[test]
    fn each_view_is_picked_for_some_seed_and_a_seed_always_picks_the_same() {
        let views = parse(THREE).expect("reads");
        for want in &views {
            assert!(
                (0..64).any(|seed| pick(&views, seed).as_ref() == Some(want)),
                "{:#06X} is never picked",
                want.camera.block
            );
        }
        for seed in [0, 1, 7, 0xDEAD_BEEF, u64::MAX] {
            assert_eq!(pick(&views, seed), pick(&views, seed), "seed {seed}");
        }
        let one = &views[1..2];
        assert!((0..64).all(|seed| pick(one, seed) == Some(views[1])));
        assert_eq!(pick(&[], 5), None);
    }
}
