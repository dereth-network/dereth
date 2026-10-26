//! The plugins' surfaces as a layer over the modern and the classic interface: drawn by the widget
//! kit in the game's own font, icons and colours (the "retail plain" skin), each plugin window
//! standing among the interface's own windows.
//!
//! Under the modern interface each plugin window has a stand-in among the interface's root
//! elements: an element as large as the window, which the interface stacks, raises and hit-tests
//! with its own windows, and which marks where in its draw list the window's own list is drawn.
//! Two more stand-ins mark where the HUDs, the marks and the bar are drawn (just over the screen
//! the game is at, under every other window) and where the labels in the world are (in the game
//! view, under its panels). Under the classic interface, whose windows are docked rather than
//! stacked, the plugin windows stand over its windows and under its own modal box.
//!
//! A press is the layer's when it lands on one of its surfaces and nothing of the interface's
//! stands over it there; its release follows it. Keys are the layer's while a plugin's line or box
//! has the keyboard, or while a plugin's choice list is open (Escape closes it).

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_client_contract::overlay::{OverlayItem, OverlayTexture};
use dereth_input::host::HostEvent;
use dereth_input::keys::MouseButton;
use dereth_plugin_ui::{Layer, Scene};
use dereth_primitives::TextureData;
use dereth_ui_kit::art::{Art, ArtSource, Image};
use dereth_ui_kit::colours::Colours;
use dereth_ui_kit::draw::{DrawList, Overlay, TextureSink};
use dereth_ui_kit::input::InputFrame;
use dereth_ui_kit::widgets::Ctx;

use crate::present::ClientPresentation;

/// The keys the layer's pictures are uploaded under: a range of their own, apart from every
/// interface's.
const KEY_BASE: u64 = 0x504C_4700_0000_0000;

/// The game's icons, read from its data for the layer's skin.
#[derive(Debug)]
struct DatIcons {
    store: Arc<dereth_dat::RetailDatStore>,
}

impl ArtSource for DatIcons {
    fn icon(&self, did: u32) -> Option<Image> {
        let textures = dereth_scene::textures::TextureStore::new(&self.store);
        textures
            .icon_data(dereth_primitives::DataId(did))
            .ok()
            .filter(|d| d.format == dereth_primitives::TextureFormat::Bgra8)
            .and_then(|d| {
                let (width, height) = (d.width, d.height);
                Some(Image {
                    width,
                    height,
                    bgra: d.levels.into_iter().next()?,
                })
            })
    }

