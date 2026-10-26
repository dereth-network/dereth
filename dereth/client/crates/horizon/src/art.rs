//! The interface art: the kit's ([`dereth_ui_kit::art`]) over the interface's own pieces and
//! fonts, as the host hands them over ([`crate::pieces`]), and the game's own icons, read from the
//! game's data ([`AcArt`]).
//!
//! Every picture the interface draws has a [`TexId`] handed out here, and the drawing asks for
//! its pixels by that id the first time it draws it.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

pub use dereth_ui_kit::art::{
    Art, ArtFiles, ArtSource, Face, Family, GivenFont, Image, Sprite, TexId, TexRef,
};

/// The game's data behind the art: its store, and the icons composed from it, by what they are
/// composed of.
#[derive(Debug)]
struct AcSource {
    store: Arc<dereth_dat::RetailDatStore>,
    composites: Mutex<HashMap<dereth_ui::region::IconRecipe, Option<TexRef>>>,
}

impl ArtSource for AcSource {
    fn icon(&self, did: u32) -> Option<Image> {
        let textures = dereth_scene::textures::TextureStore::new(&self.store);
        textures
            .icon_data(dereth_primitives::DataId(did))
            .ok()
            .filter(|d| d.format == dereth_primitives::TextureFormat::Bgra8)
            .and_then(|d| {
                let (width, height) = (d.width, d.height);
                Some(Image {
                    width,
                    height,
                    bgra: d.levels.into_iter().next()?,
                })
            })
    }

