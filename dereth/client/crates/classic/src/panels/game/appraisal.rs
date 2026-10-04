//! Classic fonts and separators over shared appraisal presentation.
use dereth_client_contract::view::AppraisalView;
use dereth_presentation::{appraisal as shared, DisplayVariant};

fn color(index: u8) -> u32 {
    match index {
        1 => 0xff00ff00,
        2 => 0xffff0000,
        _ => 0xffd2d2c8,
    }
}

pub fn rich(p: &AppraisalView) -> Vec<crate::TextRun> {
    let mut has_text = false;
    shared::item_description_runs_for(p, DisplayVariant::Classic)
        .into_iter()
        .map(|run| {
            let prefix = if has_text {
                if run.same_line {
                    "\n"
                } else {
                    "\n\n"
                }
            } else {
                ""
            };
            let text = format!("{prefix}{}", run.text);
            has_text |= !text.is_empty();
            crate::TextRun {
                text,
                color: color(run.color),
            }
        })
        .collect()
}

pub fn character(p: &AppraisalView, name: &str) -> Vec<crate::TextRun> {
    shared::char_misc_rows_for(p, name, DisplayVariant::Classic)
        .into_iter()
        .enumerate()
        .map(|(i, row)| {
            let gap = if !row.label.is_empty() && !row.value.is_empty() {
                " "
            } else {
                ""
            };
            crate::TextRun {
                text: format!(
                    "{}{}{gap}{}",
                    if i == 0 { "" } else { "\n" },
                    row.label,
                    row.value
                ),
                color: color(row.color),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: appraisal.presentation.variants-preserve-order-and-world-facts
    #[test]
    fn classic_adapter_keeps_heading_order_and_enchantment_colors() {
        let mut p = AppraisalView {
            valid_locations: 1,
            armor_level: Some(10),
            armor_mods: Some([1.0; 8]),
            ..Default::default()
        };
        p.enchantment_mods.insert(0x1c, false);
        let headings: Vec<_> = rich(&p)
            .into_iter()
            .filter(|r| r.text.contains("Armor Level:"))
            .map(|r| (r.text, r.color))
            .collect();
        assert_eq!(
            headings,
            [
                ("Armor Level:  10".into(), 0xffff0000),
                ("\n\nArmor Level: 10".into(), 0xffff0000)
            ]
        );
        p.special.imbued = Some(0xa0000000);
        let text: String = rich(&p).into_iter().map(|r| r.text).collect();
        assert!(text.starts_with("Special Properties: Magic Absorbing, Phantasmal"));
    }
}
