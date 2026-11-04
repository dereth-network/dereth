//! The Admin window: an administrator's chat commands as buttons and boxes, on seven tabs, with the
//! server's replies under them. Only a character with the administrator flag has it; for anyone
//! else it is closed every frame, before anything is drawn.
//!
//! A button sends the line a person would type. A command that acts on the selection first has
//! the server appraise it afresh, since the server may otherwise act on an older target.

use std::collections::BTreeMap;

use dereth_client_contract::UiRequest;
use dereth_client_model::chat::text_type;
use dereth_client_model::cmd::admin::{
    admin_needs_selection, admin_uses_selection, AdminCommand, AdminGroup, ADMIN_COMMANDS,
};
use dereth_primitives::ObjectId;

use super::{WindowId, Windows};
use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::{ChatLine, GameState, Relation};
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The tabs, in order.
pub const TABS: [&str; 7] = [
    "Players", "Travel", "Target", "Myself", "Spawn", "World", "Commands",
];

/// The text box ids the window's fields take the keyboard under.
pub mod field {
    pub const NAME: u32 = 1000;
    pub const AMOUNT: u32 = 1001;
    pub const BOOT_REASON: u32 = 1002;
    pub const ACCOUNT: u32 = 1003;
    pub const DAYS: u32 = 1004;
    pub const HOURS: u32 = 1005;
    pub const MINUTES: u32 = 1006;
    pub const BAN_REASON: u32 = 1007;
    pub const NEW_NAME: u32 = 1008;
    pub const COORDS: u32 = 1010;
    pub const POI: u32 = 1011;
    pub const DUNGEON: u32 = 1012;
    pub const LOCATION: u32 = 1013;
    pub const PROPERTY: u32 = 1020;
    pub const PROPERTY_VALUE: u32 = 1021;
    pub const ADJUST_NAME: u32 = 1022;
    pub const ADJUST_BY: u32 = 1023;
    pub const FACTION_RANK: u32 = 1030;
    pub const LEVEL: u32 = 1031;
    pub const WEENIE: u32 = 1040;
    pub const SPAWN_AMOUNT: u32 = 1041;
    pub const PALETTE: u32 = 1042;
    pub const SHADE: u32 = 1043;
    pub const CUSTOM_NAME: u32 = 1044;
    pub const TIER: u32 = 1045;
    pub const LOOT_ITEMS: u32 = 1046;
    pub const MATERIAL: u32 = 1047;
    pub const STRUCTURE: u32 = 1048;
    pub const WORKMANSHIP: u32 = 1049;
    pub const MESSAGE: u32 = 1050;
    pub const SHUTDOWN_DELAY: u32 = 1051;
    pub const SHUTDOWN_MESSAGE: u32 = 1052;
    pub const ENVIRONMENT: u32 = 1053;
    pub const EVENT: u32 = 1054;
    pub const SERVER_PROPERTY: u32 = 1055;
    pub const SERVER_VALUE: u32 = 1056;
    pub const SEARCH: u32 = 1060;
    pub const ARGUMENTS: u32 = 1061;
    /// The ids above.
    pub const RANGE: std::ops::RangeInclusive<u32> = 1000..=1099;
}

/// The access levels, as the server names them.
const LEVELS: [&str; 6] = [
    "Player",
    "Advocate",
    "Sentinel",
    "Envoy",
    "Developer",
    "Admin",
];
/// The server property kinds: the name on the button, and the word in the command.
const PROPERTY_KINDS: [(&str, &str); 4] = [
    ("Yes/No", "bool"),
    ("Whole", "long"),
    ("Decimal", "double"),
    ("Text", "string"),
];
/// What the Adjust box changes: the name on the button, and the command.
const ADJUST_KINDS: [(&str, &str); 3] = [
    ("Vital", "modifyvital"),
    ("Skill", "modifyskill"),
    ("Attribute", "modifyattr"),
];

/// How long after a press a dangerous button waits for the second.
const ARMED_SECONDS: f64 = 4.0;
/// How long after a command with no reply the window says so.
const NO_REPLY_SECONDS: f64 = 5.0;
/// How long a line of chat after a command or its last reply is taken as its reply.
const REPLY_SECONDS: f64 = 3.0;
/// How many reply lines are kept.
const MAX_REPLIES: usize = 200;
/// A dangerous button's rule.
const DANGER_RULE: u32 = 0xFFC8_4A3C;

/// What was last sent, and what has come back.
#[derive(Debug, Default)]
struct Sent {
    line: String,
    at: f64,
    last_reply: f64,
    replies: Vec<String>,
}

/// The window's own state.
#[derive(Debug, Default)]
pub struct AdminWindow {
    /// Whether this character may have the window.
    pub allowed: bool,
    pub tab: usize,
    /// Each button that can send this frame: what it sends, and where it is.
    pub offered: Vec<(String, Rect)>,
    /// Where each heading and widget of the forms went this frame.
    pub laid_out: Vec<Rect>,
    fields: BTreeMap<u32, String>,
    sent: Option<Sent>,
    /// The line whose dangerous button was pressed once, until when.
    armed: Option<(String, f64)>,
    /// What the window itself says in the results, in place of a command.
    notice: Option<&'static str>,
    online: Vec<String>,
    online_asked: bool,
    online_coming: Option<Vec<String>>,
    /// `@loc` was asked for, to fill the location box.
    location_wanted: bool,
    /// The map's Teleport button was pressed: the next press on the map goes there.
    pub map_armed: bool,
    /// Where the map's Teleport button was drawn this frame, for an administrator.
    pub map_button: Option<Rect>,
    recent: Vec<String>,
    slot: u8,
    property_kind: usize,
    adjust_kind: usize,
    group: Option<AdminGroup>,
    chosen: Option<&'static AdminCommand>,
    online_scroll: f32,
    list_scroll: f32,
    results_scroll: f32,
}

impl AdminWindow {
    /// What field `id` holds.
    #[must_use]
    pub fn field(&self, id: u32) -> &str {
        self.fields.get(&id).map_or("", String::as_str)
    }

    pub fn set_field(&mut self, id: u32, text: &str) {
        self.fields.insert(id, text.to_owned());
    }

    /// The players `@listplayers` last named.
    #[must_use]
    pub fn online(&self) -> &[String] {
        &self.online
    }

    /// What the last command's replies have been.
    #[must_use]
    pub fn replies(&self) -> &[String] {
        self.sent.as_ref().map_or(&[], |s| s.replies.as_slice())
    }

