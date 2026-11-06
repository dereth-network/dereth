//! The fellows' rows: one row per fellow in a list (the fellowship page's, or the small panel's),
//! kept as the player's scripts tell of the fellowship.
//!
//! [`Fellows`] makes a row for each fellow of a fellowship, the leader moved to the top; each row
//! ([`FellowRow`]) shows its place and name, the fellow's health and vigor (with what players took
//! of them shown past what is left) and, on the page, its level and its share of the experience.
//! A row pressed selects its fellow (pressed twice, a tell to it is started); the keys for the
//! first nine fellows select them; the fellow selected is lit. Unless the player's options keep
//! fellows off the map, each fellow other than the player is put on the map where it is, and taken
//! off when it leaves.

use auberean_text::{StringInfo, Value};
use auberean_ui::listener::ListenerId;
use auberean_ui::manager::global;
use auberean_ui::token::msg;
use auberean_ui::types::button_id;
use auberean_ui::{Handle, Mail, Ui};

use super::{entry, hash_order, notice, set_string_info, Fellow, Fellowship, Request};
use crate::gameplay::{Notice, Record, Request as PlayRequest, PLAY_LAYOUT_ENUM};
use crate::shortcuts::TOOLBAR_LAYOUT_ENUM;
use crate::statusbar::VitalBar;

mod name {
    use auberean_ui::Token;
    pub const TEMPLATE: Token = Token::of("FellowTemplate");
    pub const MINI_TEMPLATE: Token = Token::of("FellowMiniPanelTemplate");
    pub const ID: Token = Token::of("FellowIIDToken");
    pub const NAME_TEXT: Token = Token::of("FellowTemplateNameText");
    pub const LEVEL_TEXT: Token = Token::of("FellowTemplateLevelText");
    pub const PERCENT_TEXT: Token = Token::of("FellowTemplatePercentText");
    pub const HEALTH_METER: Token = Token::of("FellowTemplateHealthMeter");
    pub const HEALTH_TEXT: Token = Token::of("FellowTemplateHealthText");
    pub const PK_DAMAGE_METER: Token = Token::of("FellowTemplatePKDamageMeter");
    pub const PK_DAMAGE_TEXT: Token = Token::of("FellowTemplatePKDamageText");
    pub const VIGOR_METER: Token = Token::of("FellowTemplateVigorMeter");
    pub const VIGOR_TEXT: Token = Token::of("FellowTemplateVigorText");
    pub const PK_VIGOR_LOSS_METER: Token = Token::of("FellowTemplatePKVigorlossMeter");
    pub const PK_VIGOR_LOSS_TEXT: Token = Token::of("FellowTemplatePKVigorlossText");
    pub const LEADER: Token = Token::of("leader");
    pub const NOT_LEADER: Token = Token::of("notleader");
    pub const NUMBER: Token = Token::of("number");
    pub const NAME: Token = Token::of("name");
    pub const VALUE: Token = Token::of("value");
}

/// The keys that select the first nine fellows, in order.
pub mod action {
    pub const SELECT_FELLOW_1: u32 = 0x4100_0053;
    pub const SELECT_FELLOW_9: u32 = 0x4100_005B;
}

/// The map's notices the rows send: a marker put on the map (its kind, an instance id, the cell,
/// the picture, the label), one taken off, every one of a kind taken off.
pub mod marker {
    pub const ADD: u32 = 0x0085 << 16 | 0x0014;
    pub const CLEAR: u32 = 0x0085 << 16 | 0x0015;
    pub const REMOVE: u32 = 0x0085 << 16 | 0x0016;
    /// The fellows' kind of marker.
    pub const FELLOW: u32 = 4;
}

/// The player's option that keeps fellows off the map.
pub const FELLOWS_OFF_MAP: u32 = 0x8000;

/// The fellows' marker's picture, by enumeration value in the layouts' mapping.
const MARKER_PICTURE_ENUM: u32 = 0x4100_0010;
const PICTURE_MAPPING: u32 = 0x4000_0003;

