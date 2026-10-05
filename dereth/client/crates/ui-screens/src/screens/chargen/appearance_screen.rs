//! The appearance page: its set-up, input and read-outs.

use super::*;

impl CharGenScreen {
    // ---------------------------------------------------------------------------------------
    // The appearance page
    // ---------------------------------------------------------------------------------------

    /// The nine part choices, straight off the heritage's per-gender lists. Slot 4 (skin) is the
    /// one the client never fills.
    pub fn setup_parts(&mut self) {
        let Some(t) = self.tables.clone() else { return };
        let Some(sx) = self.state.sex(&t.chargen).cloned() else {
            return;
        };
        let n = |v: usize| i32::try_from(v).unwrap_or(0);
        let s = &self.state;
        self.choices[0] = PartChoice {
            current: s.hair_style,
            num: n(sx.hair_styles.len()),
            color: s.hair_color,
            num_colors: n(sx.hair_colors.len()),
            shade: s.hair_shade,
        };
        self.choices[1] = PartChoice {
            current: s.eyes_strip,
            num: n(sx.eye_strips.len()),
            color: s.eye_color,
            num_colors: n(sx.eye_colors.len()),
            shade: 0.0,
        };
        self.choices[2] = PartChoice {
            current: s.nose_strip,
            num: n(sx.nose_strips.len()),
            color: -1,
            num_colors: 0,
            shade: 0.0,
        };
        self.choices[3] = PartChoice {
            current: s.mouth_strip,
            num: n(sx.mouth_strips.len()),
            color: -1,
            num_colors: 0,
            shade: 0.0,
        };
        self.choices[5] = PartChoice {
            current: s.headgear_style,
            num: n(sx.headgear.len()),
            color: s.headgear_color,
            num_colors: s.num_headgear_colors,
            shade: s.headgear_shade,
        };
        self.choices[6] = PartChoice {
            current: s.shirt_style,
            num: n(sx.shirts.len()),
            color: s.shirt_color,
            num_colors: s.num_shirt_colors,
            shade: s.shirt_shade,
        };
        self.choices[7] = PartChoice {
            current: s.trousers_style,
            num: n(sx.pants.len()),
            color: s.trousers_color,
            num_colors: s.num_trousers_colors,
            shade: s.trousers_shade,
        };
        self.choices[8] = PartChoice {
            current: s.footwear_style,
            num: n(sx.footwear.len()),
            color: s.footwear_color,
            num_colors: s.num_footwear_colors,
            shade: s.footwear_shade,
        };
    }

    /// The two arrows inside a part row, `0x1000030A` (previous, `-1`) and `0x1000030B` (next,
    /// `+1`), whose *parent* names the part.
    ///
    /// The wrap is the client's, written out per part and identical for eight of the nine:
    /// `cur += d; if (cur < num) { if (cur < 0) cur = num - 1 } else { cur = 0 }`. **Headgear is
    /// the exception** — its floor is `-1`, "no hat", and it wraps off the top to `-1` rather than
    /// to 0, which is what makes bare-headed a reachable choice and is why ACE reads
    /// `HeadgearStyle == uint.MaxValue` as "no headgear".
    pub fn cycle_part(&mut self, ui: &mut UiSystem, part: EParts, d: i32) {
        let Some(t) = self.tables.clone() else { return };
        let i = part.choice_index();
        let (num, cur) = (self.choices[i].num, self.choices[i].current + d);
        let bare = part == EParts::Headgear;
        let cur = if cur < num {
            if cur < 0 && !(bare && cur == -1) {
                num - 1
            } else {
                cur
            }
        } else if bare {
            -1
        } else {
            0
        };
        self.choices[i].current = cur;
        match part {
            EParts::Hair => self.state.set_hair_style(cur),
            EParts::Eyes => self.state.eyes_strip = cur,
            EParts::Nose => self.state.nose_strip = cur,
            EParts::Mouth => self.state.mouth_strip = cur,
            EParts::Headgear => self.state.set_headgear_style(&t.chargen, cur),
            EParts::Shirt => self.state.set_shirt_style(&t.chargen, cur),
            EParts::Trousers => self.state.set_trousers_style(&t.chargen, cur),
            EParts::Footwear => self.state.set_footwear_style(&t.chargen, cur),
            EParts::Skin | EParts::Invalid => {}
        }
        self.set_selection(ui, part);
    }