    /// Send `lines`, the appraisal first when they act on `target`.
    fn send(&mut self, lines: &[String], target: Option<ObjectId>, now: f64, out: &mut Outcome) {
        if let Some(id) = target {
            out.requests.push(UiRequest::CancelAppraisal);
            out.requests.push(UiRequest::Appraise(id));
        }
        for text in lines {
            out.requests.push(UiRequest::ChatLineOnBehalf {
                text: text.clone(),
                window: 0,
            });
        }
        self.notice = None;
        self.results_scroll = 0.0;
        self.sent = Some(Sent {
            line: lines.join("  then  "),
            at: now,
            last_reply: now,
            replies: Vec::new(),
        });
    }

    /// Take this frame's chat: the replies to what was sent, and the two replies the window uses.
    fn hear(&mut self, lines: &[ChatLine], now: f64) {
        for l in lines {
            let reply = matches!(
                u32::from(l.chat_type),
                text_type::DEFAULT
                    | text_type::SYSTEM
                    | text_type::MAGIC
                    | text_type::HELP
                    | text_type::WORLD_BROADCAST
                    | text_type::LOCAL_ERROR
            );
            for line in l.text.lines() {
                let line = without_stamp(line);
                self.hear_online(line);
                if self.location_wanted {
                    if let Some(at) = line.find("Your location is: ") {
                        let location = line[at + "Your location is: ".len()..].trim();
                        self.set_field(field::LOCATION, location);
                        self.location_wanted = false;
                    }
                }
                if !reply {
                    continue;
                }
                if let Some(s) = &mut self.sent {
                    if now - s.last_reply <= REPLY_SECONDS && s.replies.len() < MAX_REPLIES {
                        s.replies.push(line.to_owned());
                        s.last_reply = now;
                    }
                }
            }
        }
    }

    /// One line of `@listplayers`: `name : account id`, each player, then the total.
    fn hear_online(&mut self, line: &str) {
        let Some(coming) = &mut self.online_coming else {
            return;
        };
        if line.contains("Total connected Players:") {
            self.online = self.online_coming.take().unwrap_or_default();
        } else if let Some((name, id)) = line.rsplit_once(" : ") {
            if !id.is_empty() && id.bytes().all(|b| b.is_ascii_digit()) {
                coming.push(name.trim().to_owned());
            }
        }
    }

    fn remember_spawn(&mut self, what: &str) {
        self.recent.retain(|r| r != what);
        self.recent.insert(0, what.to_owned());
        self.recent.truncate(5);
    }
}

/// The key page's rows, without the Admin window's key for anyone but an administrator.
pub fn offer_admin_key(keys: &mut crate::ui::game::KeyBindingsView, is_admin: bool) {
    if !is_admin {
        let admin = dereth_client_contract::actions::dereth::TOGGLE_ADMIN_WINDOW.0;
        keys.rows.retain(|r| r.action != admin);
    }
}

/// A chat line without the timestamp a player's options may put in front of it.
fn without_stamp(line: &str) -> &str {
    match line.strip_prefix('[').and_then(|l| l.split_once("] ")) {
        Some((_, rest)) => rest,
        None => line,
    }
}

/// A place on the map as `@tele` reads it: north or south first, as `37.3s,67.0w`.
fn tele_place(ns: f64, ew: f64) -> String {
    format!(
        "{:.1}{},{:.1}{}",
        ns.abs(),
        if ns >= 0.0 { 'n' } else { 's' },
        ew.abs(),
        if ew >= 0.0 { 'e' } else { 'w' }
    )
}

/// A value that must reach the server as one word: in double quotes when it has spaces. The
/// server's line has no way to hold a double quote, so none is kept.
fn one_word(value: &str) -> String {
    let value: Vec<&str> = value.split(' ').filter(|w| !w.is_empty()).collect();
    let value = value.join(" ").replace('"', "");
    if value.contains(' ') {
        format!("\"{value}\"")
    } else {
        value
    }
}

/// Optional words in order, as far as they go; `None` for a later one given without an earlier.
fn in_order(words: &[&str]) -> Option<String> {
    let given = words
        .iter()
        .rposition(|w| !w.is_empty())
        .map_or(0, |n| n + 1);
    if words[..given].iter().any(|w| w.is_empty()) {
        return None;
    }
    Some(words[..given].join(" "))
}

/// Whether `text` is a number no less than zero.
fn is_count(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit())
}

/// The line a form builds, when all it needs is given.
fn given(enough: bool, line: impl FnOnce() -> String) -> Option<String> {
    enough.then(line)
}

/// The window's drawing: everything a tab's rows need, and where the next one goes.
struct Form<'f, 'p, 'c> {
    p: &'f mut Painter<'p>,
    ctx: &'f mut Ctx<'c>,
    a: &'f mut AdminWindow,
    typing: &'f mut u32,
    out: &'f mut Outcome,
    target: Option<ObjectId>,
    left: f32,
    right: f32,
    x: f32,
    y: f32,
}