/// A marker put on the map, as the map hears it.
#[must_use]
pub fn add_marker(kind: u32, id: u64, cell: u32, picture: u32, label: &StringInfo) -> Notice {
    #[allow(clippy::cast_possible_truncation)]
    let (hi, lo) = ((id >> 32) as u32, id as u32);
    let mut records = vec![None; 5];
    records.push(Some(Record::Text(label.clone())));
    Notice {
        native: marker::ADD,
        words: vec![kind, hi, lo, cell, picture, 0],
        records,
        ..Notice::default()
    }
}

/// A marker taken off the map, or every one of a kind.
#[must_use]
pub fn remove_marker(kind: u32, id: u64) -> Notice {
    #[allow(clippy::cast_possible_truncation)]
    let (hi, lo) = ((id >> 32) as u32, id as u32);
    Notice {
        native: marker::REMOVE,
        words: vec![kind, hi, lo],
        ..Notice::default()
    }
}

#[must_use]
pub fn clear_markers(kind: u32) -> Notice {
    Notice {
        native: marker::CLEAR,
        words: vec![kind],
        ..Notice::default()
    }
}

/// One fellow's row.
#[derive(Debug)]
pub struct FellowRow {
    pub root: Option<Handle>,
    name_text: Option<Handle>,
    level_text: Option<Handle>,
    percent_text: Option<Handle>,
    health: VitalBar,
    pk_damage: VitalBar,
    vigor: VitalBar,
    pk_vigor_loss: VitalBar,
    /// The small panel's row (no level or share).
    mini: bool,
    pub name: StringInfo,
    /// Its place in the list, from 0.
    pub index: i32,
    pub id: u64,
    pub cell: u32,
    /// The player's object went: no notices heard any more.
    deaf: bool,
}

impl FellowRow {
    /// The row over its element: the fellow's name, where it is; its name written; its health
    /// and vigor bars (what players took shown past what is left, without numbers); on the small
    /// panel the numbers hidden, else its level and no share yet.
    // D2-03039
    pub fn new(
        ui: &mut Ui,
        root: Handle,
        fellow: &Fellow,
        id: u64,
        mini: bool,
        index: i32,
    ) -> Self {
        let child = |ui: &Ui, n| ui.child_recursive(root, n);
        let health = VitalBar::new(
            ui,
            child(ui, name::HEALTH_METER),
            child(ui, name::HEALTH_TEXT),
        );
        let pk_damage = VitalBar::new(
            ui,
            child(ui, name::PK_DAMAGE_METER),
            child(ui, name::PK_DAMAGE_TEXT),
        );
        let vigor = VitalBar::new(
            ui,
            child(ui, name::VIGOR_METER),
            child(ui, name::VIGOR_TEXT),
        );
        let pk_vigor_loss = VitalBar::new(
            ui,
            child(ui, name::PK_VIGOR_LOSS_METER),
            child(ui, name::PK_VIGOR_LOSS_TEXT),
        );
        let mut me = Self {
            root: Some(root),
            name_text: child(ui, name::NAME_TEXT),
            level_text: None,
            percent_text: None,
            health,
            pk_damage,
            vigor,
            pk_vigor_loss,
            mini,
            name: fellow.name.clone(),
            index,
            id,
            cell: fellow.cell,
            deaf: false,
        };
        if let Some(t) = me.name_text {
            let s = me.label(ui);
            set_string_info(ui, t, &s);
        }
        let f = fellow;
        me.health
            .update(ui, u64::from(f.health), u64::from(f.max_health));
        me.pk_damage.set_text_state(ui, false);
        me.pk_damage.update(
            ui,
            u64::from(f.pk_damage.wrapping_add(f.health)),
            u64::from(f.max_health),
        );
        me.vigor
            .update(ui, u64::from(f.vigor), u64::from(f.max_vigor));
        me.pk_vigor_loss.set_text_state(ui, false);
        me.pk_vigor_loss.update(
            ui,
            u64::from(f.pk_vigor_loss.wrapping_add(f.vigor)),
            u64::from(f.max_vigor),
        );
        if mini {
            me.toggle_text_state(ui);
        } else {
            me.level_text = child(ui, name::LEVEL_TEXT);
            if me.level_text.is_none() {
                return me;
            }
            me.update_level(ui, f.level);
            me.percent_text = child(ui, name::PERCENT_TEXT);
            me.update_percentage(ui, 0.0);
        }
        me
    }

