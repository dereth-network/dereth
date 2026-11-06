//! The flow from one screen to the next: each screen known by a mode token and the function that
//! makes it; a screen asked for replaces the current one at the next tick of the clock.

use crate::listener::{ListenerId, Mail};
use crate::manager::global;
use crate::system::Ui;
use crate::Token;

/// One screen, as the flow drives it. `P` is what lives across screens.
pub trait Screen<P> {
    /// Show or hide the screen.
    fn show(&mut self, ui: &mut Ui, data: &mut P, on: bool);
    /// The flow's tick while the screen is up.
    fn update(&mut self, ui: &mut Ui, data: &mut P);
    /// The text the screen was asked for with (an error to show, a name), when there is one.
    fn argument(&mut self, _ui: &mut Ui, _data: &mut P, _argument: &str) {}
    /// The screen goes.
    fn close(self: Box<Self>, ui: &mut Ui, data: &mut P);
    /// The screen as itself, for whoever made it to reach its own parts.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        None
    }
    /// The same, to look at.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        None
    }
}

/// What makes a screen.
pub type Factory<P> = fn(&mut Ui, &mut P) -> Option<Box<dyn Screen<P>>>;

/// The screens, the current one, the one asked for next, and what lives across them.
pub struct Flow<P> {
    listener: ListenerId,
    /// The current screen's mode, and the mode asked for (0 for none).
    current_mode: Token,
    queued_mode: Token,
    current: Option<Box<dyn Screen<P>>>,
    argument: String,
    factories: Vec<(Token, Factory<P>)>,
    /// What lives across screens.
    // D2-11875
    pub data: P,
}

impl<P> std::fmt::Debug for Flow<P> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Flow")
            .field("current_mode", &self.current_mode)
            .field("queued_mode", &self.queued_mode)
            .finish_non_exhaustive()
    }
}

impl<P> Flow<P> {
    /// A flow with no screen up, listening to the clock, keeping `data` across screens.
    // D2-11851
    pub fn new(ui: &mut Ui, data: P) -> Self {
        let listener = ui.new_listener();
        ui.listen_for_global_message(listener, global::TIME);
        Self {
            listener,
            current_mode: Token(0),
            queued_mode: Token(0),
            current: None,
            argument: String::new(),
            factories: Vec::new(),
            data,
        }
    }

    /// The flow goes: the screen up goes, and the flow stops listening.
    // D2-11852, D2-11854
    pub fn close(mut self, ui: &mut Ui) {
        if let Some(s) = self.current.take() {
            s.close(ui, &mut self.data);
        }
        ui.drop_listener(self.listener);
    }

    /// Know a screen by its mode (the first function given for a mode stays).
    // D2-11856, D2-11850, D2-11855
    pub fn register(&mut self, mode: Token, make: Factory<P>) {
        if !self.factories.iter().any(|(m, _)| *m == mode) {
            self.factories.push((mode, make));
        }
    }

    /// Ask for a screen, with a text for it: it comes up at the next tick.
    // D2-11853
    pub fn queue_mode(&mut self, mode: Token, argument: &str) {
        self.queued_mode = mode;
        argument.clone_into(&mut self.argument);
    }

    /// The current screen's mode.
    #[must_use]
    pub const fn mode(&self) -> Token {
        self.current_mode
    }

    /// The mode asked for and not yet up (0 for none).
    #[must_use]
    pub const fn queued_mode(&self) -> Token {
        self.queued_mode
    }

    /// The screen up.
    // D2-15480
    #[must_use]
    pub fn current(&self) -> Option<&dyn Screen<P>> {
        self.current.as_deref()
    }

    /// What lives across screens.
    #[must_use]
    pub const fn data(&self) -> &P {
        &self.data
    }

    /// The screen up, to tell it something, with what lives across screens.
    pub fn current_mut(&mut self) -> (Option<&mut (dyn Screen<P> + 'static)>, &mut P) {
        (self.current.as_deref_mut(), &mut self.data)
    }

    /// The listener the flow hears with: a game's own flow registers it for more than the clock
    /// and takes what else it heard from [`Self::tick`].
    #[must_use]
    pub const fn listener(&self) -> ListenerId {
        self.listener
    }

    /// The flow's tick: a screen asked for comes up when the clock ticks, then the screen up
    /// has its tick. Returns what else the flow's listener heard, oldest first.
    // D2-11858, D2-11849
    pub fn tick(&mut self, ui: &mut Ui) -> Vec<Mail> {
        let mut other = Vec::new();
        for m in ui.take_mail(self.listener) {
            if let Mail::Global {
                what: global::TIME, ..
            } = m
            {
                if self.queued_mode != Token(0) {
                    self.use_new_mode(ui);
                }
            } else {
                other.push(m);
            }
        }
        if let Some(s) = self.current.as_mut() {
            s.update(ui, &mut self.data);
        }
        other
    }

    /// Bring up the screen asked for, when its mode is known: the current one hidden and gone,
    /// the new one made, given its text (when there is one) and shown.
    // D2-11857
    fn use_new_mode(&mut self, ui: &mut Ui) {
        let mode = self.queued_mode;
        let Some(&(_, make)) = self.factories.iter().find(|(m, _)| *m == mode) else {
            return;
        };
        if let Some(mut s) = self.current.take() {
            s.show(ui, &mut self.data, false);
            s.close(ui, &mut self.data);
        }
        self.current_mode = mode;
        self.queued_mode = Token(0);
        let argument = std::mem::take(&mut self.argument);
        if let Some(mut s) = make(ui, &mut self.data) {
            if !argument.is_empty() {
                s.argument(ui, &mut self.data, &argument);
            }
            s.show(ui, &mut self.data, true);
            self.current = Some(s);
        }
    }
}
