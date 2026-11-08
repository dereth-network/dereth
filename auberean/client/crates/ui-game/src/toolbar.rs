//! The toolbar (centered at the screen's foot): the buttons of the toolbar's windows, the shortcut
//! bar and the chat windows hung from it, the peace and war buttons, the dismount and
//! consignment buttons on its wings.
//!
//! [`Toolbar`] makes the toolbar and the seven windows' fields (examination, inventory, map,
//! options, quests, social, skills), then its panels, each with the button that shows it (its
//! tooltip "Show ..." or "Hide ..." as the window is), the window's field and the key that
//! shows and hides it. The dismount and consignment buttons stay hidden until the player's
//! scripts say the player is mounted, or has a sale to collect (the wing changes with them and
//! the consignment alert sounds); pressed, they ask the scripts to dismount and to show the sales.
//! The help key and the plug-in manager's key open and close those panels.
//!
//! Each window is its own panel (made in [`Toolbar::setup_children`]): its button and key show and
//! hide it, its close button hides it, and it comes to the front of the windows as it shows.

use std::any::Any;

use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::{msg, state};
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Token, Ui};

use crate::chat::ChatUi;
use crate::examination::Examination;
use crate::flow;
use crate::gameplay::{self, notice, Child, Notice, Request, View};
use crate::quests::QuestWindow;
use crate::shortcuts::{Shortcuts, TOOLBAR_LAYOUT_ENUM};
use crate::skills::SkillWindow;

mod name {
    use auberean_ui::Token;
    pub const TOOLBAR_FIELD: Token = Token::of("ToolbarField");
    pub const EXAMINATION_FIELD: Token = Token::of("ExaminationField");
    pub const INVENTORY_FIELD: Token = Token::of("InventoryField");
    pub const NAVIGATION_FIELD: Token = Token::of("NavigationField");
    pub const OPTION_FIELD: Token = Token::of("OptionField");
    pub const QUEST_FIELD: Token = Token::of("QuestField");
    pub const SOCIAL_FIELD: Token = Token::of("SocialField");
    pub const SKILL_FIELD: Token = Token::of("SkillField");
    pub const DISMOUNT_BUTTON: Token = Token::of("DismountButton");
    pub const CONSIGNMENTS_BUTTON: Token = Token::of("ConsignmentsButton");
    pub const LEFT_WING_FIELD: Token = Token::of("LeftWingField");
    pub const RIGHT_WING_FIELD: Token = Token::of("RightWingField");
    pub const ALERT: Token = Token::of("alert");
    pub const PRECLUDE_HEIGHT: Token = Token::of("precludeheight");
    pub const VALUE: Token = Token::of("value");
    pub const CONSIGNMENTS: Token = Token::of("consignments");
    pub const DISMOUNT: Token = Token::of("dismount");
}

/// The interface actions the toolbar hears.
pub mod action {
    /// The toolbar's windows' keys.
    pub const EXAMINATION_PANEL: u32 = 0x4100_0001;
    pub const INVENTORY_PANEL: u32 = 0x4100_0002;
    pub const NAVIGATION_PANEL: u32 = 0x4100_0003;
    pub const OPTION_PANEL: u32 = 0x4100_0004;
    pub const QUEST_PANEL: u32 = 0x4100_0005;
    pub const SKILL_PANEL: u32 = 0x4100_0006;
    pub const SOCIAL_PANEL: u32 = 0x4100_0007;
    /// The help panel's key and the plug-in manager's.
    pub const HELP: u32 = 0x4000_00BB;
    pub const PLUGIN_MANAGER: u32 = 0x4000_00BC;
}

/// A toolbar window's parts, by name: its field, its button and the message the button's press
/// shows it with, its close button, and its tooltip's string.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WindowParts {
    pub field: &'static str,
    pub button: &'static str,
    /// The quests' button shows them as it is let go; the others' when let go on the button.
    pub on_release: bool,
    pub close: &'static str,
    pub tooltip: &'static str,
    pub keypress: u32,
}