    /// Where the fellow is.
    // D2-03026
    pub fn update_cell_id(&mut self, cell: u32) {
        self.cell = cell;
    }

    /// The level (not on the small panel).
    // D2-03027
    pub fn update_level(&self, ui: &mut Ui, level: u32) {
        if self.mini {
            return;
        }
        let mut s = entry(ui, "ID_Fellowship_Level");
        s.add_var(name::VALUE.raw(), Value::UInt(level));
        if let Some(t) = self.level_text {
            set_string_info(ui, t, &s);
        }
    }

    /// The fellow's share of the experience (none written when it has none; not on the small
    /// panel).
    // D2-03028
    pub fn update_percentage(&self, ui: &mut Ui, share: f32) {
        let (false, Some(t)) = (self.mini, self.percent_text) else {
            return;
        };
        ui.clear_all_text(t);
        if share > 0.0 {
            let mut s = entry(ui, "ID_Fellowship_Percentage");
            s.add_var(
                name::VALUE.raw(),
                Value::Float {
                    value: f64::from(share * 100.0),
                    decimals: 0,
                },
            );
            set_string_info(ui, t, &s);
        }
    }

    /// The health's and vigor's numbers shown if hidden, hidden if shown.
    // D2-03029
    pub fn toggle_text_state(&self, ui: &mut Ui) {
        self.vigor.toggle_text(ui);
        self.health.toggle_text(ui);
    }

    /// The row goes: its bars with it, no more notices heard (its element is the list's).
    // D2-03025, D2-03040
    pub fn close(mut self) {
        self.deaf = true;
    }

    /// The player's object went: no more of its notices heard.
    // D2-03035
    pub fn being_deleted(&mut self) {
        self.deaf = true;
    }

    /// A notice about this fellow: its health (and what players took), its greatest health, its
    /// vigor (likewise), its greatest vigor, its level.
    // D2-03030, D2-03031, D2-03032, D2-03033, D2-03034
    pub fn notice(&mut self, ui: &mut Ui, n: &Notice) {
        if self.deaf || n.long(0) != self.id {
            return;
        }
        let (a, b) = (u64::from(n.word_at(2)), u64::from(n.word_at(3)));
        match n.native {
            notice::UPDATE_FELLOW_HEALTH => {
                self.health.update_current(ui, a);
                self.pk_damage.update_current(ui, b);
            }
            notice::UPDATE_FELLOW_MAX_HEALTH => {
                self.health.update_max(ui, a);
                self.pk_damage.update_max(ui, a);
            }
            notice::UPDATE_FELLOW_VIGOR => {
                self.vigor.update_current(ui, a);
                self.pk_vigor_loss.update_current(ui, b);
            }
            notice::UPDATE_FELLOW_MAX_VIGOR => {
                self.vigor.update_max(ui, a);
                self.pk_vigor_loss.update_max(ui, a);
            }
            notice::UPDATE_FELLOW_LEVEL if !self.mini => self.update_level(ui, n.word_at(2)),
            _ => {}
        }
    }

    /// The fellow described again: its bars and its level.
    // D2-03041
    pub fn update_info(&mut self, ui: &mut Ui, f: &Fellow) {
        self.health
            .update(ui, u64::from(f.health), u64::from(f.max_health));
        self.pk_damage.update(
            ui,
            u64::from(f.pk_damage.wrapping_add(f.health)),
            u64::from(f.max_health),
        );
        self.vigor
            .update(ui, u64::from(f.vigor), u64::from(f.max_vigor));
        self.pk_vigor_loss.update(
            ui,
            u64::from(f.pk_vigor_loss.wrapping_add(f.vigor)),
            u64::from(f.max_vigor),
        );
        if !self.mini {
            self.level_text = self
                .root
                .and_then(|r| ui.child_recursive(r, name::LEVEL_TEXT));
            if self.level_text.is_some() {
                self.update_level(ui, f.level);
            }
        }
    }

