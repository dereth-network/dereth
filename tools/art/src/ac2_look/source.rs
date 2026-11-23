//! The player's Asheron's Call 2 files, open: shared with Lyceum through `dereth-ac2-convert`.

pub use dereth_ac2_convert::source::*;

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 look's reading)
    use super::*;

    /// Every picture the look takes from this machine's AC2 install (each ground's, its detail
    /// picture, the road's, every tree, rock and cover's, the clouds) is its list's largest level.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn every_picture_the_look_takes_is_its_largest_level() {
        let t = super::super::tables::Tables::built_in().expect("the tables");
        let ac2 = Ac2::open(&auberean_dat::testing::require_target_install()).expect("opens");
        let region = ac2.region().expect("the region");
        let mut lists = std::collections::BTreeSet::new();
        let mut group = |g: &auberean_assets::region::MaterialGroup| {
            for m in g.base.iter().chain(&g.detail) {
                lists.insert(ac2.ground_picture(m.material.0));
            }
        };
        for g in &t.terrain.ground {
            let kind = region.terrain.terrains.iter().find(|k| k.index == g.kind);
            let surface = kind.and_then(|k| {
                region
                    .surface
                    .surfaces
                    .iter()
                    .find(|s| s.index == k.surface)
            });
            for grp in &surface.expect("the kind's surface").groups {
                group(grp);
            }
        }
        for r in &region.surface.roads {
            for grp in &r.groups {
                group(grp);
            }
        }
        let entities = t
            .trees
            .model
            .iter()
            .chain(&t.rocks.model)
            .map(|m| m.entity)
            .chain(t.trees.cover.iter().map(|c| c.entity));
        for e in entities {
            for part in ac2.meshes(e).expect("the entity's meshes") {
                lists.extend(part.materials.iter().filter_map(|mi| ac2.diffuse(*mi)));
            }
        }
        lists.insert(t.sky.clouds);
        lists.remove(&0);
        assert!(lists.len() > 70, "{}", lists.len());
        for list in lists {
            let levels = ac2.levels(list);
            let largest = levels.iter().map(|(w, h)| w * h).max().expect("a level");
            let p = ac2
                .picture(list, t.terrain.picture_limit)
                .unwrap_or_else(|| panic!("{list:08X} gives no picture"));
            assert_eq!(p.width * p.height, largest, "{list:08X}: {levels:?}");
        }
    }
}