/// The toolbar's windows.
pub mod windows {
    use super::{action, WindowParts};
    pub const SKILL: WindowParts = WindowParts {
        field: "SkillField",
        button: "SkillButton",
        on_release: false,
        close: "SkillCloseButton",
        tooltip: "ID_Toolbar_TT_Skill",
        keypress: action::SKILL_PANEL,
    };
    pub const OPTION: WindowParts = WindowParts {
        field: "OptionField",
        button: "OptionButton",
        on_release: false,
        close: "OptionCloseButton",
        tooltip: "ID_Toolbar_TT_Option",
        keypress: action::OPTION_PANEL,
    };
    pub const INVENTORY: WindowParts = WindowParts {
        field: "InventoryField",
        button: "InventoryButton",
        on_release: false,
        close: "InventoryCloseButton",
        tooltip: "ID_Toolbar_TT_Inventory",
        keypress: action::INVENTORY_PANEL,
    };
    pub const QUEST: WindowParts = WindowParts {
        field: "QuestField",
        button: "QuestButton",
        on_release: true,
        close: "QuestCloseButton",
        tooltip: "ID_Toolbar_TT_Quest",
        keypress: action::QUEST_PANEL,
    };
    pub const NAVIGATION: WindowParts = WindowParts {
        field: "NavigationField",
        button: "NavigationButton",
        on_release: false,
        close: "NavigationCloseButton",
        tooltip: "ID_Toolbar_TT_Navigation",
        keypress: action::NAVIGATION_PANEL,
    };
    pub const EXAMINATION: WindowParts = WindowParts {
        field: "ExaminationField",
        button: "ExaminationButton",
        on_release: false,
        close: "ExaminationCloseButton",
        tooltip: "ID_Toolbar_TT_Examination",
        keypress: action::EXAMINATION_PANEL,
    };
    pub const SOCIAL: WindowParts = WindowParts {
        field: "SocialField",
        button: "SocialButton",
        on_release: false,
        close: "SocialCloseButton",
        tooltip: "ID_Toolbar_TT_Social",
        keypress: action::SOCIAL_PANEL,
    };
}

/// One of the toolbar's panels: the panel, its button on the toolbar, its window's field, its
/// tooltip's string, the key that shows and hides it (0 for none).
#[derive(Debug)]
pub struct ToolbarChild {
    pub child: Box<dyn Child>,
    pub button: Option<Handle>,
    pub field: Token,
    pub string: Token,
    pub keypress: u32,
    /// Its number among the toolbar's panels, by which it is registered for its notices.
    pub id: u32,
}

/// Who hears a notice on the toolbar: the toolbar itself, or a panel of its own by its number.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Handler {
    Own,
    Panel(u32),
}

/// The toolbar's panel.
#[derive(Debug)]
pub struct Toolbar {
    frame: FrameId,
    /// The toolbar.
    pub root: Option<Handle>,
    /// The windows' fields: examination, inventory, map, options, quests, social, skills.
    fields: Vec<Handle>,
    dismount: Option<Handle>,
    consignments: Option<Handle>,
    left_wing: Option<Handle>,
    right_wing: Option<Handle>,
    /// The consignment alert's sound.
    alert: u32,
    /// The help panel was open when the toolbar was last hidden.
    help_was_open: bool,
    /// The help panel and the plug-in manager are open (as the toolbar asked).
    help: bool,
    plugins: bool,
    /// The panels, in the toolbar's list (the last put first).
    children: Vec<ToolbarChild>,
    /// Who hears which notice: its panels as they were made, then the toolbar.
    registry: crate::notices::Registry<Handler>,
}

impl Toolbar {
    /// The notices the toolbar registers for once its panels are made, by their own numbers.
    pub const NOTICES: &'static [u32] = &[0x496, 0x46E];
}