    /// The leader changed: the name written again.
    // D2-03042
    pub fn set_leader(&self, ui: &mut Ui) {
        if let Some(t) = self.name_text {
            let s = self.label(ui);
            set_string_info(ui, t, &s);
        }
    }

    /// Lit as the selection, or not.
    // D2-03043
    pub fn show_selection_highlight(&self, ui: &mut Ui, on: bool) {
        let Some(t) = self.name_text else { return };
        ui.set_state(t, if on { name::LEADER } else { name::NOT_LEADER });
        let s = self.label(ui);
        set_string_info(ui, t, &s);
        if let Some(r) = self.root {
            ui.update(r);
        }
    }

    /// The row's name: its place in the list (from 1) and the fellow's name.
    // D2-03036
    pub fn label(&self, ui: &Ui) -> StringInfo {
        let mut s = entry(ui, "ID_MiniFellow_Name");
        s.add_var(name::NUMBER.raw(), Value::Int(self.index + 1));
        s.add_var(name::NAME.raw(), Value::Text(Box::new(self.name.clone())));
        s
    }

    /// Its place in the list.
    // D2-03037
    #[must_use]
    pub const fn get_index(&self) -> i32 {
        self.index
    }

    /// A new place in the list: the name written again.
    // D2-03038
    pub fn set_index(&mut self, ui: &mut Ui, index: i32) {
        self.index = index;
        if let Some(t) = self.name_text {
            let s = self.label(ui);
            set_string_info(ui, t, &s);
        }
    }
}

/// What a fellowship's rows need of the game each time: the player's id, the player's options,
/// each fellow's share of the experience as the player's scripts answer.
#[derive(Clone, Copy, Debug, Default)]
pub struct Facts<'a> {
    pub player: u64,
    pub options: Option<u32>,
    pub shares: &'a [(u64, f32)],
}

/// The fellows' rows of one list.
#[derive(Debug)]
pub struct Fellows {
    listener: ListenerId,
    list: Option<Handle>,
    /// The rows, in the order they were added (the client's table walks them in its own order:
    /// [`hash_order`]).
    rows: Vec<FellowRow>,
    /// The small panel's rows.
    mini: bool,
    leader: u64,
    /// The fellow lit as the selection.
    highlighted: u64,
    /// The fellows' marker's picture.
    picture: u32,
}

impl Fellows {
    /// The rows of a fellowship in a list (the small panel's, or the page's): the marker's
    /// picture found; the fellows' notices heard (through their owner), presses on the list and
    /// the keys; every fellow added.
    // D2-03074
    pub fn new(
        ui: &mut Ui,
        list: Option<Handle>,
        fellowship: &Fellowship,
        mini: bool,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) -> Self {
        let listener = ui.new_listener();
        let mut me = Self {
            listener,
            list,
            rows: Vec::new(),
            mini,
            leader: 0,
            highlighted: 0,
            picture: 0,
        };
        if list.is_some() && facts.player != 0 {
            me.picture = ui
                .assets()
                .data_id_by_enum(MARKER_PICTURE_ENUM, PICTURE_MAPPING);
            if let Some(id) = list.and_then(|l| ui.node(l)).map(|n| n.id) {
                ui.listen_for_message(listener, id, msg::MOUSE_PRESS);
            }
            ui.listen_for_global_message(listener, global::KEY_DOWN);
            me.add_all_fellows(ui, fellowship, facts, out);
        }
        me
    }

    /// The rows, in the order they were added.
    #[must_use]
    pub fn rows(&self) -> &[FellowRow] {
        &self.rows
    }

    fn find(&self, id: u64) -> Option<usize> {
        self.rows.iter().position(|r| r.id == id)
    }

    /// The rows in the order the client's table walks them.
    fn walk(&self) -> Vec<usize> {
        let ids: Vec<u64> = self.rows.iter().map(|r| r.id).collect();
        hash_order(&ids)
    }

