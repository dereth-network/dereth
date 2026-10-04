//! The action-fire path: control -> action. The core of the pipeline: the client's nine steps,
//! the best-match choice between bindings, and the priority table and its barriers.

use crate::spec::{activation, ControlChord, ControlCode};
use crate::{ActionId, InputMapId, MAP_BLOCK_ALL, MAP_BLOCK_KEYBOARD};

/// `ControlType` -- how the fire path should read `data`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ControlType {
    Button,
    RelAxis,
    AbsAxis,
    Pov,
}

/// The control's activation: the raw device value becomes an activation bit and an
/// extent.
///
/// Absolute axes scale by 0.0625 and POV values by 1/3600. No shipped binding uses
/// a joystick axis, so these conversions do not establish interactive joystick feel.
#[must_use]
pub fn di_data_to_activation_type(ty: ControlType, data: i32) -> (u32, f32) {
    match ty {
        ControlType::Button => {
            if data == 0x80 {
                (activation::DOWN, 1.0)
            } else {
                (activation::UP, 0.0)
            }
        }
        // A DirectInput axis packet is a small signed value; f32 represents every i32 the device
        // can produce in this path exactly enough for the client's own float maths.
        #[allow(clippy::cast_precision_loss)]
        ControlType::RelAxis => (activation::ANALOG, data as f32),
        #[allow(clippy::cast_precision_loss)]
        ControlType::AbsAxis => (activation::ANALOG, data as f32 * 0.0625),
        ControlType::Pov => {
            let extent = if data == -1 || data == 1 {
                -1.0
            } else {
                #[allow(clippy::cast_precision_loss)]
                {
                    data as f32 * 0.000_277_777_78
                }
            };
            (activation::ANALOG, extent)
        }
    }
}

/// `RecentControlState` — one entry of the active-controls table.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RecentControlState {
    pub meta_mode: u32,
    pub activation: u32,
    pub action_matched: ActionId,
    pub input_map: InputMapId,
    pub data: i32,
}

/// One entry of the button history, used for the double-click and tap tests.
#[derive(Debug, Clone, Copy)]
pub struct ButtonHistoryEntry {
    pub time: u32,
    pub mouse_pos: (i32, i32),
}

/// The four OS-derived timing constants, all read **once at construction and never refreshed**, so
/// a control-panel change mid-session has no effect.
#[derive(Debug, Clone, Copy)]
pub struct ClickTiming {
    /// The double-click interval: `GetDoubleClickTime()`.
    pub double_click_ms: u32,
    /// The tap interval: half the double-click interval (`double_click_ms >> 1`).
    pub tap_ms: u32,
    /// Half the double-click rectangle's width: `GetSystemMetrics(SM_CXDOUBLECLK) / 2`.
    pub cx_dbl_click: i32,
    /// Half the double-click rectangle's height: `GetSystemMetrics(SM_CYDOUBLECLK) / 2`.
    pub cy_dbl_click: i32,
}

impl ClickTiming {
    /// The values the four Win32 calls give on a default Windows install: 500 ms and 4×4 pixels
    /// (`SM_CXDOUBLECLK` is 4 by default and the client halves it — `4 / 2 = 2`).
    #[must_use]
    pub const fn from_system(double_click_ms: u32, cx_dblclk: i32, cy_dblclk: i32) -> Self {
        Self {
            double_click_ms,
            tap_ms: double_click_ms >> 1,
            cx_dbl_click: cx_dblclk / 2,
            cy_dbl_click: cy_dblclk / 2,
        }
    }
}

impl Default for ClickTiming {
    fn default() -> Self {
        Self::from_system(500, 4, 4)
    }
}

/// The button history, with its 5000 ms purge.
#[derive(Debug, Default)]
pub struct ButtonHistory {
    entries: Vec<(ControlCode, ButtonHistoryEntry)>,
}

/// Activation-history entries older than this are purged.
pub const HISTORY_PURGE_MS: u32 = 5000;

