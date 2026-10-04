//! What the player does: [`Player`], the `when` step that drives the client from this side.
//!
//! Three shapes, because the client has three seams a player's hand reaches:
//!
//! | step | seam | backend |
//! |---|---|---|
//! | [`Player::DoubleClick`] | the game model's own use entry point | both |
//! | [`Player::Ui`] | the requests a panel raises, through the interaction layer | both |
//! | [`Player::Hold`] | one movement action, begun and ended, in the runtime's action queue | `App` only |
//! | [`Player::Click`] | the pointer, through the client's own pump and input manager | `App` only |
//! | [`Player::Drag`] | the same pointer, pressed, moved and released | `App` only |
//! | [`Player::Grab`] / [`Player::Over`] / [`Player::Drop`] / [`Player::Release`] | the same drag, in steps, so a scenario can look at the screen while the icon is in the air | `App` only |
//! | [`Player::Press`] / [`Player::Type`] / [`Player::Focus`] | the keyboard, over [`crate::input_steps`] | `App` only |
//!
//! # The keyboard
//!
//! [`crate::input_steps`] is the crate's one key driver and is the implementation: the functions there take a
//! `&mut HeadlessClient` and are what a scenario reaches for when it wants a key *edge* -- the
//! down without the up, the tap, the bound scan code, the return. What the three variants add is
//! that a keyboard gesture can sit in a `when` list beside a pointer one, which is what a
//! scenario that walks a page reads like. Each is one line over the function it names; there is
//! no second implementation of any of them.
//!
//! [`Player::Hold`] drives an application, because the action queue is the runtime's; it goes in
//! as an action, not a key, because it is about the movement and not the binding. The direction ids
//! are the client's own -- this crate takes them from [`dereth_headless::script::Direction`] rather
//! than writing a second table of them.
//!
//! # The pointer
//!
//! [`Player::Click`] and [`Player::Drag`] are the gestures most inventory and panel scenarios are
//! written in, and each needs the same four Win32 messages. They are built here once, through [`dereth_client::pump::Pump`] -- the
//! client's **own** message mapper, so the scenario exercises the same mapping as the client --
//! and dispatched both to the pump's state and to the real input manager, which is what the
//! window loop does with them.
//!
//! A [`Target`] is an element, one row of a templated list, or a point. An element resolves to
//! the centre of its **clipped** box, because that is where the pointer would land on the part of
//! it that is on screen; a step against an element whose clip box is empty panics rather than
//! click at an arbitrary place, since an element that is drawing nothing cannot be under the
//! pointer.
//!
//! **A shipped element id does not always name one element.** A list box and an item grid build
//! every row from one template, so all of them carry the template's id -- a pack grid does it,
//! and so do many panels' lists. [`Target::Element`] **panics** on such an id rather than silently answering
//! the first one; [`Target::Nth`] names the row by its place, and [`Target::Point`] by its own
//! centre.
//!
//! The gesture's millisecond stamps come from the client's own counter, two seconds apart, so
//! that two clicks in one scenario are never read as one double click.
//!
//! # The drag in three steps
//!
//! [`Player::Drag`] is atomic -- it
//! presses, moves and releases inside one `when` -- and that is the right shape for "letting an
//! icon go over a pack moves the item". It cannot express the other half of the drag family,
//! which is everything the screen shows **while the icon is in the air**: the hints a hovered
//! target lights, the overlay a paper-doll slot draws under the cursor, the greyed source slot,
//! the proxy's own picture. Every one of those has to look at the screen between the press and
//! the release.
//!
//! So the same gesture is available in four steps. [`Player::Grab`] leaves the button **down**;
//! the client is in a live drag until a [`Player::Drop`] or a [`Player::Release`] ends it, and a
//! scenario that forgets to end one leaves the client in the state a player holding the mouse
//! down is in, which is a state the client is designed to be in.
//!
//! `UiSystem::mouse_move` starts a pending drag only once `dx² + dy² > 15` -- about four pixels.
//! [`Player::Grab`] moves by [`THRESHOLD_STEP`] in both axes, which is `128 > 15` with room to
//! spare.

use dereth_client_model::{RecordingRequests, RecordingSink};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_ui::ElementId;

pub use dereth_headless::script::Direction;

use dereth_client::app::HEADLESS_STEP;
use dereth_client_contract::UiRequest;

use crate::client::{Backend, HeadlessClient, Step};

