//! What a lever, a pressure plate, a door or a creature sets off, in the inspector and on the
//! world: the selected spawn's target (a click selects it) and a way to pick another, in the world
//! or from what stands nearby; what sets the selected spawn off, each a click away; and a line
//! drawn to each from the selected spawn. A change is one step of the history ([`Op::Link`]).
//!
//! A creature sets off its target only through an Activate step of its emotes: its card says when
//! its emotes have none, and the emotes say beside such a step what it sets off.

use std::path::PathBuf;

use dereth_primitives::Vec3;
use lyceum_foundry::edit::{links, Op, Spawn, Weenie};
use lyceum_foundry::pick;
use lyceum_foundry::sql_ids::{self, Use};

use super::{add, EditorApp, Selection, UiAction};
use crate::view::View;

/// The color of the lines between linked spawns.
const LINE: egui::Color32 = egui::Color32::from_rgb(0xFF, 0x8A, 0xD0);

/// A spawn by its file and guid, as a selection names it.
pub type SpawnRef = (PathBuf, String);

/// The links' state and what was drawn of them last.
#[derive(Debug, Clone, Default)]
pub struct Links {
    /// The spawn whose target the next click in the world picks.
    pub picking: Option<SpawnRef>,
    /// The lines between the selected spawn and what it is linked with, as drawn last, each from
    /// the one that sets the other off.
    pub lines: Vec<((f32, f32), (f32, f32))>,
    /// The card as drawn last: its target, its Remove, its "Sets off…", the nearby list's button
    /// and rows while it is open, and each row of what sets the spawn off.
    pub target: Option<egui::Rect>,
    pub remove: Option<egui::Rect>,
    pub choose: Option<egui::Rect>,
    pub nearby: Option<egui::Rect>,
    pub nearby_rows: Vec<(SpawnRef, egui::Rect)>,
    pub set_off_by: Vec<(SpawnRef, egui::Rect)>,
    /// What the selected spawn is linked with, as last read, and what it was read for.
    cache: Option<(Key, Linked)>,
    /// What a weenie's Activate steps set off, as last gathered, and for which weenie after which
    /// edit; and whether a weenie's emotes have an Activate step, once its rows are read.
    activates: Option<((Weenie, u64, usize), Activates)>,
    has_activate: Option<((Weenie, u64), bool)>,
}

/// The emote action that sets off what its weenie's spawn is linked to.
pub const ACTIVATE: u32 = 15;

/// What an Activate step of a weenie's emotes sets off: what its spawns are linked to, the
/// generators that own its spawns, the generator that makes it (no spawn of it stands), or
/// nothing.
#[derive(Debug, Clone, PartialEq)]
pub enum Activates {
    Targets(Vec<Spawn>),
    Owners(Vec<Spawn>),
    Generator,
    Nothing,
}

/// How many targets the line beside an Activate step names; the rest it counts.
const NAMED: usize = 3;

/// What the links were read for: the spawn, the edits made, and the server's weenies known.
type Key = (SpawnRef, u64, usize);

/// What a spawn is linked with.
#[derive(Debug, Clone, Default)]
struct Linked {
    /// Whether it is what sets things off (or sets something off all the same), and what it sets
    /// off.
    sets: bool,
    target: Option<Spawn>,
    /// What sets it off.
    by: Vec<Spawn>,
}

fn spawn_ref(s: &Spawn) -> SpawnRef {
    (s.file.clone(), s.guid.clone())
}

impl EditorApp {
    /// What spawn `s` is linked with, read again only after an edit.
    fn linked(&mut self, s: &Spawn) -> Linked {
        let key = (
            spawn_ref(s),
            self.stats.edits,
            self.session.server_weenies().len(),
        );
        if let Some((k, l)) = &self.links.cache {
            if *k == key {
                return l.clone();
            }
        }
        let target = self.session.sets_off(s);
        let l = Linked {
            sets: target.is_some() || self.session.sets_things_off(s),
            target,
            by: self.session.set_off_by(s),
        };
        self.links.cache = Some((key, l.clone()));
        l
    }

    /// Whether weenie `w`'s emotes have an Activate step: `None` while its rows are being read.
    fn has_activate(&mut self, w: &Weenie) -> Option<bool> {
        let edits = self.stats.edits;
        if let Some(((of, at), yes)) = &self.links.has_activate {
            if of == w && *at == edits {
                return Some(*yes);
            }
        }
        let text = match (self.session.weenie_file(w), w) {
            (Some((f, _)), _) => self.session.sql_text(&f),
            (None, Weenie::Class(c)) => self.server_class(*c)?,
            (None, Weenie::Symbol(_)) => None,
        };
        let mut yes = false;
        if let Some(text) = text {
            sql_ids::rows(&text, |row| {
                yes |= row.table == lyceum_foundry::content::EMOTE_ACTIONS
                    && row.get("type").and_then(sql_ids::number) == Some(ACTIVATE);
            });
        }
        self.links.has_activate = Some(((w.clone(), edits), yes));
        Some(yes)
    }