    fn fresh(&self) -> Arc<dyn ArtSource> {
        Arc::new(Self {
            store: Arc::clone(&self.store),
        })
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The game's label font from its data, as the kit draws it; `None` where the data has none.
fn retail_font(store: &dereth_dat::RetailDatStore) -> Option<dereth_ui_kit::art::GivenFont> {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource;
    let did = dereth_primitives::DataId(dereth_plugin_ui::skin::FONT);
    let bytes = store.read(did).ok()?;
    let font = dereth_assets::ui::Font::decode_payload(did, &bytes).ok()?;
    let sheet = dereth_scene::textures::TextureStore::new(store)
        .bgra8(font.foreground_surface)
        .ok()?;
    let bgra: Vec<u8> = sheet.pixels.iter().flat_map(|p| *p).collect();
    Some(dereth_plugin_ui::skin::retail_font(
        &font,
        sheet.width,
        sheet.height,
        &bgra,
    ))
}

/// The skin the layer is drawn in: the game's own window art under the modern interface, the
/// classic interface's own under it.
pub(crate) enum Skin {
    Retail,
    Classic(Arc<dereth_classic_ui::art::ClassicArt>),
}

/// What a skin was made for: the interface's art, and the reading of the data files.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkinKey {
    Retail(u64),
    Classic(usize, u64),
}

impl Skin {
    fn key(&self, generation: u64) -> SkinKey {
        match self {
            Self::Retail => SkinKey::Retail(generation),
            Self::Classic(art) => SkinKey::Classic(Arc::as_ptr(art) as usize, generation),
        }
    }
}

/// An image of the game's data, for the retail skin: the picture `id` names, decoded.
fn retail_image(textures: &dereth_scene::textures::TextureStore<'_>, id: u32) -> Option<Image> {
    let sheet = textures.bgra8(dereth_primitives::DataId(id)).ok()?;
    Some(Image {
        width: sheet.width,
        height: sheet.height,
        bgra: sheet.pixels.iter().flat_map(|p| *p).collect(),
    })
}

/// An image of the classic portal, for the classic skin, in the kit's pixel order.
fn classic_image(art: &dereth_classic_ui::art::ClassicArt, id: u32) -> Option<Image> {
    let image = art.image(id)?;
    Some(Image {
        width: image.width,
        height: image.height,
        bgra: image
            .rgba
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect(),
    })
}

/// The classic interface's own text, its labels' font, as the kit draws it.
fn classic_font(art: &dereth_classic_ui::art::ClassicArt) -> Option<dereth_ui_kit::art::GivenFont> {
    let atlas = art.fonts().get(CLASSIC_FONT)?;
    let glyphs: Vec<dereth_plugin_ui::skin::AtlasGlyph> = atlas
        .glyphs
        .iter()
        .filter_map(|(ch, g)| {
            Some(dereth_plugin_ui::skin::AtlasGlyph {
                ch: char::from_u32(*ch)?,
                x: g.x,
                y: g.y,
                w: g.width,
                h: g.height,
                bearing_x: g.bearing_x,
                bearing_y: g.bearing_y,
                advance: g.advance,
            })
        })
        .collect();
    Some(dereth_plugin_ui::skin::atlas_font(
        &glyphs,
        atlas.line_height,
        atlas.baseline,
        atlas.width,
        atlas.height,
        atlas.rgba.clone(),
    ))
}

/// The classic interface's labels' font, by the name its screens use.
const CLASSIC_FONT: &str = "15-6";

/// A presentation as the kit's texture sink.
struct Sink<'a>(&'a mut dyn ClientPresentation);

impl TextureSink for Sink<'_> {
    fn size(&self) -> (u32, u32) {
        self.0.size()
    }
    fn upload(&mut self, key: OverlayTexture, data: &TextureData) -> bool {
        self.0.overlay_upload(key, data).is_ok()
    }
    fn release(&mut self, key: OverlayTexture) {
        let _ = self.0.overlay_release(key);
    }
}

/// This frame's layer as overlay items, part by part.
#[derive(Debug, Default)]
pub(crate) struct Slices {
    pub labels: Vec<OverlayItem>,
    pub under: Vec<OverlayItem>,
    /// Each window, back to front.
    pub windows: Vec<((String, String), Vec<OverlayItem>)>,
    pub top: Vec<OverlayItem>,
}

impl Slices {
    /// Every part over the labels in the world, bottom to top: the HUDs, marks and bar, the
    /// windows, and what stands over them all.
    pub fn over_labels(&self) -> Vec<OverlayItem> {
        let mut out = self.under.clone();
        for (_, w) in &self.windows {
            out.extend(w.iter().cloned());
        }
        out.extend(self.top.iter().cloned());
        out
    }
}

/// The modern interface's stand-ins for the layer.
#[derive(Debug, Default)]
struct Seats {
    windows: BTreeMap<(String, String), dereth_ui::ElemHandle>,
    under: Option<dereth_ui::ElemHandle>,
    labels: Option<dereth_ui::ElemHandle>,
}

/// What the shell keeps for the layer from one frame to the next.
#[derive(Debug, Default)]
pub(crate) struct LayerHost {
    pub layer: Layer,
    /// The layer's share of the player's input this frame.
    pub input: InputFrame,
    /// The skin, what it was made for and from, and the colours of its text.
    art: Option<(SkinKey, Arc<Art>, Colours)>,
    overlay: Option<Overlay>,
    pub slices: Slices,
    /// The buttons whose press the layer took, until each comes up.
    held: [bool; 3],
    pub views: super::plugins::ViewCache,
    seats: Seats,
    last_time: Option<f64>,
    /// The pad on the layer ([`super::plugin_pad`]), and when it was last read.
    pad: super::plugin_pad::LayerPad,
    pad_ms: Option<u32>,
}

