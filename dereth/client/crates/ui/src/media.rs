//! `MediaPlayback` — the per-element media script — and the shared UI easing table.
//!
//! This module models the element-description media machine and its effects.
//!
//! The machine is a tiny sequential interpreter. Each update **unregisters from global message 3
//! first**, then executes entries from the current index forward. Each entry reports *true = done,
//! advance* or *false = blocked*. On a block it re-registers
//! for message 3 and returns; when the list is exhausted it simply returns, already unregistered.
//! That is the "register for 3 only while animating" idiom.
//!
//! Media entries are what give AC its animated UI: the pulsing vitae indicator, the flickering
//! portal-storm lamp, the button click sound, and fading tooltips are all media-entry lists.

use dereth_primitives::num::rng::Ran2;
use dereth_primitives::DataId;

use crate::desc::{MediaDesc, MediaFields};
use crate::{MessageId, StateId};

// ---------------------------------------------------------------------------------------------
// Shared UI easing table
// ---------------------------------------------------------------------------------------------

/// The UI easing table and the lookup that indexes it.
///
/// Both live in [`dereth_client_contract::media`]: they are pure arithmetic over `dereth_primitives::num`,
/// and `dereth_client_contract::teleport` (the teleport animation model)
/// is the other caller. `dereth_ui::media::level_array` and `...::anim_level` resolve through this
/// `pub use`.
pub use dereth_client_contract::media::{anim_level, level_array};

// ---------------------------------------------------------------------------------------------
// The machine
// ---------------------------------------------------------------------------------------------

/// What one media step asks the element (or the manager) to do.
///
/// The machine does not touch the element directly, so it can be tested with no element at all and
/// so the sound and movie steps can be handed to the host's media player as requests.
/// Each variant describes the effect of its update.
#[derive(Debug, Clone, PartialEq)]
pub enum MediaEffect {
    /// Set or clear the region image for an image step and each anim-step frame.
    SetImage {
        file: Option<DataId>,
        draw_mode: u32,
    },
    /// Set or clear the region's alpha image for an alpha step.
    ///
    /// Type 2 does **not** set the region blend modifier;
    /// it sets the per-pixel **alpha image**, and the payload is a
    /// `0x06xxxxxx` RenderSurface DataID.
    SetAlphaImage { file: Option<DataId> },
    /// The cursor entry with a real file.
    SetCursor {
        file: DataId,
        hot_x: i32,
        hot_y: i32,
    },
    /// The cursor entry with an invalid file id.
    ///
    /// The media machine's cursor step has two arms; without this one, a track that ends by
    /// *clearing* the element's cursor leaves the previous one on screen.
    UnSetCursor,
    /// Play the sound identified by `did` at volume `1.0` when the sound type is
    /// the invalid sound type, else a sound-table lookup. The host's audio plays it.
    PlaySound { file: DataId, sound_type: u32 },
    /// Broadcast an element message from the owner.
    BroadcastMessage { id: MessageId },
    /// Set the owner's state.
    SetState { id: StateId },
    /// Set the owning UI object's alpha.
    SetObjectAlpha(f32),
    /// Start a movie decoded by the host; this crate owns only the media step.
    PlayMovie {
        file_name: String,
        stretch_to_full_screen: bool,
    },
}

/// One runtime copy of a `MediaDesc`, with the mutable timing state that the original machine keeps
/// on each cloned descriptor.
#[derive(Debug, Clone)]
struct Entry {
    desc: MediaDesc,
    /// The pause step's end time; unset until the pause starts.
    end_time: Option<f64>,
    /// The anim or fade step's start time.
    start_time: Option<f64>,
    /// The anim step's displayed frame number, `-1` when unset.
    displayed_frame: i32,
    /// Whether the movie request has already been raised.
    movie_started: bool,
}

/// The rebuild's per-element media playback state.
#[derive(Debug, Default, Clone)]
pub struct MediaPlayback {
    entries: Vec<Entry>,
    /// The index of the current media step.
    cur_index: usize,
    /// Whether the machine currently holds a global-message-3 registration.
    pub registered_for_tick: bool,
    /// Set by the host when the movie a [`MediaEffect::PlayMovie`] asked for has finished.
    pub movie_finished: bool,
    /// Scratch: where a jump step wants the next step to be.
    jump_to: Option<usize>,
}

