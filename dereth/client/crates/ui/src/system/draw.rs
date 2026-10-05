//! Element draw traversal and material state.

use crate::{
    diffuse_color, region, Box2D, DrawEvent, DrawStep, ElemHandle, ImageSource, UiDrawBackend,
    UiDrawCmd, UiFill, UiSystem,
};

impl UiSystem {
    /// Redraw the whole UI. Emits one [`UiDrawCmd`] per visible region in
    /// the region's draw order.
    ///
    /// **The layout pass runs here, because this is where the client runs it.**
    /// The original draw path recalculates a text control's glyph list at draw start, so no
    /// panel has to call [`crate::scrollable::recalculate_dirty_text`] by hand from its
    /// `set_text`; see that function's header for the whole reading.
    pub fn draw(&mut self, back: &mut dyn UiDrawBackend) {
        let root = self.root;
        crate::scrollable::recalculate_dirty_text(self, root);
        self.draw_region(root, None, 1.0, back);
    }

    /// The surface object's material-opacity setter, reached the way
    /// the chat interface's set-opacity reaches it.
    ///
    /// The client uses the element's surface object when present; otherwise it walks parents to
    /// the first surface object. If the walk reaches the top, it writes nothing. For the object it
    /// finds, it updates the material opacity through the object's material.
    ///
    /// So the value lands on the **nearest ancestor that owns a UI object, starting at the
    /// element itself** — the should-own-object flag, bit 14,
    /// which only a layout root and an authored `0xCD != 0` ever set. Every region below that
    /// ancestor composes into its surface and is multiplied by this alpha when the surface's quad is
    /// blitted; [`Self::draw`] carries exactly that product down the recursion.
    ///
    /// Returns the element that took it, so a caller can say *which* surface moved rather than
    /// only that something did.
    pub fn set_material_opacity(&mut self, h: ElemHandle, v: f32) -> Option<ElemHandle> {
        let mut cur = h;
        loop {
            let n = self.node(cur)?;
            if n.flags.should_own_object() {
                break;
            }
            cur = n.region.parent?;
        }
        self.node_mut(cur)?.region.material_opacity = v;
        Some(cur)
    }

    /// The material alpha of the surface `h` composes into — [`Self::set_material_opacity`]'s
    /// reader, over the same walk.
    #[must_use]
    pub fn material_opacity(&self, h: ElemHandle) -> f32 {
        let mut cur = h;
        loop {
            let Some(n) = self.node(cur) else { return 1.0 };
            if n.flags.should_own_object() {
                return n.region.material_opacity;
            }
            let Some(p) = n.region.parent else { return 1.0 };
            cur = p;
        }
    }

