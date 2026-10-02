use super::*;
use dereth_primitives::num::to_i32;
pub fn make(id: &str) -> Option<Box<dyn Panel>> {
    Some(Box::new(Settings {
        tab: if id == "sound-graphics" { 2 } else { 0 },
        draft: None,
        saved: None,
        dirty: false,
        character: super::super::character_options::CharacterOptions::new(),
    }))
}
#[derive(Debug)]
struct Settings {
    tab: usize,
    draft: Option<ClassicSettings>,
    saved: Option<ClassicSettings>,
    dirty: bool,
    character: super::super::character_options::CharacterOptions,
}
use dereth_client_contract::options::classic;

/// The social window's page options on the Options page, `(id, caption, option, top)` each: the
/// Secure Trade page's only on a world with trade.
fn social_page_options(
    game: &dyn GameView,
) -> Vec<(&'static str, &'static str, &'static str, i32)> {
    let mut rows = vec![];
    if game.era().is_none_or(|e| e.features().trade) {
        rows.push(("show-trade", "Show Trade tab", classic::SHOW_TRADE_TAB));
    }
    rows.push((
        "show-friends",
        "Show Friends tab",
        classic::SHOW_FRIENDS_TAB,
    ));
    rows.push((
        "show-squelch",
        "Show Squelch tab",
        classic::SHOW_SQUELCH_TAB,
    ));
    rows.into_iter()
        .zip([260, 274, 288])
        .map(|((id, caption, name), y)| (id, caption, name, y))
        .collect()
}

