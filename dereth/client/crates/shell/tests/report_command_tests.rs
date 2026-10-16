//! `@report` and `@perfcapture` through the whole client, on a host whose clipboard records what
//! it is given: the report's line, read back, gives the camera the frame was drawn from to the
//! last bit, and a capture saves a row for every frame of its seconds beside the settings.
//! Fixture: the retail dats, with no server, a body in a device-free world.

use super::*;
use crate::{
    app::CoreApp,
    clipboard::{ClipboardError, HostClipboard},
    platform::host::{Host, NullHost},
};
use dereth_client_contract::UiRequest;
use std::cell::RefCell;

thread_local! {
    /// What the host's clipboard has been given, on this thread.
    static COPIED: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

/// A clipboard that keeps what it is given.
#[derive(Debug, Default, Clone, Copy)]
struct Recording;

impl HostClipboard for Recording {
    fn set_text(&mut self, text: &str) -> Result<(), ClipboardError> {
        COPIED.with(|c| c.borrow_mut().push(text.to_owned()));
        Ok(())
    }
    fn get_text(&mut self) -> Result<Option<String>, ClipboardError> {
        Ok(None)
    }
    fn sequence_number(&mut self) -> Option<u32> {
        None
    }
}

/// The host with nothing under it but a clipboard that records, and a name and a commit.
#[derive(Debug, Clone, Copy, Default)]
struct ReportHost;

impl Host for ReportHost {
    const BUILD_ID: &'static str = "dereth-client 9.8.7";
    const BUILD_COMMIT: &'static str = "0123456789ab";
    type Clipboard = Recording;
    fn open_platform(
        cfg: &dereth_client_runtime::config::Config,
        events: crate::platform::window::WindowEvents,
    ) -> Result<dereth_client_runtime::app::Platform, dereth_client_runtime::app::StartupError>
    {
        NullHost::open_platform(cfg, events)
    }
    fn local_utc_offset_secs(_: i64) -> i32 {
        0
    }
    fn launch_uri(_: &str) -> i32 {
        0
    }
    fn install_default_output() {}
    fn clipboard() -> Self::Clipboard {
        Recording
    }
    fn cursor_images(window: Option<isize>) -> Box<dyn crate::cursor::CursorImages> {
        NullHost::cursor_images(window)
    }
}

struct Client {
    app: CoreApp<ReportHost>,
    shell: ClientShell<ReportHost>,
    state: std::path::PathBuf,
}

impl Client {
    /// The client at the gameplay screen with a body in a device-free world, its settings kept in
    /// a folder of its own named after `name`.
    fn new(name: &str) -> Self {
        COPIED.with(|c| c.borrow_mut().clear());
        let state =
            std::env::temp_dir().join(format!("dereth-report-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&state);
        let cfg = dereth_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::dat_dir(),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<ReportHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| {
                Ok(Box::new(
                    dereth_client_runtime::sim_present::SimPresentation::new(800, 600),
                ))
            },
        )
        .expect("real tables and a device-free world");
        app.interaction.client_build_id = ReportHost::BUILD_ID;
        app.build_commit = ReportHost::BUILD_COMMIT;
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        for _ in 0..2 {
            assert!(app.frame(&mut shell));
        }
        shell
            .modern
            .ui
            .as_mut()
            .expect("UI")
            .queue(dereth_ui::framework::mode::GAME_PLAY);
        let mut c = Self { app, shell, state };
        for _ in 0..6 {
            c.frame();
        }
        let store = std::sync::Arc::clone(c.app.probe().dat_store());
        let scene = dereth_client_runtime::scene::SceneConfig {
            land_radius: 1,
            scenery_radius: 0,
            ..Default::default()
        };
        c.app
            .probe_mut()
            .load_world(&store, scene)
            .expect("a world with a body");
        for _ in 0..10 {
            c.frame();
        }
        c
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    /// `text` typed in chat window 1, and the frame that carries it out.
    fn type_line(&mut self, text: &str) {
        self.app.interaction.queue(
            Vec::new(),
            vec![UiRequest::ChatLine {
                window: 1,
                text: text.into(),
            }],
        );
        self.frame();
    }

    /// How many lines of the chat say `text`: the lines the chat windows were handed, and those
    /// still waiting for them.
    fn lines_saying(&self, text: &str) -> usize {
        let hud: &dereth_client_runtime::hud::Hud = self.app.hud();
        self.shell
            .classic
            .history()
            .map(|l| &l.body)
            .chain(hud.delivered_chat.iter().map(|m| &m.body))
            .chain(hud.pending_chat.iter().map(|(_, m)| &m.body))
            .filter(|body| body.contains(text))
            .count()
    }

    fn finish(self) {
        let Self {
            app,
            mut shell,
            state,
        } = self;
        app.shutdown(&mut shell);
        let _ = std::fs::remove_dir_all(state);
    }
}

/// Behaviour: chat.commands.report-prints-and-copies-a-line-that-reads-back-as-the-frames-camera
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn the_report_line_reads_back_as_the_camera_the_frame_was_drawn_from() {
    use dereth_client_runtime::report::VisualReport;
    let mut c = Client::new("line");
    c.type_line("/report");
    // What the frame the line was typed in was drawn from: the report is made after the camera
    // moves and before the frame is drawn, and nothing moves the camera after it.
    let ws = c.app.world.as_ref().expect("a world");
    let drawn = ws.camera;
    let block = ws
        .viewer_block()
        .expect("a block the world is drawn around");
    let body = ws.character.as_ref().expect("a body").position();
    let copied = COPIED.with(|c| c.borrow().clone());
    assert_eq!(copied.len(), 1, "one line on the clipboard: {copied:?}");
    let line = copied[0].clone();
    assert!(!line.contains('\n'), "{line}");
    // The chat shows a frame's lines from the next frame on.
    c.frame();
    assert_eq!(c.lines_saying(&line), 1, "the same line in the chat");
    assert_eq!(
        c.lines_saying(dereth_client_model::cmd::report::COPIED),
        1,
        "and that it is on the clipboard"
    );

    let report = VisualReport::parse(&line).expect("the line reads");
    let camera = report.camera.expect("the camera").to_render(block);
    // The camera the frame was drawn from, to the last bit.
    for (have, want) in [
        (camera.position.x, drawn.position.x),
        (camera.position.y, drawn.position.y),
        (camera.position.z, drawn.position.z),
        (camera.yaw, drawn.yaw),
        (camera.pitch, drawn.pitch),
    ] {
        assert_eq!(have.to_bits(), want.to_bits(), "{line}");
    }
    let loc = report.loc.expect("where the player stands");
    assert_eq!(loc.cell, body.cell);
    assert!(
        (loc.frame.origin.x - body.frame.origin.x).abs() < 1e-5,
        "{line}"
    );
    assert!(
        (loc.frame.origin.y - body.frame.origin.y).abs() < 1e-5,
        "{line}"
    );
    assert_eq!(report.mode, "chase");
    assert_eq!(report.window, (800, 600));
    assert_eq!(report.build, "dereth-client 9.8.7 0123456789ab");
    assert!(
        report.time.is_some() && !report.weather.is_empty(),
        "{line}"
    );
    c.finish();
}

/// Behaviour: chat.commands.perfcapture-saves-a-row-for-every-frame-of-its-seconds
#[test]
#[cfg_attr(not(feature = "retail-dats"), ignore = "reads retail data")]
fn perfcapture_saves_a_row_for_every_frame_of_its_seconds_beside_the_settings() {
    let mut c = Client::new("capture");
    c.type_line("/perfcapture 1");
    c.frame();
    assert_eq!(
        c.lines_saying("Recording a performance capture for 1 second."),
        1
    );
    c.type_line("/perfcapture 2");
    c.frame();
    assert_eq!(
        c.lines_saying(dereth_client_model::cmd::report::ALREADY_RECORDING),
        1,
        "one capture at a time"
    );
    // The headless clock steps a thirtieth of a second a frame: the capture's second is thirty
    // frames, saved at the start of the frame after its last.
    let mut frames = 0;
    while c.app.perf_capture.is_some() {
        assert!(frames < 200, "the capture never finished");
        c.frame();
        frames += 1;
    }
    c.frame();
    assert_eq!(c.lines_saying("Performance capture saved: "), 1);
    assert_eq!(c.lines_saying("30 frames: median "), 1);
    let saved: Vec<_> = std::fs::read_dir(&c.state)
        .expect("the settings folder")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("perf-capture-") && n.ends_with(".csv"))
        })
        .collect();
    assert_eq!(saved.len(), 1, "{saved:?}");
    let text = std::fs::read_to_string(&saved[0]).expect("the capture reads");
    assert!(
        text.lines()
            .any(|l| l.starts_with("# report: dereth report 1; loc 0x")),
        "the report the capture began with:\n{text}"
    );
    for head in ["# gpu: ", "# driver: ", "# present mode: ", "# frames: 30"] {
        assert!(text.lines().any(|l| l.starts_with(head)), "{head}:\n{text}");
    }
    let table: Vec<&str> = text.lines().filter(|l| !l.starts_with('#')).collect();
    let header: Vec<&str> = table[0].split(',').collect();
    assert_eq!(table[0], dereth_client_runtime::perf_capture::header());
    assert_eq!(table.len() - 1, 30, "a row for every frame of the second");
    let column = |name: &str| header.iter().position(|h| *h == name).expect(name);
    for (k, row) in table[1..].iter().enumerate() {
        let fields: Vec<&str> = row.split(',').collect();
        assert_eq!(fields.len(), header.len(), "{row}");
        assert_eq!(fields[0], k.to_string());
        for name in [
            "frame_ms",
            "ui",
            "world",
            "draw_world",
            "present_frame",
            "pace",
        ] {
            let ms: f32 = fields[column(name)]
                .parse()
                .unwrap_or_else(|_| panic!("{name} in {row}"));
            assert!(ms >= 0.0, "{name} in {row}");
        }
        let frame_ms: f32 = fields[column("frame_ms")].parse().unwrap();
        assert!(frame_ms > 0.0, "a frame takes time: {row}");
    }
    c.finish();
}
