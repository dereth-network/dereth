//! Horizon's screens before the world: connecting (the login and the data download), character
//! select, character creation ([`crate::ui::creation`]), entering the world, the credits, and the
//! screen that says why the session ended.
//!
//! **Character select** stands over a view of the world ([`crate::vista`]): the client's name and
//! the world it reached (its name, and a lamp saying it is online) along the top, the account's
//! characters down the right, a card each, the selected one standing in the middle as it looks,
//! and the buttons on a darker band along the foot, with the client's version under them. A click
//! selects a character, and a click on the one selected (or Enter) asks to enter the world with
//! it. Delete asks before deleting; a character being deleted shows how long until it is gone, and
//! the same button restores it, asking first. Create opens creation while the account has room;
//! Credits shows the game's credits; Quit closes the program. Creation stands over the same view.

use auberean_client_sdk::contract::session::{LoginProgress, Phase};
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::creation::Creation;
use crate::draw::Rect;
use crate::pregame::{Character, Download, Made, Pregame};
use crate::ui::vk;

/// The client's name, as the screens before the world show it.
pub const PRODUCT: &str = "Auberean";

/// The piece of Horizon's art with the client's mark: its picture with its name beside it.
pub const BRAND_MARK: &str = "brand.mark.auberean";

/// How fast the credits scroll, in layout units a second.
pub const CREDITS_SPEED: f32 = 40.0;

/// The screen shown.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Screen {
    Connecting,
    Select,
    Creation,
    /// Entering the world, until portal space shows.
    Entering,
    /// Portal space showing, entering the world or after logging out: nothing over it, as the game's
    /// own interface leaves it.
    Passage,
    Credits,
    Disconnected,
}

/// What the player asked for before the world in a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PreOut {
    /// The client's actions asked for, in order.
    pub acts: Vec<Action>,
    /// Log in again.
    pub connect: bool,
    /// Close the program.
    pub quit: bool,
    /// Begin making a character: the interface makes its avatars.
    pub create: bool,
}

/// A question character select asks before it acts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Confirm {
    Enter(u64, String),
    Delete(u64, String),
    Restore(u64, String),
}

/// Where character select puts its parts.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SelectLayout {
    pub list: Rect,
    pub enter: Rect,
    pub create: Rect,
    pub delete: Rect,
    pub credits: Rect,
    pub quit: Rect,
    /// Where the selected character stands.
    pub figure: Rect,
    /// The darker bands along the top and the foot, so the words read over the world; the
    /// buttons stand on the foot's.
    pub top: Rect,
    pub foot: Rect,
    /// Where the world's line stands, its letters centered on the rectangle's height.
    pub world: Rect,
    /// The top left of the client's version, at the very foot.
    pub version: (f32, f32),
}

/// How tall the darker bands along the top and the foot of the screens are, in layout units.
pub const TOP_BAND: f32 = 180.0;
pub const FOOT_BAND: f32 = 110.0;

/// The darker bands' colors.
pub const TOP_SHADE: u32 = 0x7000_0000;
pub const FOOT_SHADE: u32 = 0x6000_0000;

/// The line, in layout units from the top, the world's name and the character count stand on.
const COUNT_LINE: f32 = 160.0;

