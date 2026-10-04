//! Salvage and tinkering — the client mediates but does not compute.
//!
//! # Who calls what
//!
//! | half | who calls it |
//! |---|---|
//! | [`is_item_suitable`] | `dereth_client::hud`'s `GameView::salvage_item_suitable`, for `dereth_ui_screens::panels::salvage` |
//! | the line builder | the salvage-operation result receiver |
//! | tinkering-tool request | the panel's Salvage button |
//!
//! # The line builder, verified against retail
//!
//! It is **not** one line per result reading
//! `"You obtain {units} units of {material} (workmanship {:.2})"`; none of that is in retail.
//! The materials-salvaged string builder builds **one** string from
//! `"%s%d %s (ws %.2lf)"` per result, where the leading `%s` is a separator —
//! `""` on the first, `" and "` on the last, `", "` in between — and the UI salvage-result handler
//! then wraps the
//! whole of it in `"You obtain %hs using your knowledge of %hs.%s\n"`.
//!
//! So: *"units of"* is not in the client, *"workmanship"* is spelled `ws`, the per-result text is a
//! clause and not a line, and the skill name and the augmentation clause are half of what a
//! player reads. ACE's own
//! `ACE.Server/Entity/SalvageResults.cs` carries the retail line as a comment,
//! `"You obtain <amount> <material> (ws <workmanship>) using your knowledge of <skill>."`, which is
//! an independent second reading of the same two format strings.

use crate::weenie::{bitfield, Weenie};
use crate::{Request, RequestSink, World};
use dereth_primitives::ObjectId;
use dereth_protocol::items::SalvageResultMessage;

mod list;
pub use list::{SalvageEffect, SalvageList};

/// The tinkering system's material-type validity test.
///
/// Accepts 1, 2, 4–8, 10–0x37, 0x39–0x40, 0x42–0x47, 0x49–0x4D — i.e. everything in ACE's
/// `MaterialType` except 3, 9, 0x38, 0x41 and 0x48. `\[verified\]` from the switch table.
#[must_use]
pub fn is_valid_material_type(m: u32) -> bool {
    matches!(m, 1 | 2 | 4..=8 | 10..=0x37 | 0x39..=0x40 | 0x42..=0x47 | 0x49..=0x4D)
}

/// The five material ids the switch table rejects — and they are not arbitrary.
///
/// `3` is not `Bronze`. In `ACE.Entity/Enum/MaterialType.cs` the five are `Cloth` (3), `Gem` (9),
/// `Metal` (0x38), `Stone` (0x41) and `Wood` (0x48) — the five **category umbrella** values,
/// each sitting at the head of its own block of real materials. The switch is not a list of
/// oddities, it is "a category is not a material", which is why it is a contiguous-looking set
/// of five holes.
pub const REJECTED_MATERIALS: [u32; 5] = [3, 9, 0x38, 0x41, 0x48];

/// Check whether an item is suitable for salvage.
///
/// `salvage_multiple` is the `SalvageMultiple` character option (`CharacterOptions2` 0x80);
/// `panel_material` is the panel's material, which is 0 until the first item is dropped.
///
/// Checked against retail; the order below is retail's:
///
/// ```text
///   if (!w) return false;
///   if (!is_valid_material_type(w.material_type)) return false;
///   if (w.structure >= 100) return false;
///   if (!salvage_multiple && panel_material && mat != panel_material) return false;
///   return ~(w.bitfield >> 24) & 1;
/// ```
#[must_use]
pub fn is_item_suitable(item: &Weenie, salvage_multiple: bool, panel_material: u32) -> bool {
    let mat = item.pwd.material_type.unwrap_or(0);
    if !is_valid_material_type(mat) {
        return false;
    }
    // Fully-repaired items cannot be salvaged.
    if item.pwd.structure.unwrap_or(0) >= 100 {
        return false;
    }
    if !salvage_multiple && panel_material != 0 && mat != panel_material {
        return false;
    }
    // Bit `0x01000000` is retained as the observed "cannot be salvaged" flag. Give it no other
    // meaning.
    item.pwd.bitfield & bitfield::CANNOT_BE_SALVAGED == 0
}

