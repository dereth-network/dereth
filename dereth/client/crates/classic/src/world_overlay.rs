//! What the classic interface draws over the game viewport: the name and hint of the object
//! under the pointer, the transient message line that wipes away, and the flash of light on a
//! newly selected object.
use crate::int::i32_from;
use crate::Command;
use dereth_client_model::{inventory::use_object::ItemUses, weenie::NameType, World};
use dereth_primitives::num::{math, to_i32, to_i32_f64};
use dereth_primitives::ObjectId;

pub const FONT: &str = "15-6";
const LINE_HEIGHT: i32 = 15;

/// The selection indicator's colour images by colour index (the radar's colours: 1 blue, 2 gold,
/// 3 white, 4 purple, 5 red, 6 pink, 7 green, 8 yellow, 9 cyan, 10 bright green), for the 24-pixel
/// arrows and the 12-pixel corners.
const TARGET_COLOURS: [(u32, u32); 11] = [
    (0x0600_19e0, 0x0600_19d6),
    (0x0600_19ca, 0x0600_19d5),
    (0x0600_19e1, 0x0600_19d1),
    (0x0600_19e0, 0x0600_19d6),
    (0x0600_19dd, 0x0600_19d4),
    (0x0600_19df, 0x0600_19d8),
    (0x0600_19db, 0x0600_19d9),
    (0x0600_2630, 0x0600_262f),
    (0x0600_19da, 0x0600_19d7),
    (0x0600_19de, 0x0600_19d2),
    (0x0600_19dc, 0x0600_19d3),
];

