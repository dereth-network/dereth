//! The skill window's trees: one skill panel at a time laid out as a tree of nodes, the skill
//! selected shown at the foot with what may be done with it.
//!
//! A panel is named by the examination: when the player's scripts say the examination now asks
//! for a skill panel, the tree takes that panel (the one it showed goes on a history of panels
//! visited; going back to the last one takes it off). The tree is emptied and each of the panel's
//! nodes is added at its place in the tree, the nodes it hangs from first, each hung from its
//! parent. A node is a skill or a node that opens another panel: its icon (for the node that goes
//! back, the panel it goes back to), its name as a tooltip (a trained skill's with its level), and
//! its look: a skill trained shows its level (marked when buffs or debuffs move it) and whether it
//! may be raised; one not trained whether it may be trained or not at all; a trained skill that is
//! not passive may be dragged (to the shortcut bar).
//!
//! Pressing a node's icon (with either button) selects nothing and has the examination ask for the
//! skill or the panel, which opens the examination window; the examination's request for a skill
//! selects it here, for a panel loads it, for anything else clears the foot. The foot of a skill
//! selected: its icon, its name (with its level once trained, cut to fit), and by its state what
//! the player's points allow: untrained, the skill credits (or hero skill credits) and what
//! training costs, the train button lit when it may be trained (its tooltip the cost); trained,
//! the experience to spend and what raising needs, the raise button lit when it may be raised and
//! the untrain button when it may be untrained; being untrained, the experience to spend and the
//! experience the skill holds.
//!
//! Training a skill whose training cannot be undone asks first; untraining always asks (a perk's
//! question names it a perk); stopping an untraining asks. A question answered yes does it. The
//! questions go when another skill is selected, another panel loaded, or the window shown or
//! hidden.
//!
//! The root panels: a button for each kind of skill (melee, missile, magic, the rest) the
//! player's race has a root panel for (the others show their disabled look); pressing one loads
//! its root panel, the history forgotten, that button lit and ghosted and the others not.

use std::any::Any;
use std::collections::BTreeMap;

use auberean_text::{StringInfo, Value as Var};
use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui, Value};

use crate::dialog;
use crate::flow;
use crate::gameplay::{self, notice, Child, Dialog, Examination, Notice, Record, Request};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;

/// A node's kinds: a skill, and the nodes that open other panels.
pub mod node_kind {
    pub const SKILL: u32 = 0x4000_0001;
    pub const PLUS: u32 = 0x4000_0002;
    pub const MINUS: u32 = 0x4000_0003;
}

/// The node that goes back to the panel visited before.
pub const BACK: u32 = 0x8000_004B;

/// The examination's request kinds the tree hears: a skill, a skill panel.
pub const EXAMINE_SKILL: u32 = 0x4000_0003;
pub const EXAMINE_PANEL: u32 = 0x4000_0004;

/// The kinds of skill the root panels are for, by the number the scripts know each by.
pub const ROOT_KINDS: [u32; 4] = [0x8000_0000, 0x8000_0001, 0x8000_0002, 0x8000_0003];

/// The raise button's tooltip when the skill may not be raised ("You cannot raise this skill"),
/// by its token in the messages' table.
pub const CANT_RAISE_TIP: u32 = 0x8389_6C7D;

/// Why a skill may not be trained: the player may never train it.
pub const NEVER_TRAINED: u32 = 0x7558;

