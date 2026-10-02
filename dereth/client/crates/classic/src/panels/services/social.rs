use super::*;
use crate::int::i32_from;
use dereth_client_contract::view::AllegianceAction;

pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    Some(Box::new(Social {
        tab: match id {
            "fellowship" => 1,
            "trade-intro" => 2,
            _ => 0,
        },
        name: "Enter Fellowship Name".into(),
        show_xp: false,
        selected: None,
        mode: Mode::None,
        subscribed: None,
        height: 400,
        member_scroll: 0,
        error: None,
    }))
}
#[derive(Debug, PartialEq)]
enum Mode {
    None,
    Break,
    Dismiss,
    Leader,
    Recruit,
}
#[derive(Debug)]
struct Social {
    tab: usize,
    name: String,
    show_xp: bool,
    selected: Option<usize>,
    mode: Mode,
    subscribed: Option<usize>,
    height: u32,
    member_scroll: i32,
    error: Option<String>,
}
fn swear_cost(level_span: u64, breaks: u32) -> u32 {
    // The base is clamped before the quarter-per-break penalty is applied.
    let base = (level_span as f64 * f64::from(0.05_f32)).clamp(100.0, 5000.0);
    u32::try_from(dereth_primitives::num::to_i64_f64(
        base * (1.0 + 0.25 * f64::from(breaks)) + 0.5,
    ))
    .unwrap_or(u32::MAX)
}
fn cost(c: &Context<'_>) -> u32 {
    c.game.experience_header().map_or(0, |xp| {
        let breaks = c
            .game
            .player()
            .and_then(|p| c.game.int_stat(p, 0x84))
            .unwrap_or(0) as u32;
        swear_cost(xp.level_span, breaks)
    })
}
fn swear_target(c: &Context<'_>) -> Option<ObjectId> {
    let roster = c.game.allegiance_roster();
    if roster.patron.is_some() || c.game.available_experience().max(0) < i64::from(cost(c)) {
        return None;
    }
    c.game.selected_object().filter(|id| {
        Some(*id) != c.game.player()
            && !roster.vassals.iter().any(|v| v.id == *id)
            && c.game
                .selection_query_facts(*id)
                .is_some_and(|f| f.is_player)
    })
}
fn comma(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (i, c) in digits.chars().enumerate() {
        if i != 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}
impl Social {
    fn world_target(&mut self, target: Option<ObjectId>, c: &Context<'_>) -> Vec<PanelAction> {
        let Some(target) = target else {
            self.mode = Mode::None;
            return vec![];
        };
        let mode = std::mem::replace(&mut self.mode, Mode::None);
        match mode {
            Mode::Break => {
                let a = c.game.allegiance_roster();
                let action = if a.patron.as_ref().is_some_and(|p| p.id == target) {
                    Some(AllegianceAction::Break)
                } else if a.vassals.iter().any(|v| v.id == target) {
                    Some(AllegianceAction::Kick)
                } else {
                    None
                };
                action
                    .map(|action| {
                        vec![PanelAction::Host(HostAction::AllegianceSend {
                            action,
                            target,
                        })]
                    })
                    .unwrap_or_default()
            }
            Mode::Recruit => {
                if Some(target) == c.game.player() {
                    return vec![PanelAction::Host(HostAction::LocalFeedback {
                        severity: crate::panels::FeedbackSeverity::Warning,
                        text: "You can't recruit yourself.".into(),
                    })];
                }
                if c.game
                    .fellowship()
                    .is_some_and(|p| p.members.iter().any(|m| m.id == target))
                {
                    return vec![PanelAction::Host(HostAction::LocalFeedback {
                        severity: crate::panels::FeedbackSeverity::Warning,
                        text: format!(
                            "{} is already in your Fellowship",
                            c.game.name(target).unwrap_or("")
                        ),
                    })];
                }
                if c.game
                    .selection_query_facts(target)
                    .is_some_and(|f| f.is_player)
                {
                    request(UiRequest::FellowshipRecruit { target })
                } else {
                    vec![]
                }
            }
            Mode::Dismiss | Mode::Leader => {
                if let Some(index) = c
                    .game
                    .fellowship()
                    .and_then(|p| p.members.iter().position(|m| m.id == target))
                {
                    self.mode = mode;
                    self.choose_member(index, c)
                } else {
                    vec![PanelAction::Host(HostAction::LocalFeedback {
                        severity: crate::panels::FeedbackSeverity::Warning,
                        text: if mode == Mode::Leader {
                            "That person is not in the fellowship.".into()
                        } else {
                            format!(
                                "{} isn't in your Fellowship",
                                c.game.name(target).unwrap_or("")
                            )
                        },
                    })]
                }
            }
            Mode::None => vec![],
        }
    }
    fn allegiance(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = tiled(300, self.height.max(375), "06001421");
        // Stretched, the vassal list grows and the option and buttons keep to the bottom.
        let hs = i32::try_from(self.height).unwrap_or(362) - 25;
        let a = c.game.allegiance_roster();
        for y in [44, 88, 132] {
            f.image("06001420", rect(0, y, 300, 9), false, false);
        }
        label(
            &mut f,
            rect(4, 7, 292, 20),
            // A player in no allegiance is shown by name, with no followers.
            a.subject
                .as_ref()
                .map(|s| s.full_name.clone())
                .or_else(|| c.game.character_name().map(str::to_owned))
                .unwrap_or_else(|| "Please Wait...".into()),
            "16-7",
        );
        label(
            &mut f,
            rect(4, 25, 292, 20),
            format!("Followers: {}", a.total_vassals),
            "15-6",
        );
        if let Some(monarch) = &a.monarch {
            label(&mut f, rect(4, 51, 146, 20), "MONARCH", "15-6");
            label(&mut f, rect(4, 68, 194, 20), &monarch.full_name, "15-6");
            label(
                &mut f,
                rect(150, 51, 142, 20),
                format!("Followers: {}", a.total_members),
                "15-6",
            );
        }
        if let Some(patron) = &a.patron {
            label(&mut f, rect(4, 95, 146, 20), "PATRON", "15-6");
            f.button("patron", rect(4, 112, 194, 20), &patron.full_name, true)
                .images = Some(["0600141F"; 3].map(String::from));
            label(&mut f, rect(150, 95, 142, 20), "xp produced", "15-6");
            label(
                &mut f,
                rect(198, 112, 93, 20),
                a.own_cp_tithed.to_string(),
                "15-6",
            );
        } else {
            let cost = cost(c);
            let available = c.game.available_experience().max(0) as u64;
            if available < u64::from(cost) {
                let attainable = c
                    .game
                    .experience_header()
                    .is_some_and(|xp| u64::from(cost) < xp.to_level);
                centered(
                    &mut f,
                    rect(4, 56, 292, 34),
                    if attainable {
                        "You don't have enough available xp to swear Allegiance."
                    } else {
                        "You don't have enough available xp to swear Allegiance at your current level."
                    },
                    "15-6",
                );
                if attainable {
                    label(
                        &mut f,
                        rect(4, 90, 146, 20),
                        format!("xp available: {}", comma(available)),
                        "15-6",
                    );
                    f.text_box(
                        rect(152, 92, 138, 16),
                        format!("xp needed: {}", comma(u64::from(cost) - available)),
                        "15-6",
                        INK,
                        TextAlign::Right,
                        false,
                        None,
                    );
                } else {
                    centered(
                        &mut f,
                        rect(4, 90, 292, 20),
                        "Check again when your level increases.",
                        "15-6",
                    );
                }
            } else {
                if let Some(target) = swear_target(c) {
                    social_button(
                        &mut f,
                        "swear",
                        rect(4, 58, 292, 36),
                        format!("Swear Allegiance To {}", c.game.name(target).unwrap_or("")),
                        true,
                    );
                } else {
                    centered(
                        &mut f,
                        rect(4, 56, 292, 34),
                        "SELECT A CHARACTER TO SWEAR ALLEGIANCE TO.",
                        "15-6",
                    );
                }
                centered(
                    &mut f,
                    rect(4, 90, 292, 20),
                    format!("xp cost: {}", comma(u64::from(cost))),
                    "15-6",
                );
            }
        }
        label(&mut f, rect(4, 138, 142, 18), "VASSALS", "15-6");
        label(
            &mut f,
            rect(150, 138, 121, 18),
            if self.show_xp { "xp produced" } else { "Rank" },
            "15-6",
        );
        f.list(
            "vassals",
            rect(0, 156, 279, (hs - 252).max(123)),
            a.vassals
                .iter()
                .map(|v| {
                    format!(
                        "{}    {}",
                        v.full_name,
                        if self.show_xp {
                            v.cp_cached.to_string()
                        } else {
                            v.rank.to_string()
                        }
                    )
                    .into()
                })
                .collect(),
            self.selected,
            22,
        );
        f.check(
            "accept-allegiance",
            rect(25, hs - 56, 270, 13),
            "Accept Allegiance Requests",
            !c.game.player_option(PlayerOption::IgnoreAllegianceRequests),
            true,
        );
        social_button(
            &mut f,
            "show-xp",
            rect(25, hs - 36, 120, 36),
            if self.show_xp {
                "Show Titles"
            } else {
                "Show XP"
            },
            true,
        );
        social_button(
            &mut f,
            "break",
            rect(150, hs - 36, 120, 36),
            // Two lines, as the caption wraps on the button.
            "Break\nAllegiance",
            a.patron.is_some() || !a.vassals.is_empty(),
        );
        f
    }
    fn fellowship(&self, c: &Context<'_>) -> PanelFrame {
        let h = (self.height as i32 - 25).max(138);
        let mut f = tiled(300, h as u32, "06001421");
        if let Some(p) = c.game.fellowship() {
            centered(&mut f, rect(14, 2, 272, 20), &p.name, "16-7");
            label(&mut f, rect(4, 22, 125, 16), "MEMBERS", "15-6");
            label(&mut f, rect(131, 22, 141, 16), "LVL/XP%", "15-6");
            let list_height = h - 103;
            let clip = [0, 38, 279, 38 + list_height];
            f.control(
                "members",
                rect(0, 38, 279, list_height),
                ControlKind::HitList {
                    row_count: p.members.len(),
                    row_height: 32,
                    selected: self.selected,
                    offset: self.member_scroll,
                },
                true,
            )
            .paint = false;
            f.control(
                "members-scroll",
                rect(279, 38, 16, list_height),
                ControlKind::ScrollBar {
                    min: 0,
                    max: (i32_from(p.members.len()) * 32 - list_height).max(0),
                    value: self.member_scroll,
                    page: list_height,
                    step: 32,
                    vertical: true,
                    arrow_size: 16,
                    thumb_size: 16,
                },
                true,
            );
            for (i, m) in p.members.iter().enumerate() {
                let y = 38 + i32_from(i) * 32 - self.member_scroll;
                if y + 32 <= 38 || y >= 38 + list_height {
                    continue;
                }
                let image = |f: &mut PanelFrame, did: &str, r: Rect| {
                    f.image(did, r, true, false);
                    if let Some(Command::Image { clip: c, .. }) = f.screen.commands.last_mut() {
                        *c = Some(clip);
                    }
                };
                image(
                    &mut f,
                    if self.selected == Some(i) {
                        "06001451"
                    } else {
                        "06001450"
                    },
                    rect(0, y, 279, 16),
                );
                f.text_box(
                    rect(7, y + 2, 193, 12),
                    &m.name,
                    "15-6",
                    INK,
                    TextAlign::Left,
                    false,
                    Some(clip),
                );
                f.text_box(
                    rect(204, y + 2, 70, 12),
                    format!("{}/{}%", m.level, m.xp_percent),
                    "15-6",
                    INK,
                    TextAlign::Right,
                    false,
                    Some(clip),
                );
                // Each vital bar is its empty frame, then the full bar drawn at its own size and
                // cut to the vital's fraction.
                for (j, now, max, back, front) in [
                    (0, m.current_health, m.max_health, "0600251D", "0600251C"),
                    (1, m.current_stamina, m.max_stamina, "06002521", "06002520"),
                    (2, m.current_mana, m.max_mana, "0600251F", "0600251E"),
                ] {
                    let x = j * 93;
                    image(&mut f, back, rect(x, y + 16, 93, 16));
                    let width = if max == 0 {
                        0
                    } else {
                        i32::try_from(u64::from(now.min(max)) * 93 / u64::from(max)).unwrap_or(93)
                    };
                    if width > 0 {
                        f.image_native(
                            front,
                            x,
                            y + 16,
                            rect(x, y + 16, width, 16)
                                .intersect(rect(
                                    clip[0],
                                    clip[1],
                                    clip[2] - clip[0],
                                    clip[3] - clip[1],
                                ))
                                .unwrap_or_default(),
                            false,
                        );
                    }
                    f.text_box(
                        rect(x + 2, y + 18, 89, 12),
                        format!("{now}/{max}"),
                        "15-6",
                        INK,
                        TextAlign::Left,
                        false,
                        Some(clip),
                    );
                }
            }
            let leader = Some(p.leader) == c.game.player();
            for (id, text, x, y, enabled) in [
                (
                    "recruit",
                    "Recruit",
                    15,
                    h - 29,
                    !p.locked && (leader || p.open_fellow),
                ),
                ("dismiss", "Dismiss", 105, h - 29, p.members.len() > 1),
                ("disband", "Disband", 195, h - 29, true),
                ("leader", "Leader", 15, h - 61, p.members.len() > 1),
                ("quit", "Quit", 105, h - 61, true),
                (
                    "open",
                    if p.open_fellow { "Close" } else { "Open" },
                    195,
                    h - 61,
                    true,
                ),
            ] {
                if leader || id == "recruit" {
                    social_button(&mut f, id, rect(x, y, 85, 27), text, enabled);
                }
            }
            if !leader {
                social_button(
                    &mut f,
                    "quit",
                    rect(105, h - 29, 170, 27),
                    "Quit Fellowship",
                    true,
                );
            }
        } else {
            f.image("06001420", rect(0, h - 138, 300, 9), false, false);
            label(&mut f,rect(14,17,290,100),"You do not belong to a Fellowship.\nTo create a fellowship, enter a name in the box below, then click Create Fellowship.\nOnce you've created a Fellowship, you can start recruiting members.","15-6");
            if let Some(text) = &self.error {
                centered(&mut f, rect(4, 144, 292, 48), text, "15-6");
            }
            if c.game.player_option(PlayerOption::IgnoreFellowshipRequests) {
                label(&mut f,rect(14,97,290,80),"You are currently ignoring Fellowship requests. To let yourself be recruited, check Accept Fellowship Requests.","15-6");
            }
            label(
                &mut f,
                rect(14, h - 132, 134, 20),
                "FELLOWSHIP NAME:",
                "15-6",
            );
            // Clicking the name selects it, so typing replaces the suggested one.
            f.edit(
                "name",
                rect(148, h - 132, 128, 20),
                &self.name,
                32,
                false,
                true,
            )
            .select_on_focus = true;
            social_button(
                &mut f,
                "create",
                rect(30, h - 29, 240, 27),
                "Create Fellowship",
                true,
            );
        }
        if c.game.fellowship().is_none() {
            for (id, title, y, p, invert) in [
                (
                    "accept-fellow",
                    "Accept Fellowship Requests",
                    h - 112,
                    PlayerOption::IgnoreFellowshipRequests,
                    true,
                ),
                (
                    "share-xp",
                    "Share Fellowship Experience",
                    h - 94,
                    PlayerOption::FellowshipShareXP,
                    false,
                ),
                (
                    "share-loot",
                    "Share Fellowship Treasure",
                    h - 76,
                    PlayerOption::FellowshipShareLoot,
                    false,
                ),
                (
                    "auto-fellow",
                    "Auto-Accept Fellowship Requests",
                    h - 58,
                    PlayerOption::FellowshipAutoAcceptRequests,
                    false,
                ),
            ] {
                f.check(
                    id,
                    rect(25, y, 270, 13),
                    title,
                    c.game.player_option(p) ^ invert,
                    true,
                );
            }
        }
        f
    }
    fn trade(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = tiled(300, self.height.max(375), "06001421");
        f.image("06001420", rect(0, 184, 300, 9), false, false);
        let target = c.game.selected_object().filter(|id| {
            Some(*id) != c.game.player()
                && c.game
                    .selection_query_facts(*id)
                    .is_some_and(|f| f.is_player)
        });
        if c.game.trade().open {
            social_button(
                &mut f,
                "trade",
                rect(10, 40, 282, 36),
                format!("Exit trade with {}", c.game.trade().partner_name),
                true,
            );
        } else if let Some(target) = target {
            social_button(
                &mut f,
                "trade",
                rect(10, 40, 282, 36),
                format!("Trade securely with  {}", c.game.name(target).unwrap_or("")),
                true,
            );
        } else {
            centered(
                &mut f,
                rect(10, 40, 292, 60),
                "Select the person with whom to initiate Secure Trade",
                "16-7",
            );
        }
        centered(&mut f,rect(10,90,272,100),"Both of you must be in peace mode to start trade. Drag items into the Trade Panel to offer them. To remove an offered item, you must clear the Secure Trade Panel.","16-7");
        f.check(
            "ignore-trade",
            rect(50, 220, 240, 13),
            "Ignore All Trade Requests",
            c.game.player_option(PlayerOption::IgnoreTradeRequests),
            true,
        )
        .font = "16-7".into();
        centered(&mut f,rect(10,240,272,60),"Note: Use Squelch to automatically stop a specific person from starting trade with you.","16-7");
        f
    }
    fn subscription(&mut self) -> Vec<PanelAction> {
        if self.subscribed == Some(self.tab) {
            return vec![];
        }
        let mut a = vec![];
        if let Some(t) = self.subscribed {
            if t < 2 {
                a.push(PanelAction::Game(if t == 0 {
                    UiRequest::AllegianceUpdateRequest { on: false }
                } else {
                    UiRequest::FellowshipUpdateRequest { on: false }
                }));
            }
        }
        self.subscribed = Some(self.tab);
        if self.tab < 2 {
            a.push(PanelAction::Game(if self.tab == 0 {
                UiRequest::AllegianceUpdateRequest { on: true }
            } else {
                UiRequest::FellowshipUpdateRequest { on: true }
            }));
        }
        a
    }
    fn choose_member(&mut self, index: usize, c: &Context<'_>) -> Vec<PanelAction> {
        self.selected = Some(index);
        let Some(p) = c.game.fellowship() else {
            return vec![];
        };
        let Some(m) = p.members.get(index) else {
            return vec![];
        };
        let mode = std::mem::replace(&mut self.mode, Mode::None);
        if Some(m.id) == c.game.player() && matches!(mode, Mode::Dismiss | Mode::Leader) {
            return vec![PanelAction::Host(HostAction::LocalFeedback {
                severity: crate::panels::FeedbackSeverity::Warning,
                text: if mode == Mode::Dismiss {
                    "You can't dismiss yourself."
                } else {
                    "You are already the leader."
                }
                .into(),
            })];
        }
        match mode {
            Mode::Dismiss => request(UiRequest::FellowshipDismiss { target: m.id }),
            Mode::Leader => request(UiRequest::FellowshipAssignNewLeader { target: m.id }),
            _ => request(UiRequest::Select(m.id)),
        }
    }
}
impl Panel for Social {
    fn resize(&mut self, _: u32, height: u32) {
        self.height = height.max(163);
    }
    fn id(&self) -> &'static str {
        "social"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = translated(
            match self.tab {
                0 => self.allegiance(c),
                1 => self.fellowship(c),
                _ => self.trade(c),
            },
            25,
            self.height,
        );
        for (i, title) in ["Allegiance", "Fellowship", "Trade"].iter().enumerate() {
            // The tabs' art, drawn as it is: the tab of the page on show is held down, so it
            // shows the pressed art.
            let tab = f.button(
                format!("tab{i}"),
                rect(i32_from(i) * 92, 0, 92, 25),
                *title,
                true,
            );
            tab.images = Some(
                [
                    if i == self.tab {
                        "06001454"
                    } else {
                        "06001455"
                    },
                    "06001454",
                    "06001455",
                ]
                .map(String::from),
            );
            tab.keyed = false;
        }
        image_button(
            &mut f,
            "close",
            rect(276, 0, 24, 25),
            [0x0600141D, 0x0600141E, 0x0600141D],
            true,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        let targeting = self.mode != Mode::None;
        let mut out = (|| match e {
            ControlEvent::WorldTarget(target) => self.world_target(target, c),
            ControlEvent::Scroll { id, value } if id == "members-scroll" => {
                self.member_scroll = value.max(0);
                vec![]
            }
            ControlEvent::Tick => self.subscription(),
            ControlEvent::Edit { id, text } if id == "name" => {
                self.name = text;
                vec![]
            }
            ControlEvent::Check { id, checked } => {
                let (p, invert) = match id.as_str() {
                    "accept-allegiance" => (PlayerOption::IgnoreAllegianceRequests, true),
                    "accept-fellow" => (PlayerOption::IgnoreFellowshipRequests, true),
                    "share-xp" => (PlayerOption::FellowshipShareXP, false),
                    "share-loot" => (PlayerOption::FellowshipShareLoot, false),
                    "auto-fellow" => (PlayerOption::FellowshipAutoAcceptRequests, false),
                    "ignore-trade" => (PlayerOption::IgnoreTradeRequests, false),
                    _ => return vec![],
                };
                changed_option(p, checked ^ invert)
            }
            ControlEvent::Select { id, index } if id == "members" => self.choose_member(index, c),
            ControlEvent::Select { id, index } if id == "vassals" => {
                self.selected = Some(index);
                if self.mode == Mode::Break {
                    self.mode = Mode::None;
                    if let Some(v) = c.game.allegiance_roster().vassals.get(index) {
                        return vec![PanelAction::Host(HostAction::AllegianceSend {
                            action: AllegianceAction::Kick,
                            target: v.id,
                        })];
                    }
                }
                vec![]
            }
            ControlEvent::Activate(id) => match id.as_str() {
                "tab0" | "tab1" | "tab2" => {
                    self.tab = id.as_bytes()[3] as usize - b'0' as usize;
                    self.selected = None;
                    self.mode = Mode::None;
                    self.subscription()
                }
                "close" => {
                    self.mode = Mode::None;
                    let old = self.tab;
                    self.tab = 3;
                    let mut a = self.subscription();
                    self.tab = old;
                    a.push(PanelAction::Close);
                    a
                }
                "show-xp" => {
                    self.show_xp = !self.show_xp;
                    vec![]
                }
                "break" => {
                    self.mode = Mode::Break;
                    vec![PanelAction::Host(HostAction::SocialTarget(6)),PanelAction::Host(HostAction::LocalFeedback { severity: crate::panels::FeedbackSeverity::Information, text: "Click your Patron or a Vassal to break Allegiance. Click anywhere to abort.".into() })]
                }
                "patron" if self.mode == Mode::Break => {
                    self.mode = Mode::None;
                    c.game
                        .allegiance_roster()
                        .patron
                        .map(|p| {
                            vec![PanelAction::Host(HostAction::AllegianceSend {
                                action: AllegianceAction::Break,
                                target: p.id,
                            })]
                        })
                        .unwrap_or_default()
                }
                "swear" => swear_target(c)
                    .map(|target| {
                        vec![PanelAction::Host(HostAction::AllegianceSend {
                            action: AllegianceAction::Swear,
                            target,
                        })]
                    })
                    .unwrap_or_default(),
                "trade" if c.game.trade().open => request(UiRequest::TradeClose),
                "trade" => c
                    .game
                    .selected_object()
                    .map(|target| vec![PanelAction::Host(HostAction::OpenTrade(target))])
                    .unwrap_or_default(),
                "create" => {
                    self.error = if self.name.is_empty() {
                        Some("You must enter a name for your fellowship.".into())
                    } else if self.name.eq_ignore_ascii_case("Enter Fellowship Name") {
                        Some("'Enter Fellowship Name' is not an available name. Please choose a different name for your fellowship.".into())
                    } else {
                        None
                    };
                    if self.error.is_some() {
                        return vec![];
                    }
                    self.name = crate::panels::pregame::format_name(&self.name);
                    request(UiRequest::FellowshipCreate {
                        name: self.name.clone(),
                        share_xp: c.game.player_option(PlayerOption::FellowshipShareXP),
                    })
                }
                "recruit" => {
                    if let Some(target) = c.game.selected_object().filter(|id| {
                        c.game
                            .selection_query_facts(*id)
                            .is_some_and(|f| f.is_player)
                    }) {
                        self.mode = Mode::None;
                        if Some(target) == c.game.player() {
                            return vec![PanelAction::Host(HostAction::LocalFeedback {
                                severity: crate::panels::FeedbackSeverity::Warning,
                                text: "You can't recruit yourself.".into(),
                            })];
                        }
                        if c.game
                            .fellowship()
                            .is_some_and(|f| f.members.iter().any(|m| m.id == target))
                        {
                            return vec![PanelAction::Host(HostAction::LocalFeedback {
                                severity: crate::panels::FeedbackSeverity::Warning,
                                text: format!(
                                    "{} is already in your Fellowship",
                                    c.game.name(target).unwrap_or("")
                                ),
                            })];
                        }
                        return vec![
                            PanelAction::Game(UiRequest::DisplayChatText {
                                channel: 0,
                                text: "Waiting for response ...\n".into(),
                            }),
                            PanelAction::Game(UiRequest::FellowshipRecruit { target }),
                        ];
                    }
                    self.mode = Mode::Recruit;
                    vec![
                        PanelAction::Host(HostAction::SocialTarget(7)),
                        PanelAction::Host(HostAction::LocalFeedback {
                            severity: crate::panels::FeedbackSeverity::Information,
                            text: "Click a character to recruit.".into(),
                        }),
                    ]
                }
                "dismiss" => {
                    self.mode = Mode::Dismiss;
                    vec![
                        PanelAction::Host(HostAction::SocialTarget(8)),
                        PanelAction::Host(HostAction::LocalFeedback {
                            severity: crate::panels::FeedbackSeverity::Information,
                            text: "Click a character name to dismiss.".into(),
                        }),
                    ]
                }
                "leader" => {
                    self.mode = Mode::Leader;
                    vec![
                        PanelAction::Host(HostAction::SocialTarget(9)),
                        PanelAction::Host(HostAction::LocalFeedback {
                            severity: crate::panels::FeedbackSeverity::Information,
                            text: "Click a character name to assign leadership.".into(),
                        }),
                    ]
                }
                "disband" => request(UiRequest::FellowshipQuit { disband: true }),
                "quit" => {
                    let mut out = vec![];
                    if let Some(p) = c
                        .game
                        .fellowship()
                        .filter(|p| Some(p.leader) == c.game.player())
                    {
                        if let Some(m) = p.members.iter().find(|m| m.id != p.leader) {
                            out.push(PanelAction::Game(UiRequest::FellowshipAssignNewLeader {
                                target: m.id,
                            }));
                        }
                    }
                    out.push(PanelAction::Game(UiRequest::FellowshipQuit {
                        disband: false,
                    }));
                    out
                }
                "open" => request(UiRequest::FellowshipToggleOpenness),
                _ => vec![],
            },
            _ => vec![],
        })();
        if targeting && self.mode == Mode::None {
            out.insert(0, PanelAction::Host(HostAction::SocialTarget(0)));
        }
        out
    }
}
#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn allegiance_cost_clamps_before_break_penalty_and_rounds() {
        assert_eq!(swear_cost(0, 0), 100);
        assert_eq!(swear_cost(20000, 0), 1000);
        assert_eq!(swear_cost(20000, 1), 1250);
        assert_eq!(swear_cost(1_000_000, 4), 10000);
        assert_eq!(swear_cost(2010, 0), 101);
        assert_eq!(comma(1234567), "1,234,567");
    }
}