/// The stand-ins' own id range among the elements, apart from the game's layouts'.
const SEAT_IDS: u32 = 0x7E00_0000;

impl LayerHost {
    /// Whether the host's question is up in the layer: Escape and the pad's cancel are its own.
    #[must_use]
    pub fn asking(&self) -> bool {
        self.layer.asking()
    }

    /// Whether a plugin's line or box has the keyboard.
    #[must_use]
    pub fn typing(&self) -> bool {
        self.layer.ui.typing()
    }

    /// The skin `skin` asks for, for this reading of the data files: made the first time, and
    /// again when the interface or the files change (letting go of every picture made for the
    /// old one).
    fn skin(
        &mut self,
        cx: &mut super::Cx<'_, impl crate::platform::host::Host>,
        skin: &Skin,
    ) -> (Arc<Art>, Colours) {
        let key = skin.key(cx.store_generation());
        if let Some((k, art, colours)) = &self.art {
            if *k == key {
                return (Arc::clone(art), colours.clone());
            }
        }
        if let Some(overlay) = self.overlay.as_mut() {
            overlay.release(&mut Sink(cx.present_mut()));
        }
        let store = Arc::clone(cx.store());
        let icons: Arc<dyn ArtSource> = Arc::new(DatIcons {
            store: Arc::clone(&store),
        });
        let (art, colours) = match skin {
            Skin::Retail => {
                let textures = dereth_scene::textures::TextureStore::new(&store);
                let load = |id: u32| retail_image(&textures, id);
                (
                    dereth_plugin_ui::skin::retail(retail_font(&store), Some(icons), &load),
                    Colours::retail_plain(),
                )
            }
            Skin::Classic(classic) => {
                let load = |id: u32| classic_image(classic, id);
                (
                    dereth_plugin_ui::skin::classic(classic_font(classic), Some(icons), &load),
                    Colours::plain_text(0xFFD2_D2C8, 0xFF64_6464),
                )
            }
        };
        let art = Arc::new(art);
        self.art = Some((key, Arc::clone(&art), colours.clone()));
        (art, colours)
    }

    /// Draw this frame's layer, its input taken, and turn each part into overlay items.
    pub fn frame(
        &mut self,
        cx: &mut super::Cx<'_, impl crate::platform::host::Host>,
        scene: Scene<'_>,
        skin: &Skin,
    ) -> Option<(String, String)> {
        let (art, colours) = self.skin(cx, skin);
        let size = cx.present().size();
        #[allow(clippy::cast_precision_loss)]
        let screen = (size.0 as f32, size.1 as f32);
        let time = cx.now();
        let dt = self.last_time.map_or(0.0, |t| (time - t).max(0.0));
        self.last_time = Some(time);
        let mut ctx = Ctx {
            time,
            dt,
            input: &mut self.input,
            colours: &colours,
            hot: false,
        };
        let drawn = self.layer.frame(&art, 1.0, screen, &mut ctx, scene);
        let raised = drawn.raised.clone();
        let overlay = self.overlay.get_or_insert_with(|| Overlay::new(KEY_BASE));
        let mut sink = Sink(cx.present_mut());
        let mut compose = |list: &DrawList| {
            overlay.compose(&mut sink, &art, list, false, &[]);
            overlay.items().to_vec()
        };
        self.slices = Slices {
            labels: compose(&drawn.labels),
            under: compose(&drawn.under),
            windows: drawn
                .windows
                .iter()
                .map(|(k, _, list)| (k.clone(), compose(list)))
                .collect(),
            top: compose(&drawn.top),
        };
        // The pad's focus, over everything the layer draws.
        let mut focus = DrawList::default();
        self.pad.draw(&mut focus);
        if !focus.quads.is_empty() {
            let items = compose(&focus);
            self.slices.top.extend(items);
        }
        self.input.next_frame();
        raised
    }