mod name {
    use auberean_ui::Token;
    pub const SKILL_TREE: Token = Token::of("SkillTree");
    pub const SELECTED_NAME: Token = Token::of("SelectedName");
    pub const MISC_TEXT: Token = Token::of("MiscText");
    pub const SELECTED_ICON: Token = Token::of("SelectedIcon");
    pub const MODIFY_FIELD: Token = Token::of("ModifyField");
    pub const TRAIN_BUTTON: Token = Token::of("TrainButton");
    pub const UNTRAIN_BUTTON: Token = Token::of("UntrainButton");
    pub const RAISE_BUTTON: Token = Token::of("RaiseButton");
    pub const CANCEL_UNTRAIN_BUTTON: Token = Token::of("CancelUntrainButton");
    pub const NODE_ICON: Token = Token::of("SkillNodeIcon");
    pub const NODE_TEMPLATE: Token = Token::of("SkillNodeTemplate");
    pub const NODE_LEVEL_TEXT: Token = Token::of("SkillNodeLevelText");
    pub const ICON_AREA: Token = Token::of("IconArea");
    pub const BACKGROUND: Token = Token::of("Background");
    pub const CRAFT_TAB: Token = Token::of("CraftTab");
    /// The root panels' buttons, and their disabled looks, by kind.
    pub const ROOT_BUTTONS: [(Token, Token); 4] = [
        (Token::of("MeleeButton"), Token::of("MeleeButton_disabled")),
        (
            Token::of("MissileButton"),
            Token::of("MissileButton_disabled"),
        ),
        (Token::of("MagicButton"), Token::of("MagicButton_disabled")),
        (
            Token::of("NoncombatButton"),
            Token::of("NoncombatButton_disabled"),
        ),
    ];
    pub const SHORTCUT_TYPE: Token = Token::of("ShortcutType");
    pub const SHORTCUT_DATA: Token = Token::of("ShortcutData");
    pub const OPENED: Token = Token::of("opened");
    // States.
    pub const SELECTED: Token = Token::of("selected");
    pub const UNTRAINED: Token = Token::of("untrained");
    pub const TRAINED: Token = Token::of("trained");
    pub const UNTRAINING: Token = Token::of("untraining");
    pub const PANEL: Token = Token::of("panel");
    pub const SKILL: Token = Token::of("skill");
    pub const PLUS: Token = Token::of("plus");
    pub const MINUS: Token = Token::of("minus");
    pub const CAN_RAISE: Token = Token::of("canraise");
    pub const CAN_TRAIN: Token = Token::of("cantrain");
    pub const GREYED: Token = Token::of("greyed");
    pub const BUFF: Token = Token::of("buff");
    pub const DEBUFF: Token = Token::of("debuff");
    // Variables.
    pub const LEVEL: Token = Token::of("level");
    pub const VALUE: Token = Token::of("value");
    pub const COST: Token = Token::of("cost");
}

/// A node of a skill panel, as the skill panel manager gives it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Node {
    /// The skill (or panel) it stands for.
    pub skill: u32,
    pub kind: u32,
    pub name: StringInfo,
    pub description: StringInfo,
    pub icon: u32,
    pub x: i32,
    pub y: i32,
    /// The nodes it hangs from, and those whose look depends on it.
    pub parents: Vec<u32>,
    pub dependencies: Vec<u32>,
    pub passive: bool,
    /// Its training may be undone.
    pub untrainable: bool,
    pub perk: bool,
}

/// A skill's state as the player's scripts answer for it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct State {
    pub trained: bool,
    pub being_untrained: bool,
    pub hero: bool,
    /// 0 when it may be trained, untrained, raised; else why not.
    pub can_train: u32,
    pub can_untrain: u32,
    pub can_raise: u32,
    pub cost_to_train: u32,
    pub experience_to_raise: u64,
    pub experience: u64,
    /// Its own level, and its level as it stands.
    pub base_level: u32,
    pub level: u32,
}

/// What the skill window shows, as the player's scripts last answered: the panel the examination
/// last asked for and its nodes, the panels' names and icons, the panel's skills' states, the root
/// panel of each kind of skill, the player's points.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Facts {
    pub version: u64,
    pub panel: u32,
    pub nodes: Vec<Node>,
    pub panels: BTreeMap<u32, (StringInfo, u32)>,
    pub states: BTreeMap<u32, State>,
    pub roots: [Option<u32>; 4],
    pub experience: u64,
    pub credits: u32,
    pub hero_credits: u32,
}

/// What the tree heard, to act on once this frame's answers are in.
#[derive(Clone, Debug)]
enum Heard {
    Request(u32, u32),
    Skill(u32),
    Points(Option<(u64, u32, u32)>, bool),
}

/// The skill trees' panel.
#[derive(Debug)]
pub struct SkillTree {
    frame: FrameId,
    field: Option<Handle>,
    tree: Option<Handle>,
    selected_name: Option<Handle>,
    misc_text: Option<Handle>,
    selected_icon: Option<Handle>,
    modify_field: Option<Handle>,
    /// The panels visited before, the last first.
    history: Vec<u32>,
    current: u32,
    selected: u32,
    /// The panel's nodes, by skill.
    nodes: BTreeMap<u32, Node>,
    /// The root buttons heard, with their kinds.
    root_buttons: Vec<(Token, u32)>,
    roots_seen: bool,
    /// The questions up: untrain, stop untraining, train.
    untrain: u32,
    untrain_cancel: u32,
    train: u32,
    /// The player's points: experience to spend, skill credits, hero skill credits.
    experience: u64,
    credits: u32,
    hero_credits: u32,
    facts: Facts,
    heard: Vec<Heard>,
}

/// The messages' string table.
fn table(ui: &Ui) -> u32 {
    ui.assets()
        .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING)
}

fn entry(ui: &Ui, id: &str) -> StringInfo {
    StringInfo::entry(table(ui), Token::of(id).raw())
}

fn text_of(ui: &Ui, info: &StringInfo) -> String {
    ui.assets().write(info).unwrap_or_default()
}