/// The materials-salvaged string builder — **one** string for the whole results list.
///
/// Each result, in list order, appends `"%s%d %s (ws %.2lf)"` — the separator, the units, the
/// material's display name and the workmanship. The separator is empty for the first result,
/// `" and "` for the last and `", "` otherwise. The name is set to `"Unknown"` before the material
/// enum is mapped to its display name.
///
/// `"Unknown"` is pre-loaded **before** the mapper is asked and survives a miss, so a material id
/// the dat has no row for reads `"3 Unknown (ws 5.00)"` — retail's own answer, not a placeholder.
///
/// `material_name` is supplied by the caller because this crate has no dat access. The table it
/// resolves is `DualDidMapper 0x27000000`, 78 rows, and
/// the mapper replaces `_` with a space on the way out, so row `0x10 "Black_Opal"`
/// reaches the scroll as `Black Opal`.
#[must_use]
pub fn materials_salvaged_string(
    d: &SalvageResultMessage,
    material_name: &dyn Fn(u32) -> String,
) -> String {
    let mut out = String::new();
    let n = d.results.len();
    for (i, r) in d.results.iter().enumerate() {
        let sep = if i == 0 {
            ""
        } else if i + 1 == n {
            " and "
        } else {
            ", "
        };
        out.push_str(&format!(
            "{sep}{} {} (ws {:.2})",
            r.units,
            material_name(r.material),
            r.workmanship
        ));
    }
    out
}

/// The not-suitable string builder.
///
/// It starts from `" The following were not suitable for salvaging"`. Each id that resolves to a
/// weenie appends `": "` (the first) or `", "` (later ones) and the weenie's name; an id with no
/// weenie is skipped and not counted. If anything was appended the string ends with `".\n"`;
/// otherwise it becomes the empty string.
///
/// **The empty answer is not the same as no answer**: when the list is non-empty but no id resolves
/// to a weenie, the client still prints to the scroll — the null string. `None` here is
/// that case, and the caller emits nothing for it, matching how an empty string reaches the
/// scroll after `trim`.
#[must_use]
pub fn non_suitables_string(
    ids: &[ObjectId],
    item_name: &dyn Fn(ObjectId) -> Option<String>,
) -> Option<String> {
    let mut out = String::from(" The following were not suitable for salvaging");
    let mut count = 0usize;
    for id in ids {
        let Some(name) = item_name(*id) else { continue };
        out.push_str(if count == 0 { ": " } else { ", " });
        out.push_str(&name);
        count += 1;
    }
    if count == 0 {
        return None;
    }
    out.push_str(".\n");
    Some(out)
}

/// `"Salvaging Failed!\n"` — retail's literal for the arm taken when **both** lists are empty.
pub const SALVAGING_FAILED: &str = "Salvaging Failed!\n";

/// The whole handler begins by checking whether channel `0x19` is squelched.
///
/// A squelch on text type `0x19` silences the salvage report entirely: the handler returns before
/// it builds a single string.
pub const SALVAGE_TEXT_TYPE: u32 = 0x19;