    /// One of the nine part rows becomes the
    /// selected one, and the whole colour wheel is rebuilt for it.
    ///
    /// The tail takes the part's current colour and colour count, lights the current selection
    /// (state 6), builds the colour spots, then for Eyes draws the flat gradient disk and hides the
    /// shade slider, and for every other part draws the shaded disk, shows the slider and sets the
    /// part's shade; finally it writes the current colour.
    ///
    /// **Eyes is the one part with no shade**, which is why its disk is the flat `GradientPlug` and
    /// its slider is hidden.
    pub fn set_selection(&mut self, ui: &mut UiSystem, part: EParts) {
        self.current_part = part;
        self.setup_parts();
        // Selection writes the colour count; parts setup does not.
        self.fill_color_wheel();
        let i = part.choice_index();
        self.current_color = self.choices[i].color;
        // **The selection write's own head.** For each of the nine colour-wheel slots the client
        // shows the colour spot and hides its pointer; then it sets state 1 on the row that **was**
        // selected, and state 6 on the one that now is. Without the pointer half every one of the
        // nine markers the shipped layout starts visible stays visible for ever: **nine** gold
        // arrow heads around the wheel where retail shows exactly **one**, next to the chosen
        // colour. The colour write — called from this function's tail — is what puts that one back.
        if let Some(root) = self.roots.first().copied() {
            for (spot, pointer) in appearance::COLOR_SPOTS
                .iter()
                .zip(appearance::COLOR_POINTERS.iter())
            {
                if let Some(h) = ui.get_child_recursive(root, *spot) {
                    ui.set_visible(h, true);
                }
                if let Some(h) = ui.get_child_recursive(root, *pointer) {
                    ui.set_visible(h, false);
                }
            }
            for (row, state) in [
                (self.current_selection_row, STATE_ROW_UNSELECTED),
                (Self::selection_row(part), STATE_ROW_SELECTED),
            ] {
                let Some(id) = row else { continue };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_state(h, state);
                }
            }
        }
        self.current_selection_row = Self::selection_row(part);
        self.do_color_spots(ui);
        let eyes = part == EParts::Eyes;
        if let Some(root) = self.roots.first().copied() {
            if let Some(h) = ui.get_child_recursive(root, appearance::SHADE_SCROLL) {
                ui.set_visible(h, !eyes);
            }
        }
        self.set_color(ui, self.current_color);
        self.refresh_view(ui);
    }

    /// The Face / Clothes tabs — shows one of the two row groups.
    ///
    /// **The model half** is the whole of the held headgear:
    ///
    /// Face lights the Face radio (`0x10000017`, Clothes `0x10000016`), parks the headgear style in
    /// the held headgear and sets the live one to -1 (bare the head so the face is visible), shows
    /// the face rows and zooms in. Clothes saves the camera start, lights the Clothes radio,
    /// restores the held headgear when `-2 < held < headgear choice count`, shows the clothes rows
    /// and zooms out.
    ///
    /// The restore guard is `-2 <`, not `-1 <`: **-1 is a legal headgear style** ("no hat", the
    /// value ACE reads as `HeadgearStyle == uint.MaxValue`), so a player who took the hat off on
    /// the Clothes tab keeps it off. Choice slot 5 is [`EParts::Headgear`]'s row.
    pub fn set_choice(&mut self, ui: &mut UiSystem, face: bool) {
        if let Some(t) = self.tables.clone() {
            if face {
                self.hold_headgear = self.state.headgear_style;
                self.state.set_headgear_style(&t.chargen, -1);
            } else if self.hold_headgear > -2
                && self.hold_headgear < self.choices[EParts::Headgear.choice_index()].num
            {
                self.state
                    .set_headgear_style(&t.chargen, self.hold_headgear);
            }
        }
        self.apply_choice_view(ui, face);
    }

    /// The appearance choice's *view* half — the two row groups, the two radios, and the
    /// camera — with nothing that touches [`CharGenState`].
    ///
    /// It is separate because the per-frame update applies the **whole** Face choice on exactly
    /// three of the thirteen heritages — 6 (Gear Knight) and the two Olthoi, which have no Clothes
    /// tab — and on the other ten calls nothing of the sort. A rebuild that ran the full choice
    /// write from every update would park -1 in [`Self::hold_headgear`] the second time round and
    /// lose the hat for good.
    fn apply_choice_view(&mut self, ui: &mut UiSystem, face: bool) {
        self.face_tab = face;
        if let Some(root) = self.roots.first().copied() {
            for (id, want) in [
                (appearance::GROUP_FACE, face),
                (appearance::GROUP_CLOTHES, !face),
            ] {
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, want);
                }
            }
            // **The two radios**, which the choice write sets; without them no radio is selected.
            // The same `0x10000016`/`0x10000017` pair the heritage and profession buttons use.
            for (id, on) in [
                (appearance::TAB_FACE, face),
                (appearance::TAB_CLOTHES, !face),
            ] {
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_state(
                        h,
                        if on {
                            STATE_PROFESSION_ON
                        } else {
                            STATE_PROFESSION_OFF
                        },
                    );
                }
            }
        }
        // **The camera half.** The choice path's last statement is a zoom in on the
        // Face arm and a zoom out on the Clothes arm, so the Face tab is a head-and-shoulders view
        // and the Clothes tab a whole-body one -- and the page opens on Face, which is why retail
        // shows a face on arrival rather than a whole body. `zoom`'s own guard
        // is the client's — an unchanged zoom state re-latches the pressed half's button and moves
        // nothing else — so calling it from every repaint is what the update's tail does and not a
        // second zoom.
        self.zoom(ui, face);
    }

    /// The nine spots, gated on the part's colour count.
    ///
    /// Nose, mouth and skin have no colour arm at all: skin is a shade only.
    pub fn set_color(&mut self, ui: &mut UiSystem, index: i32) {
        let i = self.current_part.choice_index();
        if index >= self.choices[i].num_colors {
            return;
        }
        // The client's first two statements: the marker over the **old** spot goes away and the one
        // over the new spot appears, each guarded by `!= -1`. This is the wheel answering the
        // click. Both are done through the same element list the page binds
        // as the colour-wheel pointers.
        if let Some(root) = self.roots.first().copied() {
            for (slot, want) in [(self.current_color, false), (index, true)] {
                let Some(id) = usize::try_from(slot)
                    .ok()
                    .and_then(|s| appearance::COLOR_POINTERS.get(s).copied())
                else {
                    continue;
                };
                if let Some(h) = ui.get_child_recursive(root, id) {
                    ui.set_visible(h, want);
                }
            }
        }
        self.current_color = index;
        self.choices[i].color = index;
        match self.current_part {
            EParts::Hair => self.state.set_hair_color(index),
            EParts::Eyes => self.state.set_eye_color(index),
            EParts::Headgear => self.state.headgear_color = index,
            EParts::Shirt => self.state.shirt_color = index,
            EParts::Trousers => self.state.trousers_color = index,
            EParts::Footwear => self.state.footwear_color = index,
            EParts::Nose | EParts::Mouth | EParts::Skin | EParts::Invalid => {}
        }
        // The gradient disk is drawn flat only for Eyes (the flag is set on that arm of the switch
        // above and nowhere else), so every other part re-tints the ring by the colour just chosen.
        //
        self.do_grad_disk(ui, self.current_part == EParts::Eyes);
        self.refresh_view(ui);
    }

    /// One slider, six destinations, and **nose,
    /// mouth and skin all write the skin shade**, which is the client's own fall-through.
    pub fn set_shade(&mut self, ui: &mut UiSystem, shade: f64) {
        let i = self.current_part.choice_index();
        self.choices[i].shade = shade;
        match self.current_part {
            EParts::Hair => self.state.set_hair_shade(shade),
            EParts::Nose | EParts::Mouth | EParts::Skin => self.state.set_skin_shade(shade),
            EParts::Headgear => self.state.headgear_shade = shade,
            EParts::Shirt => self.state.shirt_shade = shade,
            EParts::Trousers => self.state.trousers_shade = shade,
            EParts::Footwear => self.state.footwear_shade = shade,
            EParts::Eyes | EParts::Invalid => {}
        }
        self.refresh_view(ui);
    }

    /// Set the gender, then update the appearance state and
    /// preview.
    pub fn choose_gender(&mut self, ui: &mut UiSystem, gender: u32) {
        let Some(t) = self.tables.clone() else { return };
        self.state.set_gender(&t.chargen, gender);
        self.appearance_update(ui);
    }

    /// Set the zoom-in or zoom-out camera target. The client eases the camera
    /// through its zoom animation; the target is what the preview is asked for and the ease is the
    /// host's, so only the target crosses the seam.
    ///
    /// **The buttons' own view** is the whole of what those two functions
    /// do besides the camera. Both set button state through the same operation that the update's
    /// gender arm uses, on **both** halves, and they are not symmetrical.
    ///
    /// Zoom in, when not already zoomed in, sets the targets, puts the zoom-in button in state
    /// **6** and zoom-out in **1**, stops the animation and marks the view zoomed in; zoom out,
    /// when zoomed in, sets the targets, puts zoom-in in **1** and zoom-out in **6**, starts the
    /// animation and clears the flag. When the state already matches, each only re-latches its own
    /// button to 6.
    ///
    /// `6` is `widgets::button::state::TOGGLED` and `1` is `NORMAL`, so **exactly one half is ever lit**:
    /// the one last pressed. The `if` that the client's own guard falls through to still
    /// re-latches the pressed half, which is why a second `+` on an already-zoomed view is not a
    /// no-op even though nothing moves.
    ///
    /// The animation half is the same pair: the client's tail starts the animation when not zoomed
    /// in and stops it otherwise, so the Face tab holds a rest pose and the Clothes tab turns.
    pub fn zoom(&mut self, ui: &mut UiSystem, in_: bool) {
        if self.view3d.zoomed_in == in_ {
            // The client's fall-through, and it touches **one** button rather than the pair: zoom
            // in re-latches the zoom-in button to 6, and zoom out the mirror image. The half that
            // is already active is re-latched; the other is left exactly as it was.
            self.set_zoom_button_states(
                ui,
                if in_ { Some(true) } else { None },
                if in_ { None } else { Some(true) },
            );
            return;
        }
        self.view3d.zoomed_in = in_;
        self.view3d.camera_position = if in_ {
            zoomed_in_camera(self.state.heritage_group)
        } else {
            zoomed_out_camera(self.state.heritage_group)
        };
        self.view3d.camera_direction = [0.0, 0.0, 0.0];
        // Stop the animation on the way in, start it on the way out.
        self.view3d.animating = !in_;
        self.set_zoom_button_states(ui, Some(in_), Some(!in_));
        self.refresh_view(ui);
    }

    /// The two state writes the zoom in/out steps make, as one statement of which half is lit.
    ///
    /// Written as a pair rather than as "light the pressed one" because the changing path writes both,
    /// and lighting only the pressed half means that after `+` then `-` **both halves stay lit at
    /// once**, because nothing ever puts one
    /// back to `NORMAL`.
    /// `None` leaves that half alone, which is what the two fall-through paths do.
    fn set_zoom_button_states(
        &mut self,
        ui: &mut UiSystem,
        zoom_in_lit: Option<bool>,
        zoom_out_lit: Option<bool>,
    ) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        use dereth_ui::widgets::button::state::{NORMAL, TOGGLED};
        for (id, lit) in [
            (appearance::ZOOM_IN, zoom_in_lit),
            (appearance::ZOOM_OUT, zoom_out_lit),
        ] {
            let Some(lit) = lit else { continue };
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_state(h, if lit { TOGGLED } else { NORMAL });
            }
        }
    }

    /// The appearance page's update.
    pub fn appearance_update(&mut self, ui: &mut UiSystem) {
        // The client's head: the two gender buttons, `0x10000017` on the chosen one and
        // `0x10000016` on the other. A gender of 0 leaves both alone, which is the client's own
        // `goto`.
        if let Some(root) = self.roots.first().copied() {
            let female = match self.state.gender {
                1 => Some(false),
                2 => Some(true),
                _ => None,
            };
            if let Some(female) = female {
                for (id, on) in [
                    (appearance::GENDER_FEMALE, female),
                    (appearance::GENDER_MALE, !female),
                ] {
                    if let Some(h) = ui.get_child_recursive(root, id) {
                        ui.set_state(
                            h,
                            if on {
                                STATE_PROFESSION_ON
                            } else {
                                STATE_PROFESSION_OFF
                            },
                        );
                    }
                }
            }
        }
        // The client's heritage-6 arm, which is the **third** call site of the randomiser and an
        // edge, not a repaint: when the heritage is 6 and the last heritage was not, the appearance
        // and the clothing are randomised — a Gear Knight is re-rolled the first time the page
        // shows one and never again. The last heritage is written on that arm only, so leaving
        // heritage 6 and coming back re-rolls.
        if self.state.heritage_group == HERITAGE_GEAR_KNIGHT {
            if self.last_heritage_group != HERITAGE_GEAR_KNIGHT {
                if let Some(t) = self.tables.clone() {
                    self.state.randomize_appearance(&t.chargen, false);
                    self.state.randomize_clothing(&t.chargen, true);
                }
            }
            self.last_heritage_group = self.state.heritage_group;
        }
        // The client's three arms: the special heritages' chrome is in
        // [`Self::apply_special_heritage_chrome`], and the choice/selection third below.
        let special = matches!(
            self.state.heritage_group,
            HERITAGE_GEAR_KNIGHT | HERITAGE_OLTHOI | HERITAGE_OLTHOI_ACID
        );
        self.apply_special_heritage_chrome(ui);
        // **Not `set_choice`.** The update runs the full Face choice only on heritage 6 and the two
        // Olthoi, which have no Clothes tab; on the other ten it leaves the current tab alone.
        // Running the model half from here would park -1 in [`Self::hold_headgear`] on every
        // repaint. See [`Self::apply_choice_view`].
        if special {
            self.set_choice(ui, true);
            self.set_selection(ui, EParts::Hair);
        } else {
            // The view half only. Initialization already runs the Face model choice once for
            // ordinary heritages, so repeating it here would discard held headgear.
            self.apply_choice_view(ui, self.face_tab);
        }
        // The client's own tail, and the line that fills the colour wheel on the way in:
        // parts setup followed by selection of the current part. Without it the nine spots are
        // never generated at all until a part row or a tab is clicked -- nine black holes on a page
        // reached through its own tab.
        self.setup_parts();
        let part = self.current_part;
        self.set_selection(ui, part);
        self.refresh_view(ui);
        // The update's own tail: when visible, it updates the preview, sets the camera, and starts
        // the animation when not zoomed in (stops it otherwise). The Face tab holds a rest pose and
        // the Clothes tab turns.
        self.view3d.animating = !self.view3d.zoomed_in;
    }

    /// The client's **three arms**, everything but the choice-and-selection pair.
    ///
    /// Heritage **6** (Gear Knight) and the two Olthoi (`0x0C`, `0x0D`) do not get the ordinary
    /// page:
    ///
    /// | | ordinary (10) | Gear Knight (6) | Olthoi (0x0C, 0x0D) |
    /// |---|---|---|---|
    /// | Clothes button, Nose spinner, Mouth spinner | visible | hidden | hidden |
    /// | Hair / Eyes / Skin captions | [`APPEARANCE_CAPTIONS_ORDINARY`] | [`APPEARANCE_CAPTIONS_GEAR`] | [`APPEARANCE_CAPTIONS_OLTHOI`] |
    /// | the Eyes row's two arrows, attribute `0x0D` | `false`  | `true`  | `true`  |
    /// | Skin row y position | `0xB4`  | `0x5A`  | `0x5A`  |
    ///
    /// The client uses its visibility and position-change operations here, and the
    /// Boolean-attribute setter for the arrows. The resulting state changes establish those roles.
    ///
    /// **Why the whole page is a Face tab for these three.** They have no clothing at all, so
    /// hiding the Clothes button makes the Face choice the only reachable state; and with the Nose
    /// and Mouth rows gone the Skin row is the third of three rather than the fifth of five, which
    /// is the move.
    ///
    /// The camera each arm parks is [`zoomed_in_camera`]'s value for that heritage — the arms set
    /// both target and current camera position to it before applying the camera — so it is not
    /// repeated here; [`Self::refresh_view`] already asks for the same three positions.
    fn apply_special_heritage_chrome(&mut self, ui: &mut UiSystem) {
        let Some(root) = self.roots.first().copied() else {
            return;
        };
        let captions = match self.state.heritage_group {
            HERITAGE_GEAR_KNIGHT => APPEARANCE_CAPTIONS_GEAR,
            HERITAGE_OLTHOI | HERITAGE_OLTHOI_ACID => APPEARANCE_CAPTIONS_OLTHOI,
            _ => APPEARANCE_CAPTIONS_ORDINARY,
        };
        let special = captions != APPEARANCE_CAPTIONS_ORDINARY;
        // The Clothes tab and the two facial spinners a heritage with no nose and no clothes has
        // nothing to put in.
        for id in [
            appearance::TAB_CLOTHES,
            appearance::NOSE_ROW,
            appearance::MOUTH_ROW,
        ] {
            if let Some(h) = ui.get_child_recursive(root, id) {
                ui.set_visible(h, !special);
            }
        }
        // The three row labels.
        for (id, token) in [
            appearance::HAIR_ROW,
            appearance::EYES_ROW,
            appearance::SKIN_ROW,
        ]
        .iter()
        .zip(captions)
        {
            let text = self.string(ui, token);
            if let Some(t) = ui
                .get_child_recursive(root, *id)
                .and_then(|h| ui.text_element_mut(h))
            {
                t.set_text(&text);
            }
        }
        // The Eyes row's own two arrows: a Gear Knight and an Olthoi have one eye strip each, so
        // the arrows are greyed rather than hidden. The page initialisation does the same to the
        // **Skin** row's pair once at construction, on every heritage, because skin is a colour and
        // never a style.
        for (row, disabled) in [
            (appearance::EYES_ROW, special),
            (appearance::SKIN_ROW, true),
        ] {
            let Some(r) = ui.get_child_recursive(root, row) else {
                continue;
            };
            for arrow in [appearance::ARROW_PREV, appearance::ARROW_NEXT] {
                if let Some(h) = ui.get_child_recursive(r, arrow) {
                    ui.set_attribute_bool(h, dereth_ui::props::attr::DISABLED, disabled);
                }
            }
        }
        // And the Skin row slides up into the gap.
        if let Some(h) = ui.get_child_recursive(root, appearance::SKIN_ROW) {
            ui.move_to(
                h,
                0,
                if special {
                    SKIN_ROW_Y_SPECIAL
                } else {
                    SKIN_ROW_Y_ORDINARY
                },
            );
        }
    }
}
