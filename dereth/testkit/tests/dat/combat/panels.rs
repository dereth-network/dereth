use std::rc::Rc;
use std::sync::Arc;

use dereth_primitives::{AssetSource, DataId, ObjectId};
use dereth_ui::{Delivery, ElemHandle, RecordingDrawBackend, Screen as _, StateId, UiSystem};
use dereth_ui_screens::hud::combat_window::CombatWindow;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, Vital};

/// The shipped `classic_gameplay` tree, built by the screen's own startup.
pub fn gameplay() -> (UiSystem, GamePlayScreen) {
    let store = Arc::new(dereth_dat::testing::open_store_or_fail());
    let master_id = DataId(0x3900_0001);
    let master = <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(
        master_id,
        &store.read(master_id).expect("the shipped property table"),
    )
    .expect("it decodes");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    ui.strings = Some(Rc::new(
        dereth_client_shell::ui_draw::DatStringResolver::new(Arc::clone(&store)),
    ));
    ui.fonts = Some(Rc::new(dereth_client_shell::ui_draw::DatFontProvider::new(
        Arc::clone(&store),
    )));
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let assets = Rc::new(store);
    let resolver = Rc::new(
        dereth_ui::framework::DidMapperResolver::load_via_master(assets.as_ref())
            .expect("the shipped mapper"),
    );
    dereth_ui_screens::env::install(&mut ui, assets, resolver);
    let mut screen = GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen builds from the shipped layout");
    // The screen's own start-up requests and notices belong to the screen and not to any
    // scenario; a bench that left them queued would hand the first frame somebody else's work.
    ui.requests.clear();
    ui.notice_inbox.clear();
    (ui, screen)
}

/// The same, with the combat window bound off it.
pub fn a_combat_window() -> (UiSystem, GamePlayScreen, CombatWindow) {
    let (mut ui, mut screen) = gameplay();
    pump(&mut ui, &mut screen);
    let mut combat = CombatWindow::default();
    combat.post_init(&mut ui, screen.root().expect("the gameplay root"));
    assert_eq!(
        combat.failures, 0,
        "every child the window looks up is in the shipped tree"
    );
    (ui, screen, combat)
}

/// Deliver what the tree has queued until it settles.
pub fn pump(ui: &mut UiSystem, screen: &mut GamePlayScreen) {
    for _ in 0..16 {
        let batch = ui.drain_outbox();
        if batch.is_empty() {
            return;
        }
        for d in batch {
            if let Delivery::Element { msg, .. } = d {
                screen.on_element_message(&mut dereth_ui::framework::ScreenCx::new(ui), &msg);
            }
        }
    }
    panic!("the tree's own messages did not settle");
}

/// Put the window up, as a combat mode arriving does.
pub fn show_combat(ui: &mut UiSystem, screen: &mut GamePlayScreen) {
    screen.on_set_combat_mode(ui, 4, false, 0);
    pump(ui, screen);
}

pub fn text(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .expect("a text element")
        .glyphs
        .inq_text(false)
}

/// What the tree really put in the draw list this pass.
pub fn drawn(ui: &mut UiSystem) -> RecordingDrawBackend {
    let mut back = RecordingDrawBackend::default();
    ui.draw(&mut back);
    back
}

/// The glyphs one element really contributed to it.
pub fn drawn_text(back: &RecordingDrawBackend, h: ElemHandle) -> String {
    String::from_utf16(
        &back
            .calls
            .iter()
            .filter(|c| c.who == h)
            .flat_map(|c| c.glyphs.iter().map(|g| g.ch))
            .collect::<Vec<_>>(),
    )
    .expect("the client's own glyphs are text")
}

/// Whether every height button is drawing the picture a chosen or an unchosen one gets.
pub fn height_art_holds(ui: &mut UiSystem, combat: &CombatWindow, selected: u32) -> bool {
    let back = drawn(ui);
    let mut holds = true;
    for &(h, height) in &combat.height_buttons {
        let chosen = height == selected;
        let state = StateId(if chosen { 6 } else { 1 });
        let n = ui.node(h).expect("alive");
        // The two pictures are the shipped button's own state media, read back off the
        // element rather than inferred.
        let want = DataId(if chosen { 0x0600_4D1E } else { 0x0600_4D1C });
        let declared = n.desc.access_state(state).is_some_and(|s| {
            s.media.iter().any(|m| {
                matches!(m.fields, dereth_assets::ui::MediaFields::Image { file, .. }
                        if file == want)
            })
        });
        let images: Vec<DataId> = back
            .calls
            .iter()
            .filter(|c| c.who == h)
            .filter_map(|c| c.image)
            .collect();
        holds &= n.state == state
            && n.merged_properties().get_bool(0x0E).unwrap_or(false) == chosen
            && declared
            && images == vec![want];
    }
    holds
}

/// The three numbers a vitals row is drawn from.
#[derive(Debug)]
pub struct Vitals(pub [(u32, u32); 3]);

impl GameView for Vitals {
    fn player(&self) -> Option<ObjectId> {
        Some(ObjectId(0x5066_0042))
    }
    fn vital(&self, _: ObjectId, vital: Vital) -> Option<(u32, u32)> {
        Some(
            self.0[match vital {
                Vital::Health => 0,
                Vital::Stamina => 1,
                Vital::Mana => 2,
            }],
        )
    }
}
