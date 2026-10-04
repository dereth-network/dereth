//! The startup screen and its connection and update progress bars.
use super::*;
use crate::int::u32_from;
use dereth_client_contract::pregame::{DddEvent, PregameView};
use dereth_primitives::num::to_i32_f64;
#[derive(Debug)]
pub(super) struct Startup {
    expected: u64,
    downloaded: u64,
    complete: bool,
    frame: usize,
    last_animation: dereth_primitives::LocalTime,
}
impl Startup {
    pub fn new(now: dereth_primitives::LocalTime) -> Self {
        Self {
            expected: 0,
            downloaded: 0,
            complete: false,
            frame: 0,
            last_animation: now,
        }
    }
}
impl Startup {
    pub fn tick(&mut self, view: &PregameView, now: dereth_primitives::LocalTime) -> bool {
        for event in &view.ddd {
            match event {
                DddEvent::PatchtimeInterrogation => {
                    self.expected = 0;
                    self.downloaded = 0;
                    self.complete = false;
                }
                DddEvent::PatchtimePending { .. } => {}
                DddEvent::PatchtimeBegin { expected } => {
                    self.expected = *expected;
                    self.downloaded = 0;
                    self.complete = false;
                }
                DddEvent::DataDownloaded { bytes } => {
                    self.downloaded = self.downloaded.saturating_add(*bytes);
                }
                DddEvent::PatchtimeEnd => self.complete = true,
            }
        }
        self.complete |= view.patch_finished;
        if crate::clock::seconds(now, self.last_animation) >= 0.067 {
            self.frame = (self.frame + 1) % 15;
            self.last_animation = now;
        }
        view.error.is_none()
            && (!view.has_packet_controller
                || (view.connected && self.complete && view.received_set))
    }
    fn update_fraction(&self) -> f64 {
        if self.complete {
            1.0
        } else if self.expected > 0 {
            (self.downloaded as f64 / self.expected as f64).min(1.0)
        } else {
            0.0
        }
    }
    fn bar(&self, f: &mut PanelFrame, x: i32, fraction: f64) {
        let background = if self.frame < 10 {
            0x06001966 + u32_from(self.frame)
        } else {
            0x06001975 + u32_from(self.frame - 10)
        };
        let foreground = if self.frame < 10 {
            0x0600195c + u32_from(self.frame)
        } else {
            0x06001970 + u32_from(self.frame - 10)
        };
        // Two native-size images, the filled one clipped at trunc(width*fraction)-1.
        let split = to_i32_f64(354.0 * fraction.clamp(0.0, 1.0)) - 1;
        for (did, left, right) in [
            (background, split.max(0), 354),
            (foreground, 0, split.max(0)),
        ] {
            if right <= left {
                continue;
            }
            f.image(&format!("{did:08X}"), rect(x, 514, 325, 52), false, false);
            if let Some(crate::Command::Image { clip, .. }) = f.screen.commands.last_mut() {
                *clip = Some([x + left, 514, x + right, 560]);
            }
        }
    }
    pub fn paint(&self, view: &PregameView) -> PanelFrame {
        let mut f = PanelFrame::new(800, 600);
        f.fill(rect(0, 0, 800, 600), 0xff000000);
        f.image("06001343", rect(0, 30, 800, 482), false, false);
        f.text_box(rect(0,467,800,45),
            "Copyright 1996-2004 Turbine Entertainment Software Corporation. All rights reserved.\nThis program is protected by U.S. and International copyright laws as described in Help/About Asheron's Call.",
            "14-5",COLOR,TextAlign::Left,true,None);
        // The shared view exposes connection completion, not the classic transport's step/total.
        let connected = view.connected || !view.has_packet_controller;
        self.bar(&mut f, 28, if connected { 1.0 } else { 0.0 });
        self.bar(&mut f, 408, self.update_fraction());
        text(
            &mut f,
            rect(28, 560, 354, 20),
            if connected {
                "Connected!"
            } else {
                "Connect progress"
            },
        );
        let fraction = self.update_fraction();
        let update = if fraction >= 1.0 {
            "Updated!".to_owned()
        } else if fraction > 0.0 {
            format!(
                "Updating... {}% done",
                u32::try_from(to_i32_f64(fraction * 100.0)).unwrap_or(0)
            )
        } else {
            "Update progress".to_owned()
        };
        f.text_box(
            rect(408, 560, 354, 20),
            update,
            "16-7",
            COLOR,
            TextAlign::Center,
            false,
            None,
        );
        let c = f.button("cancel", rect(696, 30, 99, 45), "Cancel", true);
        c.images = Some(["060016C7".into(), "060016C8".into(), "060016C7".into()]);
        f
    }
}
#[cfg(test)]
mod tests {
    //! Behaviour: none (classic front-end adapter; no retail behaviour claim).
    use super::*;
    #[test]
    fn supplied_ticks_advance_one_animation_frame_from_the_constructor_time() {
        use dereth_primitives::LocalTime;
        let mut startup = Startup::new(LocalTime(100.0));
        let view = PregameView::default();
        for (time, frame) in [
            (100.0, 0),
            (100.066, 0),
            (100.08, 1),
            (101.0, 2),
            (99.0, 2),
            (101.01, 2),
        ] {
            startup.tick(&view, LocalTime(time));
            assert_eq!(startup.frame, frame, "time={time}");
            assert!(startup.paint(&view).screen.commands.iter().any(|command| matches!(command,
                crate::Command::Image { did, .. } if did == &format!("{:08X}", 0x0600195c + frame)
            )));
        }
    }
    #[test]
    fn startup_waits_for_patch_completion_and_character_set_and_tracks_download_bytes() {
        let mut s = Startup::new(dereth_primitives::LocalTime(0.0));
        let mut v = PregameView {
            has_packet_controller: true,
            connected: true,
            received_set: true,
            ddd: vec![
                DddEvent::PatchtimeBegin { expected: 200 },
                DddEvent::DataDownloaded { bytes: 50 },
            ],
            ..Default::default()
        };
        assert!(!s.tick(&v, dereth_primitives::LocalTime(0.0)));
        assert_eq!(s.update_fraction(), 0.25);
        let f = s.paint(&v);
        assert!(f
            .screen
            .commands
            .iter()
            .any(|c| matches!(c,crate::Command::TextBox{text,..} if text=="Updating... 25% done")));
        v.ddd = vec![DddEvent::DataDownloaded { bytes: 150 }];
        assert!(!s.tick(&v, dereth_primitives::LocalTime(0.0))); // Receiving all bytes is not the completion notice.
        v.ddd = vec![DddEvent::PatchtimeEnd];
        v.received_set = false;
        assert!(!s.tick(&v, dereth_primitives::LocalTime(0.0)));
        v.ddd.clear();
        v.received_set = true;
        assert!(s.tick(&v, dereth_primitives::LocalTime(0.0)));
        assert_eq!(s.update_fraction(), 1.0);
    }
    #[test]
    fn startup_without_packet_controller_advances_without_invented_network_progress() {
        assert!(Startup::new(dereth_primitives::LocalTime(0.0))
            .tick(&PregameView::default(), dereth_primitives::LocalTime(0.0)));
    }
}