impl SkillTree {
    /// The trees under the skill window's framework, in its field: their elements found, the
    /// root buttons for the player's race (once the scripts answer), the buttons and the node
    /// icons heard.
    // D2-02694
    pub fn new(ui: &mut Ui, parent: FrameId, field: Option<Handle>) -> Self {
        let frame = ui.new_child_frame(parent);
        let find = |ui: &Ui, n| field.and_then(|f| ui.child_recursive(f, n));
        let mut me = Self {
            frame,
            field,
            tree: find(ui, name::SKILL_TREE),
            selected_name: find(ui, name::SELECTED_NAME),
            misc_text: find(ui, name::MISC_TEXT),
            selected_icon: find(ui, name::SELECTED_ICON),
            modify_field: find(ui, name::MODIFY_FIELD),
            history: Vec::new(),
            current: 0,
            selected: 0,
            nodes: BTreeMap::new(),
            root_buttons: Vec::new(),
            roots_seen: false,
            untrain: 0,
            untrain_cancel: 0,
            train: 0,
            experience: 0,
            credits: 0,
            hero_credits: 0,
            facts: Facts::default(),
            heard: Vec::new(),
        };
        if let Some(l) = ui.frame_listener(frame) {
            for b in [
                name::TRAIN_BUTTON,
                name::UNTRAIN_BUTTON,
                name::RAISE_BUTTON,
                name::CANCEL_UNTRAIN_BUTTON,
            ] {
                ui.listen_for_message(l, b, msg::BUTTON_RELEASE_ON);
            }
            ui.listen_for_message(l, name::NODE_ICON, msg::MOUSE_PRESS);
            ui.listen_for_message(l, name::CRAFT_TAB, name::OPENED);
        }
        me.facts.version = u64::MAX;
        me
    }

    /// The panel shown, and the skill selected (0: none).
    #[must_use]
    pub const fn shown(&self) -> (u32, u32) {
        (self.current, self.selected)
    }

    /// The panels visited before, the last first.
    #[must_use]
    pub fn history(&self) -> &[u32] {
        &self.history
    }

    /// The root buttons by the player's race: each kind with a root panel heard and its button
    /// shown, the others' disabled looks shown instead.
    // D2-02687
    fn root_buttons(&mut self, ui: &mut Ui) {
        let Some(f) = self.field else { return };
        self.root_buttons.clear();
        for (i, (button, disabled)) in name::ROOT_BUTTONS.iter().enumerate() {
            let b = ui.child_recursive(f, *button);
            let d = ui.child_recursive(f, *disabled);
            let has = self.facts.roots[i].is_some();
            if has {
                if let Some(l) = ui.frame_listener(self.frame) {
                    ui.listen_for_message(l, *button, msg::MOUSE_PRESS);
                }
                self.root_buttons.push((*button, ROOT_KINDS[i]));
            }
            if let (Some(b), Some(d)) = (b, d) {
                ui.set_visible(b, has);
                ui.set_visible(d, !has);
            }
        }
    }

    /// A skill's state, as last answered.
    fn state(&self, skill: u32) -> State {
        self.facts.states.get(&skill).copied().unwrap_or_default()
    }

    /// A panel's name and icon, as last answered.
    fn panel(&self, panel: u32) -> (StringInfo, u32) {
        self.facts.panels.get(&panel).cloned().unwrap_or_default()
    }

    /// A skill's level as text: its level as it stands, and (with `value`) how far buffs or
    /// debuffs move it; the look its level takes (buffed, debuffed, or plain).
    // D2-02666
    fn make_level_text(&self, ui: &Ui, skill: u32, value: bool) -> (StringInfo, Token) {
        let s = self.state(skill);
        #[allow(clippy::cast_possible_wrap)]
        let diff = s.level.wrapping_sub(s.base_level) as i32;
        let mut info = entry(ui, "ID_Skill_LevelNormal");
        info.add_var(name::LEVEL.raw(), Var::UInt(s.level));
        let mut look = state::STATELESS;
        if diff != 0 {
            if value {
                let id = if diff < 0 {
                    "ID_Skill_LevelDebuffed"
                } else {
                    "ID_Skill_LevelBuffed"
                };
                info.string_id = Token::of(id).raw();
                #[allow(clippy::cast_possible_wrap)]
                info.add_var(name::VALUE.raw(), Var::Int(diff.unsigned_abs() as i32));
            }
            look = if diff < 0 { name::DEBUFF } else { name::BUFF };
        }
        (info, look)
    }

