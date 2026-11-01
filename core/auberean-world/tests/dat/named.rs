//! The records the September 2013 region's descriptions name.

use auberean_assets::region::{
    DayDesc, DetailMap, EncounterDesc, FogDesc, PropertyDesc, Region, SceneDesc, SkyDesc,
    SoundDesc, SurfaceDesc, TerrainDesc, WaterDesc,
};
use auberean_assets::visual::Scene;
use auberean_dat::QualifiedIds;
use auberean_world::records::REGION_ID;

use crate::common::need_world;

/// The region names its map, its landscape constants and every description it lists; each
/// description names its block maps, materials, pictures, days, the calendar and scenes; each day
/// its sky models and flare pictures; each scene its objects. Every record so named is in the data
/// but the world map's picture (none of the portal, cell and high-resolution files holds it), read
/// from the region down: the region, its descriptions, every day of the sky and every scene the
/// scenery kinds name.
///
/// Behaviour: world.region.every-record-a-description-names-is-there
#[test]
fn every_record_a_description_names_is_there() {
    let w = need_world!();
    let r = &w.records;
    let region: Region = r.get(REGION_ID).expect("the region");
    let mut named = QualifiedIds::new();
    region.sub_ids(&mut named);
    let first = |list: &[auberean_assets::region::IndexedRecord]| {
        list.first().map(|x| x.record.0).expect("a record listed")
    };
    let mut kinds = 0;
    macro_rules! described {
        ($list:expr, $t:ty) => {
            for e in $list.iter() {
                let d: $t = r.get(e.record.0).expect("the description decodes");
                d.sub_ids(&mut named);
                kinds += 1;
            }
        };
    }
    described!(region.sounds, SoundDesc);
    described!(region.scenes, SceneDesc);
    described!(region.terrains, TerrainDesc);
    described!(region.surfaces, SurfaceDesc);
    described!(region.encounters, EncounterDesc);
    described!(region.detail_maps, DetailMap);
    described!(region.skies, SkyDesc);
    described!(region.waters, WaterDesc);
    described!(region.fogs, FogDesc);
    described!(region.properties, PropertyDesc);
    let sky: SkyDesc = r.get(first(&region.skies)).expect("the sky");
    for d in &sky.days {
        let day: DayDesc = r.get(d.day.0).expect("a day decodes");
        day.sub_ids(&mut named);
    }
    let scenes: SceneDesc = r.get(first(&region.scenes)).expect("the scenery kinds");
    let mut scene_ids = QualifiedIds::new();
    scenes.sub_ids(&mut scene_ids);
    for s in scene_ids.iter() {
        let scene: Scene = r.get(s.id).expect("a scene decodes");
        scene.sub_ids(&mut named);
    }
    let missing: Vec<String> = named
        .iter()
        .filter(|q| r.bytes(q.id).is_none())
        .map(|q| format!("{:08X}", q.id))
        .collect();
    assert_eq!(kinds, 10, "one of each description");
    assert!(named.len() > 800, "{} records named", named.len());
    assert_eq!(
        missing,
        ["41100000"],
        "named but not there: only the world map's picture, which none of the portal, cell and          high-resolution files holds"
    );
}
