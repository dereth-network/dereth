//! Vendor drag gestures through the desktop's actual pointer/drop route.
use super::*;
use dereth_client_contract::view::ShopView;
use dereth_primitives::ObjectId;

#[derive(Debug)]
struct Shop;
impl GameView for Shop {
    fn shop(&self) -> ShopView {
        ShopView {
            open: true,
            vendor: Some(ObjectId(9)),
            ..Default::default()
        }
    }
    fn vendor_drag_item_accepted(&self, _: ObjectId) -> bool {
        true
    }
}

/// Behaviour: vendor.drag.classic-opens-selling-before-drop
#[test]
fn dragging_over_the_vendor_opens_selling_before_the_item_is_dropped() {
    let c = Context {
        resources: &crate::resources::Resources::default(),
        layout: crate::panels::Layout::default(),
        now: dereth_primitives::LocalTime(0.0),
        game: &Shop,
        pregame: &PregameView::default(),
        keyboard: &KeyboardState::default(),
        settings: &ClassicSettings::default(),
        classic: &ClassicState::default(),
        map_teleport_allowed: false,
    };
    let mut d = Desktop::new(crate::panels::factory, (800, 600));
    let token = d.open("vendor", &c).unwrap();
    d.refresh(&c);
    let w = d.windows.iter().find(|w| w.token == token).unwrap();
    let (x, y) = (w.x + 25, w.y + 60);
    let overlay = Window {
        token: 999,
        key: "overlay".into(),
        panel: crate::panels::factory("vendor", dereth_primitives::LocalTime(0.0), c.resources)
            .unwrap(),
        controls: ControlHost::default(),
        frame: PanelFrame::new(100, 100),
        x: x - 10,
        y: y - 10,
    };
    d.windows.push(overlay);
    d.drag_payload = Some(DragPayload::Object(ObjectId(42)));
    d.input(Input::PointerMove { x, y }, &c);
    assert!(
        !d.windows
            .iter()
            .find(|w| w.token == token)
            .unwrap()
            .frame
            .controls
            .iter()
            .any(|b| matches!(&b.kind,ControlKind::Button{caption} if caption=="Sell Item")),
        "an overlapping window owns the hover"
    );
    d.windows.retain(|w| w.token != 999);
    d.input(Input::PointerMove { x, y }, &c);
    let w = d.windows.iter().find(|w| w.token == token).unwrap();
    assert!(w
        .frame
        .controls
        .iter()
        .any(|b| matches!(&b.kind, ControlKind::Button { caption } if caption == "Sell Item")));
    assert!(d.requests.is_empty(), "hover alone does not add or sell");
    d.input(Input::PointerUp { x, y }, &c);
    assert_eq!(
        d.requests,
        [UiRequest::VendorAddToSell { item: ObjectId(42) }]
    );
}