/// The selection indicator's commands for one frame, in window coordinates, given where the
/// selected object projects into the game viewport `view` and its radar colour index: the four
/// 12-pixel corner marks around its box (each pointing in at the object), or, when it is out of
/// view, the 24-pixel arrow of its eighth of the compass at the viewport's edge. Both keep 8
/// pixels inside the viewport, and each mark's white takes the object's colour.
#[must_use]
pub fn target_marks(
    projection: Option<dereth_client_contract::target::Projection>,
    view: crate::widgets::Rect,
    colour: u8,
) -> Vec<crate::Command> {
    use dereth_client_contract::target::Projection;
    use dereth_presentation::target::{off_screen_position, on_screen_box};
    let (large, small) = TARGET_COLOURS[usize::from(colour).min(10)];
    let image = |did: u32, tint: u32, x: i32, y: i32, size: u32| crate::Command::Image {
        did: format!("{did:08X}+{tint:08X}"),
        x: view.x + x,
        y: view.y + y,
        width: size,
        height: size,
        clip: Some([view.x, view.y, view.x + view.w, view.y + view.h]),
        color_key: Some([0, 0, 0]),
        key_bits: Some([5, 6, 5]),
        tile: false,
    };
    match projection {
        Some(Projection::OnScreen(rect)) => {
            let (x, y, w, h) = on_screen_box(rect, (12, 12), (view.w, view.h));
            vec![
                image(0x0600_19cf, small, x, y, 12),
                image(0x0600_19d0, small, x + w - 12, y, 12),
                image(0x0600_19cd, small, x, y + h - 12, 12),
                image(0x0600_19ce, small, x + w - 12, y + h - 12, 12),
            ]
        }
        Some(Projection::OffScreen(heading)) => {
            // Up, up-right, right, down-right, down, down-left, left, up-left; each eighth starts
            // 23 degrees past the previous direction.
            const ARROWS: [u32; 8] = [
                0x0600_19c4,
                0x0600_19cc,
                0x0600_19c9,
                0x0600_19c7,
                0x0600_19c5,
                0x0600_19c6,
                0x0600_19c8,
                0x0600_19cb,
            ];
            let sector = dereth_primitives::num::to_i32((heading + 22.0).rem_euclid(360.0) / 45.0);
            let arrow = ARROWS[usize::try_from(sector.rem_euclid(8)).unwrap_or(0)];
            let (x, y) = off_screen_position(heading, (24, 24), (view.w, view.h));
            vec![image(arrow, large, x, y, 24)]
        }
        None => vec![],
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LightingChange {
    Set {
        object: ObjectId,
        minimum: f32,
        maximum: f32,
    },
    Restore(ObjectId),
}

#[derive(Debug, Default)]
pub struct OverlayState {
    hovered: Option<ObjectId>,
    selection: Option<ObjectId>,
    name: TransientText,
    hint: TransientText,
    message: TransientText,
    message_color: Option<u32>,
    normal_message_color: Option<u32>,
    last_warning: Option<f64>,
    power: PowerBarState,
    flash: Option<Flash>,
    lighting: Vec<LightingChange>,
}
#[derive(Debug)]
struct Flash {
    object: ObjectId,
    since: f64,
    dark: bool,
}
impl OverlayState {
    /// `found` is the current world pick, or None when the pointer is outside
    /// the viewport. This consumes the host's pick; it does not run ray tests.
    pub fn hover(&mut self, world: &World, found: Option<ObjectId>, now: f64) {
        let found = found.filter(|id| {
            world
                .weenie(*id)
                .is_some_and(|w| w.pwd.bitfield & 0x80 == 0)
        });
        if found == self.hovered {
            return;
        }
        self.hovered = found;
        if let Some(id) = found {
            self.name.set(classic_name(world, id), now);
            // The viewport does not refresh the hint for equipment worn by self.
            if world.weenie(id).is_some_and(|w| {
                w.pwd.wielder_id.unwrap_or_default() != world.player.unwrap_or_default()
            }) {
                match action_hint(world, id) {
                    Hint::Set(text) => self.hint.set(text, now),
                    Hint::Clear => self.hint.clear(),
                    Hint::Keep => {}
                }
            }
        } else {
            self.name.hide(now);
            self.hint.hide(now);
        }
    }
    /// Replaces or appends to the third line, the message line. The return requests
    /// sound 0x6E once per timer value; host audio availability remains a gate.
    pub fn message(&mut self, text: &str, append: bool, warning: bool, now: f64) -> bool {
        let text = if append {
            format!("{}{text}", self.message.text)
        } else {
            text.to_owned()
        };
        self.message.set(text, now);
        self.message_color = Some(if warning {
            0xffffff00
        } else {
            self.normal_message_color.unwrap_or(0xffd2d2c8)
        });
        if warning {
            self.normal_message_color = Some(0xffffffff);
        }
        let sound = warning && self.last_warning != Some(now);
        if warning {
            self.last_warning = Some(now);
        }
        sound
    }
    pub fn power_bar_notice(&mut self, notice: dereth_client_model::combat::PowerBarNotice) {
        self.power.observe(notice);
    }
    pub fn classic_power_level(&self) -> f32 {
        self.power.classic_level
    }
    /// Call for every selection notice, including a forced same-object notice.
    /// The previous object's lighting is restored and the new one's flash begins.
    pub fn flash(&mut self, selected: Option<ObjectId>, now: f64) {
        self.selection = selected.filter(|id| id.0 != 0);
        if let Some(old) = self.flash.take() {
            self.lighting.push(LightingChange::Restore(old.object));
        }
        if let Some(object) = self.selection {
            self.lighting.push(LightingChange::Set {
                object,
                minimum: 0.99,
                maximum: 1.0,
            });
            self.flash = Some(Flash {
                object,
                since: now,
                dark: false,
            });
        }
    }
    /// Polling fallback for hosts without selection-notice delivery. Forced
    /// same-object notifications must still call flash explicitly.
    pub fn sync_selection(&mut self, selected: Option<ObjectId>, now: f64) {
        if self.selection != selected.filter(|id| id.0 != 0) {
            self.flash(selected, now);
        }
    }
    pub fn tick(&mut self, now: f64) {
        self.name.tick(now);
        self.hint.tick(now);
        self.message.tick(now);
        if let Some(flash) = &mut self.flash {
            if now - flash.since >= 0.2 {
                if flash.dark {
                    self.lighting.push(LightingChange::Restore(flash.object));
                    self.flash = None;
                } else {
                    flash.dark = true;
                    flash.since = now;
                    self.lighting.push(LightingChange::Set {
                        object: flash.object,
                        minimum: 0.0,
                        maximum: 0.35,
                    });
                }
            }
        }
    }
    pub fn drain_lighting(&mut self) -> Vec<LightingChange> {
        std::mem::take(&mut self.lighting)
    }
    pub fn reset(&mut self) {
        if let Some(old) = self.flash.take() {
            self.lighting.push(LightingChange::Restore(old.object));
        }
        self.selection = None;
        self.hovered = None;
        self.name.clear();
        self.hint.clear();
        self.message.clear();
        self.message_color = None;
        self.normal_message_color = None;
        self.last_warning = None;
        self.power = PowerBarState::default();
    }
    /// Convenience for the normal viewport whose top is 28. Native hosts with
    /// an explicit viewport should call commands_at.
    pub fn commands(
        &self,
        viewport_width: i32,
        panel_visible: bool,
        stretch: bool,
        measure: impl Fn(&str) -> i32,
    ) -> Vec<Command> {
        self.commands_at(
            crate::widgets::Rect {
                x: 0,
                y: 28,
                w: viewport_width,
                h: 0,
            },
            panel_visible,
            stretch,
            measure,
        )
    }
    /// The name, hint and message lines sit 5, 21 and 37 pixels below the viewport's top; the
    /// 30, 46 and 62 they start at are replaced on the first layout.
    pub fn commands_at(
        &self,
        viewport: crate::widgets::Rect,
        panel_visible: bool,
        stretch: bool,
        measure: impl Fn(&str) -> i32,
    ) -> Vec<Command> {
        let width = (viewport.w - if panel_visible && !stretch { 10 } else { 130 }).max(0);
        let mut result = Vec::new();
        self.name
            .draw(5, width, 17, 0xffd2d2c8, &measure, &mut result);
        self.hint
            .draw(21, width, 17, 0xffd2d2c8, &measure, &mut result);
        self.message.draw(
            37,
            width,
            30,
            self.message_color.unwrap_or(0xffd2d2c8),
            &measure,
            &mut result,
        );
        self.power.draw(viewport.w, viewport.h, &mut result);
        result
            .into_iter()
            .map(|command| crate::desktop::translate_command(command, viewport.x, viewport.y))
            .collect()
    }
}

#[derive(Debug, Default)]
struct TransientText {
    text: String,
    active: bool,
    hiding: bool,
    since: f64,
    erased_level: i32,
}
impl TransientText {
    fn set(&mut self, text: String, now: f64) {
        // The classic line holds 199 ANSI bytes. The renderer maps unsupported
        // characters to '?'; retain at most 199 rendered characters here.
        self.text = text.chars().take(199).collect();
        self.active = true;
        self.hiding = false;
        self.since = now;
        self.erased_level = 0;
    }
    fn clear(&mut self) {
        self.active = false;
        self.hiding = false;
        self.erased_level = 0;
    }
    fn hide(&mut self, now: f64) {
        self.hiding = true;
        self.since = now;
    }
    fn tick(&mut self, now: f64) {
        if !self.active {
            return;
        }
        let elapsed = (now - self.since).max(0.0);
        if !self.hiding {
            if elapsed >= 5.0 {
                self.hiding = true;
                self.since = now;
            }
        } else if elapsed >= 1.0 {
            self.clear();
        } else {
            // The wipe erases the drawn line, so an interrupted wipe cannot
            // restore previously cleared rows without a fresh text set.
            // Under a second here, so the narrowing to `f32` only rounds.
            #[allow(clippy::cast_possible_truncation)]
            let t = elapsed as f32;
            self.erased_level = self.erased_level.max(animation_level(t));
        }
    }
    fn draw(
        &self,
        y: i32,
        width: i32,
        height: i32,
        foreground: u32,
        measure: &impl Fn(&str) -> i32,
        out: &mut Vec<Command>,
    ) {
        if !self.active || width <= 0 {
            return;
        }
        let lines = split_lines(&self.text, width, measure);
        let surface_height = (LINE_HEIGHT * i32_from(lines.len())).min(height);
        let erased = surface_height * self.erased_level / 1024;
        let clip = Some([10, y + erased, 10 + width, y + surface_height]);
        for (n, line) in lines.into_iter().enumerate() {
            let top = y - 1 + i32_from(n) * LINE_HEIGHT;
            for (dx, dy, color) in [
                (-1, 0, 0xff101010),
                (0, 1, 0xff101010),
                (1, 0, 0xff101010),
                (0, -1, 0xff101010),
                (0, 0, foreground),
            ] {
                out.push(Command::Text {
                    text: line.to_owned(),
                    x: 11 + dx,
                    y: top + dy,
                    font: FONT.into(),
                    color,
                    clip,
                });
            }
        }
    }
}

/// A line too wide for the viewport splits once at the last fitting space, never at an
/// arbitrary glyph.
fn split_lines<'a>(text: &'a str, width: i32, measure: &impl Fn(&str) -> i32) -> Vec<&'a str> {
    if measure(text) > width {
        for (index, c) in text.char_indices().rev() {
            if index > 0 && index + 1 < text.len() && c == ' ' && measure(&text[..index]) <= width {
                return vec![&text[..index], &text[index + 1..]];
            }
        }
    }
    vec![text]
}