impl MediaPlayback {
    /// Behavior: deep-copy the state's media list, set the current index to 0, then
    /// `Update`.
    pub fn reset(&mut self, media: &[MediaDesc]) {
        self.entries = media
            .iter()
            .map(|d| Entry {
                desc: d.clone(),
                end_time: None,
                start_time: None,
                displayed_frame: -1,
                movie_started: false,
            })
            .collect();
        self.cur_index = 0;
        self.movie_finished = false;
        self.registered_for_tick = false;
    }

    #[must_use]
    pub fn is_idle(&self) -> bool {
        self.cur_index >= self.entries.len()
    }

    #[must_use]
    pub fn cur_index(&self) -> usize {
        self.cur_index
    }

    /// The media machine's per-frame update.
    ///
    /// Returns the effects raised this call. [`Self::registered_for_tick`] afterwards is the
    /// machine's global-message-3 registration, which is the point of the idiom: a static
    /// element does zero work per frame.
    ///
    /// `owner_initialized` is the owner's initialised flag, which the movie step requires.
    pub fn update(
        &mut self,
        now: f64,
        rng: &mut Ran2,
        owner_initialized: bool,
    ) -> Vec<MediaEffect> {
        // The first thing the original does.
        self.registered_for_tick = false;
        let mut out = Vec::new();
        while self.cur_index < self.entries.len() {
            self.jump_to = None;
            let (done, mut fx) = self.step(self.cur_index, now, rng, owner_initialized);
            out.append(&mut fx);
            if !done {
                self.registered_for_tick = true;
                return out;
            }
            // The original jump step sets the current index to the jump target minus one and the loop's `++`
            // lands on the target; expressed here as an explicit target so index 0 needs no
            // unsigned wrap.
            self.cur_index = self.jump_to.take().unwrap_or(self.cur_index + 1);
        }
        out
    }

    /// The machine's global-message listener — message 3 runs `Update`.
    pub fn listen_to_global_message(
        &mut self,
        id: MessageId,
        now: f64,
        rng: &mut Ran2,
        owner_initialized: bool,
    ) -> Vec<MediaEffect> {
        if id.0 == crate::msg::global::TICK.0 {
            self.update(now, rng, owner_initialized)
        } else {
            Vec::new()
        }
    }

