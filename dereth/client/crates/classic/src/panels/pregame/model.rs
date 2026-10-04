//! Read-only control values projected from the shared creation state.
use super::data::*;
use dereth_chargen::{Attr, CharGenState};
use std::collections::BTreeMap;

pub use dereth_presentation::social::format_name;

pub(super) struct SelectionView<'a> {
    pub source: &'a CharGenState,
    pub heritage: usize,
    pub sex: usize,
    pub template: usize,
    pub attrs: [i32; 6],
    pub skills: BTreeMap<u32, i32>,
    pub styles: [usize; 4],
    pub colors: [usize; 4],
    pub shades: [f64; 4],
    pub face: [usize; 3],
    pub hair_style: usize,
    pub hair_color: usize,
    pub eye_color: usize,
    pub skin_shade: f64,
    pub hair_shade: f64,
    pub area: usize,
}
fn index(v: i32) -> usize {
    usize::try_from(v).unwrap_or(usize::MAX)
}
impl<'a> SelectionView<'a> {
    pub fn new(s: &'a CharGenState, d: &CreationData) -> Self {
        let heritage = d
            .heritages
            .iter()
            .position(|v| v.key == s.heritage_group)
            .unwrap_or(0);
        let sex = d
            .heritages
            .get(heritage)
            .and_then(|v| v.sexes.iter().position(|v| v.key == s.gender))
            .unwrap_or(0);
        Self {
            source: s,
            heritage,
            sex,
            template: index(s.template),
            attrs: Attr::BALANCE_ORDER.map(|a| s.get(a)),
            skills: s
                .skill_levels
                .iter()
                .enumerate()
                .filter_map(|(i, v)| u32::try_from(i).ok().map(|i| (i, *v as i32)))
                .collect(),
            styles: [
                s.headgear_style,
                s.shirt_style,
                s.trousers_style,
                s.footwear_style,
            ]
            .map(index),
            colors: [
                s.headgear_color,
                s.shirt_color,
                s.trousers_color,
                s.footwear_color,
            ]
            .map(index),
            shades: [
                s.headgear_shade,
                s.shirt_shade,
                s.trousers_shade,
                s.footwear_shade,
            ],
            face: [s.eyes_strip, s.nose_strip, s.mouth_strip].map(index),
            hair_style: index(s.hair_style),
            hair_color: index(s.hair_color),
            eye_color: index(s.eye_color),
            skin_shade: s.skin_shade,
            hair_shade: s.hair_shade,
            area: usize::try_from(s.start_area).unwrap_or(0),
        }
    }
    pub fn sex<'d>(&self, d: &'d CreationData) -> &'d Sex {
        &d.heritages[self.heritage].sexes[self.sex]
    }
    pub fn clothing_colors(&self, _d: &CreationData, i: usize) -> Vec<ClothingColor> {
        let s = self.source;
        let (ids, sets) = match i {
            0 => (&s.headgear_palette_template_ids, &s.headgear_pal_set_ids),
            1 => (&s.shirt_palette_template_ids, &s.shirt_pal_set_ids),
            2 => (&s.trousers_palette_template_ids, &s.trousers_pal_set_ids),
            _ => (&s.footwear_palette_template_ids, &s.footwear_pal_set_ids),
        };
        ids.iter()
            .zip(sets)
            .map(|(&key, set)| ClothingColor {
                key,
                palette_set: set.0,
            })
            .collect()
    }
    pub fn remaining_attributes(&self, _d: &CreationData) -> i32 {
        self.source.remaining_atrb_credits
    }
    pub fn cost(&self, d: &CreationData, id: u32, level: i32) -> i32 {
        let (trained, specialized) = CharGenState::skill_costs(
            &d.tables.chargen,
            &d.tables.skills,
            self.source.heritage_group,
            id,
        );
        match level {
            2 => trained,
            3 => specialized,
            _ => 0,
        }
    }
    pub fn skill_value(&self, d: &CreationData, s: &Skill) -> i32 {
        self.source.skill_score(&d.tables.skills, s.id)
    }
}