    /// One frame of the pad (`None` with none) on the layer, at `time_ms`, read against what the
    /// layer drew last; `free` says whether nothing of the interface's stands over a place.
    pub fn pad(
        &mut self,
        pad: Option<crate::gamepad::PadState>,
        time_ms: u32,
        free: &dyn Fn(f32, f32) -> bool,
    ) {
        let dt = self
            .pad_ms
            .map_or(0.0, |ms| f64::from(time_ms.wrapping_sub(ms)) / 1000.0);
        self.pad_ms = Some(time_ms);
        let snap = self.input.nav.last.clone();
        let asking = self.layer.asking();
        self.pad
            .frame(pad, dt, time_ms, &snap, &mut self.input, asking, free);
    }

    /// Where the pad's focus is on the layer, if it has one.
    #[cfg(test)]
    #[must_use]
    pub fn pad_focus(&self) -> Option<dereth_ui_kit::draw::Rect> {
        self.pad.cursor.focus.map(|f| f.rect)
    }

    /// What stands over the interface's windows (an open choice's list, the host's own windows
    /// and question), the modern interface's stand-ins for the plugin windows, and the plugin bar
    /// over the screen: what a press on the layer is weighed against, apart from the layer.
    #[must_use]
    pub fn free_places(
        &self,
    ) -> (
        Vec<dereth_ui_kit::draw::Rect>,
        Vec<dereth_ui::ElemHandle>,
        Vec<dereth_ui_kit::draw::Rect>,
    ) {
        (
            self.layer.drawn().top_hits.clone(),
            self.seats.windows.values().copied().collect(),
            self.layer.bar_rect().into_iter().collect(),
        )
    }

    /// Where the layer stands over the classic interface's panels: what it draws over everything,
    /// each plugin window and the plugin bar. A plugin's HUD is not among them: the panels stand
    /// over it.
    #[must_use]
    pub fn over_classic(&self) -> Vec<dereth_ui_kit::draw::Rect> {
        let drawn = self.layer.drawn();
        drawn
            .top_hits
            .iter()
            .copied()
            .chain(drawn.windows.iter().map(|(_, r, _)| *r))
            .chain(self.layer.bar_rect())
            .collect()
    }

    /// Hand the layer one window event, before the interface sees it: whether the layer keeps
    /// it (the interface then never sees it). `free` says whether nothing of the interface's
    /// stands over a place on screen.
    pub fn route(
        &mut self,
        event: &HostEvent,
        time_ms: u32,
        free: &dyn Fn(f32, f32) -> bool,
    ) -> bool {
        let f = &mut self.input;
        match event {
            HostEvent::CursorMoved { x, y } => {
                #[allow(clippy::cast_possible_truncation)]
                {
                    f.mouse = (*x as f32, *y as f32);
                }
                false
            }
            HostEvent::MouseInput { button, pressed } => {
                let i = match button {
                    MouseButton::Left => 0,
                    MouseButton::Right => 1,
                    MouseButton::Middle => 2,
                    _ => return false,
                };
                let (mx, my) = f.mouse;
                if *pressed {
                    // A press on the layer: a plugin's window, HUD or bar, or the host's own.
                    if self.layer.hits(mx, my) && free(mx, my) {
                        f.down[i] = true;
                        f.pressed[i] = true;
                        if i == 0 {
                            f.note_left_press(time_ms);
                        }
                        self.held[i] = true;
                        return true;
                    }
                    // A press anywhere else lets go of the keyboard and any open choice.
                    self.layer.ui.let_go();
                    false
                } else {
                    f.down[i] = false;
                    if std::mem::take(&mut self.held[i]) {
                        f.released[i] = true;
                        return true;
                    }
                    false
                }
            }
            HostEvent::MouseWheel { notches } => {
                let (mx, my) = f.mouse;
                if self.layer.hits(mx, my) && free(mx, my) {
                    f.wheel += notches;
                    return true;
                }
                false
            }
            HostEvent::KeyboardInput { key, pressed, text } => {
                if matches!(key.virtual_key, 0x10 | 0xA0 | 0xA1) {
                    f.shift = *pressed;
                }
                if matches!(key.virtual_key, 0x11 | 0xA2 | 0xA3) {
                    f.ctrl = *pressed;
                }
                let escape = key.virtual_key == 0x1B && self.layer.ui.choosing();
                // A key goes to the line or box with the keyboard, and Escape to an open choice or
                // to the host's question while it is up, and never on to the game; its release
                // still does, so nothing the game holds stays held.
                let refuses = key.virtual_key == 0x1B && self.layer.asking();
                if *pressed && (self.layer.ui.typing() || escape || refuses) {
                    f.keys.push(key.virtual_key);
                    if let Some(t) = text {
                        f.chars.extend(t.chars().filter(|c| !c.is_control()));
                    }
                    return true;
                }
                false
            }
            _ => false,
        }
    }