impl World {
    /// Handle salvage-operation result data — the
    /// receiver for `0x02B4`.
    ///
    /// This is the `SalvageResult` reader, and `dereth_ui_screens::panels::salvage` is the
    /// sender of the request it answers.
    ///
    /// The three scroll prints are **independent**, in this order, and all three are
    /// at window 0 on chat type **0**, not the `0x19` the squelch is keyed on:
    ///
    /// 1. When the results list is non-empty: the materials clause, the skill
    ///    name and the augmentation clause in one line.
    /// 2. When the not-salvagable list is non-empty: [`non_suitables_string`].
    /// 3. When **both** are empty: [`SALVAGING_FAILED`]. It is not an `else` on the
    ///    first two — both conditions are re-tested after them.
    ///
    /// The augmentation clause is `" Your augmentation has given you a return bonus of %d%%!"`
    /// and is appended **only when the augmentation bonus is non-zero**. `skill_name`
    /// returning `None` is the skill-name formatter answering `false`, which
    /// leaves the second `%hs` pointed at the null string — an empty run, not a skipped line.
    ///
    /// **`None` is the squelch and `Some(0)` is "nothing to say"**, and they are different facts:
    /// a `0x02B4` whose results list is empty and whose not-salvagable list resolves to no weenie
    /// at all writes nothing and is *not* silenced. A `u32` alone would conflate the two and make
    /// a squelch counter read 2 where 1 is right.
    ///
    /// The scroll is a live consumer (`Hud::drain_scroll` → `ChatMessage` in the very next batch).
    pub fn recv_salvage_operations_result(
        &mut self,
        d: &SalvageResultMessage,
        material_name: &dyn Fn(u32) -> String,
        skill_name: &dyn Fn(u32) -> Option<String>,
        item_name: &dyn Fn(ObjectId) -> Option<String>,
    ) -> Option<u32> {
        if self.chat.is_squelched(ObjectId(0), "", SALVAGE_TEXT_TYPE) {
            return None;
        }
        let mut lines = 0u32;
        if !d.results.is_empty() {
            let materials = materials_salvaged_string(d, material_name);
            let skill = skill_name(d.skill_used).unwrap_or_default();
            let aug = if d.aug_bonus == 0 {
                String::new()
            } else {
                format!(
                    " Your augmentation has given you a return bonus of {}%!",
                    d.aug_bonus
                )
            };
            self.scroll.add_text_to_scroll(
                &format!("You obtain {materials} using your knowledge of {skill}.{aug}\n"),
                0,
                true,
                0,
            );
            lines += 1;
        }
        if !d.not_salvagable.is_empty() {
            if let Some(line) = non_suitables_string(&d.not_salvagable, item_name) {
                self.scroll.add_text_to_scroll(&line, 0, true, 0);
                lines += 1;
            }
        }
        if d.results.is_empty() && d.not_salvagable.is_empty() {
            self.scroll.add_text_to_scroll(SALVAGING_FAILED, 0, true, 0);
            lines += 1;
        }
        Some(lines)
    }