impl Form<'_, '_, '_> {
    fn dim(&self) -> TextStyle {
        TextStyle::new(Family::Body, 12.0, self.ctx.colours.dim()).edge(self.ctx.colours.edge())
    }

    fn text_style(&self) -> TextStyle {
        TextStyle::new(Family::Body, 13.0, self.ctx.colours.text()).edge(self.ctx.colours.edge())
    }

    /// Lay out from here on between `left` and `right`, from `y` down.
    fn area(&mut self, left: f32, right: f32, y: f32) {
        (self.left, self.right, self.x, self.y) = (left, right, left, y);
    }

    /// The next row, unless this one is still empty.
    fn row(&mut self) {
        if self.x > self.left {
            self.x = self.left;
            self.y += 30.0 * self.p.scale;
        }
    }

    /// A rectangle `w` wide (layout units) on this row, or the next if it does not fit.
    fn slot(&mut self, w: f32) -> Rect {
        let k = self.p.scale;
        let w = w * k;
        if self.x > self.left && self.x + w > self.right {
            self.row();
        }
        let r = Rect::new(self.x, self.y, w, 26.0 * k);
        self.x += w + 5.0 * k;
        self.a.laid_out.push(r);
        r
    }

    /// What is left of this row after `keep` (layout units), in layout units.
    fn rest(&self, keep: f32) -> f32 {
        ((self.right - self.x) / self.p.scale - keep).max(40.0)
    }

    fn heading(&mut self, text: &str) {
        self.row();
        let k = self.p.scale;
        let style = TextStyle::new(Family::Body, 13.0, self.ctx.colours.heading())
            .edge(self.ctx.colours.edge());
        let r = Rect::new(self.left, self.y, self.right - self.left, 18.0 * k);
        self.a.laid_out.push(r);
        let text = self.p.fit(&style, text, r.w);
        self.p.text_in(&style, r, Align::Left, &text);
        self.y += 21.0 * k;
    }

    fn caption(&mut self, text: &str, w: f32) {
        let r = self.slot(w);
        let style = self.dim();
        self.p.text_in(&style, r, Align::Left, text);
    }

    /// A box `w` wide, and what it holds with the spaces round it gone. Empty, it shows `hint`.
    fn field(&mut self, id: u32, hint: &str, w: f32) -> String {
        let r = self.slot(w);
        let text = self.a.fields.entry(id).or_default();
        kit::text_box_holding(self.p, self.ctx, r, text, self.typing, id, Align::Left, 200);
        let text = text.trim().to_owned();
        if text.is_empty() && *self.typing != id {
            let k = self.p.scale;
            let style = self.dim();
            self.p
                .text_in(&style, r.offset(8.0 * k, 0.0), Align::Left, hint);
        }
        text
    }

    /// [`Self::field`], holding `first` until the player changes it.
    fn field_from(&mut self, id: u32, first: &str, hint: &str, w: f32) -> String {
        self.a.fields.entry(id).or_insert_with(|| first.to_owned());
        self.field(id, hint, w)
    }

    /// A button for the window itself, sending nothing: `true` when pressed.
    fn local(&mut self, label: &str, enabled: bool) -> bool {
        let w = self.width(label);
        let r = self.slot(w);
        kit::button(self.p, self.ctx, r, label, enabled)
    }

    /// One of several buttons, the one chosen lit: `true` when pressed.
    fn choice(&mut self, label: &str, chosen: bool) -> bool {
        let w = self.width(label);
        let r = self.slot(w);
        kit::button_lit(self.p, self.ctx, r, label, true, chosen)
    }

    /// A button's width for `label`, in layout units.
    fn width(&self, label: &str) -> f32 {
        (kit::button_label_width(self.p, label) / self.p.scale + 18.0).max(42.0)
    }

    /// A button sending `lines`, when it has them. A dangerous one takes a second press.
    fn button(
        &mut self,
        label: &str,
        lines: Option<Vec<String>>,
        danger: bool,
        target: Option<ObjectId>,
    ) -> bool {
        const AGAIN: &str = "Press Again";
        let now = self.ctx.time;
        let key = lines.as_ref().map(|l| l.join("\n"));
        if self.a.armed.as_ref().is_some_and(|(_, until)| now > *until) {
            self.a.armed = None;
        }
        let armed = key.is_some() && self.a.armed.as_ref().map(|(k, _)| k) == key.as_ref();
        let w = if danger {
            self.width(label).max(self.width(AGAIN))
        } else {
            self.width(label)
        };
        let r = self.slot(w);
        if let Some(l) = &lines {
            self.a.offered.push((l.join("\n"), r));
        }
        let shown = if armed { AGAIN } else { label };
        let pressed = kit::button_lit(self.p, self.ctx, r, shown, lines.is_some(), armed);
        if danger && lines.is_some() {
            self.p.outline(r, self.p.scale, DANGER_RULE);
        }
        let (true, Some(lines)) = (pressed, lines) else {
            return false;
        };
        if danger && !armed {
            self.a.armed = key.map(|k| (k, now + ARMED_SECONDS));
            return false;
        }
        self.a.armed = None;
        self.a.send(&lines, target, now, self.out);
        true
    }

    /// A button sending `line`.
    fn act(&mut self, label: &str, line: Option<String>) -> bool {
        self.button(label, line.map(|l| vec![l]), false, None)
    }

    /// A button sending `line`, after a second press.
    fn risky(&mut self, label: &str, line: Option<String>) -> bool {
        self.button(label, line.map(|l| vec![l]), true, None)
    }

    /// A button sending `line`, about the selection, there being one.
    fn on_target(&mut self, label: &str, line: Option<String>, danger: bool) {
        let target = self.target;
        let line = line.filter(|_| target.is_some()).map(|l| vec![l]);
        self.button(label, line, danger, target);
    }
}

impl Windows {
    /// Every frame, open or not: the flag decides whether there is a window at all, and the
    /// replies are heard while it is closed too.
    pub(super) fn follow_admin(&mut self, ctx: &Ctx<'_>, state: &GameState) {
        if !state.is_admin {
            if self.admin.allowed || self.is_open(WindowId::Admin) {
                self.close(WindowId::Admin);
                self.admin = AdminWindow::default();
            }
            if field::RANGE.contains(&self.typing_field) {
                self.typing_field = 0;
            }
            return;
        }
        self.admin.allowed = true;
        if !self.is_open(WindowId::Map) {
            self.admin.map_armed = false;
        }
        if !self.is_open(WindowId::Admin) && field::RANGE.contains(&self.typing_field) {
            self.typing_field = 0;
        }
        self.admin.hear(&state.new_chat, ctx.time);
    }

    /// The map's Teleport button, for an administrator: once pressed, the next press on the
    /// picture sends them there. `at` is the map's body, its picture and the picture's scale.
    pub(super) fn map_teleport(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        at: (Rect, Rect, f32),
        out: &mut Outcome,
    ) {
        use dereth_ui_screens::mapradar::map::{map_coords_at, SHIPPED_NOTE_BINDING};
        const CANCEL: &str = "Cancel Teleport";
        let (body, area, fit) = at;
        let k = p.scale;
        self.admin.map_button = None;
        if !state.is_admin {
            return;
        }
        let armed = self.admin.map_armed;
        let w = kit::button_label_width(p, CANCEL) + 18.0 * k;
        let button = Rect::new(
            body.right() - w - 8.0 * k,
            area.bottom() + 4.0 * k,
            w,
            26.0 * k,
        );
        self.admin.map_button = Some(button);
        let label = if armed { CANCEL } else { "Teleport" };
        if kit::button_lit(p, ctx, button, label, true, armed) {
            self.admin.map_armed = !armed;
            return;
        }
        if !armed || fit <= 0.0 || !ctx.input.hover(&area) {
            return;
        }
        let (_, _, markers) = SHIPPED_NOTE_BINDING;
        let x = f64::from((ctx.input.mouse.0 - area.x) / fit);
        let y = f64::from((ctx.input.mouse.1 - area.y) / fit);
        let Some((ns, ew)) = map_coords_at(markers, x, y) else {
            return;
        };
        let place = tele_place(ns, ew);
        self.tip = Some(("Teleport Here".to_owned(), vec![place.clone()]));
        if ctx.over(&area) && ctx.input.clicked(&area) {
            self.admin.map_armed = false;
            let line = format!("@tele {place}");
            self.admin.send(&[line], None, ctx.time, out);
        }
    }