/// The wipe's progress curve: 100 integer sine weights, then their normalized cumulative
/// sum, indexed by 99*t truncated. The wipe erases this fraction of the line.
fn animation_level(t: f32) -> i32 {
    static LEVELS: std::sync::OnceLock<[i32; 100]> = std::sync::OnceLock::new();
    let levels = LEVELS.get_or_init(|| {
        // The classic interface's own value of pi, kept so its table matches.
        #[allow(clippy::approx_constant)]
        let weights: [i32; 100] =
            std::array::from_fn(|i| to_i32_f64(math::sin(i as f64 * 3.141592 / 99.0) * 1024.0));
        let total: i32 = weights.iter().sum();
        let mut sum = 0;
        std::array::from_fn(|i| {
            sum += weights[i];
            sum * 1024 / total
        })
    });
    levels[usize::try_from(to_i32_f64(f64::from(t.clamp(0.0, 1.0)) * 99.0)).unwrap_or(0)]
}

fn classic_name(world: &World, id: ObjectId) -> String {
    let Some(w) = world.weenie(id) else {
        return String::new();
    };
    let name = w.object_name(NameType::Appropriate);
    // The classic name has no material prefix; later clients added one.
    if w.pwd.bitfield & 0x40 != 0 && name.starts_with('+') {
        name[1..].into()
    } else {
        name
    }
}
#[derive(Debug, PartialEq)]
enum Hint {
    Set(String),
    Keep,
    Clear,
}
fn action_hint(world: &World, id: ObjectId) -> Hint {
    let Some(w) = world.weenie(id) else {
        return Hint::Clear;
    };
    let p = &w.pwd;
    let action = classify(world, id);
    let text = match action {
        1 => {
            let kind = w.inq_type();
            if w.is_corpse() {
                "Double click to loot corpse"
            } else if p.bitfield & 0x200 != 0 {
                "Double click to begin shopping"
            } else if p.bitfield & 0x4000 != 0 {
                "Double click to set your resurrection location"
            } else if kind & 0x10 != 0 {
                if world.combat.combat_mode.raw() == 1 || world.combat.advanced_combat_mode {
                    return Hint::Keep;
                }
                "Double click to attack"
            } else if kind & 0x10000 != 0 {
                "Double click to enter portal"
            } else if kind & 0x20 != 0 {
                "Double click to consume"
            } else if kind & 0x200 != 0 {
                "Double click to open and close"
            } else if kind & 0x2000 != 0 {
                "Double click to read or write"
            } else if kind & 0x4000 != 0 {
                "Double click to unlock your last selected item"
            } else if kind & 0x80000 != 0 {
                if p.effects.unwrap_or(0) & 1 != 0 {
                    "Double click to transfer Mana to an item."
                } else {
                    "Double click to drain Mana from, and destroy, an item."
                }
            } else if p.useability.unwrap_or(0) <= 1 {
                return Hint::Keep;
            } else {
                "Double click to use"
            }
        }
        2 if p.wielder_id.unwrap_or_default() == world.player.unwrap_or_default() => {
            "Double click to unwield/take off"
        }
        2 => "Double click to pick up",
        3 => "Double click to wield",
        4 => "Double click to wear (if there's room)",
        5 => {
            return Hint::Set(format!(
                "Double click to trade items with {}",
                classic_name(world, id)
            ));
        }
        _ => return Hint::Clear,
    };
    Hint::Set(text.into())
}

