//! The pointer held for a camera drag, as a page holds it: locked to the canvas.
//!
//! The page cannot hide or place the pointer itself; it can ask the browser to lock it to the
//! canvas, which hides it and keeps it where it is until it is unlocked, and it reads the mouse's
//! own movement off every pointer event. The client's hold ([`PagePointer`]) is kept here as
//! [`PageLock`], which the page reads once a frame: whether to lock or unlock the pointer
//! ([`PageLock::take_request`]). While the client holds the pointer the page's pointer movements
//! reach it as the mouse's movement ([`HostEvent::PointerMotion`]), whether the browser granted
//! the lock or not, so a drag still turns the camera with the lock refused, the pointer shown.
//!
//! A browser locks the pointer only soon after the player pressed a key or a button on the page,
//! which a camera drag always follows; a lock the browser refuses leaves the pointer shown. When
//! the lock ends the browser shows the pointer where it was locked, which is where the drag began
//! or a few pixels from it.
//!
//! The page also says which buttons are down with each movement, so a button let go of where the
//! page never heard it (outside the page, after the browser ended the lock) is let go of here.

use std::cell::RefCell;

use dereth_client_shell::pointer::HostPointer;
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;

/// The client's hold on the pointer, and the buttons the page has said are down.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PageLock {
    /// The client holds the pointer: the page's pointer movements are the mouse's movement.
    held: bool,
    /// What the page is to do: lock the pointer (`true`) or unlock it (`false`); `None` once it
    /// has been told.
    request: Option<bool>,
    /// The buttons the page has said are down, as the page's own `buttons` bits.
    down: u16,
}

/// The page's `buttons` bit for its button number `button` (0 left, 1 middle, 2 right, 3 back,
/// 4 forward); none for another.
const fn button_bit(button: u16) -> u16 {
    match button {
        0 => 1,
        1 => 4,
        2 => 2,
        3 => 8,
        4 => 16,
        _ => 0,
    }
}

/// The button the page numbers `button`.
#[must_use]
pub const fn page_button(button: u16) -> MouseButton {
    match button {
        0 => MouseButton::Left,
        1 => MouseButton::Middle,
        2 => MouseButton::Right,
        3 => MouseButton::Back,
        4 => MouseButton::Forward,
        n => MouseButton::Other(n),
    }
}

impl PageLock {
    /// The client holds the pointer: the page is to lock it.
    pub fn capture(&mut self) {
        self.held = true;
        self.request = Some(true);
    }

    /// The client lets the pointer go: the page is to unlock it.
    pub fn release(&mut self) {
        self.held = false;
        self.request = Some(false);
    }

    /// Whether the client holds the pointer.
    #[must_use]
    pub const fn held(&self) -> bool {
        self.held
    }

    /// What the page is to do with the pointer, once: lock it (`true`) or unlock it (`false`).
    pub fn take_request(&mut self) -> Option<bool> {
        self.request.take()
    }

    /// Button `button`, by the page's numbering, went down or up.
    pub fn button(&mut self, button: u16, pressed: bool) {
        let bit = button_bit(button);
        if pressed {
            self.down |= bit;
        } else {
            self.down &= !bit;
        }
    }

    /// The pointer moved to `(x, y)` in canvas pixels, the mouse by `movement`, with the buttons
    /// `buttons` down when the page says (its `buttons` bits): the events the client is to see.
    /// A button the page said was down and is not is let go of first; then the movement, as the
    /// mouse's while the client holds the pointer and as the pointer's otherwise.
    pub fn moved(
        &mut self,
        (x, y): (f64, f64),
        movement: (f64, f64),
        buttons: Option<u16>,
    ) -> Vec<HostEvent> {
        let mut events = Vec::new();
        if let Some(buttons) = buttons {
            for button in 0..5 {
                let bit = button_bit(button);
                if self.down & bit != 0 && buttons & bit == 0 {
                    self.down &= !bit;
                    events.push(HostEvent::MouseInput {
                        button: page_button(button),
                        pressed: false,
                    });
                }
            }
        }
        if !self.held {
            events.push(HostEvent::CursorMoved { x, y });
        } else if movement != (0.0, 0.0) {
            events.push(HostEvent::PointerMotion {
                dx: movement.0,
                dy: movement.1,
            });
        }
        events
    }
}