    /// The interface is put away for another: the layer lets go of what it holds.
    pub fn suspend(&mut self) {
        self.layer.put_away();
        self.slices = Slices::default();
        self.input = InputFrame::default();
        self.held = [false; 3];
        self.pad.reset();
    }

    /// Whether nothing of the modern interface's stands over `(x, y)`: what is there is the game
    /// view or the bare screen the game is at (which the layer's HUDs stand over), one of the
    /// plugin windows' stand-ins, or nothing.
    pub fn modern_free(
        &self,
        ui: &dereth_ui::UiSystem,
        screen: Option<dereth_ui::ElemHandle>,
        x: f32,
        y: f32,
    ) -> bool {
        let seats: Vec<dereth_ui::ElemHandle> = self.seats.windows.values().copied().collect();
        let bar: Vec<dereth_ui_kit::draw::Rect> = self.layer.bar_rect().into_iter().collect();
        modern_free_at(ui, screen, &seats, &bar, x, y)
    }
}

/// Whether nothing of the modern interface's stands over `(x, y)`: what is there is the game view
/// or the bare screen the game is at, one of the plugin windows' stand-ins `seats`, the screen
/// under one of the layer's own surfaces drawn over it (`bar`, the plugin bar), or nothing.
/// Every other panel and control of the game's (its chat window and line, its toolbar, its radar)
/// is the game's, whatever HUD of a plugin's is drawn over it.
pub(crate) fn modern_free_at(
    ui: &dereth_ui::UiSystem,
    screen: Option<dereth_ui::ElemHandle>,
    seats: &[dereth_ui::ElemHandle],
    bar: &[dereth_ui_kit::draw::Rect],
    x: f32,
    y: f32,
) -> bool {
    let on_bar = bar.iter().any(|r| r.contains(x, y));
    let (x, y) = (
        dereth_primitives::num::to_i32(x),
        dereth_primitives::num::to_i32(y),
    );
    let Some(hit) = ui.hit_test_screen(x, y) else {
        return true;
    };
    let root = ui.root();
    let world = ui
        .node(hit)
        .is_some_and(|n| n.element_id() == dereth_ui_screens::hud::world_view::SMART_BOX);
    if hit == root || Some(hit) == screen || world {
        return true;
    }
    // The root element of the screen that holds what was hit.
    let mut top = hit;
    while let Some(p) = ui.parent(top) {
        if p == root {
            break;
        }
        top = p;
    }
    seats.contains(&top) || (Some(top) == screen && on_bar)
}