    /// The window: the tabs, the chosen tab's forms, and the results under them.
    pub(super) fn admin_window(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        body: Rect,
        out: &mut Outcome,
    ) {
        let k = p.scale;
        self.admin.offered.clear();
        self.admin.laid_out.clear();
        kit::tabs_filling(
            p,
            ctx,
            Rect::new(body.x, body.y, body.w, 26.0 * k),
            &TABS,
            &mut self.admin.tab,
        );
        let results = Rect::new(body.x, body.bottom() - 92.0 * k, body.w, 92.0 * k);
        let top = body.y + 34.0 * k;
        let mut f = Form {
            p,
            ctx,
            a: &mut self.admin,
            typing: &mut self.typing_field,
            out,
            target: state.target.as_ref().map(|t| t.id),
            left: body.x,
            right: body.right(),
            x: body.x,
            y: top,
        };
        match f.a.tab {
            0 => players(&mut f, state, top, results.y),
            1 => travel(&mut f, top),
            2 => target_tab(&mut f, state, top),
            3 => myself(&mut f, top),
            4 => spawn(&mut f, top),
            5 => world(&mut f, top),
            _ => all_commands(&mut f, top, results.y),
        }
        results_strip(&mut f, results);
    }
}

/// The columns of a tab: two sharing the width between `left` and `right`.
fn columns(f: &Form<'_, '_, '_>) -> (f32, f32, f32, f32) {
    let k = f.p.scale;
    let mid = (f.left + f.right) / 2.0;
    (f.left, mid - 8.0 * k, mid + 8.0 * k, f.right)
}

/// The players: who is on, what to do with the one named, and the moderation.
fn players(f: &mut Form<'_, '_, '_>, state: &GameState, top: f32, foot: f32) {
    let k = f.p.scale;
    let (l0, l1, r0, r1) = columns(f);
    let (all_left, all_right) = (f.left, f.right);
    if !f.a.online_asked {
        f.a.online_asked = true;
        f.a.online_coming = Some(Vec::new());
        let now = f.ctx.time;
        f.a.send(&["@listplayers".to_owned()], None, now, f.out);
    }
    f.area(l0, l1, top);
    f.heading("Online");
    let keep = f.width("Refresh") + 5.0;
    let name = {
        let w = f.rest(keep);
        f.field(field::NAME, "Search, or type a name", w)
    };
    if f.act("Refresh", Some("@listplayers".to_owned())) {
        f.a.online_coming = Some(Vec::new());
    }
    f.row();
    // The list keeps the rows the moderation leaves it.
    let list_h = (foot - f.y - 136.0 * k).max(44.0 * k);
    let list = Rect::new(l0, f.y, l1 - l0, list_h);
    let wanted = name.to_ascii_lowercase();
    let rows: Vec<String> =
        f.a.online
            .iter()
            .filter(|n| n.to_ascii_lowercase().contains(&wanted))
            .cloned()
            .collect();
    f.p.fill(list, 0x6010_0C08);
    #[allow(clippy::cast_precision_loss)]
    let content = rows.len() as f32 * 22.0 * k;
    let offset = kit::scroll(f.p, f.ctx, list, content, &mut f.a.online_scroll);
    f.p.list.push_clip(list);
    let style = f.text_style();
    let dim = f.dim();
    if rows.is_empty() {
        f.p.text_in(&dim, list, Align::Centre, "No one to show");
    }
    let mut pick = None;
    for (n, who) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let row = Rect::new(
            list.x,
            list.y + n as f32 * 22.0 * k - offset,
            list.w,
            22.0 * k,
        );
        if row.bottom() < list.y || row.y > list.bottom() {
            continue;
        }
        let on = *who == name;
        if on || f.ctx.input.hover(&row) {
            crate::ui::pregame::list_highlight(f.p, row, on);
        }
        f.p.text_in(&style, row.offset(8.0 * k, 0.0), Align::Left, who);
        if f.ctx.over(&row) && f.ctx.input.clicked(&row) {
            pick = Some(who.clone());
        }
    }
    f.p.list.pop_clip();
    if let Some(who) = pick {
        f.a.set_field(field::NAME, &who);
    }
    let name = f.a.field(field::NAME).trim().to_owned();
    let named = !name.is_empty();
    let with = |verb: &str| given(named, || format!("@{verb} {name}"));

    f.area(r0, r1, top);
    f.heading(&if named {
        format!("Player: {name}")
    } else {
        "Player: name one".to_owned()
    });
    let selected = state
        .target
        .as_ref()
        .filter(|t| t.relation == Relation::Player)
        .map(|t| t.name.clone());
    if f.local("Use Selected", selected.is_some()) {
        if let Some(who) = selected {
            f.a.set_field(field::NAME, &who);
        }
    }
    f.act("Who Is This", with("finger"));
    f.row();
    f.act("Go To", with("teleto"));
    f.act("Bring Here", with("teletome"));
    f.act("Send Back", with("telereturn"));
    f.row();
    // With a level, or a name ending in a digit would lose the digit to it.
    f.act("Buff", given(named, || format!("@buff {name} 8")));
    f.act("Fellowship Buff", with("fellowbuff"));
    f.row();
    let amount = f.field(field::AMOUNT, "Amount", 90.0);
    let some = named && is_count(&amount);
    f.act(
        "Give XP",
        given(some, || format!("@grantxp {name} {amount}")),
    );
    f.act(
        "Give Luminance",
        given(some, || format!("@grantluminance {name} {amount}")),
    );
    f.row();
    f.heading("Set Access");
    for level in LEVELS {
        f.risky(
            level,
            given(named, || {
                format!("@set-characteraccess {} {level}", one_word(&name))
            }),
        );
    }
    f.act("List Staff", Some("@watchmen".to_owned()));

    f.area(all_left, all_right, foot - 129.0 * k);
    f.heading("Moderation");
    f.act("Gag", with("gag"));
    f.act("Ungag", with("ungag"));
    f.risky("Force Logoff", with("forcelogout"));
    let reason = {
        let w = f.rest(f.width("Press Again") + 5.0);
        f.field(field::BOOT_REASON, "Reason for booting", w)
    };
    f.risky(
        "Boot",
        given(named, || {
            // The comma against the name is how the server finds where the name ends.
            if reason.is_empty() {
                format!("@boot char {name}")
            } else {
                format!("@boot char {name}, {reason}")
            }
        }),
    );
    f.row();
    let account = f.field(field::ACCOUNT, "Account", 110.0);
    let days = f.field(field::DAYS, "Days", 52.0);
    let hours = f.field(field::HOURS, "Hours", 52.0);
    let minutes = f.field(field::MINUTES, "Mins", 52.0);
    let ban_reason = {
        let w = f.rest(f.width("Press Again") + 5.0);
        f.field(field::BAN_REASON, "Reason for banning", w)
    };
    let span: Vec<f64> = [&days, &hours, &minutes]
        .iter()
        .map(|t| if t.is_empty() { Ok(0.0) } else { t.parse() })
        .collect::<Result<_, _>>()
        .unwrap_or_default();
    let long_enough = span.iter().all(|t| *t >= 0.0) && span.iter().sum::<f64>() > 0.0;
    f.risky(
        "Ban",
        given(!account.is_empty() && long_enough, || {
            let or_none = |t: &str| {
                if t.is_empty() {
                    "0".to_owned()
                } else {
                    t.to_owned()
                }
            };
            format!(
                "@ban {} {} {} {} {ban_reason}",
                one_word(&account),
                or_none(&days),
                or_none(&hours),
                or_none(&minutes),
            )
            .trim_end()
            .to_owned()
        }),
    );
    f.row();
    f.act("Ban List", Some("@banlist".to_owned()));
    f.act(
        "Unban",
        given(!account.is_empty(), || {
            format!("@unban {}", one_word(&account))
        }),
    );
    let new_name = {
        let w = f.rest(f.width("Press Again") + 5.0);
        f.field(field::NEW_NAME, "New name", w)
    };
    f.risky(
        "Rename",
        given(named && !new_name.is_empty(), || {
            format!("@rename {name}, {new_name}")
        }),
    );
}

