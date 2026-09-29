//! `DataPatchScreen` — mode `0x10000003`, the first screen the client shows.

use dereth_primitives::LocalTime;
use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{LayoutEnum, Screen};
use dereth_ui::{ElemHandle, ElementId, ElementMessage, MessageId, UiError, UiMode, UiSystem};

use crate::bind::{attr, bind_children, child, set_attr_float, Bound, ChildBinding};

/// The screen's root: layout enum `0x10000001`, element `0x1000041A`.
const LAYOUT: LayoutEnum = LayoutEnum(0x1000_0001);
const ROOT: ElementId = ElementId(0x1000_041A);

/// The Quit button; element message 1 → queue mode `0x10000009`.
pub const QUIT_BUTTON: ElementId = ElementId(0x1000_041C);

/// The screen's cached children, by element id.
pub const CHILDREN: &[ChildBinding] = &[
    child("connect_meter", 0x1000_041E),
    child("patch_meter", 0x1000_041F),
    child("connect_text", 0x1000_0420),
    child("patch_text", 0x1000_0421),
    child("quit_button", 0x1000_041C),
];

/// The disk-space re-check interval: the next check is the current time + 60.
pub const DISKSPACE_CHECK_SECONDS: f64 = 60.0;

/// A DDD event → the string id the data-patch screen shows for it. It travels in the pre-game
/// view, so it lives in `dereth_client_contract::pregame`; this is its historical path.
pub use dereth_client_contract::pregame::DddEvent;

/// The connect-status text's two code states, from the shipped `patch` layout (`0x21000000`, element
/// `0x10000420`): `0x1000003B` is "Connecting..." and `0x1000003C` is "Connected!".
///
/// The connect-level routine ends in an unresolved indirect call on the connect-status text,
/// consistent with selecting one of its states. The *data* is not ambiguous — the element carries
/// exactly these three states and no other code touches it — but which level selects which
/// remains inferred from the strings. The third state, `1`, is "Connect progress", the
/// design-time label.
pub const CONNECT_TEXT_CONNECTING: dereth_ui::StateId = dereth_ui::StateId(0x1000_003B);
/// See [`CONNECT_TEXT_CONNECTING`].
pub const CONNECT_TEXT_CONNECTED: dereth_ui::StateId = dereth_ui::StateId(0x1000_003C);

/// The patch panel's string-info message, resolved from table `table` and substituted.
///
/// The string ids in a `StringTable` are the client's string hash of the symbolic name, which is how
/// `ID_DataPatch_PatchingDone` becomes
/// the key `31989477`. A row holds a **list of variants**, and the ones with substitutions are
/// stored as the literal pieces *around* the variables — `ID_DataPatch_PatchProgress` is
/// `["", "% of ", "K complete..."]`.
///
/// The pieces go through the string renderer's meta-language arm rather than being interleaved
/// here. For this row the two agree character for character; what the renderer adds is its
/// excess-space trimming at the end, so a substituted value that arrives with a doubled space does
/// not draw one.
///
/// The two values go in **by name**
/// ([`dereth_ui::UiSystem::resolve_string_named`]). The rows name them `percent` and `total`,
/// lower case [measured, `0x23000002`]: `ID_DataPatch_PatchProgress` is `percent, total` and
/// `ID_DataPatch_Waiting` is `total` alone. Naming them is what makes a status that carries
/// only `token_total` land in `total` rather than in whichever slot came first; a positional
/// list would print `ID_DataPatch_Waiting`'s byte count as a percentage.
#[must_use]
pub fn patch_status_text(
    ui: &UiSystem,
    table: Option<dereth_primitives::DataId>,
    st: &PatchStatus,
) -> String {
    let Some(table) = table else {
        return String::new();
    };
    let hash = dereth_primitives::num::hash::str_hash(st.string_id.as_bytes());
    let mut values: Vec<(&str, String)> = Vec::new();
    if let Some(p) = st.token_percent {
        values.push(("percent", format!("{}", dereth_primitives::num::to_i32(p))));
    }
    if let Some(t) = st.token_total {
        values.push(("total", format!("{}", t / 1024)));
    }
    let named: Vec<(&str, &str)> = values.iter().map(|(n, v)| (*n, v.as_str())).collect();
    ui.resolve_string_named(table, hash, &named)
        .unwrap_or_default()
}

/// What [`DataPatchScreen::on_ddd_event`] decided to show.
#[derive(Debug, Clone, PartialEq)]
pub struct PatchStatus {
    /// The string id, from table enum `0x10000002`.
    pub string_id: &'static str,
    /// `token_total`, where the event carries one.
    pub token_total: Option<u64>,
    /// `token_percent`, for `DDD_DataDownloaded`.
    pub token_percent: Option<f32>,
}