    /// Send event `0x027D` to create a tinkering tool from the tool id and `items`.
    ///
    /// The tinkering panel is the **only** caller in retail, and its two guards
    /// are the client's own: the collected list must be non-empty and the tool id must be
    /// set. Both are repeated here because a chat command or a test could reach
    /// this without passing through the panel.
    ///
    /// Returns `None` when nothing was sent.
    pub fn create_tinkering_tool(
        &mut self,
        req: &mut dyn RequestSink,
        tool: ObjectId,
        items: &[ObjectId],
    ) -> Option<()> {
        if tool.0 == 0 || items.is_empty() {
            return None;
        }
        req.send(Request::CreateTinkeringTool(
            dereth_protocol::items::InventoryCreateTinkeringTool {
                tool,
                items: items.to_vec(),
            },
        ));
        Some(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::ObjectId;
    use dereth_protocol::items::SalvageResult;
    use dereth_protocol::types::PublicWeenieDesc;

    fn item(material: u32, structure: u16, retained: bool) -> Weenie {
        let mut w = Weenie::new(ObjectId(1));
        w.pwd = PublicWeenieDesc {
            material_type: Some(material),
            structure: Some(structure),
            bitfield: if retained {
                bitfield::CANNOT_BE_SALVAGED
            } else {
                0
            },
            ..PublicWeenieDesc::default()
        };
        w
    }

    /// Oracle: §12, the material-type validity test's switch table.
    #[test]
    fn exactly_five_material_ids_are_rejected_inside_the_range() {
        for m in REJECTED_MATERIALS {
            assert!(
                !is_valid_material_type(m),
                "material 0x{m:X} must be rejected"
            );
        }
        for m in 1..=0x4Du32 {
            let expected = !REJECTED_MATERIALS.contains(&m);
            assert_eq!(is_valid_material_type(m), expected, "material 0x{m:X}");
        }
        assert!(!is_valid_material_type(0));
        assert!(!is_valid_material_type(0x4E));
    }

    /// Oracle: §12's item-suitability test, in order.
    #[test]
    fn suitability_follows_the_four_tests_in_order() {
        assert!(is_item_suitable(&item(0x10, 50, false), false, 0));
        assert!(
            !is_item_suitable(&item(3, 50, false), false, 0),
            "invalid material"
        );
        assert!(
            !is_item_suitable(&item(0x10, 100, false), false, 0),
            "fully repaired items cannot be salvaged"
        );
        assert!(
            !is_item_suitable(&item(0x10, 50, true), false, 0),
            "retained"
        );
        assert!(
            !is_item_suitable(&item(0x10, 50, false), false, 0x11),
            "without SalvageMultiple the panel locks to the first material"
        );
        assert!(
            is_item_suitable(&item(0x10, 50, false), true, 0x11),
            "with SalvageMultiple, mixing is allowed"
        );
    }

    fn results(rs: &[(u32, f64, i32)]) -> SalvageResultMessage {
        SalvageResultMessage {
            skill_used: 40,
            not_salvagable: Vec::new(),
            results: rs
                .iter()
                .map(|(m, w, u)| SalvageResult {
                    material: *m,
                    workmanship: *w,
                    units: *u,
                })
                .collect(),
            aug_bonus: 0,
        }
    }

    fn names(m: u32) -> String {
        match m {
            0x10 => "Black Opal".into(),
            0x33 => "Ivory".into(),
            0x4D => "Teak".into(),
            _ => "Unknown".into(),
        }
    }

    /// The materials-salvaged builder's separator ladder, all three arms.
    ///
    /// One result takes the **first** arm, not the last: the `first` flag is tested
    /// before `bl` (`node.next == NULL`) is consulted, so a single-element list gets
    /// `""` and never `" and "`.
    #[test]
    fn the_materials_clause_joins_with_a_comma_and_an_and() {
        assert_eq!(
            materials_salvaged_string(&results(&[(0x10, 5.0, 3)]), &names),
            "3 Black Opal (ws 5.00)"
        );
        assert_eq!(
            materials_salvaged_string(&results(&[(0x10, 5.0, 3), (0x33, 4.5, 2)]), &names),
            "3 Black Opal (ws 5.00) and 2 Ivory (ws 4.50)"
        );
        assert_eq!(
            materials_salvaged_string(
                &results(&[(0x10, 5.0, 3), (0x33, 4.5, 2), (0x4D, 1.0, 1)]),
                &names
            ),
            "3 Black Opal (ws 5.00), 2 Ivory (ws 4.50) and 1 Teak (ws 1.00)"
        );
        assert_eq!(materials_salvaged_string(&results(&[]), &names), "");
    }

    /// The mapper's miss arm, which is retail's and not a placeholder: `"Unknown"` is written into
    /// the string **before** the mapper is called and is what survives a `false`.
    #[test]
    fn an_unmapped_material_reads_unknown() {
        assert_eq!(
            materials_salvaged_string(&results(&[(0xFF, 2.0, 7)]), &names),
            "7 Unknown (ws 2.00)"
        );
    }

    /// The not-suitable builder: `": "` once, then `", "`, then `".\n"`; and an
    /// id that resolves to no weenie bumps no counter, so it takes no separator with it.
    #[test]
    fn the_not_suitable_line_skips_ids_with_no_weenie() {
        let known = |id: ObjectId| match id.0 {
            1 => Some("Sword".to_owned()),
            2 => Some("Shield".to_owned()),
            _ => None,
        };
        assert_eq!(
            non_suitables_string(&[ObjectId(1), ObjectId(2)], &known).unwrap(),
            " The following were not suitable for salvaging: Sword, Shield.\n"
        );
        // The unresolvable id is in the middle, so a separator bug would show as ", , ".
        assert_eq!(
            non_suitables_string(&[ObjectId(1), ObjectId(9), ObjectId(2)], &known).unwrap(),
            " The following were not suitable for salvaging: Sword, Shield.\n"
        );
        assert_eq!(
            non_suitables_string(&[ObjectId(9)], &known),
            None,
            "the null-string arm"
        );
    }
}