/// Getting about: to a place by any of its names, and the recall slots.
fn travel(f: &mut Form<'_, '_, '_>, top: f32) {
    let (l0, l1, r0, r1) = columns(f);
    let (all_left, all_right) = (f.left, f.right);
    f.area(l0, l1, top);
    f.heading("Coordinates");
    let keep = f.width("Go") + 5.0;
    let w = f.rest(keep);
    let coords = f.field(field::COORDS, "37.3S, 67W", w);
    f.act(
        "Go",
        given(!coords.is_empty(), || format!("@tele {coords}")),
    );
    f.heading("Point of Interest");
    let keep = f.width("Go") + f.width("List") + 10.0;
    let w = f.rest(keep);
    let poi = f.field(field::POI, "A place's name", w);
    f.act("Go", given(!poi.is_empty(), || format!("@telepoi {poi}")));
    f.act("List", Some("@telepoi list".to_owned()));
    f.heading("Dungeon");
    let keep = f.width("Go") + 5.0;
    let w = f.rest(keep);
    let dungeon = f.field(field::DUNGEON, "Name or landblock", w);
    f.act(
        "Go",
        given(!dungeon.is_empty(), || format!("@teledungeon {dungeon}")),
    );

    f.area(r0, r1, top);
    f.heading("Exact Location");
    let keep = f.width("Go") + 5.0;
    let w = f.rest(keep);
    let location = f.field(field::LOCATION, "0x7F0401AD [12.3 -28.4 0.0]", w);
    f.act(
        "Go",
        given(!location.is_empty(), || format!("@teleloc {location}")),
    );
    f.row();
    if f.act("Fill From Where I Am", Some("@loc".to_owned())) {
        f.a.location_wanted = true;
    }
    f.heading("Everyone");
    f.risky("Bring All Players Here", Some("@teleallto".to_owned()));

    f.area(all_left, all_right, top + 220.0 * f.p.scale);
    f.heading("Recall");
    f.caption("Sanctuary", 84.0);
    f.act("Go", Some("@home".to_owned()));
    f.act("Set Here", Some("@save".to_owned()));
    f.row();
    f.caption("Slot", 84.0);
    for n in 1..=9u8 {
        if f.choice(&n.to_string(), f.a.slot == n) {
            f.a.slot = n;
        }
    }
    let slot = f.a.slot;
    let chosen = slot != 0;
    f.act("Go", given(chosen, || format!("@home {slot}")));
    f.act("Set Here", given(chosen, || format!("@save {slot}")));
}

/// The selection: what it is, and what can be asked of it.
fn target_tab(f: &mut Form<'_, '_, '_>, state: &GameState, top: f32) {
    let k = f.p.scale;
    let (l0, l1, r0, r1) = columns(f);
    let (all_left, all_right) = (f.left, f.right);
    let style = f.text_style();
    let dim = f.dim();
    let card = Rect::new(all_left, top, all_right - all_left, 28.0 * k);
    f.p.fill(card, 0x6010_0C08);
    match &state.target {
        Some(t) => {
            f.p.text_in(&style, card.offset(8.0 * k, 0.0), Align::Left, &t.name);
            let level = t.level.map_or(String::new(), |l| format!("Level {l}   "));
            let about = format!("{level}{:#010X}", t.id.0);
            let at = Rect::new(card.x, card.y, card.w - 8.0 * k, card.h);
            f.p.text_in(&dim, at, Align::Right, &about);
        }
        None => {
            f.p.text_in(&dim, card, Align::Centre, "Nothing is selected");
        }
    }
    let below = top + 34.0 * k;
    f.area(l0, l1, below);
    f.heading("Inspect");
    for (label, verb) in [
        ("Properties", "propertydump"),
        ("Info", "getinfo"),
        ("Stats", "showstats"),
        ("Location", "targetloc"),
        ("Distance", "dist"),
        ("Enchantments", "getenchantments"),
        ("Resistances", "resist-info"),
        ("Generator", "generatordump"),
        ("Vendor", "vendordump"),
    ] {
        f.on_target(label, Some(format!("@{verb}")), false);
    }
    f.area(r0, r1, below);
    f.heading("Act On It");
    for (label, verb) in [
        ("Heal", "heal"),
        ("Turn To Me", "turnto"),
        ("Move To Me", "movetome"),
        ("Crack", "crack"),
        ("Regen", "regen"),
    ] {
        f.on_target(label, Some(format!("@{verb}")), false);
    }
    f.on_target("Smite", Some("@smite".to_owned()), true);
    f.on_target("Delete", Some("@delete".to_owned()), true);

    let lower = below + 150.0 * k;
    f.area(l0, l1, lower);
    f.heading("Property");
    let keep = f.width("Read") + 5.0;
    let w = f.rest(keep);
    let property = f.field(field::PROPERTY, "PropertyInt.Name", w);
    f.on_target(
        "Read",
        given(!property.is_empty(), || format!("@getproperty {property}")),
        false,
    );
    f.row();
    let keep = f.width("Press Again") + 5.0;
    let w = f.rest(keep);
    let value = f.field(field::PROPERTY_VALUE, "Value", w);
    f.on_target(
        "Write",
        given(!property.is_empty() && !value.is_empty(), || {
            format!("@setproperty {property} {}", one_word(&value))
        }),
        true,
    );
    f.area(r0, r1, lower);
    f.heading("Adjust");
    for (n, (label, _)) in ADJUST_KINDS.iter().enumerate() {
        if f.choice(label, f.a.adjust_kind == n) {
            f.a.adjust_kind = n;
        }
    }
    f.row();
    let name = f.field(field::ADJUST_NAME, "Exact name", 150.0);
    let by = f.field(field::ADJUST_BY, "Change", 74.0);
    let verb = ADJUST_KINDS[f.a.adjust_kind.min(2)].1;
    let whole = by.parse::<i64>().is_ok() && !name.contains(' ') && !name.is_empty();
    f.on_target(
        "Apply",
        given(whole, || format!("@{verb} {name} {by}")),
        true,
    );
}

