//! Behaviour: none (classic front-end adapter; no retail behaviour claim).
use super::*;
use dereth_client_contract::view::{AllegianceAction, BookPageView, BookView, TradeRow, TradeView};
#[derive(Debug, Default)]
struct View {
    no_selection: bool,
    coords: Option<(f32, f32)>,
    now: f64,
    pings: u64,
    book: Option<BookView>,
    book_session: dereth_client_contract::book::BookSessionView,
    trade: TradeView,
    fellow: Option<dereth_client_contract::view::FellowshipView>,
    slumlord: Option<dereth_client_contract::view::SlumlordView>,
    paid: bool,
    payments: dereth_client_contract::panels::slumlord::PaymentListsView,
    salvage: dereth_client_contract::panels::salvage::SalvageListView,
    xp: Option<dereth_client_contract::statmgmt::XpHeader>,
    available: i64,
    mini: Option<dereth_client_contract::view::MiniGameView>,
    roster: dereth_client_contract::view::AllegianceRoster,
    era: Option<dereth_client_contract::EraView>,
    friends: Vec<dereth_client_contract::view::FriendEntry>,
    squelches: Vec<dereth_client_contract::view::SquelchEntry>,
    allegiance_breaks: Option<i32>,
    oath_cost: Option<u32>,
}
impl GameView for View {
    fn player_coords(&self) -> Option<(f32, f32)> {
        self.coords
    }
    fn player_outside(&self) -> bool {
        self.coords.is_some()
    }
    fn oath_xp_cost(&self) -> Option<u32> {
        self.oath_cost
    }
    fn int_stat(&self, _: ObjectId, prop: u32) -> Option<i32> {
        (prop == 0x84).then_some(self.allegiance_breaks).flatten()
    }
    fn allegiance_roster(&self) -> dereth_client_contract::view::AllegianceRoster {
        self.roster.clone()
    }
    fn era(&self) -> Option<&dereth_client_contract::EraView> {
        self.era.as_ref()
    }
    fn friends(&self) -> Vec<dereth_client_contract::view::FriendEntry> {
        self.friends.clone()
    }
    fn squelch_list(&self) -> Vec<dereth_client_contract::view::SquelchEntry> {
        self.squelches.clone()
    }
    fn minigame(&self) -> Option<dereth_client_contract::view::MiniGameView> {
        self.mini
    }
    fn payment_lists(&self) -> dereth_client_contract::panels::slumlord::PaymentListsView {
        self.payments.clone()
    }
    fn salvage_list(&self) -> dereth_client_contract::panels::salvage::SalvageListView {
        self.salvage.clone()
    }
    fn salvage_item_suitable(&self, _: ObjectId, _: u32) -> bool {
        true
    }
    fn experience_header(&self) -> Option<dereth_client_contract::statmgmt::XpHeader> {
        self.xp
    }
    fn available_experience(&self) -> i64 {
        self.available
    }
    fn selection_query_facts(
        &self,
        _: ObjectId,
    ) -> Option<dereth_client_contract::view::SelectionQueryFacts> {
        Some(dereth_client_contract::view::SelectionQueryFacts {
            is_player: true,
            has_pet_owner: false,
            attackable: false,
            owned_by_player: false,
        })
    }
    fn player(&self) -> Option<ObjectId> {
        Some(ObjectId(7))
    }
    fn fellowship(&self) -> Option<dereth_client_contract::view::FellowshipView> {
        self.fellow.clone()
    }
    fn slumlord(&self) -> Option<dereth_client_contract::view::SlumlordView> {
        self.slumlord.clone()
    }
    fn slumlord_payment(
        &self,
        _: bool,
        _: &[(u32, i32, Option<i32>)],
    ) -> dereth_client_contract::view::SlumlordPayment {
        dereth_client_contract::view::SlumlordPayment {
            paid_in_full: self.paid,
            ..Default::default()
        }
    }
    fn slumlord_needs_more(
        &self,
        _: bool,
        _: &[(u32, i32, Option<i32>)],
        _: u32,
        _: Option<i32>,
    ) -> bool {
        true
    }
    fn slumlord_pay(
        &self,
        _: bool,
        _: &[(u32, i32, Option<i32>)],
        _: u32,
        _: i32,
        _: Option<i32>,
    ) -> bool {
        true
    }
    fn item_owned_by_player(&self, _: ObjectId) -> bool {
        true
    }
    fn item_house_payment(&self, _: ObjectId) -> i32 {
        1
    }