    /// Select the fellow at a place in the list: the row selected, the fellow made the
    /// selection.
    // D2-03044
    pub fn select_fellow_by_idx(&self, ui: &mut Ui, index: usize, out: &mut Vec<PlayRequest>) {
        let Some(l) = self.list else { return };
        ui.list_select_index(l, index, true);
        let id = ui
            .list_item(l, index)
            .and_then(|i| ui.node(i))
            .and_then(|n| n.instance_id_attr(name::ID))
            .unwrap_or(0);
        if id != 0 {
            out.push(PlayRequest::SetSelection(id));
        }
    }

    /// What the rows heard: a fellow's key (the first nine); a press on a row (its fellow
    /// selected; pressed twice, a tell to it started).
    // D2-03045, D2-03056
    pub fn hear(&mut self, ui: &mut Ui, out: &mut Vec<PlayRequest>) {
        for m in ui.take_mail(self.listener) {
            match m {
                Mail::Global {
                    what: global::KEY_DOWN,
                    param,
                } if (action::SELECT_FELLOW_1..=action::SELECT_FELLOW_9).contains(&param) => {
                    let index = (param - action::SELECT_FELLOW_1) as usize;
                    self.select_fellow_by_idx(ui, index, out);
                }
                Mail::Element { what, param, .. } if what == msg::MOUSE_PRESS => {
                    let Some(l) = self.list else { continue };
                    let Some(item) = ui.listbox_item_under_mouse(l) else {
                        continue;
                    };
                    let id = ui
                        .node(item)
                        .and_then(|n| n.instance_id_attr(name::ID))
                        .unwrap_or(0);
                    if id == 0 {
                        continue;
                    }
                    out.push(PlayRequest::SetSelection(id));
                    if param == button_id::DOUBLE_LEFT {
                        if let Some(i) = self.find(id) {
                            let s = self.rows[i].name.clone();
                            let who = super::write(ui, &s);
                            out.push(PlayRequest::Social(Request::AssistedTell(who)));
                        }
                    }
                }
                _ => {}
            }
        }
    }

    /// Every row's health and vigor numbers shown or hidden.
    // D2-03048
    pub fn toggle_text_state(&self, ui: &mut Ui) {
        for i in self.walk() {
            self.rows[i].toggle_text_state(ui);
        }
    }

    /// A new leader: the old one's and the new one's names written again; the new one moved to
    /// the list's top, the rows above its old place moved down one.
    // D2-03057
    pub fn set_leader(&mut self, ui: &mut Ui, leader: u64) {
        let Some(l) = self.list else { return };
        let old = std::mem::replace(&mut self.leader, leader);
        if let Some(i) = self.find(old) {
            self.rows[i].set_leader(ui);
        }
        let Some(i) = self.find(leader) else { return };
        self.rows[i].set_leader(ui);
        if let Some(root) = self.rows[i].root {
            if let Some(at) = ui.list_index_of(l, root) {
                ui.list_multi_remove(l, at);
            }
            ui.list_multi_insert(l, root, 0);
            ui.list_changes_done(l);
        }
        let was = self.rows[i].get_index();
        self.rows[i].set_index(ui, 0);
        for k in self.walk() {
            if self.rows[k].id != leader && self.rows[k].get_index() < was {
                let n = self.rows[k].get_index() + 1;
                self.rows[k].set_index(ui, n);
            }
        }
    }

    /// The selection changed: the fellow lit before no longer, the new one (when a fellow) lit.
    // D2-03059
    pub fn selection_changed(&mut self, ui: &mut Ui, id: u64) {
        if self.highlighted != 0 {
            if let Some(i) = self.find(self.highlighted) {
                self.rows[i].show_selection_highlight(ui, false);
            }
            self.highlighted = 0;
        }
        if let Some(i) = self.find(id) {
            self.rows[i].show_selection_highlight(ui, true);
            self.highlighted = id;
        }
    }