/// The administrator's own powers. Most are told on and off apart: the client cannot see which
/// is in force.
fn myself(f: &mut Form<'_, '_, '_>, top: f32) {
    let (l0, l1, r0, r1) = columns(f);
    f.area(l0, l1, top);
    f.heading("Powers");
    let line = |verb: &str, word: &str| Some(format!("@{verb} {word}"));
    for (caption, verb, words) in [
        (
            "Cloak",
            "cloak",
            &[
                ("On", "on"),
                ("Off", "off"),
                ("Player", "player"),
                ("Creature", "creature"),
            ][..],
        ),
        ("Never Die", "neversaydie", &[("On", "on"), ("Off", "off")]),
        ("Attackable", "attackable", &[("On", "on"), ("Off", "off")]),
        ("Vision", "adminvision", &[("On", "on"), ("Off", "off")]),
        ("Run Boost", "run", &[("On", "on"), ("Off", "off")]),
    ] {
        f.caption(caption, 84.0);
        for (label, word) in words {
            f.act(label, line(verb, word));
        }
        f.row();
    }
    f.caption("Bypass", 84.0);
    f.act("Portals", Some("@portal_bypass".to_owned()));
    f.act("Housing", Some("@mrt".to_owned()));
    f.area(r0, r1, top);
    f.heading("Character");
    f.act("God", Some("@god".to_owned()));
    f.act("Ungod", Some("@ungod".to_owned()));
    f.act("Buff Me", Some("@buff".to_owned()));
    // With something selected, the server heals that instead.
    let alone = f.target.is_none();
    f.act("Heal Me", given(alone, || "@heal".to_owned()));
    f.act("Dispel", Some("@dispel".to_owned()));
    f.row();
    f.caption("PK State", 64.0);
    for (label, word) in [
        ("None", "npk"),
        ("PK", "pk"),
        ("PKL", "pkl"),
        ("Free", "free"),
    ] {
        f.act(label, line("pk", word));
    }
    f.row();
    f.caption("Faction", 64.0);
    let rank = f.field(field::FACTION_RANK, "Rank", 52.0);
    for (label, word) in [("None", "none"), ("CH", "ch"), ("EW", "ew"), ("RB", "rb")] {
        let line = if rank.is_empty() || word == "none" {
            format!("@faction {word}")
        } else {
            format!("@faction {word} {rank}")
        };
        f.act(label, Some(line));
    }
    f.row();
    f.act("All Spells", Some("@addallspells".to_owned()));
    f.act("All Titles", Some("@addalltitles".to_owned()));
    f.row();
    let level = f.field(field::LEVEL, "Level", 70.0);
    f.risky(
        "Delevel",
        given(is_count(&level), || format!("@delevel {level}")),
    );
}

/// Making things: by name in the world or the pack, loot, salvage and the test kits.
fn spawn(f: &mut Form<'_, '_, '_>, top: f32) {
    let (l0, l1, r0, r1) = columns(f);
    let (all_left, all_right) = (f.left, f.right);
    f.area(all_left, all_right, top);
    f.heading("Object");
    let w = f.rest(3.0 * 81.0);
    let weenie = f.field(field::WEENIE, "Weenie class name or id", w);
    let amount = f.field(field::SPAWN_AMOUNT, "Amount", 76.0);
    let palette = f.field(field::PALETTE, "Palette", 76.0);
    let shade = f.field(field::SHADE, "Shade", 76.0);
    f.row();
    let details = in_order(&[&amount, &palette, &shade]);
    let line = |verb: &str| {
        details
            .as_ref()
            .filter(|_| !weenie.is_empty())
            .map(|d| format!("@{verb} {weenie} {d}").trim_end().to_owned())
    };
    let in_world = f.act("In The World", line("create"));
    let in_pack = f.act("In My Pack", line("ci"));
    let keep = f.width("Spawn Named") + 5.0;
    let w = f.rest(keep);
    let custom = f.field(field::CUSTOM_NAME, "Custom name", w);
    let count = if amount.is_empty() { "1" } else { &amount };
    let named = f.act(
        "Spawn Named",
        given(!weenie.is_empty() && !custom.is_empty(), || {
            format!("@createnamed {weenie} {count} {custom}")
        }),
    );
    if in_world || in_pack || named {
        f.a.remember_spawn(&weenie);
    }
    f.row();
    f.caption("Recent", 54.0);
    let recent = f.a.recent.clone();
    for r in &recent {
        if f.local(r, true) {
            f.a.set_field(field::WEENIE, r);
        }
    }
    f.row();
    let lower = f.y;
    f.area(l0, l1, lower);
    f.heading("Loot");
    let tier = f.field(field::TIER, "Tier", 52.0);
    let items = f.field(field::LOOT_ITEMS, "Items", 52.0);
    f.act(
        "Roll Into Pack",
        given(is_count(&tier), || {
            format!("@ciloot {tier} {items}").trim_end().to_owned()
        }),
    );
    f.heading("Salvage");
    let material = f.field(field::MATERIAL, "Material", 90.0);
    let structure = f.field(field::STRUCTURE, "Size", 52.0);
    let work = f.field(field::WORKMANSHIP, "Work", 52.0);
    let bag = in_order(&[&structure, &work]);
    f.act(
        "Make Bag",
        bag.filter(|_| !material.is_empty())
            .map(|b| format!("@cm {material} {b}").trim_end().to_owned()),
    );
    f.area(r0, r1, lower);
    f.heading("Test Kits");
    for (label, verb) in [
        ("Weapons", "weapons"),
        ("Sample Items", "inv"),
        ("Stackables", "splits"),
        ("Components", "comps"),
        ("Food", "food"),
        ("Currency", "currency"),
    ] {
        f.act(label, Some(format!("@{verb}")));
    }
}

