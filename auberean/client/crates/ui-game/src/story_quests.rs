//! The quest window's story page: the story's acts as the player's scripts list them, each a row
//! with its name and an icon per scene, open to list its scenes by name.
//!
//! When the scripts say an act changed, it becomes the act last updated: it is open at the next
//! laying out, and (unless the whole story was given) it is marked changed; while the page is not
//! shown the window's button shows that something changed and the story tab is opened. The page
//! is laid out again at the next tick of the clock after anything changed: every visible act, in
//! the scripts' order, its name (marked when changed), lit when open, its scenes' icons (the
//! hidden ones left out) completed when the scene may be played again, else not, and, open, a row
//! per scene. Pressing an act's row opens or closes it (no longer changed); a scene's row pressed
//! twice plays the scene when it may be played again. Hiding the page closes the act last
//! updated.

use std::any::Any;
use std::collections::BTreeMap;

use auberean_text::StringInfo;
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Ui, Value};

use crate::gameplay::{self, notice, Child, Notice, Request};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;

/// How the scripts say the story changed: the whole story given (no act marked changed).
pub const WHOLE_STORY: u32 = 0x4000_0002;

mod name {
    use auberean_ui::Token;
    pub const STORY_PAGE: Token = Token::of("StoryPage");
    pub const ACT_LIST: Token = Token::of("ActListBox");
    pub const ACT_TEMPLATE: Token = Token::of("ActTemplate");
    pub const ACT_NAME: Token = Token::of("ActName");
    pub const SCENE_TEMPLATE: Token = Token::of("SceneTemplate");
    pub const SCENE_NAME: Token = Token::of("SceneName");
    pub const SCENE_ICONS: Token = Token::of("SceneIconListBox");
    pub const SCENE_ICON: Token = Token::of("SceneIconTemplate");
    pub const ACT_ID: Token = Token::of("actid");
    pub const SCENE_ID: Token = Token::of("sceneid");
    pub const NORMAL: Token = Token::of("normal");
    pub const HIGHLIGHT: Token = Token::of("highlight");
    pub const CHANGED: Token = Token::of("changed");
    pub const COMPLETED: Token = Token::of("completed");
    pub const NOT_COMPLETED: Token = Token::of("notcompleted");
    pub const QUEST_BUTTON: Token = Token::of("QuestButton");
    pub const STORY_TAB: Token = Token::of("StoryTab");
}

/// A scene of an act.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub id: u32,
    pub name: StringInfo,
    /// Hidden (locked): not listed.
    pub hidden: bool,
    /// It may be played again (the player has seen it): shown completed.
    pub playable: bool,
}

/// An act of the story, its scenes in their order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Act {
    pub id: u32,
    pub name: StringInfo,
    /// Listed on the page.
    pub visible: bool,
    pub scenes: Vec<Scene>,
}

/// The story page's panel.
#[derive(Debug)]
pub struct StoryQuests {
    frame: FrameId,
    list: Option<Handle>,
    acts: Vec<Act>,
    /// The acts' rows, by act.
    rows: BTreeMap<u32, Handle>,
    /// To be laid out at the next tick; the tick came (laid out once this frame's list is in).
    dirty: bool,
    due: bool,
    /// The acts open, and those marked changed.
    open: BTreeMap<u32, bool>,
    changed: BTreeMap<u32, bool>,
    /// The act last updated.
    last: u32,
    /// The acts as the scripts last listed them.
    given: Vec<Act>,
}

impl StoryQuests {
    /// The page under the quest window's framework: its list found, its rows and the clock
    /// heard.
    // D2-02736
    pub fn new(ui: &mut Ui, parent: FrameId) -> Self {
        let frame = ui.new_child_frame(parent);
        let list = ui
            .element(name::STORY_PAGE)
            .and_then(|p| ui.child_recursive(p, name::ACT_LIST));
        if list.is_some() {
            if let Some(l) = ui.frame_listener(frame) {
                ui.listen_for_message(l, name::ACT_TEMPLATE, msg::BUTTON_RELEASE_ON);
                ui.listen_for_message(l, name::SCENE_TEMPLATE, msg::MOUSE_PRESS);
                ui.listen_for_global_message(l, global::TIME);
            }
        }
        Self {
            frame,
            list,
            acts: Vec::new(),
            rows: BTreeMap::new(),
            dirty: false,
            due: false,
            open: BTreeMap::new(),
            changed: BTreeMap::new(),
            last: 0,
            given: Vec::new(),
        }
    }

    /// The acts' rows laid out, in order.
    #[must_use]
    pub fn acts(&self) -> &[Act] {
        &self.acts
    }

    /// The scripts said an act changed (`how`: the whole story, or one act): the act last
    /// updated closes and this one takes its place; marked changed unless the whole story was
    /// given; while the page is not shown the window's button shows the change and the story tab
    /// opens.
    // D2-02730
    pub fn update_story_quests(&mut self, ui: &mut Ui, how: u32, act: u32) {
        self.open.insert(self.last, false);
        self.last = act;
        self.dirty = true;
        if how != WHOLE_STORY {
            self.changed.insert(act, true);
            if !ui.frame_shown(self.frame) {
                if let Some(b) = ui.element(name::QUEST_BUTTON) {
                    ui.set_state(b, name::CHANGED);
                }
                crate::quests::QuestWindow::set_active_panel(ui, name::STORY_TAB);
            }
        }
    }

