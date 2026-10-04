//! The real display-request consumer and host resize callback, without a graphics device.
use super::*;
use dereth_client_contract::resolution::{
    ResolutionAction, ResolutionPolicy, ResolutionPromptKind,
};
use dereth_client_contract::{options::store, PrefValue, UiRequest};
use std::cell::Cell;
use std::rc::Rc;

struct Window {
    granted: Rc<Cell<(u32, u32)>>,
    requests: Rc<Cell<u32>>,
}
impl crate::platform::window::WindowHost for Window {
    fn has_window(&self) -> bool {
        true
    }
    fn screen_metrics(
        &self,
        _frame: (i32, i32, i32),
    ) -> dereth_client_contract::window_proc::ScreenMetrics {
        crate::platform::window::headless_screen_metrics()
    }
    fn client_size(&self) -> (u32, u32) {
        self.granted.get()
    }
    fn request_inner_size(&self, _w: u32, _h: u32) -> (u32, u32) {
        self.requests.set(self.requests.get() + 1);
        self.granted.get()
    }
}
fn app() -> App<NullShell> {
    store::init();
    let cfg = Config {
        headless: true,
        connect: false,
        sound: false,
        ui: false,
        width: 800,
        height: 600,
        dat_dir: dereth_dat::testing::dat_dir(),
        ..Config::default()
    };
    let mut a = App::<NullShell>::bring_up_with_store(
        cfg,
        Some(std::sync::Arc::new(
            dereth_dat::testing::open_store().expect("retail data"),
        )),
        |_| Ok(Platform::headless(800, 600)),
        |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
    )
    .unwrap();
    a.use_forced_resolution = false;
    a.applied_resolution = (800, 600);
    a.cfg.width = 800;
    a.cfg.height = 600;
    store::set_value("Display.Resolution", PrefValue::Int((800 << 16) | 600));
    a
}
fn dispatch(a: &mut App<NullShell>, r: ResolutionAction, now: f64) {
    a.timer.cur_time = now;
    assert!(a
        .apply_display_preference_requests(&NullShell, vec![UiRequest::Resolution(r)])
        .is_empty());
    a.apply_changed_display_presentation(&mut NullShell);
}
fn begin(policy: ResolutionPolicy) -> ResolutionAction {
    ResolutionAction::Begin {
        size: (1024, 768),
        policy,
        persist: true,
    }
}
fn pref() -> Option<PrefValue> {
    store::inq_value("Display.Resolution")
}

/// Behaviour: presentation.resolution.shared-transaction
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail data: --features retail-dats"
)]
fn display_requests_wait_for_actual_host_size_and_rearm_each_revert() {
    let mut a = app();
    let granted = Rc::new(Cell::new((800, 600)));
    let requests = Rc::new(Cell::new(0));
    a.window = Box::new(Window {
        granted: granted.clone(),
        requests: requests.clone(),
    });
    store::set_value("Render.ScreenBrightness", PrefValue::Float(0.4));
    dispatch(&mut a, begin(ResolutionPolicy::Modern), 1.0);
    assert_eq!(requests.get(), 1);
    assert_eq!(a.present.size(), (800, 600));
    assert!(
        a.resolution.prompt().is_none(),
        "queueing and the old host extent are not success"
    );
    a.resolution_host_result(true, (900, 700));
    assert!(
        a.resolution.pending(),
        "an unrelated failed size report cannot fail the awaited request"
    );
    a.timer.cur_time = 1.5;
    a.handle_window_event(
        &mut NullShell,
        &HostEvent::Resized {
            width: 1024,
            height: 768,
        },
        0,
    );
    assert_eq!(a.present.size(), (1024, 768));
    a.tick_resolution(&mut NullShell, 1.6);
    assert!(a.resolution.prompt().is_none());
    a.tick_resolution(&mut NullShell, 1.7);
    let p = a.resolution.prompt().unwrap();
    assert_eq!(p.deadline, Some(11.7));
    store::set_value("UI.Interface", PrefValue::Int(1));
    a.tick_resolution(&mut NullShell, 2.0);
    assert_eq!(a.resolution.prompt().unwrap().deadline, Some(11.7));
    dispatch(
        &mut a,
        ResolutionAction::Answer {
            token: p.token,
            yes: true,
        },
        2.1,
    );
    assert!(a.resolution.pending(), "old presenter's answer is stale");
    // The host refuses the restore by continuing to report the tested size.
    granted.set((1024, 768));
    let token = a.resolution.prompt().unwrap().token;
    dispatch(&mut a, ResolutionAction::Answer { token, yes: false }, 3.0);
    assert_eq!(requests.get(), 2);
    a.tick_resolution(&mut NullShell, 4.999);
    assert!(a.resolution.prompt().is_none());
    a.tick_resolution(&mut NullShell, 5.0);
    assert_eq!(
        a.resolution.prompt().unwrap().kind,
        ResolutionPromptKind::RevertFailed
    );
    assert_eq!(pref(), Some(PrefValue::Int((1024 << 16) | 768)));
    assert_eq!(
        store::inq_value("Render.ScreenBrightness"),
        Some(PrefValue::Float(0.4))
    );
    a.apply_changed_display_presentation(&mut NullShell);
    assert_eq!(
        requests.get(),
        2,
        "failure reconciliation does not issue another resize"
    );
}

/// Behaviour: presentation.resolution.shared-transaction
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail data: --features retail-dats"
)]
fn classic_acceptance_and_noninteractive_display_paths_keep_their_save_policy() {
    let mut a = app();
    dispatch(&mut a, begin(ResolutionPolicy::Classic), 10.0);
    assert_eq!(a.present.size(), (800, 600));
    let token = a.resolution.prompt().unwrap().token;
    dispatch(&mut a, ResolutionAction::Answer { token, yes: true }, 11.0);
    assert_eq!(a.present.size(), (1024, 768));
    assert_eq!(
        pref(),
        Some(PrefValue::Int((800 << 16) | 600)),
        "Classic test not yet saved"
    );
    assert_eq!(a.resolution.prompt().unwrap().deadline, Some(26.0));
    let token = a.resolution.prompt().unwrap().token;
    dispatch(&mut a, ResolutionAction::Answer { token, yes: true }, 12.0);
    assert_eq!(pref(), Some(PrefValue::Int((1024 << 16) | 768)));
    let r = UiRequest::SetPreference("Display.Resolution", PrefValue::Int((800 << 16) | 600));
    store::set_value("Display.Resolution", PrefValue::Int((800 << 16) | 600));
    a.apply_display_preference_requests(&NullShell, vec![r]);
    a.apply_changed_display_presentation(&mut NullShell);
    assert!(!a.resolution.pending());
    assert_eq!(a.present.size(), (800, 600));
    a.pump.state.full_screen = true;
    dispatch(&mut a, begin(ResolutionPolicy::Modern), 20.0);
    assert!(
        !a.resolution.pending(),
        "fullscreen stores the later windowed size without a test"
    );
    a.pump.state.full_screen = false;
    a.use_forced_resolution = true;
    a.forced_resolution = (800, 600);
    dispatch(&mut a, begin(ResolutionPolicy::Modern), 21.0);
    assert!(!a.resolution.pending());
    assert_eq!(a.present.size(), (800, 600));
    dispatch(
        &mut a,
        ResolutionAction::Begin {
            size: (640, 480),
            policy: ResolutionPolicy::Modern,
            persist: false,
        },
        22.0,
    );
    assert_eq!(pref(), Some(PrefValue::Int((640 << 16) | 480)));
    assert_eq!(a.present.size(), (800, 600));
}