/// One thing the player does.
#[derive(Debug, Clone)]
pub enum Player {
    /// Double-click an object in the world or in a container.
    ///
    /// This is the game model's own use entry point, and it is what the pick path ends in. Its
    /// notices land in the scenario's own sink, so [`HeadlessClient::notices`] carries them.
    ///
    /// [`HeadlessClient::notices`]: crate::HeadlessClient::notices
    DoubleClick(ObjectId),
    /// Raise UI requests, as a panel does when the player clicks it.
    Ui(Vec<UiRequest>),
    /// Hold one movement action for `secs` of simulated time, then release it.
    ///
    /// The start event begins the hold, the frames run, and the stop event ends it -- the same
    /// pair of edges a held key produces, without this crate transcribing a keyboard.
    Hold { direction: Direction, secs: f64 },
    /// Say a line, as typing into the chat entry and pressing return does.
    Say(String),
    /// Press and release the left button over `target`, and run the frames that dispatch what it
    /// raised. `App` only: the input manager is part of the UI shell.
    Click(Target),
    /// Press the left button over `from`, hold it for `hold_frames`, move to `to`, and release.
    ///
    /// The move is delivered **while the button is down**, which is what makes it a drag rather
    /// than two clicks: the element manager latches its drag origin on the press and reads the
    /// motion against it. `App` only.
    Drag {
        from: Target,
        to: Target,
        hold_frames: u64,
    },
    /// Press the pointer on this target and move far enough for the shell to call it a drag. The
    /// button is left **down**. See the module docs.
    Grab(Target),
    /// Move the pointer over this target with the button still down.
    Over(Target),
    /// Move the pointer over this target and let the button go there.
    Drop(Target),
    /// Let the button go where the pointer already is, without moving first.
    ///
    /// The gesture a player makes when they change their mind: a [`Player::Grab`] then a
    /// `Release` is a press and a drag that ended over nothing in particular.
    Release,
    /// One bound UI action, one-shot, as pressing the key it is bound to does.
    ///
    /// The **action** goes in rather than a scan code, because what key an action is bound to is
    /// the keymap's own claim and has rows of its own: a step that pressed `A` would be asserting
    /// over the shipped binding as well as over what the action does. The step that goes in at
    /// the key is [`crate::input_steps::press_bound`], and the difference between the two is
    /// deliberate. `App` only: the input manager is part of the UI shell.
    Press(dereth_input::ActionId),
    /// Type this text into whatever holds the keyboard, one character at a time.
    ///
    /// Not [`Player::Say`], which queues a whole chat line through the interaction layer: this is
    /// the keyboard, one `WM_CHAR` per character, which is the message an edit box reads.
    /// `App` only, and deliberately ASCII -- see [`crate::input_steps::type_text`].
    Type(String),
    /// Put the caret in this target: the pointer goes there, and the button goes down and up.
    ///
    /// It is [`Player::Click`] under the name of what it is for. A scenario whose subject is the
    /// **focus edges** -- a click that drops a caret in, a click elsewhere that commits what was
    /// typed -- reads as the claim it is making when the step says `Focus`. `App` only.
    Focus(Target),
}

/// The offset, in both axes, that [`Player::Grab`]'s move after the press uses. See the module
/// docs.
pub const THRESHOLD_STEP: i32 = 8;

/// Where a pointer gesture happens: an element of the shipped layout, one row of a templated
/// list, or a screen point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    /// The centre of this element's clipped box.
    ///
    /// The id must name **one** live element. A templated list's rows all carry the same shipped
    /// id, and this panics on one rather than silently answering the first; see [`Target::Nth`].
    Element(ElementId),
    /// The `index`-th live element carrying `id`, in tree order, counting from zero.
    ///
    /// A list box and an item grid build every row from one
    /// template, so the shipped id names the template and not a row, and a scenario whose claim
    /// is about the third row of a list has no other way to point at it. Tree order is the order
    /// the shell itself walks the subtree in, which for a list is top to bottom.
    Nth { id: ElementId, index: usize },
    /// An absolute screen position.
    Point(ScreenPoint),
}

/// An absolute screen position, in the client's own pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScreenPoint {
    pub x: i32,
    pub y: i32,
}

impl ScreenPoint {
    /// One point.
    #[must_use]
    pub const fn new(x: i32, y: i32) -> Self {
        Self { x, y }
    }
}

impl From<ElementId> for Target {
    fn from(id: ElementId) -> Self {
        Self::Element(id)
    }
}

impl From<ScreenPoint> for Target {
    fn from(p: ScreenPoint) -> Self {
        Self::Point(p)
    }
}

