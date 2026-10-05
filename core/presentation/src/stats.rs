//! Character statistics, skill groups, titles and vitae wording shared by interfaces.
use crate::numfmt::exact_number;
use crate::DisplayVariant;
use dereth_client_contract::{GameView, SkillEntry, VitaeDisplay};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SkillGroup {
    Specialized,
    Trained,
    Untrained,
    Unusable,
}
impl SkillGroup {
    pub const ALL: [Self; 4] = [
        Self::Specialized,
        Self::Trained,
        Self::Untrained,
        Self::Unusable,
    ];
    pub const fn of(sac: u32, min_level: u32) -> Self {
        match sac {
            3 => Self::Specialized,
            2 => Self::Trained,
            1 if min_level <= 1 => Self::Untrained,
            _ => Self::Unusable,
        }
    }
    pub const fn header_template(self) -> usize {
        self as usize + 1
    }
    pub const fn label(self) -> &'static str {
        match self {
            Self::Specialized => "Specialized Skills",
            Self::Trained => "Trained Skills",
            Self::Untrained => "Untrained Skills",
            Self::Unusable => "Unusable Skills",
        }
    }
    pub fn for_entry(entry: &SkillEntry, variant: DisplayVariant) -> Option<Self> {
        match variant {
            DisplayVariant::Modern => Some(Self::of(entry.sac, entry.min_level)),
            DisplayVariant::Classic => match entry.sac {
                3 => Some(Self::Specialized),
                2 => Some(Self::Trained),
                1 if entry.effective > 0 => Some(Self::Untrained),
                1 if entry.effective == 0 => Some(Self::Unusable),
                _ => None,
            },
        }
    }
}

pub fn skill_groups(
    skills: &[SkillEntry],
    variant: DisplayVariant,
) -> Vec<(SkillGroup, Vec<&SkillEntry>)> {
    SkillGroup::ALL
        .into_iter()
        .map(|group| {
            let mut rows: Vec<_> = skills
                .iter()
                .filter(|s| SkillGroup::for_entry(s, variant) == Some(group))
                .collect();
            rows.sort_by(|a, b| {
                let order = a.name.encode_utf16().cmp(b.name.encode_utf16());
                match variant {
                    DisplayVariant::Classic => order,
                    DisplayVariant::Modern => order.then(a.id.cmp(&b.id)),
                }
            });
            (group, rows)
        })
        .collect()
}

/// The effective number with the vitae contribution removed determines its enchantment color.
pub const fn value_font(effective: i32, raw: i32, vitae: i32) -> u32 {
    let adjusted = effective.saturating_sub(vitae);
    if raw < adjusted {
        1
    } else if adjusted < raw {
        2
    } else {
        0
    }
}

pub fn title_rows(titles: &[(u32, String)]) -> Vec<(u32, String)> {
    let mut rows: Vec<_> = titles
        .iter()
        .filter(|(id, name)| *id != 0 && !name.is_empty())
        .cloned()
        .collect();
    rows.sort_by(|a, b| a.1.encode_utf16().cmp(b.1.encode_utf16()));
    rows
}
pub fn can_set_title(selected: Option<u32>, current: u32, rows: &[(u32, String)]) -> bool {
    selected.is_some_and(|id| id != current && rows.iter().any(|r| r.0 == id))
}