impl Toolbar {
    /// The toolbar under the play screen: the toolbar and the windows' fields made, its panels,
    /// their tooltips; the dismount and consignment buttons hidden; the interface keys and the
    /// two buttons heard; shown.
    // D2-02467
    pub fn new(ui: &mut Ui, parent: FrameId, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_child_frame(parent);
        let make = |ui: &mut Ui, n| ui.frame_create_root(frame, TOOLBAR_LAYOUT_ENUM, n);
        let root = make(ui, name::TOOLBAR_FIELD);
        let fields = [
            name::EXAMINATION_FIELD,
            name::INVENTORY_FIELD,
            name::NAVIGATION_FIELD,
            name::OPTION_FIELD,
            name::QUEST_FIELD,
            name::SOCIAL_FIELD,
            name::SKILL_FIELD,
        ]
        .into_iter()
        .filter_map(|n| make(ui, n))
        .collect();
        let mut me = Self {
            frame,
            root,
            fields,
            dismount: None,
            consignments: None,
            left_wing: None,
            right_wing: None,
            alert: 0,
            help_was_open: false,
            help: false,
            plugins: false,
            children: Vec::new(),
            registry: crate::notices::Registry::new(),
        };
        me.setup_children(ui);
        me.registry
            .register_all(&crate::notices::keys(Self::NOTICES), Handler::Own);
        me.manage_children(ui);
        let part = |ui: &Ui, n| root.and_then(|r| ui.child_recursive(r, n));
        me.dismount = part(ui, name::DISMOUNT_BUTTON);
        if let Some(d) = me.dismount {
            ui.set_visible(d, false);
        }
        me.consignments = part(ui, name::CONSIGNMENTS_BUTTON);
        if let Some(c) = me.consignments {
            ui.set_visible(c, false);
            me.alert = ui
                .node(c)
                .and_then(|n| n.data_id_attr(name::ALERT))
                .unwrap_or(0);
        }
        me.right_wing = part(ui, name::RIGHT_WING_FIELD);
        me.left_wing = part(ui, name::LEFT_WING_FIELD);
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_global_message(l, global::KEY_DOWN);
            ui.listen_for_message(l, name::DISMOUNT_BUTTON, msg::MOUSE_RELEASE);
            ui.listen_for_message(l, name::CONSIGNMENTS_BUTTON, msg::MOUSE_RELEASE);
        }
        me.show(ui, true, out);
        me
    }

    /// The panels: the shortcut bar (on the toolbar), the chat windows, and the seven windows,
    /// each with its button, field, tooltip and key; each window's field heard as it shows and
    /// hides.
    // D2-02463
    pub fn setup_children(&mut self, ui: &mut Ui) {
        let f = self.frame;
        let shortcuts = Shortcuts::new(ui, f, self.root);
        let chat = ChatUi::new(ui, f);
        let plain = |child: Box<dyn Child>, id| ToolbarChild {
            child,
            button: None,
            field: Token(0),
            string: Token(0),
            keypress: 0,
            id,
        };
        // Made in this order.
        let skill = ToolbarChild {
            child: Box::new(SkillWindow::new(ui, f)),
            button: ui.element(Token::of(windows::SKILL.button)),
            field: Token::of(windows::SKILL.field),
            string: Token::of(windows::SKILL.tooltip),
            keypress: windows::SKILL.keypress,
            id: 2,
        };
        let option = ToolbarChild {
            child: Box::new(crate::options::OptionWindow::new(ui, f)),
            button: ui.element(Token::of(windows::OPTION.button)),
            field: Token::of(windows::OPTION.field),
            string: Token::of(windows::OPTION.tooltip),
            keypress: windows::OPTION.keypress,
            id: 3,
        };
        let inventory = ToolbarChild {
            child: Box::new(crate::inventory::Inventory::new(ui, f, None)),
            button: ui.element(Token::of(windows::INVENTORY.button)),
            field: Token::of(windows::INVENTORY.field),
            string: Token::of(windows::INVENTORY.tooltip),
            keypress: windows::INVENTORY.keypress,
            id: 4,
        };
        let quest = ToolbarChild {
            child: Box::new(QuestWindow::new(ui, f)),
            button: ui.element(Token::of(windows::QUEST.button)),
            field: Token::of(windows::QUEST.field),
            string: Token::of(windows::QUEST.tooltip),
            keypress: windows::QUEST.keypress,
            id: 5,
        };
        let navigation = ToolbarChild {
            child: Box::new(crate::map::Navigation::new(ui, f)),
            button: ui.element(Token::of(windows::NAVIGATION.button)),
            field: Token::of(windows::NAVIGATION.field),
            string: Token::of(windows::NAVIGATION.tooltip),
            keypress: windows::NAVIGATION.keypress,
            id: 6,
        };
        let examination = ToolbarChild {
            child: Box::new(Examination::new(ui, f, &mut Vec::new())),
            button: ui.element(Token::of(windows::EXAMINATION.button)),
            field: Token::of(windows::EXAMINATION.field),
            string: Token::of(windows::EXAMINATION.tooltip),
            keypress: windows::EXAMINATION.keypress,
            id: 7,
        };
        let social = ToolbarChild {
            child: Box::new(crate::social::SocialWindow::new(ui, f)),
            button: ui.element(Token::of(windows::SOCIAL.button)),
            field: Token::of(windows::SOCIAL.field),
            string: Token::of(windows::SOCIAL.tooltip),
            keypress: windows::SOCIAL.keypress,
            id: 8,
        };
        // Put in the list in this order, each at its front.
        let put = [
            plain(Box::new(chat), 1),
            examination,
            inventory,
            navigation,
            option,
            quest,
            plain(Box::new(shortcuts), 0),
            skill,
            social,
        ];
        for c in put {
            self.children.insert(0, c);
        }
        // Each registered for its notices in the order they were made.
        let mut made: Vec<(u32, Vec<u32>)> = self
            .children
            .iter()
            .map(|c| (c.id, c.child.notices()))
            .collect();
        made.sort_by_key(|(id, _)| *id);
        for (id, notices) in made {
            self.registry.register_all(&notices, Handler::Panel(id));
        }
        if let Some(l) = ui.frame_listener(f) {
            for c in &self.children {
                if c.field != Token(0) {
                    ui.listen_for_message(l, c.field, msg::SET_SHOW_STATE);
                }
            }
        }
    }

    /// Each window's button's tooltip: its string, saying "Show" or "Hide" as the window is.
    // D2-02455
    pub fn manage_children(&self, ui: &mut Ui) {
        let table = ui
            .assets()
            .data_id_by_enum(flow::MESSAGES, flow::TABLE_MAPPING);
        for c in &self.children {
            let Some(b) = c.button else { continue };
            let which = if c.child.shown(ui) {
                "ID_Toolbar_TT_Hide"
            } else {
                "ID_Toolbar_TT_Show"
            };
            let mut info = auberean_text::StringInfo::entry(table, c.string.raw());
            info.variables.push(auberean_text::Variable {
                id: name::VALUE.raw(),
                value: auberean_text::Value::Text(Box::new(auberean_text::StringInfo::entry(
                    0,
                    Token::of(which).raw(),
                ))),
            });
            if let Some(text) = ui.assets().write(&info) {
                ui.set_tooltip(b, &text);
            }
        }
    }

    /// Show or hide every panel a key is for.
    // D2-02456
    pub fn child_key_press(&mut self, ui: &mut Ui, key: u32, out: &mut Vec<Request>) {
        for c in &mut self.children {
            if c.keypress == key {
                let shown = c.child.shown(ui);
                c.child.show(ui, !shown, out);
            }
        }
    }

    /// The help panel opened or closed.
    // D2-02449
    pub fn show_key_stone(&mut self, open: bool, out: &mut Vec<Request>) {
        self.help = open;
        out.push(Request::Help { open });
    }

    /// The plug-in manager opened or closed.
    // D2-02450
    pub fn show_plugin(&mut self, open: bool, out: &mut Vec<Request>) {
        self.plugins = open;
        out.push(Request::PluginManager { open });
    }

    /// The player's scripts say a sale waits to be collected (the consignment button shown, the
    /// left wing lit, the alert sounded) or none does.
    // D2-02451
    pub fn show_sale_reminder(&self, ui: &mut Ui, show: bool, out: &mut Vec<Request>) {
        let (Some(c), Some(w)) = (self.consignments, self.left_wing) else {
            return;
        };
        if show {
            ui.set_visible(c, true);
            ui.set_state(w, name::CONSIGNMENTS);
            if self.alert != 0 {
                out.push(Request::Sound(self.alert));
            }
        } else {
            ui.set_state(w, state::STATELESS);
            ui.set_visible(c, false);
        }
    }

    /// The player's scripts say the player is mounted (the dismount button shown, the right wing
    /// lit) or not.
    // D2-02452
    pub fn update_mount_state(&self, ui: &mut Ui, mounted: bool) {
        let (Some(d), Some(w)) = (self.dismount, self.right_wing) else {
            return;
        };
        if mounted {
            ui.set_visible(d, true);
            ui.set_state(w, name::DISMOUNT);
        } else {
            ui.set_state(w, state::STATELESS);
            ui.set_visible(d, false);
        }
    }

    /// The panels, to reach one.
    #[must_use]
    pub fn children(&self) -> &[ToolbarChild] {
        &self.children
    }

    /// The first panel of a kind.
    #[must_use]
    pub fn child<T: Child>(&self) -> Option<&T> {
        self.children
            .iter()
            .find_map(|c| c.child.as_any().downcast_ref::<T>())
    }

    /// The first panel of a kind, to change.
    pub fn child_mut<T: Child>(&mut self) -> Option<&mut T> {
        self.children
            .iter_mut()
            .find_map(|c| c.child.as_any_mut().downcast_mut::<T>())
    }

    /// The window with this field.
    #[must_use]
    pub fn window(&self, field: &str) -> Option<&ToolbarChild> {
        self.children.iter().find(|c| c.field == Token::of(field))
    }

    /// The panels go: each one, and the windows' fields no longer heard.
    // D2-02465
    pub fn cleanup_children(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let l = ui.frame_listener(self.frame);
        for c in self.children.drain(..) {
            if let (Some(l), true) = (l, c.field != Token(0)) {
                ui.unregister_for_message(
                    auberean_ui::Listener::Other(l),
                    c.field,
                    msg::SET_SHOW_STATE,
                );
            }
            c.child.close(ui, out);
        }
    }
}