impl ButtonHistory {
    #[must_use]
    pub fn get(&self, cs: ControlCode) -> Option<ButtonHistoryEntry> {
        self.entries.iter().find(|(k, _)| *k == cs).map(|(_, v)| *v)
    }

    pub fn set(&mut self, cs: ControlCode, e: ButtonHistoryEntry) {
        match self.entries.iter_mut().find(|(k, _)| *k == cs) {
            Some(slot) => slot.1 = e,
            None => self.entries.push((cs, e)),
        }
    }

    pub fn remove(&mut self, cs: ControlCode) {
        self.entries.retain(|(k, _)| *k != cs);
    }

    /// Message time wraps every 49.7 days, so the age is computed with wrapping arithmetic — the
    /// same subtraction the client does on the `DWORD`s.
    pub fn purge(&mut self, now: u32) {
        self.entries
            .retain(|(_, e)| now.wrapping_sub(e.time) <= HISTORY_PURGE_MS);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// The map stack `fire_input_event` walks: `{map_id, callback, priority}` sorted **descending by
/// priority** by the sorted insert.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InputMapEntry {
    pub map: InputMapId,
    pub priority: i32,
    pub callback: crate::CallbackId,
    /// Restrict an interface-specific registration to one action without changing its saved map.
    pub action_filter: Option<ActionId>,
}

/// The result of the map walk.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MapWalkResult {
    pub action: ActionId,
    pub input_map: InputMapId,
    pub binding: ControlChord,
    pub callback: crate::CallbackId,
}

/// Input-map priority walk, including both barriers.
///
/// * map id **2** (`MAP_BLOCK_ALL`) breaks the walk for everything;
/// * map id **1** (`MAP_BLOCK_KEYBOARD`) breaks it for a control on the **keyboard** device and is
///   skipped otherwise, which is why you can still click-to-select in the world while the chat bar
///   has focus.
///
/// Neither is a real map; neither contains bindings. A text element registering id 1 at
/// the focused-UI priority is the *entire* mechanism that stops typing from walking the
/// character — there is no "does the UI want this key?" test anywhere in the client.
///
/// `lookup` resolves an input map id to its binding list, if the keymap defines one.
pub fn walk_input_maps<'a, F>(
    stack: &[InputMapEntry],
    event: &ControlChord,
    is_keyboard: bool,
    lookup: F,
) -> Option<MapWalkResult>
where
    F: Fn(InputMapId) -> Option<&'a crate::keymap::InputMap>,
{
    let mut best: Option<MapWalkResult> = None;
    for entry in stack {
        if entry.map == MAP_BLOCK_ALL {
            break;
        }
        if entry.map == MAP_BLOCK_KEYBOARD {
            if is_keyboard {
                break;
            }
            continue;
        }
        let Some(map) = lookup(entry.map) else {
            continue;
        };
        let Some((action, binding)) = map.find_best_match_for_action(event, entry.action_filter)
        else {
            continue;
        };
        let better = match &best {
            None => true,
            Some(b) => event.is_better_match(&binding, &b.binding),
        };
        if better {
            best = Some(MapWalkResult {
                action,
                input_map: entry.map,
                binding,
                callback: entry.callback,
            });
        }
    }
    best
}

/// The four actions suppressed when the cursor is over the HTML
/// help overlay: left click, right click, left double-click and right double-click, so the help
/// browser gets its own clicks.
pub const KEYSTONE_SUPPRESSED_ACTIONS: [ActionId; 4] =
    [ActionId(7), ActionId(8), ActionId(10), ActionId(11)];

/// Four special action ids. 0 and 1 do nothing; 2 and 3 accumulate
/// the non-mouse pointer movement, i.e. a joystick driving the pointer.
pub const ACTION_POINTER_X: ActionId = ActionId(2);
pub const ACTION_POINTER_Y: ActionId = ActionId(3);

#[cfg(test)]
mod tests {
    use super::*;
    use crate::keymap::InputMap;
    use crate::spec::SubControlIndex;
    use crate::CallbackId;

