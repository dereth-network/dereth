use super::*;
use crate::int::i32_from;
use dereth_client_contract::view::{AllegianceAction, AllegianceEntry, AllegianceRoster};

pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    Some(Box::new(Social {
        tab: match id {
            "fellowship" => 1,
            "trade-intro" => 2,
            "friends" => 3,
            "squelch" => 4,
            _ => 0,
        },
        name: "Enter Fellowship Name".into(),
        show_xp: false,
        selected: None,
        mode: Mode::None,
        subscribed: None,
        height: 400,
        member_scroll: 0,
        vassal_scroll: 0,
        error: None,
        entry: String::new(),
        list_scroll: 0,
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
    vassal_scroll: i32,
    error: Option<String>,
    /// The name typed on the Friends or Squelch page.
    entry: String,
    /// The Friends or Squelch page's list scroll.
    list_scroll: i32,
}

/// The window's pages as `(page, tab caption)`, in tab order: Allegiance and Fellowship always;
/// Secure Trade on a world with trade when the player shows it (off at first); Friends and
/// Squelch when the player shows them (on at first), on any world.
fn pages(game: &dyn GameView) -> Vec<(usize, &'static str)> {
    use dereth_client_contract::options::classic;
    let mut pages = vec![(0, "Allegiance"), (1, "Fellowship")];
    if game.era().is_none_or(|e| e.features().trade) && classic::shown(classic::SHOW_TRADE_TAB) {
        pages.push((2, "Trade"));
    }
    if classic::shown(classic::SHOW_FRIENDS_TAB) {
        pages.push((3, "Friends"));
    }
    if classic::shown(classic::SHOW_SQUELCH_TAB) {
        pages.push((4, "Squelch"));
    }
    pages
}

/// Whether the window shows `page`.
fn shows(game: &dyn GameView, page: usize) -> bool {
    pages(game).iter().any(|(p, _)| *p == page)
}

/// The friends in the page's order: those logged in first, each group by name.
fn friends_shown(game: &dyn GameView) -> Vec<dereth_client_contract::view::FriendEntry> {
    let mut friends = game.friends();
    friends.sort_by_key(|f| (!f.online, f.name.to_lowercase()));
    friends
}

/// The squelched characters by name.
fn squelches_shown(game: &dyn GameView) -> Vec<dereth_client_contract::view::SquelchEntry> {
    let mut squelches = game.squelch_list();
    squelches.sort_by_key(|q| q.name.to_lowercase());
    squelches
}
/// Whether the world charges experience for an oath; without it the panel shows no cost.
fn oath_costs_xp(game: &dyn GameView) -> bool {
    game.era().is_some_and(|e| e.features().swear_xp_cost)
}
/// What the world charges for an oath: nothing without the charge or before a first break.
fn cost(c: &Context<'_>) -> u32 {
    if !oath_costs_xp(c.game) {
        return 0;
    }
    c.game.experience_header().map_or(0, |xp| {
        let breaks = c
            .game
            .player()
            .and_then(|p| c.game.int_stat(p, 0x84))
            .unwrap_or(0) as u32;
        // Past the curve's end the next level counts as the most a count can hold.
        let span = if xp.level_span == 0 {
            u64::from(u32::MAX)
        } else {
            xp.level_span
        };
        dereth_rules::allegiance::swear_xp_cost_after_breaks(span, breaks)
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
/// The rank on the header line: the allegiance's own, or, while a spell raises it, the raised
/// rank and the difference ("5 (+1)").
fn rank_text(a: &AllegianceRoster) -> String {
    let tree = a.subject.as_ref().map_or(0, |s| i32::from(s.rank));
    let quality = a.player_rank_quality;
    if quality <= 0 || quality == tree {
        tree.to_string()
    } else {
        format!("{quality} (+{})", quality - tree)
    }
}
/// A member's name colour: the panel's ink while logged in, grey while not.
fn member_color(m: &AllegianceEntry) -> u32 {
    if m.logged_in {
        INK
    } else {
        OFFLINE
    }
}
const OFFLINE: u32 = 0xff96_9696;
/// The name without its rank title (the full name is the title, a space, then the name).
fn bare_name(full_name: &str) -> &str {
    full_name
        .split_once(' ')
        .map_or(full_name, |(_, name)| name)
}
fn right_label(f: &mut PanelFrame, r: Rect, text: impl Into<String>) {
    f.text_box(
        rect(r.x + 2, r.y, (r.w - 4).max(0), r.h),
        text,
        "15-6",
        INK,
        TextAlign::Right,
        false,
        Some([r.x, r.y, r.x + r.w, r.y + r.h]),
    );
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
        // A monarch above a patron of their own takes two sections; a patron who is the monarch
        // (or a player with no patron) takes one, and the vassals' heading moves up under it.
        let both = a
            .monarch
            .as_ref()
            .zip(a.patron.as_ref())
            .is_some_and(|(m, p)| m.id != p.id);
        let rules: &[i32] = if both { &[44, 88, 132] } else { &[44, 108] };
        for y in rules {
            f.image("06001420", rect(0, *y, 300, 9), false, false);
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
            format!(
                "Followers: {}    Rank: {}",
                comma(u64::from(a.total_vassals)),
                rank_text(&a)
            ),
            "15-6",
        );
        if let Some(patron) = &a.patron {
            let head = |f: &mut PanelFrame, y: i32, title: &str, who: &AllegianceEntry| {
                label(f, rect(4, y, 146, 20), title, "15-6");
                label_color(
                    f,
                    rect(4, y + 17, 194, 20),
                    &who.full_name,
                    "15-6",
                    member_color(who),
                );
            };
            let right = |f: &mut PanelFrame, y: i32, title: &str, value: String| {
                right_label(f, rect(150, y, 142, 20), title);
                right_label(f, rect(198, y + 17, 93, 20), value);
            };
            let produced = |f: &mut PanelFrame, y: i32, title: &str| {
                if self.show_xp {
                    right(f, y, title, comma(u64::from(a.own_cp_tithed)));
                }
            };
            let patron_y = if both { 95 } else { 51 };
            if let Some(monarch) = a.monarch.as_ref().filter(|_| both) {
                head(&mut f, 51, "MONARCH", monarch);
                right(
                    &mut f,
                    51,
                    "Followers:",
                    comma(u64::from(a.total_members.saturating_sub(1))),
                );
                head(&mut f, 95, "PATRON", patron);
                produced(&mut f, 95, "xp produced");
            } else {
                head(&mut f, 51, "PATRON/MONARCH", patron);
                produced(&mut f, 51, "xp produced:");
            }
            // The name is also where a click picks the patron to break from.
            f.button("patron", rect(4, patron_y + 17, 194, 20), "", true)
                .paint = false;
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
                // A player with no patron may swear, followers or not: a monarch could swear to
                // another and bring the allegiance under them.
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
                // The cost line is the era's: a world that charges nothing for an oath has none.
                if oath_costs_xp(c.game) {
                    centered(
                        &mut f,
                        rect(4, 90, 292, 20),
                        format!("xp cost: {}", comma(u64::from(cost))),
                        "15-6",
                    );
                }
            }
        }
        let heading = if both { 138 } else { 114 };
        label(&mut f, rect(4, heading, 142, 18), "VASSALS", "15-6");
        // Titles mode names each vassal by title and name alone; the experience column is shown
        // only with Show XP.
        if self.show_xp {
            right_label(&mut f, rect(150, heading, 121, 18), "xp produced");
        }
        self.vassal_list(&mut f, &a, rect(0, 156, 279, (hs - 252).max(123)));
        f.check(
            "accept-allegiance",
            rect(25, hs - 56, 270, 13),
            "Accept Allegiance Requests",
            !c.game.player_option(PlayerOption::IgnoreAllegianceRequests),
            true,
        );
        let anyone = a.patron.is_some() || !a.vassals.is_empty();
        social_button(
            &mut f,
            "show-xp",
            rect(25, hs - 36, 120, 36),
            if self.show_xp {
                "Show Titles"
            } else {
                "Show XP"
            },
            anyone,
        );
        social_button(
            &mut f,
            "break",
            rect(150, hs - 36, 120, 36),
            // Two lines, as the caption wraps on the button.
            "Break\nAllegiance",
            anyone,
        );
        f
    }
    /// The vassals: one 20-pixel row each, two pixels apart, on the row art, a logged-out
    /// vassal in grey. "NONE" when the player has no followers.
    fn vassal_list(&self, f: &mut PanelFrame, a: &AllegianceRoster, r: Rect) {
        const PITCH: i32 = 22;
        let clip = [r.x, r.y, r.x + r.w, r.y + r.h];
        let max = (i32_from(a.vassals.len()) * PITCH - r.h).max(0);
        let offset = self.vassal_scroll.clamp(0, max);
        f.control(
            "vassals",
            r,
            ControlKind::HitList {
                row_count: a.vassals.len(),
                row_height: PITCH,
                selected: self.selected,
                offset,
            },
            true,
        )
        .paint = false;
        f.control(
            "vassals-scroll",
            rect(r.x + r.w, r.y, 21, r.h),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: offset,
                page: r.h,
                step: PITCH,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        if a.vassals.is_empty() {
            f.text_box(
                rect(r.x + 5, r.y, r.w - 26, 20),
                "NONE",
                "16-7",
                INK,
                TextAlign::Left,
                false,
                Some(clip),
            );
            return;
        }
        let half = r.w / 2;
        for (i, v) in a.vassals.iter().enumerate() {
            let y = r.y + i32_from(i) * PITCH - offset;
            if y + PITCH <= r.y || y >= r.y + r.h {
                continue;
            }
            let row = rect(r.x, y, r.w, 20);
            f.image_native(
                "0600141F",
                row.x,
                row.y,
                row.intersect(r).unwrap_or_default(),
                false,
            );
            let color = member_color(v);
            let mut text = |x: i32, w: i32, s: String, align: TextAlign| {
                f.text_box(
                    rect(r.x + x, y + 2, w, 16),
                    s,
                    "15-6",
                    color,
                    align,
                    false,
                    Some(clip),
                );
            };
            if self.show_xp {
                text(
                    5,
                    half - 5,
                    bare_name(&v.full_name).to_owned(),
                    TextAlign::Left,
                );
                text(
                    half,
                    half - 5,
                    comma(u64::from(v.cp_cached)),
                    TextAlign::Right,
                );
            } else {
                text(5, r.w - 5, v.full_name.clone(), TextAlign::Left);
            }
        }
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
        // While a negotiation is on (a cancelled one, its window still up, is over).
        if c.game.trade().partner.is_some() {
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
    /// A list of names on the row art, as the vassals are drawn: `(text, colour)` each.
    fn name_list(&self, f: &mut PanelFrame, id: &str, r: Rect, rows: &[(String, u32)]) {
        const PITCH: i32 = 22;
        let clip = [r.x, r.y, r.x + r.w, r.y + r.h];
        let max = (i32_from(rows.len()) * PITCH - r.h).max(0);
        let offset = self.list_scroll.clamp(0, max);
        f.control(
            id,
            r,
            ControlKind::HitList {
                row_count: rows.len(),
                row_height: PITCH,
                selected: self.selected,
                offset,
            },
            true,
        )
        .paint = false;
        f.control(
            format!("{id}-scroll"),
            rect(r.x + r.w, r.y, 21, r.h),
            ControlKind::ScrollBar {
                min: 0,
                max,
                value: offset,
                page: r.h,
                step: PITCH,
                vertical: true,
                arrow_size: 16,
                thumb_size: 16,
            },
            true,
        );
        for (i, (text, color)) in rows.iter().enumerate() {
            let y = r.y + i32_from(i) * PITCH - offset;
            if y + PITCH <= r.y || y >= r.y + r.h {
                continue;
            }
            let row = rect(r.x, y, r.w, 20);
            f.image_native(
                if self.selected == Some(i) {
                    "06001451"
                } else {
                    "0600141F"
                },
                row.x,
                row.y,
                row.intersect(r).unwrap_or_default(),
                false,
            );
            f.text_box(
                rect(r.x + 5, y + 2, r.w - 10, 16),
                text,
                "15-6",
                *color,
                TextAlign::Left,
                false,
                Some(clip),
            );
        }
    }
    /// The name box and its caption above a page's buttons.
    fn name_entry(&self, f: &mut PanelFrame, y: i32) {
        label(f, rect(14, y, 60, 20), "NAME:", "15-6");
        f.edit("entry", rect(74, y, 202, 20), &self.entry, 32, false, true)
            .select_on_focus = true;
    }
    fn friends(&self, c: &Context<'_>) -> PanelFrame {
        let h = (self.height as i32 - 25).max(200);
        let mut f = tiled(300, h as u32, "06001421");
        let friends = friends_shown(c.game);
        let online = friends.iter().filter(|f| f.online).count();
        label(
            &mut f,
            rect(4, 4, 292, 20),
            format!("Friends: {}    Online: {online}", friends.len()),
            "16-7",
        );
        f.image("06001420", rect(0, 24, 300, 9), false, false);
        let rows: Vec<_> = friends
            .iter()
            .map(|f| (f.name.clone(), if f.online { INK } else { OFFLINE }))
            .collect();
        self.name_list(&mut f, "friend-rows", rect(0, 36, 279, h - 136), &rows);
        if friends.is_empty() {
            label(&mut f, rect(4, 36, 275, 20), "NONE", "16-7");
        }
        self.name_entry(&mut f, h - 92);
        let chosen = self.selected.and_then(|i| friends.get(i));
        social_button(
            &mut f,
            "add-friend",
            rect(15, h - 64, 85, 27),
            "Add",
            !self.entry.trim().is_empty(),
        );
        social_button(
            &mut f,
            "remove-friend",
            rect(105, h - 64, 85, 27),
            "Remove",
            chosen.is_some(),
        );
        social_button(
            &mut f,
            "tell-friend",
            rect(195, h - 64, 85, 27),
            "Tell",
            chosen.is_some_and(|f| f.online),
        );
        // The character's own presence to its friends, set as the fellowship's options are.
        f.check(
            "appear-offline",
            rect(25, h - 26, 270, 13),
            "Appear Offline",
            c.game.player_option(PlayerOption::AppearOffline),
            true,
        );
        f
    }
    fn squelch(&self, c: &Context<'_>) -> PanelFrame {
        let h = (self.height as i32 - 25).max(200);
        let mut f = tiled(300, h as u32, "06001421");
        let squelches = squelches_shown(c.game);
        label(
            &mut f,
            rect(4, 4, 292, 20),
            format!("Squelched: {}", squelches.len()),
            "16-7",
        );
        f.image("06001420", rect(0, 24, 300, 9), false, false);
        let rows: Vec<_> = squelches
            .iter()
            .map(|q| {
                (
                    if q.account {
                        format!("{} (account)", q.name)
                    } else {
                        q.name.clone()
                    },
                    INK,
                )
            })
            .collect();
        self.name_list(&mut f, "squelch-rows", rect(0, 36, 279, h - 136), &rows);
        if squelches.is_empty() {
            label(&mut f, rect(4, 36, 275, 20), "NONE", "16-7");
        }
        self.name_entry(&mut f, h - 92);
        let typed = !self.entry.trim().is_empty();
        social_button(
            &mut f,
            "squelch-character",
            rect(15, h - 64, 130, 27),
            "Squelch",
            typed,
        );
        social_button(
            &mut f,
            "squelch-account",
            rect(150, h - 64, 130, 27),
            "Squelch Account",
            typed,
        );
        social_button(
            &mut f,
            "unsquelch",
            rect(15, h - 32, 265, 27),
            "Remove Squelch",
            self.selected.is_some_and(|i| i < squelches.len()),
        );
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
        let pages = pages(c.game);
        // A page the window does not show (opened by its key, or turned off) gives way to the
        // allegiance page.
        let shown = if shows(c.game, self.tab) { self.tab } else { 0 };
        let mut f = translated(
            match shown {
                1 => self.fellowship(c),
                2 => self.trade(c),
                3 => self.friends(c),
                4 => self.squelch(c),
                _ => self.allegiance(c),
            },
            25,
            self.height,
        );
        let width = 276 / i32_from(pages.len());
        for (i, (page, title)) in pages.iter().enumerate() {
            // The tabs' art, drawn as it is: the tab of the page on show is held down, so it
            // shows the pressed art. Four or five tabs share the row in a smaller hand.
            let tab = f.button(
                format!("tab{page}"),
                rect(i32_from(i) * width, 0, width, 25),
                *title,
                true,
            );
            if pages.len() > 3 {
                tab.font = "14-5".into();
            }
            tab.images = Some(
                [
                    if *page == shown {
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
        if !shows(c.game, self.tab) {
            self.tab = 0;
        }
        let targeting = self.mode != Mode::None;
        let mut out = (|| match e {
            ControlEvent::WorldTarget(target) => self.world_target(target, c),
            ControlEvent::Scroll { id, value } if id == "members-scroll" => {
                self.member_scroll = value.max(0);
                vec![]
            }
            ControlEvent::Scroll { id, value } if id == "vassals-scroll" || id == "vassals" => {
                self.vassal_scroll = value.max(0);
                vec![]
            }
            ControlEvent::Tick => self.subscription(),
            ControlEvent::Edit { id, text } if id == "name" => {
                self.name = text;
                vec![]
            }
            ControlEvent::Edit { id, text } if id == "entry" => {
                self.entry = text;
                vec![]
            }
            ControlEvent::Scroll { id, value }
                if id.starts_with("friend-rows") || id.starts_with("squelch-rows") =>
            {
                self.list_scroll = value.max(0);
                vec![]
            }
            ControlEvent::Select { id, index } if id == "friend-rows" || id == "squelch-rows" => {
                self.selected = Some(index);
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
                    "appear-offline" => (PlayerOption::AppearOffline, false),
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
                "tab0" | "tab1" | "tab2" | "tab3" | "tab4" => {
                    let tab = id.as_bytes()[3] as usize - b'0' as usize;
                    if !shows(c.game, tab) {
                        return vec![];
                    }
                    self.tab = tab;
                    self.selected = None;
                    self.list_scroll = 0;
                    self.mode = Mode::None;
                    self.subscription()
                }
                // The Friends and Squelch pages' buttons, while the window shows the page.
                "add-friend" | "remove-friend" | "tell-friend" if !shows(c.game, 3) => vec![],
                "squelch-character" | "squelch-account" | "unsquelch" if !shows(c.game, 4) => {
                    vec![]
                }
                "add-friend" => {
                    let name = std::mem::take(&mut self.entry).trim().to_owned();
                    if name.is_empty() {
                        return vec![];
                    }
                    request(UiRequest::AddFriend { name })
                }
                "remove-friend" => self
                    .selected
                    .and_then(|i| friends_shown(c.game).get(i).map(|f| f.id))
                    .map(|target| {
                        self.selected = None;
                        request(UiRequest::RemoveFriend { target })
                    })
                    .unwrap_or_default(),
                "tell-friend" => self
                    .selected
                    .and_then(|i| friends_shown(c.game).get(i).cloned())
                    .map(|f| vec![PanelAction::Host(HostAction::StartTell(f.name))])
                    .unwrap_or_default(),
                "squelch-character" | "squelch-account" => {
                    let name = std::mem::take(&mut self.entry).trim().to_owned();
                    if name.is_empty() {
                        return vec![];
                    }
                    request(if id == "squelch-account" {
                        UiRequest::ModifyAccountSquelch { add: true, name }
                    } else {
                        UiRequest::ModifyCharacterSquelch {
                            object: ObjectId(0),
                            add: true,
                            account: name,
                            message_type: 1,
                        }
                    })
                }
                "unsquelch" => self
                    .selected
                    .and_then(|i| squelches_shown(c.game).get(i).cloned())
                    .map(|q| {
                        self.selected = None;
                        request(if q.account {
                            UiRequest::ModifyAccountSquelch {
                                add: false,
                                name: q.name,
                            }
                        } else {
                            UiRequest::ModifyCharacterSquelch {
                                object: ObjectId(0),
                                add: false,
                                account: q.name,
                                message_type: 1,
                            }
                        })
                    })
                    .unwrap_or_default(),
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
                "trade" if c.game.trade().partner.is_some() => request(UiRequest::TradeClose),
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
    fn counts_are_drawn_with_thousands_separators() {
        assert_eq!(comma(1234567), "1,234,567");
    }
}
