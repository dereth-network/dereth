//! Read-only labels and colour samples for the world's creation controls.
use dereth_assets::tables::{ObjDesc, SexCg};
use dereth_chargen::CreationTables;
use dereth_primitives::{AssetSource, DataId};
use std::{collections::BTreeMap, rc::Rc};

#[derive(Debug, Clone, Default)]
pub struct ClothingColor {
    pub key: u32,
    pub palette_set: u32,
}
#[derive(Debug, Clone, Default)]
pub struct Named {
    pub name: String,
}
#[derive(Debug, Clone, Default)]
pub struct Template {
    pub name: String,
    pub icon: u32,
    pub resource: u32,
}
#[derive(Debug, Clone, Default)]
pub struct Strip {
    pub icon: u32,
    pub bald: u8,
    pub texture: String,
    pub bald_texture: String,
}
#[derive(Debug, Clone, Default)]
pub struct Sex {
    pub key: u32,
    pub name: String,
    pub icon: u32,
    pub templates: Vec<Template>,
    pub base_palette: u32,
    pub skin_palette: u32,
    pub hair_colors: Vec<u32>,
    pub eye_colors: Vec<u32>,
    pub hair_styles: Vec<Strip>,
    pub eyes: Vec<Strip>,
    pub noses: Vec<Strip>,
    pub mouths: Vec<Strip>,
    pub headgear: Vec<Named>,
    pub shirts: Vec<Named>,
    pub trousers: Vec<Named>,
    pub footwear: Vec<Named>,
}
impl Sex {
    pub fn clothes(&self, i: usize) -> &[Named] {
        match i {
            0 => &self.headgear,
            1 => &self.shirts,
            2 => &self.trousers,
            _ => &self.footwear,
        }
    }
}
#[derive(Debug, Clone, Default)]
pub struct Heritage {
    pub key: u32,
    pub name: String,
    pub icon: u32,
    pub description: u32,
    pub animation: u32,
    pub primary_areas: Vec<usize>,
    pub secondary_areas: Vec<usize>,
    pub sexes: Vec<Sex>,
}
#[derive(Debug, Clone, Default)]
pub struct Area {
    pub name: String,
}
#[derive(Debug, Clone, Default)]
pub struct Skill {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub icon: u32,
    pub chargen: i32,
}
#[derive(Debug, Clone, Default)]
pub struct AppearanceData {
    pub palettes: BTreeMap<String, Vec<[u8; 4]>>,
    pub palette_sets: BTreeMap<String, Vec<u32>>,
}
#[derive(Debug, Clone)]
pub struct CreationData {
    pub tables: Rc<CreationTables>,
    pub appearance: AppearanceData,
    pub heritages: Vec<Heritage>,
    pub areas: Vec<Area>,
    pub skills: Vec<Skill>,
    pub help_ids: Vec<u32>,
    pub help_text: BTreeMap<String, String>,
    pub text_heights: BTreeMap<String, i32>,
}
fn texture(desc: &ObjDesc, mirror: bool) -> String {
    desc.texture_changes
        .first()
        .map_or_else(String::new, |(_, _, id)| {
            format!("world:{:08X}{}", id.0, if mirror { "-mirror" } else { "" })
        })
}
impl CreationData {
    /// Project only presentation values; spending and selection remain in the shared model.
    pub fn load(tables: Rc<CreationTables>, store: &dereth_dat::RetailDatStore) -> Self {
        Self::project(tables, Some(store))
    }
    pub fn from_tables(tables: Rc<CreationTables>) -> Self {
        Self::project(tables, None)
    }
    fn project(tables: Rc<CreationTables>, store: Option<&dereth_dat::RetailDatStore>) -> Self {
        let mut out = Self {
            tables: Rc::clone(&tables),
            appearance: Default::default(),
            heritages: vec![],
            areas: tables
                .chargen
                .starter_areas
                .iter()
                .map(|v| Area {
                    name: v.name.clone(),
                })
                .collect(),
            skills: tables
                .skills
                .skills
                .iter()
                .map(|(&id, s)| Skill {
                    id,
                    name: s.name.clone(),
                    description: s.description.clone(),
                    icon: s.icon,
                    chargen: s.chargen_use,
                })
                .collect(),
            help_ids: tables.chargen.help_strings.iter().map(|v| v.0).collect(),
            help_text: BTreeMap::new(),
            text_heights: BTreeMap::new(),
        };
        for &key in tables.heritage_keys() {
            let Some(h) = tables.chargen.heritage_groups.get(&key) else {
                continue;
            };
            let animation = match key {
                dereth_chargen::HERITAGE_OLTHOI | dereth_chargen::HERITAGE_OLTHOI_ACID => {
                    use dereth_ui::framework::{DidMapperResolver, LayoutEnum, LayoutEnumResolver};
                    let e = if key == dereth_chargen::HERITAGE_OLTHOI {
                        0x10000011
                    } else {
                        0x10000013
                    };
                    store
                        .and_then(|store| DidMapperResolver::load_group(store, 7).ok())
                        .and_then(|r| r.resolve(LayoutEnum(e)))
                        .map_or(0, |v| v.0)
                }
                _ => 0x03000001,
            };
            let mut heritage = Heritage {
                animation,
                key,
                name: h.name.clone(),
                icon: h.icon,
                description: h.description.map_or(0, |v| v.0),
                primary_areas: h
                    .primary_start_areas
                    .iter()
                    .filter_map(|&v| usize::try_from(v).ok())
                    .collect(),
                secondary_areas: h
                    .secondary_start_areas
                    .iter()
                    .filter_map(|&v| usize::try_from(v).ok())
                    .collect(),
                sexes: vec![],
            };
            for &sex_key in tables.sex_keys(key) {
                let Some(s) = h.sexes.get(&sex_key) else {
                    continue;
                };
                let clothes = |items: &[dereth_assets::tables::GearItem]| {
                    items
                        .iter()
                        .map(|item| Named {
                            name: item.name.clone(),
                        })
                        .collect()
                };
                let sex = Sex {
                    key: sex_key,
                    name: s.name.clone(),
                    icon: s.icon,
                    templates: h
                        .templates
                        .iter()
                        .enumerate()
                        .map(|(i, t)| {
                            let p = h.template_presentation(sex_key, i);
                            Template {
                                name: t.name.clone(),
                                icon: p.map_or(t.icon, |v| v.icon),
                                resource: p.and_then(|v| v.description).map_or(0, |v| v.0),
                            }
                        })
                        .collect(),
                    base_palette: s.base_palette.0,
                    skin_palette: s.skin_palset.0,
                    hair_colors: s.hair_colors.clone(),
                    eye_colors: s.eye_colors.clone(),
                    hair_styles: s
                        .hair_styles
                        .iter()
                        .map(|h| Strip {
                            icon: h.icon,
                            bald: h.bald,
                            ..Default::default()
                        })
                        .collect(),
                    eyes: s
                        .eye_strips
                        .iter()
                        .map(|e| Strip {
                            icon: e.icon,
                            texture: texture(&e.objdesc, true),
                            bald_texture: texture(&e.objdesc_bald, true),
                            ..Default::default()
                        })
                        .collect(),
                    noses: s
                        .nose_strips
                        .iter()
                        .map(|(icon, d)| Strip {
                            icon: *icon,
                            texture: texture(d, false),
                            ..Default::default()
                        })
                        .collect(),
                    mouths: s
                        .mouth_strips
                        .iter()
                        .map(|(icon, d)| Strip {
                            icon: *icon,
                            texture: texture(d, false),
                            ..Default::default()
                        })
                        .collect(),
                    headgear: clothes(&s.headgear),
                    shirts: clothes(&s.shirts),
                    trousers: clothes(&s.pants),
                    footwear: clothes(&s.footwear),
                };
                if let Some(store) = store {
                    out.read_palettes(store, s, &sex);
                    for template in &sex.templates {
                        if template.resource != 0 {
                            if let Ok(bytes) = store.read(DataId(template.resource)) {
                                if let Ok(text) = dereth_classic_dat::creation::decode_string(
                                    template.resource,
                                    &bytes,
                                ) {
                                    out.help_text.insert(template.resource.to_string(), text);
                                }
                            }
                        }
                    }
                }
                heritage.sexes.push(sex);
            }
            out.heritages.push(heritage);
        }
        if let Some(store) = store {
            let ids = out
                .help_ids
                .iter()
                .copied()
                .chain(out.heritages.iter().map(|h| h.description))
                .chain(
                    tables
                        .chargen
                        .heritage_groups
                        .values()
                        .flat_map(|h| h.sexes.values())
                        .filter_map(|s| s.naming_help.map(|v| v.0)),
                );
            for id in ids.filter(|id| *id != 0) {
                if let Ok(bytes) = store.read(DataId(id)) {
                    if let Ok(text) = dereth_classic_dat::creation::decode_string(id, &bytes) {
                        out.help_text.insert(id.to_string(), text);
                    }
                }
            }
        }
        out
    }
    /// Credits are interface text, independent of the active world tables.
    pub fn read_chrome(
        &mut self,
        art: &crate::art::ClassicArt,
        fonts: &crate::renderer::FontMetrics,
    ) {
        for id in [0x3100_0020, 0x3100_0022] {
            if let Some(bytes) = art.portal().get(id) {
                if let Ok(text) = dereth_classic_dat::creation::decode_string(id, &bytes) {
                    if let Some(height) = fonts.text_height("16-7", &text, 400) {
                        self.text_heights.insert(id.to_string(), height);
                    }
                    self.help_text.insert(id.to_string(), text);
                }
            }
        }
    }
    fn read_palettes(&mut self, store: &dereth_dat::RetailDatStore, s: &SexCg, _view: &Sex) {
        let mut palettes = std::collections::BTreeSet::from([s.base_palette.0]);
        palettes.extend(&s.eye_colors);
        let mut sets = std::collections::BTreeSet::from([s.skin_palset.0]);
        sets.extend(&s.hair_colors);
        for id in s
            .headgear
            .iter()
            .chain(&s.shirts)
            .chain(&s.pants)
            .chain(&s.footwear)
            .map(|v| v.clothing_table)
        {
            if let Some(t) = self.tables.clothing.get(&id) {
                for p in t.palette_templates.values() {
                    sets.extend(p.subpalette_effects.iter().map(|e| e.palette_set.0));
                }
            }
        }
        for id in sets.into_iter().filter(|id| *id != 0) {
            if let Ok(bytes) = store.read(DataId(id)) {
                if let Ok(ids) = dereth_classic_dat::appearance::palette_set(&bytes) {
                    palettes.extend(&ids);
                    self.appearance
                        .palette_sets
                        .insert(format!("{id:08X}"), ids);
                }
            }
        }
        for id in palettes.into_iter().filter(|id| *id != 0) {
            if let Ok(bytes) = store.read(DataId(id)) {
                use dereth_assets::Decode;
                if let Ok(palette) =
                    dereth_assets::material::Palette::decode_payload(DataId(id), &bytes)
                {
                    let colors = palette
                        .colors_argb
                        .into_iter()
                        .map(|c| {
                            let [_, r, g, b] = c.to_be_bytes();
                            [r, g, b, 255]
                        })
                        .collect();
                    self.appearance.palettes.insert(format!("{id:08X}"), colors);
                }
            }
        }
    }
}