    fn now(&self) -> f64 {
        self.now
    }
    fn ping_returns(&self) -> u64 {
        self.pings
    }
    fn open_book(&self) -> Option<BookView> {
        self.book.clone()
    }
    fn book_session(&self) -> dereth_client_contract::book::BookSessionView {
        self.book_session.clone()
    }
    fn trade(&self) -> TradeView {
        self.trade.clone()
    }
    fn selected_object(&self) -> Option<ObjectId> {
        (!self.no_selection).then_some(ObjectId(9))
    }
}
fn with_context<T>(v: &View, f: impl FnOnce(&Context<'_>) -> T) -> T {
    let pregame = PregameView::default();
    let keyboard = KeyboardState::default();
    let settings = ClassicSettings::default();
    f(&Context {
        game: v,
        pregame: &pregame,
        keyboard: &keyboard,
        settings: &settings,
        map_teleport_allowed: false,
        classic: &ClassicState::default(),
    })
}
fn event(p: &mut dyn Panel, e: ControlEvent, v: &View) -> Vec<PanelAction> {
    with_context(v, |c| p.event(e, c))
}
fn activate(p: &mut dyn Panel, id: &str, v: &View) -> Vec<PanelAction> {
    event(p, ControlEvent::Activate(id.into()), v)
}
/// Behaviour: options.pages.the-classic-window-draws-the-shared-pages-its-own-way
#[test]
fn the_options_page_has_the_shared_buttons_and_the_interface_choice_is_a_client_row() {
    use dereth_client_contract::options::interface::{Interface, INTERFACE};
    dereth_client_contract::options::store::init();
    let v = View::default();
    let mut p = make("options").unwrap();
    let shown = with_context(&v, |c| p.frame(c));
    let ids: Vec<&str> = shown.controls.iter().map(|c| c.id.as_str()).collect();
    for id in ["leave", "exit", "keyboard", "urgent", "abuse"] {
        assert!(ids.contains(&id), "{id} in {ids:?}");
    }
    for gone in ["retail-interface", "acceleration", "help", "show-trade"] {
        assert!(!ids.contains(&gone), "{gone} in {ids:?}");
    }
    assert!(matches!(
        &activate(&mut *p, "exit", &v)[0],
        PanelAction::Confirm { accept, .. } if *accept == vec![PanelAction::Host(HostAction::Quit)]
    ));
    // Configure Keyboard leaves the world, and the key page opens on the character screen after,
    // not in the world it is leaving.
    assert!(matches!(
        &activate(&mut *p, "keyboard", &v)[0],
        PanelAction::Confirm { accept, .. } if *accept == vec![
            PanelAction::Game(UiRequest::EndCharacterSession { ask: false }),
            PanelAction::Host(HostAction::KeyboardOnLeaving),
        ]
    ));
    // The interface is chosen on the Client page, as the other interface's is, and written
    // on Apply.
    activate(&mut *p, "sound", &v);
    let client = with_context(&v, |c| p.frame(c));
    let row = format!("row:{INTERFACE}");
    assert!(client.controls.iter().any(|c| c.id == row));
    event(&mut *p, ControlEvent::Select { id: row, index: 1 }, &v);
    let applied = activate(&mut *p, "apply", &v);
    assert!(
        applied.contains(&PanelAction::Game(UiRequest::SetPreference(
            INTERFACE,
            dereth_client_contract::PrefValue::Int(Interface::Classic.value())
        ))),
        "{applied:?}"
    );
}
#[test]
fn subscriptions_switch_once_and_close_unsubscribes() {
    let v = View::default();
    let mut p = make("allegiance").unwrap();
    assert_eq!(
        event(&mut *p, ControlEvent::Tick, &v),
        request(UiRequest::AllegianceUpdateRequest { on: true })
    );
    assert!(event(&mut *p, ControlEvent::Tick, &v).is_empty());
    assert_eq!(
        activate(&mut *p, "tab1", &v),
        vec![
            PanelAction::Game(UiRequest::AllegianceUpdateRequest { on: false }),
            PanelAction::Game(UiRequest::FellowshipUpdateRequest { on: true })
        ]
    );
    assert_eq!(
        activate(&mut *p, "close", &v),
        vec![
            PanelAction::Game(UiRequest::FellowshipUpdateRequest { on: false }),
            PanelAction::Close
        ]
    );
}
#[test]
fn allegiance_swear_has_no_local_confirmation() {
    let v = View::default();
    let mut p = make("allegiance").unwrap();
    assert_eq!(
        activate(&mut *p, "swear", &v),
        vec![PanelAction::Host(HostAction::AllegianceSend {
            action: AllegianceAction::Swear,
            target: ObjectId(9)
        })]
    );
}
#[test]
fn urgent_enable_latch_survives_deletion_and_sends_its_actual_text() {
    let v = View::default();
    let mut p = make("urgent-assistance").unwrap();
    activate(&mut *p, "begin", &v);
    event(
        &mut *p,
        ControlEvent::Edit {
            id: "text".into(),
            text: "Please help".into(),
        },
        &v,
    );
    event(
        &mut *p,
        ControlEvent::Edit {
            id: "text".into(),
            text: String::new(),
        },
        &v,
    );
    assert_eq!(
        activate(&mut *p, "send", &v),
        request(UiRequest::ChannelBroadcast {
            channel: 0x400,
            text: String::new()
        })
    );
    assert!(activate(&mut *p, "send", &v).is_empty());
}
#[test]
fn abuse_off_prefix_sends_disable_request_and_retains_complaint() {
    let v = View::default();
    let mut p = make("abuse").unwrap();
    activate(&mut *p, "begin", &v);
    for (id, text) in [("name", "  OFF, Character"), ("text", "reason")] {
        event(
            &mut *p,
            ControlEvent::Edit {
                id: id.into(),
                text: text.into(),
            },
            &v,
        );
    }
    assert_eq!(
        activate(&mut *p, "send", &v),
        vec![PanelAction::Host(HostAction::AbuseLog {
            target: "Character".into(),
            enabled: false,
            complaint: "reason".into()
        })]
    );
}
#[test]
fn ping_waits_ten_seconds_and_expires_only_after_two_minutes() {
    let mut v = View::default();
    let mut p = make("link-status").unwrap();
    assert_eq!(
        event(&mut *p, ControlEvent::Tick, &v),
        request(UiRequest::RequestPing)
    );
    for now in [9., 10., 119.] {
        v.now = now;
        assert!(event(&mut *p, ControlEvent::Tick, &v).is_empty());
    }
    v.now = 129.;
    assert_eq!(
        event(&mut *p, ControlEvent::Tick, &v),
        request(UiRequest::RequestPing)
    );
    v.now = 129.02;
    v.pings = 1;
    assert!(event(&mut *p, ControlEvent::Tick, &v).is_empty());
    v.now = 139.;
    assert_eq!(
        event(&mut *p, ControlEvent::Tick, &v),
        request(UiRequest::RequestPing)
    );
}
fn open_test_book(world: &mut dereth_client_model::World, id: u32, text: &[&str]) {
    world.player = Some(ObjectId(7));
    world.open_book(
        ObjectId(id),
        8,
        dereth_protocol::trade::PageDataList {
            pages: text
                .iter()
                .map(|text| dereth_protocol::trade::PageData {
                    author_id: ObjectId(7),
                    page_text: Some((*text).into()),
                    ..Default::default()
                })
                .collect(),
            ..Default::default()
        },
        String::new(),
        ObjectId(0),
        String::new(),
        &mut dereth_client_model::NullSink,
        dereth_primitives::ServerTime(0.0),
    );
}
fn book_event(
    panel: &mut dyn Panel,
    e: ControlEvent,
    world: &mut dereth_client_model::World,
) -> Vec<PanelAction> {
    let v = View {
        book_session: world.book_session_view(),
        book: world.book.open.as_ref().map(|b| BookView {
            book_id: b.book_id,
            player_id: ObjectId(7),
            max_num_pages: b.max_num_pages,
            pages: b
                .pages
                .pages
                .iter()
                .map(|p| BookPageView {
                    author_id: p.author_id,
                    author_name: p.author_name.clone(),
                    author_account: p.author_account.clone(),
                    ignore_author: p.ignore_author,
                    text: p.page_text.clone(),
                })
                .collect(),
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut out = vec![];
    for action in event(panel, e, &v) {
        if let PanelAction::Game(UiRequest::Book(action)) = action {
            world.book_action(action);
            out.extend(
                world
                    .take_book_requests()
                    .into_iter()
                    .map(PanelAction::Game),
            );
        } else {
            out.push(action);
        }
    }
    out
}
#[test]
fn deleting_blank_book_page_keeps_shifted_shared_page_on_later_events() {
    let mut world = dereth_client_model::World::new();
    open_test_book(&mut world, 50, &[" ", "second"]);
    let mut p = make("book").unwrap();
    assert_eq!(
        book_event(&mut *p, ControlEvent::Activate("next".into()), &mut world),
        request(UiRequest::BookDeletePage {
            book: ObjectId(50),
            page: 0
        })
    );
    book_event(
        &mut *p,
        ControlEvent::Edit {
            id: "text".into(),
            text: "revised second".into(),
        },
        &mut world,
    );
    assert_eq!(world.book_session_view().draft, "revised second");
    assert_eq!(
        book_event(&mut *p, ControlEvent::Activate("close".into()), &mut world),
        vec![
            PanelAction::Game(UiRequest::BookModifyPage {
                book: ObjectId(50),
                page: 0,
                text: "revised second".into()
            }),
            PanelAction::Game(UiRequest::UnregisterBookRange),
            PanelAction::Close
        ]
    );
}
#[test]
fn add_page_receipts_update_the_shared_session_without_panel_replay() {
    let mut world = dereth_client_model::World::new();
    open_test_book(&mut world, 50, &[]);
    assert_eq!(
        world.take_book_requests(),
        [UiRequest::BookAddPage { book: ObjectId(50) }]
    );
    let mut p = make("book").unwrap();
    assert!(book_event(&mut *p, ControlEvent::Tick, &mut world).is_empty());
    world.book_add_page_response(ObjectId(50), 0, false);
    assert_eq!(
        world.take_book_requests(),
        [UiRequest::BookData { book: ObjectId(50) }]
    );
    assert!(book_event(&mut *p, ControlEvent::Tick, &mut world).is_empty());
    world.book_add_page_response(ObjectId(50), 0, true);
    assert!(book_event(&mut *p, ControlEvent::Tick, &mut world).is_empty());
    book_event(
        &mut *p,
        ControlEvent::Edit {
            id: "text".into(),
            text: "new page".into(),
        },
        &mut world,
    );
    assert!(
        matches!(book_event(&mut *p, ControlEvent::Activate("close".into()), &mut world).first(), Some(PanelAction::Game(UiRequest::BookModifyPage { text, .. })) if text == "new page")
    );
}

#[test]
fn trade_accept_uses_both_displayed_counts_and_offer_scroll_is_independent() {
    let v = View {
        trade: TradeView {
            open: true,
            self_rows: vec![TradeRow::default(); 12],
            partner_rows: vec![TradeRow::default(); 3],
            partner: Some(ObjectId(77)),
            ..Default::default()
        },
        ..Default::default()
    };
    let mut p = make("trade").unwrap();
    p.resize(600, 110);
    with_context(&v, |c| p.frame(c));
    assert_eq!(
        activate(&mut *p, "accept", &v),
        request(UiRequest::TradeAccept {
            displayed_self: 12,
            displayed_partner: 3
        })
    );
    event(
        &mut *p,
        ControlEvent::Scroll {
            id: "offer-scroll".into(),
            value: 32,
        },
        &v,
    );
    let f = with_context(&v, |c| p.frame(c));
    for (id, want) in [("offer", 32), ("partner", 0)] {
        assert!(
            matches!(&f.controls.iter().find(|c|c.id==id).unwrap().kind,ControlKind::ItemStrip{offset,..}if *offset==want)
        );
    }
}
/// Behaviour: options.client-page.the-classic-defaults-reset-the-texture-sizes-as-the-retail-ones-do
#[test]
fn sound_reset_restores_saved_draft_and_defaults_reset_the_texture_sizes() {
    use dereth_client_contract::options::{interface, store};
    use dereth_client_contract::PrefValue;
    store::init();
    store::set_value(interface::INTERFACE, PrefValue::Int(1));
    store::set_value("Camera.AdjustmentSpeed", PrefValue::Float(5.0));
    store::set_value("Input.MouseLookSensitivity", PrefValue::Float(0.01));
    store::set_value("Sound.InterfaceSoundVolume", PrefValue::Float(0.2));
    let v = View::default();
    let pregame = PregameView::default();
    let keyboard = KeyboardState::default();
    let settings = ClassicSettings {
        sound_available: true,
        detail_available: true,
        resolutions: vec![(1024, 768), (800, 600)],
        effects_volume: 0.9,
        landscape_detail: true,
        ..Default::default()
    };
    let c = Context {
        game: &v,
        pregame: &pregame,
        keyboard: &keyboard,
        settings: &settings,
        map_teleport_allowed: false,
        classic: &ClassicState::default(),
    };
    let mut p = make("sound-graphics").unwrap();
    p.event(ControlEvent::Tick, &c);
    p.event(
        ControlEvent::Value {
            id: "vol:Sound.SoundVolume".into(),
            value: 10,
        },
        &c,
    );
    assert_eq!(
        p.event(ControlEvent::Activate("reset".into()), &c),
        vec![PanelAction::Host(HostAction::ResetClassicSettings)]
    );
    let out = p.event(ControlEvent::Activate("defaults".into()), &c);
    let PanelAction::Host(HostAction::DefaultClassicSettings(s)) = &out[0] else {
        panic!("missing default settings application")
    };
    // The environment's detail textures on and the landscape's off, the shared set's defaults.
    assert!(s.environment_detail && !s.landscape_detail);
    assert_eq!(s.resolution, 0, "the size the client starts at, 1024x768");
    assert_eq!(s.effects_volume, 1.0);
    assert!((s.camera_stiffness - 0.23).abs() < 0.000001);
    assert!(!s.auto_degrade);
    assert!(s.effects && s.ambient && s.interface && s.stereo);
    assert_eq!(s.ambient_volume, 1.0);
    assert_eq!(s.brightness, 0.5);
    assert_eq!(s.performance, 0.5);
    assert!(!s.full_screen);
    let apply = p.event(ControlEvent::Activate("apply".into()), &c);
    for (name, value) in [
        ("Camera.AdjustmentSpeed", PrefValue::Float(40.0)),
        ("Input.MouseLookSensitivity", PrefValue::Float(0.55)),
        ("Sound.InterfaceSoundVolume", PrefValue::Float(1.0)),
        ("Render.TextureFiltering", PrefValue::Int(1)),
    ] {
        assert!(apply.contains(&PanelAction::Game(UiRequest::SetPreference(name, value))));
    }
    assert!(!apply.iter().any(|a| matches!(
        a,
        PanelAction::Game(UiRequest::SetPreference(interface::INTERFACE, _))
    )));
    assert_eq!(
        store::inq_value(interface::INTERFACE),
        Some(PrefValue::Int(1))
    );
    let f = p.frame(&c);
    assert!(matches!(
        &f.controls
            .iter()
            .find(|c| c.id == "row:Display.Resolution")
            .unwrap()
            .kind,
        ControlKind::Choice { .. }
    ));
    // The texture sizes at the shared set's defaults, Medium and High, written on Apply.
    for (name, want) in [
        ("Render.LandscapeTextureDetail", 2),
        ("Render.EnvironmentTextureDetail", 1),
    ] {
        assert!(
            apply.contains(&PanelAction::Game(UiRequest::SetPreference(
                name,
                PrefValue::Int(want)
            ))),
            "{name}"
        );
        assert_eq!(store::inq_value(name), Some(PrefValue::Int(want)), "{name}");
    }
    // Unavailable sound stays off; detail defaults retain the shared values. A missing
    // default display mode leaves the selected supported mode in place.
    let unavailable = ClassicSettings {
        sound_available: false,
        detail_available: false,
        resolutions: vec![(800, 600)],
        resolution: 0,
        effects: true,
        ambient: true,
        interface: true,
        environment_detail: true,
        landscape_detail: true,
        ..settings.clone()
    };
    let limited = Context {
        settings: &unavailable,
        ..c
    };
    let mut p = make("sound-graphics").unwrap();
    p.event(ControlEvent::Tick, &limited);
    let out = p.event(ControlEvent::Activate("defaults".into()), &limited);
    let PanelAction::Host(HostAction::DefaultClassicSettings(s)) = &out[0] else {
        panic!()
    };
    assert!(!s.effects && !s.ambient && !s.interface);
    assert!(s.environment_detail && !s.landscape_detail);
    assert_eq!(s.resolutions[s.resolution], (800, 600));
    assert_eq!(s.effects_volume, 1.0);
    assert_eq!(s.ambient_volume, 1.0);
}

/// Every control and every piece of text the classic Client Options page draws over its whole
/// scroll, from the top down.
fn whole_client_page(p: &mut dyn Panel, c: &Context<'_>) -> (Vec<String>, Vec<Control>) {
    let (mut words, mut controls) = (Vec::new(), Vec::<Control>::new());
    for scroll in (0..2_000).step_by(24) {
        p.event(
            ControlEvent::Scroll {
                id: "scroll".into(),
                value: scroll,
            },
            c,
        );
        let f = p.frame(c);
        words.extend(texts(&f));
        for k in f.controls {
            if !controls.iter().any(|o| o.id == k.id) {
                controls.push(k);
            }
        }
    }
    (words, controls)
}

/// Behaviour: options.client-page.the-classic-graphics-rows-read-as-the-retail-ones
#[test]
fn the_classic_graphics_rows_carry_the_retail_captions_end_labels_and_texture_steps() {
    use dereth_client_contract::options::store;
    use dereth_client_contract::PrefValue;
    store::init();
    assert!(store::set_value(
        "Render.LandscapeTextureDetail",
        PrefValue::Int(0)
    ));
    assert!(store::set_value(
        "Render.EnvironmentTextureDetail",
        PrefValue::Int(3)
    ));
    let v = View::default();
    let pregame = PregameView::default();
    let keyboard = KeyboardState::default();
    let settings = ClassicSettings {
        detail_available: true,
        resolutions: vec![(1024, 768)],
        ..Default::default()
    };
    let c = Context {
        game: &v,
        pregame: &pregame,
        keyboard: &keyboard,
        settings: &settings,
        map_teleport_allowed: false,
        classic: &ClassicState::default(),
    };
    let mut p = make("sound-graphics").unwrap();
    p.event(ControlEvent::Tick, &c);
    let (words, controls) = whole_client_page(&mut *p, &c);
    let has = |w: &str| words.iter().any(|t| t == w);
    for caption in [
        "Screen Brightness",
        "Adaptive Degrade",
        "Adaptive Degrade Bias",
        "Degrade Distance",
        "Landscape Texture Detail",
        "Environment Texture Detail",
        "Texture Filtering",
        "Landscape Draw Distance",
        "Environment Detail Textures",
        "Multiple Pass Alpha",
    ] {
        assert!(has(caption), "{caption} is not on the page: {words:?}");
    }
    for gone in [
        "Auto-Degrade",
        "Graphics Performance",
        "Manual Degrade Bias",
    ] {
        assert!(!has(gone), "{gone} is still on the page");
    }
    for end in ["Dark", "Bright", "Speed", "Detail", "Close", "Far"] {
        assert!(has(end), "the end caption {end} is not drawn");
    }
    let menu = |name: &str| {
        let id = format!("row:{name}");
        match &controls.iter().find(|k| k.id == id).expect("the menu").kind {
            ControlKind::Choice { options, selected } => (options.clone(), *selected),
            other => panic!("{name} is {other:?}"),
        }
    };
    let steps: Vec<String> = ["Very Low", "Low", "Medium", "High", "Very High"]
        .map(String::from)
        .to_vec();
    // Every stored value shows as itself: Very High (0) and Low (3).
    assert_eq!(menu("Render.LandscapeTextureDetail"), (steps.clone(), 4));
    assert_eq!(menu("Render.EnvironmentTextureDetail"), (steps, 1));
    // Both detail-texture boxes can be changed.
    for name in [
        "Render.BuildingDetailTextures",
        "Render.LandscapeDetailTextures",
    ] {
        let id = format!("row:{name}");
        assert!(
            controls
                .iter()
                .find(|k| k.id == id)
                .expect("the box")
                .enabled,
            "{name} is greyed"
        );
    }
    // Choosing Very High for the environment writes 0, and nothing rewrites the landscape's 0.
    p.event(
        ControlEvent::Select {
            id: "row:Render.EnvironmentTextureDetail".into(),
            index: 4,
        },
        &c,
    );
    let out = p.event(ControlEvent::Activate("apply".into()), &c);
    assert!(out.contains(&PanelAction::Game(UiRequest::SetPreference(
        "Render.EnvironmentTextureDetail",
        PrefValue::Int(0)
    ))));
    assert_eq!(
        store::inq_value("Render.LandscapeTextureDetail"),
        Some(PrefValue::Int(0))
    );
}

#[test]
fn fellowship_member_layout_hides_creation_options_and_uses_live_vital_rows() {
    use dereth_client_contract::view::{FellowEntry, FellowshipView};
    let v = View {
        fellow: Some(FellowshipView {
            leader: ObjectId(9),
            members: vec![FellowEntry {
                id: ObjectId(7),
                current_health: 12,
                max_health: 20,
                ..Default::default()
            }],
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut p = make("fellowship").unwrap();
    p.resize(300, 400);
    let f = with_context(&v, |c| p.frame(c));
    assert!(!f.controls.iter().any(|c| matches!(
        c.id.as_str(),
        "auto-fellow" | "accept-fellow" | "dismiss" | "leader" | "disband"
    )));
    let quit = f.controls.iter().find(|c| c.id == "quit").unwrap();
    assert_eq!(quit.rect, rect(105, 371, 170, 27));
    assert!(f
        .screen
        .commands
        .iter()
        .any(|c| matches!(c,Command::TextBox{text,..}if text=="12/20")));
    assert!(matches!(
        &f.controls.iter().find(|c| c.id == "members").unwrap().kind,
        ControlKind::HitList { row_height: 32, .. }
    ));
}
#[test]
fn partial_own_maintenance_submits_shared_rows_and_refuses_after_model_clear() {
    use dereth_client_contract::panels::slumlord::{HouseOp, PaymentAction, PaymentItem};
    let mut v = View {
        slumlord: Some(dereth_client_contract::view::SlumlordView {
            slumlord: ObjectId(10),
            owner: ObjectId(7),
            am_i_the_owner: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    v.payments.op = HouseOp::Rent;
    v.payments.rent.push(PaymentItem {
        id: ObjectId(20),
        wcid: 273,
        amount: 1,
        trade_note_value: None,
    });
    let mut p = make("maintenance").unwrap();
    assert_eq!(
        activate(&mut *p, "pay", &v),
        request(UiRequest::PaymentList(PaymentAction::Submit))
    );
    v.payments.rent.clear();
    assert!(activate(&mut *p, "pay", &v).is_empty());
}

/// On a world that charges for an oath, a character with a break cannot swear without the cost
/// (5% of a 20,000 step, a quarter more for the break: 1,250), even by activating the button.
#[test]
fn allegiance_insufficient_xp_blocks_even_direct_activation() {
    let v = View {
        xp: Some(dereth_client_contract::statmgmt::XpHeader {
            level_span: 20000,
            to_level: 100_000,
            ..Default::default()
        }),
        available: 1249,
        oath_cost: Some(1250),
        allegiance_breaks: Some(1),
        era: Some(dereth_client_contract::EraView {
            era: dereth_primitives::era::EraId::Infiltration,
            era_announced: true,
            ..Default::default()
        }),
        ..Default::default()
    };
    let mut p = make("allegiance").unwrap();
    assert!(activate(&mut *p, "swear", &v).is_empty());
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    assert!(shown.contains(&"xp needed: 1".to_string()), "{shown:?}");
    let v = View {
        available: 1250,
        ..v
    };
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    assert!(shown.contains(&"xp cost: 1,250".to_string()), "{shown:?}");
    assert!(matches!(
        activate(&mut *p, "swear", &v).as_slice(),
        [PanelAction::Host(HostAction::AllegianceSend {
            action: AllegianceAction::Swear,
            ..
        })]
    ));
}

/// A first oath is free, and a world that charges nothing for an oath (the end of retail) shows
/// no cost line at all.
#[test]
fn the_oath_cost_line_is_the_eras_and_a_first_oath_is_free() {
    let infiltration = dereth_client_contract::EraView {
        era: dereth_primitives::era::EraId::Infiltration,
        era_announced: true,
        ..Default::default()
    };
    let v = View {
        xp: Some(dereth_client_contract::statmgmt::XpHeader {
            level_span: 20000,
            ..Default::default()
        }),
        era: Some(infiltration),
        ..Default::default()
    };
    let p = make("allegiance").unwrap();
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    assert!(shown.contains(&"xp cost: 0".to_string()), "{shown:?}");
    let v = View {
        era: None,
        allegiance_breaks: Some(3),
        ..v
    };
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    assert!(!shown.iter().any(|t| t.contains("xp")), "{shown:?}");
    // Nor does experience gate the oath there.
    let mut p = make("allegiance").unwrap();
    assert!(matches!(
        activate(&mut *p, "swear", &v).as_slice(),
        [PanelAction::Host(HostAction::AllegianceSend {
            action: AllegianceAction::Swear,
            ..
        })]
    ));
}
#[test]
fn switching_tabs_cancels_the_global_social_target_mode() {
    let v = View::default();
    let mut p = make("allegiance").unwrap();
    activate(&mut *p, "break", &v);
    assert!(activate(&mut *p, "tab1", &v).contains(&PanelAction::Host(HostAction::SocialTarget(0))));
    assert!(event(&mut *p, ControlEvent::WorldTarget(Some(ObjectId(9))), &v).is_empty());
}

#[test]
fn replacing_a_dirty_book_saves_the_original_object() {
    let mut world = dereth_client_model::World::new();
    open_test_book(&mut world, 100, &["old"]);
    let mut p = make("book").unwrap();
    book_event(
        &mut *p,
        ControlEvent::Edit {
            id: "text".into(),
            text: "saved text".into(),
        },
        &mut world,
    );
    open_test_book(&mut world, 200, &["other"]);
    assert_eq!(
        world.take_book_requests(),
        [UiRequest::BookModifyPage {
            book: ObjectId(100),
            page: 0,
            text: "saved text".into()
        }]
    );
    assert!(book_event(&mut *p, ControlEvent::Tick, &mut world).is_empty());
    assert_eq!(world.book_session_view().draft, "other");
}

#[test]
fn partial_housing_drop_routes_quantity_and_does_not_insert_created_stack() {
    use dereth_client_contract::panels::slumlord::PaymentAction;
    let v = View::default();
    let mut p = make("maintenance").unwrap();
    assert_eq!(
        event(
            &mut *p,
            ControlEvent::DropStack {
                id: "items".into(),
                object: ObjectId(20),
                amount: 4,
                max_amount: 10,
                slot: 0,
            },
            &v
        ),
        vec![
            PanelAction::Game(UiRequest::StackSliderChanged { split: 4, max: 10 }),
            PanelAction::Game(UiRequest::PaymentList(PaymentAction::Add(ObjectId(20)))),
        ]
    );
    assert!(event(&mut *p, ControlEvent::SplitReady(ObjectId(21)), &v).is_empty());
    assert!(activate(&mut *p, "pay", &v).is_empty());
}

#[test]
fn salvage_notices_do_not_replay_shared_mutations() {
    use dereth_client_contract::panels::salvage::{SalvageAction, SalvageNotice::*};
    let mut v = View::default();
    v.salvage.tool = Some(ObjectId(20));
    v.salvage.items = vec![ObjectId(2)];
    let mut p = make("salvage").unwrap();
    for notice in [
        Open(ObjectId(20)),
        Add(ObjectId(1)),
        Add(ObjectId(1)),
        Remove(ObjectId(1)),
    ] {
        assert!(event(&mut *p, ControlEvent::Salvage(notice), &v).is_empty());
    }
    assert_eq!(
        activate(&mut *p, "salvage", &v),
        request(UiRequest::SalvageList(SalvageAction::Submit))
    );
    assert_eq!(v.salvage.items, vec![ObjectId(2)]);
}
#[test]
fn game_center_fits_sidebar_height_and_hides_status_below_413() {
    let v = View::default();
    with_context(&v, |c| {
        let state = ClassicState {
            game_status: "It is your turn to move.\n".into(),
            ..Default::default()
        };
        let c = Context {
            classic: &state,
            ..*c
        };
        let mut p = make("game-center").unwrap();
        p.resize(300, 362);
        let f = p.frame(&c);
        assert_eq!(f.screen.height, 362);
        assert!(!f
            .screen
            .commands
            .iter()
            .any(|x| matches!(x,Command::TextBox{text,..} if text.contains("your turn"))));
        p.resize(300, 482);
        assert!(p
            .frame(&c)
            .screen
            .commands
            .iter()
            .any(|x| matches!(x,Command::TextBox{text,..} if text.contains("your turn"))));
    });
}

#[test]
fn game_center_live_board_changes_preserve_cell_mapping_and_stalemate_latch() {
    use dereth_client_contract::view::MiniGameView;
    let mut game = MiniGameView {
        visible: true,
        game: ObjectId(30),
        selected_cell: Some(9),
        ..Default::default()
    };
    game.piece_slots[9] = Some(1);
    let mut v = View {
        mini: Some(game),
        ..Default::default()
    };
    let mut p = make("game-center").unwrap();
    with_context(&v, |c| {
        let f = p.frame(c);
        assert!(f
            .screen
            .commands
            .iter()
            .any(|x| matches!(x, Command::Image { did, x:54, y:77, .. } if did=="06001FCC")));
        assert!(f
            .screen
            .commands
            .iter()
            .any(|x| matches!(x, Command::Image { did, x:54, y:77, .. } if did=="06000F7E")));
        let cell = f.controls.iter().find(|x| x.id == "cell9").unwrap();
        assert_eq!(cell.rect, rect(54, 77, 32, 32));
        assert!(!cell.paint);
    });
    assert_eq!(
        event(
            &mut *p,
            ControlEvent::Select {
                id: "cell9".into(),
                index: 0
            },
            &v
        ),
        request(UiRequest::MiniGameBoardPress(9))
    );
    assert!(event(
        &mut *p,
        ControlEvent::Select {
            id: "cell64".into(),
            index: 0
        },
        &v
    )
    .is_empty());
    assert_eq!(
        activate(&mut *p, "stalemate", &v),
        request(UiRequest::MiniGameButton(0x10000177))
    );
    game.piece_slots[9] = None;
    game.piece_slots[10] = Some(1);
    game.selected_cell = None;
    game.stalemate = true;
    game.draws += 1;
    v.mini = Some(game);
    with_context(&v, |c| {
        let f = p.frame(c);
        assert!(f
            .screen
            .commands
            .iter()
            .any(|x| matches!(x, Command::Image { did, x:86, y:77, .. } if did=="06001FCC")));
        assert!(!f
            .screen
            .commands
            .iter()
            .any(|x| matches!(x, Command::Image { did, .. } if did=="06000F7E")));
        assert_eq!(
            f.controls
                .iter()
                .find(|x| x.id == "stalemate")
                .unwrap()
                .images
                .as_ref()
                .unwrap()[0],
            "06002346"
        );
    });
    assert_eq!(activate(&mut *p, "close", &v), vec![PanelAction::Close]);
    let confirm = activate(&mut *p, "resign", &v);
    assert!(
        matches!(&confirm[0],PanelAction::Confirm { accept, text, .. } if accept==&request(UiRequest::MiniGameQuitAnswer(true)) && text.contains("Default is no."))
    );
}

#[test]
fn failed_housing_split_and_late_result_do_not_change_shared_rows() {
    use dereth_client_contract::panels::slumlord::PaymentAction;
    let v = View::default();
    let mut p = make("maintenance").unwrap();
    assert!(event(&mut *p, ControlEvent::SplitFailed, &v).is_empty());
    assert!(event(&mut *p, ControlEvent::SplitReady(ObjectId(21)), &v).is_empty());
    assert_eq!(
        event(
            &mut *p,
            ControlEvent::DropStack {
                id: "items".into(),
                object: ObjectId(20),
                amount: 4,
                max_amount: 10,
                slot: 0,
            },
            &v
        ),
        vec![
            PanelAction::Game(UiRequest::StackSliderChanged { split: 4, max: 10 }),
            PanelAction::Game(UiRequest::PaymentList(PaymentAction::Add(ObjectId(20)))),
        ]
    );
    assert!(v.payments.buy.is_empty() && v.payments.rent.is_empty());
}

#[test]
fn abuse_empty_name_and_invalid_name_share_text_but_not_emphasis() {
    use crate::panels::FeedbackSeverity::{Information, Warning};
    let v = View::default();
    for (name, severity) in [
        ("", Information),
        ("   ", Warning),
        ("off,", Warning),
        ("Enter exact name of offending character here.", Warning),
    ] {
        let mut p = make("abuse").unwrap();
        activate(&mut *p, "begin", &v);
        for (id, text) in [("name", name), ("text", "reason")] {
            event(
                &mut *p,
                ControlEvent::Edit {
                    id: id.into(),
                    text: text.into(),
                },
                &v,
            );
        }
        assert_eq!(
            activate(&mut *p, "send", &v),
            vec![PanelAction::Host(HostAction::LocalFeedback {
                text: "Please specify the character to log.".into(),
                severity
            })]
        );
    }
}

#[test]
fn fellowship_pick_prompt_returns_the_chosen_target_to_the_runtime() {
    use crate::panels::FeedbackSeverity::Information;
    let v = View {
        no_selection: true,
        ..Default::default()
    };
    let mut p = make("fellowship").unwrap();
    let prompt = activate(&mut *p, "recruit", &v);
    assert!(
        prompt.contains(&PanelAction::Host(HostAction::LocalFeedback {
            text: "Click a character to recruit.".into(),
            severity: Information,
        }))
    );
    assert_eq!(
        event(&mut *p, ControlEvent::WorldTarget(Some(ObjectId(7))), &v),
        vec![
            PanelAction::Host(HostAction::SocialTarget(0)),
            PanelAction::Game(UiRequest::FellowshipRecruit {
                target: ObjectId(7)
            })
        ]
    );
}

/// Every piece of text a frame draws.
fn texts(f: &PanelFrame) -> Vec<String> {
    f.screen
        .commands
        .iter()
        .filter_map(|c| match c {
            crate::Command::TextBox { text, .. } | crate::Command::Text { text, .. } => {
                Some(text.clone())
            }
            _ => None,
        })
        .collect()
}
fn member(id: u32, full_name: &str, rank: u16, cp: u32, logged_in: bool) -> AllegianceEntry {
    AllegianceEntry {
        id: ObjectId(id),
        full_name: full_name.into(),
        logged_in,
        rank,
        cp_cached: cp,
    }
}
use dereth_client_contract::view::{AllegianceEntry, AllegianceRoster};
#[test]
fn a_sworn_vassal_sees_their_patron_and_titled_vassals_without_the_swear_prompt() {
    let v = View {
        roster: AllegianceRoster {
            total_members: 3,
            total_vassals: 1,
            own_cp_tithed: 250,
            subject: Some(member(7, "Knight Aerin", 2, 0, true)),
            player_rank_quality: 2,
            monarch: Some(member(3, "Lord Bram", 3, 0, false)),
            patron: Some(member(3, "Lord Bram", 3, 0, false)),
            vassals: vec![member(11, "Yeoman Eve", 1, 500, true)],
            ..Default::default()
        },
        ..Default::default()
    };
    let mut p = make("allegiance").unwrap();
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    assert!(shown.contains(&"PATRON/MONARCH".to_string()), "{shown:?}");
    assert!(
        shown.contains(&"Followers: 1    Rank: 2".to_string()),
        "{shown:?}"
    );
    // Titles mode: the vassal by title and name, no rank or experience column.
    assert!(shown.contains(&"Yeoman Eve".to_string()), "{shown:?}");
    assert!(!shown
        .iter()
        .any(|t| t.contains("SWEAR") || t.contains("xp cost")));
    assert!(!shown
        .iter()
        .any(|t| t == "Rank" || t.contains("xp produced")));
    // Show XP: the bare name with the experience it passed up, and the patron's line.
    activate(&mut *p, "show-xp", &v);
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    for want in ["Eve", "500", "xp produced", "xp produced:", "250"] {
        assert!(shown.contains(&want.to_string()), "{want} in {shown:?}");
    }
}
#[test]
fn a_monarch_above_the_patron_gets_a_section_of_its_own() {
    let v = View {
        roster: AllegianceRoster {
            total_members: 10,
            subject: Some(member(7, "Aerin", 1, 0, true)),
            monarch: Some(member(2, "King Cole", 6, 0, true)),
            patron: Some(member(3, "Lord Bram", 3, 0, true)),
            ..Default::default()
        },
        ..Default::default()
    };
    let p = make("allegiance").unwrap();
    let shown = with_context(&v, |c| texts(&p.frame(c)));
    for want in [
        "MONARCH",
        "King Cole",
        "PATRON",
        "Lord Bram",
        "Followers:",
        "9",
        "NONE",
    ] {
        assert!(shown.contains(&want.to_string()), "{want} in {shown:?}");
    }
    assert!(!shown.contains(&"PATRON/MONARCH".to_string()));
}
#[test]
fn a_raised_rank_shows_the_difference_and_show_xp_waits_for_someone_to_show() {
    let v = View {
        roster: AllegianceRoster {
            subject: Some(member(7, "Aerin", 2, 0, true)),
            player_rank_quality: 3,
            ..Default::default()
        },
        ..Default::default()
    };
    let p = make("allegiance").unwrap();
    let f = with_context(&v, |c| p.frame(c));
    assert!(texts(&f).contains(&"Followers: 0    Rank: 3 (+1)".to_string()));
    assert!(f.controls.iter().any(|c| c.id == "show-xp" && !c.enabled));
}
#[test]
fn the_social_windows_later_pages_follow_the_classic_page_options_on_any_world() {
    use dereth_client_contract::options::{classic as options, store};
    use dereth_client_contract::PrefValue;
    store::init();
    let early = dereth_client_contract::EraView {
        era: dereth_primitives::era::EraId::Infiltration,
        era_announced: true,
        ..Default::default()
    };
    let classic = View {
        era: Some(early),
        ..Default::default()
    };
    let ids = |v: &View, p: &dyn Panel| {
        with_context(v, |c| p.frame(c))
            .controls
            .iter()
            .map(|c| c.id.clone())
            .collect::<Vec<_>>()
    };
    let has = |v: &View, p: &dyn Panel, id: &str| ids(v, p).contains(&id.to_string());
    // At first: Friends and Squelch on, on the classic world too; Secure Trade off.
    let p = make("friends").unwrap();
    assert!(has(&classic, &*p, "tab3") && has(&classic, &*p, "tab4"));
    assert!(!has(&classic, &*p, "tab2"));
    assert!(has(&classic, &*p, "add-friend"));
    // Secure Trade shown where the world has trade; its tab is pressed to its page.
    store::set_value(options::SHOW_TRADE_TAB, PrefValue::Bool(true));
    let mut q = make("allegiance").unwrap();
    assert!(has(&classic, &*q, "tab2"));
    activate(&mut *q, "tab2", &classic);
    assert!(has(&classic, &*q, "ignore-trade"));
    let mut no_trade = dereth_client_contract::EraView {
        era: dereth_primitives::era::EraId::Infiltration,
        era_announced: true,
        ..Default::default()
    };
    no_trade.announced_features.set("trade", false);
    let tradeless = View {
        era: Some(no_trade),
        ..Default::default()
    };
    assert!(
        !has(&tradeless, &*q, "tab2"),
        "never on a world without trade"
    );
    store::set_value(options::SHOW_TRADE_TAB, PrefValue::Bool(false));
    // Friends turned off: its tab goes, and its buttons do nothing.
    store::set_value(options::SHOW_FRIENDS_TAB, PrefValue::Bool(false));
    let mut q = make("friends").unwrap();
    assert!(!has(&classic, &*q, "tab3") && has(&classic, &*q, "tab4"));
    event(
        &mut *q,
        ControlEvent::Edit {
            id: "entry".into(),
            text: "Bob".into(),
        },
        &classic,
    );
    assert!(activate(&mut *q, "add-friend", &classic).is_empty());
    assert!(activate(&mut *q, "tab3", &classic).is_empty());
    store::set_value(options::SHOW_FRIENDS_TAB, PrefValue::Bool(true));
    let later = View {
        friends: vec![
            dereth_client_contract::view::FriendEntry {
                id: ObjectId(20),
                name: "Zed".into(),
                online: false,
            },
            dereth_client_contract::view::FriendEntry {
                id: ObjectId(21),
                name: "Ann".into(),
                online: true,
            },
        ],
        squelches: vec![dereth_client_contract::view::SquelchEntry {
            name: "Spammer".into(),
            account: true,
        }],
        ..Default::default()
    };
    let mut p = make("friends").unwrap();
    let shown = ids(&later, &*p);
    assert!(shown.contains(&"tab3".to_string()) && shown.contains(&"tab4".to_string()));
    assert!(shown.contains(&"add-friend".to_string()));
    // Typed and added by name; the online friend is listed first, and removed by id.
    event(
        &mut *p,
        ControlEvent::Edit {
            id: "entry".into(),
            text: " Bob ".into(),
        },
        &later,
    );
    assert_eq!(
        activate(&mut *p, "add-friend", &later),
        vec![PanelAction::Game(UiRequest::AddFriend {
            name: "Bob".into()
        })]
    );
    event(
        &mut *p,
        ControlEvent::Select {
            id: "friend-rows".into(),
            index: 0,
        },
        &later,
    );
    assert_eq!(
        activate(&mut *p, "remove-friend", &later),
        vec![PanelAction::Game(UiRequest::RemoveFriend {
            target: ObjectId(21)
        })]
    );
    // The Squelch page: an account squelch is lifted by its own message.
    activate(&mut *p, "tab4", &later);
    event(
        &mut *p,
        ControlEvent::Select {
            id: "squelch-rows".into(),
            index: 0,
        },
        &later,
    );
    assert_eq!(
        activate(&mut *p, "unsquelch", &later),
        vec![PanelAction::Game(UiRequest::ModifyAccountSquelch {
            add: false,
            name: "Spammer".into()
        })]
    );
}

#[test]
fn a_click_on_the_map_teleports_only_a_character_allowed_to() {
    let v = View::default();
    let mut p = make("map").unwrap();
    let press = ControlEvent::Pointer {
        x: 150,
        y: 25 + 166,
        pressed: true,
    };
    assert!(event(&mut *p, press.clone(), &v).is_empty());
    let pregame = PregameView::default();
    let keyboard = KeyboardState::default();
    let settings = ClassicSettings::default();
    let allowed = Context {
        game: &v,
        pregame: &pregame,
        keyboard: &keyboard,
        settings: &settings,
        map_teleport_allowed: true,
        classic: &ClassicState::default(),
    };
    assert!(matches!(
        p.event(press, &allowed).as_slice(),
        [PanelAction::Host(HostAction::MapTeleport { .. })]
    ));
}
/// The stretched page's list grows, and its buttons keep to the foot of the page.
#[test]
fn a_taller_options_page_shows_more_of_its_list_and_keeps_its_buttons_at_the_foot() {
    let page = crate::panels::OptionsPage::new(337);
    assert_eq!(page.view, rect(4, 16, 280, 264));
    assert_eq!(page.buttons_y, 297);
    let tall = crate::panels::OptionsPage::new(537);
    assert_eq!(tall.view.h, 264 + 200);
    assert_eq!(tall.buttons_y, 497);
    assert!(tall.max_scroll(1000) < page.max_scroll(1000));
}

/// Behaviour: map.regions.follow-world-profile
#[test]
fn map_rollovers_follow_profile_and_never_reuse_a_stale_hover() {
    use dereth_primitives::EraId;
    let mut v = View::default();
    let mut panel = make("map").unwrap();
    for (profile, count) in [
        (EraId::Eor, 53),
        (EraId::Infiltration, 50),
        (EraId::Eor, 53),
    ] {
        v.era = Some(dereth_client_contract::EraView {
            era: profile,
            ..Default::default()
        });
        assert_eq!(
            dereth_client_contract::panels::map::notes(profile).count(),
            count
        );
        for (x, y, name) in [
            (78, 71, "Fiun Outpost"),
            (72, 100, "Sanamar"),
            (63, 83, "Silyun"),
        ] {
            event(
                &mut *panel,
                ControlEvent::Pointer {
                    x,
                    y,
                    pressed: false,
                },
                &v,
            );
            let text = with_context(&v, |c| texts(&panel.frame(c)));
            assert_eq!(text.iter().any(|t| t == name), profile == EraId::Eor);
        }
        for (x, expected) in [
            (198, profile == EraId::Eor),
            (205, profile == EraId::Infiltration),
        ] {
            event(
                &mut *panel,
                ControlEvent::Pointer {
                    x,
                    y: 206,
                    pressed: false,
                },
                &v,
            );
            assert_eq!(
                with_context(&v, |c| texts(&panel.frame(c)))
                    .iter()
                    .any(|t| t == "Yanshi"),
                expected
            );
        }
    }
    event(
        &mut *panel,
        ControlEvent::Pointer {
            x: 78,
            y: 71,
            pressed: false,
        },
        &v,
    );
    assert!(with_context(&v, |c| texts(&panel.frame(c)))
        .iter()
        .any(|t| t == "Fiun Outpost"));
    v.era.as_mut().unwrap().era = EraId::Infiltration;
    assert!(!with_context(&v, |c| texts(&panel.frame(c)))
        .iter()
        .any(|t| t == "Fiun Outpost"));
}

/// Behaviour: map.coordinates.zero-has-no-hemisphere
#[test]
fn map_frame_formats_zero_and_tiny_signed_coordinates() {
    let mut v = View {
        coords: Some((0.0, -0.0)),
        ..Default::default()
    };
    let panel = make("map").unwrap();
    assert!(with_context(&v, |c| texts(&panel.frame(c))).contains(&"0.0, 0.0".into()));
    v.coords = Some((-0.001, 0.001));
    assert!(with_context(&v, |c| texts(&panel.frame(c))).contains(&"0.0S, 0.0E".into()));
}