    /// What an Activate step of weenie `w`'s emotes sets off: for the selected spawn when it is
    /// one of `w`, else for each spawn of it in the world. Gathered again only after an edit.
    pub(super) fn activates(&mut self, w: &Weenie) -> Activates {
        let key = (
            w.clone(),
            self.stats.edits,
            self.session.server_weenies().len(),
        );
        let selected = self
            .selected_spawn()
            .filter(|s| self.session.spawn_weenie(s).as_ref() == Some(w));
        if selected.is_none() {
            if let Some((k, a)) = &self.links.activates {
                if *k == key {
                    return a.clone();
                }
            }
        }
        let spawns: Vec<Spawn> = match &selected {
            Some(s) => vec![s.clone()],
            None => self.appears_of(w).to_vec(),
        };
        let (mut targets, mut owners): (Vec<Spawn>, Vec<Spawn>) = (Vec::new(), Vec::new());
        let add = |list: &mut Vec<Spawn>, x: Spawn| {
            if !list.iter().any(|y| y.file == x.file && y.guid == x.guid) {
                list.push(x);
            }
        };
        for s in &spawns {
            match self.session.sets_off(s) {
                Some(x) => add(&mut targets, x),
                None => {
                    if let Some(o) = self.session.link_owner(s) {
                        add(&mut owners, o);
                    }
                }
            }
        }
        let a = if !targets.is_empty() {
            Activates::Targets(targets)
        } else if !owners.is_empty() {
            Activates::Owners(owners)
        } else if spawns.is_empty()
            && self
                .session
                .uses_of(w)
                .iter()
                .any(|(_, how)| *how == Use::Generator)
        {
            Activates::Generator
        } else {
            Activates::Nothing
        };
        if selected.is_none() {
            self.links.activates = Some((key, a.clone()));
        }
        a
    }

    /// Beside an Activate step of weenie `w`'s emotes, what it sets off: each target a click
    /// away, else the generator, or nothing.
    pub(super) fn activate_note(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let (key, list) = match self.activates(w) {
            Activates::Targets(list) => ("emotes.activate_sets_off", list),
            Activates::Owners(list) => ("emotes.activate_generator", list),
            Activates::Generator => ("emotes.activate_made_by", Vec::new()),
            Activates::Nothing => ("emotes.activate_nothing", Vec::new()),
        };
        ui.horizontal_wrapped(|ui| {
            ui.weak(t.get(key));
            for x in list.iter().take(NAMED) {
                if ui.link(self.spawn_name(x)).clicked() {
                    out.push(UiAction::GoToSpawn(x.file.clone(), x.guid.clone()));
                }
            }
            if list.len() > NAMED {
                ui.weak(t.fill("emotes.activate_more", &[("n", &(list.len() - NAMED))]));
            }
        });
    }

    /// A spawn's name: its weenie's, else its weenie as written.
    fn spawn_name(&self, s: &Spawn) -> String {
        self.session
            .spawn_weenie(s)
            .and_then(|w| self.weenie_entry(&w))
            .map_or_else(|| s.weenie.clone(), |e| e.name.clone())
    }