/// The page's background, the side panel's height less the tabs.
fn background() -> PanelFrame {
    let height = crate::panels::side_height() - 25;
    let mut f = PanelFrame::new(300, height);
    crate::panels::sub_page_background(&mut f, height as i32);
    f
}
fn separator(f: &mut PanelFrame, y: i32) {
    f.image("060012C5", rect(4, y, 275, 8), true, false);
    f.image("060012C4", rect(279, y, 17, 8), false, false);
}
impl Settings {
    fn general(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = background();
        separator(&mut f, 303);
        // The six buttons of the classic page, closed up to make room for the way back to the
        // retail interface and the social window's page options.
        for (id, title, y) in [
            ("leave", "Leave World", 6),
            ("acceleration", "Setup 3D Acceleration", 42),
            ("keyboard", "Configure Keyboard", 78),
            ("help", "In-Game Help", 114),
            ("urgent", "Urgent Assistance", 150),
            ("abuse", "Report Abuse", 186),
            ("retail-interface", "Retail Interface", 222),
        ] {
            f.button(id, rect(30, y, 240, 34), title, true);
        }
        for (id, caption, name, y) in social_page_options(c.game) {
            f.check(
                id,
                rect(40, y, 230, 13),
                caption,
                classic::shown(name),
                true,
            )
            .font = "15-6".into();
        }
        centered(
            &mut f,
            rect(0, 314, 300, 18),
            concat!("Version ", env!("CARGO_PKG_VERSION")),
            "15-6",
        );
        f
    }
    fn sound(&self, c: &Context<'_>) -> PanelFrame {
        let s = self.draft.as_ref().unwrap_or(c.settings);
        let mut f = background();
        separator(&mut f, 162);
        separator(&mut f, 208);
        f.text_box(
            rect(25, 8, 95, 18),
            "Sound",
            "15-6",
            INK,
            TextAlign::Right,
            false,
            None,
        );
        let stereo = f.control(
            "stereo",
            rect(130, 8, 140, 18),
            ControlKind::Choice {
                options: vec!["Stereo".into(), "Mono".into()],
                selected: usize::from(!s.stereo),
            },
            true,
        );
        stereo.enabled = s.sound_available && s.effects;
        stereo.list_skin = Some(crate::panels::ListSkin::OPTIONS);
        for (id, txt, y, on) in [
            ("effects", "Sound Effects", 34, s.effects),
            ("ambient", "Ambient Sounds", 54, s.ambient),
            ("interface", "Interface Sound", 74, s.interface),
        ] {
            f.text_box(
                rect(0, y - 5, 110, 18),
                txt,
                "15-6",
                INK,
                TextAlign::Right,
                false,
                None,
            );
            f.check(id, rect(115, y, 13, 13), "", on, s.sound_available);
        }
        for (id, y, v, on) in [
            ("effects-volume", 34, s.effects_volume, s.effects),
            ("ambient-volume", 54, s.ambient_volume, s.ambient),
        ] {
            let slider = f.slider(
                id,
                rect(140, y, 120, 12),
                0,
                100,
                to_i32((v * 100.0).round()),
                1,
            );
            slider.enabled = s.sound_available && on;
            slider.images = Some(["06001285", "06001286", "06001286"].map(String::from));
        }
        f.text_box(
            rect(25, 99, 95, 18),
            "Graphics",
            "15-6",
            INK,
            TextAlign::Right,
            false,
            None,
        );
        f.control(
            "resolution",
            rect(130, 99, 140, 18),
            ControlKind::Choice {
                options: s
                    .resolutions
                    .iter()
                    .map(|(w, h)| format!("{w} x {h}"))
                    .collect(),
                selected: s.resolution,
            },
            true,
        )
        .list_skin = Some(crate::panels::ListSkin::OPTIONS);
        for (id, text, y, v) in [
            ("brightness", "Brightness", 124, s.brightness),
            ("stiffness", "Camera Stiffness", 144, s.camera_stiffness),
        ] {
            f.text_box(
                rect(5, y - 5, 115, 18),
                text,
                "15-6",
                INK,
                TextAlign::Right,
                false,
                None,
            );
            f.slider(
                id,
                rect(130, y, 120, 12),
                0,
                100,
                to_i32((v * 100.0).round()),
                1,
            )
            .images = Some(["06001285", "06001286", "06001286"].map(String::from));
        }
        f.text_box(
            rect(0, 171, 150, 18),
            "Graphics Performance",
            "15-6",
            INK,
            TextAlign::Right,
            false,
            None,
        );
        f.slider(
            "performance",
            rect(160, 174, 120, 12),
            0,
            100,
            to_i32((s.performance * 100.0).round()),
            1,
        )
        .images = Some(["06001285", "06001286", "06001286"].map(String::from));
        f.check(
            "auto-degrade",
            rect(30, 188, 130, 13),
            "Auto-Degrade",
            s.auto_degrade,
            true,
        );
        label(&mut f, rect(160, 188, 50, 18), "Detail", "15-6");
        label(&mut f, rect(210, 188, 70, 18), "Speed", "15-6");
        // Only the settings this client's renderer carries out are offered: the environment's
        // detail texture, and two texture sizes (the landscape's, and every other image's).
        label(&mut f, rect(10, 216, 89, 18), "Detail Texture", "15-6");
        f.check(
            "environment-detail",
            rect(271, 218, 13, 13),
            "",
            s.environment_detail,
            s.detail_available,
        );
        label(&mut f, rect(190, 216, 81, 18), "Environment", "15-6");
        for (i, title, x, y) in [(0, "Landscape", 10, 242), (2, "Object", 155, 242)] {
            f.text_box(
                rect(x, y, 70, 18),
                title,
                "15-6",
                INK,
                TextAlign::Right,
                false,
                None,
            );
            f.control(
                format!("texture{i}"),
                rect(x + 75, y, 60, 18),
                ControlKind::Choice {
                    options: ["Full", "1/2", "1/4", "1/8"].map(String::from).to_vec(),
                    selected: s.texture_levels[i] as usize,
                },
                true,
            )
            .list_skin = Some(crate::panels::ListSkin::OPTIONS);
        }
        // Full screen is a borderless window over the whole monitor; the page offers it beside
        // the window sizes (the era's game chose it outside the game).
        f.check(
            "full-screen",
            rect(30, 270, 150, 13),
            "Full Screen",
            s.full_screen,
            true,
        );
        for (id, text, x) in [
            ("apply", "Apply", 25),
            ("reset", "Reset", 110),
            ("defaults", "Defaults", 195),
        ] {
            f.button(
                id,
                rect(x, 296, 80, 36),
                text,
                self.dirty || id == "defaults",
            );
        }
        f
    }
}
impl Panel for Settings {
    fn id(&self) -> &'static str {
        "options"
    }
    fn frame(&self, c: &Context<'_>) -> PanelFrame {
        let mut f = translated(
            match self.tab {
                2 => self.sound(c),
                1 => self.character.frame(c),
                _ => self.general(c),
            },
            25,
            crate::panels::side_height(),
        );
        for (id, title, x, w) in [
            ("general", "Options", 0, 72),
            ("character", "Character", 72, 82),
            ("sound", "Sound/Graphics", 154, 122),
        ] {
            let b = f.button(id, rect(x, 0, w, 25), title, true);
            let selected = (id == "sound" && self.tab == 2)
                || (id == "general" && self.tab == 0)
                || (id == "character" && self.tab == 1);
            b.images = Some(
                [
                    if selected { "0600128F" } else { "06001291" },
                    "0600128F",
                    "06001291",
                ]
                .map(String::from),
            );
            b.endcaps = Some(
                [
                    if selected { "06001290" } else { "06001292" },
                    "06001290",
                    "06001292",
                ]
                .map(String::from),
            );
        }
        image_button(
            &mut f,
            "close",
            rect(276, 0, 24, 25),
            [0x06001283, 0x06001282, 0x06001283],
            true,
        );
        f
    }
    fn event(&mut self, e: ControlEvent, c: &Context<'_>) -> Vec<PanelAction> {
        if self.tab == 1 {
            let host_event = matches!(&e,ControlEvent::Activate(id) if matches!(id.as_str(),"general"|"character"|"sound"|"close"));
            if !host_event {
                let e = match e {
                    ControlEvent::Pointer { x, y, pressed } => ControlEvent::Pointer {
                        x,
                        y: y - 25,
                        pressed,
                    },
                    e => e,
                };
                return self.character.event(e, c);
            }
        }
        if self.draft.is_none() {
            self.draft = Some(c.settings.clone());
            self.saved = Some(c.settings.clone());
        }
        match e{
            ControlEvent::Activate(id)=>match id.as_str(){
                "general"=>{self.tab=0;vec![]},"sound"=>{self.tab=2;vec![]},"character"=>{self.tab=1;self.character.event(ControlEvent::Tick,c)},
                "close"=>vec![PanelAction::Close],"leave"=>vec![PanelAction::Confirm{id:"leave-world".into(),text:"\n\nThis will exit your character from the game world.\n\nAre you sure?".into(),accept:request(UiRequest::EndCharacterSession{ask:false})}],
                "keyboard"=>vec![PanelAction::Confirm{id:"configure-keyboard".into(),text:"\n\nTo configure your keyboard, you need to leave the world.\nProceed?".into(),accept:vec![PanelAction::Game(UiRequest::EndCharacterSession{ask:false}),PanelAction::Open("keyboard".into())]}],
                "acceleration"=>vec![PanelAction::Confirm{id:"acceleration-info".into(),text:"\nTo change your 3D acceleration settings, exit the game. Relaunch the game from your desktop icon or Start menu, then choose Settings.".into(),accept:vec![]}],
                "help"=>vec![PanelAction::Host(HostAction::LegacyHelp(51))],
                "retail-interface"=>{
                    use dereth_client_contract::options::interface::{Interface, INTERFACE};
                    vec![PanelAction::Game(UiRequest::SetPreference(INTERFACE, dereth_client_contract::PrefValue::Int(Interface::Retail.value())))]
                },"urgent"=>vec![PanelAction::Open("urgent-assistance".into())],"abuse"=>vec![PanelAction::Open("abuse".into())],
                "apply"=>{self.dirty=false;let s=self.draft.clone().unwrap();self.saved=Some(s.clone());vec![PanelAction::Host(HostAction::ApplyClassicSettings(s))]},
                "reset"=>{self.dirty=false;self.draft=self.saved.clone();vec![PanelAction::Host(HostAction::ResetClassicSettings)]},
                "defaults"=>{
                    let s=self.draft.as_mut().unwrap();s.effects=s.sound_available;s.ambient=s.sound_available;s.interface=s.sound_available;s.stereo=true;
                    s.effects_volume=1.0;s.ambient_volume=1.0;s.auto_degrade=true;s.performance=0.5;s.brightness=0.5;s.camera_stiffness=0.23;s.full_screen=false;
                    if let Some(i)=s.resolutions.iter().position(|r|*r==(1024,768)){s.resolution=i;}
                    if !s.detail_available{s.landscape_detail=false;s.environment_detail=false;}
                    self.dirty=true;vec![PanelAction::Host(HostAction::DefaultClassicSettings(s.clone()))]
                },_=>vec![]},
            // The social window's page options are kept at once, as the interface choice is.
            ControlEvent::Check { id, checked } if id.starts_with("show-") => {
                social_page_options(c.game)
                    .into_iter()
                    .find(|row| row.0 == id)
                    .map(|row| {
                        vec![PanelAction::Game(UiRequest::SetPreference(
                            row.2,
                            dereth_client_contract::PrefValue::Bool(checked),
                        ))]
                    })
                    .unwrap_or_default()
            }
            ControlEvent::Check{id,checked}=>{
                let s=self.draft.as_mut().unwrap();match id.as_str(){"effects"=>s.effects=checked,"ambient"=>s.ambient=checked,"interface"=>s.interface=checked,"auto-degrade"=>s.auto_degrade=checked,"landscape-detail"=>s.landscape_detail=checked,"environment-detail"=>s.environment_detail=checked,"full-screen"=>s.full_screen=checked,_=>return vec![]};self.dirty=true;vec![]
            },
            ControlEvent::Value{id,value}=>{
                let s=self.draft.as_mut().unwrap();let v=value.clamp(0,100)as f32/100.0;
                match id.as_str(){"effects-volume"=>s.effects_volume=v,"ambient-volume"=>s.ambient_volume=v,"brightness"=>s.brightness=v,"stiffness"=>s.camera_stiffness=v,"performance"=>s.performance=v,_=>return vec![]};self.dirty=true;
                if matches!(id.as_str(),"brightness"|"stiffness"|"performance"){vec![PanelAction::Host(HostAction::PreviewClassicSettings(s.clone()))]}else{vec![]}
            },
            ControlEvent::Select{id,index}=>{
                let s=self.draft.as_mut().unwrap();match id.as_str(){"stereo"=>s.stereo=index==0,"resolution" if index<s.resolutions.len()=>s.resolution=index,_=>{if let Some(i)=id.strip_prefix("texture").and_then(|s|s.parse::<usize>().ok()).filter(|i|*i<4){s.texture_levels[i]=u8::try_from(index.min(3)).unwrap_or(3);}else{return vec![];}}}self.dirty=true;vec![]
            },_=>vec![],
        }
    }
}
