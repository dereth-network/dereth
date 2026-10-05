//! Character, equipment and examination preview updates with independent state.

use super::{element_is_drawn, Ui};
use crate::platform::host::Host;
use dereth_primitives::DataId;
/// What the 3D character preview space was last built and animated for.
///
/// # What this key is a superset OF
///
/// The gated block does two things and this key holds the inputs of both. It **rebuilds** the space
/// when the model setup changes, when the background setup changes,
/// or when the dressed `ObjDesc` changes — [`Self::same_space`], the
/// three fields that decide a rebuild. It then **re-issues the sequence** from the animating state
/// (starting animation or parking it) and from whichever of
/// `CharGenScreen::view3d`'s two `UIASSET` enum values that arm selects.
///
/// # Why the two animation enums are in the key
///
/// [`Self::animation_enum`] and [`Self::rest_animation_enum`] are read by the body this key
/// gates. Without them, a heritage change that moved the animation enums without moving the
/// setup would leave the turntable playing the previous heritage's animation.
///
/// It cannot happen on the shipped table (`CharGen_CharacterData 0x0E000002`, checked by a test).
/// Character-generation preview initialization selects between **three** enum groups (default,
/// Olthoi `0x0C`, OlthoiAcid `0x0D`), all three distinct, and the two Olthoi heritages' setups
/// are `0x02001A21` and `0x02001A20` — different, so a key without the enums would still fire
/// on every reachable transition.
///
/// **The near miss is why the enums are in the key anyway:** those two heritages
/// share one `environment_setup` (`0x020005A4`), and both sexes inside each Olthoi heritage share
/// one setup — so the heritage table demonstrably *does* reuse ids across rows, and containment by
/// "the shipped data happens not to collide on this column" is a property of the dats, not of this
/// code. A DDD patch, or a heritage row nobody has looked at, ends it silently.
///
/// This is deliberately **not** the audio guard's case: retail's update re-reads the enums from the same
/// heritage-table row every call, and this rebuild assumes nothing more than the client does. The
/// key is simply a projection of its own body.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct ChargenPreviewKey {
    /// The model setup id.
    pub(super) setup: DataId,
    /// Whether the preview is animating.
    pub(super) animating: bool,
    /// The background setup id from the heritage row's environment setup.
    pub(super) bg_setup: DataId,
    /// The **dressed** part array, because the preview's meshes are baked from it.
    pub(super) objdesc: dereth_animation::parts::ObjDesc,
    /// The `UIASSET` enum played while `animating`.
    pub(super) animation_enum: u32,
    /// …and the one used for the parked pose when it is not.
    pub(super) rest_animation_enum: u32,
}

impl ChargenPreviewKey {
    /// Every field, taken off the view the gated block itself reads.
    ///
    /// A constructor rather than a struct literal at the call site so that the *correspondence*
    /// between the key's fields and `Cg3dView`'s has a station: a test can build a `Cg3dView`, move
    /// one field, and require the key to follow — which is the half a struct-equality assertion
    /// cannot see, since a derived `PartialEq` compares whatever fields happen to be there.
    pub(super) fn from_view(
        v: &dereth_ui_screens::screens::chargen::Cg3dView,
        objdesc: dereth_animation::parts::ObjDesc,
    ) -> Self {
        Self {
            setup: v.setup,
            animating: v.animating,
            bg_setup: v.bg_setup,
            objdesc,
            animation_enum: v.animation_enum,
            rest_animation_enum: v.rest_animation_enum,
        }
    }

    /// The three fields that decide a **rebuild** of the space, as opposed to a re-issue of the
    /// sequence. The animation enums are deliberately outside this: changing which animation plays
    /// must not tear down and reload the model, which is exactly what starting and
    /// stopping the animation being separate calls at the end of the preview update means.
    pub(super) fn same_space(&self, other: &Self) -> bool {
        self.setup == other.setup
            && self.bg_setup == other.bg_setup
            && self.objdesc == other.objdesc
    }
}

/// Advance one preview's clock before its visibility or resource guards.
fn preview_delta(now: f64, previous: &mut f64) -> f64 {
    let dt = (now - *previous).clamp(0.0, 0.25);
    *previous = now;
    dt
}