    /// The selected spawn's links on its card: what it sets off and the ways to change that, when
    /// it is what sets things off; what sets it off, when anything does.
    pub(super) fn links_card(&mut self, ui: &mut egui::Ui, s: &Spawn, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        self.links.target = None;
        self.links.remove = None;
        self.links.choose = None;
        self.links.nearby = None;
        self.links.nearby_rows.clear();
        self.links.set_off_by.clear();
        let linked = self.linked(s);
        if !linked.sets && linked.by.is_empty() {
            return;
        }
        let me = spawn_ref(s);
        ui.separator();
        if linked.sets {
            ui.horizontal_wrapped(|ui| {
                ui.strong(t.get("inspector.sets_off"));
                match &linked.target {
                    Some(x) => {
                        let r = ui.link(self.spawn_name(x));
                        self.links.target = Some(r.rect);
                        if r.clicked() {
                            out.push(UiAction::Select(Selection::Spawn {
                                file: x.file.clone(),
                                guid: x.guid.clone(),
                            }));
                        }
                        let r = ui.small_button(t.get("inspector.sets_off_remove"));
                        self.links.remove = Some(r.rect);
                        if r.clicked() {
                            out.push(UiAction::Link(me.clone(), None));
                        }
                    }
                    None => {
                        ui.weak(t.get("inspector.sets_off_nothing"));
                    }
                }
            });
            // A creature sets it off only through an Activate step of its emotes.
            let creature = self.session.spawn_weenie(s).filter(|w| {
                self.weenie_entry(w)
                    .is_some_and(|e| e.weenie_type == Some(links::CREATURE))
            });
            if let Some(w) = creature {
                if self.has_activate(&w) == Some(false) {
                    ui.weak(t.get("inspector.sets_off_no_activate"));
                }
            }
            if self.links.picking.as_ref() == Some(&me) {
                ui.label(t.get("inspector.sets_off_pick"));
                ui.horizontal(|ui| {
                    let (rows, target) = (&mut self.links.nearby_rows, &linked.target);
                    let session = &self.session;
                    let names = |x: &Spawn| {
                        session
                            .spawn_weenie(x)
                            .and_then(|w| {
                                self.weenie_at
                                    .get(&w)
                                    .map(|i| self.weenies[*i].name.clone())
                            })
                            .unwrap_or_else(|| x.weenie.clone())
                    };
                    let combo = egui::ComboBox::from_id_salt("sets off nearby")
                        .selected_text(t.get("inspector.sets_off_nearby"))
                        .height(240.0)
                        .show_ui(ui, |ui| {
                            let near = session.link_targets(s);
                            if near.is_empty() {
                                ui.weak(t.get("inspector.sets_off_none_near"));
                            }
                            for x in near {
                                let d = distance(s.frame.origin, x.frame.origin);
                                let on = target
                                    .as_ref()
                                    .is_some_and(|y| y.file == x.file && y.guid == x.guid);
                                let r = ui.selectable_label(
                                    on,
                                    t.fill(
                                        "inspector.sets_off_near",
                                        &[("name", &names(&x)), ("m", &format!("{d:.0}"))],
                                    ),
                                );
                                rows.push((spawn_ref(&x), r.rect));
                                if r.clicked() && !on {
                                    out.push(UiAction::Link(me.clone(), Some(spawn_ref(&x))));
                                }
                            }
                        });
                    self.links.nearby = Some(combo.response.rect);
                    if ui.button(t.get("inspector.sets_off_cancel")).clicked() {
                        out.push(UiAction::PickLinkTarget(None));
                    }
                });
            } else {
                let r = ui.button(t.get("inspector.sets_off_choose"));
                self.links.choose = Some(r.rect);
                if r.clicked() {
                    out.push(UiAction::PickLinkTarget(Some(me.clone())));
                }
            }
        }
        if !linked.by.is_empty() {
            ui.strong(t.get("inspector.set_off_by"));
            for x in &linked.by {
                let r = ui.selectable_label(false, self.spawn_name(x));
                self.links.set_off_by.push((spawn_ref(x), r.rect));
                if r.clicked() {
                    out.push(UiAction::Select(Selection::Spawn {
                        file: x.file.clone(),
                        guid: x.guid.clone(),
                    }));
                }
            }
        }
    }

    /// Spawn `child` made to set off `target`, or nothing, as one step; the picking ends.
    pub(super) fn link_spawn(
        &mut self,
        child: &SpawnRef,
        target: Option<&SpawnRef>,
    ) -> Result<(), String> {
        self.links.picking = None;
        self.apply(
            &Op::Link {
                file: child.0.clone(),
                guid: child.1.clone(),
                target: target.cloned(),
            },
            false,
        )
        .map(|_| ())
    }

    /// A click at `p` while a target is being picked: the spawn under it set off by the one
    /// picking, when it can be; else the picking goes on, and the status bar says what can be.
    /// Whether the click was taken.
    pub(super) fn pick_link_target(&mut self, p: (f32, f32)) -> bool {
        let Some(child) = self.links.picking.clone() else {
            return false;
        };
        let target = match self.picked(p) {
            Some(Selection::Spawn { file, guid }) => self.session.spawn(&file, &guid),
            _ => None,
        };
        let Some(target) = target else {
            self.panels.message = self.say("inspector.sets_off_pick_one");
            return true;
        };
        if !self.session.can_be_set_off(&target) {
            let name = self.spawn_name(&target);
            self.panels.message = self.say_with("inspector.sets_off_cannot", &[("name", &name)]);
            return true;
        }
        if let Err(e) = self.link_spawn(&child, Some(&spawn_ref(&target))) {
            self.panels.message = e;
        }
        true
    }

    /// The lines between selected spawn `s` and what it is linked with: to what it sets off, and
    /// from each that sets it off.
    pub(super) fn draw_links(&mut self, painter: &egui::Painter, view: &View, s: &Spawn) {
        self.links.lines.clear();
        let linked = self.linked(s);
        let at = |x: &Spawn| {
            let o = add(pick::block_origin(x.landblock()), x.frame.origin);
            view.project(Vec3::new(o.x, o.y, o.z + 0.5))
        };
        let Some(here) = at(s) else {
            return;
        };
        let mut pairs: Vec<((f32, f32), (f32, f32))> = Vec::new();
        if let Some(q) = linked.target.as_ref().and_then(at) {
            pairs.push((here, q));
        }
        for x in &linked.by {
            if let Some(q) = at(x) {
                pairs.push((q, here));
            }
        }
        for (a, b) in &pairs {
            let (a, b) = (egui::pos2(a.0, a.1), egui::pos2(b.0, b.1));
            painter.line_segment([a, b], egui::Stroke::new(2.0, LINE));
            painter.circle_filled(b, 4.0, LINE);
        }
        self.links.lines = pairs;
    }
}

/// How far apart two places of one landblock are, in meters.
fn distance(a: Vec3, b: Vec3) -> f32 {
    let (x, y, z) = (a.x - b.x, a.y - b.y, a.z - b.z);
    (x * x + y * y + z * z).sqrt()
}