/// `DataPatchScreen` — mode `0x10000003`.
#[derive(Debug, Default)]
pub struct DataPatchScreen {
    roots: Vec<ElemHandle>,
    bound: Bound,
    /// The connect level. "Both start at `-1.0`; the constructor forces both to `0.0`."
    pub connect_level: f32,
    /// The patch level.
    pub patch_level: f32,
    /// The expected byte count from `DDD_PatchtimeBegin`.
    pub expected: u64,
    /// The running total from `DDD_DataDownloaded`.
    pub received: u64,
    /// The framework's persistent "received set" flag, mirrored so the per-frame step can test it.
    pub received_set: bool,
    /// Whether a packet controller exists; [`Self::use_time`] proceeds when there is none.
    pub has_packet_controller: bool,
    /// The network layer reports a live connection.
    pub connected: bool,
    /// The next drive-free-space query time.
    pub next_diskspace_check: f64,
}

impl DataPatchScreen {
    /// The factory registered for this screen's mode.
    #[must_use]
    pub fn create_screen() -> Box<dyn Screen> {
        Box::new(Self {
            connect_level: -1.0,
            patch_level: -1.0,
            ..Self::default()
        })
    }

    /// Write the connect-meter level and refresh the text beside it.
    ///
    /// The data-patch meters use attribute **`0x66`**, not the `0x69` every other meter uses.
    pub fn set_connect_level(&mut self, ui: &mut UiSystem, level: f32) {
        self.connect_level = level;
        if let Some(h) = self.bound.get("connect_meter") {
            set_attr_float(ui, h, attr::PATCH_METER_LEVEL, level);
        }
    }

    /// Write the patch-meter level and refresh the text beside it.
    pub fn set_patch_level(&mut self, ui: &mut UiSystem, level: f32) {
        self.patch_level = level;
        if let Some(h) = self.bound.get("patch_meter") {
            set_attr_float(ui, h, attr::PATCH_METER_LEVEL, level);
        }
    }

    /// Map one DDD event to the status line, the meters and the "may leave" answer.
    pub fn on_ddd_event(&mut self, ui: &mut UiSystem, e: DddEvent) -> PatchStatus {
        match e {
            DddEvent::PatchtimeInterrogation => PatchStatus {
                string_id: "ID_DataPatch_Interrogation",
                token_total: None,
                token_percent: None,
            },
            DddEvent::PatchtimePending { total } => PatchStatus {
                string_id: "ID_DataPatch_Waiting",
                token_total: Some(total),
                token_percent: None,
            },
            DddEvent::PatchtimeBegin { expected } => {
                self.expected = expected;
                self.received = 0;
                PatchStatus {
                    string_id: "ID_DataPatch_Patching",
                    token_total: Some(expected),
                    token_percent: None,
                }
            }
            DddEvent::DataDownloaded { bytes } => {
                self.received += bytes;
                let level = if self.expected == 0 {
                    0.0
                } else {
                    #[allow(clippy::cast_precision_loss)]
                    let l = self.received as f32 / self.expected as f32;
                    l.clamp(0.0, 1.0)
                };
                self.set_patch_level(ui, level);
                PatchStatus {
                    string_id: "ID_DataPatch_PatchProgress",
                    token_total: Some(self.expected),
                    token_percent: Some(level * 100.0),
                }
            }
            DddEvent::PatchtimeEnd => {
                self.set_patch_level(ui, 1.0);
                PatchStatus {
                    string_id: "ID_DataPatch_PatchingDone",
                    token_total: None,
                    token_percent: None,
                }
            }
        }
    }

    /// The data-patch screen's per-frame step, driven by global message 3.
    ///
    /// "sets the connect level to 1.0 once the network layer reports a live connection; every 60 s
    /// it re-queries the drive's free space while the expected byte count is non-zero; when
    /// **both** levels are ≥ 1.0 and either there is no packet controller or the persistent
    /// "received set" is true,
    /// it queues mode `0x10000001` (intro)."
    pub fn use_time(&mut self, ui: &mut UiSystem, now: f64) -> Option<UiMode> {
        if self.connected && self.connect_level < 1.0 {
            self.set_connect_level(ui, 1.0);
        }
        if self.expected != 0 && now >= self.next_diskspace_check {
            self.next_diskspace_check = now + DISKSPACE_CHECK_SECONDS;
        }
        if self.connect_level >= 1.0
            && self.patch_level >= 1.0
            && (!self.has_packet_controller || self.received_set)
        {
            return Some(dereth_ui::framework::mode::INTRO);
        }
        None
    }

