//! Frame sequencing and media updates.

use crate::{
    media_image_blit_mode, msg, ElemHandle, GraphicRef, InputPump, MediaEffect, StateId, UiSystem,
};
use dereth_primitives::{DataId, LocalTime};

impl UiSystem {
    // ---- the frame ------------------------------------------------------------------------

    /// Behavior: the seven fixed steps, in order: clean the delete queue; process the deferred
    /// message removals; update the mouse if a hit test is pending; check the tooltip; broadcast
    /// global message 3; advance the input manager's time; draw the dirty regions.
    ///
    /// The last step is a no-op here: a D3D12 rebuild redraws the whole UI each frame
    /// ([`Self::draw`]), while preserving the three relevant drawing controls:
    /// draw-after-children, `BlitMode` and the alpha blend modifier.
    pub fn use_time(&mut self, now: LocalTime, input: &mut dyn InputPump) {
        self.now = now;
        // The client polls the input manager's shift-key query inside the two geometry functions that
        // want it. Nothing here can reach the pump from there, so the answer is latched once a
        // frame instead — the same value those functions would have read, sampled at step 0 of
        // `use_time` rather than mid-frame.
        self.shift_key_down = input.shift_key_down();
        self.clean_delete_queue();
        self.process_removal_data();
        if self.mouse.perform_hit_test {
            self.do_mouse_update(now);
        }
        // The truncation recalculation — see
        // [`Self::recalculate_truncation_tooltips`] for why it runs here rather than in the draw.
        // It must precede `check_tooltip`, which reads the has-tooltip flag it writes.
        self.recalculate_truncation_tooltips();
        self.check_tooltip(now);
        self.tick_media(now);
        self.broadcast_global(msg::global::TICK, 0);
        input.use_time(now);
    }

    /// The media machines' half of message 3. Split out only so it can be driven in a test without
    /// a listener table; `MediaPlayback` registers for message 3 exactly as the widgets do.
    pub(crate) fn tick_media(&mut self, now: LocalTime) {
        self.media_effects.clear();
        let live: Vec<ElemHandle> = self.element_list.clone();
        for h in live {
            let Some(n) = self.node(h) else { continue };
            if !n.media.registered_for_tick {
                continue;
            }
            let init = n.flags.is_initialized();
            let mut machine = match self.node_mut(h) {
                Some(n) => std::mem::take(&mut n.media),
                None => continue,
            };
            let fx = machine.update(now.0, &mut self.rng, init);
            if let Some(n) = self.node_mut(h) {
                n.media = machine;
            }
            for e in fx {
                self.apply_media_effect(h, e);
            }
        }
    }

    /// Apply one [`MediaEffect`] to its owning element, exactly as a media-machine update would.
    ///
    /// Public so a test can drive the client's two
    /// arms without building a media track for them.
    pub fn apply_media_effect(&mut self, h: ElemHandle, e: MediaEffect) {
        match &e {
            MediaEffect::SetImage { file, draw_mode } => {
                if let Some(n) = self.node_mut(h) {
                    n.region.image = file.map(|d| GraphicRef::opaque_surface(d, 0, 0));
                    n.region.blit_mode = media_image_blit_mode(*draw_mode);
                }
            }
            MediaEffect::SetAlphaImage { file } => {
                if let Some(n) = self.node_mut(h) {
                    n.region.alpha_image = file.map(|d| GraphicRef::opaque_surface(d, 0, 0));
                }
            }
            // The media machine's cursor step -> the element's set-cursor,
            // whose tail is the manager's cursor check. Without the
            // second line an animated cursor track would write the element's cursor and nothing
            // would ever look at it again.
            MediaEffect::SetCursor { file, hot_x, hot_y } => {
                self.element_set_cursor(h, *file, *hot_x, *hot_y);
            }
            // The cursor step's `INVALID_DID` arm -> the element's unset-cursor.
            MediaEffect::UnSetCursor => {
                self.element_unset_cursor(h);
            }
            MediaEffect::SetObjectAlpha(a) => {
                if let Some(n) = self.node_mut(h) {
                    n.region.alpha_blend_mod = *a;
                }
            }
            MediaEffect::BroadcastMessage { id } => {
                self.broadcast_element_message(h, *id, 0, 0);
            }
            MediaEffect::SetState { id } => {
                self.set_state(h, *id);
            }
            // The host's media player's, raised as a request.
            MediaEffect::PlaySound { .. } | MediaEffect::PlayMovie { .. } => {}
        }
        self.media_effects.push((h, e));
    }

    /// Behavior: replace an element's media machine with one
    /// immediate image and apply its draw mode.
    ///
    /// The original cleans up the media machine, inserts one image step, resets the
    /// current index to zero, and updates it. The resulting image effect reaches the draw path,
    /// where draw modes 2 and 3 select the three-alpha
    /// and four-alpha blits; every other value selects the normal blit. The immediate end state is
    /// enough here: a single image has no later tick to service.
    pub fn set_media_image(&mut self, h: ElemHandle, file: DataId, draw_mode: u32) {
        let Some(n) = self.node_mut(h) else { return };
        n.media.reset(&[]);
        n.region.image = (file.0 != 0).then(|| GraphicRef::opaque_surface(file, 0, 0));
        n.region.blit_mode = media_image_blit_mode(draw_mode);
    }

    /// Behavior: replace one declared state's complete
    /// media list with a single image, and update the live region immediately only when that is
    /// the element's current state.
    ///
    /// An undeclared state is a no-op, matching retail's state-table lookup.
    /// The state-media replacement clears every old media entry before installing the
    /// new type-5 image step; a later [`Self::set_state`] therefore runs exactly this image.
    pub fn set_media_image_for_state(
        &mut self,
        h: ElemHandle,
        file: DataId,
        draw_mode: u32,
        state: StateId,
    ) {
        let current = {
            let Some(n) = self.node_mut(h) else { return };
            let Some(desc) = n.desc.states.get_mut(&state) else {
                return;
            };
            desc.media.clear();
            desc.media.push(crate::desc::MediaDesc {
                media_type: 5,
                type_echo_ok: true,
                fields: crate::desc::MediaFields::Image { file, draw_mode },
            });
            n.state == state
        };
        if current {
            self.set_media_image(h, file, draw_mode);
        }
    }
}