/// The world: telling everyone, the server's state, and what it holds.
fn world(f: &mut Form<'_, '_, '_>, top: f32) {
    let k = f.p.scale;
    let (l0, l1, r0, r1) = columns(f);
    let (all_left, all_right) = (f.left, f.right);
    f.area(all_left, all_right, top);
    f.heading("Message Everyone");
    let keep = f.width("Send") + 5.0;
    let w = f.rest(keep);
    let message = f.field(field::MESSAGE, "What everyone playing reads", w);
    if f.act(
        "Send",
        given(!message.is_empty(), || format!("@gamecast {message}")),
    ) {
        f.a.set_field(field::MESSAGE, "");
    }
    f.row();
    let upper = f.y;
    f.area(l0, l1, upper);
    f.heading("Server");
    f.act("Save World", Some("@save-world".to_owned()));
    f.risky("Open", Some("@world open".to_owned()));
    f.risky("Close", Some("@world close".to_owned()));
    f.risky("Close And Boot", Some("@world close boot".to_owned()));
    f.row();
    let delay = f.field_from(field::SHUTDOWN_DELAY, "300", "Seconds", 60.0);
    let w = f.rest(0.0);
    let why = f.field(field::SHUTDOWN_MESSAGE, "Shutdown message", w);
    f.row();
    // The delay is sent each time: the server keeps whatever it was last told.
    let valid = is_count(&delay) && delay.len() <= 5;
    f.button(
        "Shut Down",
        valid.then(|| {
            vec![
                format!("@set-shutdown-interval {delay}"),
                format!("@shutdown {why}").trim_end().to_owned(),
            ]
        }),
        true,
        None,
    );
    f.act("Cancel Shutdown", Some("@cancel-shutdown".to_owned()));
    f.area(r0, r1, upper);
    f.heading("Status");
    for (label, verb) in [
        ("Server", "serverstatus"),
        ("Performance", "serverperformance"),
        ("Landblocks", "landblockperformance"),
        ("Network", "netstats"),
        ("Game Time", "time"),
    ] {
        f.act(label, Some(format!("@{verb}")));
    }
    f.row();
    f.risky(
        "Reload This Landblock",
        Some("@reload-landblock".to_owned()),
    );

    let middle = upper + 140.0 * k;
    f.area(l0, l1, middle);
    f.heading("Environment");
    let keep = f.width("List") + 5.0;
    let w = f.rest(keep);
    let effect = f.field(field::ENVIRONMENT, "Effect", w);
    f.act("List", Some("@setlbenviron list".to_owned()));
    f.row();
    f.act(
        "Here",
        given(!effect.is_empty(), || format!("@setlbenviron {effect}")),
    );
    f.act("Reset Here", Some("@setlbenviron".to_owned()));
    f.risky(
        "Everywhere",
        given(!effect.is_empty(), || format!("@setglobalenviron {effect}")),
    );
    f.risky("Reset All", Some("@setglobalenviron".to_owned()));
    f.area(r0, r1, middle);
    f.heading("Event");
    let w = f.rest(0.0);
    let event = f.field(field::EVENT, "Event name", w);
    f.row();
    for (label, word) in [("Status", "status"), ("Start", "start"), ("Stop", "stop")] {
        f.act(
            label,
            given(!event.is_empty(), || {
                format!("@event {word} {}", one_word(&event))
            }),
        );
    }

    f.area(all_left, all_right, middle + 116.0 * k);
    f.heading("Server Properties");
    for (n, (label, _)) in PROPERTY_KINDS.iter().enumerate() {
        if f.choice(label, f.a.property_kind == n) {
            f.a.property_kind = n;
        }
    }
    f.act("List All", Some("@showprops".to_owned()));
    f.row();
    let kind = PROPERTY_KINDS[f.a.property_kind.min(3)].1;
    let property = f.field(field::SERVER_PROPERTY, "Property", 230.0);
    f.act(
        "Read",
        given(!property.is_empty(), || format!("@fetch{kind} {property}")),
    );
    let keep = f.width("Press Again") + 5.0;
    let w = f.rest(keep);
    let value = f.field(field::SERVER_VALUE, "Value", w);
    f.risky(
        "Write",
        given(!property.is_empty() && !value.is_empty(), || {
            format!("@modify{kind} {property} {}", one_word(&value))
        }),
    );
}