    /// `extra_clip` is the narrowing an ancestor's child-draw override imposed — see
    /// [`crate::element::Element::child_clip`]. It rides down the subtree because the client's clip box is
    /// a parameter of the per-element draw and is passed on to every child.
    ///
    /// `surface_alpha` is the material opacity of the UI object this element composes into —
    /// see [`Self::set_material_opacity`]. It replaces itself at every element that owns an
    /// object, and every command below that element carries `alpha_blend_mod * surface_alpha`:
    /// the region's own blit into the surface, then the surface's own quad.
    pub(crate) fn draw_region(
        &self,
        h: ElemHandle,
        extra_clip: Option<Box2D>,
        surface_alpha: f32,
        back: &mut dyn UiDrawBackend,
    ) {
        let Some(n) = self.node(h) else { return };
        if !n.region.flags.visible {
            return;
        }
        // The UI-object construction returns false without bit 14, and
        // such an element has no material of its own to modulate.
        let surface_alpha = if n.flags.should_own_object() {
            n.region.material_opacity
        } else {
            surface_alpha
        };
        let alpha = n.region.alpha_blend_mod * surface_alpha;
        // Step 1: bail out when the clip box is empty.
        let mut clip = self.screen_clip_box(h);
        if let Some(e) = extra_clip {
            clip = clip.intersect(&e);
        }
        if !clip.is_valid() {
            return;
        }
        let after = n.region.flags.draw_after_children;
        if after {
            self.draw_children(h, extra_clip, surface_alpha, back);
        }
        let screen = self.screen_box(h);
        // In the original draw path, the base operation blits the image while text-bearing
        // controls draw glyphs separately. The rebuild represents that relationship through
        // behavior composition, so a transparent element still draws its text: transparency
        // suppresses the background blit, not the glyph pass.
        let glyphs = n
            .behaviour
            .as_ref()
            .map(|b| b.compose_text(screen))
            .unwrap_or_default();
        // `TextElement`'s outline pass is a property of the element, read once per
        // self-draw alongside its glyphs, not once per glyph.
        let text_outline = n.behaviour.as_ref().and_then(|b| b.text_outline_color());
        // The self-draw's selection arm intersects each selected glyph's cell with the
        // **surface window** — against `(0, 0, clip_w, clip_h)`,
        // which is the clip box translated to the surface origin — and skips a cell that falls
        // outside it entirely. Done here, where both boxes are in hand, so the draw list carries
        // what retail computed rather than a rectangle the renderer has to re-clip.
        let invert: Vec<Box2D> = n
            .behaviour
            .as_ref()
            .map(|b| b.selection_boxes(screen))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                let hit = r.intersect(&clip).intersect(&screen);
                hit.is_valid().then_some(hit)
            })
            .collect();
        // Step 2: `EraseSelf` if `erase_background`, then whatever the widget itself
        // generated into its surface. The client erases the background
        // once per dirty rectangle; with no dirty-rectangle machinery the dirty region
        // is the whole clip box, which is the one rectangle the client produces for a full redraw.
        let mut fills: Vec<UiFill> = Vec::new();
        if n.region.flags.erase_background && clip.is_valid() {
            fills.push(UiFill {
                x: clip.x0,
                y: clip.y0,
                w: clip.width(),
                h: clip.height(),
                color: UiFill::NULL,
            });
        }
        if !n.region.flags.transparent || !glyphs.is_empty() || !fills.is_empty() {
            let transparent = n.region.flags.transparent;
            back.draw_region(&UiDrawCmd {
                who: h,
                screen,
                clip,
                image: if transparent {
                    None
                } else {
                    n.region.image.as_ref().map(|g| g.did)
                },
                image_op: if transparent {
                    None
                } else {
                    n.region.image.as_ref().and_then(|g| g.op)
                },
                image_source: n
                    .region
                    .image
                    .as_ref()
                    .map_or(ImageSource::Interface, |g| g.source),
                blit_mode: n.region.blit_mode,
                alpha_blend_mod: alpha,
                tiling_offset: n.region.tiling_offset,
                rotation_z_degrees: 0,
                color: diffuse_color(alpha),
                glyphs,
                text_outline,
                invert,
                fills,
            });
        }
        // **The caret.** The draw fills it *after* the
        // glyph loop, only when the element has focus and text bits `1` and `0x200` are both set,
        // and the caret is intersected with the surface window first.
        // It is a command of its own rather than an entry in `fills` above
        // because the renderer paints a command's fills *before* its glyphs — that is where
        // the self-erase belongs — and the caret must paint over the glyph it sits against. Issued
        // here, still inside the self-draw's turn, so it lies under this element's children exactly
        // as retail's surface does. The focus gate is the manager's focused element, applied
        // here because the behaviour cannot see it.
        if self.focus_element() == Some(h) {
            if let Some((caret, color)) = n.behaviour.as_ref().and_then(|b| b.caret(screen)) {
                let hit = caret.intersect(&clip);
                if hit.is_valid() {
                    back.draw_region(&UiDrawCmd {
                        who: h,
                        screen,
                        clip,
                        image: None,
                        image_op: None,
                        image_source: ImageSource::Interface,
                        blit_mode: n.region.blit_mode,
                        alpha_blend_mod: alpha,
                        tiling_offset: (0, 0),
                        rotation_z_degrees: 0,
                        color: diffuse_color(alpha),
                        glyphs: Vec::new(),
                        text_outline: None,
                        invert: Vec::new(),
                        fills: vec![UiFill {
                            x: hit.x0,
                            y: hit.y0,
                            w: hit.width(),
                            h: hit.height(),
                            color,
                        }],
                    });
                }
            }
        }
        if !after {
            self.draw_children(h, extra_clip, surface_alpha, back);
        }
        // **The generated pixels come after the children**, which is the one shipped draw-order
        // override that matters here: the radar draws its children first, then its blips, then the
        // four fills of the centre cross.
        // and the radar's own children include the dial face `0x1000003F`, a 120x120 opaque
        // graphic. Emitting the blips with the element's own blit would put them *under* the
        // dial, which is exactly what happened the first time. So they are a command of their
        // own, issued at the client's object-draw step.
        //
        // Coordinates are element-relative, as the client's `sx`/`sy` are; the draw
        // list is absolute.
        if !n.region.surface_fills.is_empty() {
            let fills = n
                .region
                .surface_fills
                .iter()
                .map(|f| UiFill {
                    x: screen.x0 + f.x,
                    y: screen.y0 + f.y,
                    w: f.w,
                    h: f.h,
                    color: f.color,
                })
                .collect();
            back.draw_region(&UiDrawCmd {
                who: h,
                screen,
                clip,
                image: None,
                image_op: None,
                image_source: ImageSource::Interface,
                blit_mode: n.region.blit_mode,
                alpha_blend_mod: alpha,
                tiling_offset: (0, 0),
                rotation_z_degrees: 0,
                color: diffuse_color(alpha),
                glyphs: Vec::new(),
                text_outline: None,
                invert: Vec::new(),
                fills,
            });
        }
    }

    pub(crate) fn draw_children(
        &self,
        h: ElemHandle,
        extra_clip: Option<Box2D>,
        surface_alpha: f32,
        back: &mut dyn UiDrawBackend,
    ) {
        // The child list iterates head to tail, so later children paint on top.
        for c in self.children(h) {
            // The one shipped override that narrows a child's clip is the meter's child-image
            // clip. Everything else returns `None` and pays nothing.
            let own = self
                .node(h)
                .and_then(|n| n.behaviour.as_ref())
                .and_then(|b| {
                    let cn = self.node(c)?;
                    b.child_clip(cn.element_id(), self.screen_box(c))
                });
            let ex = match (extra_clip, own) {
                (Some(a), Some(b)) => Some(a.intersect(&b)),
                (Some(a), None) => Some(a),
                (None, b) => b,
            };
            self.draw_region(c, ex, surface_alpha, back);
        }
    }

    /// The nine-step draw trace over the whole tree, for tests.
    #[must_use]
    pub fn draw_trace(&self) -> Vec<DrawEvent> {
        let mut out = Vec::new();
        self.trace_region(self.root, &mut out);
        out
    }

    pub(crate) fn trace_region(&self, h: ElemHandle, out: &mut Vec<DrawEvent>) {
        let Some(n) = self.node(h) else { return };
        if !n.region.flags.visible || !self.screen_clip_box(h).is_valid() {
            return;
        }
        for step in region::draw_steps(
            n.region.flags.erase_background,
            n.region.flags.draw_after_children,
        ) {
            out.push(DrawEvent { who: h, step });
            if step == DrawStep::DrawChildren {
                for c in self.children(h) {
                    self.trace_region(c, out);
                }
            }
        }
    }
}