thread_local! {
    /// The client's hold on the pointer, between the client and the page.
    static PAGE_LOCK: RefCell<PageLock> = RefCell::new(PageLock::default());
}

/// Read or change the page's hold on the pointer.
pub fn with_page_lock<T>(f: impl FnOnce(&mut PageLock) -> T) -> T {
    PAGE_LOCK.with(|lock| f(&mut lock.borrow_mut()))
}

/// The page's hold on the pointer, as the client shell's host holds it.
#[derive(Debug, Default, Clone, Copy)]
pub struct PagePointer;

impl HostPointer for PagePointer {
    /// Always held: the movement reaches the client as the mouse's whether the browser locks the
    /// pointer or not.
    fn capture(&mut self, _at: (f64, f64)) -> bool {
        with_page_lock(PageLock::capture);
        true
    }

    /// The browser shows the pointer where it locked it; a page cannot place it.
    fn release(&mut self, _at: (f64, f64)) {
        with_page_lock(PageLock::release);
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the page's pointer lock for a camera drag, and its button bookkeeping).
    use super::*;

    /// Holding the pointer asks the page once to lock it and letting it go once to unlock it;
    /// while it is held the page's pointer movements are the mouse's movement, and a movement of
    /// nothing is not reported.
    #[test]
    fn a_held_pointer_locks_the_page_and_its_movement_is_the_mouses() {
        let mut lock = PageLock::default();
        assert_eq!(
            lock.moved((10.0, 20.0), (1.0, 0.0), None),
            [HostEvent::CursorMoved { x: 10.0, y: 20.0 }]
        );
        assert_eq!(lock.take_request(), None);
        lock.capture();
        assert_eq!(lock.take_request(), Some(true));
        assert_eq!(lock.take_request(), None, "asked once");
        assert_eq!(
            lock.moved((10.0, 20.0), (4.0, -3.0), None),
            [HostEvent::PointerMotion { dx: 4.0, dy: -3.0 }]
        );
        assert_eq!(lock.moved((10.0, 20.0), (0.0, 0.0), None), []);
        lock.release();
        assert_eq!(lock.take_request(), Some(false));
        assert_eq!(
            lock.moved((12.0, 20.0), (2.0, 0.0), None),
            [HostEvent::CursorMoved { x: 12.0, y: 20.0 }]
        );
    }

    /// A button the page said was down and moves without is let go of before the movement,
    /// once; a movement that says nothing of the buttons lets nothing go.
    #[test]
    fn a_button_let_go_where_the_page_never_heard_it_is_let_go_with_the_next_movement() {
        let mut lock = PageLock::default();
        lock.button(2, true);
        lock.button(0, true);
        assert_eq!(
            lock.moved((1.0, 1.0), (0.0, 0.0), None),
            [HostEvent::CursorMoved { x: 1.0, y: 1.0 }]
        );
        assert_eq!(
            lock.moved((1.0, 1.0), (0.0, 0.0), Some(3)),
            [HostEvent::CursorMoved { x: 1.0, y: 1.0 }],
            "both still down"
        );
        assert_eq!(
            lock.moved((2.0, 1.0), (1.0, 0.0), Some(1)),
            [
                HostEvent::MouseInput {
                    button: MouseButton::Right,
                    pressed: false
                },
                HostEvent::CursorMoved { x: 2.0, y: 1.0 }
            ]
        );
        assert_eq!(
            lock.moved((3.0, 1.0), (1.0, 0.0), Some(1)),
            [HostEvent::CursorMoved { x: 3.0, y: 1.0 }],
            "let go of once"
        );
        lock.button(0, false);
        assert_eq!(
            lock.moved((3.0, 1.0), (0.0, 0.0), Some(0)),
            [HostEvent::CursorMoved { x: 3.0, y: 1.0 }],
            "already let go of"
        );
    }

    /// The client's hold reaches the page through the host's pointer.
    #[test]
    fn the_hosts_pointer_is_the_pages_lock() {
        with_page_lock(|lock| *lock = PageLock::default());
        let mut pointer = PagePointer;
        assert!(pointer.capture((5.0, 5.0)));
        assert!(with_page_lock(|lock| lock.held()));
        assert_eq!(with_page_lock(PageLock::take_request), Some(true));
        pointer.release((5.0, 5.0));
        assert!(!with_page_lock(|lock| lock.held()));
        assert_eq!(with_page_lock(PageLock::take_request), Some(false));
    }
}
