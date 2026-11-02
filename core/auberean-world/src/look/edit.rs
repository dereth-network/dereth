//! A visual description the client keeps and changes: an object's own description, with
//! appearance values added and taken off, other descriptions added to it, taken from it or put in
//! its place, and the appearance its effects lay over it.
//!
//! An object keeps two descriptions: its own (what the server sent, extending a stored
//! description) and the appearance values its running effects apply, as changes to every part
//! group. Its own description always carries the effects' values laid over it; the object draws
//! the total of its own.
//!
//! An appearance value is added to a part group of the description; a group the description lacks
//! is first copied from the description it extends (its key, connection point and parent key
//! only). A value is taken off the description's own group when it has it; when only the
//! description it extends gives it, it is overridden with [`REMOVED`] instead.

use auberean_assets::fx::AppearanceKey;
use auberean_dat::qualified::divine_type;
use auberean_primitives::DataId;

use super::{Desc, Group, Keyed, Look, LookError, LookRecords, GLOBAL_KEY, REMOVED};

/// The type of the stored visual descriptions.
const VISUAL_DESC_TYPE: u32 = 0x4000_0021;

impl Look {
    /// A description as the client copies one into its own: extending what `desc` extends, with
    /// `desc` added and the root found again.
    // D2-13851, D2-13839, D2-13845
    #[must_use]
    pub fn local(desc: &Desc) -> Self {
        let mut l = Self {
            parent: desc.parent,
            ..Self::default()
        };
        let _ = l.apply_additive(desc);
        l.reset_root();
        l
    }

    /// Stored description `id` as the client copies it into its own; `None` when `id` names no
    /// stored visual description.
    // D2-13848
    #[must_use]
    pub fn local_of_record(records: &dyn LookRecords, id: DataId) -> Option<Self> {
        if divine_type(id.0) != VISUAL_DESC_TYPE {
            return None;
        }
        let v = records.visual(id)?;
        Some(Self::local(&Desc::from_stored(&v)))
    }

    /// The description as one to add to another: its fields, its groups in table order, each
    /// hung from its parent.
    #[must_use]
    pub fn to_desc(&self) -> Desc {
        Desc {
            parent: self.parent,
            motion_desc: self.motion_desc,
            behavior_table: self.behavior_table,
            modes: self.modes,
            scale: self.scale,
            child_scale: self.child_scale,
            particle_scale: self.particle_scale,
            global: self.global.clone(),
            groups: self.groups.entries.iter().map(|(_, g)| g.clone()).collect(),
            icon: self.icon.clone(),
        }
    }

    /// Find the root again: the first group in table order that hangs from none (the changes to
    /// every group when that group's key is theirs); none when every group hangs from another.
    // D2-13809
    pub fn reset_root(&mut self) {
        self.root = self
            .groups
            .entries
            .iter()
            .find(|(_, g)| g.parent.is_none())
            .map(|(_, g)| g.key);
    }

    /// Does the description add nothing: no parent, no groups, and no appearance values or
    /// effects for every group?
    // D2-13801
    #[must_use]
    pub fn adds_nothing(&self) -> bool {
        self.parent.is_none()
            && self.root.is_none()
            && self.global.appearance.is_empty()
            && !self.global.has_fx()
    }

    /// The value group `group` gives key `key` of appearance table `apr`, when it gives one.
    // D2-13820
    #[must_use]
    pub fn applied_value(&self, group: u32, apr: u32, key: u32) -> Option<f32> {
        self.group(group)?.applied_value(apr, key)
    }

    /// The appearance tables of group `group` and their values.
    // D2-13821
    #[must_use]
    pub fn appearance_of(&self, group: u32) -> Option<&Keyed<Keyed<f32>>> {
        self.group(group).map(|g| &g.appearance)
    }

    /// The appearance tables of group `group` with copies of their values, as the client gathers
    /// them: each put first, so the last in table order comes first.
    // D2-13813
    #[must_use]
    pub fn gather_appearance(&self, group: u32) -> Vec<(u32, Keyed<f32>)> {
        let Some(g) = self.group(group) else {
            return Vec::new();
        };
        g.appearance
            .entries
            .iter()
            .rev()
            .map(|(apr, values)| (*apr, values.copied()))
            .collect()
    }

    /// Copy group `key` of the description this one extends into this one: its key, connection
    /// point and parent key only, added as [`Look::add_group`] adds one. The groups of this
    /// description that hang from it in the extended one are marked as hanging from the copy (they
    /// are not added to its children). `false` when this description extends none, the extended
    /// one lacks the group, or the copy could not be hung.
    // D2-13829
    fn copy_group_from_parent(&mut self, records: &dyn LookRecords, key: u32) -> bool {
        if self.parent.is_none() {
            return false;
        }
        let Some(v) = records.visual(self.parent) else {
            return false;
        };
        let parent = Desc::from_stored(&v);
        let from = if key == GLOBAL_KEY {
            Some(&parent.global)
        } else {
            parent.groups.iter().find(|g| g.key == key)
        };
        let Some(from) = from else {
            return false;
        };
        let copy = Group {
            key: from.key,
            connection: from.connection,
            parent_key: from.parent_key,
            ..Group::default()
        };
        if self.add_group(&copy).is_err() {
            return false;
        }
        for child in &from.children {
            if let Some(g) = self.group_mut(*child) {
                g.parent = Some(key);
                g.parent_key = key;
            }
        }
        if self.root_group().is_some_and(|r| r.parent.is_some()) {
            self.reset_root();
        }
        true
    }