impl LayerHost {
    /// Bring the modern interface's stand-ins in line with the layer: each window's, as large as
    /// the window and stacked among the interface's windows (raised with a window a press brought
    /// to the front); the HUDs' just over the screen the game is at; the labels' in the game view.
    pub fn seat_in_modern(
        &mut self,
        ui: &mut dereth_ui::UiSystem,
        screen: Option<dereth_ui::ElemHandle>,
        view: Option<dereth_ui::ElemHandle>,
        raised: Option<&(String, String)>,
    ) {
        let drawn: Vec<((String, String), dereth_ui_kit::draw::Rect)> = self
            .layer
            .drawn()
            .windows
            .iter()
            .map(|(k, r, _)| (k.clone(), *r))
            .collect();
        let gone: Vec<(String, String)> = self
            .seats
            .windows
            .keys()
            .filter(|k| !drawn.iter().any(|(d, _)| d == *k))
            .cloned()
            .collect();
        for k in gone {
            if let Some(h) = self.seats.windows.remove(&k) {
                ui.remove_and_delete_root(h);
            }
        }
        for (k, r) in &drawn {
            let h = match self.seats.windows.get(k) {
                Some(h) if ui.node(*h).is_some() => *h,
                _ => {
                    let Some(h) = stand_in(ui, ui.root(), true) else {
                        continue;
                    };
                    self.seats.windows.insert(k.clone(), h);
                    h
                }
            };
            ui.move_to(
                h,
                dereth_primitives::num::to_i32(r.x),
                dereth_primitives::num::to_i32(r.y),
            );
            ui.resize_to(
                h,
                dereth_primitives::num::to_i32(r.w).max(1),
                dereth_primitives::num::to_i32(r.h).max(1),
            );
        }
        if let Some(h) = raised.and_then(|k| self.seats.windows.get(k)) {
            ui.bring_to_front(*h);
        }
        // The HUDs' stand-in, while there are HUDs, marks or a bar to draw: just over the screen
        // the game is at, under every other root.
        let under = match (
            !self.slices.under.is_empty(),
            self.seats.under.filter(|h| ui.node(*h).is_some()),
        ) {
            (true, Some(h)) => Some(h),
            (true, None) => stand_in(ui, ui.root(), false),
            (false, old) => {
                if let Some(h) = old {
                    ui.remove_and_delete_root(h);
                }
                None
            }
        };
        self.seats.under = under;
        if let Some(h) = under {
            let root = ui.root();
            if let Some(n) = ui.node_mut(root) {
                let list = &mut n.region.children;
                list.retain(|c| *c != h);
                let at = screen
                    .and_then(|s| list.iter().position(|c| *c == s))
                    .map_or(0, |i| i + 1);
                list.insert(at.min(list.len()), h);
            }
        }
        // The labels' stand-in, while there are labels to draw: the game view's last child,
        // under its panels.
        let view = view.filter(|_| !self.slices.labels.is_empty());
        match (view, self.seats.labels.filter(|h| ui.node(*h).is_some())) {
            (Some(v), Some(h)) if ui.parent(h) == Some(v) => {
                if ui.children(v).last() != Some(&h) {
                    ui.set_parent(h, Some(v));
                }
            }
            (Some(v), old) => {
                if let Some(h) = old {
                    ui.remove_and_delete_root(h);
                }
                self.seats.labels = stand_in(ui, v, false);
            }
            (None, old) => {
                if let Some(h) = old {
                    ui.remove_and_delete_root(h);
                }
                self.seats.labels = None;
            }
        }
    }

    /// Take the modern interface's stand-ins down (the interface is put away).
    pub fn unseat_from_modern(&mut self, ui: &mut dereth_ui::UiSystem) {
        for (_, h) in std::mem::take(&mut self.seats.windows) {
            ui.remove_and_delete_root(h);
        }
        for h in [self.seats.under.take(), self.seats.labels.take()]
            .into_iter()
            .flatten()
        {
            ui.remove_and_delete_root(h);
        }
    }

    /// Where in the modern interface's draw list each part of the layer is drawn: the labels at
    /// their stand-in, the HUDs' part at theirs, each window at its own; a part whose stand-in
    /// drew nothing goes over the list.
    pub fn modern_inserts<'s>(
        &'s self,
        cmds: &[dereth_ui::UiDrawCmd],
    ) -> Vec<(usize, &'s [OverlayItem])> {
        let at = |h: Option<dereth_ui::ElemHandle>| {
            h.and_then(|h| cmds.iter().position(|c| c.who == h))
                .unwrap_or(cmds.len())
        };
        let mut out = vec![
            (at(self.seats.labels), self.slices.labels.as_slice()),
            (at(self.seats.under), self.slices.under.as_slice()),
        ];
        for (k, items) in &self.slices.windows {
            out.push((at(self.seats.windows.get(k).copied()), items.as_slice()));
        }
        out
    }
}