    /// A node's background in the tree, selected or not.
    // D2-02667
    fn select_node(&self, ui: &mut Ui, node: &Node, on: bool) {
        let Some(t) = self.tree else { return };
        if let Some(b) = ui
            .tree_item(t, node.x, node.y)
            .and_then(|i| ui.child_recursive(i, name::BACKGROUND))
        {
            ui.set_state(b, if on { name::SELECTED } else { state::STATELESS });
        }
    }

    /// Shown or hidden: the questions go.
    // D2-02668
    fn show_tree(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        self.close_questions(out);
    }

    /// The three questions taken down.
    fn close_questions(&mut self, out: &mut Vec<Request>) {
        for q in [&mut self.untrain, &mut self.untrain_cancel, &mut self.train] {
            out.push(Request::DialogDown(*q));
            *q = 0;
        }
    }

    /// A question closed: yes to untraining untrains the skill selected, yes to stopping stops
    /// the untraining, yes to training trains it.
    // D2-02669
    pub fn close_confirmation_dialog(&mut self, id: u32, yes: bool, out: &mut Vec<Request>) {
        let s = self.selected;
        if self.untrain == id {
            self.untrain = 0;
            if yes {
                out.push(Request::Skill(SkillRequest::Untrain(s)));
            }
        }
        if self.untrain_cancel == id {
            self.untrain_cancel = 0;
            if yes {
                out.push(Request::Skill(SkillRequest::CancelUntrain));
            }
        }
        if self.train == id {
            self.train = 0;
            if yes {
                out.push(Request::Skill(SkillRequest::Train(s)));
            }
        }
    }

    /// The node table emptied.
    // D2-02670, D2-02689
    fn clear_nodes(&mut self) {
        self.nodes.clear();
    }

    /// The name at the foot, written and cut to fit (an ellipsis after what fits).
    // D2-02671
    fn set_string_and_truncate(&self, ui: &mut Ui, info: &StringInfo) {
        let Some(n) = self.selected_name else { return };
        let text = text_of(ui, info);
        ui.set_text(n, &text);
        let ellipsis = text_of(ui, &entry(ui, "ID_Ellipsis"));
        ui.truncate_text(n, &ellipsis);
    }

    /// A question asked, kept by its number.
    fn ask(ui: &Ui, text: &StringInfo, out: &mut Vec<Request>) -> u32 {
        let id = dialog::new_context();
        out.push(Request::Dialog(Dialog {
            id,
            kind: dialog::kind::CONFIRMATION,
            text: text_of(ui, text),
            modal: false,
            info: None,
        }));
        id
    }

