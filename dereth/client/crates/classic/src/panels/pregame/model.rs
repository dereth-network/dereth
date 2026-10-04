use super::data::*;
use crate::int::{i32_from, u32_from};
use dereth_client_contract::pregame::CharGenResultData;
use dereth_primitives::num::to_i32;
use std::collections::BTreeMap;

#[derive(Debug, Clone)]
pub struct Creation {
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
    pub name: String,
    pub area: usize,
    pub selected_skill: Option<usize>,
    pub spells: Vec<bool>,
    pub heraldry: Option<usize>,
    pub heraldry_color: usize,
    pub rotation_velocity: f32,
    pub zoom_face: bool,
    rotate_attr: usize,
    rng: dereth_primitives::num::rng::CrtRand,
}
impl Default for Creation {
    fn default() -> Self {
        Self {
            heritage: 0,
            sex: 0,
            template: usize::MAX,
            attrs: [50; 6],
            skills: BTreeMap::new(),
            styles: [usize::MAX, 0, 0, 0],
            colors: [0; 4],
            shades: [0.5; 4],
            face: [0; 3],
            hair_style: 0,
            hair_color: 0,
            eye_color: 0,
            skin_shade: 0.5,
            hair_shade: 0.5,
            name: String::new(),
            area: 0,
            selected_skill: None,
            spells: vec![],
            heraldry: None,
            heraldry_color: 7,
            rotation_velocity: 0.0,
            zoom_face: true,
            rotate_attr: 0,
            rng: dereth_primitives::num::rng::CrtRand::new(1),
        }
    }
}
impl Creation {
    pub fn rotate(&mut self, left: bool) {
        // The classic interface's own rounding of two pi, kept so the turn rate matches.
        #[allow(clippy::approx_constant)]
        let speed = 6.283_f32 / 2.5;
        self.rotation_velocity = if left {
            if self.rotation_velocity >= 0.0 {
                -speed
            } else {
                0.0
            }
        } else if self.rotation_velocity <= 0.0 {
            speed
        } else {
            0.0
        };
    }
    pub fn enter_preview_page(&mut self, page: &str) {
        match page {
            "appearance" => self.zoom_face = true,
            "clothing" => self.zoom_face = false,
            _ => {}
        }
    }
    pub fn seed(&mut self, seed: u32) {
        self.rng = dereth_primitives::num::rng::CrtRand::new(seed);
    }
    pub fn sex<'a>(&self, d: &'a CreationData) -> &'a Sex {
        &d.heritages[self.heritage].sexes[self.sex]
    }
    pub fn clothing_colors<'a>(&self, d: &'a CreationData, i: usize) -> &'a [ClothingColor] {
        self.sex(d)
            .clothes(i)
            .get(self.styles[i])
            .map_or(&[], |v| v.colors.as_slice())
    }
    pub fn clothing_color(&self, d: &CreationData, i: usize) -> u32 {
        self.clothing_colors(d, i)
            .get(self.colors[i])
            .map_or(0, |v| v.key)
    }
    pub fn remaining_attributes(&self, d: &CreationData) -> i32 {
        self.sex(d).attribute_credits - self.attrs.iter().sum::<i32>()
    }
    pub fn cost(&self, d: &CreationData, id: u32, level: i32) -> i32 {
        let discount = self.sex(d).skill_discounts.iter().find(|v| v.skill == id);
        let base = d.skills.iter().find(|v| v.id == id);
        match level {
            2 => discount
                .map(|v| v.trained)
                .or_else(|| base.map(|v| v.trained))
                .unwrap_or(i32::MAX / 4),
            3 => discount
                .map(|v| v.specialized)
                .or_else(|| base.map(|v| v.specialized))
                .unwrap_or(i32::MAX / 4),
            _ => 0,
        }
    }
    pub fn skill_credits(&self, d: &CreationData) -> i32 {
        self.sex(d).skill_credits
            - self
                .skills
                .iter()
                .map(|(&id, &level)| self.cost(d, id, level))
                .sum::<i32>()
    }
    pub fn skill_value(&self, s: &Skill) -> i32 {
        let level = self.skills.get(&s.id).copied().unwrap_or(1);
        if level < s.min_level || s.formula[3] == 0 {
            return 0;
        }
        let attribute = |id: u32| match id {
            1 => self.attrs[0],
            2 => self.attrs[1],
            3 => self.attrs[3],
            4 => self.attrs[2],
            5 => self.attrs[4],
            6 => self.attrs[5],
            _ => 0,
        };
        let numerator = s.formula[0] as f32
            + s.formula[1] as f32 * attribute(s.formula[4]) as f32
            + s.formula[2] as f32 * attribute(s.formula[5]) as f32;
        to_i32((numerator / s.formula[3] as f32 + 0.5).floor())
            + match level {
                2 => 5,
                3 => 10,
                _ => 0,
            }
    }
    pub fn reset_skills(&mut self, d: &CreationData) {
        self.skills.clear();
        for s in &d.skills {
            let level = if self.cost(d, s.id, 2) == 0 {
                if self.cost(d, s.id, 3) == 0 {
                    3
                } else {
                    2
                }
            } else {
                1
            };
            self.skills.insert(s.id, level);
        }
    }
    pub fn skill(&mut self, d: &CreationData, id: u32, level: i32) -> bool {
        if !(1..=3).contains(&level) || !d.skills.iter().any(|s| s.id == id && s.chargen != 0) {
            return false;
        }
        let old = *self.skills.get(&id).unwrap_or(&1);
        if level < old && self.cost(d, id, old) == 0 {
            return false;
        }
        if self.cost(d, id, level) > self.skill_credits(d) + self.cost(d, id, old) {
            return false;
        }
        self.skills.insert(id, level);
        true
    }
    pub fn attribute(&mut self, d: &CreationData, index: usize, value: i32) {
        if index >= 6 {
            return;
        }
        // Raising one attribute past the credits left takes the difference from the others,
        // one point at a time in turn.
        self.attrs[index] = value.clamp(10, 100);
        while self.remaining_attributes(d) < 0 {
            if (0..6).all(|j| j == index || self.attrs[j] <= 10) {
                // Nothing left to take: the edited attribute stops at what the credits allow.
                self.attrs[index] = (self.attrs[index] + self.remaining_attributes(d)).max(10);
                break;
            }
            let i = self.rotate_attr;
            self.rotate_attr = (i + 1) % 6;
            if i != index && self.attrs[i] > 10 {
                self.attrs[i] -= 1;
            }
        }
    }
    pub fn apply_template(&mut self, d: &CreationData, index: usize) {
        let Some(t) = self.sex(d).templates.get(index) else {
            return;
        };
        if t.profiles.is_empty() {
            return;
        }
        let pick = (u32::from(self.rng.next_u16()) * u32_from(t.profiles.len()) / 32768) as usize;
        let p = t.profiles[pick].clone();
        self.template = index;
        self.attrs = p.attributes;
        self.reset_skills(d);
        for id in p.trained {
            self.skill(d, id, 2);
        }
        for id in p.specialized {
            self.skill(d, id, 3);
        }
        self.spells = vec![false; self.sex(d).legacy_68.len()];
        for i in p.legacy_third {
            self.spell(d, i as usize, true);
        }
    }
    pub fn constrain(&mut self, d: &CreationData) {
        self.heritage = self.heritage.min(d.heritages.len() - 1);
        self.sex = self.sex.min(d.heritages[self.heritage].sexes.len() - 1);
        let s = self.sex(d);
        let cap = |i: usize, n: usize| i.min(n.saturating_sub(1));
        self.hair_style = cap(self.hair_style, s.hair_styles.len());
        self.hair_color = cap(self.hair_color, s.hair_colors.len());
        self.eye_color = cap(self.eye_color, s.eye_colors.len());
        self.face = [
            cap(self.face[0], s.eyes.len()),
            cap(self.face[1], s.noses.len()),
            cap(self.face[2], s.mouths.len()),
        ];
        for i in 0..4 {
            if i != 0 || self.styles[i] != usize::MAX {
                self.styles[i] = cap(self.styles[i], s.clothes(i).len());
            }
            self.colors[i] = cap(self.colors[i], self.clothing_colors(d, i).len());
        }
        self.heraldry = (!s.legacy_80.is_empty()).then_some(0);
        self.area = d.heritages[self.heritage]
            .primary_areas
            .first()
            .copied()
            .unwrap_or(0);
        self.name = "Enter name".into();
        if self.template < self.sex(d).templates.len() {
            self.apply_template(d, self.template);
        } else {
            self.reset_skills(d);
        }
    }
    pub fn random(&mut self, n: usize, previous: usize) -> usize {
        if n < 2 {
            return 0;
        }
        loop {
            let v = (u32::from(self.rng.next_u16()) * u32_from(n) / 32768) as usize;
            if v != previous {
                return v;
            }
        }
    }
    pub fn quick(&mut self, d: &CreationData) {
        self.heritage = self.random(d.heritages.len(), usize::MAX);
        self.sex = self.random(d.heritages[self.heritage].sexes.len(), usize::MAX);
        self.constrain(d);
        self.face = [usize::MAX; 3];
        self.hair_color = usize::MAX;
        self.eye_color = usize::MAX;
        self.hair_style = usize::MAX;
        self.randomize(d, "appearance");
        self.styles = [usize::MAX; 4];
        self.colors = [usize::MAX; 4];
        // This headgear roll excludes nothing: the usual roll excludes the previous choice,
        // which is None (-1) here.
        let count = self.sex(d).headgear.len();
        self.styles[0] = ((u32::from(self.rng.next_u16()) * u32_from(count + 1) / 32768) as usize)
            .wrapping_sub(1);
        self.colors[0] = self.random(self.clothing_colors(d, 0).len(), usize::MAX);
        self.shades[0] = f64::from(self.rng.next_u16()) / 32767.;
        for i in 1..4 {
            self.styles[i] = self.random(self.sex(d).clothes(i).len(), usize::MAX);
            self.colors[i] = self.random(self.clothing_colors(d, i).len(), usize::MAX);
            self.shades[i] = f64::from(self.rng.next_u16()) / 32767.;
        }
        self.randomize(d, "profession");
        // The quick path applies its chosen profession template a second time.
        self.apply_template(d, self.template);
    }
    pub fn randomize(&mut self, d: &CreationData, page: &str) {
        match page {
            "heritage" => {
                self.heritage = self.random(d.heritages.len(), self.heritage);
                self.constrain(d);
            }
            "sex" => {
                self.sex = self.random(d.heritages[self.heritage].sexes.len(), self.sex);
                self.constrain(d);
            }
            "profession" => {
                let count = self.sex(d).templates.len();
                if count > 1 {
                    let n = self.random(count - 1, self.template.wrapping_sub(1)) + 1;
                    self.apply_template(d, n);
                }
            }
            "appearance" => {
                let s = self.sex(d);
                for (i, n) in [s.eyes.len(), s.noses.len(), s.mouths.len()]
                    .into_iter()
                    .enumerate()
                {
                    if n > 0 {
                        self.face[i] = self.random(n, self.face[i]);
                    }
                }
                self.skin_shade = f64::from(self.rng.next_u16()) / 32767.;
                self.hair_shade = f64::from(self.rng.next_u16()) / 32767.;
                self.hair_color = self.random(s.hair_colors.len(), self.hair_color);
                self.eye_color = self.random(s.eye_colors.len(), self.eye_color);
                self.hair_style = self.random(s.hair_styles.len(), self.hair_style);
            }
            "clothing" => {
                for i in 0..4 {
                    let n = self.sex(d).clothes(i).len();
                    if n > 0 {
                        self.styles[i] = if i == 0 {
                            self.random(n + 1, self.styles[i].wrapping_add(1))
                                .wrapping_sub(1)
                        } else {
                            self.random(n, self.styles[i])
                        };
                    }
                    self.colors[i] = self.random(self.clothing_colors(d, i).len(), self.colors[i]);
                    self.shades[i] = f64::from(self.rng.next_u16()) / 32767.;
                }
            }
            "attributes" => {
                // Three complementary pairs, assigned to attributes without replacement.
                let mut remaining: Vec<_> = (0..6).collect();
                for center in [25, 55, 85] {
                    let delta = i32_from(self.random(15, 15));
                    for value in [center + delta, center - delta] {
                        let pick = (u32::from(self.rng.next_u16()) * u32_from(remaining.len())
                            / 32768) as usize;
                        let i = remaining.remove(pick);
                        self.attrs[i] = value;
                    }
                }
            }
            "skills" => {
                self.reset_skills(d);
                // At most 100 random skill indices, then one ordered pass.
                let n = d
                    .skills
                    .iter()
                    .map(|s| s.id as usize + 1)
                    .max()
                    .unwrap_or(0);
                for iteration in 0..100 {
                    if n == 0 {
                        break;
                    }
                    let id = u32_from(self.random(n, n));
                    match self.skills.get(&id).copied() {
                        Some(1) if iteration & 1 != 0 => {
                            self.skill(d, id, 2);
                        }
                        Some(2) => {
                            self.skill(d, id, 3);
                        }
                        _ => {}
                    }
                    if self.skill_credits(d) == 0 {
                        break;
                    }
                }
                for id in 0..u32_from(n) {
                    if self.skill_credits(d) == 0 {
                        break;
                    }
                    if let Some(level) = self.skills.get(&id).copied() {
                        if level < 3 {
                            self.skill(d, id, level + 1);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    pub fn spell(&mut self, d: &CreationData, index: usize, known: bool) -> bool {
        let s = self.sex(d);
        if index >= s.legacy_68.len() {
            return false;
        }
        let spent: u32 = self
            .spells
            .iter()
            .enumerate()
            .filter(|(_, v)| **v)
            .map(|(i, _)| s.legacy_68[i].resource)
            .sum();
        if known && !self.spells[index] && spent + s.legacy_68[index].resource > s.legacy_60 as u32
        {
            return false;
        }
        self.spells[index] = known;
        true
    }
    pub fn result(&self, d: &CreationData, slot: i32) -> CharGenResultData {
        let colors = std::array::from_fn::<_, 4, _>(|i| self.clothing_color(d, i));
        let mut skills = vec![
            0;
            d.skills
                .iter()
                .map(|s| s.id as usize + 1)
                .max()
                .unwrap_or(1)
        ];
        for (&id, &v) in &self.skills {
            skills[id as usize] = v;
        }
        CharGenResultData {
            heritage_group: u32_from(self.heritage),
            gender: u32_from(self.sex),
            eyes_strip: crate::int::choice_i32(self.face[0]),
            nose_strip: crate::int::choice_i32(self.face[1]),
            mouth_strip: crate::int::choice_i32(self.face[2]),
            hair_color: crate::int::choice_i32(self.hair_color),
            eye_color: crate::int::choice_i32(self.eye_color),
            hair_style: crate::int::choice_i32(self.hair_style),
            headgear_style: crate::int::choice_i32(self.styles[0]),
            headgear_color: colors[0],
            shirt_style: crate::int::choice_i32(self.styles[1]),
            shirt_color: colors[1],
            trousers_style: crate::int::choice_i32(self.styles[2]),
            trousers_color: colors[2],
            footwear_style: crate::int::choice_i32(self.styles[3]),
            footwear_color: colors[3],
            skin_shade: self.skin_shade,
            hair_shade: self.hair_shade,
            headgear_shade: self.shades[0],
            shirt_shade: self.shades[1],
            trousers_shade: self.shades[2],
            footwear_shade: self.shades[3],
            template_num: crate::int::choice_i32(self.template),
            strength: self.attrs[0],
            endurance: self.attrs[1],
            coordination: self.attrs[2],
            quickness: self.attrs[3],
            focus: self.attrs[4],
            self_: self.attrs[5],
            slot,
            class_id: 1,
            skill_advancement_classes: skills,
            name: self.name.clone(),
            start_area: u32_from(self.area),
            is_admin: 0,
            is_envoy: 0,
        }
    }
}

/// A new character's name as the wizard formats it: ASCII punctuation filtering and
/// capitalisation, including Roman numerals.
pub use dereth_presentation::social::format_name;

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    fn data() -> CreationData {
        CreationData {
            heritages: vec![Heritage {
                sexes: vec![Sex {
                    attribute_credits: 330,
                    skill_credits: 8,
                    skill_discounts: vec![Discount {
                        skill: 1,
                        trained: 0,
                        specialized: 4,
                    }],
                    ..Default::default()
                }],
                ..Default::default()
            }],
            skills: vec![
                Skill {
                    id: 1,
                    trained: 4,
                    specialized: 8,
                    chargen: 1,
                    ..Default::default()
                },
                Skill {
                    id: 2,
                    trained: 6,
                    specialized: 10,
                    chargen: 1,
                    ..Default::default()
                },
            ],
            ..Default::default()
        }
    }
    #[test]
    fn balancing_excludes_edited_attribute_and_rotates_reductions() {
        let d = data();
        let mut s = Creation {
            attrs: [55; 6],
            ..Default::default()
        };
        s.attribute(&d, 0, 60);
        assert_eq!(s.attrs, [60, 54, 54, 54, 54, 54]);
        s.attribute(&d, 1, 59);
        assert_eq!(s.attrs, [59, 59, 53, 53, 53, 53]);
        assert_eq!(s.remaining_attributes(&d), 0);
    }
    #[test]
    fn training_refunds_old_total_and_rejects_unaffordable_specialization() {
        let d = data();
        let mut s = Creation::default();
        s.reset_skills(&d);
        assert_eq!(s.skills[&1], 2);
        assert!(s.skill(&d, 2, 2));
        assert!(!s.skill(&d, 2, 3));
        assert_eq!(s.skills[&2], 2);
        assert!(s.skill(&d, 2, 1));
        assert!(s.skill(&d, 1, 3));
        assert_eq!(s.skill_credits(&d), 4);
    }
    #[test]
    fn random_attributes_have_three_complementary_pairs_and_fixed_total() {
        let d = data();
        let mut s = Creation::default();
        s.randomize(&d, "attributes");
        assert_eq!(s.attrs.iter().sum::<i32>(), 330);
        let mut values = s.attrs;
        values.sort();
        assert_eq!(values[0] + values[1], 50);
        assert_eq!(values[2] + values[3], 110);
        assert_eq!(values[4] + values[5], 170);
    }
    #[test]
    fn free_training_cannot_be_removed_and_random_profession_excludes_custom() {
        let mut d = data();
        let profile = Profile {
            attributes: [50; 6],
            ..Default::default()
        };
        d.heritages[0].sexes[0].templates = vec![
            Template {
                profiles: vec![profile.clone()],
                ..Default::default()
            },
            Template {
                profiles: vec![profile],
                ..Default::default()
            },
        ];
        let mut s = Creation::default();
        s.reset_skills(&d);
        assert!(!s.skill(&d, 1, 1));
        s.randomize(&d, "profession");
        assert_eq!(s.template, 1);
        s.randomize(&d, "profession");
        assert_eq!(s.template, 1);
    }
    #[test]
    fn skill_score_uses_minimum_training_formula_rounding_and_training_bonus() {
        let mut s = Creation {
            attrs: [51, 50, 50, 50, 50, 50],
            ..Default::default()
        };
        let skill = Skill {
            id: 1,
            min_level: 2,
            formula: [0, 1, 1, 4, 1, 4],
            ..Default::default()
        };
        assert_eq!(s.skill_value(&skill), 0);
        s.skills.insert(1, 2);
        assert_eq!(s.skill_value(&skill), 30);
        s.skills.insert(1, 3);
        assert_eq!(s.skill_value(&skill), 35);
    }
    #[test]
    fn name_formatter_preserves_roman_numerals_and_prefix_case() {
        assert_eq!(format_name("probe walker"), "Probe walker");
        assert_eq!(format_name("probe Walker"), "Probe Walker");
        assert_eq!(format_name("  tEST123  IV  "), "Test IV");
        assert_eq!(format_name("mAcDonald"), "MacDonald");
        assert_eq!(format_name("'bob--smith'"), "Bob-smith'");
    }
}

#[cfg(test)]
mod preview_controls_tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    #[allow(clippy::approx_constant)] // the interface's own two pi, as `rotate` uses
    fn rotate_buttons_toggle_or_reverse_continuous_angular_velocity() {
        let mut c = Creation::default();
        assert_eq!(c.rotation_velocity, 0.0);
        c.rotate(true);
        assert_eq!(c.rotation_velocity, -6.283_f32 / 2.5);
        c.rotate(true);
        assert_eq!(c.rotation_velocity, 0.0);
        c.rotate(false);
        assert_eq!(c.rotation_velocity, 6.283_f32 / 2.5);
        c.rotate(true);
        assert_eq!(c.rotation_velocity, -6.283_f32 / 2.5);
        c.rotate(false);
        assert_eq!(c.rotation_velocity, 6.283_f32 / 2.5);
        c.rotate(false);
        assert_eq!(c.rotation_velocity, 0.0);
    }
    #[test]
    fn face_and_clothing_entry_choose_their_camera_target() {
        let mut c = Creation::default();
        c.enter_preview_page("clothing");
        assert!(!c.zoom_face);
        c.enter_preview_page("profession");
        assert!(!c.zoom_face);
        c.enter_preview_page("appearance");
        assert!(c.zoom_face);
    }
}