    fn step(
        &mut self,
        i: usize,
        now: f64,
        rng: &mut Ran2,
        owner_initialized: bool,
    ) -> (bool, Vec<MediaEffect>) {
        let mut fx = Vec::new();
        // A `Jump` may rewrite `cur_index`, so read the fields we need before mutating.
        let fields = self.entries[i].desc.fields.clone();
        match fields {
            // The movie step refuses until the element is initialised. The return value
            // reports whether the movie has finished so the intro screen can wait.
            MediaFields::Movie {
                file_name,
                stretch_to_full_screen,
            } => {
                if !owner_initialized {
                    return (false, fx);
                }
                if !self.entries[i].movie_started {
                    self.entries[i].movie_started = true;
                    fx.push(MediaEffect::PlayMovie {
                        file_name,
                        stretch_to_full_screen: stretch_to_full_screen != 0,
                    });
                }
                (self.movie_finished, fx)
            }
            // Behavior: the per-pixel alpha *image*, not the blend modifier.
            MediaFields::Alpha { file } => {
                fx.push(MediaEffect::SetAlphaImage {
                    file: (file.0 != 0).then_some(file),
                });
                (true, fx)
            }
            // The anim step.
            MediaFields::Anim {
                duration,
                draw_mode,
                frames,
            } => {
                let start = *self.entries[i].start_time.get_or_insert(now);
                if self.entries[i].start_time == Some(now) {
                    self.entries[i].displayed_frame = -1;
                }
                let t = if duration.abs() >= 0.0002 {
                    ((now - start) / f64::from(duration)).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                if !frames.is_empty() {
                    let last = i32::try_from(frames.len() - 1).unwrap_or(0);
                    let frame = dereth_primitives::num::to_i32_f64(t * f64::from(last));
                    if frame != self.entries[i].displayed_frame {
                        let idx = usize::try_from(frame.clamp(0, last)).unwrap_or(0);
                        fx.push(MediaEffect::SetImage {
                            file: Some(frames[idx]),
                            draw_mode,
                        });
                        self.entries[i].displayed_frame = frame;
                    }
                }
                let done = t >= 1.0;
                if done {
                    self.entries[i].start_time = None;
                    self.entries[i].displayed_frame = -1;
                }
                (done, fx)
            }
            // The cursor step, both arms:
            //
            // A valid file selects the owner's cursor and hotspot; an invalid file clears the
            // owner's cursor. Both paths complete the media entry.
            MediaFields::Cursor {
                file,
                x_hotspot,
                y_hotspot,
            } => {
                if file.0 == 0 {
                    fx.push(MediaEffect::UnSetCursor);
                } else {
                    fx.push(MediaEffect::SetCursor {
                        file,
                        hot_x: x_hotspot,
                        hot_y: y_hotspot,
                    });
                }
                (true, fx)
            }
            // The image step.
            MediaFields::Image { file, draw_mode } => {
                fx.push(MediaEffect::SetImage {
                    file: (file.0 != 0).then_some(file),
                    draw_mode,
                });
                (true, fx)
            }
            // The jump step: roll, and on success set the current index to `target - 1` so the
            // caller's `++` lands on `target`. Always advances.
            MediaFields::Jump {
                jump_item_index,
                probability,
            } => {
                let roll = rng.roll_f32(0.0, 1.0);
                if roll < probability {
                    self.jump_to = Some(jump_item_index as usize);
                }
                (true, fx)
            }
            // The message step.
            MediaFields::Message {
                message_id,
                probability,
            } => {
                let roll = rng.roll_f32(0.0, 1.0);
                if roll < probability {
                    fx.push(MediaEffect::BroadcastMessage {
                        id: MessageId(message_id),
                    });
                }
                (true, fx)
            }
            // The pause step rolls its end time once with the Numerical Recipes generator, which
            // is `dereth_primitives::num::rng::Ran2` and must not be merged with the CRT one.
            MediaFields::Pause {
                min_duration,
                max_duration,
            } => {
                let end = *self.entries[i].end_time.get_or_insert_with(|| {
                    f64::from(rng.roll_f32(min_duration, max_duration)) + now
                });
                let done = end <= now;
                if done {
                    self.entries[i].end_time = None;
                }
                (done, fx)
            }
            // The sound step.
            MediaFields::Sound { file, sound_type } => {
                if file.0 != 0 {
                    fx.push(MediaEffect::PlaySound { file, sound_type });
                }
                (true, fx)
            }
            // The state step. **It returns false unconditionally**, so a state entry
            // never advances the machine. The state change normally resets the media machine with the new state's
            // list, which is what actually moves the machine on. Preserved deliberately: a rebuild
            // that "fixes" this to `return true` runs the entries after a state change that the
            // client never reaches.
            MediaFields::State {
                state_id,
                probability,
            } => {
                let roll = rng.roll_f32(0.0, 1.0);
                if roll < probability {
                    fx.push(MediaEffect::SetState {
                        id: StateId(state_id),
                    });
                }
                (false, fx)
            }
            // The fade step: a zero duration jumps straight to t = 1.
            MediaFields::Fade {
                start_alpha,
                end_alpha,
                duration,
            } => {
                let start = *self.entries[i].start_time.get_or_insert(now);
                let t = if duration.abs() >= 0.0002 {
                    ((now - start) / f64::from(duration)).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                #[allow(clippy::cast_possible_truncation)] // the original interpolates in f32
                let a = start_alpha + (end_alpha - start_alpha) * (t as f32);
                fx.push(MediaEffect::SetObjectAlpha(a));
                let done = t >= 1.0;
                if done {
                    self.entries[i].start_time = None;
                }
                (done, fx)
            }
            // The loader has no case for an unknown type, and the update switch's default advances
            // the current index: an unrecognised entry advances rather than blocking.
            _ => (true, fx),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn md(fields: MediaFields) -> MediaDesc {
        let t = match &fields {
            MediaFields::Movie { .. } => 1,
            MediaFields::Alpha { .. } => 2,
            MediaFields::Anim { .. } => 3,
            MediaFields::Cursor { .. } => 4,
            MediaFields::Image { .. } => 5,
            MediaFields::Jump { .. } => 6,
            MediaFields::Message { .. } => 7,
            MediaFields::Pause { .. } => 8,
            MediaFields::Sound { .. } => 9,
            MediaFields::State { .. } => 10,
            MediaFields::Fade { .. } => 11,
            _ => 0,
        };
        MediaDesc {
            media_type: t,
            type_echo_ok: true,
            fields,
        }
    }

    /// Oracle: the retail easing table (see [`level_array`]) — a running
    /// sum of `trunc(sin(i·3.141592/99) · 1024)`, rescaled so the last entry is 1024.
    #[test]
    fn the_easing_table_is_a_monotone_curve_ending_at_1024() {
        let t = level_array();
        assert_eq!(t[0], 0);
        assert_eq!(t[99], 1024);
        assert!(t.windows(2).all(|w| w[1] >= w[0]), "must be non-decreasing");
        // Spot values from evaluating the formula.
        assert_eq!(t[1], 0);
        assert_eq!(t[2], 1);
        assert_eq!(t[49], 512, "the curve is symmetric about the midpoint");
        assert_eq!(t[50], 528);
        assert_eq!(t[98], 1024);
        // Ease-in/ease-out: the middle moves fastest.
        assert!(t[50] - t[49] > t[2] - t[1]);
        assert!(t[50] - t[49] > t[99] - t[98]);
    }

    /// The behavior clamps the value to [0,1], then
    /// looks up the easing table at `trunc(f·99)`.
    #[test]
    fn get_anim_level_clamps_and_truncates() {
        let t = level_array();
        assert_eq!(anim_level(&t, 0.0), 0);
        assert_eq!(anim_level(&t, 1.0), 1024);
        assert_eq!(anim_level(&t, -5.0), 0);
        assert_eq!(anim_level(&t, 5.0), 1024);
        assert_eq!(
            anim_level(&t, 0.5),
            t[49],
            "0.5 * 99 = 49.5 truncates to 49"
        );
    }

    /// Pinned behavior: it unregisters from message 3 first and only
    /// re-registers when an entry blocks. A static element does zero work per frame.
    #[test]
    fn the_machine_only_holds_the_tick_registration_while_blocked() {
        let mut rng = Ran2::new(1);
        let mut m = MediaPlayback::default();
        m.reset(&[md(MediaFields::Image {
            file: DataId(0x0600_0001),
            draw_mode: 1,
        })]);
        let fx = m.update(0.0, &mut rng, true);
        assert_eq!(
            fx,
            vec![MediaEffect::SetImage {
                file: Some(DataId(0x0600_0001)),
                draw_mode: 1
            }]
        );
        assert!(m.is_idle());
        assert!(!m.registered_for_tick, "a finished list goes idle");

        m.reset(&[
            md(MediaFields::Pause {
                min_duration: 2.0,
                max_duration: 2.0,
            }),
            md(MediaFields::Image {
                file: DataId(7),
                draw_mode: 0,
            }),
        ]);
        let fx = m.update(0.0, &mut rng, true);
        assert!(fx.is_empty());
        assert!(m.registered_for_tick, "blocked on the pause");
        assert_eq!(m.cur_index(), 0);
        let fx = m.update(1.0, &mut rng, true);
        assert!(fx.is_empty(), "still inside the pause");
        let fx = m.update(2.0, &mut rng, true);
        assert_eq!(
            fx.len(),
            1,
            "the pause completes and the image runs in the same call"
        );
        assert!(!m.registered_for_tick);
    }

    /// Pinned behavior: a jump step back to index 0 is what
    /// makes an animation loop. Without the jump the machine goes idle at the end.
    #[test]
    fn a_jump_to_zero_loops_the_list() {
        let mut rng = Ran2::new(3);
        let mut m = MediaPlayback::default();
        m.reset(&[
            md(MediaFields::Image {
                file: DataId(1),
                draw_mode: 0,
            }),
            md(MediaFields::Pause {
                min_duration: 1.0,
                max_duration: 1.0,
            }),
            md(MediaFields::Jump {
                jump_item_index: 0,
                probability: 1.0,
            }),
        ]);
        let fx = m.update(0.0, &mut rng, true);
        assert_eq!(fx.len(), 1);
        assert!(m.registered_for_tick);
        let fx = m.update(1.0, &mut rng, true);
        // pause completes, jump fires, index 0 runs again, pause blocks again
        assert_eq!(fx.len(), 1, "the image is set a second time: {fx:?}");
        assert!(m.registered_for_tick);
        assert!(!m.is_idle());
    }

    /// Pinned behavior: the `return false` is outside the `if`, so
    /// the entry never advances.
    #[test]
    fn a_state_entry_blocks_forever() {
        let mut rng = Ran2::new(5);
        let mut m = MediaPlayback::default();
        m.reset(&[
            md(MediaFields::State {
                state_id: 4,
                probability: 1.0,
            }),
            md(MediaFields::Image {
                file: DataId(1),
                draw_mode: 0,
            }),
        ]);
        let fx = m.update(0.0, &mut rng, true);
        assert_eq!(fx, vec![MediaEffect::SetState { id: StateId(4) }]);
        assert!(m.registered_for_tick);
        assert_eq!(m.cur_index(), 0, "still on the state entry");
        // and again next frame — the image after it is unreachable unless SetState resets the list
        let fx = m.update(1.0, &mut rng, true);
        assert_eq!(fx, vec![MediaEffect::SetState { id: StateId(4) }]);
        assert_eq!(m.cur_index(), 0);
    }

    /// Pinned behavior: `frame = trunc(t · (n-1))`, and the image is
    /// re-set only when the frame index changes, and the entry completes at `t == 1`.
    #[test]
    fn an_anim_walks_its_frames_and_completes_at_one() {
        let mut rng = Ran2::new(7);
        let mut m = MediaPlayback::default();
        let frames = vec![DataId(10), DataId(11), DataId(12), DataId(13), DataId(14)];
        m.reset(&[md(MediaFields::Anim {
            duration: 4.0,
            draw_mode: 2,
            frames,
        })]);
        let fx = m.update(0.0, &mut rng, true);
        assert_eq!(
            fx,
            vec![MediaEffect::SetImage {
                file: Some(DataId(10)),
                draw_mode: 2
            }]
        );
        assert!(m.registered_for_tick);
        assert!(
            m.update(0.5, &mut rng, true).is_empty(),
            "same frame, no re-set"
        );
        let fx = m.update(1.0, &mut rng, true);
        assert_eq!(
            fx,
            vec![MediaEffect::SetImage {
                file: Some(DataId(11)),
                draw_mode: 2
            }]
        );
        let fx = m.update(4.0, &mut rng, true);
        assert_eq!(
            fx,
            vec![MediaEffect::SetImage {
                file: Some(DataId(14)),
                draw_mode: 2
            }]
        );
        assert!(m.is_idle());
        assert!(!m.registered_for_tick);
    }

    /// Oracle: including the `|duration| < 0.0002` shortcut
    /// that jumps straight to the end alpha.
    #[test]
    fn a_fade_interpolates_and_a_zero_duration_snaps() {
        let mut rng = Ran2::new(11);
        let mut m = MediaPlayback::default();
        m.reset(&[md(MediaFields::Fade {
            start_alpha: 0.0,
            end_alpha: 1.0,
            duration: 2.0,
        })]);
        assert_eq!(
            m.update(0.0, &mut rng, true),
            vec![MediaEffect::SetObjectAlpha(0.0)]
        );
        assert_eq!(
            m.update(1.0, &mut rng, true),
            vec![MediaEffect::SetObjectAlpha(0.5)]
        );
        assert_eq!(
            m.update(2.0, &mut rng, true),
            vec![MediaEffect::SetObjectAlpha(1.0)]
        );
        assert!(m.is_idle());

        m.reset(&[md(MediaFields::Fade {
            start_alpha: 0.25,
            end_alpha: 0.75,
            duration: 0.0,
        })]);
        assert_eq!(
            m.update(0.0, &mut rng, true),
            vec![MediaEffect::SetObjectAlpha(0.75)]
        );
        assert!(m.is_idle());
    }

    /// Oracle: refuses while the owner's initialised flag is clear.
    #[test]
    fn a_movie_waits_for_the_element_to_be_initialised() {
        let mut rng = Ran2::new(13);
        let mut m = MediaPlayback::default();
        m.reset(&[md(MediaFields::Movie {
            file_name: "intro.bik".into(),
            stretch_to_full_screen: 1,
        })]);
        assert!(m.update(0.0, &mut rng, false).is_empty());
        assert!(m.registered_for_tick);
        let fx = m.update(0.0, &mut rng, true);
        assert_eq!(
            fx,
            vec![MediaEffect::PlayMovie {
                file_name: "intro.bik".into(),
                stretch_to_full_screen: true
            }]
        );
        assert!(m.registered_for_tick, "blocked until the movie finishes");
        m.movie_finished = true;
        assert!(m.update(1.0, &mut rng, true).is_empty());
        assert!(m.is_idle());
    }

    /// Oracle: retail sets the alpha image by data id, or clears it when there is none. This is the
    /// documentation correction recorded on [`MediaEffect::SetAlphaImage`].
    #[test]
    fn md_data_alpha_sets_the_alpha_image_not_the_blend_modifier() {
        let mut rng = Ran2::new(17);
        let mut m = MediaPlayback::default();
        m.reset(&[md(MediaFields::Alpha {
            file: DataId(0x0600_00AA),
        })]);
        assert_eq!(
            m.update(0.0, &mut rng, true),
            vec![MediaEffect::SetAlphaImage {
                file: Some(DataId(0x0600_00AA))
            }]
        );
        m.reset(&[md(MediaFields::Alpha { file: DataId(0) })]);
        assert_eq!(
            m.update(0.0, &mut rng, true),
            vec![MediaEffect::SetAlphaImage { file: None }]
        );
    }
}
