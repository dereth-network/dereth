//! The classic interface's keyboard commands that act on the host: mouse look, and the keys that
//! toggle a character option (automatic shortcuts, inverted mouse look, right-click mouse look,
//! the stretched interface and muting when the window loses focus).
//! The option word is literal legacy data; modern option ordinal names differ.
use crate::runtime::Cx;
use dereth_client_contract::{
    actions::{camera, Action},
    PrefValue, UiRequest,
};
use dereth_client_model::World;
use dereth_client_runtime::shell::Shell;
use dereth_primitives::ObjectId;

pub const AUTO_SHORTCUTS: u32 = 1;
pub const INVERT_LOOK: u32 = 0x10;
pub const RIGHT_CLICK_LOOK: u32 = 0x4000;
pub const STRETCH_UI: u32 = 0x20_0000;
pub const MUTE_INACTIVE: u32 = 0x100_0000;

/// The classic interface's own settings among the character's option bits: inverted mouse look,
/// right-click mouse look, the stretched interface and muting when the window loses the focus.
/// The same bits of the character's word mean other things to the final client, which shares the
/// character, so they are never written to the word: they are preferences in the shared store.
pub const CLASSIC_ONLY: u32 = INVERT_LOOK | RIGHT_CLICK_LOOK | STRETCH_UI | MUTE_INACTIVE;

/// Each of [`CLASSIC_ONLY`]'s bits and the preference that holds it: the classic interface's
/// own three (`UI.Classic.*`), and muting when inactive, which is the shared
/// `Sound.PlaySoundOnlyWhenActive` both interfaces' pages edit.
pub const BIT_PREFERENCES: [(u32, &str); 4] = [
    (
        INVERT_LOOK,
        dereth_client_contract::options::classic::INVERT_MOUSE_LOOK,
    ),
    (
        RIGHT_CLICK_LOOK,
        dereth_client_contract::options::classic::RIGHT_CLICK_MOUSE_LOOK,
    ),
    (
        STRETCH_UI,
        dereth_client_contract::options::classic::STRETCH_UI,
    ),
    (MUTE_INACTIVE, "Sound.PlaySoundOnlyWhenActive"),
];

/// This client's own settings, as the bits of [`CLASSIC_ONLY`], read from the shared store.
#[must_use]
pub fn classic_bits() -> u32 {
    BIT_PREFERENCES
        .iter()
        .filter(|(_, name)| dereth_client_contract::options::classic::on(name))
        .fold(0, |bits, (bit, _)| bits | bit)
}

/// Set this client's own settings from the bits of [`CLASSIC_ONLY`] in `bits`, in the shared
/// store.
pub fn set_classic_bits(bits: u32) {
    for (bit, name) in BIT_PREFERENCES {
        let _ = dereth_client_contract::options::store::set_value(
            name,
            PrefValue::Bool(bits & bit != 0),
        );
    }
}

/// Whether the interface is stretched to the window's height.
#[must_use]
pub fn stretch_ui() -> bool {
    classic_bits() & STRETCH_UI != 0
}

fn option_mask(name: &str) -> Option<u32> {
    Some(match name {
        "AutoCreateShortcuts" => AUTO_SHORTCUTS,
        "InvertMouseLook" => INVERT_LOOK,
        "RightClickToMouseLook" => RIGHT_CLICK_LOOK,
        "StretchUI" => STRETCH_UI,
        "MuteOnLosingFocus" => MUTE_INACTIVE,
        _ => return None,
    })
}

/// The character's option word with `mask` toggled, sent to the server.
fn toggle_option(world: &World, mask: u32) -> UiRequest {
    let options = &world.player_system.options;
    UiRequest::SetOptionWords {
        options: options.options ^ mask,
        options2: options.options2,
        timestamp_format: None,
        save: true,
    }
}