impl SelectLayout {
    /// The parts on a screen of `screen` pixels at `k` pixels a layout unit.
    #[must_use]
    pub fn new(screen: (f32, f32), k: f32) -> Self {
        let (sw, sh) = screen;
        let top = 196.0 * k;
        let foot = Rect::new(0.0, sh - FOOT_BAND * k, sw, FOOT_BAND * k);
        let list_w = 360.0 * k;
        let buttons_h = 2.0 * 34.0 * k + 10.0 * k;
        let list = Rect::new(
            sw - list_w - 40.0 * k,
            top,
            list_w,
            foot.y + 54.0 * k - top - buttons_h - 12.0 * k,
        );
        let enter = Rect::new(list.x, foot.y + 12.0 * k, list.w, 34.0 * k);
        let half = (list.w - 10.0 * k) / 2.0;
        let create = Rect::new(list.x, enter.bottom() + 8.0 * k, half, 30.0 * k);
        let delete = Rect::new(create.right() + 10.0 * k, create.y, half, 30.0 * k);
        let quit = Rect::new(40.0 * k, create.y, 120.0 * k, create.h);
        let credits = Rect::new(quit.right() + 10.0 * k, quit.y, 120.0 * k, quit.h);
        let figure_w = (list.x - 96.0 * k - 400.0 * k).max(200.0 * k);
        let figure = Rect::new(
            list.x - 48.0 * k - figure_w,
            150.0 * k,
            figure_w,
            foot.y - 10.0 * k - 150.0 * k,
        );
        Self {
            list,
            enter,
            create,
            delete,
            credits,
            quit,
            figure,
            top: Rect::new(0.0, 0.0, sw, TOP_BAND * k),
            foot,
            world: Rect::new(
                40.0 * k,
                (COUNT_LINE - 30.0) * k,
                list.x - 80.0 * k,
                60.0 * k,
            ),
            version: (25.0 * k, sh - 17.0 * k),
        }
    }

    /// The card of the list's `i`th character, the list scrolled by `offset` pixels.
    #[must_use]
    pub fn card(&self, i: usize, offset: f32, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let y = self.list.y + (CARD + GAP) * k * i as f32 - offset;
        Rect::new(self.list.x, y, self.list.w - 10.0 * k, CARD * k)
    }
}

/// A character's card and the room between cards, in layout units.
const CARD: f32 = 60.0;
const GAP: f32 = 8.0;

/// The pieces of Horizon's art a character's card is drawn with: lit while it is selected or
/// under the pointer.
pub const CARD_NORMAL: &str = "card.normal";
pub const CARD_SELECTED: &str = "card.selected";

/// Where the picture on a card stands, in its left piece's own units, when the art does not say.
const PORTRAIT: [f32; 4] = [27.0, 27.0, 109.0, 110.0];

/// How far the picture in a card's well stands clear of the well's edges, in layout units.
pub const WELL_MARGIN: f32 = 3.0;

/// The lamp beside the world's name, lit: the world is online.
pub const ONLINE: u32 = 0xFF7B_D67B;

/// What character select drew last, as a test reads it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SelectShown {
    /// The client's version, and the top left it was written at.
    pub version: Option<(String, (f32, f32))>,
    /// The world's name, the word beside its lamp, and the lamp.
    pub world: Option<(String, &'static str, Rect)>,
    /// Each card drawn: its rectangle and the pieces it was drawn with (none: drawn plainly, the
    /// art having no card).
    pub cards: Vec<(Rect, Option<&'static str>)>,
    /// Each card's well, where the client's picture stands, for the cards drawn with Horizon's
    /// art.
    pub wells: Vec<Rect>,
    /// Where the client's mark was drawn along the top (none: its name written plainly, the art
    /// having no mark).
    pub brand: Option<Rect>,
}

/// Where the disconnected and connecting screens put their buttons.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PanelLayout {
    pub panel: Rect,
    /// Log in again (the disconnected screen, with an account to log in with).
    pub again: Rect,
    pub quit: Rect,
}

impl PanelLayout {
    #[must_use]
    pub fn new(screen: (f32, f32), k: f32) -> Self {
        let (w, h) = (520.0 * k, 230.0 * k);
        let panel = Rect::new(
            ((screen.0 - w) / 2.0).round(),
            ((screen.1 - h) / 2.0).round(),
            w,
            h,
        );
        let by = panel.bottom() - 52.0 * k;
        let quit = Rect::new(panel.right() - 150.0 * k, by, 130.0 * k, 32.0 * k);
        let again = Rect::new(quit.x - 140.0 * k, by, 130.0 * k, 32.0 * k);
        Self { panel, again, quit }
    }
}