    /// Each fellow's share of the experience, as the player's scripts answer; walking them stops
    /// at the first fellow with no row.
    // D2-03063
    pub fn update_xp(&mut self, ui: &mut Ui, facts: &Facts<'_>) {
        if facts.player == 0 {
            return;
        }
        for (id, share) in facts.shares {
            let Some(i) = self.find(*id) else { return };
            self.rows[i].update_percentage(ui, *share);
        }
    }

    /// A fellow taken off: its row deleted, the rows below moved up one; unless fellows are kept
    /// off the map, its marker taken off (not the player's).
    // D2-03065
    pub fn remove_fellow(
        &mut self,
        ui: &mut Ui,
        id: u64,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) {
        let Some(l) = self.list else { return };
        let Some(i) = self.find(id) else { return };
        let row = self.rows.remove(i);
        if let Some(r) = row.root {
            ui.list_delete(l, r);
        }
        let was = row.get_index();
        row.close();
        for k in self.walk() {
            if was < self.rows[k].get_index() {
                let n = self.rows[k].get_index() - 1;
                self.rows[k].set_index(ui, n);
            }
        }
        if facts.player != 0
            && id != facts.player
            && facts.options.is_some_and(|o| o & FELLOWS_OFF_MAP == 0)
        {
            out.push(PlayRequest::Notice(remove_marker(marker::FELLOW, id)));
        }
    }

    /// Every fellow taken off: the list emptied; unless fellows are kept off the map, the
    /// fellows' markers taken off.
    // D2-03066
    pub fn remove_all_fellows(
        &mut self,
        ui: &mut Ui,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) {
        for r in self.rows.drain(..) {
            r.close();
        }
        let Some(l) = self.list else { return };
        ui.list_flush(l);
        if facts.player != 0 && facts.options.is_some_and(|o| o & FELLOWS_OFF_MAP == 0) {
            out.push(PlayRequest::Notice(clear_markers(marker::FELLOW)));
        }
    }

    /// Where a fellow is: kept; unless fellows are kept off the map, its marker put there (not
    /// the player's).
    // D2-03067
    pub fn update_fellow_cell_id(
        &mut self,
        id: u64,
        cell: u32,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) {
        let Some(i) = self.find(id) else { return };
        self.rows[i].update_cell_id(cell);
        if facts.player != 0
            && id != facts.player
            && facts.options.is_some_and(|o| o & FELLOWS_OFF_MAP == 0)
        {
            let label = self.rows[i].name.clone();
            out.push(PlayRequest::Notice(add_marker(
                marker::FELLOW,
                id,
                cell,
                self.picture,
                &label,
            )));
        }
    }

    /// A fellow gone: its row taken off; the shares asked again.
    // D2-03068
    pub fn clear_fellow(
        &mut self,
        ui: &mut Ui,
        id: u64,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) {
        self.remove_fellow(ui, id, facts, out);
        self.update_xp(ui, facts);
    }

    /// The player's options changed: unless fellows are kept off the map, each fellow (not the
    /// player) put on the map where it is; else the fellows' markers taken off.
    // D2-03069
    pub fn reload_gameplay_options(&self, facts: &Facts<'_>, out: &mut Vec<PlayRequest>) {
        let (true, Some(o)) = (facts.player != 0, facts.options) else {
            return;
        };
        if o & FELLOWS_OFF_MAP != 0 {
            out.push(PlayRequest::Notice(clear_markers(marker::FELLOW)));
            return;
        }
        for i in self.walk() {
            let r = &self.rows[i];
            if r.id != facts.player {
                out.push(PlayRequest::Notice(add_marker(
                    marker::FELLOW,
                    r.id,
                    r.cell,
                    self.picture,
                    &r.name,
                )));
            }
        }
    }

    /// The rows go: nothing heard any more, every fellow taken off.
    // D2-03070
    pub fn close(mut self, ui: &mut Ui, facts: &Facts<'_>, out: &mut Vec<PlayRequest>) {
        ui.drop_listener(self.listener);
        self.remove_all_fellows(ui, facts, out);
    }

