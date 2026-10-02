//! Behaviour: none (file-format decoders: the portal's character-creation tables)
//!
//! The creation tables read whole from the portal, and the face textures they name; with the
//! reference exports, every field the creation structs carry equals the exported tables, and
//! every face texture equals the exported indices, dimensions and palettes.

use dereth_classic_dat::creation::{self, CreationData};
use serde_json::Value;

/// `data` as JSON, with the per-font text heights (not a dat field) left out.
fn comparable(mut data: CreationData) -> Value {
    data.text_heights.clear();
    serde_json::to_value(data).expect("creation data serialises")
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the classic portal: DERETH_CLASSIC_PORTAL"
)]
fn the_creation_tables_read_whole_and_equal_the_exported_tables() {
    let Some(portal) =
        crate::portal("the_creation_tables_read_whole_and_equal_the_exported_tables")
    else {
        return;
    };
    let data = creation::read(portal).expect("creation tables decode");
    assert!(!data.heritages.is_empty() && data.heritages.iter().all(|h| !h.sexes.is_empty()));
    assert!(!data.skills.is_empty() && !data.help_text.is_empty());
    let Some(dir) = crate::reference("DERETH_CLASSIC_REFERENCE", "creation table comparison")
    else {
        return;
    };
    let exported: CreationData =
        serde_json::from_value(crate::read_json(&dir.join("pregame.json"))).expect("export parses");
    let (got, want) = (comparable(data), comparable(exported));
    for key in want.as_object().expect("object").keys() {
        assert!(got[key] == want[key], "field {key} differs from the export");
    }
    assert_eq!(got, want);
}

#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the classic portal: DERETH_CLASSIC_PORTAL"
)]
fn the_face_textures_equal_the_exported_indices_and_palettes() {
    let Some(portal) = crate::portal("the_face_textures_equal_the_exported_indices_and_palettes")
    else {
        return;
    };
    let data = creation::read(portal).expect("creation tables decode");
    let assets = creation::indexed_assets(portal, &data).expect("face textures decode");
    assert!(!assets.textures.is_empty());
    let Some(dir) = crate::reference("DERETH_CLASSIC_REFERENCE", "face texture comparison") else {
        return;
    };
    let bundle = crate::read_json(&dir.join("appearance.json"));
    let exported = bundle["indexed_assets"]
        .as_object()
        .expect("indexed assets");
    let keys: Vec<&String> = assets.textures.keys().collect();
    assert_eq!(keys, exported.keys().collect::<Vec<_>>());
    for (key, t) in &assets.textures {
        let row = &exported[key];
        assert_eq!(
            (row["width"].clone(), row["height"].clone()),
            (t.width.into(), t.height.into()),
            "{key}"
        );
        let file = dir.join(row["index_file"].as_str().expect("index file"));
        let want = std::fs::read(&file).unwrap_or_else(|e| panic!("{}: {e}", file.display()));
        assert!(want == t.indices, "{key} indices differ from the export");
    }
    assert_eq!(
        serde_json::to_value(&data.appearance.palettes).unwrap(),
        bundle["palettes"]
    );
    assert_eq!(
        serde_json::to_value(&data.appearance.palette_sets).unwrap(),
        bundle["palette_sets"]
    );
}
