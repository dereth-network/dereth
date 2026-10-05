use super::*;

/// Oracle: the HUD's vital-stat table uses current id 2 and maximum id 1 for health.
#[test]
fn the_vital_stat_numbers_are_the_documented_attribute_2nd_types() {
    assert_eq!(Vital::Health.stats(), (2, 1));
    assert_eq!(Vital::Stamina.stats(), (4, 3));
    assert_eq!(Vital::Mana.stats(), (6, 5));
}

/// Oracle: the panel id is itself an element id, so a request
/// carries the raw id and never a "which panel is this" enum.
#[test]
fn a_panel_visibility_request_carries_the_raw_panel_id() {
    let r = UiRequest::SetPanelVisibility {
        panel: 0x1000_018B,
        visible: true,
    };
    assert_eq!(
        r,
        UiRequest::SetPanelVisibility {
            panel: 0x1000_018B,
            visible: true
        }
    );
}
