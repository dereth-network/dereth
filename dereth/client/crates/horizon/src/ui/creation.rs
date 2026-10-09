//! Character creation: one window stepping through the choices a new character is made of, over
//! the shared creation model ([`dereth_chargen`]). The heritage, the sex, the face and hair, the
//! clothes, the profession, the attributes, the skills, the town and the name are the model's;
//! every rule (the budgets, the costs, what a heritage may choose, the name's length) is the
//! model's too, and the server checks them again. What is here is only how they are shown.
//!
//! The finished character goes to the server as the other interfaces send it
//! ([`UiRequest::CharGenAction`]); its answer comes back as the game's verification notice, and an
//! accepted character goes straight into the world once the character list names it.

use std::rc::Rc;

use dereth_chargen::{Attr, CharGenState, CreationEntry, CreationPolicy, CreationTables};
use dereth_client_contract::pregame::CharGenAction;
use dereth_client_contract::UiRequest;
use dereth_rules::chargen::SkillAdvancementClass;

use crate::art::Family;
use crate::draw::Rect;
use crate::ui::game::GameState;
use crate::ui::kit::{self, Ctx};
use crate::ui::paint::{Align, Painter, TextStyle};
use crate::ui::Outcome;

/// The way the model first faces, in degrees: three-quarters on.
pub const HEADING: f32 = 150.0;
/// How far the model turns for each point the pointer is dragged across it, in degrees.
const TURN_DEGREES_PER_POINT: f32 = 0.6;

/// The string table the creation wizard's refusals are words of.
pub const CREATION_STRING_TABLE: u32 = 0x1000_0002;

/// The steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    Heritage,
    Sex,
    Appearance,
    Attire,
    Profession,
    Attributes,
    Skills,
    Town,
    Name,
}

impl Step {
    const ALL: [Self; 9] = [
        Self::Heritage,
        Self::Sex,
        Self::Appearance,
        Self::Attire,
        Self::Profession,
        Self::Attributes,
        Self::Skills,
        Self::Town,
        Self::Name,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Heritage => "Heritage",
            Self::Sex => "Sex",
            Self::Appearance => "Appearance",
            Self::Attire => "Attire",
            Self::Profession => "Profession",
            Self::Attributes => "Attributes",
            Self::Skills => "Skills",
            Self::Town => "Town",
            Self::Name => "Name",
        }
    }

    /// Whether the step is shown for `heritage`: the Olthoi skip the face, the clothes and the
    /// profession, as the game's wizard does.
    fn shown_for(self, heritage: u32) -> bool {
        let olthoi = heritage == dereth_chargen::HERITAGE_OLTHOI
            || heritage == dereth_chargen::HERITAGE_OLTHOI_ACID;
        !(olthoi && matches!(self, Self::Appearance | Self::Attire | Self::Profession))
    }
}

/// The attributes in the order the pages list them.
const ATTRIBUTES: [(Attr, &str); 6] = [
    (Attr::Strength, "Strength"),
    (Attr::Endurance, "Endurance"),
    (Attr::Coordination, "Coordination"),
    (Attr::Quickness, "Quickness"),
    (Attr::Focus, "Focus"),
    (Attr::Self_, "Self"),
];

/// A creation in progress.
#[derive(Debug)]
pub struct Creation {
    pub state: CharGenState,
    tables: Rc<CreationTables>,
    texts: Rc<dereth_chargen::CreationTexts>,
    pub step: Step,
    name: String,
    name_focus: u32,
    /// Waiting for the server: the character has gone and its answer has not come, or it was
    /// accepted and the character list does not name it yet.
    pub waiting: bool,
    /// The name sent, while its answer is awaited.
    sent_name: Option<String>,
    /// The verification notices seen, so each answer is read once.
    responses_seen: u32,
    /// The server accepted the character.
    accepted: bool,
    /// What the window says under its steps: a refusal, or what it is waiting for.
    pub status: Option<String>,
    skills_scroll: f32,
    /// Where the model is shown this frame.
    pub preview: Option<Rect>,
    /// The way the model faces, in degrees, turned by dragging it.
    pub heading: f32,
    /// Where the pointer was last frame while the model is being turned.
    turning: Option<f32>,
    /// Asked to spend the attribute credits left before creating.
    confirm_unspent: bool,
    /// Whether the account may make what the third expansion added, for the Random buttons.
    tod: bool,
    /// The dice have been seeded from the client's start-up, as the other interfaces seed them.
    seeded: bool,
    /// The clothes have been come to: from then on the model wears them.
    attire_seen: bool,
}

/// A heritage's name as the game shows it to players: the table's own name, but for the two
/// it names by their kind (`Gear`, `OlthoiAcid`).
fn heritage_title(key: u32, name: &str) -> String {
    match key {
        6 => "Gear Knight".to_owned(),
        13 => "Olthoi Spitter".to_owned(),
        _ => name.to_owned(),
    }
}

/// What is left of a budget, as the creation pages say it.
fn remaining(left: i32, total: i32) -> String {
    format!("Remaining: {left} / {total}")
}

/// The styles a piece of clothing offers: a hat may be left off (the game's style -1), and the
/// shirt, trousers and footwear are always worn, as the server requires.
fn style_names(part: usize, items: &[dereth_assets::tables::GearItem]) -> Vec<&str> {
    (part == 0)
        .then_some("None")
        .into_iter()
        .chain(items.iter().map(|g| g.name.as_str()))
        .collect()
}

/// The row of [`style_names`] that style `style` of `part` is shown on.
fn style_row(part: usize, style: i32) -> usize {
    usize::try_from(style + i32::from(part == 0)).unwrap_or(0)
}

/// The style the row `row` of [`style_names`] chooses.
fn row_style(part: usize, row: usize) -> i32 {
    i32::try_from(row).unwrap_or(0) - i32::from(part == 0)
}

/// Where a skill of `level` stands in the skills list: specialized, then trained, then the rest.
/// A step cost of this or more is one the skill cannot take: the game's mark for a skill that
/// cannot be specialized (or trained) by this heritage.
pub const UNBUYABLE: i32 = 999;

/// One line of the skills list: a heading over a group, or the skill in a row of the list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SkillLine {
    Heading(&'static str),
    Skill(usize),
}

/// The skills list's lines for rows whose levels are `levels`, already sorted specialized, then
/// trained, then untrained: each group under its heading.
#[must_use]
pub fn skill_lines(levels: &[SkillAdvancementClass]) -> Vec<SkillLine> {
    let mut out = Vec::with_capacity(levels.len() + 3);
    let mut last = None;
    for (i, level) in levels.iter().enumerate() {
        let word = match level {
            SkillAdvancementClass::Specialized => "Specialized",
            SkillAdvancementClass::Trained => "Trained",
            _ => "Untrained",
        };
        if last != Some(word) {
            out.push(SkillLine::Heading(word));
            last = Some(word);
        }
        out.push(SkillLine::Skill(i));
    }
    out
}

