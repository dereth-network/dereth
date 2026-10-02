//! Global messages — the flat broadcast channel.
//!
//! Global UI-message registration and delivery live here.
//!
//! Broadcasting a global message walks the listener table entry for its id and invokes every registered
//! listener. **No bubbling, no serial-number filter, no return value.**
//!
//! Ten ids are live. Ids 4, 8, 9 and 0x0A are unused in this build.

use crate::MessageId;

/// 1 — **key down** not consumed by the focused or active element.
/// Broadcast by the input manager. There are 19 registration sites, including
/// the waiting dialog (so `Esc` dismisses it) and the chat interface.
pub const KEY_DOWN_UNCONSUMED: MessageId = MessageId(1);

/// 2 — **key up** not consumed.
pub const KEY_UP_UNCONSUMED: MessageId = MessageId(2);

/// 3 — **frame tick**, broadcast every frame.
///
/// The single most important id in the UI: there is no recursive per-frame walk of the element
/// tree, so media playback, meters, scrollbars, text elements, buttons, dialogs and the screen
/// flow register for this **only while they have work** and unregister as soon as they are
/// idle. There are 75 registration sites. The flow controller performs the UI-mode
/// switch here, which is why the mode switch must run last.
pub const TICK: MessageId = MessageId(3);

/// 5 — **display refresh / device reset**, hooked to the renderer's reset callback.
pub const REFRESH: MessageId = MessageId(5);

/// 6 — **element created**: when the element's notify-on-create flag is set.
pub const ELEMENT_CREATED: MessageId = MessageId(6);

/// 7 — **element visibility / geometry changed**: set-visible,
/// [`crate::UiSystem::resize_to`].
pub const ELEMENT_GEOMETRY: MessageId = MessageId(7);

/// 0x0B — **login complete**: the player object and its whole inventory exist.
pub const LOGIN_COMPLETE: MessageId = MessageId(0x0B);

/// 0x0C — **apply the mouse-turning settings**: the Client Options page sets its six
/// mouse-turning rows and the key bindings page binds the wheel to the camera zoom.
pub const MOUSE_TURNING_DEFAULTS: MessageId = MessageId(0x0C);

/// 0x0D — **UI lock toggled**. All twelve floaty windows listen; they
/// enable and disable their drag bars and resize bars.
pub const UI_LOCK_TOGGLED: MessageId = MessageId(0x0D);

/// 0x0E — **game window moved or resized** (windowed mode `SetWindowPos`).
pub const WINDOW_MOVED: MessageId = MessageId(0x0E);

/// Every live id, in the order the documentation table lists them. Ids 4, 8, 9 and 0x0A are absent
/// because they are unused in this build.
pub const LIVE_IDS: [MessageId; 10] = [
    KEY_DOWN_UNCONSUMED,
    KEY_UP_UNCONSUMED,
    TICK,
    REFRESH,
    ELEMENT_CREATED,
    ELEMENT_GEOMETRY,
    LOGIN_COMPLETE,
    MOUSE_TURNING_DEFAULTS,
    UI_LOCK_TOGGLED,
    WINDOW_MOVED,
];