/// Horizon's screens before the world.
#[derive(Debug, Default)]
pub struct Screens {
    /// The selected character, by its place in the list.
    pub selected: usize,
    /// The character to select when the list next holds it (one just made).
    pub select_id: Option<u64>,
    /// A question asked and not yet answered.
    pub confirm: Option<Confirm>,
    /// The character being made, while creation is up.
    pub creation: Option<Creation>,
    pub creation_window: crate::ui::creation::CreationWindow,
    /// The credits are shown: since when.
    pub credits: Option<f64>,
    /// The player asked to enter the world, and waits.
    pub entering: bool,
    /// A line character select shows under the list: why the last entry was refused.
    pub error: Option<String>,
    /// Where the selected character stands this frame, and the mark in the draw list it stands
    /// over.
    pub figure: Option<(Rect, usize)>,
    /// The parts of the question box last drawn: its buttons.
    pub confirm_buttons: Vec<Rect>,
    /// The running client's version, as its host names it.
    pub version: String,
    /// What character select drew last.
    pub shown: SelectShown,
    list_scroll: f32,
    time: f64,
}

/// The heading style.
fn heading(ctx: &Ctx<'_>, size: f32) -> TextStyle {
    TextStyle::new(Family::Heading, size, ctx.colours.heading()).edge(0xFF00_0000)
}

/// The body style.
fn body(ctx: &Ctx<'_>, size: f32) -> TextStyle {
    TextStyle::new(Family::Body, size, ctx.colours.text()).edge(ctx.colours.edge())
}

/// A count of seconds as minutes and seconds.
#[must_use]
pub fn clock(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = seconds.ceil().max(0.0) as u64;
    format!("{}:{:02}", s / 60, s % 60)
}

/// What the connecting screen says of the login and the download.
#[must_use]
pub fn connecting_line(pre: &Pregame, has_login: bool) -> String {
    match pre.download {
        Download::Looking => return "Updating...".to_owned(),
        Download::Waiting(n) => return format!("Waiting to update: {n} ahead."),
        Download::Fetching { .. } => return "Updating...".to_owned(),
        Download::Done if pre.characters.is_none() => {
            return "Obtaining the character list...".to_owned()
        }
        Download::None | Download::Done => {}
    }
    match (pre.phase, pre.login) {
        (Phase::Connecting, Some(LoginProgress::Asking { attempt, of })) => {
            format!("Connecting... ({attempt} of {of})")
        }
        (Phase::Connecting, Some(LoginProgress::Waiting)) => "Waiting for the server...".to_owned(),
        (Phase::Connecting | Phase::Characters, _) => "Obtaining the character list...".to_owned(),
        (Phase::Disconnected, _) if !has_login => {
            "No account: start with --account <account>:<password>.".to_owned()
        }
        _ => "Connecting...".to_owned(),
    }
}

impl Screens {
    /// The screen up for the game as `pre` has it.
    #[must_use]
    pub fn screen(&self, pre: &Pregame) -> Screen {
        let screen = match pre.phase {
            Phase::Disconnected if pre.ended.is_some() => Screen::Disconnected,
            Phase::Disconnected | Phase::Connecting => Screen::Connecting,
            Phase::Characters if pre.characters.is_none() => Screen::Connecting,
            Phase::Characters if self.creation.is_some() => Screen::Creation,
            Phase::Characters if self.credits.is_some() => Screen::Credits,
            Phase::Characters if self.entering => Screen::Entering,
            Phase::Characters => Screen::Select,
            Phase::Entering | Phase::World => Screen::Entering,
        };
        // Portal space showing: the way in, or the way out of a log-off kept in sight.
        if pre.portal_space
            && matches!(
                screen,
                Screen::Entering | Screen::Select | Screen::Connecting
            )
        {
            Screen::Passage
        } else {
            screen
        }
    }

