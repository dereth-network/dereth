//! The interfaces a host offers with Horizon among them: the game's own and Horizon; and a handle
//! that shows one interface from more than one place (a test that looks at the one shown).

use std::cell::RefCell;
use std::rc::Rc;

use auberean_shell::input::Event;
use auberean_shell::interface::{Choice, Game, Interface, InterfaceMaker, Overlay, Request};

/// One interface shown from more than one place: each handle drives the same interface. Only one
/// interface is shown at a time, so the handles never drive it at once.
pub struct Shared<I: ?Sized>(pub Rc<RefCell<I>>);

impl<I: ?Sized> std::fmt::Debug for Shared<I> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Shared")
    }
}

impl<I: ?Sized> Clone for Shared<I> {
    fn clone(&self) -> Self {
        Self(Rc::clone(&self.0))
    }
}

impl<I: Interface + ?Sized> Interface for Shared<I> {
    fn event(&mut self, event: &Event, game: &Game<'_>) -> bool {
        self.0.borrow_mut().event(event, game)
    }
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.borrow_mut().update(game, requests);
    }
    fn after_world(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.borrow_mut().after_world(game, requests);
    }
    fn draw(&mut self, game: &Game<'_>, overlay: &mut Overlay) {
        self.0.borrow_mut().draw(game, overlay);
    }
    fn before_view(&mut self, requests: &mut Vec<Request>) {
        self.0.borrow_mut().before_view(requests);
    }
    fn enter(&mut self, game: &Game<'_>) {
        self.0.borrow_mut().enter(game);
    }
    fn leave(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        self.0.borrow_mut().leave(game, requests);
    }
    fn release_textures(&mut self, overlay: &mut Overlay) {
        self.0.borrow_mut().release_textures(overlay);
    }
    fn wants_pointer(&self, game: &Game<'_>) -> bool {
        self.0.borrow().wants_pointer(game)
    }
    fn text_mode(&self) -> bool {
        self.0.borrow().text_mode()
    }
    fn hears_the_pointer_in_background(&self) -> bool {
        self.0.borrow().hears_the_pointer_in_background()
    }
    fn set_random(&mut self, random: &auberean_primitives::random::Random) {
        self.0.borrow_mut().set_random(random);
    }
    fn set_clipboard(&mut self, text: Option<String>) {
        self.0.borrow_mut().set_clipboard(text);
    }
    fn take_copied(&mut self) -> Option<String> {
        self.0.borrow_mut().take_copied()
    }
}

/// The maker of the interfaces a host offers: the game's own `retail` one, and Horizon as
/// `horizon` builds it the first time it is chosen (or why it can't be shown).
pub fn maker<'w>(
    retail: impl Interface + 'w,
    mut horizon: impl FnMut() -> Result<Box<dyn Interface + 'w>, String> + 'w,
) -> Box<dyn InterfaceMaker<'w> + 'w> {
    let mut retail: Option<Box<dyn Interface + 'w>> = Some(Box::new(retail));
    let mut horizon_made = false;
    Box::new(
        move |choice: Choice| -> Result<Box<dyn Interface + 'w>, String> {
            match choice {
                Choice::Retail => retail
                    .take()
                    .ok_or_else(|| "The retail interface is shown already".to_owned()),
                Choice::Horizon if !horizon_made => {
                    let made = horizon()?;
                    horizon_made = true;
                    Ok(made)
                }
                Choice::Horizon => Err("The Horizon interface is shown already".to_owned()),
            }
        },
    )
}