    /// The bound children, for tests and for the acceptance gate.
    #[must_use]
    pub fn bound(&self) -> &Bound {
        &self.bound
    }
}

impl Screen for DataPatchScreen {
    fn create(&mut self, cx: &mut ScreenCx<'_>) -> Result<(), UiError> {
        let ui = &mut *cx.ui;
        let root = ui
            .require_env()
            .and_then(|e| e.create_and_add_root_element(ui, LAYOUT, ROOT))?;
        self.roots.push(root);
        self.bound = bind_children(ui, root, CHILDREN);
        // The constructor "forces both to 0.0" after initialising them to -1.0.
        self.set_connect_level(ui, 0.0);
        self.set_patch_level(ui, 0.0);
        // Subscribes to global message 3 so `use_time` runs every frame.
        ui.register_for_global_message(
            dereth_ui::msg::global::TICK,
            dereth_ui::ListenerId::External(LAYOUT.0),
        );
        Ok(())
    }

    fn update(&mut self, cx: &mut ScreenCx<'_>, now: LocalTime) -> Option<UiMode> {
        let ui = &mut *cx.ui;
        self.use_time(ui, now.0)
    }

    fn on_pregame(
        &mut self,
        cx: &mut ScreenCx<'_>,
        p: &dereth_ui::framework::PregameCx<'_>,
    ) -> Option<UiMode> {
        let host = p.view;
        // The three globals reads.
        self.connected = host.connected;
        self.has_packet_controller = host.has_packet_controller;
        self.received_set = p.received_set;

        // The cache event reaches the data-patch plugin and then the screen's handler.
        // The status line it decides is a string in table enum `0x10000002`.
        let mut status = None;
        for e in &host.ddd {
            let st = self.on_ddd_event(cx.ui, *e);
            status = Some(st);
        }
        // `DDD_PatchtimeEnd` → "unregister the plugin, force the patch level to 1.0". Against ACE
        // `DDD.EnableDATPatching` is normally off, so the whole phase is interrogation →
        // `DDD_EndDDD` and this is the event that ends it; `patch_finished` is the same
        // statement for a run with no network at all.
        if host.patch_finished && self.patch_level < 1.0 {
            status = Some(self.on_ddd_event(cx.ui, DddEvent::PatchtimeEnd));
        }
        // The data-patch screen's first update step, read here rather than a frame later:
        // setting the connect level
        // refreshes the connect-status text at the moment the level changes, and the level changes as
        // soon as the network state reports a connection.
        let connected = self.connected || self.connect_level >= 1.0;
        let patch_text = self.bound().get("patch_text");
        let connect_text = self.bound().get("connect_text");
        if let (Some(st), Some(h)) = (status, patch_text) {
            let text = patch_status_text(cx.ui, p.ui_strings, &st);
            if let Some(t) = cx.ui.text_element_mut(h) {
                t.set_text(&text);
            }
        }
        // Setting the connect level's second act: refresh the connect-status text. See
        // [`CONNECT_TEXT_CONNECTING`] for what is verified here and what is inferred.
        if let Some(h) = connect_text {
            let want = if connected {
                CONNECT_TEXT_CONNECTED
            } else {
                CONNECT_TEXT_CONNECTING
            };
            if cx.ui.node(h).map(|n| n.state) != Some(want) {
                cx.ui.set_state(h, want);
            }
        }
        None
    }