impl Child for Toolbar {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Shown or hidden: the toolbar with it; how much of the screen's foot it covers said; the
    /// help panel closed with it (remembered) and opened again with it; the shortcut bar with it.
    // D2-02453
    fn show(&mut self, ui: &mut Ui, on: bool, out: &mut Vec<Request>) {
        ui.frame_show(self.frame, on);
        // The panels hear their flags at once.
        for c in &mut self.children {
            gameplay::hear(c.child.as_mut(), ui, out);
        }
        let shown = ui.frame_shown(self.frame);
        let Some(r) = self.root else { return };
        ui.set_visible(r, shown);
        let height = ui
            .node(r)
            .and_then(|n| n.int_attr(name::PRECLUDE_HEIGHT))
            .unwrap_or(0);
        // D2-00321
        out.push(Request::PrecludeFromBottom(if shown { height } else { 0 }));
        if shown {
            let again = self.help_was_open;
            self.show_key_stone(again, out);
        } else {
            self.help_was_open = self.help;
            self.show_key_stone(false, out);
        }
        if let Some(s) = self.child_mut::<Shortcuts>() {
            s.show(ui, shown, out);
        }
    }

    /// The interface keys while the toolbar shows: the plug-in manager's and the help panel's
    /// keys, the windows' keys (and the right button, the examination's); a window's field shown
    /// or hidden (the tooltips follow); the dismount and consignment buttons.
    // D2-02457, D2-02458
    fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Global {
                what: global::KEY_DOWN,
                param,
            } => {
                if !ui.frame_shown(self.frame) {
                    return;
                }
                match param {
                    action::PLUGIN_MANAGER => {
                        let open = !self.plugins;
                        self.show_plugin(open, out);
                    }
                    button_id::RIGHT => self.child_key_press(ui, action::EXAMINATION_PANEL, out),
                    action::HELP => {
                        let open = !self.help;
                        self.show_key_stone(open, out);
                    }
                    k if k > 0x4100_0000 && k < 0x4100_0008 => self.child_key_press(ui, k, out),
                    _ => {}
                }
            }
            Mail::Element { element, what, .. } => {
                if what == msg::SET_SHOW_STATE {
                    self.manage_children(ui);
                }
                if what == msg::MOUSE_RELEASE {
                    if element == name::DISMOUNT_BUTTON {
                        out.push(Request::Dismount);
                    } else if element == name::CONSIGNMENTS_BUTTON {
                        out.push(Request::ShowSales);
                    }
                }
            }
            _ => {}
        }
    }

    /// Its panels' notices, then its own, as each registered.
    fn notices(&self) -> Vec<u32> {
        let mut made: Vec<&ToolbarChild> = self.children.iter().collect();
        made.sort_by_key(|c| c.id);
        crate::notices::union(
            made.into_iter()
                .map(|c| c.child.notices())
                .chain([crate::notices::keys(Self::NOTICES)]),
        )
    }

    fn notice(&mut self, ui: &mut Ui, n: &Notice, out: &mut Vec<Request>) {
        for h in self.registry.handlers(n.native).to_vec() {
            match h {
                Handler::Own => match n.native {
                    notice::UPDATE_MOUNT_STATE => self.update_mount_state(ui, n.word() != 0),
                    notice::SHOW_SALE_REMINDER => self.show_sale_reminder(ui, n.word() != 0, out),
                    _ => {}
                },
                Handler::Panel(id) => {
                    if let Some(c) = self.children.iter_mut().find(|c| c.id == id) {
                        c.child.notice(ui, n, out);
                    }
                }
            }
        }
    }

    /// Its panels' frame; then what the toolbar heard of them (a window shown or hidden), at
    /// once, as the client's messages reach their listeners when sent.
    fn update(&mut self, ui: &mut Ui, view: &View<'_>, out: &mut Vec<Request>) {
        for c in &mut self.children {
            gameplay::drive(c.child.as_mut(), ui, view, &[], out);
        }
        for m in crate::camera::take(ui, self.frame) {
            match m {
                Mail::Show { shown } => self.show(ui, shown, out),
                m => self.listen(ui, &m, out),
            }
        }
    }

    /// The toolbar goes: its panels, its toolbar and windows' fields, the help panel closed.
    // D2-02468
    fn close(mut self: Box<Self>, ui: &mut Ui, out: &mut Vec<Request>) {
        self.cleanup_children(ui, out);
        for r in self.root.into_iter().chain(self.fields.iter().copied()) {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
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