/// The classic hint classifier predates the wield-on-use and left-hand wield uses.
fn classify(world: &World, id: ObjectId) -> u8 {
    let Some(w) = world.weenie(id) else {
        return 0;
    };
    let p = &w.pwd;
    let container = p.container_id.unwrap_or_default();
    let wielder = p.wielder_id.unwrap_or_default();
    let player = world.player.unwrap_or_default();
    if ((container.0 == 0 && p.bitfield & 4 == 0)
        || world
            .ground_object
            .is_some_and(|g| g.0 != 0 && g == container))
        && (wielder.0 == 0 || wielder == player)
        && ((p.bitfield & 0x800000 == 0
            && p.items_capacity.unwrap_or(0) == 0
            && p.containers_capacity.unwrap_or(0) == 0)
            || (0x3ba4..=0x3ba7).contains(&p.wcid))
    {
        return 2;
    }
    let kind = w.inq_type();
    if world.is_owned_by_player(id) {
        if (p.combat_use.unwrap_or(0) != 0 || kind & 0x8000 != 0) && wielder != player {
            return 3;
        }
        if [0x7e00, 0x1ff, 0xf8000].iter().any(|mask| {
            p.valid_locations.unwrap_or(0) & mask != 0 && p.location.unwrap_or(0) & mask == 0
        }) {
            return 4;
        }
        if kind & 0x20000000 != 0 {
            return 6;
        }
    } else if kind & 0x80000000 != 0 {
        return 7;
    }
    if ItemUses(p.useability.unwrap_or(0)).is_useable() {
        return 1;
    }
    if w.is_player() && id != player {
        return 5;
    }
    0
}