    /// Add appearance table `apr`'s values `values` to group `group` (copied from the extended
    /// description when this one lacks it). `false` when the group or the table is 0 or the group
    /// is in neither.
    // D2-13831
    pub fn add_appearance(
        &mut self,
        records: &dyn LookRecords,
        group: u32,
        apr: u32,
        values: &Keyed<f32>,
    ) -> bool {
        if group == 0 || apr == 0 {
            return false;
        }
        if self.group(group).is_none() && !self.copy_group_from_parent(records, group) {
            return false;
        }
        match self.group_mut(group) {
            Some(g) => {
                g.augment_appearance(apr, values);
                true
            }
            None => false,
        }
    }

    /// Add one appearance value to group `group`, as [`Look::add_appearance`] adds a table's.
    // D2-13841, D2-13846
    pub fn add_appearance_key(
        &mut self,
        records: &dyn LookRecords,
        group: u32,
        apr: u32,
        value: AppearanceKey,
    ) -> bool {
        if group == 0 || apr == 0 {
            return false;
        }
        let mut values = Keyed::default();
        // D2-13922
        values.set(value.key, value.scalar);
        self.add_appearance(records, group, apr, &values)
    }

    /// Add every appearance table of `tables` to group `group`; `false` when one did not take
    /// (the others are added all the same).
    // D2-13832
    pub fn add_appearance_tables(
        &mut self,
        records: &dyn LookRecords,
        group: u32,
        tables: &Keyed<Keyed<f32>>,
    ) -> bool {
        let mut all = true;
        for (apr, values) in &tables.entries {
            all &= self.add_appearance(records, group, *apr, values);
        }
        all
    }

    /// Take key `key` of appearance table `apr` off group `group`: off this description's group
    /// when it has the table; else, when the extended description gives the key, overridden with
    /// [`REMOVED`]. `false` when neither.
    // D2-13842, D2-13847
    pub fn remove_appearance_key(
        &mut self,
        records: &dyn LookRecords,
        group: u32,
        apr: u32,
        key: u32,
    ) -> bool {
        if group == 0 || apr == 0 {
            return false;
        }
        if self
            .group_mut(group)
            .is_some_and(|g| g.remove_appearance_key(apr, key))
        {
            return true;
        }
        if self.parent.is_none() {
            return false;
        }
        let Some(parent) = records.visual(self.parent) else {
            return false;
        };
        let parent = Desc::from_stored(&parent);
        let given = if group == GLOBAL_KEY {
            Some(&parent.global)
        } else {
            parent.groups.iter().find(|g| g.key == group)
        };
        if given.and_then(|g| g.applied_value(apr, key)).is_none() {
            return false;
        }
        let removed = AppearanceKey {
            key,
            scalar: REMOVED,
        };
        self.add_appearance_key(records, group, apr, removed)
    }

    /// Take group `key` out: unhooked from the group it hangs from, the groups hanging from it
    /// left hanging from none, the root found again when it was the root. `false` when the
    /// description's table has no such group.
    // D2-13810
    pub fn remove_group(&mut self, key: u32) -> bool {
        if self.groups.get(key).is_none() {
            return false;
        }
        self.unhook(key);
        let was_root = self.root == Some(key);
        self.groups.remove(key);
        if was_root {
            self.reset_root();
        }
        true
    }

    /// Unhook group `key` before it goes: off its parent's children, its children hanging from
    /// none.
    // D2-14036, D2-14031
    fn unhook(&mut self, key: u32) {
        let Some(g) = self.group(key) else { return };
        let parent = g.parent;
        let children = g.children.clone();
        if let Some(p) = parent.and_then(|p| self.group_mut(p)) {
            if let Some(i) = p.children.iter().position(|c| *c == key) {
                p.children.remove(i);
            }
        }
        for c in children {
            if let Some(child) = self.group_mut(c) {
                child.parent = None;
            }
        }
    }

    /// Take group `key` out with every group hanging from it, those first. `false` when one of
    /// them is not in the description's table.
    // D2-13822
    pub fn remove_group_tree(&mut self, key: u32) -> bool {
        self.remove_group_tree_at(key, 0)
    }

    fn remove_group_tree_at(&mut self, key: u32, depth: usize) -> bool {
        let Some(g) = self.group(key) else {
            return false;
        };
        let children = g.children.len();
        if depth < 32 {
            for _ in 0..children {
                let Some(first) = self.group(key).and_then(|g| g.children.first().copied()) else {
                    break;
                };
                if !self.remove_group_tree_at(first, depth + 1) {
                    return false;
                }
            }
        }
        self.remove_group(key)
    }

