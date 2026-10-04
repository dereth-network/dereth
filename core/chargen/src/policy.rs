//! Creation entry and random-button policies over shared world rules.

use super::*;

/// Decoded creation rules and the clothing tables those rules reference.
#[derive(Debug)]
pub struct CreationTables {
    pub chargen: CharGen,
    pub skills: SkillTable,
    pub clothing: Rc<BTreeMap<DataId, ClothingTable>>,
}
impl CreationTables {
    #[must_use]
    pub fn heritage_keys(&self) -> &[u32] {
        &self.chargen.heritage_order
    }
    #[must_use]
    pub fn sex_keys(&self, heritage: u32) -> &[u32] {
        self.chargen
            .heritage_groups
            .get(&heritage)
            .map_or(&[], |h| h.sex_order.as_slice())
    }
}

/// Supported interface sequencing; world tables continue to decide valid choices and costs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationPolicy {
    Modern,
    Classic,
}
/// Ordinary wizard entry or quick entry directly to the summary.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationEntry {
    Normal,
    Quick,
}
/// The group of choices changed by a page's Random control.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CreationRandom {
    Heritage,
    Sex,
    Template,
    Appearance,
    Clothing,
    Attributes,
    Skills,
    Town,
}

impl CharGenState {
    #[must_use]
    pub fn with_policy(policy: CreationPolicy) -> Self {
        Self {
            policy,
            ..Self::default()
        }
    }

    /// Start a new creation session using this interface's supported entry sequence.
    pub fn begin_creation(
        &mut self,
        tables: &CreationTables,
        entry: CreationEntry,
        account_has_tod: bool,
    ) {
        self.clothing = Rc::clone(&tables.clothing);
        if self.policy == CreationPolicy::Modern {
            self.randomize_character(&tables.chargen, &tables.skills, account_has_tod);
            return;
        }
        self.reset(&tables.chargen, &tables.skills);
        self.heraldry_color = 7;
        match entry {
            CreationEntry::Normal => self.classic_refresh(tables),
            CreationEntry::Quick => self.classic_quick(tables),
        }
    }

    /// Select a world heritage key, refusing a key the connected world does not define.
    pub fn choose_heritage(&mut self, tables: &CreationTables, key: u32) -> bool {
        if !tables.chargen.heritage_groups.contains_key(&key) {
            return false;
        }
        if self.policy == CreationPolicy::Classic {
            self.classic_heritage(tables, key);
        } else {
            self.set_heritage_group(&tables.chargen, &tables.skills, key);
        }
        true
    }

    /// Select a world sex key within the currently selected heritage.
    pub fn choose_gender(&mut self, tables: &CreationTables, key: u32) -> bool {
        if !tables
            .chargen
            .heritage_groups
            .get(&self.heritage_group)
            .is_some_and(|h| h.sexes.contains_key(&key))
        {
            return false;
        }
        if self.policy == CreationPolicy::Classic {
            self.classic_gender(tables, key);
        } else {
            self.set_gender(&tables.chargen, key);
        }
        true
    }

    /// Select a template. Classic resolves the explicit -1 Custom request to world template 0;
    /// Modern retains its unselected sentinel.
    pub fn choose_template(&mut self, tables: &CreationTables, index: i32) -> bool {
        let index = if index == -1 && self.policy == CreationPolicy::Classic {
            0
        } else {
            index
        };
        if index != -1
            && !usize::try_from(index).ok().is_some_and(|i| {
                tables
                    .chargen
                    .heritage_groups
                    .get(&self.heritage_group)
                    .is_some_and(|h| i < h.templates.len())
            })
        {
            return false;
        }
        self.template = index;
        if self.template != -1 {
            self.apply_policy_template(tables);
        }
        true
    }