/// The classic combat panel's power meter is kept separate from the viewport's
/// charge meter. The ordered journal preserves intermediate level-zero events.
#[derive(Debug, Default)]
struct PowerBarState {
    classic_level: f32,
    overlay: Option<(
        dereth_client_contract::powerbar::PowerBarMode,
        ChargeKind,
        f32,
    )>,
}
#[derive(Clone, Copy, Debug)]
enum ChargeKind {
    Power,
    Accuracy,
    Height,
}
impl PowerBarState {
    fn observe(&mut self, notice: dereth_client_model::combat::PowerBarNotice) {
        use dereth_client_contract::powerbar::PowerBarMode;
        use dereth_client_model::combat::PowerBarNotice::*;
        match notice {
            SetLevel {
                mode: PowerBarMode::Combat,
                level,
            } => self.classic_level = level,
            Begin {
                mode: PowerBarMode::Combat,
                ..
            } => self.classic_level = 0.0,
            Begin { mode, melee, .. }
                if mode == PowerBarMode::AdvancedCombat || mode == PowerBarMode::Jump =>
            {
                self.overlay = Some((
                    mode,
                    if mode == PowerBarMode::Jump {
                        ChargeKind::Height
                    } else if melee {
                        ChargeKind::Power
                    } else {
                        ChargeKind::Accuracy
                    },
                    0.0,
                ));
            }
            SetLevel { mode, level } => {
                if let Some((active, _, value)) = &mut self.overlay {
                    if *active == mode {
                        *value = level;
                    }
                }
            }
            Finish { mode }
                if self
                    .overlay
                    .as_ref()
                    .is_some_and(|(active, _, _)| *active == mode) =>
            {
                self.overlay = None;
            }
            _ => {}
        }
    }
    fn draw(&self, width: i32, height: i32, out: &mut Vec<Command>) {
        let Some((_, kind, level)) = self.overlay else {
            return;
        };
        if height < 18 {
            return;
        }
        let width = (width - 20).max(0);
        let fill = to_i32(width as f32 * level.clamp(0.0, 1.0));
        if fill == 0 {
            return;
        }
        let y = height - 18;
        let (did, text, color) = match kind {
            ChargeKind::Height => ("06001354", "Height", 0xff000000),
            ChargeKind::Power => ("06001200", "Power", 0xffc8b478),
            ChargeKind::Accuracy => ("06001200", "Accuracy", 0xffc8b478),
        };
        let clip = Some([10, y, 10 + fill, y + 13]);
        out.push(Command::Image {
            did: did.into(),
            x: 10,
            y,
            width: fill as u32,
            height: 13,
            clip,
            color_key: None,
            key_bits: None,
            tile: true,
        });
        out.push(Command::Text {
            text: text.into(),
            x: 18,
            y: y - 1,
            font: "14-6".into(),
            color,
            clip,
        });
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn the_selection_indicator_frames_a_visible_object_and_points_at_a_hidden_one() {
        use dereth_client_contract::target::Projection;
        let view = crate::panels::rect(0, 28, 491, 472);
        let dids = |p| -> Vec<(String, i32, i32)> {
            target_marks(Some(p), view, 3)
                .into_iter()
                .filter_map(|c| match c {
                    crate::Command::Image { did, x, y, .. } => Some((did, x, y)),
                    _ => None,
                })
                .collect()
        };
        let marks = dids(Projection::OnScreen((100, 100, 200, 180)));
        assert_eq!(marks.len(), 4);
        assert_eq!(marks[0], ("060019CF+060019D6".into(), 88, 116));
        assert_eq!(marks[3].0, "060019CE+060019D6");
        // Straight to the right: the right-pointing arrow on the right edge.
        let arrow = dids(Projection::OffScreen(90.0));
        assert_eq!(arrow.len(), 1);
        assert_eq!(arrow[0].0, "060019C9+060019E0");
        assert_eq!(arrow[0].1, 491 - 24 - 8);
        assert!(target_marks(None, view, 3).is_empty());
        // Up-right is the arrow pointing up and to the right.
        assert_eq!(dids(Projection::OffScreen(45.0))[0].0, "060019CC+060019E0");
    }
    use dereth_client_model::weenie::Weenie;
    fn world() -> World {
        let mut world = World::new();
        world.player = Some(ObjectId(1));
        world
            .tables
            .weenies
            .insert(ObjectId(1), Weenie::new(ObjectId(1)));
        let mut w = Weenie::new(ObjectId(2));
        w.pwd.name = "Sword".into();
        world.tables.weenies.insert(ObjectId(2), w);
        world
    }
    #[test]
    fn selection_flashes_bright_then_dark_then_restores_without_brackets() {
        let mut o = OverlayState::default();
        o.flash(Some(ObjectId(2)), 0.0);
        assert_eq!(
            o.drain_lighting(),
            vec![LightingChange::Set {
                object: ObjectId(2),
                minimum: 0.99,
                maximum: 1.0
            }]
        );
        o.tick(0.19);
        assert!(o.drain_lighting().is_empty());
        o.tick(0.2);
        assert_eq!(
            o.drain_lighting(),
            vec![LightingChange::Set {
                object: ObjectId(2),
                minimum: 0.0,
                maximum: 0.35
            }]
        );
        o.tick(0.41);
        assert_eq!(
            o.drain_lighting(),
            vec![LightingChange::Restore(ObjectId(2))]
        );
    }
    #[test]
    fn replacing_or_forcing_selection_restores_previous_lighting_first() {
        let mut o = OverlayState::default();
        o.flash(Some(ObjectId(2)), 0.0);
        o.drain_lighting();
        o.flash(Some(ObjectId(2)), 0.1);
        assert_eq!(o.drain_lighting()[0], LightingChange::Restore(ObjectId(2)));
        o.flash(None, 0.2);
        assert_eq!(
            o.drain_lighting(),
            vec![LightingChange::Restore(ObjectId(2))]
        );
    }
    #[test]
    fn unchanged_hover_expires_after_five_seconds_and_one_second_wipe() {
        let world = world();
        let mut o = OverlayState::default();
        o.hover(&world, Some(ObjectId(2)), 0.0);
        assert_eq!(o.hint.text, "Double click to pick up");
        o.tick(5.0);
        o.hover(&world, Some(ObjectId(2)), 5.0);
        o.tick(5.5);
        let c = o.commands(500, false, false, |s| i32_from(s.len()) * 6);
        assert!(matches!(&c[0],Command::Text { clip:Some([10, top, 380,48]),.. } if *top>33));
        o.tick(6.0);
        assert!(o
            .commands(500, false, false, |s| i32_from(s.len()))
            .is_empty());
    }
    #[test]
    fn leaving_hover_starts_wipe_and_reentry_redraws_erased_rows() {
        let world = world();
        let mut o = OverlayState::default();
        o.hover(&world, Some(ObjectId(2)), 0.0);
        o.hover(&world, None, 0.1);
        o.tick(0.6);
        assert!(o.name.erased_level > 0);
        o.hover(&world, Some(ObjectId(2)), 0.7);
        assert_eq!(o.name.erased_level, 0);
    }
    #[test]
    fn legacy_component_pack_exception_and_owned_wield_gates_are_retained() {
        let mut world = world();
        let id = ObjectId(2);
        let w = world.weenie_mut(id).unwrap();
        w.pwd.items_capacity = Some(10);
        w.pwd.wcid = 0x3ba4;
        assert_eq!(classify(&world, id), 2);
        world.weenie_mut(id).unwrap().pwd.wcid = 0x3ba8;
        assert_eq!(classify(&world, id), 1);
        let w = world.weenie_mut(id).unwrap();
        w.pwd.container_id = Some(ObjectId(1));
        w.pwd.combat_use = Some(1);
        assert_eq!(classify(&world, id), 3);
    }
    #[test]
    fn creature_noncombat_hint_preserves_prior_text_and_hidden_objects_leave_hover() {
        let mut world = world();
        let id = ObjectId(2);
        let mut o = OverlayState::default();
        o.hover(&world, Some(id), 0.0);
        let mut creature = Weenie::new(ObjectId(3));
        creature.pwd.name = "Creature".into();
        creature.pwd.obj_type = 0x10;
        creature.pwd.bitfield = 4;
        world.tables.weenies.insert(ObjectId(3), creature);
        o.hover(&world, Some(ObjectId(3)), 0.1);
        assert_eq!(o.hint.text, "Double click to pick up");
        world.weenie_mut(id).unwrap().pwd.bitfield = 0x80;
        o.hover(&world, Some(id), 0.2);
        assert_eq!(o.hovered, None);
    }
    #[test]
    fn outline_uses_parent_clip_and_two_line_wrap_never_splits_a_word() {
        let mut t = TransientText::default();
        t.set("one two three".into(), 0.0);
        let mut commands = Vec::new();
        t.draw(
            30,
            45,
            17,
            0xffd2d2c8,
            &|s| i32_from(s.len()) * 6,
            &mut commands,
        );
        assert_eq!(commands.len(), 10);
        assert!(
            matches!(&commands[4],Command::Text {text,x:11,y:29,clip:Some([10,30,55,47]),..} if text=="one two")
        );
        assert_eq!(
            split_lines("unsplittable", 12, &|s| i32_from(s.len()) * 6),
            vec!["unsplittable"]
        );
        assert_eq!(animation_level(0.0), 0);
        assert_eq!(animation_level(1.0), 1024);
    }
    #[test]
    fn third_line_appends_warns_once_per_tick_and_resets_to_white() {
        let mut o = OverlayState::default();
        assert!(o.message("Cannot ", false, true, 1.0));
        assert!(!o.message("use that", true, true, 1.0));
        assert_eq!(o.message.text, "Cannot use that");
        assert_eq!(o.message_color, Some(0xffffff00));
        assert!(!o.message("Logging off...", false, false, 1.1));
        assert_eq!(o.message_color, Some(0xffffffff));
        let commands = o.commands_at(
            crate::widgets::Rect {
                x: 20,
                y: 28,
                w: 500,
                h: 300,
            },
            false,
            false,
            |s| i32_from(s.len()) * 6,
        );
        assert!(commands.iter().any(|c|matches!(c,Command::Text{text,x:31,y:64,color:0xffffffff,..} if text=="Logging off...")));
    }
    #[test]
    fn ordered_charge_notices_keep_jump_separate_and_finish_only_matching_mode() {
        use dereth_client_contract::powerbar::PowerBarMode::*;
        use dereth_client_model::combat::PowerBarNotice::*;
        let mut o = OverlayState::default();
        o.power_bar_notice(SetLevel {
            mode: Combat,
            level: 0.8,
        });
        o.power_bar_notice(SetLevel {
            mode: Combat,
            level: 0.0,
        });
        assert_eq!(o.classic_power_level(), 0.0);
        o.power_bar_notice(Begin {
            mode: Jump,
            melee: false,
            recklessness_sac: 0,
        });
        o.power_bar_notice(SetLevel {
            mode: Jump,
            level: 0.5,
        });
        o.power_bar_notice(Finish {
            mode: AdvancedCombat,
        });
        let commands = o.commands_at(
            crate::widgets::Rect {
                x: 0,
                y: 28,
                w: 500,
                h: 300,
            },
            false,
            false,
            |_| 0,
        );
        assert!(commands.iter().any(|c|matches!(c,Command::Image {did,x:10,y:310,width:240,height:13,..} if did=="06001354")));
        o.power_bar_notice(Finish { mode: Jump });
        assert!(o
            .commands_at(
                crate::widgets::Rect {
                    x: 0,
                    y: 28,
                    w: 500,
                    h: 300
                },
                false,
                false,
                |_| 0
            )
            .is_empty());
    }
}