    fn fresh(&self) -> Arc<dyn ArtSource> {
        Arc::new(Self {
            store: Arc::clone(&self.store),
            composites: Mutex::new(HashMap::new()),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The game's data behind `art`, when the interface has put it there.
fn ac_source(art: &Art) -> Option<Arc<dyn ArtSource>> {
    art.source()
        .filter(|s| s.as_any().downcast_ref::<AcSource>().is_some())
}

/// The game's own icons, read from the game's data behind the art: spells, items, effects,
/// components and skills, each as the game's own lists compose it.
pub trait AcArt {
    /// Hand the art the Asheron's Call data, for the game's own icons.
    fn set_ac_store(&self, store: Arc<dereth_dat::RetailDatStore>);
    /// The Asheron's Call picture `did` (a spell, item or effect icon), decoded.
    fn ac_icon(&self, did: u32) -> Option<Sprite>;
    /// An item's picture as the game's item lists compose it: the item type's tile, the item's
    /// underlay, then its icon and overlay with the white of the icon's outline replaced by the
    /// item's effects picture. The composition and the choice of pictures are the game's shared
    /// ones ([`dereth_ui::region::composite`], [`dereth_ui_screens::items::widget::object_recipe_by`]).
    fn ac_item(&self, item: &crate::ui::game::Item) -> Option<Sprite>;
    /// What follows the pointer while `item` is dragged: the game's drag picture, its icon with
    /// the overlay and the effects picture in its outline, on no tile.
    fn ac_item_drag(&self, item: &crate::ui::game::Item) -> Option<Sprite>;
    /// A spell's icon as the game's spellbook composes it: the power level's background under
    /// the icon, the wash its bitfield picks, and its badge.
    fn ac_spell(&self, icon: u32, power: u32, bitfield: u32) -> Option<Sprite>;
    /// A spell component's icon as the game's magic window draws it: its opaque white outline
    /// turned opaque black.
    fn ac_component(&self, icon: u32) -> Option<Sprite>;
    /// A skill's icon with its opaque black ground cleared, so the window shows through it.
    fn ac_skill(&self, icon: u32) -> Option<Sprite>;
    /// The game's icon `icon` as its icon lists draw it, alone, on no tile.
    fn ac_plain_icon(&self, icon: u32) -> Option<Sprite>;
}

impl AcArt for Art {
    fn set_ac_store(&self, store: Arc<dereth_dat::RetailDatStore>) {
        self.set_source(Arc::new(AcSource {
            store,
            composites: Mutex::new(HashMap::new()),
        }));
    }

    fn ac_icon(&self, did: u32) -> Option<Sprite> {
        if did == 0 {
            return None;
        }
        let key = format!("ac:{did:08x}");
        if let Some(hit) = self.lookup(&key) {
            return hit;
        }
        let store = ac_store(self)?;
        self.cached(&key, || {
            dereth_scene::textures::TextureStore::new(&store)
                .bgra8(dereth_primitives::DataId(did))
                .ok()
                .map(|d| Image {
                    width: d.width,
                    height: d.height,
                    bgra: d.pixels.concat(),
                })
        })
    }

    fn ac_item(&self, item: &crate::ui::game::Item) -> Option<Sprite> {
        let (store, recipe) = item_recipe(self, item)?;
        composite(self, &store, recipe)
    }

    fn ac_item_drag(&self, item: &crate::ui::game::Item) -> Option<Sprite> {
        let (store, recipe) = item_recipe(self, item)?;
        composite(self, &store, recipe.drag_surface())
    }

    fn ac_spell(&self, icon: u32, power: u32, bitfield: u32) -> Option<Sprite> {
        if icon == 0 {
            return None;
        }
        let store = ac_store(self)?;
        let lookup =
            |group: u32, value: u32| dereth_client_runtime::assets::enum_did(&*store, group, value);
        let recipe = dereth_ui_screens::items::widget::spell_recipe_by(
            &lookup,
            power,
            Some(dereth_primitives::DataId(icon)),
            bitfield,
        );
        composite(self, &store, recipe)
    }

    fn ac_component(&self, icon: u32) -> Option<Sprite> {
        use dereth_ui::region::SurfaceOp;
        icon_replacing(self, icon, SurfaceOp::OPAQUE_WHITE, SurfaceOp::OPAQUE_BLACK)
    }

    fn ac_skill(&self, icon: u32) -> Option<Sprite> {
        use dereth_ui::region::SurfaceOp;
        icon_replacing(self, icon, SurfaceOp::OPAQUE_BLACK, 0)
    }

    fn ac_plain_icon(&self, icon: u32) -> Option<Sprite> {
        // A colour no icon holds, so nothing is replaced.
        icon_replacing(self, icon, 0x0001_0203, 0x0001_0203)
    }
}

/// The game's store behind `art`.
fn ac_store(art: &Art) -> Option<Arc<dereth_dat::RetailDatStore>> {
    let source = ac_source(art)?;
    let ac = source.as_any().downcast_ref::<AcSource>()?;
    Some(Arc::clone(&ac.store))
}

/// Icon `icon` with every texel of exactly `from` made `to`, made once.
fn icon_replacing(art: &Art, icon: u32, from: u32, to: u32) -> Option<Sprite> {
    let op = dereth_ui::region::SurfaceOp::ReplaceColor { from, to };
    let key = format!("replaced:{icon:08x}:{from:08x}:{to:08x}");
    if let Some(hit) = art.lookup(&key) {
        return hit;
    }
    let store = ac_store(art)?;
    art.cached(&key, || {
        let textures = dereth_scene::textures::TextureStore::new(&store);
        textures
            .icon_data(dereth_primitives::DataId(icon))
            .ok()
            .filter(|d| d.format == dereth_primitives::TextureFormat::Bgra8)
            .and_then(|d| {
                let mut bgra = d.levels.into_iter().next()?;
                for px in bgra.as_chunks_mut::<4>().0 {
                    *px = op.apply(u32::from_le_bytes(*px)).to_le_bytes();
                }
                Some(Image {
                    width: d.width,
                    height: d.height,
                    bgra,
                })
            })
    })
}

/// An item's icon recipe, with the store its layers are read from.
fn item_recipe(
    art: &Art,
    item: &crate::ui::game::Item,
) -> Option<(
    Arc<dereth_dat::RetailDatStore>,
    dereth_ui::region::IconRecipe,
)> {
    let store = ac_store(art)?;
    let decoration = item
        .decoration
        .unwrap_or(dereth_client_contract::view::SlotDecoration {
            icon_id: item.ac_icon.unwrap_or(0),
            icon_underlay_id: item.underlay.map(dereth_primitives::DataId),
            icon_overlay_id: item.overlay.map(dereth_primitives::DataId),
            ..dereth_client_contract::view::SlotDecoration::default()
        });
    let lookup =
        |group: u32, value: u32| dereth_client_runtime::assets::enum_did(&*store, group, value);
    let recipe = dereth_ui_screens::items::widget::object_recipe_by(&lookup, &decoration);
    Some((store, recipe))
}

/// A composed icon, made once per recipe.
fn composite(
    art: &Art,
    store: &dereth_dat::RetailDatStore,
    recipe: dereth_ui::region::IconRecipe,
) -> Option<Sprite> {
    let source = ac_source(art)?;
    let ac = source.as_any().downcast_ref::<AcSource>()?;
    let hit = ac.composites.lock().ok()?.get(&recipe).copied();
    let tex = match hit {
        Some(t) => t,
        None => {
            let textures = dereth_scene::textures::TextureStore::new(store);
            let data = dereth_ui::region::composite(recipe, &|id| textures.icon_data(id).ok());
            let tex = data.and_then(|d| {
                let image = Image {
                    width: d.width,
                    height: d.height,
                    bgra: d.levels.into_iter().next()?,
                };
                let (width, height) = (image.width, image.height);
                Some(TexRef {
                    id: art.add_image(image),
                    scale: 1.0,
                    width,
                    height,
                })
            });
            ac.composites.lock().ok()?.insert(recipe, tex);
            tex
        }
    }?;
    Some(Sprite::whole(&tex))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;

    /// An item's picture is the game's composite: where its icon's outline is white, the
    /// composite shows the item's effects picture and never the white.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn an_item_s_icon_outline_takes_its_effects_picture_and_never_shows_white() {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let textures = dereth_scene::textures::TextureStore::new(&store);
        let white = |d: &dereth_primitives::TextureData| {
            d.levels[0]
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0xFF; 4])
                .count()
        };
        // The first icon in the files whose outline is white.
        let icon = (0x0600_1000..0x0600_4000)
            .map(dereth_primitives::DataId)
            .find(|id| {
                textures.icon_data(*id).ok().is_some_and(|d| {
                    d.width == 32
                        && d.height == 32
                        && d.format == dereth_primitives::TextureFormat::Bgra8
                        && white(&d) > 0
                })
            })
            .expect("an icon with a white outline");
        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let item = crate::ui::game::Item {
            ac_icon: Some(icon.0),
            decoration: Some(dereth_client_contract::view::SlotDecoration {
                icon_id: icon.0,
                obj_type: 1,
                ..dereth_client_contract::view::SlotDecoration::default()
            }),
            ..crate::ui::game::Item::default()
        };
        let sprite = art.ac_item(&item).expect("the composite");
        let image = art.image(sprite.tex).expect("its pixels");
        assert_eq!((image.width, image.height), (32, 32));
        assert_eq!(
            image
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0xFF; 4])
                .count(),
            0,
            "icon {icon:?}: the outline's white was replaced"
        );
        assert!(
            image.bgra.as_chunks::<4>().0.iter().all(|p| p[3] == 0xFF),
            "the item type's tile is under the whole icon"
        );
        // The same item again is the same picture, made once.
        assert_eq!(art.ac_item(&item).map(|s| s.tex), Some(sprite.tex));
    }

    /// A component's icon is drawn with its white outline black, and a dragged item's picture is
    /// its drag picture: no white, and none of the type tile the slot shows under it.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_component_s_outline_is_black_and_a_dragged_item_shows_no_white_and_no_tile() {
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let textures = dereth_scene::textures::TextureStore::new(&store);
        let white = |bgra: &[u8]| {
            bgra.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0xFF; 4])
                .count()
        };
        let icon = (0x0600_1000..0x0600_4000)
            .map(dereth_primitives::DataId)
            .find(|id| {
                textures.icon_data(*id).ok().is_some_and(|d| {
                    d.width == 32
                        && d.height == 32
                        && d.format == dereth_primitives::TextureFormat::Bgra8
                        && white(&d.levels[0]) > 0
                })
            })
            .expect("an icon with a white outline");
        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let component = art.ac_component(icon.0).expect("the component's icon");
        assert_eq!(white(&art.image(component.tex).unwrap().bgra), 0);
        let item = crate::ui::game::Item {
            ac_icon: Some(icon.0),
            decoration: Some(dereth_client_contract::view::SlotDecoration {
                icon_id: icon.0,
                obj_type: 1,
                ..dereth_client_contract::view::SlotDecoration::default()
            }),
            ..crate::ui::game::Item::default()
        };
        let slot = art.image(art.ac_item(&item).unwrap().tex).unwrap();
        let drag = art.image(art.ac_item_drag(&item).unwrap().tex).unwrap();
        assert_eq!(white(&drag.bgra), 0);
        assert_ne!(drag.bgra, slot.bgra, "the drag picture is not the tile's");
    }

    /// A skill's icon is drawn on the window, not on its black ground: none of it stays opaque
    /// black, and what was black is clear.
    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the retail dats: --features retail-dats"
    )]
    fn a_skill_s_icon_has_no_black_ground() {
        use dereth_assets::Decode as _;
        use dereth_primitives::asset::AssetSource as _;
        let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
            panic!(
                "the retail dats are this test's oracle and they are not under {}",
                dereth_dat::testing::dat_dir().display()
            )
        }));
        let id = dereth_primitives::DataId(0x0E00_0004);
        let table = dereth_assets::tables::SkillTable::decode_payload_in(
            store.era_of(id),
            id,
            &store.read(id).expect("the skill table"),
        )
        .expect("the skill table decodes");
        let textures = dereth_scene::textures::TextureStore::new(&store);
        let black = |bgra: &[u8]| {
            bgra.as_chunks::<4>()
                .0
                .iter()
                .filter(|p| **p == [0, 0, 0, 0xFF])
                .count()
        };
        let art = Art::empty();
        art.set_ac_store(Arc::clone(&store));
        let mut checked = 0;
        for skill in table.skills.values().filter(|s| s.icon != 0) {
            let raw = textures
                .icon_data(dereth_primitives::DataId(skill.icon))
                .expect("the skill's icon");
            let was = black(&raw.levels[0]);
            let shown = art.image(art.ac_skill(skill.icon).unwrap().tex).unwrap();
            assert_eq!(black(&shown.bgra), 0, "{}", skill.name);
            let clear = shown
                .bgra
                .as_chunks::<4>()
                .0
                .iter()
                .filter(|p| p[3] == 0)
                .count();
            assert_eq!(clear, was, "{}: what was black is clear", skill.name);
            checked += usize::from(was > 0);
        }
        assert!(
            checked > 10,
            "the skills' icons have black grounds: {checked}"
        );
    }
}