/// Apply after loading character options as well as after changing their word.
pub fn sync_options<S: Shell>(cx: &mut Cx<'_, S>) {
    let word = classic_bits();
    // The classic option inverts only vertical mouse look. The shared modern preference
    // negates both axes, so it stays off and `cursor_moved` applies this option.
    let now = dereth_primitives::LocalTime(cx.now());
    for request in [
        UiRequest::SetPreference("Input.InvertMouseLookYAxis", PrefValue::Bool(false)),
        UiRequest::SetPreference(
            "Sound.PlaySoundOnlyWhenActive",
            PrefValue::Bool(word & MUTE_INACTIVE != 0),
        ),
    ] {
        cx.run_request(request, now, &mut |_, _| false);
    }
}

/// The sample to hand on for the real one at `(x, y)`. `previous` is the last real sample and
/// `handed_on` the last sample handed on: inverted, the next one handed on moves from `handed_on`
/// by the real vertical motion reversed, so the motion the camera sees is the real motion with
/// its vertical part reversed.
fn cursor_sample(
    previous: Option<(f64, f64)>,
    handed_on: Option<(f64, f64)>,
    x: f64,
    y: f64,
    inverted: bool,
) -> (f64, f64) {
    match (previous, handed_on) {
        (Some((_, last_y)), Some((_, handed_y))) if inverted => (x, handed_y - (y - last_y)),
        _ => (x, y),
    }
}

/// Native absolute cursor delivery, with legacy vertical-only inversion. `last` is the last real
/// sample, kept by the caller: reflected coordinates are only a delta adapter.
pub fn cursor_moved<S: Shell>(cx: &mut Cx<'_, S>, last: &mut Option<(f64, f64)>, x: f64, y: f64) {
    let inverted = cx.mouse_look() && classic_bits() & INVERT_LOOK != 0;
    let (sample_x, sample_y) = cursor_sample(*last, cx.last_cursor(), x, y, inverted);
    cx.cursor_moved(sample_x, sample_y);
    *last = Some((x, y));
}

/// Return false only for a command belonging to another host consumer.
pub fn handle<S: Shell>(cx: &mut Cx<'_, S>, name: &str, pressed: bool) -> Result<bool, String> {
    if name == "ShiftView" {
        cx.inject_action(if pressed {
            Action::begin(camera::TOGGLE_MOUSELOOK)
        } else {
            Action::end(camera::TOGGLE_MOUSELOOK)
        });
        cx.mouse_look_button(pressed);
        return Ok(true);
    }
    let Some(mask) = option_mask(name) else {
        return Ok(false);
    };
    if !pressed {
        return Ok(true);
    }
    if mask & CLASSIC_ONLY != 0 {
        set_classic_bits(classic_bits() ^ mask);
        sync_options(cx);
        return Ok(true);
    }
    let request = toggle_option(cx.model(), mask);
    cx.queue(Vec::new(), vec![request]);
    sync_options(cx);
    // Toggling StretchUI changes the interface layout. The host builds its layout
    // each frame, so the changed option is visible immediately.
    Ok(true)
}

/// Called after the player uses or equips an item; never when an item arrives in the inventory.
/// Automatic creation is quiet, owned-only, duplicate-free and limited to nine slots.
pub fn auto_shortcut_after_use(world: &World, object: ObjectId) -> Option<UiRequest> {
    if world.player_system.options.options & AUTO_SHORTCUTS == 0
        || !world.is_owned_by_player(object)
    {
        return None;
    }
    let item = world.weenie(object)?;
    if !shortcut_candidate(item)
        || world.vendor_id().filter(|id| id.0 != 0) == item.pwd.container_id.filter(|id| id.0 != 0)
            && item.pwd.container_id.is_some_and(|id| id.0 != 0)
    {
        return None;
    }
    let slots: [Option<ObjectId>; 9] = std::array::from_fn(|i| world.player_system.shortcut_at(i));
    first_free_shortcut(&slots, object)?;
    Some(UiRequest::CreateShortcut(object))
}