impl From<(i32, i32)> for Target {
    fn from((x, y): (i32, i32)) -> Self {
        Self::Point(ScreenPoint::new(x, y))
    }
}

impl Player {
    /// [`Player::Ui`] with one request.
    #[must_use]
    pub fn ui(r: UiRequest) -> Self {
        Self::Ui(vec![r])
    }

    /// [`Player::Click`] on an element of the shipped layout.
    #[must_use]
    pub const fn click(id: ElementId) -> Self {
        Self::Click(Target::Element(id))
    }

    /// [`Player::Click`] on the `index`-th live element carrying `id`. See [`Target::Nth`].
    #[must_use]
    pub const fn click_nth(id: ElementId, index: usize) -> Self {
        Self::Click(Target::Nth { id, index })
    }

    /// [`Player::Drag`] from one element to another, with one frame of hold.
    #[must_use]
    pub const fn drag(from: ElementId, to: ElementId) -> Self {
        Self::Drag {
            from: Target::Element(from),
            to: Target::Element(to),
            hold_frames: 1,
        }
    }
}

impl Step for Player {
    fn apply(self, client: &mut HeadlessClient) {
        match self {
            Self::DoubleClick(id) => double_click(client, id),
            Self::Ui(r) => ui(client, r),
            Self::Say(text) => ui(client, vec![UiRequest::ChatLine { text, window: 0 }]),
            Self::Hold { direction, secs } => hold(client, direction, secs),
            Self::Click(target) => click(client, target),
            Self::Drag {
                from,
                to,
                hold_frames,
            } => drag(client, from, to, hold_frames),
            Self::Grab(target) => grab(client, target),
            Self::Over(target) => over(client, target),
            Self::Drop(target) => drop_on(client, target),
            Self::Release => release(client),
            Self::Press(action) => crate::input_steps::press(client, action),
            Self::Type(text) => crate::input_steps::type_text(client, &text),
            Self::Focus(target) => crate::input_steps::focus(client, target),
        }
    }
}

/// Every live element under `roots` that carries `id`, in tree order.
///
/// A list box and an item grid build their rows and their slots from **one** template, so every
/// one of them carries the same shipped element id. That is what [`Target::Nth`] indexes and what
/// [`Target::Element`] refuses. See [`at`].
fn carriers(
    ui: &dereth_ui::UiSystem,
    roots: &[dereth_ui::ElemHandle],
    id: ElementId,
) -> Vec<dereth_ui::ElemHandle> {
    fn walk(
        ui: &dereth_ui::UiSystem,
        h: dereth_ui::ElemHandle,
        id: ElementId,
        out: &mut Vec<dereth_ui::ElemHandle>,
    ) {
        if ui.node(h).is_some_and(|k| k.element_id() == id) {
            out.push(h);
        }
        for c in ui.children(h) {
            walk(ui, c, id, out);
        }
    }
    let mut out = Vec::new();
    for r in roots {
        walk(ui, *r, id, &mut out);
    }
    out
}