/// Every command: found by search or group, and run with whatever it is given.
fn all_commands(f: &mut Form<'_, '_, '_>, top: f32, foot: f32) {
    let k = f.p.scale;
    f.area(f.left, f.right, top);
    let label = format!("Group: {}", f.a.group.map_or("All", AdminGroup::name));
    let keep = f.width("Group: Properties") + 5.0;
    let w = f.rest(keep);
    let search = f.field(field::SEARCH, "Search the commands", w);
    let wide = f.width("Group: Properties");
    let r = f.slot(wide);
    if kit::button(f.p, f.ctx, r, &label, true) {
        // On to the next group, and from the last to all of them.
        let at =
            f.a.group
                .and_then(|g| AdminGroup::ALL.iter().position(|o| *o == g));
        f.a.group = match at {
            None => Some(AdminGroup::ALL[0]),
            Some(n) => AdminGroup::ALL.get(n + 1).copied(),
        };
        f.a.list_scroll = 0.0;
    }
    f.row();
    let wanted = search.to_ascii_lowercase();
    let group = f.a.group;
    let rows: Vec<&'static AdminCommand> = ADMIN_COMMANDS
        .iter()
        .filter(|c| group.is_none_or(|g| c.group == g))
        .filter(|c| {
            c.verb.to_ascii_lowercase().contains(&wanted)
                || c.about.to_ascii_lowercase().contains(&wanted)
        })
        .collect();
    let list = Rect::new(f.left, f.y, f.right - f.left, foot - f.y - 70.0 * k);
    f.p.fill(list, 0x6010_0C08);
    #[allow(clippy::cast_precision_loss)]
    let content = rows.len() as f32 * 22.0 * k;
    let offset = kit::scroll(f.p, f.ctx, list, content, &mut f.a.list_scroll);
    let style = f.text_style();
    let dim = f.dim();
    f.p.list.push_clip(list);
    if rows.is_empty() {
        f.p.text_in(&dim, list, Align::Centre, "No command matches");
    }
    let about_x = list.x + 190.0 * k;
    for (n, c) in rows.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let row = Rect::new(
            list.x,
            list.y + n as f32 * 22.0 * k - offset,
            list.w,
            22.0 * k,
        );
        if row.bottom() < list.y || row.y > list.bottom() {
            continue;
        }
        let on = f.a.chosen.is_some_and(|o| o.verb == c.verb);
        if on || f.ctx.input.hover(&row) {
            crate::ui::pregame::list_highlight(f.p, row, on);
        }
        let verb = format!("@{}", c.verb);
        f.p.text_in(&style, row.offset(8.0 * k, 0.0), Align::Left, &verb);
        let about_w = row.right() - about_x - 24.0 * k;
        let about = f.p.fit(&dim, c.about, about_w);
        let at = Rect::new(about_x, row.y, about_w, row.h);
        f.p.text_in(&dim, at, Align::Left, &about);
        if f.ctx.over(&row) && f.ctx.input.clicked(&row) {
            f.a.chosen = Some(*c);
            f.a.set_field(field::ARGUMENTS, "");
        }
    }
    f.p.list.pop_clip();
    f.y = list.bottom() + 6.0 * k;
    f.x = f.left;
    let Some(c) = f.a.chosen else {
        f.heading("Choose a command to run it");
        return;
    };
    let usage = format!("@{} {}", c.verb, c.usage);
    f.heading(usage.trim_end());
    let keep = f.width("Press Again") + 5.0;
    let w = f.rest(keep);
    let empty_hint = if c.usage.is_empty() {
        "Takes nothing"
    } else {
        "Arguments"
    };
    let args = f.field(field::ARGUMENTS, empty_hint, w);
    let line = format!("@{} {args}", c.verb).trim_end().to_owned();
    let target = if admin_uses_selection(c.verb) {
        f.target
    } else {
        None
    };
    let without = target.is_none() && args.is_empty() && admin_needs_selection(c.verb);
    if without {
        // The server would act on whatever it last took for the target.
        if f.local("Run", true) {
            f.a.sent = None;
            f.a.notice = Some("Select something first: this command acts on the selection.");
        }
    } else {
        f.button("Run", Some(vec![line]), c.danger, target);
    }
}

/// Under every tab: what was last sent, and what came back.
fn results_strip(f: &mut Form<'_, '_, '_>, r: Rect) {
    let k = f.p.scale;
    let style = f.text_style();
    let dim = f.dim();
    f.p.fill(r, 0x9010_0C08);
    let inner = Rect::new(r.x + 8.0 * k, r.y + 4.0 * k, r.w - 16.0 * k, 18.0 * k);
    let now = f.ctx.time;
    if let Some(text) = f.a.notice {
        f.p.text_in(&style, inner, Align::Left, text);
        return;
    }
    let Some(sent) = &f.a.sent else {
        f.p.text_in(
            &dim,
            inner,
            Align::Left,
            "Replies from the server show here",
        );
        return;
    };
    let line = f.p.fit(&style, &sent.line, inner.w);
    f.p.text_in(&style, inner, Align::Left, &line);
    let area = Rect::new(
        r.x,
        inner.bottom() + 2.0 * k,
        r.w,
        r.bottom() - inner.bottom() - 4.0 * k,
    );
    if sent.replies.is_empty() {
        let waited = now - sent.at > NO_REPLY_SECONDS;
        let text = if waited {
            "No reply from the server"
        } else {
            "..."
        };
        f.p.text_in(&dim, inner.offset(0.0, 20.0 * k), Align::Left, text);
        return;
    }
    let replies = sent.replies.clone();
    #[allow(clippy::cast_precision_loss)]
    let content = replies.len() as f32 * 17.0 * k;
    let offset = kit::scroll(f.p, f.ctx, area, content, &mut f.a.results_scroll);
    f.p.list.push_clip(area);
    for (n, reply) in replies.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let row = Rect::new(
            inner.x,
            area.y + n as f32 * 17.0 * k - offset,
            inner.w - 16.0 * k,
            17.0 * k,
        );
        if row.bottom() < area.y || row.y > area.bottom() {
            continue;
        }
        let text = f.p.fit(&dim, reply, row.w);
        f.p.text_in(&dim, row, Align::Left, &text);
    }
    f.p.list.pop_clip();
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the lines this client's own window builds)
    use super::*;

    #[test]
    fn a_value_of_several_words_is_quoted_so_it_reaches_the_server_as_one() {
        assert_eq!(one_word("Foo"), "Foo");
        assert_eq!(one_word("Foo  Bar"), "\"Foo Bar\"");
        assert_eq!(one_word("say \"hi\" now"), "\"say hi now\"");
    }

    #[test]
    fn optional_words_need_every_one_before_them() {
        assert_eq!(in_order(&["", "", ""]).as_deref(), Some(""));
        assert_eq!(in_order(&["3", "", ""]).as_deref(), Some("3"));
        assert_eq!(in_order(&["3", "4", ""]).as_deref(), Some("3 4"));
        assert_eq!(in_order(&["", "4", ""]), None);
    }

    #[test]
    fn a_place_on_the_map_is_worded_as_the_teleport_command_reads_it() {
        assert_eq!(tele_place(37.34, -67.0), "37.3n,67.0w");
        assert_eq!(tele_place(-0.5, 12.25), "0.5s,12.2e");
    }

    #[test]
    fn a_timestamp_is_taken_off_a_line_but_a_location_keeps_its_brackets() {
        assert_eq!(without_stamp("[12:30] Bob : 5"), "Bob : 5");
        assert_eq!(without_stamp("0x1 [1 2 3] 1 0 0 0"), "0x1 [1 2 3] 1 0 0 0");
    }
}
