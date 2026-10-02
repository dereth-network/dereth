use super::*;
use crate::int::i32_from;
#[derive(Debug)]
pub struct GameCenter {
    height: u32,
}
impl Default for GameCenter {
    fn default() -> Self {
        Self { height: 413 }
    }
}
impl Panel for GameCenter {
    fn resize(&mut self, _: u32, height: u32) {
        self.height = height;
    }
    fn id(&self) -> &'static str {
        "game-center"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = tiled(300, self.height, "06001398");
        header(&mut f, "Game Center");
        for (did, r) in [
            ("06001FD6", rect(10, 33, 280, 12)),
            ("06001FD8", rect(10, 45, 12, 256)),
            ("06001FD9", rect(278, 45, 12, 256)),
            ("06001FD7", rect(10, 301, 280, 12)),
            ("06001FD5", rect(22, 45, 256, 256)),
        ] {
            f.image(did, r, true, false);
        }
        if let Some(g) = c.game.minigame() {
            // The array follows the board's piece-art slots, including the
            // bishop/knight interchange in the numeric DID sequence.
            let art = [
                0x06001FC9, 0x06001FCC, 0x06001FCA, 0x06001FCB, 0x06001FCD, 0x06001FCE, 0x06001FCF,
                0x06001FD2, 0x06001FD0, 0x06001FD1, 0x06001FD3, 0x06001FD4,
            ];
            for (i, p) in g.piece_slots.iter().enumerate() {
                let r = rect(22 + i32_from(i % 8) * 32, 45 + i32_from(i / 8) * 32, 32, 32);
                if let Some(slot) = p {
                    if let Some(did) = art.get(*slot as usize) {
                        f.image(&format!("{did:08X}"), r, false, true);
                    }
                }
                if g.selected_cell == Some(i) {
                    f.image("06000F7E", r, false, true);
                }
                // Hit regions have no renderer-owned art; board art stays above.
                f.control(
                    format!("cell{i}"),
                    r,
                    ControlKind::HitList {
                        row_count: 1,
                        row_height: 32,
                        selected: None,
                        offset: 0,
                    },
                    true,
                )
                .paint = false;
            }
            for (id, text, x, enabled) in [
                ("resign", "Resign", 8, true),
                ("stalemate", "Stalemate", 202, true),
            ] {
                let b = f.button(id, rect(x, 321, 89, 22), text, enabled);
                b.images = Some(
                    [
                        if id == "stalemate" && g.stalemate {
                            "06002346"
                        } else {
                            "06002344"
                        },
                        "06002345",
                        "06002346",
                    ]
                    .map(String::from),
                );
                b.font = "15-6".into();
            }
        }
        // The status text has zero margins. The classic board has a Pass control that is never
        // shown, so none is drawn here.
        f.text_box(
            rect(8, 351, 284, 54),
            if self.height >= 413 {
                &c.classic.game_status
            } else {
                ""
            },
            "16-7",
            INK,
            TextAlign::Left,
            true,
            None,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, _: &Context<'_>) -> Vec<PanelAction> {
        match e{
        ControlEvent::Select{id,..}if id.starts_with("cell")=>id[4..].parse::<usize>().ok().filter(|i|*i<64).map(|i|request(UiRequest::MiniGameBoardPress(i))).unwrap_or_default(),
        ControlEvent::Activate(id)=>match id.as_str(){
            "close"=>vec![PanelAction::Close],
            "resign"=>vec![PanelAction::Confirm{id:"resign".into(),text:"\n\nIf a game is in progress, resigning will be recorded as your loss. Are you sure you want to resign? (Default is no.)".into(),accept:request(UiRequest::MiniGameQuitAnswer(true))}],
            "pass"=>request(UiRequest::MiniGameButton(0x10000176)),"stalemate"=>request(UiRequest::MiniGameButton(0x10000177)),_=>vec![]},_=>vec![],
    }
    }
}