    /// The character selected, while the list holds one.
    #[must_use]
    pub fn selected<'a>(&self, pre: &'a Pregame) -> Option<&'a Character> {
        pre.characters.as_ref()?.get(self.selected)
    }

    /// Follow what the game said this frame: an entry refused (back at the list, saying why),
    /// a character made (into the world with it) or not (creation says why), the list's changes.
    pub fn follow(&mut self, pre: &Pregame, out: &mut PreOut) {
        if let Some(r) = &pre.refused {
            self.entering = false;
            if r.to_list {
                self.error = Some(r.line.clone());
            }
        }
        match &pre.made {
            Some(Made::Made(id)) if self.creation.is_some() => {
                self.creation = None;
                self.creation_window.reset();
                self.select_id = Some(*id);
                self.entering = true;
                out.acts.push(Action::PlayCharacter { character: *id });
            }
            Some(Made::Refused(line)) => self.creation_window.refused(line.clone()),
            _ => {}
        }
        if pre.phase != Phase::Characters {
            self.confirm = None;
            if pre.phase != Phase::Entering {
                self.entering = false;
            }
            if matches!(pre.phase, Phase::Disconnected | Phase::Connecting) {
                self.creation = None;
                self.credits = None;
                self.creation_window.reset();
            }
        }
        if let Some(list) = &pre.characters {
            if let Some(id) = self.select_id {
                if let Some(i) = list.iter().position(|c| c.id == id) {
                    self.selected = i;
                    self.select_id = None;
                }
            }
            if self.selected >= list.len() {
                self.selected = 0;
            }
        }
    }

    /// Back to how they open, as the player goes into the world.
    pub fn reset(&mut self) {
        let admin = self.creation_window.admin_offered;
        let version = std::mem::take(&mut self.version);
        *self = Self::default();
        self.creation_window.admin_offered = admin;
        self.version = version;
    }

    /// Whether `screen` stands over the view of the world ([`crate::vista`]) rather than on its
    /// own: character select and creation do.
    #[must_use]
    pub const fn over_the_world(screen: Screen) -> bool {
        matches!(screen, Screen::Select | Screen::Creation)
    }

    /// Whether the player is typing (creation's name).
    #[must_use]
    pub fn typing(&self) -> bool {
        self.creation.is_some() && self.creation_window.typing()
    }

    /// Draw a frame before the world; what the player asked.
    #[allow(clippy::too_many_arguments)]
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        pre: &Pregame,
        has_login: bool,
        credits: &[String],
        out: &mut PreOut,
    ) -> Screen {
        self.time = ctx.time;
        self.figure = None;
        self.confirm_buttons.clear();
        self.shown = SelectShown::default();
        let (sw, sh) = p.screen;
        let screen = self.screen(pre);
        // The backdrop: the other screens before the world stand on their own, but for portal space.
        if screen != Screen::Passage && !Self::over_the_world(screen) {
            p.fill(Rect::new(0.0, 0.0, sw, sh), 0xFF0C_0F16);
        }
        match screen {
            Screen::Connecting => self.connecting(p, ctx, pre, has_login, out),
            Screen::Select => self.select(p, ctx, pre, out),
            Screen::Creation => {
                let back = self
                    .creation
                    .as_mut()
                    .is_some_and(|c| self.creation_window.frame(p, ctx, c, out) == Some(false));
                if back {
                    self.creation = None;
                    self.creation_window.reset();
                }
            }
            Screen::Entering => self.message(p, ctx, "Entering the world..."),
            Screen::Passage => {}
            Screen::Credits => self.show_credits(p, ctx, credits),
            Screen::Disconnected => self.disconnected(p, ctx, pre, has_login, out),
        }
        // The version at the foot of the screens the player stops at, not over the credits or
        // the way into the world.
        if matches!(
            screen,
            Screen::Connecting | Screen::Select | Screen::Creation | Screen::Disconnected
        ) {
            let at = SelectLayout::new(p.screen, p.scale).version;
            self.version_line(p, at);
        }
        screen
    }

    /// The client's name on the darker band along the top of the screen: with Horizon's art, its
    /// mark (its picture with its name beside it) centered on the band; where the mark was drawn.
    fn title(p: &mut Painter<'_>, ctx: &Ctx<'_>) -> Option<Rect> {
        let k = p.scale;
        let (sw, _) = p.screen;
        p.fill(Rect::new(0.0, 0.0, sw, TOP_BAND * k), TOP_SHADE);
        if let Some(mark) = p.piece(BRAND_MARK) {
            return Some(kit::brand_mark(p, &mark, TOP_BAND * k));
        }
        let big = heading(ctx, 60.0).tracking(4.0);
        p.text(&big, 40.0 * k, 30.0 * k, &PRODUCT.to_uppercase());
        None
    }

    /// The world the client reached, on one line in `r`: its name, then a lamp and the word that
    /// say it is online.
    fn world_line(&mut self, p: &mut Painter<'_>, ctx: &Ctx<'_>, pre: &Pregame, r: Rect) {
        let k = p.scale;
        let head = TextStyle::new(Family::Heading, 34.0, 0xFFFF_FFFF)
            .edge(ctx.colours.ui(dereth_ui_kit::colours::row::MENU_GLOW));
        let word = TextStyle::new(Family::Numerals, 14.0, 0xFFE0_DCD0).edge(0xFF00_0000);
        let mut x = r.x;
        if !pre.world.is_empty() {
            p.text(&head, x, p.text_top(&head, r.y, r.h), &pre.world);
            x += p.measure(&head, &pre.world) + 16.0 * k;
        }
        // A lamp: the online color in a dark ring, a glint in its corner, then the word.
        let mid = r.y + r.h / 2.0;
        let lamp = Rect::new(x, mid - 7.0 * k, 14.0 * k, 14.0 * k);
        p.fill(lamp, 0xFF14_100C);
        p.fill(lamp.inset(2.0 * k), ONLINE);
        p.fill(
            Rect::new(lamp.x + 4.0 * k, lamp.y + 4.0 * k, 3.0 * k, 3.0 * k),
            0x80FF_FFFF,
        );
        p.text(&word, x + 22.0 * k, p.text_top(&word, r.y, r.h), "Online");
        self.shown.world = Some((pre.world.clone(), "Online", lamp));
    }

    /// The client's version, small and plain at the very foot, under the buttons.
    fn version_line(&mut self, p: &mut Painter<'_>, at: (f32, f32)) {
        if self.version.is_empty() {
            return;
        }
        let plain = TextStyle::new(Family::Body, 11.0, 0xFFB8_B4AA);
        p.text(&plain, at.0, at.1, &self.version);
        self.shown.version = Some((self.version.clone(), at));
    }

    /// A line in the middle of the screen.
    fn message(&self, p: &mut Painter<'_>, ctx: &Ctx<'_>, line: &str) {
        Self::title(p, ctx);
        let k = p.scale;
        let (sw, sh) = p.screen;
        p.text_in(
            &body(ctx, 18.0),
            Rect::new(0.0, sh / 2.0 - 12.0 * k, sw, 24.0 * k),
            Align::Centre,
            line,
        );
    }

    /// Connecting: the login's progress and the data download's, and Quit.
    fn connecting(
        &self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        pre: &Pregame,
        has_login: bool,
        out: &mut PreOut,
    ) {
        Self::title(p, ctx);
        let k = p.scale;
        let l = PanelLayout::new(p.screen, k);
        kit::panel(p, l.panel, 0.9);
        p.text(
            &heading(ctx, 22.0),
            l.panel.x + 24.0 * k,
            l.panel.y + 20.0 * k,
            "Connecting",
        );
        let line = connecting_line(pre, has_login);
        p.text(
            &body(ctx, 14.0),
            l.panel.x + 24.0 * k,
            l.panel.y + 70.0 * k,
            &line,
        );
        if let Some(share) = pre.download.share() {
            let bar = Rect::new(
                l.panel.x + 24.0 * k,
                l.panel.y + 104.0 * k,
                l.panel.w - 48.0 * k,
                14.0 * k,
            );
            kit::gauge(p, bar, share, 0xFF5A_A0E0);
        }
        if kit::button(p, ctx, l.quit, "Exit", true) {
            out.quit = true;
        }
    }

    /// Why the session ended, and the ways on: log in again, or Quit.
    fn disconnected(
        &self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        pre: &Pregame,
        has_login: bool,
        out: &mut PreOut,
    ) {
        Self::title(p, ctx);
        let k = p.scale;
        let l = PanelLayout::new(p.screen, k);
        kit::panel(p, l.panel, 0.9);
        p.text(
            &heading(ctx, 22.0),
            l.panel.x + 24.0 * k,
            l.panel.y + 20.0 * k,
            "Disconnected",
        );
        let style = body(ctx, 14.0);
        let why = pre.ended.as_deref().unwrap_or_default();
        let mut y = l.panel.y + 70.0 * k;
        for line in p.wrap(&style, why, l.panel.w - 48.0 * k) {
            p.text(&style, l.panel.x + 24.0 * k, y, &line);
            y += p.line_height(&style);
        }
        if has_login && kit::button(p, ctx, l.again, "Log In", true) {
            out.connect = true;
        }
        if kit::button(p, ctx, l.quit, "Exit", true) {
            out.quit = true;
        }
    }

    /// The credits: the game's credits scrolling up from the foot of the screen; Escape or a
    /// click ends them, and they end by themselves once the last line has gone by.
    fn show_credits(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, lines: &[String]) {
        let Some(since) = self.credits else { return };
        let k = p.scale;
        let (sw, sh) = p.screen;
        let style = body(ctx, 16.0);
        let lh = p.line_height(&style);
        #[allow(clippy::cast_possible_truncation)]
        let gone = ((ctx.time - since) as f32) * CREDITS_SPEED * k;
        let mut y = sh - gone;
        for line in lines {
            if y > -lh && y < sh {
                p.text_in(&style, Rect::new(0.0, y, sw, lh), Align::Centre, line);
            }
            y += lh;
        }
        let pressed = ctx.input.pressed[0] || ctx.input.pressed[1];
        if ctx.input.take_key(vk::ESCAPE) || pressed || y < 0.0 {
            self.credits = None;
        }
    }

    /// Character select.
    fn select(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, pre: &Pregame, out: &mut PreOut) {
        let k = p.scale;
        let l = SelectLayout::new(p.screen, k);
        let list = pre.characters.as_deref().unwrap_or_default();
        let ready = self.confirm.is_none();
        if ready && !list.is_empty() {
            let n = list.len();
            if ctx.input.take_key(vk::DOWN) {
                self.selected = (self.selected + 1) % n;
            }
            if ctx.input.take_key(vk::UP) {
                self.selected = (self.selected + n - 1) % n;
            }
            let c = &list[self.selected.min(n - 1)];
            if ctx.input.take_key(vk::ENTER) && c.deleting == 0 {
                self.confirm = Some(Confirm::Enter(c.id, c.name.clone()));
            }
            if ctx.input.take_key(vk::DELETE) {
                self.confirm = Some(Self::delete_or_restore(c));
            }
        }
        // The selected character stands over the world and under everything else, the bands
        // along the top and the foot included.
        if self.selected(pre).is_some_and(|c| c.deleting == 0) {
            self.figure = Some((l.figure, p.list.mark()));
        }
        self.shown.brand = Self::title(p, ctx);
        p.fill(l.foot, FOOT_SHADE);
        self.world_line(p, ctx, pre, l.world);
        // How many characters of how many, and the list's heading, on the world's line.
        let line = Rect::new(
            l.list.x,
            (COUNT_LINE - 14.0) * k,
            l.list.w - 8.0 * k,
            28.0 * k,
        );
        if pre.slots > 0 {
            let count = TextStyle::new(Family::Numerals, 22.0, 0xFFF0_E6CC).edge(0xFF00_0000);
            p.text_in(
                &count,
                line,
                Align::Right,
                &format!("{} / {}", list.len(), pre.slots),
            );
        }
        p.text_in(&heading(ctx, 22.0), line, Align::Left, "Characters");
        self.cards(p, ctx, pre, &l, ready);
        if let Some(e) = &self.error {
            let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_9A6A).edge(0xFF00_0000);
            let mut y = l.figure.bottom() - 40.0 * k;
            for line in p.wrap(&style, e, l.figure.w) {
                p.text_in(
                    &style,
                    Rect::new(l.figure.x, y, l.figure.w, 20.0 * k),
                    Align::Centre,
                    &line,
                );
                y += p.line_height(&style);
            }
        }
        let selected = self.selected(pre).cloned();
        let can_enter = ready && selected.as_ref().is_some_and(|c| c.deleting == 0);
        if kit::button(p, ctx, l.enter, "Log In", can_enter) {
            if let Some(c) = &selected {
                self.confirm = Some(Confirm::Enter(c.id, c.name.clone()));
            }
        }
        if kit::button(p, ctx, l.create, "Create", ready && pre.has_room()) {
            out.create = true;
            self.error = None;
        }
        let restoring = selected.as_ref().is_some_and(|c| c.deleting > 0);
        let label = if restoring { "Restore" } else { "Delete" };
        if kit::button(p, ctx, l.delete, label, ready && selected.is_some()) {
            if let Some(c) = &selected {
                self.confirm = Some(Self::delete_or_restore(c));
            }
        }
        if kit::button(p, ctx, l.credits, "Credits", ready) {
            self.credits = Some(ctx.time);
        }
        if kit::button(p, ctx, l.quit, "Exit", ready) {
            out.quit = true;
        }
        self.ask(p, ctx, out);
    }

    /// The question Delete or Restore asks about `c`.
    fn delete_or_restore(c: &Character) -> Confirm {
        if c.deleting > 0 {
            Confirm::Restore(c.id, c.name.clone())
        } else {
            Confirm::Delete(c.id, c.name.clone())
        }
    }

    /// The account's characters, a card each, scrolled with the wheel when they outrun the room.
    fn cards(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        pre: &Pregame,
        l: &SelectLayout,
        ready: bool,
    ) {
        let k = p.scale;
        let list = pre.characters.as_deref().unwrap_or_default();
        if list.is_empty() {
            p.text(&body(ctx, 14.0), l.list.x, l.list.y, "No characters yet.");
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let content = (CARD + GAP) * k * list.len() as f32 - GAP * k;
        let offset = kit::scroll(p, ctx, l.list, content, &mut self.list_scroll);
        let name = TextStyle::new(Family::Body, 18.0, 0xFFFF_FFFF).edge(0xFF00_0000);
        let info = TextStyle::new(Family::Body, 12.0, 0xFFFF_9A6A).edge(0xFF00_0000);
        p.list.push_clip(l.list);
        for (i, c) in list.iter().enumerate() {
            let r = l.card(i, offset, k);
            if r.bottom() < l.list.y || r.y > l.list.bottom() {
                continue;
            }
            // A card is a place for the pad's focus to rest.
            ctx.stop(&r);
            let selected = i == self.selected;
            let over = ready && ctx.input.hover(&r) && ctx.input.hover(&l.list);
            let text_x = if let Some((group, well)) = Self::card(p, r, selected, over) {
                self.shown.cards.push((r, Some(group)));
                self.shown.wells.push(well);
                well.right() + 12.0 * k
            } else {
                self.shown.cards.push((r, None));
                kit::panel(p, r, 0.8);
                kit::list_highlight(p, r, selected);
                r.x + 16.0 * k
            };
            if c.deleting > 0 {
                // Dimmed red, its countdown under its name.
                p.fill(r.inset(3.0 * k), 0x40B0_2018);
                p.text(&name, text_x, r.y + 8.0 * k, &c.name);
                let left = pre.seconds_left(c, ctx.time);
                p.text(
                    &info,
                    text_x,
                    r.y + 36.0 * k,
                    &format!("Deleting in {}", clock(left)),
                );
            } else {
                // One line on the card: the name, centered on it.
                p.text_in(
                    &name,
                    Rect::new(text_x, r.y, r.right() - text_x, r.h),
                    Align::Left,
                    &c.name,
                );
            }
            if ready && ctx.input.pressed[0] && ctx.input.hover(&r) && ctx.input.hover(&l.list) {
                if i == self.selected && c.deleting == 0 {
                    self.confirm = Some(Confirm::Enter(c.id, c.name.clone()));
                } else {
                    self.selected = i;
                }
            }
        }
        p.list.pop_clip();
    }

    /// A card of Horizon's art over `r`, lit while `selected` (or under the pointer, a little
    /// dimmed), with a dark well holding the client's picture; the pieces it was drawn with and
    /// the well, or none when the art has no card.
    fn card(
        p: &mut Painter<'_>,
        r: Rect,
        selected: bool,
        over: bool,
    ) -> Option<(&'static str, Rect)> {
        let group = if selected || over {
            CARD_SELECTED
        } else {
            CARD_NORMAL
        };
        let tint = if selected || !over {
            crate::draw::WHITE
        } else {
            0xC0FF_FFFF
        };
        if !p.halves(group, r, tint) {
            return None;
        }
        let well = p
            .art
            .piece_value("card.portrait")
            .filter(|v| v.len() == 4)
            .unwrap_or_else(|| PORTRAIT.to_vec());
        let height = p
            .art
            .piece(&format!("{CARD_NORMAL}.left"), 1.0)
            .map_or(136.0, |s| s.h);
        let s = r.h / height.max(1.0);
        let well = Rect::new(
            r.x + well[0] * s,
            r.y + well[1] * s,
            (well[2] - well[0]) * s,
            (well[3] - well[1]) * s,
        );
        p.fill(well, 0xFF19_1E28);
        // The client's own picture in the well, a little clear of its edges.
        if let Some(mark) = p.piece(BRAND_MARK) {
            let margin = WELL_MARGIN * p.scale;
            kit::brand_picture_in(p, &mark, well, margin);
        }
        Some((group, well))
    }

    /// The question asked, in a box over the middle; its answer.
    fn ask(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, out: &mut PreOut) {
        let Some(confirm) = self.confirm.clone() else {
            return;
        };
        let (title, text, yes) = match &confirm {
            Confirm::Enter(_, n) => ("Log In", format!("Log in with {n}?"), "Yes"),
            Confirm::Delete(_, n) => (
                "Delete Character",
                format!(
                    "Delete {n}? The server keeps a deleted character for a time before it is \
                     gone, and it can be restored until then."
                ),
                "Delete",
            ),
            Confirm::Restore(_, n) => ("Restore Character", format!("Restore {n}?"), "Restore"),
        };
        match self.question(p, ctx, title, &text, &[yes, "No"]) {
            Some(0) => {
                self.confirm = None;
                match confirm {
                    Confirm::Enter(character, _) => {
                        self.entering = true;
                        self.error = None;
                        out.acts.push(Action::PlayCharacter { character });
                    }
                    Confirm::Delete(character, _) => {
                        out.acts.push(Action::DeleteCharacter { character });
                    }
                    Confirm::Restore(character, name) => {
                        out.acts.push(Action::RestoreCharacter { character, name });
                    }
                }
            }
            Some(_) => self.confirm = None,
            None => {
                if ctx.input.take_key(vk::ESCAPE) {
                    self.confirm = None;
                }
            }
        }
    }

    /// A box over the middle with a title, its words and its buttons; the button pressed.
    fn question(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        title: &str,
        text: &str,
        buttons: &[&str],
    ) -> Option<usize> {
        let k = p.scale;
        let style = body(ctx, 14.0);
        let width = 460.0;
        let lines = p.wrap(&style, text, (width - 40.0) * k);
        #[allow(clippy::cast_precision_loss)]
        let words = p.line_height(&style) / k * lines.len().max(1) as f32;
        let size = kit::window_size_for(p, (width, words + 70.0));
        let (sw, sh) = p.screen;
        let at = (
            ((sw - size.0 * k) / 2.0).round(),
            ((sh - size.1 * k) / 2.0).round(),
        );
        let mut window = WindowState {
            open: true,
            pos: Some(at),
            opened_at: ctx.time - 1.0,
            no_close: true,
            ..WindowState::default()
        };
        let w = kit::window(p, ctx, &mut window, title, size, at);
        let mut y = w.body.y + 8.0 * k;
        for line in &lines {
            p.text_in(
                &style,
                Rect::new(w.body.x, y, w.body.w, 20.0 * k),
                Align::Centre,
                line,
            );
            y += p.line_height(&style);
        }
        let bw = 120.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let n = buttons.len() as f32;
        let x0 = w.body.x + w.body.w / 2.0 - (bw * n + 10.0 * k * (n - 1.0)) / 2.0;
        let by = w.body.bottom() - 44.0 * k;
        let mut pressed = None;
        for (i, label) in buttons.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(x0 + i as f32 * (bw + 10.0 * k), by, bw, 30.0 * k);
            self.confirm_buttons.push(r);
            if kit::button(p, ctx, r, label, true) {
                pressed = Some(i);
            }
        }
        pressed
    }
}