    /// Randomize the indicated page without replacing the shared mutation rules.
    pub fn randomize_page(
        &mut self,
        tables: &CreationTables,
        target: CreationRandom,
        account_has_tod: bool,
    ) {
        let cg = &tables.chargen;
        let skills = &tables.skills;
        match target {
            CreationRandom::Heritage if self.policy == CreationPolicy::Modern => {
                self.randomize_heritage_group(cg, skills, account_has_tod)
            }
            CreationRandom::Heritage => self.classic_random_heritage(tables),
            CreationRandom::Sex => self.classic_random_gender(tables),
            CreationRandom::Template => {
                self.policy_random_template(tables);
                if self.policy == CreationPolicy::Classic {
                    self.apply_policy_template(tables);
                }
            }
            CreationRandom::Appearance => self.randomize_appearance(cg, false),
            CreationRandom::Clothing => {
                self.randomize_clothing(cg, self.policy == CreationPolicy::Classic)
            }
            CreationRandom::Attributes => self.randomize_attributes(),
            CreationRandom::Skills => {
                let range = if self.policy == CreationPolicy::Classic {
                    skills
                        .skills
                        .keys()
                        .next_back()
                        .and_then(|id| usize::try_from(*id).ok())
                        .map_or(0, |n| n + 1)
                } else {
                    TOTAL_NUM_SKILLS
                };
                self.randomize_skills_in_range(cg, skills, range);
            }
            CreationRandom::Town => self.randomize_start_area(cg),
        }
    }

    /// Complete an unvisited clothing page without changing choices made on an earlier visit.
    pub fn prepare_clothing(&mut self, tables: &CreationTables) {
        if self.shirt_style == -1 {
            self.randomize_clothing(&tables.chargen, self.policy == CreationPolicy::Classic);
        }
    }

    /// Fill choices not yet made before presenting a summary, without rerolling complete choices.
    pub fn prepare_summary(&mut self, tables: &CreationTables) {
        if self.policy == CreationPolicy::Classic {
            self.classic_summary(tables);
        }
    }
}

impl CharGenState {
    fn apply_policy_template(&mut self, tables: &CreationTables) {
        let valid = self.sex(&tables.chargen).is_some()
            && usize::try_from(self.template).ok().is_some_and(|i| {
                tables
                    .chargen
                    .heritage_groups
                    .get(&self.heritage_group)
                    .is_some_and(|h| i < h.templates.len())
            });
        if valid && self.policy == CreationPolicy::Classic {
            // Supported tables contain one profile; a direct draw still advances the stream.
            self.rng.rand_int(1);
        }
        self.apply_template(&tables.chargen, &tables.skills);
    }

    fn classic_heraldry(&mut self, tables: &CreationTables) {
        if self.sex(&tables.chargen).is_some() {
            // The supported tables have no selectable symbols; the color draw remains.
            self.heraldry_color = self.rng.rand_int_excluding(16, self.heraldry_color);
        }
    }

    fn classic_heritage(&mut self, tables: &CreationTables, key: u32) {
        let old_sex = tables
            .sex_keys(self.heritage_group)
            .iter()
            .position(|k| *k == self.gender);
        self.heritage_group = key;
        let hg = &tables.chargen.heritage_groups[&key];
        if self.setup_id != hg.setup {
            self.setup_changed = true;
            self.setup_id = hg.setup;
        }
        // The prior sex ordinal remains in force until the heritage's nested operations finish.
        self.gender = old_sex
            .and_then(|i| hg.sex_order.get(i))
            .copied()
            .unwrap_or(0);
        self.apply_policy_template(tables);
        self.name.clear();
        self.randomize_start_area(&tables.chargen);
        self.classic_heraldry(tables);
        let sex = old_sex
            .and_then(|i| {
                hg.sex_order
                    .get(i.min(hg.sex_order.len().saturating_sub(1)))
            })
            .copied()
            .unwrap_or(0);
        self.classic_gender(tables, sex);
    }

    fn classic_gender(&mut self, tables: &CreationTables, key: u32) {
        self.gender = key;
        if self.sex(&tables.chargen).is_some() {
            let hg = &tables.chargen.heritage_groups[&self.heritage_group];
            self.total_atrb_credits = i32::try_from(hg.attribute_credits).unwrap_or(0);
            self.recompute_remaining();
            self.total_skill_credits = i32::try_from(hg.skill_credits).unwrap_or(0);
            self.remaining_skill_credits = self.total_skill_credits;
            self.apply_policy_template(tables);
            self.name.clear();
            self.classic_heraldry(tables);
            self.constrain_all_by_heritage(&tables.chargen, &tables.skills);
        } else {
            self.template = -1;
            self.total_atrb_credits = 0;
            self.recompute_remaining();
            self.total_skill_credits = 0;
            self.remaining_skill_credits = 0;
            self.name.clear();
            self.start_area = u32::MAX;
            self.apply_default_template(&tables.chargen, &tables.skills);
        }
        self.set_gender(&tables.chargen, key);
    }