/// Where on the screen a [`Target`] is.
///
/// # Panics
/// Panics on the model backend, on an element the shipped layout does not carry, on an element
/// whose clipped box is empty -- because an element that is drawing nothing cannot be under the
/// pointer, and clicking at an arbitrary place would be a gesture the player could not have made
/// -- and on a [`Target::Element`] whose id **more than one** live element carries.
///
/// A pack grid builds a hundred slots from one template and a list box builds its
/// rows the same way, so `Target::Element` on a slot would silently answer the **first** of them
/// however many the list is showing, and a scenario written that way passes while gesturing
/// somewhere the player never pointed. One of them is named by its place in the list
/// ([`Target::Nth`]) or by its own centre ([`Target::Point`]), and the panic says both, because
/// the first person to try the id will not guess.
fn at(client: &mut HeadlessClient, target: Target) -> (i32, i32) {
    let (id, want) = match target {
        Target::Point(p) => return (p.x, p.y),
        Target::Element(id) => (id, None),
        Target::Nth { id, index } => (id, Some(index)),
    };
    let app = client.app_mut();
    let shell = app.ui_mut().expect(
        "a pointer gesture on an element needs the UI shell; build the scenario with \
         ClientSpec::gameplay(..) or ClientSpec::screen(..)",
    );
    let roots: Vec<dereth_ui::ElemHandle> = shell
        .flow
        .current()
        .expect("a screen is current")
        .roots()
        .to_vec();
    let carrying = carriers(&shell.ui, &roots, id);
    assert!(
        !carrying.is_empty(),
        "{id:?} is not under this screen's roots"
    );
    let h = match want {
        None => {
            assert!(
                carrying.len() == 1,
                "{} live elements carry {id:?}, so it is a template's id and not one element's \
                 -- a grid's slots and a list box's rows are all built from one. Target::Element \
                 would answer the first of them whatever the player was pointing at. Name the \
                 one you mean: Target::Nth {{ id, index }} for its place in the list, or \
                 Target::Point at its own centre.",
                carrying.len()
            );
            carrying[0]
        }
        Some(i) => *carrying.get(i).unwrap_or_else(|| {
            panic!(
                "{id:?} is carried by {} live element(s) and this gesture asks for number {i}; \
                 a list that is not showing that row cannot have been pointed at",
                carrying.len()
            )
        }),
    };
    let b = shell.ui.screen_clip_box(h);
    assert!(
        b.is_valid(),
        "{id:?} is clipped away entirely, so nothing the player did could land on it"
    );
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// One pump message, delivered where the window loop delivers it: to the pump's own state, and to
/// the real input manager.
pub(crate) fn deliver(
    client: &mut HeadlessClient,
    pump: &mut dereth_client::pump::Pump,
    m: dereth_client::pump::Win32Message,
) {
    pump.dispatch(m);
    client
        .app_mut()
        .input_manager_mut()
        .expect("the input manager is part of the shell this scenario asked for")
        .on_message(m);
}

/// A pump with the two flags the window loop sets while the client is up.
pub(crate) fn pointer_pump() -> dereth_client::pump::Pump {
    let mut p = dereth_client::pump::Pump::new();
    p.state.is_ready = true;
    p.state.is_active_app = true;
    p
}

fn button(
    pump: &mut dereth_client::pump::Pump,
    down: bool,
    t: u32,
) -> dereth_client::pump::Win32Message {
    pump.button_message(dereth_client::platform::keys::MouseButton::Left, down, t)
        .expect("the left button is one of the messages the client's table names")
}

/// The frames a gesture needs for what it raised to reach the panels. Four, which is what every
/// station that drove this pointer by hand ran.
const SETTLE_FRAMES: u64 = 4;

fn click(client: &mut HeadlessClient, target: Target) {
    let (x, y) = at(client, target);
    let t = client.next_pointer_time();
    let mut pump = pointer_pump();
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), t);
    deliver(client, &mut pump, m);
    for (down, dt) in [(true, 1), (false, 2)] {
        let m = button(&mut pump, down, t + dt);
        deliver(client, &mut pump, m);
    }
    client.tick(SETTLE_FRAMES);
}

fn drag(client: &mut HeadlessClient, from: Target, to: Target, hold_frames: u64) {
    let (x0, y0) = at(client, from);
    let (x1, y1) = at(client, to);
    let t = client.next_pointer_time();
    let mut pump = pointer_pump();

    let m = pump.mouse_move_message(f64::from(x0), f64::from(y0), t);
    deliver(client, &mut pump, m);
    let m = button(&mut pump, true, t + 1);
    deliver(client, &mut pump, m);
    client.tick(hold_frames);

    // The motion is delivered while the button is down -- that is what makes this a drag. It goes
    // in two steps so that a widget whose drag threshold is measured in pixels sees the pointer
    // leave its origin before it arrives anywhere.
    for n in [1_i32, 2] {
        let x = x0 + (x1 - x0) * n / 2;
        let y = y0 + (y1 - y0) * n / 2;
        #[allow(clippy::cast_sign_loss)]
        let stamp = t + 2 + n as u32;
        let m = pump.mouse_move_message(f64::from(x), f64::from(y), stamp);
        deliver(client, &mut pump, m);
        client.tick(1);
    }

    let m = button(&mut pump, false, t + 10);
    deliver(client, &mut pump, m);
    client.tick(SETTLE_FRAMES);
}

// -------------------------------------------------------------------------------------------
// The same drag, in four steps. See the module docs.
// -------------------------------------------------------------------------------------------

fn grab(client: &mut HeadlessClient, target: Target) {
    let (x, y) = at(client, target);
    let t = client.next_pointer_time();
    let mut pump = pointer_pump();
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), t);
    deliver(client, &mut pump, m);
    let m = button(&mut pump, true, t + 1);
    deliver(client, &mut pump, m);
    client.tick(1);

    let (x, y) = (x + THRESHOLD_STEP, y + THRESHOLD_STEP);
    let mut pump = pointer_pump();
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), t + 2);
    deliver(client, &mut pump, m);
    client.tick(1);
}