/// A skill's two steps on the skills page, as the game's arrows offer them: raising, to what and
/// for how much, and whether it can be done now; lowering, to what and for how much back, and
/// whether it may be. A step costing [`UNBUYABLE`] or more, or more than the `credits` left, is
/// not on; a skill the heritage grants (a cost of nothing) may not be lowered past it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SkillSteps {
    pub raise: Option<(SkillAdvancementClass, i32)>,
    pub raise_on: bool,
    pub lower: Option<(SkillAdvancementClass, i32)>,
    pub lower_on: bool,
}

#[must_use]
pub fn skill_steps(
    level: SkillAdvancementClass,
    train: i32,
    spec: i32,
    credits: i32,
) -> SkillSteps {
    use SkillAdvancementClass as L;
    let (raise, lower) = match level {
        L::Untrained => (Some((L::Trained, train)), None),
        L::Trained => (
            Some((L::Specialized, spec - train)),
            Some((L::Untrained, train)),
        ),
        L::Specialized => (None, Some((L::Trained, spec - train))),
        _ => (None, None),
    };
    SkillSteps {
        raise,
        raise_on: raise.is_some_and(|(_, cost)| cost < UNBUYABLE && cost <= credits),
        lower,
        lower_on: match level {
            L::Trained => train != 0,
            L::Specialized => spec != 0,
            _ => false,
        },
    }
}

fn skill_rank(level: SkillAdvancementClass) -> u8 {
    match level {
        SkillAdvancementClass::Specialized => 0,
        SkillAdvancementClass::Trained => 1,
        _ => 2,
    }
}

impl Creation {
    /// Whether the model wears its clothes: not until the clothes are come to (or a step after
    /// them), so the body is chosen unclothed. What is made wears them either way.
    #[must_use]
    pub fn dressed(&self) -> bool {
        let at = |s: Step| Step::ALL.iter().position(|x| *x == s);
        self.attire_seen || at(self.step) >= at(Step::Attire)
    }

    /// Whether the model is shown close, on the face and shoulders: while the face is chosen.
    #[must_use]
    pub fn close_up(&self) -> bool {
        self.step == Step::Appearance
    }

    /// A new creation over `tables`, its choices drawn as the game's wizard draws them on entry.
    /// `responses_seen` is how many verification notices the game has already had.
    #[must_use]
    pub fn new(
        tables: Rc<CreationTables>,
        texts: Rc<dereth_chargen::CreationTexts>,
        account_has_tod: bool,
        responses_seen: u32,
    ) -> Self {
        let mut state = CharGenState::with_policy(CreationPolicy::Modern);
        state.begin_creation(&tables, CreationEntry::Normal, account_has_tod);
        Self {
            state,
            tables,
            texts,
            step: Step::Heritage,
            name: String::new(),
            name_focus: 0,
            waiting: false,
            sent_name: None,
            responses_seen,
            accepted: false,
            status: None,
            skills_scroll: 0.0,
            preview: None,
            heading: HEADING,
            turning: None,
            confirm_unspent: false,
            tod: account_has_tod,
            seeded: false,
            attire_seen: false,
        }
    }

    /// Roll the first character again from the client's start-up seeds, once they are known, as
    /// the other interfaces roll it: without them every creation would open on the same one.
    fn seed(&mut self, seeds: Option<(i32, u32)>) {
        let Some((ran2, crt)) = seeds.filter(|_| !self.seeded) else {
            return;
        };
        self.seeded = true;
        let mut state = CharGenState::with_policy(CreationPolicy::Modern);
        state.rng = dereth_chargen::CharGenRng::new(ran2, crt);
        state.begin_creation(&self.tables, CreationEntry::Normal, self.tod);
        self.state = state;
    }

    /// The creation tables the model reads.
    #[must_use]
    pub fn tables(&self) -> &CreationTables {
        &self.tables
    }

    fn heritage(&self) -> Option<&dereth_assets::tables::HeritageGroup> {
        self.tables
            .chargen
            .heritage_groups
            .get(&self.state.heritage_group)
    }

    fn sex(&self) -> Option<&dereth_assets::tables::SexCg> {
        self.state.sex(&self.tables.chargen)
    }

    /// The steps shown for the heritage chosen.
    fn steps(&self) -> Vec<Step> {
        Step::ALL
            .into_iter()
            .filter(|s| s.shown_for(self.state.heritage_group))
            .collect()
    }

    /// Follow the game's answer to a character sent: a refusal in the game's words, or, once
    /// accepted and listed, the new character into the world. True when it goes into the world.
    pub fn follow_server(&mut self, state: &GameState, out: &mut Outcome) -> bool {
        if state.chargen_response_notices != self.responses_seen {
            self.responses_seen = state.chargen_response_notices;
            if self.sent_name.is_some() {
                match &state.chargen_refusal {
                    None => {
                        self.accepted = true;
                        self.status = Some("Waiting for the character list...".into());
                    }
                    Some(words) => {
                        self.waiting = false;
                        self.sent_name = None;
                        self.status = Some(words.clone());
                    }
                }
            }
        }
        if !self.accepted {
            return false;
        }
        let Some(name) = &self.sent_name else {
            return false;
        };
        // A server may put a mark before a privileged account's names.
        if let Some(c) = state.characters.iter().find(|c| {
            c.id.0 != 0
                && c.delete_seconds == 0
                && c.name.trim_start_matches('+').eq_ignore_ascii_case(name)
        }) {
            out.requests
                .push(UiRequest::CharGenAction(CharGenAction::LogOn(c.id)));
            self.sent_name = None;
            self.accepted = false;
            self.status = Some("Entering the world...".into());
            return true;
        }
        false
    }

    /// Turn the model by dragging it, and say so under it.
    fn turn(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, preview: Rect) {
        let k = p.scale;
        let x = ctx.input.mouse.0;
        if ctx.input.pressed[0] && ctx.over(&preview) {
            self.turning = Some(x);
            ctx.input.captured = true;
        }
        if let Some(from) = self.turning {
            if ctx.input.down[0] {
                // The model turns the way the pointer pulls its front: a drag to the right
                // brings its face round to the right, which is a smaller heading.
                self.heading = (self.heading - (x - from) * TURN_DEGREES_PER_POINT / k) % 360.0;
                self.turning = Some(x);
            } else {
                self.turning = None;
            }
        }
        let hint = TextStyle::new(Family::Body, 12.0, 0xC0E0_E0E0).edge(0xFF00_0000);
        p.text_in(
            &hint,
            Rect::new(preview.x, preview.bottom() + 6.0 * k, preview.w, 18.0 * k),
            Align::Centre,
            "Drag to turn",
        );
    }