fn shortcut_candidate(item: &dereth_client_model::weenie::Weenie) -> bool {
    let p = &item.pwd;
    if !item.is_player() && (p.bitfield & 4 != 0 || p.obj_type & 0x10 != 0) {
        return false;
    }
    p.spell_id.unwrap_or(0) != 0
        || p.valid_locations.unwrap_or(0) & 0xfffff != 0
        || p.useability.unwrap_or(1) & 1 == 0
}
fn first_free_shortcut(slots: &[Option<ObjectId>; 9], object: ObjectId) -> Option<usize> {
    if slots.contains(&Some(object)) {
        None
    } else {
        slots.iter().position(Option::is_none)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    use crate::int::u32_from;
    #[test]
    fn toggles_preserve_unrelated_options_and_restore_on_second_press() {
        let mut world = World::default();
        world.player_system.options.options = 0x8123_4567;
        world.player_system.options.options2 = 0x1234_5678;
        world.player_system.module = Some(dereth_protocol::login::PlayerModule {
            options: 0x8123_4567,
            options2: 0x1234_5678,
            ..Default::default()
        });
        for name in [
            "AutoCreateShortcuts",
            "InvertMouseLook",
            "RightClickToMouseLook",
            "StretchUI",
            "MuteOnLosingFocus",
        ] {
            let mask = option_mask(name).unwrap();
            let UiRequest::SetOptionWords {
                options,
                options2,
                save,
                ..
            } = toggle_option(&world, mask)
            else {
                panic!("expected the option words");
            };
            assert_eq!(options, 0x8123_4567 ^ mask);
            assert_eq!(options2, 0x1234_5678);
            assert!(save);
        }
        assert_eq!(option_mask("HoldRun"), None);
    }
    #[test]
    fn automatic_shortcuts_do_not_expand_beyond_nine_or_duplicate_items() {
        let full = std::array::from_fn(|i| Some(ObjectId(u32_from(i) + 1)));
        assert_eq!(first_free_shortcut(&full, ObjectId(99)), None);
        let mut slots = full;
        slots[3] = None;
        assert_eq!(first_free_shortcut(&slots, ObjectId(1)), None);
        assert_eq!(first_free_shortcut(&slots, ObjectId(99)), Some(3));
    }
    #[test]
    fn automatic_shortcuts_accept_spell_equipment_or_useable_items() {
        let mut item = dereth_client_model::weenie::Weenie::default();
        assert!(!shortcut_candidate(&item));
        item.pwd.spell_id = Some(42);
        assert!(shortcut_candidate(&item));
        item.pwd.spell_id = None;
        item.pwd.valid_locations = Some(1 << 20);
        assert!(!shortcut_candidate(&item));
        item.pwd.valid_locations = Some(1 << 19);
        assert!(shortcut_candidate(&item));
        item.pwd.valid_locations = None;
        item.pwd.useability = Some(2);
        assert!(shortcut_candidate(&item));
        item.pwd.bitfield = 4;
        assert!(!shortcut_candidate(&item));
    }

    #[test]
    fn inverted_cursor_reverses_every_vertical_step_and_keeps_horizontal_motion() {
        // The real pointer moves steadily down and right; the camera must see it move up and right
        // by the same amount at every step, not only the first.
        let real = [
            (20.0, 30.0),
            (22.0, 35.0),
            (25.0, 36.0),
            (25.0, 40.0),
            (30.0, 41.0),
        ];
        let mut previous = None;
        let mut handed_on: Option<(f64, f64)> = None;
        let mut seen = Vec::new();
        for (x, y) in real {
            let sample = cursor_sample(previous, handed_on, x, y, true);
            if let Some(h) = handed_on {
                seen.push((sample.0 - h.0, sample.1 - h.1));
            }
            handed_on = Some(sample);
            previous = Some((x, y));
        }
        assert_eq!(seen, [(2.0, -5.0), (3.0, -1.0), (0.0, -4.0), (5.0, -1.0)]);
        // Not inverted, or with nothing handed on yet (a fresh hold), the sample is the real one.
        assert_eq!(
            cursor_sample(Some((22.0, 35.0)), Some((22.0, 25.0)), 25.0, 36.0, false),
            (25.0, 36.0)
        );
        assert_eq!(
            cursor_sample(Some((22.0, 35.0)), None, 25.0, 36.0, true),
            (25.0, 36.0)
        );
    }
}