fn over(client: &mut HeadlessClient, target: Target) {
    let (x, y) = at(client, target);
    let t = client.next_pointer_time();
    let mut pump = pointer_pump();
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), t);
    deliver(client, &mut pump, m);
    client.tick(1);
}

fn drop_on(client: &mut HeadlessClient, target: Target) {
    let (x, y) = at(client, target);
    let t = client.next_pointer_time();
    let mut pump = pointer_pump();
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), t);
    deliver(client, &mut pump, m);
    client.tick(1);
    let mut pump2 = pointer_pump();
    let m = pump2.mouse_move_message(f64::from(x), f64::from(y), t + 1);
    deliver(client, &mut pump2, m);
    let m = button(&mut pump2, false, t + 2);
    deliver(client, &mut pump2, m);
    client.tick(SETTLE_FRAMES);
}

fn release(client: &mut HeadlessClient) {
    let (x, y) = client
        .app_mut()
        .ui_mut()
        .expect("a pointer gesture needs the UI shell")
        .ui
        .mouse_pos();
    let t = client.next_pointer_time();
    let mut pump = pointer_pump();
    let m = pump.mouse_move_message(f64::from(x), f64::from(y), t);
    deliver(client, &mut pump, m);
    let m = button(&mut pump, false, t + 1);
    deliver(client, &mut pump, m);
    client.tick(SETTLE_FRAMES);
}

fn double_click(client: &mut HeadlessClient, id: ObjectId) {
    match client.backend_mut() {
        Backend::Model(m) => {
            // **Two clicks are not one click.** The use path refuses a second attempt that
            // arrives at the same instant as the first, which is how it stops a double-click from
            // being sent twice; a scenario whose steps all happened at time zero would silently
            // lose every gesture after the first. One step of the client's own clock per gesture
            // is what a player at a keyboard cannot help doing.
            m.now += dereth_client::app::HEADLESS_STEP;
            let now = ServerTime(m.now);
            let mut req = RecordingRequests::default();
            let mut sink = RecordingSink::default();
            let _outcome = m.objects.world.use_object(
                &mut req,
                &mut sink,
                id,
                dereth_client_model::inventory::SplitState::default(),
                now,
            );
            let requests = std::mem::take(&mut req.0);
            client.absorb_sink(sink);
            client.note_requests(requests);
        }
        Backend::App(_) => ui(client, vec![UiRequest::Use(id)]),
    }
}

fn ui(client: &mut HeadlessClient, requests: Vec<UiRequest>) {
    client.note_ui_requests(&requests);
    match client.backend_mut() {
        Backend::Model(m) => {
            // See `double_click`: one step of the client's own clock per gesture.
            m.now += dereth_client::app::HEADLESS_STEP;
            let now = ServerTime(m.now);
            m.interaction.queue(Vec::new(), requests);
            // **The second argument is the HUD's own flag, as `App::frame` passes it.** A fresh
            // HUD carries `false`; a scenario whose claim is about what a *logged-in* client
            // does -- the chat commands are the ones that care -- has no other way to say so, and
            // a hard-coded `false` would assert over a client that had never received its own
            // description.
            let logged_in = m.hud.player_desc_received;
            let unowned = m
                .interaction
                .run_ui_requests(&mut m.objects.world, logged_in, now);
            assert!(
                unowned.is_empty(),
                "the client has no production arm for {unowned:?}; a scenario that dropped them \
                 would be asserting over a gesture that went nowhere"
            );
        }
        Backend::App(app) => {
            app.probe_mut()
                .interaction_mut()
                .queue(Vec::new(), requests);
        }
    }
}

fn hold(client: &mut HeadlessClient, direction: Direction, secs: f64) {
    let Backend::App(_) = client.backend() else {
        panic!(
            "Player::Hold drives the runtime's action queue, which an application has; build \
             the scenario with ClientSpec::gameplay(..) or drive the model directly"
        )
    };
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let frames = (secs / HEADLESS_STEP).ceil().max(0.0) as u64;
    movement(client, direction, true);
    client.tick(frames);
    movement(client, direction, false);
    client.tick(1);
}

fn movement(client: &mut HeadlessClient, direction: Direction, start: bool) {
    use dereth_client_runtime::actions::Action;
    let Backend::App(app) = client.backend_mut() else {
        unreachable!("checked by the caller")
    };
    // The movement intent goes in as the action it is, into the runtime's queue: what key it is
    // bound to is the keymap's claim, and the keyboard path has scenarios of its own.
    let id = direction.action();
    app.inject_action(if start {
        Action::begin(id)
    } else {
        Action::end(id)
    });
}