    /// One frame of the window. `Some(false)` when the player backs out to the lobby.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) -> Option<bool> {
        self.seed(state.chargen_seeds);
        let k = p.scale;
        let (sw, sh) = p.screen;
        let (w, h) = (760.0, 640.0);
        let mut ws = kit::WindowState {
            open: true,
            pos: Some((sw - (w + 60.0) * k, (sh - h * k) / 2.0)),
            ..kit::WindowState::default()
        };
        let win = kit::window(p, ctx, &mut ws, "Character Creation", (w, h), (0.0, 0.0));
        // The model, to the left of the window, in a portrait-shaped frame.
        let room = (win.rect.x - 80.0 * k).max(0.0);
        let (ph, pw) = (sh * 0.8, (sh * 0.8 * 0.6).min(room));
        let preview = Rect::new(40.0 * k + (room - pw) / 2.0, sh * 0.1, pw, ph);
        self.preview = Some(preview);
        self.turn(p, ctx, preview);
        if win.closed {
            return Some(false);
        }
        let body = win.body;
        // The steps down the left of the window.
        let steps = self.steps();
        if !steps.contains(&self.step) {
            self.step = Step::Heritage;
        }
        let list = Rect::new(body.x, body.y, 150.0 * k, body.h - 50.0 * k);
        let row_h = 30.0 * k;
        let item = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        for (i, s) in steps.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let r = Rect::new(list.x, list.y + row_h * i as f32, list.w, row_h);
            crate::ui::pregame::list_highlight(p, r, *s == self.step);
            p.text_in(&item, r.offset(10.0 * k, 0.0), Align::Left, s.label());
            if !self.waiting && ctx.over(&r) && ctx.input.clicked(&r) {
                self.step = *s;
            }
        }
        let page = Rect::new(
            list.right() + 16.0 * k,
            body.y,
            body.right() - list.right() - 16.0 * k,
            body.h - 50.0 * k,
        );
        if !self.waiting {
            match self.step {
                Step::Heritage => self.heritage_page(p, ctx, page),
                Step::Sex => self.sex_page(p, ctx, page),
                Step::Appearance => self.appearance_page(p, ctx, page),
                Step::Attire => {
                    self.attire_seen = true;
                    self.attire_page(p, ctx, page);
                }
                Step::Profession => self.profession_page(p, ctx, page),
                Step::Attributes => self.attributes_page(p, ctx, page),
                Step::Skills => self.skills_page(p, ctx, page),
                Step::Town => self.town_page(p, ctx, page, state),
                Step::Name => self.name_page(p, ctx, page),
            }
        }
        // The foot: what the window is waiting for, Back and Next or Create.
        let foot = Rect::new(body.x, body.bottom() - 40.0 * k, body.w, 36.0 * k);
        if let Some(s) = &self.status {
            let note = TextStyle::new(Family::Body, 13.0, 0xFFFF_D080).edge(0xFF00_0000);
            p.text_in(&note, foot, Align::Left, s);
        }
        let at = steps.iter().position(|s| *s == self.step).unwrap_or(0);
        let bw = 130.0 * k;
        let back = Rect::new(foot.right() - 2.0 * bw - 10.0 * k, foot.y, bw, foot.h);
        let next = Rect::new(foot.right() - bw, foot.y, bw, foot.h);
        // The pad's cancel steps back, as this button does.
        ctx.input.nav.cancel(back);
        if kit::button(
            p,
            ctx,
            back,
            if at == 0 { "Cancel" } else { "Back" },
            !self.waiting,
        ) {
            if at == 0 {
                return Some(false);
            }
            self.step = steps[at - 1];
        }
        let last = at + 1 == steps.len();
        let label = if last { "Create" } else { "Next" };
        ctx.input.nav.home(next);
        if kit::button(
            p,
            ctx,
            next,
            label,
            !self.waiting && (!last || self.can_create()),
        ) {
            if last {
                self.create(state, out);
            } else {
                self.step = steps[at + 1];
            }
        }
        if self.confirm_unspent {
            match crate::ui::pregame::message_box(
                p,
                ctx,
                "Character Creation",
                "You have not spent all your attribute credits. Create this character anyway?",
                &["Create", "Cancel"],
            ) {
                Some(0) => {
                    self.confirm_unspent = false;
                    self.send(state, out);
                }
                Some(_) => self.confirm_unspent = false,
                None => {}
            }
        }
        None
    }

    /// Whether the character can be sent: it has a name.
    fn can_create(&self) -> bool {
        !self.name.trim().is_empty()
    }

    /// Create: asked about unspent credits first, as the game asks.
    fn create(&mut self, state: &GameState, out: &mut Outcome) {
        if self.state.remaining_atrb_credits > 0 {
            self.confirm_unspent = true;
            return;
        }
        self.send(state, out);
    }

    /// Send the character, in the first free slot of the account.
    fn send(&mut self, state: &GameState, out: &mut Outcome) {
        if !self.state.set_name(self.name.trim()) {
            self.status = Some("That name cannot be used.".into());
            return;
        }
        let Some(slot) = state.free_slot else {
            self.status = Some("All character slots are full.".into());
            return;
        };
        self.state.prepare_summary(&self.tables);
        self.state.set_slot(slot);
        out.requests
            .push(UiRequest::CharGenAction(CharGenAction::SendCharGenResult(
                Box::new(self.state.get_char_gen_result()),
            )));
        self.sent_name = Some(self.name.trim().to_owned());
        self.waiting = true;
        self.status = Some("Creating the character...".into());
    }

    fn heading(p: &mut Painter<'_>, ctx: &Ctx<'_>, r: Rect, text: &str) -> f32 {
        let k = p.scale;
        let style = TextStyle::new(Family::Heading, 23.0, ctx.colours.heading()).edge(0xFF00_0000);
        p.text(&style, r.x, r.y, text);
        r.y + 34.0 * k
    }

    fn paragraph(p: &mut Painter<'_>, ctx: &Ctx<'_>, x: f32, y: f32, w: f32, text: &str) {
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let lh = p.line_height(&style);
        let mut ly = y;
        let lines: Vec<String> = text.lines().flat_map(|l| p.wrap(&style, l, w)).collect();
        for line in lines {
            p.text(&style, x, ly, &line);
            ly += lh;
        }
    }

    /// A list of choices; the index clicked.
    fn choices(
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        names: &[String],
        at: Option<usize>,
    ) -> Option<usize> {
        let k = p.scale;
        let row_h = 28.0 * k;
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let mut chosen = None;
        for (i, n) in names.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let row = Rect::new(r.x, r.y + row_h * i as f32, r.w, row_h);
            if row.bottom() > r.bottom() {
                break;
            }
            crate::ui::pregame::list_highlight(p, row, Some(i) == at);
            p.text_in(&style, row.offset(10.0 * k, 0.0), Align::Left, n);
            if ctx.over(&row) && ctx.input.clicked(&row) {
                chosen = Some(i);
            }
        }
        chosen
    }

    /// A labelled row with a stepper over `count` numbered choices; the choice made.
    fn numbered(
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        row: Rect,
        label: &str,
        count: usize,
        at: i32,
    ) -> Option<i32> {
        if count == 0 {
            return None;
        }
        let (x, y, w) = (row.x, row.y, row.w);
        let k = p.scale;
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        p.text(&style, x, y + 4.0 * k, label);
        let names: Vec<String> = (1..=count).map(|n| format!("{n} of {count}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        let current = usize::try_from(at).unwrap_or(0).min(count - 1);
        kit::stepper(
            p,
            ctx,
            Rect::new(x + w * 0.4, y, w * 0.6, 26.0 * k),
            &refs,
            current,
        )
        .and_then(|i| i32::try_from(i).ok())
    }

    /// A labelled shade slider, 0 to 1; the shade chosen.
    fn shade(
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        row: Rect,
        label: &str,
        value: f64,
    ) -> Option<f64> {
        let (x, y, w) = (row.x, row.y, row.w);
        let k = p.scale;
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        p.text(&style, x, y + 4.0 * k, label);
        #[allow(clippy::cast_possible_truncation)]
        let v = value.clamp(0.0, 1.0) as f32;
        kit::slider(
            p,
            ctx,
            Rect::new(x + w * 0.4, y, w * 0.6, 26.0 * k),
            v,
            0.0,
            1.0,
        )
        .map(f64::from)
    }

    fn random_button(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        r: Rect,
        page: dereth_chargen::CreationRandom,
    ) {
        let k = p.scale;
        let b = Rect::new(r.right() - 110.0 * k, r.y, 110.0 * k, 28.0 * k);
        if kit::button(p, ctx, b, "Random", true) {
            self.state.randomize_page(&self.tables, page, self.tod);
        }
    }

    fn heritage_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let k = p.scale;
        let y = Self::heading(p, ctx, r, "Heritage");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Heritage);
        let keys = self.tables.heritage_keys().to_vec();
        let names: Vec<String> = keys
            .iter()
            .map(|k| {
                self.tables
                    .chargen
                    .heritage_groups
                    .get(k)
                    .map_or_else(String::new, |h| heritage_title(*k, &h.name))
            })
            .collect();
        let at = keys.iter().position(|k| *k == self.state.heritage_group);
        let list = Rect::new(r.x, y, r.w * 0.45, r.bottom() - y);
        if let Some(i) = Self::choices(p, ctx, list, &names, at) {
            self.state.choose_heritage(&self.tables, keys[i]);
        }
        if let Some(text) = self.heritage().and_then(|h| self.texts.get(h.description)) {
            let text = text.to_owned();
            Self::paragraph(
                p,
                ctx,
                list.right() + 16.0 * k,
                y,
                r.w * 0.55 - 16.0 * k,
                &text,
            );
        }
    }

    fn sex_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let y = Self::heading(p, ctx, r, "Sex");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Sex);
        let keys = self.tables.sex_keys(self.state.heritage_group).to_vec();
        let names: Vec<String> = keys
            .iter()
            .map(|k| {
                self.heritage()
                    .and_then(|h| h.sexes.get(k))
                    .map_or_else(String::new, |s| s.name.clone())
            })
            .collect();
        let at = keys.iter().position(|k| *k == self.state.gender);
        if let Some(i) = Self::choices(
            p,
            ctx,
            Rect::new(r.x, y, r.w * 0.45, r.bottom() - y),
            &names,
            at,
        ) {
            self.state.choose_gender(&self.tables, keys[i]);
        }
    }

    fn appearance_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let k = p.scale;
        let mut y = Self::heading(p, ctx, r, "Appearance");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Appearance);
        let Some(sex) = self.sex().cloned() else {
            return;
        };
        let step = 34.0 * k;
        let s = &mut self.state;
        if let Some(v) = Self::numbered(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Eyes",
            sex.eye_strips.len(),
            s.eyes_strip,
        ) {
            s.eyes_strip = v;
        }
        y += step;
        if let Some(v) = Self::numbered(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Nose",
            sex.nose_strips.len(),
            s.nose_strip,
        ) {
            s.nose_strip = v;
        }
        y += step;
        if let Some(v) = Self::numbered(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Mouth",
            sex.mouth_strips.len(),
            s.mouth_strip,
        ) {
            s.mouth_strip = v;
        }
        y += step;
        if let Some(v) = Self::numbered(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Hair style",
            sex.hair_styles.len(),
            s.hair_style,
        ) {
            s.set_hair_style(v);
        }
        y += step;
        if let Some(v) = Self::numbered(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Hair colour",
            sex.hair_colors.len(),
            s.hair_color,
        ) {
            s.set_hair_color(v);
        }
        y += step;
        if let Some(v) = Self::shade(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Hair shade",
            s.hair_shade,
        ) {
            s.set_hair_shade(v);
        }
        y += step;
        if let Some(v) = Self::numbered(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Eye colour",
            sex.eye_colors.len(),
            s.eye_color,
        ) {
            s.set_eye_color(v);
        }
        y += step;
        if let Some(v) = Self::shade(
            p,
            ctx,
            Rect::new(r.x, y, r.w, 26.0 * k),
            "Skin",
            s.skin_shade,
        ) {
            s.set_skin_shade(v);
        }
    }

    fn attire_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let k = p.scale;
        let mut y = Self::heading(p, ctx, r, "Attire");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Clothing);
        let Some(sex) = self.sex().cloned() else {
            return;
        };
        self.state.prepare_clothing(&self.tables);
        let cg = &self.tables.chargen;
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let parts: [(&str, &[dereth_assets::tables::GearItem]); 4] = [
            ("Headgear", &sex.headgear),
            ("Shirt", &sex.shirts),
            ("Trousers", &sex.pants),
            ("Footwear", &sex.footwear),
        ];
        for (part, (label, items)) in parts.into_iter().enumerate() {
            if items.is_empty() {
                continue;
            }
            let s = &mut self.state;
            let (style_at, colour_at, colours, shade) = match part {
                0 => (
                    s.headgear_style,
                    s.headgear_color,
                    s.num_headgear_colors,
                    s.headgear_shade,
                ),
                1 => (
                    s.shirt_style,
                    s.shirt_color,
                    s.num_shirt_colors,
                    s.shirt_shade,
                ),
                2 => (
                    s.trousers_style,
                    s.trousers_color,
                    s.num_trousers_colors,
                    s.trousers_shade,
                ),
                _ => (
                    s.footwear_style,
                    s.footwear_color,
                    s.num_footwear_colors,
                    s.footwear_shade,
                ),
            };
            p.text(
                &style.colour(ctx.colours.heading()),
                r.x,
                y + 4.0 * k,
                label,
            );
            y += 30.0 * k;
            let names = style_names(part, items);
            let at = style_row(part, style_at).min(names.len() - 1);
            p.text(&style, r.x, y + 4.0 * k, "Style");
            if let Some(i) = kit::stepper(
                p,
                ctx,
                Rect::new(r.x + r.w * 0.4, y, r.w * 0.6, 26.0 * k),
                &names,
                at,
            ) {
                let i = row_style(part, i);
                match part {
                    0 => s.set_headgear_style(cg, i),
                    1 => s.set_shirt_style(cg, i),
                    2 => s.set_trousers_style(cg, i),
                    _ => s.set_footwear_style(cg, i),
                }
            }
            y += 30.0 * k;
            let count = usize::try_from(colours).unwrap_or(0);
            if let Some(v) = Self::numbered(
                p,
                ctx,
                Rect::new(r.x, y, r.w, 26.0 * k),
                "Colour",
                count,
                colour_at,
            ) {
                match part {
                    0 => s.headgear_color = v,
                    1 => s.shirt_color = v,
                    2 => s.trousers_color = v,
                    _ => s.footwear_color = v,
                }
            }
            y += 30.0 * k;
            if let Some(v) = Self::shade(p, ctx, Rect::new(r.x, y, r.w, 26.0 * k), "Shade", shade) {
                match part {
                    0 => s.headgear_shade = v,
                    1 => s.shirt_shade = v,
                    2 => s.trousers_shade = v,
                    _ => s.footwear_shade = v,
                }
            }
            y += 36.0 * k;
        }
    }

    fn profession_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let y = Self::heading(p, ctx, r, "Profession");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Template);
        let Some(h) = self.heritage() else {
            return;
        };
        let mut names: Vec<String> = h.templates.iter().map(|t| t.name.clone()).collect();
        names.push("Custom".into());
        let custom = names.len() - 1;
        let at = usize::try_from(self.state.template).map_or(Some(custom), Some);
        if let Some(i) = Self::choices(
            p,
            ctx,
            Rect::new(r.x, y, r.w * 0.5, r.bottom() - y),
            &names,
            at,
        ) {
            let index = if i == custom {
                -1
            } else {
                i32::try_from(i).unwrap_or(-1)
            };
            self.state.choose_template(&self.tables, index);
        }
    }

    fn attributes_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let k = p.scale;
        let mut y = Self::heading(p, ctx, r, "Attributes");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Attributes);
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        let left = remaining(
            self.state.remaining_atrb_credits,
            self.state.total_atrb_credits,
        );
        p.text(&style, r.x, y, &left);
        y += 34.0 * k;
        for (attr, name) in ATTRIBUTES {
            p.text(&style, r.x, y + 4.0 * k, name);
            let v = self.state.get(attr);
            let (lo, hi) = (self.state.atrb_min, self.state.atrb_max);
            #[allow(clippy::cast_precision_loss)]
            if let Some(nv) = kit::slider(
                p,
                ctx,
                Rect::new(r.x + r.w * 0.3, y, r.w * 0.6, 26.0 * k),
                v as f32,
                lo as f32,
                hi as f32,
            ) {
                let nv = dereth_primitives::num::to_i32(nv.round());
                self.state.set_attribute_balanced(attr, nv, true);
            }
            y += 34.0 * k;
        }
    }

    /// The skills a character of this heritage can have, each with its training and
    /// specialization costs: specialized first, then trained, then untrained, each by name.
    fn skill_rows(&self) -> Vec<(u32, String, (i32, i32))> {
        let mut rows: Vec<(u32, String, (i32, i32))> = self
            .tables
            .skills
            .skills
            .iter()
            .filter(|(id, _)| {
                self.state
                    .skill_levels
                    .get(**id as usize)
                    .is_some_and(|l| *l != SkillAdvancementClass::Inactive)
            })
            .map(|(id, s)| {
                (
                    *id,
                    s.name.clone(),
                    CharGenState::skill_costs(
                        &self.tables.chargen,
                        &self.tables.skills,
                        self.state.heritage_group,
                        *id,
                    ),
                )
            })
            .collect();
        rows.sort_by_key(|(id, name, _)| (skill_rank(self.state.skill_level(*id)), name.clone()));
        rows
    }

    fn skills_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let k = p.scale;
        let mut y = Self::heading(p, ctx, r, "Skills");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Skills);
        let style = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let left = remaining(
            self.state.remaining_skill_credits,
            self.state.total_skill_credits,
        );
        p.text(&style, r.x, y, &left);
        y += 30.0 * k;
        let rows = self.skill_rows();
        let levels: Vec<SkillAdvancementClass> = rows
            .iter()
            .map(|(id, _, _)| self.state.skill_level(*id))
            .collect();
        let lines = skill_lines(&levels);
        let row_h = 26.0 * k;
        let area = Rect::new(r.x, y, r.w, r.bottom() - y);
        #[allow(clippy::cast_precision_loss)]
        let content = row_h * lines.len() as f32;
        let offset = kit::scroll(p, ctx, area, content, &mut self.skills_scroll);
        let credits = self.state.remaining_skill_credits;
        let heading =
            TextStyle::new(Family::Body, 12.0, ctx.colours.heading()).edge(ctx.colours.edge());
        p.list.push_clip(area);
        for (i, line) in lines.iter().enumerate() {
            #[allow(clippy::cast_precision_loss)]
            let ry = area.y + row_h * i as f32 - offset;
            if ry + row_h < area.y || ry > area.bottom() {
                continue;
            }
            let row = match line {
                SkillLine::Heading(word) => {
                    p.text(&heading, r.x, ry + 8.0 * k, word);
                    continue;
                }
                SkillLine::Skill(row) => *row,
            };
            let (id, name, (train, spec)) = &rows[row];
            let level = levels[row];
            let (word, colour) = match level {
                SkillAdvancementClass::Specialized => ("Specialized", 0xFFFF_D080),
                SkillAdvancementClass::Trained => ("Trained", 0xFF90_E0FF),
                _ => ("Untrained", ctx.colours.text()),
            };
            p.text(&style, r.x, ry + 4.0 * k, name);
            p.text(&style.colour(colour), r.x + r.w * 0.42, ry + 4.0 * k, word);
            // Each step's cost on its own button: what raising costs, what lowering returns.
            let raise = Rect::new(
                r.right() - 120.0 * k,
                ry + 2.0 * k,
                54.0 * k,
                row_h - 4.0 * k,
            );
            let lower = Rect::new(
                r.right() - 60.0 * k,
                ry + 2.0 * k,
                54.0 * k,
                row_h - 4.0 * k,
            );
            let steps = skill_steps(level, *train, *spec, credits);
            let up = steps.raise.map(|(to, _)| to);
            let down = steps.lower.map(|(to, _)| to);
            // A step that cannot be bought (a skill that cannot be specialized costs 999 or
            // more) shows greyed with no cost, as one the credits cannot pay for does.
            let raise_label = steps
                .raise
                .filter(|(_, cost)| *cost < UNBUYABLE)
                .map_or_else(String::new, |(_, cost)| cost.to_string());
            let lower_label = steps
                .lower
                .filter(|_| steps.lower_on)
                .map_or_else(String::new, |(_, back)| back.to_string());
            let up = up.filter(|_| steps.raise_on);
            let down = down.filter(|_| steps.lower_on);
            if kit::step_button(p, ctx, raise, true, &raise_label, up.is_some()) {
                if let Some(to) = up {
                    self.state
                        .set_skill_level(&self.tables.chargen, &self.tables.skills, *id, to);
                }
            }
            if kit::step_button(p, ctx, lower, false, &lower_label, down.is_some()) {
                if let Some(to) = down {
                    self.state
                        .set_skill_level(&self.tables.chargen, &self.tables.skills, *id, to);
                }
            }
        }
        p.list.pop_clip();
    }

    fn town_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, state: &GameState) {
        let y = Self::heading(p, ctx, r, "Starting Town");
        self.random_button(p, ctx, r, dereth_chargen::CreationRandom::Town);
        let Some(h) = self.heritage() else {
            return;
        };
        let areas: Vec<u32> = h
            .primary_start_areas
            .iter()
            .chain(&h.secondary_start_areas)
            .copied()
            .collect();
        let names: Vec<String> = areas
            .iter()
            .map(|a| {
                self.tables
                    .chargen
                    .starter_areas
                    .get(*a as usize)
                    .map_or_else(|| format!("Town {a}"), |s| s.name.clone())
            })
            .collect();
        let at = areas.iter().position(|a| *a == self.state.start_area);
        if let Some(i) = Self::choices(
            p,
            ctx,
            Rect::new(r.x, y, r.w * 0.5, r.bottom() - y),
            &names,
            at,
        ) {
            self.state.set_start_area(areas[i]);
        }
        let _ = state;
    }

    fn name_page(&mut self, p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect) {
        let k = p.scale;
        let mut y = Self::heading(p, ctx, r, "Name");
        let style = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
        if let Some(help) = self.sex().and_then(|s| self.texts.get(s.naming_help)) {
            let help = help.to_owned();
            Self::paragraph(p, ctx, r.x, y, r.w, &help);
            y += 90.0 * k;
        }
        // The box a little taller than the name's face needs.
        p.text(&style, r.x, y + 7.0 * k, "Name");
        let box_r = Rect::new(r.x + 80.0 * k, y, r.w - 80.0 * k, 34.0 * k);
        kit::text_box(p, ctx, box_r, &mut self.name, &mut self.name_focus, 1);
        // The name's length is the model's: at most the buffer the game keeps it in.
        if self.name.chars().count() > 32 {
            self.name = self.name.chars().take(32).collect();
        }
        y += 50.0 * k;
        // The summary of the choices.
        let summary =
            TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let heritage = self
            .heritage()
            .map(|h| heritage_title(self.state.heritage_group, &h.name))
            .unwrap_or_default();
        let sex = self.sex().map(|s| s.name.clone()).unwrap_or_default();
        let profession = self
            .heritage()
            .and_then(|h| {
                usize::try_from(self.state.template)
                    .ok()
                    .and_then(|t| h.templates.get(t))
            })
            .map_or_else(|| "Custom".to_owned(), |t| t.name.clone());
        let town = self
            .tables
            .chargen
            .starter_areas
            .get(self.state.start_area as usize)
            .map(|s| s.name.clone())
            .unwrap_or_default();
        for line in [
            format!("{heritage} {sex}, {profession}"),
            ATTRIBUTES
                .iter()
                .map(|(a, n)| format!("{} {}", &n[..3], self.state.get(*a)))
                .collect::<Vec<_>>()
                .join("  "),
            format!("Starting in {town}"),
        ] {
            p.text(&summary, r.x, y, &line);
            y += p.line_height(&summary);
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental Horizon interface)
    use super::*;
    use dereth_primitives::DataId;

    /// A world of one heritage, one sex and two towns, as a test's creation tables.
    fn tables() -> dereth_chargen::CreationTables {
        use dereth_assets::tables::*;
        use std::collections::BTreeMap;
        use std::rc::Rc;
        let desc = ObjDesc {
            version: 0x11,
            palette: None,
            subpalettes: vec![],
            texture_changes: vec![],
            anim_part_changes: vec![],
        };
        let clothes = vec![
            GearItem {
                name: "First".into(),
                clothing_table: DataId(0x10000001),
                weenie_default: 0,
            },
            GearItem {
                name: "Last".into(),
                clothing_table: DataId(0x10000002),
                weenie_default: 0,
            },
        ];
        let sex = SexCg {
            naming_help: None,
            name: "Female".into(),
            scale: 100,
            setup: DataId(0x2000001),
            sound_table: DataId(0),
            icon: 0,
            base_palette: DataId(0),
            skin_palset: DataId(0),
            physics_table: DataId(0),
            motion_table: DataId(0),
            combat_table: DataId(0),
            base_objdesc: desc.clone(),
            hair_colors: vec![],
            hair_styles: vec![],
            eye_colors: vec![],
            eye_strips: vec![],
            nose_strips: vec![],
            mouth_strips: vec![],
            headgear: clothes.clone(),
            shirts: clothes,
            pants: vec![],
            footwear: vec![],
            clothing_colors: vec![1, 2, 3, 4, 5, 6],
        };
        let heritage = HeritageGroup {
            description: None,
            name: "Aluvian".into(),
            icon: 0,
            setup: DataId(0),
            environment_setup: DataId(0),
            attribute_credits: 330,
            skill_credits: 100,
            primary_start_areas: vec![0, 1],
            secondary_start_areas: vec![],
            skills: vec![],
            templates: vec![CharGenTemplate {
                name: "Adventurer".into(),
                icon: 0,
                title: 0,
                attributes: [50; 6],
                normal_skills: vec![],
                primary_skills: vec![],
            }],
            sex_table_marker: 0,
            sex_order: vec![2],
            sexes: BTreeMap::from([(2, sex)]),
            template_presentations: BTreeMap::new(),
        };
        let skills = BTreeMap::from([
            (1, ("Unused", 0, 0, 0)),
            (7, ("First skill", 1, 4, 8)),
            (8, ("Second skill", 1, 6, 12)),
        ])
        .into_iter()
        .map(
            |(id, (name, chargen_use, trained_cost, specialized_cost))| {
                (
                    id,
                    SkillBase {
                        description: String::new(),
                        name: name.into(),
                        icon: 0,
                        trained_cost,
                        specialized_cost,
                        category: 0,
                        chargen_use,
                        min_level: 1,
                        formula: SkillFormula {
                            w: 0,
                            x: 1,
                            y: 1,
                            z: 4,
                            attr1: 1,
                            attr2: 4,
                        },
                        upper_bound: 0.0,
                        lower_bound: 0.0,
                        learn_mod: 0.0,
                    },
                )
            },
        )
        .collect();
        let clothing = (1..=2)
            .map(|n| {
                let id = DataId(0x10000000 + n);
                (
                    id,
                    dereth_assets::motion::ClothingTable {
                        id,
                        clothing_base_buckets: 0,
                        clothing_bases: BTreeMap::new(),
                        palette_template_buckets: 1,
                        palette_templates: if n == 1 {
                            (1..=6)
                                .map(|i| {
                                    (
                                        i,
                                        dereth_assets::motion::PaletteTemplate {
                                            icon: DataId(0),
                                            subpalette_effects: vec![
                                                dereth_assets::motion::PaletteEffect {
                                                    ranges: vec![],
                                                    palette_set: DataId(0),
                                                },
                                            ],
                                        },
                                    )
                                })
                                .collect()
                        } else {
                            BTreeMap::new()
                        },
                    },
                )
            })
            .collect();
        dereth_chargen::CreationTables {
            chargen: CharGen {
                help_strings: vec![],
                id: DataId(0),
                second_data_id: DataId(0),
                starter_areas: vec![
                    StarterArea {
                        name: "North".into(),
                        locations: vec![],
                    },
                    StarterArea {
                        name: "South".into(),
                        locations: vec![],
                    },
                ],
                hg_table_marker: 0,
                heritage_order: vec![1],
                heritage_groups: BTreeMap::from([(1, heritage)]),
            },
            skills: SkillTable {
                id: DataId(0),
                buckets: 0,
                skills,
            },
            clothing: Rc::new(clothing),
        }
    }

    fn creation() -> Creation {
        Creation::new(
            Rc::new(tables()),
            Rc::new(dereth_chargen::CreationTexts::default()),
            false,
            0,
        )
    }

    fn lobby() -> GameState {
        GameState {
            free_slot: Some(2),
            ..GameState::default()
        }
    }

    #[test]
    fn a_named_character_goes_to_the_server_in_the_first_free_slot() {
        let mut c = creation();
        c.name = "Aurelia".into();
        let mut out = Outcome::default();
        c.send(&lobby(), &mut out);
        let sent = out.requests.iter().find_map(|r| match r {
            UiRequest::CharGenAction(CharGenAction::SendCharGenResult(r)) => Some(r.clone()),
            _ => None,
        });
        let sent = sent.expect("the character went to the server");
        assert_eq!(sent.slot, 2);
        assert_eq!(sent.name, "Aurelia");
        assert!(c.waiting);
    }

    #[test]
    fn an_account_with_no_free_slot_sends_nothing_and_says_so() {
        let mut c = creation();
        c.name = "Aurelia".into();
        let mut out = Outcome::default();
        c.send(&GameState::default(), &mut out);
        assert!(out.requests.is_empty());
        assert!(!c.waiting);
        assert_eq!(c.status.as_deref(), Some("All character slots are full."));
    }

    #[test]
    fn a_refusal_is_shown_in_the_game_s_words_and_the_choices_can_be_changed_again() {
        let mut c = creation();
        c.name = "Aurelia".into();
        let mut out = Outcome::default();
        c.send(&lobby(), &mut out);
        let refused = GameState {
            chargen_response: Some(3),
            chargen_response_notices: 1,
            chargen_refusal: Some("That name is reserved.".into()),
            ..lobby()
        };
        assert!(!c.follow_server(&refused, &mut out));
        assert!(!c.waiting);
        assert_eq!(c.status.as_deref(), Some("That name is reserved."));
        // The same answer read again is not a second answer.
        c.status = None;
        c.follow_server(&refused, &mut out);
        assert_eq!(c.status, None);
    }

    #[test]
    fn an_accepted_character_goes_into_the_world_once_the_list_names_it() {
        let mut c = creation();
        c.name = "Aurelia".into();
        let mut out = Outcome::default();
        c.send(&lobby(), &mut out);
        let mut accepted = GameState {
            chargen_response: Some(1),
            chargen_response_notices: 1,
            ..lobby()
        };
        let mut out = Outcome::default();
        assert!(!c.follow_server(&accepted, &mut out), "not listed yet");
        assert!(out.requests.is_empty());
        accepted.characters.push(crate::ui::game::CharacterEntry {
            id: dereth_primitives::ObjectId(0x5000_0042),
            name: "+Aurelia".into(),
            delete_seconds: 0,
        });
        assert!(c.follow_server(&accepted, &mut out));
        assert_eq!(
            out.requests,
            [UiRequest::CharGenAction(CharGenAction::LogOn(
                dereth_primitives::ObjectId(0x5000_0042)
            ))]
        );
    }

    #[test]
    fn unspent_attribute_credits_are_asked_about_before_anything_is_sent() {
        let mut c = creation();
        c.name = "Aurelia".into();
        c.state.remaining_atrb_credits = 5;
        let mut out = Outcome::default();
        c.create(&lobby(), &mut out);
        assert!(out.requests.is_empty());
        assert!(c.confirm_unspent);
    }

    #[test]
    fn the_olthoi_skip_the_face_the_clothes_and_the_profession() {
        let olthoi = dereth_chargen::HERITAGE_OLTHOI;
        let shown: Vec<Step> = Step::ALL
            .into_iter()
            .filter(|s| s.shown_for(olthoi))
            .collect();
        assert_eq!(
            shown,
            [
                Step::Heritage,
                Step::Sex,
                Step::Attributes,
                Step::Skills,
                Step::Town,
                Step::Name
            ]
        );
        assert_eq!(Step::ALL.into_iter().filter(|s| s.shown_for(1)).count(), 9);
    }

    /// One frame of turning the model shown at (100, 100, 200, 400), with no art behind it.
    fn turn_frame(c: &mut Creation, input: &mut crate::ui::input::InputFrame) {
        let art = crate::art::Art::empty();
        let colours = crate::ui::colours::Colours::default();
        let mut list = crate::draw::DrawList::default();
        let mut p = Painter {
            list: &mut list,
            art: &art,
            scale: 1.0,
            screen: (800.0, 600.0),
            fade: 1.0,
        };
        let mut drag = None;
        let mut ctx = Ctx {
            time: 0.0,
            dt: 1.0 / 60.0,
            input,
            colours: &colours,
            hot: false,
            drag: &mut drag,
            drops: Vec::new(),
        };
        c.turn(&mut p, &mut ctx, Rect::new(100.0, 100.0, 200.0, 400.0));
        input.next_frame();
    }

    #[test]
    fn the_model_stands_still_and_turns_only_while_dragged() {
        let mut c = creation();
        let mut input = crate::ui::input::InputFrame {
            mouse: (150.0, 300.0),
            ..Default::default()
        };
        for _ in 0..120 {
            turn_frame(&mut c, &mut input);
        }
        assert_eq!(c.heading, HEADING, "left alone, it does not turn");
        input.pressed[0] = true;
        input.down[0] = true;
        turn_frame(&mut c, &mut input);
        input.down[0] = true;
        input.mouse = (200.0, 300.0);
        turn_frame(&mut c, &mut input);
        // A drag to the right turns its front to the right.
        assert!((c.heading - (HEADING - 50.0 * TURN_DEGREES_PER_POINT)).abs() < 1e-3);
        // Let go, and moving the pointer turns it no further.
        input.down[0] = false;
        turn_frame(&mut c, &mut input);
        let held = c.heading;
        input.mouse = (260.0, 300.0);
        turn_frame(&mut c, &mut input);
        assert_eq!(c.heading, held);
    }

    #[test]
    fn the_skills_list_puts_specialized_then_trained_then_untrained() {
        let mut c = creation();
        let set = |c: &mut Creation, id: usize, level| c.state.skill_levels[id] = level;
        set(&mut c, 1, SkillAdvancementClass::Inactive);
        set(&mut c, 7, SkillAdvancementClass::Untrained);
        set(&mut c, 8, SkillAdvancementClass::Specialized);
        let order = |c: &Creation| c.skill_rows().iter().map(|r| r.0).collect::<Vec<_>>();
        assert_eq!(order(&c), [8, 7]);
        set(&mut c, 7, SkillAdvancementClass::Trained);
        set(&mut c, 8, SkillAdvancementClass::Untrained);
        assert_eq!(order(&c), [7, 8]);
    }

    #[test]
    fn the_budgets_read_what_remains_of_the_whole() {
        assert_eq!(remaining(12, 330), "Remaining: 12 / 330");
    }

    #[test]
    fn a_hat_may_be_left_off_and_the_rest_of_the_clothes_may_not() {
        let gear = |name: &str| dereth_assets::tables::GearItem {
            name: name.into(),
            clothing_table: dereth_primitives::DataId(0),
            weenie_default: 0,
        };
        let items = [gear("Cap"), gear("Hood")];
        assert_eq!(style_names(0, &items), ["None", "Cap", "Hood"]);
        assert_eq!(style_names(1, &items), ["Cap", "Hood"]);
        // No hat is the first row, and the first row chooses no hat.
        assert_eq!(style_row(0, -1), 0);
        assert_eq!(row_style(0, 0), -1);
        assert_eq!(row_style(0, 2), 1);
        assert_eq!(style_row(1, 1), 1);
        assert_eq!(row_style(1, 1), 1);
    }

    #[test]
    fn a_skill_that_cannot_be_specialized_offers_no_step_up_and_shows_no_999() {
        use SkillAdvancementClass as L;
        let steps = skill_steps(L::Trained, 6, 6 + UNBUYABLE, 5000);
        assert!(!steps.raise_on, "999 or more cannot be bought");
        assert!(steps.lower_on);
        let steps = skill_steps(L::Trained, 6, 12, 50);
        assert!(steps.raise_on);
        assert_eq!(steps.raise, Some((L::Specialized, 6)));
        assert!(
            !skill_steps(L::Untrained, 8, 16, 4).raise_on,
            "more than the credits left"
        );
        assert!(
            !skill_steps(L::Trained, 0, 4, 50).lower_on,
            "granted by the heritage"
        );
    }

    #[test]
    fn the_skills_list_has_a_heading_over_each_group() {
        use SkillAdvancementClass as L;
        let lines = skill_lines(&[L::Specialized, L::Trained, L::Trained, L::Untrained]);
        assert_eq!(
            lines,
            [
                SkillLine::Heading("Specialized"),
                SkillLine::Skill(0),
                SkillLine::Heading("Trained"),
                SkillLine::Skill(1),
                SkillLine::Skill(2),
                SkillLine::Heading("Untrained"),
                SkillLine::Skill(3),
            ]
        );
        assert_eq!(
            skill_lines(&[L::Untrained]),
            [SkillLine::Heading("Untrained"), SkillLine::Skill(0)],
            "a group with no skills has no heading"
        );
    }

    #[test]
    fn the_model_is_unclothed_until_the_clothes_are_come_to() {
        let mut c = creation();
        assert!(!c.dressed(), "the body is chosen unclothed");
        c.step = Step::Appearance;
        assert!(!c.dressed());
        c.step = Step::Profession;
        assert!(c.dressed(), "a step after the clothes shows them");
        c.step = Step::Sex;
        assert!(!c.dressed(), "the clothes not yet seen");
        c.attire_seen = true;
        assert!(c.dressed(), "once seen, worn on every step");
    }

    #[test]
    fn the_model_is_shown_close_while_the_face_is_chosen() {
        let mut c = creation();
        assert!(!c.close_up());
        c.step = Step::Appearance;
        assert!(c.close_up());
        c.step = Step::Attire;
        assert!(!c.close_up());
    }

    #[test]
    fn the_first_character_is_rolled_from_the_client_s_start_up_seeds() {
        let next = |c: &Creation| c.state.rng.crt.clone().next_u16();
        let unseeded = creation();
        let mut c = creation();
        c.seed(Some((7, 99)));
        let mut reference = CharGenState::with_policy(CreationPolicy::Modern);
        reference.rng = dereth_chargen::CharGenRng::new(7, 99);
        reference.begin_creation(&c.tables, CreationEntry::Normal, false);
        assert_eq!(next(&c), reference.rng.crt.clone().next_u16());
        assert_ne!(next(&c), next(&unseeded), "the seeds took");
        // Once only: the player's choices are not rolled over again.
        let before = next(&c);
        c.seed(Some((8, 100)));
        assert_eq!(next(&c), before);
    }
}