    fn on_element_message(&mut self, cx: &mut ScreenCx<'_>, m: &ElementMessage) {
        let requests_out = &mut cx.ui.requests;
        // "(Quit button) `0x1000041C`, element message 1 → queue mode `0x10000009`".
        if m.source_id == QUIT_BUTTON && m.id == MessageId(1) {
            requests_out.emit(crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::EPILOGUE,
            ));
        }
    }

    fn roots(&self) -> &[ElemHandle] {
        &self.roots
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the recovered screen catalogue's child table and the shipped layout-enum map
    /// (enum `0x10000001` → `0x21000000` `patch`).
    #[test]
    fn the_screen_names_the_documented_layout_root_and_children() {
        assert_eq!(LAYOUT, LayoutEnum(0x1000_0001));
        assert_eq!(ROOT, ElementId(0x1000_041A));
        let ids: Vec<u32> = CHILDREN.iter().map(|c| c.id.0).collect();
        assert_eq!(
            ids,
            vec![
                0x1000_041E,
                0x1000_041F,
                0x1000_0420,
                0x1000_0421,
                0x1000_041C
            ]
        );
        assert_eq!(QUIT_BUTTON, ElementId(0x1000_041C));
    }

    /// Oracle: §3's DDD-event paragraph — the five events, their string ids, the accumulation
    /// and the clamp.
    #[test]
    fn the_ddd_events_map_to_the_documented_strings_and_drive_the_patch_meter() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = DataPatchScreen {
            connect_level: -1.0,
            patch_level: -1.0,
            ..Default::default()
        };

        assert_eq!(
            s.on_ddd_event(&mut ui, DddEvent::PatchtimeInterrogation)
                .string_id,
            "ID_DataPatch_Interrogation"
        );
        let st = s.on_ddd_event(&mut ui, DddEvent::PatchtimePending { total: 12 });
        assert_eq!(
            (st.string_id, st.token_total),
            ("ID_DataPatch_Waiting", Some(12))
        );

        s.on_ddd_event(&mut ui, DddEvent::PatchtimeBegin { expected: 1000 });
        assert_eq!(s.expected, 1000);

        let st = s.on_ddd_event(&mut ui, DddEvent::DataDownloaded { bytes: 250 });
        assert_eq!(st.string_id, "ID_DataPatch_PatchProgress");
        assert_eq!(s.patch_level, 0.25);
        assert_eq!(st.token_percent, Some(25.0));
        // The accumulation is a running total, and the level clamps at 1.0.
        s.on_ddd_event(&mut ui, DddEvent::DataDownloaded { bytes: 5000 });
        assert_eq!(s.received, 5250);
        assert_eq!(s.patch_level, 1.0, "clamp(received/expected, 0, 1)");

        assert_eq!(
            s.on_ddd_event(&mut ui, DddEvent::PatchtimeEnd).string_id,
            "ID_DataPatch_PatchingDone"
        );
        assert_eq!(s.patch_level, 1.0);
    }

    /// Oracle: §3's per-frame-step paragraph — the three-part condition that queues the intro.
    #[test]
    fn use_time_queues_the_intro_only_when_both_meters_are_full_and_the_set_arrived() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = DataPatchScreen {
            has_packet_controller: true,
            ..Default::default()
        };
        s.connect_level = 0.0;
        s.patch_level = 0.0;
        assert_eq!(s.use_time(&mut ui, 0.0), None);

        // A live connection forces the connect meter to 1.0.
        s.connected = true;
        assert_eq!(s.use_time(&mut ui, 0.0), None);
        assert_eq!(s.connect_level, 1.0);

        s.patch_level = 1.0;
        assert_eq!(
            s.use_time(&mut ui, 0.0),
            None,
            "still waiting for the received set"
        );
        s.received_set = true;
        assert_eq!(
            s.use_time(&mut ui, 0.0),
            Some(dereth_ui::framework::mode::INTRO)
        );

        // With no packet controller at all the set is not required.
        let mut s2 = DataPatchScreen {
            has_packet_controller: false,
            ..Default::default()
        };
        s2.connect_level = 1.0;
        s2.patch_level = 1.0;
        assert_eq!(
            s2.use_time(&mut ui, 0.0),
            Some(dereth_ui::framework::mode::INTRO)
        );
    }

    /// Oracle: §3's per-frame step — "every 60 s it re-queries the drive's free space **while the
    /// expected byte count is non-zero**".
    #[test]
    fn the_disk_space_check_is_every_sixty_seconds_and_only_while_patching() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = DataPatchScreen::default();
        s.use_time(&mut ui, 100.0);
        assert_eq!(
            s.next_diskspace_check, 0.0,
            "no expected byte count, no check"
        );
        s.expected = 1;
        s.use_time(&mut ui, 100.0);
        assert_eq!(s.next_diskspace_check, 160.0);
        s.use_time(&mut ui, 130.0);
        assert_eq!(s.next_diskspace_check, 160.0, "not yet due");
        assert_eq!(DISKSPACE_CHECK_SECONDS, 60.0);
    }

    /// Oracle: §3's child table last row — the Quit button queues the epilogue, which is the
    /// client's only clean exit path (§10).
    #[test]
    fn the_quit_button_queues_the_epilogue() {
        let mut ui = UiSystem::new((800, 600));
        let mut s = DataPatchScreen::default();
        s.on_element_message(
            &mut dereth_ui::framework::ScreenCx::new(&mut ui),
            &ElementMessage {
                source_id: QUIT_BUTTON,
                source: ElemHandle::for_test(1),
                id: MessageId(1),
                p1: 0,
                p2: 0,
                point: dereth_ui::msg::MessagePoint::default(),
                serial: 1,
            },
        );
        assert_eq!(
            ui.requests.take(),
            vec![crate::view::UiRequest::QueueMode(
                dereth_ui::framework::mode::EPILOGUE
            )]
        );
    }
}