    /// A fellow added: its row made from the list's template (the small panel's or the page's),
    /// knowing its place, marked with the fellow's id, put at the list's end; unless fellows are
    /// kept off the map, its marker put on the map (not the player's). Whether it was added.
    // D2-03071
    pub fn add_fellow(
        &mut self,
        ui: &mut Ui,
        id: u64,
        fellow: &Fellow,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) -> bool {
        let Some(l) = self.list else { return false };
        let (layout, template) = if self.mini {
            (PLAY_LAYOUT_ENUM, name::MINI_TEMPLATE)
        } else {
            (TOOLBAR_LAYOUT_ENUM, name::TEMPLATE)
        };
        let Some(root) = ui.frame_create_element(layout, template) else {
            return false;
        };
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let index = ui.list_items(l).len() as i32;
        let info = FellowRow::new(ui, root, fellow, id, self.mini, index);
        ui.set_attr(root, name::ID, auberean_ui::Value::InstanceId(id));
        ui.list_add(l, root);
        let added = self.find(id).is_none();
        if added {
            self.rows.push(info);
        }
        if facts.player != 0
            && id != facts.player
            && facts.options.is_some_and(|o| o & FELLOWS_OFF_MAP == 0)
        {
            out.push(PlayRequest::Notice(add_marker(
                marker::FELLOW,
                id,
                fellow.cell,
                self.picture,
                &fellow.name,
            )));
        }
        added
    }

    /// Every fellow of a fellowship added, in the order its table walks them; the leader set;
    /// the shares asked. Whether every one was added.
    // D2-03072
    pub fn add_all_fellows(
        &mut self,
        ui: &mut Ui,
        fellowship: &Fellowship,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) -> bool {
        let ids: Vec<u64> = fellowship.fellows.iter().map(|(id, _)| *id).collect();
        let mut all = true;
        for i in hash_order(&ids) {
            let (id, f) = &fellowship.fellows[i];
            all &= self.add_fellow(ui, *id, f, facts, out);
        }
        self.set_leader(ui, fellowship.leader);
        self.update_xp(ui, facts);
        all
    }

    /// A fellow described: its row made when it has none, else brought up to date; the shares
    /// asked again.
    // D2-03073
    pub fn update_fellow(
        &mut self,
        ui: &mut Ui,
        id: u64,
        fellow: &Fellow,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) {
        match self.find(id) {
            None => {
                self.add_fellow(ui, id, fellow, facts, out);
            }
            Some(i) => self.rows[i].update_info(ui, fellow),
        }
        self.update_xp(ui, facts);
    }

    /// A notice of the player's scripts the rows hear: a fellow described or gone, where one is,
    /// its level (the shares asked again), the leader, the selection, the player's options; and
    /// each row's own (its bars, its level).
    // D2-03058, D2-03064
    pub fn notice(
        &mut self,
        ui: &mut Ui,
        n: &Notice,
        facts: &Facts<'_>,
        out: &mut Vec<PlayRequest>,
    ) {
        for r in &mut self.rows {
            r.notice(ui, n);
        }
        match n.native {
            notice::UPDATE_FELLOW => {
                if let Some(Some(Record::Fellow(f))) = n.records.get(2) {
                    let f = f.as_ref().clone();
                    self.update_fellow(ui, n.long(0), &f, facts, out);
                }
            }
            notice::CLEAR_FELLOW => self.clear_fellow(ui, n.long(0), facts, out),
            notice::UPDATE_FELLOW_CELL => {
                self.update_fellow_cell_id(n.long(0), n.word_at(2), facts, out);
            }
            notice::UPDATE_FELLOW_LEVEL => self.update_xp(ui, facts),
            notice::UPDATE_LEADER => self.set_leader(ui, n.long(0)),
            notice::SELECTION_CHANGED => self.selection_changed(ui, n.long(0)),
            notice::OPTIONS_CHANGED => self.reload_gameplay_options(facts, out),
            _ => {}
        }
    }

    /// The player's object went: the rows hear no more of its notices.
    pub fn being_deleted(&mut self) {
        for r in &mut self.rows {
            r.being_deleted();
        }
    }
}