    /// Whether to stop the untraining under way, unless asked already.
    // D2-02664
    fn make_untrain_cancel_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        if self.untrain_cancel == 0 {
            self.untrain_cancel = Self::ask(ui, &entry(ui, "ID_Skill_UntrainCancelConfirm"), out);
        }
    }

    /// Whether to train, unless asked already.
    // D2-02665
    fn make_train_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        if self.train == 0 {
            self.train = Self::ask(ui, &entry(ui, "ID_Skill_TrainConfirm"), out);
        }
    }

    /// Whether to untrain the skill selected (a perk's question its own), naming it, unless asked
    /// already.
    // D2-02681
    fn make_untrain_dialog(&mut self, ui: &Ui, out: &mut Vec<Request>) {
        if self.untrain != 0 {
            return;
        }
        let Some(node) = self.nodes.get(&self.selected) else {
            return;
        };
        let mut text = entry(
            ui,
            if node.perk {
                "ID_Skill_Perk_UntrainText"
            } else {
                "ID_Skill_UntrainText"
            },
        );
        text.add_var(name::VALUE.raw(), Var::Text(Box::new(node.name.clone())));
        self.untrain = Self::ask(ui, &text, out);
    }

    /// The foot for a skill selected: its icon, its name (with its level once trained), what its
    /// state allows and costs, the buttons lit by what may be done.
    // D2-02673
    fn update_selected_skill_display(&self, ui: &mut Ui, node: &Node) {
        if let Some(i) = self.selected_icon {
            ui.set_image(i, node.icon, 2);
        }
        let mut shown_name = StringInfo::default();
        if let Some(m) = self.modify_field {
            ui.set_state(m, state::STATELESS);
            let s = self.state(node.skill);
            let button = |ui: &Ui, n| ui.child_recursive(m, n);
            let (text, value, cost): (&str, u64, u64);
            if s.trained {
                if s.being_untrained {
                    ui.set_state(m, name::UNTRAINING);
                    (text, value, cost) =
                        ("ID_Skill_AvailableXPUntrain", self.experience, s.experience);
                } else {
                    ui.set_state(m, name::TRAINED);
                    let can = s.can_raise == 0;
                    (text, value, cost) = (
                        if can {
                            "ID_Skill_AvailableXPRaise"
                        } else {
                            "ID_Skill_CantRaise"
                        },
                        self.experience,
                        s.experience_to_raise,
                    );
                    let Some(r) = button(ui, name::RAISE_BUTTON) else {
                        return;
                    };
                    ui.set_attr(r, attr::GHOSTED, Value::Bool(!can));
                    let mut tip = if can {
                        entry(ui, "ID_Skill_TT_ClickToRaise")
                    } else {
                        StringInfo::entry(table(ui), CANT_RAISE_TIP)
                    };
                    tip.add_var(name::VALUE.raw(), Var::ULong(s.experience_to_raise));
                    let tip = text_of(ui, &tip);
                    ui.set_tooltip(r, &tip);
                    let Some(u) = button(ui, name::UNTRAIN_BUTTON) else {
                        return;
                    };
                    ui.set_attr(u, attr::GHOSTED, Value::Bool(s.can_untrain != 0));
                }
                let (level, _) = self.make_level_text(ui, node.skill, true);
                shown_name = entry(ui, "ID_Skill_NameLevel");
                shown_name.add_var(name::VALUE.raw(), Var::Text(Box::new(node.name.clone())));
                shown_name.add_var(name::LEVEL.raw(), Var::Text(Box::new(level)));
            } else {
                ui.set_state(m, name::UNTRAINED);
                let ok = s.can_train == 0;
                let never = s.can_train == NEVER_TRAINED;
                text = match (s.hero, never) {
                    (_, true) => "ID_Skill_CantTrain",
                    (true, false) => "ID_Skill_AvailableHSPTrain",
                    (false, false) => "ID_Skill_AvailableSPTrain",
                };
                cost = u64::from(s.cost_to_train);
                value = u64::from(if s.hero {
                    self.hero_credits
                } else {
                    self.credits
                });
                shown_name = node.name.clone();
                let Some(t) = button(ui, name::TRAIN_BUTTON) else {
                    return;
                };
                ui.set_attr(t, attr::GHOSTED, Value::Bool(!ok));
                if ok {
                    let mut tip = entry(ui, "ID_Skill_TT_ClickToTrain");
                    tip.add_var(name::VALUE.raw(), Var::ULong(cost));
                    let tip = text_of(ui, &tip);
                    ui.set_tooltip(t, &tip);
                }
            }
            if let Some(misc) = self.misc_text {
                let mut info = entry(ui, text);
                info.add_var(name::VALUE.raw(), Var::ULong(value));
                info.add_var(name::COST.raw(), Var::ULong(cost));
                let written = text_of(ui, &info);
                ui.set_text(misc, &written);
            }
        }
        if self.selected_name.is_some() {
            self.set_string_and_truncate(ui, &shown_name);
        }
    }

    /// A node brought up to date in the tree: its icon (the node going back shows the panel it
    /// goes back to) as the shortcut it would be, its look by kind, its tooltip; a skill's level
    /// text, whether it may be dragged, and its look by what may be done; the foot when it is the
    /// skill selected.
    // D2-02674
    fn refresh_skill_node(&self, ui: &mut Ui, node: &Node) {
        let back = (node.skill == BACK)
            .then(|| self.history.first().copied())
            .flatten()
            .unwrap_or(0);
        let Some(t) = self.tree else { return };
        let Some(item) = ui.tree_item(t, node.x, node.y) else {
            return;
        };
        let (back_name, back_icon) = self.panel(back);
        let icon = if back != 0 { back_icon } else { node.icon };
        let Some(icon_el) = ui.child_recursive(item, name::NODE_ICON) else {
            return;
        };
        ui.add_image_to_state(icon_el, icon, 2, state::STATELESS);
        ui.set_attr(icon_el, name::SHORTCUT_TYPE, Value::Id(node_kind::SKILL));
        ui.set_attr(icon_el, name::SHORTCUT_DATA, Value::Id(node.skill));
        let mut tip = if back != 0 {
            back_name
        } else {
            node.name.clone()
        };
        let look = match node.kind {
            node_kind::SKILL => name::SKILL,
            node_kind::PLUS => name::PLUS,
            node_kind::MINUS => name::MINUS,
            _ => Token(back),
        };
        ui.set_state(item, look);
        let Some(area) = ui.node(icon_el).and_then(|n| n.parent) else {
            return;
        };
        let mut area_look = state::STATELESS;
        if node.kind == node_kind::SKILL {
            let s = self.state(node.skill);
            let Some(level) = ui.child(item, name::NODE_LEVEL_TEXT) else {
                return;
            };
            let dragable = if s.trained {
                let (lt, look) = self.make_level_text(ui, node.skill, false);
                ui.set_state(level, look);
                let written = text_of(ui, &lt);
                ui.set_text(level, &written);
                let (with_value, _) = self.make_level_text(ui, node.skill, true);
                tip = entry(ui, "ID_Skill_NameLevel");
                tip.add_var(name::VALUE.raw(), Var::Text(Box::new(node.name.clone())));
                tip.add_var(name::LEVEL.raw(), Var::Text(Box::new(with_value)));
                if s.can_raise == 0 {
                    area_look = name::CAN_RAISE;
                }
                !node.passive
            } else {
                ui.set_state(level, state::STATELESS);
                ui.clear_all_text(level);
                area_look = if s.can_train == 0 {
                    name::CAN_TRAIN
                } else {
                    name::GREYED
                };
                false
            };
            ui.set_attr(icon_el, attr::DRAGABLE, Value::Bool(dragable));
        } else {
            ui.set_attr(icon_el, attr::DRAGABLE, Value::Bool(false));
        }
        ui.set_state(area, area_look);
        let tip = text_of(ui, &tip);
        ui.set_tooltip(item, &tip);
        if node.skill == self.selected {
            self.update_selected_skill_display(ui, node);
        }
    }

    /// A skill node's look by what may be done with it, changed only when it differs; the foot
    /// when it is the skill selected.
    // D2-02675
    fn fast_refresh_skill_node(&self, ui: &mut Ui, node: &Node) {
        if node.kind != node_kind::SKILL {
            return;
        }
        let s = self.state(node.skill);
        let look = if !s.trained {
            if s.can_train != 0 {
                name::GREYED
            } else {
                name::CAN_TRAIN
            }
        } else if s.can_raise == 0 {
            name::CAN_RAISE
        } else {
            state::STATELESS
        };
        let Some(area) = self
            .tree
            .and_then(|t| ui.tree_item(t, node.x, node.y))
            .and_then(|i| ui.child_recursive(i, name::ICON_AREA))
        else {
            return;
        };
        if ui.node(area).map(|n| n.state) != Some(look) {
            ui.set_state(area, look);
        }
        if node.skill == self.selected {
            self.update_selected_skill_display(ui, node);
        }
    }

    /// A root button pressed: its kind's root panel asked of the examination (nothing selected,
    /// the examination window opened); that button lit and ghosted, the others not.
    // D2-02676
    fn handle_element(&mut self, ui: &mut Ui, element: Token, out: &mut Vec<Request>) {
        for (button, kind) in self.root_buttons.clone() {
            let pressed = button == element;
            if pressed {
                let i = ROOT_KINDS.iter().position(|k| *k == kind).unwrap_or(0);
                if let Some(root) = self.facts.roots[i] {
                    out.push(Request::Examine(Examination {
                        kind: EXAMINE_PANEL,
                        data_enum: root,
                        ..Examination::default()
                    }));
                    out.push(Request::OpenExaminationPanel);
                }
            }
            if self.tree.is_some() {
                if let Some(b) = self.field.and_then(|f| ui.child_recursive(f, button)) {
                    ui.set_attr(b, attr::HIGHLIGHTED, Value::Bool(pressed));
                    ui.set_attr(b, attr::GHOSTED, Value::Bool(pressed));
                }
            }
        }
    }

    /// The player's points changed: kept, and every node's look brought up to date (or, with
    /// `full`, every node refreshed whole).
    // D2-02678, D2-02679
    fn update_skill_panel(&mut self, ui: &mut Ui, points: (u64, u32, u32), full: bool) {
        (self.experience, self.credits, self.hero_credits) = points;
        let nodes: Vec<Node> = self.nodes.values().cloned().collect();
        for n in &nodes {
            if full {
                self.refresh_skill_node(ui, n);
            } else {
                self.fast_refresh_skill_node(ui, n);
            }
        }
    }

    /// A skill selected (another's questions down and its node no longer selected): its node
    /// selected and the foot shown, when it is on the panel.
    // D2-02680
    fn select_skill(&mut self, ui: &mut Ui, skill: u32, out: &mut Vec<Request>) {
        if skill != self.selected {
            self.close_questions(out);
            if let Some(n) = self.nodes.get(&self.selected).cloned() {
                if self.selected != 0 {
                    self.select_node(ui, &n, false);
                }
            }
        }
        self.selected = 0;
        let Some(node) = self.nodes.get(&skill).cloned() else {
            return;
        };
        self.select_node(ui, &node, true);
        self.selected = skill;
        self.update_selected_skill_display(ui, &node);
    }

    /// Nothing selected: the node no longer selected, the foot emptied, the questions down; the
    /// panel forgotten (its nodes stay).
    // D2-02682
    fn clear_selected_info(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if self.selected != 0 {
            if let Some(n) = self.nodes.get(&self.selected).cloned() {
                self.select_node(ui, &n, false);
            }
        }
        self.current = 0;
        self.selected = 0;
        for t in [self.selected_name, self.misc_text].into_iter().flatten() {
            ui.clear_all_text(t);
        }
        if let Some(i) = self.selected_icon {
            ui.clear_image(i);
        }
        if let Some(m) = self.modify_field {
            ui.set_state(m, state::STATELESS);
        }
        self.close_questions(out);
    }

    /// A skill changed: the nodes that depend on it refreshed.
    // D2-02685
    fn update_skill(&mut self, ui: &mut Ui, skill: u32) {
        let Some(node) = self.nodes.get(&skill) else {
            return;
        };
        let deps: Vec<Node> = node
            .dependencies
            .iter()
            .filter_map(|d| self.nodes.get(d).cloned())
            .collect();
        for d in &deps {
            self.refresh_skill_node(ui, d);
        }
    }

    /// A node put in the tree at its place, unless one is there: the nodes it hangs from first
    /// (those placed), it hung from each; then brought up to date.
    // D2-02686
    fn add_node(&self, ui: &mut Ui, node: &Node, depth: u32) {
        if depth > 64 {
            return;
        }
        if let Some(t) = self.tree {
            if ui.tree_is_occupied(t, node.x, node.y) {
                return;
            }
        }
        let item = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::NODE_TEMPLATE);
        if let (Some(t), Some(i)) = (self.tree, item) {
            ui.tree_add(t, i, node.x, node.y);
        }
        for p in &node.parents {
            let Some(parent) = self.nodes.get(p) else {
                continue;
            };
            if parent.x >= 0 && parent.y >= 0 {
                self.add_node(ui, &parent.clone(), depth + 1);
                if let (Some(t), Some(i)) = (self.tree, item) {
                    ui.tree_set_parent(t, i, parent.x, parent.y);
                }
            }
        }
        self.refresh_skill_node(ui, node);
    }

    /// A panel loaded (when it is not the one shown): the one shown goes on the history (or, when
    /// going back to the last one visited, the history loses it), the tree emptied and the
    /// panel's nodes added; the questions down, nothing selected; the foot shows the panel's name
    /// and icon.
    // D2-02688
    fn load_panel(&mut self, ui: &mut Ui, panel: u32, out: &mut Vec<Request>) {
        if panel != self.current {
            if self.current != 0 {
                if self.history.first() == Some(&panel) {
                    self.history.remove(0);
                } else {
                    self.history.insert(0, self.current);
                }
            }
            self.current = panel;
            self.clear_nodes();
            if let Some(t) = self.tree {
                ui.tree_flush(t);
                if self.facts.panel == panel {
                    self.nodes = self
                        .facts
                        .nodes
                        .iter()
                        .map(|n| (n.skill, n.clone()))
                        .collect();
                }
                ui.tree_begin_changes(t);
                let nodes: Vec<Node> = self.nodes.values().cloned().collect();
                for n in &nodes {
                    self.add_node(ui, n, 0);
                }
                ui.tree_changes_done(t);
                ui.tree_end(t);
            }
        }
        self.close_questions(out);
        self.selected = 0;
        let (name, icon) = self.panel(self.current);
        if self.selected_name.is_some() {
            self.set_string_and_truncate(ui, &name);
        }
        if let Some(m) = self.misc_text {
            ui.clear_all_text(m);
        }
        if let Some(i) = self.selected_icon {
            ui.set_image(i, icon, 2);
        }
        if let Some(m) = self.modify_field {
            ui.set_state(m, name::PANEL);
        }
    }

    /// The examination's request changed: a skill panel loaded (when another), a skill selected
    /// (when another), anything else the foot cleared.
    // D2-02690
    fn examination_request_changed(
        &mut self,
        ui: &mut Ui,
        kind: u32,
        data: u32,
        out: &mut Vec<Request>,
    ) {
        match kind {
            EXAMINE_PANEL => {
                if data != self.current {
                    self.load_panel(ui, data, out);
                }
            }
            EXAMINE_SKILL => {
                if data != self.selected {
                    self.select_skill(ui, data, out);
                }
            }
            _ => self.clear_selected_info(ui, out),
        }
    }
}