    fn key(offset: u16) -> ControlCode {
        ControlCode::new(0, SubControlIndex::None, offset)
    }

    /// Records activation history for matching controls.
    #[test]
    fn button_data_becomes_down_or_up() {
        assert_eq!(
            di_data_to_activation_type(ControlType::Button, 0x80),
            (activation::DOWN, 1.0)
        );
        assert_eq!(
            di_data_to_activation_type(ControlType::Button, 0),
            (activation::UP, 0.0)
        );
        let (a, e) = di_data_to_activation_type(ControlType::AbsAxis, 16);
        assert_eq!(a, activation::ANALOG);
        assert!((e - 1.0).abs() < 1e-6);
    }

    /// entries older than 5000 ms are purged, and the
    /// arithmetic is on wrapping `DWORD`s.
    #[test]
    fn history_purges_at_five_seconds_and_survives_wrap() {
        let mut h = ButtonHistory::default();
        h.set(
            key(0x11),
            ButtonHistoryEntry {
                time: 1000,
                mouse_pos: (0, 0),
            },
        );
        h.set(
            key(0x12),
            ButtonHistoryEntry {
                time: 5500,
                mouse_pos: (0, 0),
            },
        );
        h.purge(6001);
        assert!(h.get(key(0x11)).is_none());
        assert!(h.get(key(0x12)).is_some());

        let mut h = ButtonHistory::default();
        h.set(
            key(0x11),
            ButtonHistoryEntry {
                time: u32::MAX - 10,
                mouse_pos: (0, 0),
            },
        );
        h.purge(100);
        assert!(
            h.get(key(0x11)).is_some(),
            "the wrap must not look like five seconds ago"
        );
    }

    /// map 1 stops keyboard controls and
    /// lets mouse controls through; map 2 stops everything.
    #[test]
    fn the_two_barriers_do_what_the_trap_says() {
        let cb = CallbackId(1);
        let mut world = InputMap::new(InputMapId(4));
        let k = key(0x11);
        world.add_mapping(ControlChord::new(k, 0, activation::CLICK), ActionId(0x29));
        let mouse = ControlCode::new(1, SubControlIndex::None, 0x0C);
        let mut ui = InputMap::new(InputMapId(3));
        ui.add_mapping(ControlChord::new(mouse, 0, activation::CLICK), ActionId(7));

        let lookup = |m: InputMapId| -> Option<&InputMap> {
            match m.0 {
                4 => Some(&world),
                3 => Some(&ui),
                _ => None,
            }
        };
        // The chat bar has focus: barrier 1 at 3000, the movement map at 1000.
        let stack = [
            InputMapEntry {
                map: MAP_BLOCK_KEYBOARD,
                priority: 3000,
                callback: cb,
                action_filter: None,
            },
            InputMapEntry {
                map: InputMapId(4),
                priority: 1000,
                callback: cb,
                action_filter: None,
            },
            InputMapEntry {
                map: InputMapId(3),
                priority: 0,
                callback: cb,
                action_filter: None,
            },
        ];
        let ev = ControlChord::new(k, 0, activation::DOWN | activation::LIVE);
        assert!(
            walk_input_maps(&stack, &ev, true, lookup).is_none(),
            "W must not walk the character"
        );

        let ev = ControlChord::new(mouse, 0, activation::DOWN | activation::LIVE);
        let hit =
            walk_input_maps(&stack, &ev, false, lookup).expect("the mouse passes the barrier");
        assert_eq!(hit.action, ActionId(7));

        // Map 2 stops the mouse too.
        let stack = [
            InputMapEntry {
                map: MAP_BLOCK_ALL,
                priority: 3000,
                callback: cb,
                action_filter: None,
            },
            InputMapEntry {
                map: InputMapId(3),
                priority: 0,
                callback: cb,
                action_filter: None,
            },
        ];
        assert!(walk_input_maps(&stack, &ev, false, lookup).is_none());
    }
}