/// A stand-in element under `parent`: a field over nothing, its one clear fill marking where it
/// is drawn; `blocks` makes it take the pointer, as the window it stands in for does.
fn stand_in(
    ui: &mut dereth_ui::UiSystem,
    parent: dereth_ui::ElemHandle,
    blocks: bool,
) -> Option<dereth_ui::ElemHandle> {
    #[derive(Debug)]
    struct NoAssets;
    impl dereth_primitives::AssetSource for NoAssets {
        fn read(
            &self,
            id: dereth_primitives::DataId,
        ) -> Result<Vec<u8>, dereth_primitives::AssetError> {
            Err(dereth_primitives::AssetError::NotFound(id))
        }
        fn exists(&self, _: dereth_primitives::DataId) -> bool {
            false
        }
        fn iter_type(
            &self,
            _: dereth_primitives::DataType,
        ) -> Box<dyn Iterator<Item = dereth_primitives::DataId> + '_> {
            Box::new(std::iter::empty())
        }
    }
    use std::sync::atomic::{AtomicU32, Ordering};
    static NEXT: AtomicU32 = AtomicU32::new(0);
    let id = dereth_ui::ElementId(SEAT_IDS + (NEXT.fetch_add(1, Ordering::Relaxed) & 0x00FF_FFFF));
    let desc = dereth_ui::ElementDesc {
        base: dereth_ui::StateDesc {
            incorporation: dereth_ui::desc::incorporation::LEGACY_ALL_GEOMETRY,
            width: 1,
            height: 1,
            ..dereth_ui::StateDesc::default()
        },
        element_id: id,
        ty: dereth_ui::factory::ty::FIELD,
        ..dereth_ui::ElementDesc::default()
    };
    let layout = dereth_ui::LayoutDesc {
        did: dereth_primitives::DataId(0),
        display_width: 800,
        display_height: 600,
        elements: std::iter::once((id, desc.clone())).collect(),
    };
    let h = ui
        .create_element_recursive_from_full_desc(&NoAssets, &layout, &desc)
        .ok()
        .flatten()?;
    ui.set_parent(h, Some(parent));
    ui.initialize_tree(h);
    if let Some(n) = ui.node_mut(h) {
        // A clear fill: drawn as nothing, but where it is drawn is marked in the draw list.
        n.region.surface_fills.push(dereth_ui::UiFill {
            x: 0,
            y: 0,
            w: 1,
            h: 1,
            color: 0,
        });
        if blocks {
            n.region.flags.block_clicks = true;
        }
    }
    ui.set_mouse_visible(h, blocks);
    Some(h)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (this client's own skin for plugin windows)

    /// The layer's text is the game's label font, every letter of it, with the sheet's coverage
    /// kept: a letter has pixels to draw.
    #[test]
    #[cfg_attr(not(feature = "retail-dats"), ignore = "reads the retail dats")]
    fn the_layer_s_text_is_the_game_s_label_font_and_its_letters_have_pixels() {
        let store = dereth_dat::testing::open_store_or_fail();
        let given = super::retail_font(&store).expect("the game's label font");
        assert!(given.font.glyphs.len() > 900, "{}", given.font.glyphs.len());
        let a = given.font.glyph('A').expect("its A");
        let page = &given.pages[0];
        let covered = (a.y..a.y + u16::from(a.h))
            .flat_map(|y| (a.x..a.x + u16::from(a.w)).map(move |x| (u32::from(x), u32::from(y))))
            .filter(|(x, y)| page.bgra[((y * page.width + x) * 4 + 3) as usize] > 128)
            .count();
        assert!(covered > 8, "the A has {covered} pixels");
    }
}