    /// The acts laid out again from the scripts' list: each visible act, its name (marked when
    /// changed), lit when open, its scenes' icons, and, open, its scenes' rows; the act last
    /// updated open and scrolled to (its place counting the acts not listed).
    // D2-02731
    fn reload_acts(&mut self, ui: &mut Ui) {
        let Some(list) = self.list else { return };
        ui.list_flush(list);
        self.rows.clear();
        self.acts = self.given.clone();
        let mut last_row = None;
        let acts = self.acts.clone();
        for (index, act) in acts.iter().enumerate() {
            if !act.visible {
                continue;
            }
            if act.id == self.last {
                last_row = Some(index);
                self.open.insert(act.id, true);
            }
            let Some(row) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::ACT_TEMPLATE) else {
                continue;
            };
            ui.list_multi_add(list, row);
            self.rows.insert(act.id, row);
            let Some(act_name) = ui.child_recursive(row, name::ACT_NAME) else {
                continue;
            };
            ui.set_string_info(act_name, &act.name);
            let open = self.open.get(&act.id).copied().unwrap_or(false);
            ui.set_state(row, if open { name::HIGHLIGHT } else { name::NORMAL });
            if self.changed.get(&act.id).copied().unwrap_or(false) {
                ui.set_state(act_name, name::CHANGED);
            }
            ui.set_attr(row, name::ACT_ID, Value::Id(act.id));
            let Some(icons) = ui.child_recursive(row, name::SCENE_ICONS) else {
                continue;
            };
            for scene in act.scenes.iter().filter(|s| !s.hidden) {
                let Some(icon) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::SCENE_ICON)
                else {
                    continue;
                };
                ui.list_multi_add(icons, icon);
                let look = if scene.playable {
                    name::COMPLETED
                } else {
                    name::NOT_COMPLETED
                };
                ui.set_state(icon, look);
                if !open {
                    continue;
                }
                let Some(srow) = ui.frame_create_element(TOOLBAR_LAYOUT_ENUM, name::SCENE_TEMPLATE)
                else {
                    continue;
                };
                ui.set_state(srow, look);
                ui.set_attr(srow, name::SCENE_ID, Value::Id(scene.id));
                ui.set_attr(srow, name::ACT_ID, Value::Id(act.id));
                ui.list_multi_add(list, srow);
                if let Some(n) = ui.child_recursive(srow, name::SCENE_NAME) {
                    ui.set_string_info(n, &scene.name);
                }
            }
            ui.list_changes_done(icons);
        }
        ui.list_changes_done(list);
        if let Some(i) = last_row {
            ui.list_scroll_to_row(list, i32::try_from(i).unwrap_or(0), true);
        }
    }
}

impl StoryQuests {
    /// The notices it registers for as it is made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x41B, 0x1_040C];

    /// Its notices as the panels name them.
    #[must_use]
    pub fn notices_of() -> Vec<u32> {
        crate::notices::keys(Self::NOTICES)
    }
}

impl Child for StoryQuests {
    fn notices(&self) -> Vec<u32> {
        Self::notices_of()
    }

    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Hidden while its list was shown: the act last updated closes, no longer changed.
    // D2-02732
    fn show(&mut self, ui: &mut Ui, on: bool, _out: &mut Vec<Request>) {
        let list_shown = self.list.is_some_and(|l| ui.shown(l));
        ui.frame_show(self.frame, on);
        if list_shown && !ui.frame_shown(self.frame) && self.last != 0 {
            self.open.insert(self.last, false);
            self.changed.insert(self.last, false);
            self.last = 0;
            self.dirty = true;
        }
    }

    /// An act's row pressed: opened or closed, no longer changed; a scene's row pressed twice:
    /// the scene played when it may be played again; the clock: laid out again when anything
    /// changed.
    // D2-02734, D2-02735
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::TIME, ..
            } if self.dirty => {
                self.dirty = false;
                self.due = true;
            }
            Mail::Element {
                from,
                element,
                what,
                param,
            } => {
                let attr = |n| ui.node(from).and_then(|x| x.id_attr(n)).unwrap_or(0);
                if element == name::ACT_TEMPLATE && what == msg::BUTTON_RELEASE_ON {
                    let act = attr(name::ACT_ID);
                    self.last = 0;
                    let open = self.open.get(&act).copied().unwrap_or(false);
                    self.open.insert(act, !open);
                    self.changed.insert(act, false);
                    self.dirty = true;
                } else if element == name::SCENE_TEMPLATE
                    && what == msg::MOUSE_PRESS
                    && param == button_id::DOUBLE_LEFT
                {
                    let (act, scene) = (attr(name::ACT_ID), attr(name::SCENE_ID));
                    let playable = self
                        .acts
                        .iter()
                        .filter(|a| a.id == act)
                        .flat_map(|a| a.scenes.iter())
                        .any(|s| s.id == scene && s.playable);
                    if playable {
                        out.push(Request::PlayScene { act, scene });
                    }
                }
            }
            _ => {}
        }
    }

    /// The scripts said the story changed: how (a word), and the act.
    fn notice(&mut self, ui: &mut Ui, n: &Notice, _out: &mut Vec<Request>) {
        if n.native == notice::UPDATE_STORY_QUESTS {
            let w = |i: usize| n.words.get(i).copied().unwrap_or(0);
            self.update_story_quests(ui, w(0), w(1));
        }
    }

    /// The scripts' list of acts taken, and the acts laid out when the tick came.
    fn update(&mut self, ui: &mut Ui, view: &gameplay::View<'_>, _out: &mut Vec<Request>) {
        if let Some(a) = view.acts {
            if a != self.given.as_slice() {
                self.given = a.to_vec();
            }
        }
        if self.due {
            self.due = false;
            self.reload_acts(ui);
        }
    }

    /// It goes: its acts, rows and listening.
    // D2-02729, D2-02733
    fn close(mut self: Box<Self>, ui: &mut Ui, _out: &mut Vec<Request>) {
        self.acts.clear();
        self.rows.clear();
        if let Some(l) = self.list {
            ui.list_flush(l);
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