    fn classic_random_heritage(&mut self, tables: &CreationTables) {
        let keys = tables.heritage_keys();
        if keys.is_empty() {
            return;
        }
        let previous = keys
            .iter()
            .position(|k| *k == self.heritage_group)
            .map_or(-1, |i| i32::try_from(i).unwrap_or(-1));
        let index = self.rng.rand_int_excluding(len_i32(keys.len()), previous);
        if let Some(&key) = usize::try_from(index).ok().and_then(|i| keys.get(i)) {
            self.classic_heritage(tables, key);
        }
    }

    fn classic_random_gender(&mut self, tables: &CreationTables) {
        let keys = tables.sex_keys(self.heritage_group);
        if keys.is_empty() {
            return;
        }
        let previous = keys
            .iter()
            .position(|k| *k == self.gender)
            .map_or(-1, |i| i32::try_from(i).unwrap_or(-1));
        let index = self.rng.rand_int_excluding(len_i32(keys.len()), previous);
        if let Some(&key) = usize::try_from(index).ok().and_then(|i| keys.get(i)) {
            if self.policy == CreationPolicy::Classic {
                self.classic_gender(tables, key);
            } else {
                self.set_gender(&tables.chargen, key);
            }
        }
    }

    fn policy_random_template(&mut self, tables: &CreationTables) {
        if self.policy == CreationPolicy::Modern {
            self.randomize_template(&tables.chargen, &tables.skills);
            return;
        }
        let Some(hg) = tables.chargen.heritage_groups.get(&self.heritage_group) else {
            return;
        };
        if self.sex(&tables.chargen).is_none() {
            return;
        }
        if matches!(self.heritage_group, HERITAGE_OLTHOI | HERITAGE_OLTHOI_ACID) {
            self.template = 0;
            self.apply_policy_template(tables);
        } else if hg.templates.len() > 1 {
            self.template = self
                .rng
                .rand_int_excluding(len_i32(hg.templates.len()) - 1, self.template - 1)
                + 1;
            self.apply_policy_template(tables);
        }
    }

    fn classic_refresh(&mut self, tables: &CreationTables) {
        let [heritage, sex, appearance] = self.frozen.map(|f| !f);
        if heritage {
            self.classic_random_heritage(tables);
        }
        if sex {
            self.classic_random_gender(tables);
        }
        if appearance {
            self.randomize_appearance(&tables.chargen, false);
        }
        if heritage || sex {
            self.classic_heraldry(tables);
            self.policy_random_template(tables);
            self.name.clear();
            self.randomize_start_area(&tables.chargen);
        }
    }

    fn classic_quick(&mut self, tables: &CreationTables) {
        self.classic_random_heritage(tables);
        self.classic_random_gender(tables);
        self.randomize_appearance(&tables.chargen, false);
        self.randomize_clothing(&tables.chargen, false);
        self.policy_random_template(tables);
        self.apply_policy_template(tables);
        self.name.clear();
        self.randomize_start_area(&tables.chargen);
        self.classic_heraldry(tables);
        self.frozen = [true; 3];
    }

    fn classic_summary(&mut self, tables: &CreationTables) {
        if self.heritage_group == 0 {
            self.classic_random_heritage(tables);
        }
        self.frozen[0] = true;
        if self.gender == 0 {
            self.classic_random_gender(tables);
        }
        self.frozen[1] = true;
        if self.eyes_strip == -1 {
            self.randomize_appearance(&tables.chargen, false);
        }
        self.frozen[2] = true;
        self.prepare_clothing(tables);
    }

    /// Three complementary pairs assigned without replacement, then constrained by shared locks
    /// and the connected world's budget. Ordinary unlocked 330-credit characters retain each pair.
    pub fn randomize_attributes(&mut self) {
        let mut remaining: Vec<_> = Attr::BALANCE_ORDER.into_iter().collect();
        let mut values = Vec::new();
        for center in [25, 55, 85] {
            let delta = self.rng.rand_int(15);
            for value in [center + delta, center - delta] {
                let index =
                    usize::try_from(self.rng.rand_int(len_i32(remaining.len()))).unwrap_or(0);
                let attribute = remaining.remove(index);
                values.push((attribute, value));
            }
        }
        // Lower unlocked values first to free their credits before assigning the rolled values.
        for &(a, _) in &values {
            if !self.is_locked(a) {
                self.set_attribute_balanced(a, self.atrb_min, false);
            }
        }
        for (a, value) in values {
            if !self.is_locked(a) {
                self.set_attribute(a, value);
            }
        }
    }
}