/// What the skill window asks of a skill.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SkillRequest {
    Train(u32),
    Untrain(u32),
    Raise(u32),
    CancelUntrain,
}

impl SkillTree {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x436, 0x45D, 0x40D, 0x4A3, 0x415, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for SkillTree {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        self.show_tree(ui, on, out);
    }

    /// A node's icon pressed (either button): the skill or panel it stands for asked of the
    /// examination (the node going back: the panel visited before); the train, untrain, raise and
    /// stop-untraining buttons, for the skill selected; a root button or the crafting tab: the
    /// history forgotten and the root buttons' looks set.
    // D2-02684
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        let Mail::Element {
            from,
            element,
            what,
            param,
        } = *mail
        else {
            return;
        };
        if element == name::NODE_ICON
            && what == msg::MOUSE_PRESS
            && (param == button_id::LEFT || param == button_id::RIGHT)
        {
            let id = ui
                .node(from)
                .and_then(|n| n.id_attr(name::SHORTCUT_DATA))
                .unwrap_or(0);
            let Some(node) = self.nodes.get(&id) else {
                return;
            };
            let kind = if node.kind == node_kind::SKILL {
                EXAMINE_SKILL
            } else {
                EXAMINE_PANEL
            };
            let mut data = id;
            if id == BACK {
                match self.history.first() {
                    Some(&p) if p != 0 => data = p,
                    _ => return,
                }
            }
            out.push(Request::Examine(Examination {
                kind,
                data_enum: data,
                ..Examination::default()
            }));
            out.push(Request::OpenExaminationPanel);
        }
        if [
            name::TRAIN_BUTTON,
            name::UNTRAIN_BUTTON,
            name::RAISE_BUTTON,
            name::CANCEL_UNTRAIN_BUTTON,
        ]
        .contains(&element)
        {
            let s = self.selected;
            if s == 0 {
                return;
            }
            if element == name::TRAIN_BUTTON {
                if self.state(s).can_train != 0 {
                    return;
                }
                let Some(node) = self.nodes.get(&s) else {
                    return;
                };
                if node.untrainable {
                    out.push(Request::Skill(SkillRequest::Train(s)));
                } else {
                    self.make_train_dialog(ui, out);
                }
            }
            if element == name::UNTRAIN_BUTTON {
                self.make_untrain_dialog(ui, out);
            }
            if element == name::RAISE_BUTTON {
                out.push(Request::Skill(SkillRequest::Raise(s)));
            }
            if element == name::CANCEL_UNTRAIN_BUTTON {
                self.make_untrain_cancel_dialog(ui, out);
            }
        }
        let root = self.root_buttons.iter().any(|(b, _)| *b == element);
        if root || element == name::CRAFT_TAB {
            self.history.clear();
            self.handle_element(ui, element, out);
        }
    }

    /// The examination's request, a skill changed, the player's points changed: acted on once
    /// this frame's answers are in; a question closed.
    fn notice(&mut self, _ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        if let Some((id, yes)) = dialog::confirmation_closed(n) {
            self.close_confirmation_dialog(id, yes, out);
            return;
        }
        let w = |i: usize| n.words.get(i).copied().unwrap_or(0);
        match n.native {
            notice::EXAMINATION_REQUEST_CHANGED => {
                if let Some(Record::Request(r)) = n.records.iter().flatten().next() {
                    self.heard.push(Heard::Request(r.kind, r.data_enum));
                }
            }
            notice::UPDATE_SKILL => self.heard.push(Heard::Skill(w(0))),
            notice::UPDATE_SKILL_PANEL | notice::UPDATE_SKILL_PANEL_FULL => {
                let points = (n.words.len() >= 4).then(|| (n.long(0), w(2), w(3)));
                self.heard.push(Heard::Points(
                    points,
                    n.native == notice::UPDATE_SKILL_PANEL_FULL,
                ));
            }
            _ => {}
        }
    }

    /// This frame's answers of the scripts taken (the root buttons and the player's points the
    /// first time), then what was heard acted on.
    fn update(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, out: &mut Vec<Request>) {
        if let Some(f) = view.skills {
            if f.version != self.facts.version {
                self.facts = f.clone();
                if !self.roots_seen {
                    self.roots_seen = true;
                    self.experience = f.experience;
                    self.credits = f.credits;
                    self.hero_credits = f.hero_credits;
                    self.root_buttons(ui);
                }
            }
        }
        for h in std::mem::take(&mut self.heard) {
            match h {
                Heard::Request(kind, data) => self.examination_request_changed(ui, kind, data, out),
                Heard::Skill(s) => self.update_skill(ui, s),
                Heard::Points(p, full) => {
                    let p = p.unwrap_or((
                        self.facts.experience,
                        self.facts.credits,
                        self.facts.hero_credits,
                    ));
                    self.update_skill_panel(ui, p, full);
                }
            }
        }
    }

    /// It goes: its nodes, its questions (they go with the play screen), its framework.
    // D2-02695
    fn close(mut self: Box<Self>, ui: &mut Ui) {
        self.clear_nodes();
        if let Some(t) = self.tree {
            ui.tree_flush(t);
        }
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