pub const fn can_raise(cost: u64, available: u64) -> bool {
    cost != 0 && available >= cost
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SelectedStat {
    pub id: u32,
    pub cost: u32,
    pub credits: bool,
    pub value: i32,
    pub name: String,
}
pub fn selected_stat(
    view: &dyn GameView,
    id: u32,
    secondary: bool,
    skill: bool,
    attribute_name: &str,
) -> Option<SelectedStat> {
    if skill {
        let entry = view.skills().iter().find(|s| s.id == id)?;
        let adv = view.skill_advancement(id)?;
        Some(SelectedStat {
            id,
            cost: adv.cost_to_raise,
            credits: adv.sac < 2,
            value: entry.effective,
            name: entry.name.clone(),
        })
    } else {
        let adv = view.attribute_advancement(id, secondary)?;
        Some(SelectedStat {
            id,
            cost: adv.cost_to_raise,
            credits: false,
            value: adv.effective,
            name: attribute_name.into(),
        })
    }
}
pub fn stat_title(name: &str, value: i32, untrained: bool, variant: DisplayVariant) -> String {
    match variant {
        DisplayVariant::Classic if untrained => format!("{name} (Must be trained)"),
        DisplayVariant::Classic => format!("{name} {value}"),
        DisplayVariant::Modern => format!("{name}: {value}"),
    }
}
pub fn signed_suffix(delta: i32) -> String {
    match delta.cmp(&0) {
        std::cmp::Ordering::Equal => String::new(),
        std::cmp::Ordering::Greater => format!(" (+{delta})"),
        std::cmp::Ordering::Less => format!(" ({delta})"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VitaeContent {
    pub penalty: i32,
    pub experience: i64,
}
pub fn vitae_content(display: VitaeDisplay, variant: DisplayVariant) -> VitaeContent {
    let strength = f64::from(display.multiplier) * 100.0;
    let penalty = match variant {
        DisplayVariant::Modern => {
            100i32.saturating_sub(dereth_primitives::num::to_i32_f64(strength + 0.5))
        }
        DisplayVariant::Classic => {
            let loss = 100.0 - strength;
            if loss > 0.0 {
                dereth_primitives::num::to_i32_f64(loss.max(1.0) + 0.5)
            } else {
                0
            }
        }
    };
    VitaeContent {
        penalty,
        experience: i64::from(display.threshold) - i64::from(display.cp_pool),
    }
}
impl VitaeContent {
    pub fn classic_text(self) -> String {
        let Self {
            penalty,
            experience,
        } = self;
        format!("\n\nDue to your recent death, you have temporarily lost {penalty}% of your Vitae, or life force.\n\nThis means that your health, stamina, mana, and skills are temporarily reduced by {penalty}%.  A reduction of less than 15% will not hinder you much, but beware losing much more than that.\n\nYou will regain 1% of your Vitae once you earn {} more experience point{}.\n", exact_number(experience), if experience == 1 { "" } else { "s" })
    }
}

pub fn meter_level(current: u32, maximum: u32) -> f32 {
    if maximum == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    {
        current as f32 / maximum as f32
    }
}

use dereth_client_contract::statmgmt::XpHeader;
const LUMINANCE_MIN_LEVEL: i32 = 200;
const LUMINANCE_LABEL: &str = "Luminance:";
/// Everything one stat-management header render reads, joined by the caller.
///
/// A struct rather than six arguments because both subclasses draw the **same** eight fields off
/// their own copies of the elements, so the gather happens once.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct HeaderInputs {
    /// The player's full object name.
    pub name: String,
    /// [`XpHeader`]; `None` is the level property (`0x19`) being absent, which is the `"???"`
    /// level.
    pub xp: Option<XpHeader>,
    /// The gender/heritage display string.
    pub heritage: Option<String>,
    /// The display title for the character's title id.
    pub title: Option<String>,
    /// The pk status update's answer.
    pub pk: dereth_client_contract::PkStatus,
    /// 64-bit properties 6 and 7 -- available and maximum luminance.
    pub luminance: (i64, i64),
}

impl HeaderInputs {
    /// Gather one header's inputs off the seam. Both `SkillsPanel` and `AttributesPanel` call this.
    #[must_use]
    pub fn gather(view: &dyn dereth_client_contract::GameView) -> Self {
        Self {
            name: view
                .character_name()
                .or_else(|| view.player().and_then(|p| view.name(p)))
                .unwrap_or_default()
                .to_owned(),
            xp: view.experience_header(),
            heritage: view.gender_heritage_display(),
            title: view.display_title(),
            pk: view.pk_status(),
            luminance: if view.era_features().luminance {
                view.luminance()
            } else {
                (0, 0)
            },
        }
    }

    /// The character info update's heritage text: the gender/heritage display string, then —
    /// if the title lookup succeeds — a space and the title.
    ///
    /// The space is a separate append of the literal `L" "` and only happens when the title
    /// lookup succeeded, so a character with no display title gets no trailing space.
    #[must_use]
    pub fn heritage_line(&self) -> String {
        self.heritage_line_for(DisplayVariant::Modern)
    }
    pub fn heritage_line_for(&self, variant: DisplayVariant) -> String {
        let mut s = self.heritage.clone().unwrap_or_default();
        if let Some(t) = self.title.as_ref().filter(|t| !t.is_empty()) {
            if variant == DisplayVariant::Modern || !s.is_empty() {
                s.push(' ');
            }
            s.push_str(t);
        }
        s
    }

    /// The experience update's luminance arm, gate and all.
    ///
    /// `(label, value)`, both empty when the arm cleared them. The gate is **two** tests
    /// and the second one is easy to miss: `level < 200` *or* `MaximumLuminance == 0`.
    #[must_use]
    pub fn luminance_line(&self) -> (String, String) {
        let level = self.xp.map_or(0, |x| x.level);
        let (available, maximum) = self.luminance;
        if level < LUMINANCE_MIN_LEVEL || maximum == 0 {
            return (String::new(), String::new());
        }
        (
            LUMINANCE_LABEL.to_owned(),
            format!("{} / {}", exact_number(available), exact_number(maximum)),
        )
    }
}

#[cfg(test)]
mod tests;

/// The classic artwork clips the ratio after computing it in double precision.
pub fn classic_meter_ratio(current: u32, maximum: u32) -> f64 {
    if maximum == 0 {
        0.0
    } else {
        (f64::from(current) / f64::from(maximum)).clamp(0.0, 1.0)
    }
}