    /// Take another description out of this one: the appearance values it gives every group, then
    /// for each of its groups in table order its appearance values (as
    /// [`Look::remove_appearance_key`] takes one off), or, for a group with no appearance and no
    /// effects, the group and those hanging from it.
    // D2-13852, D2-13843
    pub fn apply_subtractive(&mut self, records: &dyn LookRecords, desc: &Desc) -> bool {
        if desc.adds_no_groups() {
            return true;
        }
        self.global.remove_appearance_keys(&desc.global);
        for g in &desc.groups {
            if g.appearance.is_empty() {
                if !g.has_fx() {
                    self.remove_group_tree(g.key);
                }
                continue;
            }
            for (apr, values) in &g.appearance.entries {
                for (k, _) in &values.entries {
                    self.remove_appearance_key(records, g.key, *apr, *k);
                }
            }
        }
        true
    }

    /// Put another description in this one's place. When both extend the same description, it is
    /// added to this one and every group it did not add to or merge into goes; else this one
    /// becomes a copy of it.
    // D2-13844
    pub fn replace(&mut self, desc: &Desc) -> Result<(), LookError> {
        if self.parent != desc.parent {
            *self = Self::local(desc);
            return Ok(());
        }
        let mut touched = Vec::new();
        self.apply_additive_noting(desc, &mut touched)?;
        self.remove_untouched(&touched);
        Ok(())
    }

    /// Take out every group whose key is not in `touched`, each unhooked first; the root found
    /// again when it goes.
    // D2-13811
    fn remove_untouched(&mut self, touched: &[u32]) {
        let gone: Vec<u32> = self
            .groups
            .entries
            .iter()
            .map(|(k, _)| *k)
            .filter(|k| !touched.contains(k))
            .collect();
        for key in gone {
            self.remove_group(key);
        }
    }
}

/// An object's descriptions as the client keeps them: its own and the appearance its effects
/// apply.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct ObjectLook {
    /// Its own description, the effects' appearance laid over it.
    pub local: Look,
    /// The appearance values its effects apply, as changes to every group.
    pub effects: Look,
}

impl ObjectLook {
    /// An object's descriptions with its own set to `desc`.
    #[must_use]
    pub fn new(desc: &Desc) -> Self {
        let mut o = Self::default();
        o.set_local(desc);
        o
    }

    /// Set its own description: a copy of `desc`, the effects' appearance laid over it.
    // D2-06200, D2-08138
    pub fn set_local(&mut self, desc: &Desc) {
        self.local = Look::local(desc);
        let _ = self.local.apply_additive(&self.effects.to_desc());
    }

    /// An effect applies value `value` of appearance table `apr` to the whole object: kept with
    /// the effects' appearance and, when it took, laid over its own description again.
    // D2-06195, D2-06479
    pub fn do_appearance_fx(
        &mut self,
        records: &dyn LookRecords,
        apr: u32,
        value: AppearanceKey,
    ) -> bool {
        if !self
            .effects
            .add_appearance_key(records, GLOBAL_KEY, apr, value)
        {
            return false;
        }
        let _ = self.local.apply_additive(&self.effects.to_desc());
        true
    }

    /// An effect's appearance value `key` of table `apr` comes off: off the effects' appearance
    /// and off its own description.
    // D2-06196, D2-06480
    pub fn remove_appearance_fx(&mut self, records: &dyn LookRecords, apr: u32, key: u32) {
        self.effects
            .remove_appearance_key(records, GLOBAL_KEY, apr, key);
        self.local
            .remove_appearance_key(records, GLOBAL_KEY, apr, key);
    }

    /// Add a description to its own.
    // D2-06201, D2-08139
    pub fn apply_additive(&mut self, desc: &Desc) -> Result<(), LookError> {
        self.local.apply_additive(desc)
    }

    /// Take a description out of its own.
    // D2-06202, D2-08140
    pub fn apply_subtractive(&mut self, records: &dyn LookRecords, desc: &Desc) -> bool {
        self.local.apply_subtractive(records, desc)
    }

    /// Its own description becomes stored description `id`, keeping the appearance values the
    /// whole object had: the total's values for every group are gathered, its own is set to the
    /// record with the effects' appearance laid over it, and the gathered values are added back.
    /// `false` when `id` is not in the data.
    // D2-06261, D2-08137
    pub fn combine(&mut self, records: &dyn LookRecords, id: DataId) -> bool {
        let Some(v) = records.visual(id) else {
            return false;
        };
        let kept = Look::of(records, &self.local.to_desc())
            .map(|t| t.gather_appearance(GLOBAL_KEY))
            .unwrap_or_default();
        self.local = Look::local(&Desc::from_stored(&v));
        let _ = self.local.apply_additive(&self.effects.to_desc());
        for (apr, values) in &kept {
            self.local.add_appearance(records, GLOBAL_KEY, *apr, values);
        }
        true
    }

    /// The total description to draw: its own put together with every description it extends.
    pub fn total(&self, records: &dyn LookRecords) -> Result<Look, LookError> {
        Look::of(records, &self.local.to_desc())
    }
}