/// Convert the inclusive clipped element box into a nonnegative device viewport.
#[allow(clippy::cast_sign_loss)]
fn preview_viewport(area: dereth_ui::region::Box2D) -> dereth_render::camera::Viewport {
    dereth_render::camera::Viewport {
        x: area.x0.max(0) as u32,
        y: area.y0.max(0) as u32,
        width: (area.x1 - area.x0.max(0) + 1).max(0) as u32,
        height: (area.y1 - area.y0.max(0) + 1).max(0) as u32,
    }
}

/// One read of the wizard or barber, including its clipped viewport and dressing inputs.
struct CreationPreview {
    view3d: dereth_ui_screens::screens::chargen::Cg3dView,
    who: dereth_ui::ElemHandle,
    area: dereth_ui::region::Box2D,
    state: dereth_chargen::CharGenState,
    tables: Option<std::rc::Rc<dereth_ui_screens::screens::chargen::CharGenTables>>,
}

impl<H: Host> Ui<'_, '_, H> {
    /// The backpack panel's paper doll.
    ///
    /// The third consumer of the creature-mode preview space, and the only one whose object is
    /// **dressed**. The preview initialization and later redressing behavior,
    /// in full:
    ///
    /// * the viewport is `0x100001D5`; camera `(0.12, -2.4, 0.88)` looking at the origin, one
    ///   `DISTANT_LIGHT` at **2.0** along `(0.3, +1.9, 0.65)`, and sharp mode;
    /// * the object is built from the player -- a **clone of the player's own
    ///   physics object**, so its setup record is the player's, not a heritage default;
    /// * `set_heading(191.3679, 1)` and `set_sequence_animation(<DID for enum 0x10000005>, clear =
    ///   1, low = 1, **0.0** fps)` -- a held pose, not a loop;
    /// * apply the player's visual descriptor to the clone, which is what
    ///   puts the player's *worn gear* on the doll. The player-visual descriptor writer receives
    ///   `0xF625 Item_ObjDescEvent` for the player, which triggers the update.
    ///   On this side it is `ObjectStream::presence(player).objdesc` -- the
    ///   same value `WorldScene::apply_player_objdesc` already dresses the walking body with, and
    ///   **not** the char-gen `ObjDesc`, which is assembled from `CharGenState` and a
    ///   `ClothingTable` and describes a character who does not exist yet.
    /// * receipt of the player description then reads `PropertyInt 0xBC HeritageGroup`
    ///   and applies the heritage-specific camera adjustment for six heritages
    ///   and falls through for the rest.
    ///
    /// The visibility gate is not decoration: `Renderer::draw_ui`'s fallback arm draws a queued
    /// space whose element emitted no blit command, which is right for the portal space (a
    /// transparent full-screen region) and would put the doll over the world whenever the backpack
    /// panel was *closed*. So the queue only happens for an element that is visible with every
    /// ancestor visible -- the same first test used by the region draw path.
    pub(super) fn paper_doll_use_time(&mut self) {
        use dereth_ui_screens::panels::inventory::paper_doll as pd;
        use dereth_ui_screens::screens::gameplay_host::GameCall;

        let now = self.cx.now();
        let dt = preview_delta(now, &mut self.front.paper_doll_last_time);

        // Where it draws, and whether it draws at all.
        let where_ = self.front.ui.as_mut().and_then(|shell| {
            let screen = crate::hud_drive::game_screen(&mut shell.flow)?;
            let GameCall::PaperDollViewport(h) = crate::hud_drive::game_call(
                &mut shell.ui,
                screen,
                GameCall::PaperDollViewport(None),
            ) else {
                return None;
            };
            let h = h?;
            if !element_is_drawn(&shell.ui, h) {
                return None;
            }
            let b = shell.ui.screen_clip_box(h);
            b.is_valid().then_some((h, b))
        });
        let Some((who, area)) = where_ else { return };

        // The clone's setup record and the descriptor that dresses it. Both are the *player's*, read
        // off the object stream by player id on this side of the seam.
        let player = self.cx.objects().player().or(self.cx.hud().player);
        let dress = player
            .and_then(|p| self.cx.objects().presence(p))
            .and_then(|p| {
                Some((
                    p.setup_id?,
                    crate::world::to_anim_objdesc(&p.objdesc),
                    p.scale,
                ))
            });
        let Some((setup, objdesc, scale)) = dress else {
            return;
        };

        let assets = std::sync::Arc::clone(self.cx.anim_assets());
        let store = std::sync::Arc::clone(self.cx.store());
        let id = crate::gpu::PreviewId::PaperDoll;
        let fresh = self.cx.present_mut().preview_ensure(id, &assets);
        if fresh {
            self.cx.present_mut().preview_use_sharp_mode(id);
            self.cx.present_mut().preview_set_light(
                id,
                dereth_client_contract::overlay::PreviewLight::Directional,
                pd::LIGHT_INTENSITY,
                dereth_primitives::Vec3::new(
                    pd::LIGHT_DIRECTION.0,
                    pd::LIGHT_DIRECTION.1,
                    pd::LIGHT_DIRECTION.2,
                ),
            );
        }

        // The space's own 45-degree lens, every frame: the space is shared with the classic
        // interface, whose doll has a lens of its own.
        self.cx
            .present_mut()
            .preview_set_fov(id, dereth_world_render::creature_mode::DEFAULT_FOV_RADIANS);

        // Rebuild when no preview object exists or its visual descriptor changed, because the
        // meshes are baked from the dressed part array. See [`Self::paper_doll_built`].
        let look = self.cx.present().objects_in_other_look();
        if self.front.paper_doll_built.as_ref() != Some(&(setup, objdesc.clone(), look)) {
            self.cx.present_mut().preview_remove_all_objects(id);
            match self.cx.present_mut().preview_add_object_dressed(
                id,
                &store,
                setup,
                Some(&objdesc),
            ) {
                Ok(Some(_)) => {
                    let anim = crate::assets::enum_did(
                        &*store,
                        crate::preview::UIASSET_GROUP,
                        self.paper_doll_animation_enum(),
                    );
                    self.cx
                        .present_mut()
                        .preview_set_heading(id, 0, pd::HEADING_DEGREES);
                    match anim {
                        Some(a) => {
                            if !self.cx.present_mut().preview_set_sequence_animation(
                                id,
                                0,
                                a,
                                true,
                                pd::LOW_FRAME,
                                pd::FRAMERATE,
                            ) {
                                tracing::warn!(target: "dereth_client_shell::front_end", "the paper-doll animation {a:?} is not in the dat");
                            }
                        }
                        None => {
                            tracing::warn!(target: "dereth_client_shell::front_end", "UIASSET PaperDollAnimation does not resolve")
                        }
                    }
                    self.front.paper_doll_built = Some((setup, objdesc, look));
                }
                Ok(None) => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "the paper-doll setup {setup:?} would not load");
                    self.front.paper_doll_built = None;
                }
                Err(e) => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "the paper-doll space failed: {e}");
                    self.front.paper_doll_built = None;
                }
            }
        }

        self.cx.present_mut().preview_set_scale(id, 0, scale);

        // Re-apply the heritage camera every tick because setting the camera is idempotent and
        // heritage arrives with `0x0013`, which may be after the first build.
        let heritage = self.paper_doll_heritage();
        let camera = pd::for_race(heritage).map_or(pd::CAMERA_POSITION, |(c, _)| c);
        self.cx.present_mut().preview_set_camera_position(
            id,
            dereth_primitives::Vec3::new(camera.0, camera.1, camera.2),
        );
        self.cx.present_mut().preview_set_camera_direction(
            id,
            dereth_primitives::Vec3::new(
                pd::CAMERA_TARGET.0,
                pd::CAMERA_TARGET.1,
                pd::CAMERA_TARGET.2,
            ),
        );
        // Zero framerate, so this advances nothing -- it is here because the preview update is
        // what fires the animation's hooks, and because giving the doll a moving pose later must
        // not also require adding the tick.
        self.cx.present_mut().preview_use_time(id, dt);

        // The paper-doll selection notice calls
        // the part-selection-lighting start for the item when an item is selected. On that edge
        // (see [`Self::paper_doll_selection_seen`]) it then listens to global message 3, which
        // drives the part-selection-lighting update, the per-frame tick, on the doll's own object.
        let selected = self.cx.model().selected;
        if self.front.paper_doll_selection_seen != selected {
            self.front.paper_doll_selection_seen = selected;
            if let Some(item) = selected {
                let world = self.cx.model();
                let inventory = player.and_then(|p| world.tables.inventories.get(p));
                let upper = |loc: u32| inventory.and_then(|inv| inv.upper_inv_obj(loc));
                let mask = crate::preview::PaperDollSelectionLighting::selection_mask_from_object(
                    item,
                    world.player,
                    &upper,
                );
                let doll = self.cx.present_mut().preview_part_array_mut(id, 0);
                self.front.paper_doll_lighting.begin(mask, now, doll);
            }
        }
        let doll = self.cx.present_mut().preview_part_array_mut(id, 0);
        self.front.paper_doll_lighting.update(now, doll);

        let rect = preview_viewport(area);
        self.cx.present_mut().preview_queue(id, who, rect);
    }

    /// **The identify window's 3D portrait.**
    ///
    /// Appraising a creature or an NPC shows the creature beside the numbers. The viewport element
    /// is registered as engine class `0x0D` (`factory.rs`), and [`crate::preview::PreviewSpace`]
    /// drives it as it drives the three other viewports; this is what binds `0x10000148` and
    /// queues a space for it. Without it the element is built from the layout and draws an
    /// empty box.
    ///
    /// # What the client does, in its own order
    ///
    /// Appraisal delivery picks the pane and initializes it for the object before updating its
    /// text. Initialization is virtual: the item-examine panel inherits the two-store base behavior, which is
    /// why an appraised **item** gets no portrait. The basic-creature examine panel overrides it with:
    ///
    /// ```text
    /// set a distant light with intensity 2.0 and direction (0.3, 1.9, 0.65)
    /// remove all preview objects
    /// discard any previous preview object
    /// p = look up the live physics object by object id
    /// if p exists:
    ///     clone p for the preview
    ///     set the clone's heading to 191.3679 degrees
    ///     compute its bounding box
    ///     set the camera from `portrait::camera_position`
    ///     add the clone to the preview
    /// end if
    /// ```
    ///
    /// # Where this deviates, and why it is the same behaviour
    ///
    /// * **Rebuild trigger.** Panel initialization runs once per `0x00C9` reply and never per frame; this runs
    ///   per frame and rebuilds when `Self::examine_3d_built` disagrees with the panel's current
    ///   object. Same builds, driven from the state rather than from the edge, which is what the
    ///   other three viewports here already do.
    /// * **Clone order.** Retail boxes the clone *before* adding it; the box is taken here after
    ///   the add, because [`crate::preview::PreviewSpace::add_object_dressed`] is what establishes
    ///   the part frames at all. Adding only sets cell membership and
    ///   placement frame 0, both of which the add already applied, so the box is of the
    ///   same posed object either way.
    /// * **Dressing.** Object creation clones a live physics object, which is already wearing its
    ///   `ObjDesc`; the clone here is rebuilt from the setup, so the descriptor is applied
    ///   explicitly. Without it an appraised player would be portrayed naked.
    ///
    /// **No animation is set**, deliberately: initialization sets no sequence animation, so the
    /// object keeps its setup-default animation, which
    /// `add_object_dressed` already applies. The portrait is therefore *live* — retail's
    /// update path advances every visible preview object each frame, and
    /// [`crate::preview::PreviewSpace::use_time`] provides that behavior here.
    pub(super) fn examine_3d_use_time(&mut self) {
        use dereth_ui_screens::panels::examination::portrait;
        use dereth_ui_screens::screens::gameplay_host::GameCall;

        let now = self.cx.now();
        let dt = preview_delta(now, &mut self.front.examine_3d_last_time);

        // Where it draws, whether it draws at all, and which object the panel is showing.
        //
        // The `ExamineSubUi::Item` arm is the virtual `Init` fork: the item pane's `Init` adds no
        // object, so an appraised item must leave the space untouched rather than portray itself.
        let where_ = self.front.ui.as_mut().and_then(|shell| {
            let screen = crate::hud_drive::game_screen(&mut shell.flow)?;
            let GameCall::ExaminePreview(p) =
                crate::hud_drive::game_call(&mut shell.ui, screen, GameCall::ExaminePreview(None))
            else {
                return None;
            };
            let (item_pane, current, viewport) = p?;
            if item_pane {
                return None;
            }
            let object = current?;
            let h = viewport?;
            if !element_is_drawn(&shell.ui, h) {
                return None;
            }
            let b = shell.ui.screen_clip_box(h);
            // The creature's attribute list is laid over the viewport, and its translucent rows
            // would shade the model: the model is drawn over the list's pictures and under its
            // text.
            let mut over: Vec<dereth_ui::ElemHandle> = shell
                .ui
                .parent(h)
                .and_then(|p| {
                    shell.ui.get_child_recursive(
                        p,
                        dereth_ui_screens::panels::examination::CREATURE_STAT_LIST,
                    )
                })
                .filter(|l| element_is_drawn(&shell.ui, *l))
                .into_iter()
                .collect();
            let mut at = 0;
            while at < over.len() {
                let more = shell.ui.children(over[at]);
                over.extend(more);
                at += 1;
            }
            b.is_valid().then_some((h, b, object, over))
        });
        let Some((who, area, object, over)) = where_ else {
            return;
        };

        // Look up the *live* object by id. A null there
        // is the client's `if (p)` and leaves the space with whatever it last held.
        let dress = self
            .cx
            .objects()
            .presence(object)
            .and_then(|p| Some((p.setup_id?, crate::world::to_anim_objdesc(&p.objdesc))));
        let Some((setup, objdesc)) = dress else {
            return;
        };

        let assets = std::sync::Arc::clone(self.cx.anim_assets());
        let store = std::sync::Arc::clone(self.cx.store());
        let id = crate::gpu::PreviewId::Examine;
        let fresh = self.cx.present_mut().preview_ensure(id, &assets);
        let rebuild = fresh || self.front.examine_3d_built != Some((object, setup));

        if rebuild {
            // Setting this light is the first initialization act and removes all lights, adds one,
            // and sets its direction underneath, so
            // re-issuing it on every rebuild is what the client does rather than an extra.
            self.cx.present_mut().preview_set_light(
                id,
                dereth_client_contract::overlay::PreviewLight::Directional,
                portrait::LIGHT_INTENSITY,
                dereth_primitives::Vec3::new(
                    portrait::LIGHT_DIRECTION.0,
                    portrait::LIGHT_DIRECTION.1,
                    portrait::LIGHT_DIRECTION.2,
                ),
            );
            self.cx.present_mut().preview_remove_all_objects(id);
            match self.cx.present_mut().preview_add_object_dressed(
                id,
                &store,
                setup,
                Some(&objdesc),
            ) {
                Ok(Some(_)) => {
                    self.cx
                        .present_mut()
                        .preview_set_heading(id, 0, portrait::HEADING_DEGREES);
                    self.front.examine_3d_built = Some((object, setup));
                }
                Ok(None) => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "the identify portrait's setup {setup:?} would not load");
                    self.front.examine_3d_built = None;
                }
                Err(e) => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "the identify portrait's space failed: {e}");
                    self.front.examine_3d_built = None;
                }
            }
        }

        // Read the bounding box, then set the camera, re-issued every tick: the element's box is the other
        // half of the arithmetic and a resized or re-laid-out panel must re-frame the creature.
        // Setting the camera is two stores and a rotate, so this is cheap and idempotent.
        let bb = self.cx.present().preview_object_bounding_box(id, 0, &store);
        if let Some(bb) = bb {
            let pos = portrait::camera_position(bb.min, bb.max, area.width(), area.height());
            self.cx.present_mut().preview_set_camera_position(id, pos);
            self.cx.present_mut().preview_set_camera_direction(
                id,
                dereth_primitives::Vec3::new(
                    portrait::CAMERA_DIRECTION.0,
                    portrait::CAMERA_DIRECTION.1,
                    portrait::CAMERA_DIRECTION.2,
                ),
            );
            // The portrait renderer's `update_position` loop makes the
            // portrait live rather than a still.
            self.cx.present_mut().preview_use_time(id, dt);
        }

        let rect = preview_viewport(area);
        if over.is_empty() {
            self.cx.present_mut().preview_queue(id, who, rect);
        } else {
            self.cx
                .present_mut()
                .preview_queue_under_text(id, over, rect);
        }
    }

    /// `PropertyInt 0xBC HeritageGroup` off the `0x0013` qualities, which is what
    /// the player-description receiver hands to the heritage-specific camera adjustment.
    ///
    /// Zero is the integer-property query's own answer for a property the description did not carry, and zero is
    /// also the heritage table's fall-through — so an absent heritage keeps the initial camera, which
    /// is exactly what the client does.
    pub(super) fn paper_doll_heritage(&self) -> u32 {
        let h = self.cx.hud().player_desc(self.cx.model()).map_or(0, |q| {
            q.inq_int(dereth_ui_screens::panels::inventory::HERITAGE_GROUP_PROPERTY)
        });
        u32::try_from(h).unwrap_or(0)
    }

    /// `0x10000005 PaperDollAnimation`, replaced by the two
    /// Olthoi heritages' own enums during heritage-specific preview adjustment.
    pub(super) fn paper_doll_animation_enum(&self) -> u32 {
        use dereth_ui_screens::panels::inventory::{paper_doll as pd, PAPER_DOLL_ANIMATION_ENUM};
        pd::for_race(self.paper_doll_heritage())
            .and_then(|(_, a)| a)
            .unwrap_or(PAPER_DOLL_ANIMATION_ENUM)
    }

    /// The 3D character preview's slot: keep the char-gen preview space in step with the wizard
    /// and say where it draws this frame.
    ///
    /// This is the host half of the preview update and camera setup. The
    /// *model* is [`dereth_ui_screens::screens::chargen::Cg3dView`], which the wizard fills in: the setup, the heading, the camera and the animation **enums**. Nothing
    /// here decides anything about the wizard; it resolves a dat id, moves a camera and points at
    /// a rectangle.
    ///
    /// Three pieces of the client's own bookkeeping are reproduced rather than simplified:
    ///
    /// * the object is rebuilt only when the setup id changes, which is the update's own test -- a
    ///   rebuild every frame would re-upload the body's textures sixty times a second;
    /// * the animation start and stop are re-issued on the
    ///   *animating* edge, at **30 fps** and **0 fps** respectively, with `clear = 1` both times;
    /// * the light and sharp mode are the space's, not the object's, and
    ///   the light direction is `(0.3, **1.9**, 0.65)` -- positive y, where the portal space's is
    ///   negative.
    pub(super) fn preview_use_time(&mut self) {
        let now = self.cx.now();
        // Elapsed seconds since the last tick, clamped exactly as the world's own delta is.
        // Nothing below accumulates a per-frame increment: takes `dt`.
        let dt = preview_delta(now, &mut self.front.preview_last_time);

        self.tick_creation_preview(dt);
        let Some(want) = self.creation_preview() else {
            return;
        };
        self.update_creation_space(&want.view3d, &want.state, &want.tables);
        self.draw_creation_preview(&want.view3d, want.who, want.area, dt);
    }

    /// Tick wizard rotation and zoom, falling back to the barber only when it did not answer.
    fn tick_creation_preview(&mut self, dt: f64) {
        use dereth_ui_screens::screens::pregame_host::PregameCall;
        // The appearance page's global-message 3 arm is the
        // per-frame tick: zoom animation when a zoom is animating, then rotation when rotating.
        // `CharGenScreen::tick_preview` is the rotation step: the two rotate arrows set the
        // rotating flag and direction, and this is what advances the heading. Driven off elapsed
        // seconds, like everything else in this step.
        if let Some(shell) = self.front.ui.as_mut() {
            let wizard_took = shell.flow.current_mut().is_some_and(|w| {
                crate::hud_drive::pregame_call(
                    &mut shell.ui,
                    &mut **w,
                    PregameCall::TickPreview(dt),
                )
                .0
            });
            if let (false, Some(w)) = (wizard_took, crate::hud_drive::game_screen(&mut shell.flow))
            {
                crate::hud_drive::game_call(
                    &mut shell.ui,
                    w,
                    dereth_ui_screens::screens::gameplay_host::GameCall::BarberTick(dt),
                );
            }
        }
    }

    /// Read the active creation view and its world-backed dressing inputs in one borrow.
    fn creation_preview(&mut self) -> Option<CreationPreview> {
        use dereth_ui_screens::screens::pregame_host::PregameCall;
        // What the wizard wants drawn, and where. `screen_clip_box` rather than `screen_box`: a
        // viewport scrolled under its parent must draw inside the parent's clipping rectangle.
        self.front.ui.as_mut().and_then(|shell| {
            let wizard = {
                let w = shell.flow.current_mut()?;
                match crate::hud_drive::pregame_call(
                    &mut shell.ui,
                    &mut **w,
                    PregameCall::WizardPreview(None),
                ) {
                    (true, PregameCall::WizardPreview(p)) => p,
                    _ => None,
                }
            };
            let (view3d, state, tables) = match wizard {
                Some(w) => w,
                None => {
                    use dereth_ui_screens::screens::gameplay_host::GameCall;
                    let screen = crate::hud_drive::game_screen(&mut shell.flow)?;
                    let GameCall::BarberPreview(barber) = crate::hud_drive::game_call(
                        &mut shell.ui,
                        screen,
                        GameCall::BarberPreview(None),
                    ) else {
                        return None;
                    };
                    barber?
                }
            };
            let screen = shell.flow.current()?;
            let id = view3d.viewport?;
            let root = *screen.roots().first()?;
            let h = shell.ui.get_child_recursive(root, id)?;
            let b = shell.ui.screen_clip_box(h);
            // Character-generation preview dressing needs the state and tables as well as the
            // view, because the `ObjDesc` that dresses the default parts is built from them. Both are cheap to carry -- `CharGenState` is plain data and the tables are an
            // `Rc` -- and taking them here keeps the whole read of the shell in one borrow.
            b.is_valid().then_some(CreationPreview {
                view3d,
                who: h,
                area: b,
                state,
                tables,
            })
        })
    }

    /// Rebuild dressing and room objects only on their own key edges, then update animation.
    fn update_creation_space(
        &mut self,
        view3d: &dereth_ui_screens::screens::chargen::Cg3dView,
        cg_state: &dereth_chargen::CharGenState,
        cg_tables: &Option<std::rc::Rc<dereth_ui_screens::screens::chargen::CharGenTables>>,
    ) {
        let assets = std::sync::Arc::clone(self.cx.anim_assets());
        let id = crate::gpu::PreviewId::CharGen;
        self.cx.present_mut().preview_ensure(id, &assets);
        // The space's own 45-degree lens, as the paper doll's: the classic interface's creation
        // model has a lens of its own in the same space.
        self.cx
            .present_mut()
            .preview_set_fov(id, dereth_world_render::creature_mode::DEFAULT_FOV_RADIANS);
        let store = std::sync::Arc::clone(self.cx.store());

        // Assemble the character-generation preview's appearance — everything that dresses the model.
        // Without it the turntable draws the naked setup record: the arrows move `CharGenState`'s
        // indices and the wizard rebuilds the view, but the model is never dressed. See
        // [`crate::preview::chargen_objdesc`].
        let (objdesc, dress_stats) = match cg_tables.as_ref() {
            Some(t) => {
                let s = std::sync::Arc::clone(self.cx.store());
                let cache = &mut self.front.chargen_pal_sets;
                crate::preview::chargen_objdesc(
                    &t.chargen,
                    cg_state,
                    &t.clothing,
                    view3d.setup,
                    &mut |id| cache.palettes(&s, id),
                )
            }
            None => (
                dereth_animation::parts::ObjDesc::default(),
                crate::preview::ChargenDressStats::default(),
            ),
        };
        self.front.chargen_dress = dress_stats;

        // The preview object's setup-changed test, the background object's own
        // environment-setup-changed test, and the animation edge, together.
        //
        // The `ObjDesc` is part of the key for the reason the paper doll's is: the part
        // meshes are baked from the **dressed** part array
        // ([`crate::preview::PreviewSpace::add_object_dressed`]), so a redress is a rebuild. The
        // client can be cheaper -- it reapplies the descriptor to the object it
        // already has -- and gets the same picture.
        //
        // The two animation enums are in the key because the block below reads them. See
        // [`ChargenPreviewKey`].
        let key = ChargenPreviewKey::from_view(view3d, objdesc.clone());
        if self.front.preview_chargen.as_ref() != Some(&key) {
            let rebuild =
                !matches!(self.front.preview_chargen.as_ref(), Some(k) if k.same_space(&key));
            if rebuild {
                self.cx.present_mut().preview_remove_all_objects(id);
                // Enable sharp rendering after rebuilding the preview; this is the preview update's
                // final operation and the reason the turntable is sharper than the world.
                self.cx.present_mut().preview_use_sharp_mode(id);
                self.cx.present_mut().preview_set_light(
                    id,
                    dereth_client_contract::overlay::PreviewLight::Directional,
                    2.0,
                    dereth_primitives::Vec3::new(0.3, 1.9, 0.65),
                );
                // The player object stays **index 0**, which is what `set_heading` and
                // `set_sequence_animation` below address; the client holds a pointer
                // to the player object and adds the background first, so its indices are the other
                // way round. Nothing observable turns on the order: the pass clears depth and
                // draws opaque before blended for the whole space, so the two objects resolve
                // against the Z-buffer either way.
                match self.cx.present_mut().preview_add_object_dressed(
                    id,
                    &store,
                    key.setup,
                    Some(&objdesc),
                ) {
                    Ok(Some(_)) => {}
                    Ok(None) => tracing::warn!(target: "dereth_client_shell::front_end",
                        "the char-gen preview setup {:?} would not load",
                        view3d.setup
                    ),
                    Err(e) => {
                        tracing::warn!(target: "dereth_client_shell::front_end", "the char-gen preview failed: {e}")
                    }
                }
                // The background object -- the room the model stands in, the heritage group's
                // environment setup. `INVALID_DID` means the heritage has none and the client
                // adds nothing.
                if view3d.bg_setup.0 != 0 {
                    match self
                        .cx
                        .present_mut()
                        .preview_add_object(id, &store, view3d.bg_setup)
                    {
                        Ok(Some(_)) => {}
                        Ok(None) => tracing::warn!(target: "dereth_client_shell::front_end",
                            "the char-gen background {:?} would not load",
                            view3d.bg_setup
                        ),
                        Err(e) => {
                            tracing::warn!(target: "dereth_client_shell::front_end", "the char-gen background failed: {e}")
                        }
                    }
                }
            }
            // Start / stop the animation.
            let (enum_value, framerate) = if view3d.animating {
                (view3d.animation_enum, 30.0)
            } else {
                (view3d.rest_animation_enum, 0.0)
            };
            match crate::assets::enum_did(&*store, crate::preview::UIASSET_GROUP, enum_value) {
                Some(a) => {
                    self.cx.present_mut().preview_clear_sequence_anims(id, 0);
                    if !self
                        .cx
                        .present_mut()
                        .preview_set_sequence_animation(id, 0, a, true, 0, framerate)
                    {
                        tracing::warn!(target: "dereth_client_shell::front_end", "the char-gen animation {a:?} is not in the dat");
                    }
                }
                None => {
                    tracing::warn!(target: "dereth_client_shell::front_end", "UIASSET enum {enum_value:#010X} does not resolve");
                }
            }
            self.front.preview_chargen = Some(key);
        }
    }

    /// Place and step the current creation space, then queue it in its viewport.
    fn draw_creation_preview(
        &mut self,
        view3d: &dereth_ui_screens::screens::chargen::Cg3dView,
        who: dereth_ui::ElemHandle,
        area: dereth_ui::region::Box2D,
        dt: f64,
    ) {
        let id = crate::gpu::PreviewId::CharGen;
        let p = view3d.camera_position;
        let d = view3d.camera_direction;
        self.cx
            .present_mut()
            .preview_set_camera_position(id, dereth_primitives::Vec3::new(p[0], p[1], p[2]));
        self.cx
            .present_mut()
            .preview_set_camera_direction(id, dereth_primitives::Vec3::new(d[0], d[1], d[2]));
        self.cx
            .present_mut()
            .preview_set_heading(id, 0, view3d.heading);
        self.cx.present_mut().preview_use_time(id, dt);
        let rect = preview_viewport(area);
        self.cx.present_mut().preview_queue(id, who, rect);
    }
}
