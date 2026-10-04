//! The Examine window and its item, creature, character and spell pages. Content always comes
//! from an appraisal delivery.
use super::super::*;
use super::common::*;
use crate::int::i32_from;
use dereth_client_contract::examination::{appraisal_pane, inscription_editable, AppraisalPane};
use dereth_client_contract::view::AppraisalView;
use dereth_presentation::spell;
#[derive(Debug, Default)]
pub struct Examine {
    object: Option<ObjectId>,
    spell: Option<u32>,
    inscription: Option<String>,
    inscription_dirty: bool,
    inscription_focus: bool,
    scroll: i32,
}
fn separator(f: &mut PanelFrame, y: i32) {
    image(f, 0x060012c5, rect(4, y, 275, 8), None, true, false);
    image(f, 0x060012c4, rect(279, y, 17, 8), None, false, false);
}
fn val<T: ToString>(v: Option<T>) -> String {
    v.map(|v| v.to_string()).unwrap_or_else(|| "???".into())
}
impl Examine {
    fn commit_inscription(&mut self, ctx: &Context<'_>) -> Vec<PanelAction> {
        if !std::mem::take(&mut self.inscription_dirty) {
            return vec![];
        }
        let Some(object) = self.object.or_else(|| ctx.game.selected_object()) else {
            return vec![];
        };
        let Some(a) = ctx
            .game
            .appraisal(object)
            .filter(|a| can_inscribe(object, a, ctx.game))
        else {
            return vec![];
        };
        let text = self.inscription.get_or_insert_default();
        if text == " " || text == "\n" {
            text.clear();
        }
        if text.is_empty() && a.inscription.as_deref().unwrap_or("").is_empty() {
            return vec![];
        }
        vec![PanelAction::Game(UiRequest::SetInscription {
            object,
            text: text.clone(),
        })]
    }
}
impl Panel for Examine {
    fn id(&self) -> &'static str {
        if self.spell.is_some() {
            "examine-spell"
        } else {
            "examine"
        }
    }
    fn set_object(&mut self, id: ObjectId) {
        self.object = Some(id);
        self.spell = None;
        self.inscription = None;
        self.inscription_dirty = false;
        self.inscription_focus = false;
        self.scroll = 0;
    }
    fn set_spell(&mut self, id: u32) {
        self.spell = Some(id);
        self.object = None;
        self.inscription = None;
        self.inscription_dirty = false;
        self.inscription_focus = false;
        self.scroll = 0;
    }
    fn frame(&self, ctx: &Context<'_>) -> PanelFrame {
        let g = ctx.game;
        let height = crate::panels::side_height();
        let mut f = PanelFrame::new(300, height);
        panel_backdrop(&mut f, height as i32);
        image(&mut f, 0x06001291, rect(0, 0, 276, 25), None, true, false);
        close(&mut f, 0x06001283, 0x06001282);
        if let Some(id) = self.spell {
            let data = g.spell_examine(id);
            let name = g
                .spell(id)
                .map(|s| s.name)
                .or_else(|| data.as_ref().map(|s| s.name.clone()))
                .unwrap_or_default();
            text(
                &mut f,
                rect(2, 2, 272, 20),
                name,
                "16-7",
                CREAM,
                1,
                false,
                None,
            );
            if let Some(s) = data {
                if let Some(entry) = g.spell(id) {
                    spell_icon(
                        &mut f,
                        s.icon,
                        entry.icon_power,
                        entry.bitfield,
                        rect(245, 30, 32, 32),
                        None,
                    );
                }
                text(
                    &mut f,
                    rect(9, 35, 235, 20),
                    format!("School: {}", spell::school_name(s.school)),
                    "16-7",
                    CREAM,
                    1,
                    false,
                    None,
                );
                separator(&mut f, 65);
                separator(&mut f, 113);
                separator(&mut f, 291);
                text(
                    &mut f,
                    rect(9, 76, 137, 20),
                    spell::mana_text(s.base_mana, s.mana_mod),
                    "15-6",
                    CREAM,
                    0,
                    false,
                    None,
                );
                if let Some(duration) = spell::duration_text(s.duration) {
                    text(
                        &mut f,
                        rect(150, 76, 146, 20),
                        duration,
                        "15-6",
                        CREAM,
                        2,
                        false,
                        None,
                    );
                }
                if let Some(range) = spell::range_text(s.range) {
                    text(
                        &mut f,
                        rect(9, 93, 137, 20),
                        range,
                        "15-6",
                        CREAM,
                        0,
                        false,
                        None,
                    );
                }
                rich_scroll(
                    &mut f,
                    rect(9, 121, 266, 170),
                    vec![crate::TextRun {
                        text: spell::display_text(
                            &s.description,
                            &spell::surviving_components(&s.components)
                                .into_iter()
                                .map(|(c, _)| c.name)
                                .collect::<Vec<_>>(),
                        ),
                        color: CREAM,
                    }],
                    "15-6",
                    self.scroll,
                );
                text(
                    &mut f,
                    rect(4, 299, 292, 20),
                    "FORMULA",
                    "15-6",
                    CREAM,
                    1,
                    false,
                    None,
                );
                let x = 150 - i32_from(s.components.len()) * 16;
                for (i, c) in s.components.iter().enumerate() {
                    if let Some(c) = c {
                        if let Some(icon) = c.icon {
                            component_icon(
                                &mut f,
                                icon.0,
                                rect(x + i32_from(i) * 32, 319, 32, 32),
                                None,
                            );
                        }
                        f.button(
                            format!("component:{}", c.scid),
                            rect(x + i32_from(i) * 32, 319, 32, 32),
                            "",
                            true,
                        )
                        .paint = false;
                    }
                }
            }
            return f;
        }
        let Some(object) = self.object.or_else(|| g.selected_object()) else {
            return f;
        };
        let appraisal = g.appraisal(object);
        let object_name = g.name(object).unwrap_or("");
        let title = appraisal
            .as_ref()
            .filter(|a| a.character_title)
            .and_then(|a| a.allegiance_title.as_ref())
            .map(|t| format!("{t} {object_name}"))
            .unwrap_or_else(|| object_name.into());
        text(
            &mut f,
            rect(2, 2, 272, 20),
            title,
            "16-7",
            CREAM,
            1,
            false,
            None,
        );
        let Some(a) = appraisal else {
            text(
                &mut f,
                rect(9, 85, 266, 192),
                "Receiving information...",
                "15-6",
                CREAM,
                0,
                true,
                None,
            );
            return f;
        };
        let pane = appraisal_pane(a.creature, a.template, a.character_title);
        if pane != AppraisalPane::Item {
            // A creature with a face (a character, or a person of the world such as a shopkeeper)
            // shows the face; any other creature its type's icon.
            let face = ctx.classic.portraits.get(&object);
            if pane == AppraisalPane::Creature {
                if let Some(portrait) = face {
                    draw_portrait(&mut f, portrait);
                    image(&mut f, 0x060012c6, rect(172, 25, 7, 60), None, true, false);
                } else {
                    tile_icon(&mut f, g, object, 190, 40);
                }
            }
            separator(&mut f, 85);
            separator(&mut f, 291);
            image(&mut f, 0x060012c6, rect(230, 25, 7, 60), None, true, false);
            text(
                &mut f,
                rect(232, 26, 66, 15),
                "character",
                "15-6",
                CREAM,
                1,
                false,
                None,
            );
            text(
                &mut f,
                rect(232, 40, 66, 15),
                "level",
                "15-6",
                CREAM,
                1,
                false,
                None,
            );
            text(
                &mut f,
                rect(232, 52, 66, 33),
                val(a.level),
                "35-16",
                CREAM,
                1,
                false,
                None,
            );
            if pane == AppraisalPane::Character {
                if let Some(portrait) = ctx.classic.portraits.get(&object) {
                    draw_portrait(&mut f, portrait);
                }
                image(&mut f, 0x060012c6, rect(172, 25, 7, 60), None, true, false);
                // The character's lines sit two pixels inside their box.
                rich_scroll_inset(
                    &mut f,
                    rect(0, 299, 281, crate::panels::side_height() as i32 - 303),
                    2,
                    super::appraisal::character(&a, g.name(object).unwrap_or("")),
                    "15-6",
                    self.scroll,
                );
                text(
                    &mut f,
                    rect(2, 29, 168, 17),
                    a.gender_heritage_display
                        .clone()
                        .unwrap_or_else(|| "Unknown Gender".into()),
                    "15-6",
                    CREAM,
                    1,
                    false,
                    None,
                );
                text(
                    &mut f,
                    rect(2, 46, 168, 17),
                    a.profession
                        .clone()
                        .unwrap_or_else(|| "Unknown Profession".into()),
                    "15-6",
                    CREAM,
                    1,
                    false,
                    None,
                );
                text(
                    &mut f,
                    rect(2, 63, 168, 17),
                    if a.weenie_is_pk {
                        "Player Killer"
                    } else if a.weenie_is_pk_lite {
                        "Player Killer Lite"
                    } else {
                        "Non-Player Killer"
                    },
                    "15-6",
                    CREAM,
                    1,
                    false,
                    None,
                );
            } else {
                text(
                    &mut f,
                    rect(2, 52, if face.is_some() { 168 } else { 176 }, 20),
                    a.creature_display_name.clone().unwrap_or_default(),
                    "15-6",
                    CREAM,
                    1,
                    false,
                    None,
                );
            }
            for (i, (name, value)) in dereth_presentation::appraisal::creature_rows(&a)
                .iter()
                .enumerate()
            {
                let y = 93 + i32_from(i) * 22;
                image(&mut f, 0x060012c3, rect(4, y, 292, 20), None, false, false);
                text(
                    &mut f,
                    rect(11, y + 2, 141, 16),
                    name,
                    "16-7",
                    CREAM,
                    0,
                    false,
                    None,
                );
                text(
                    &mut f,
                    rect(156, y + 2, 136, 16),
                    value,
                    "16-7",
                    CREAM,
                    2,
                    false,
                    None,
                );
            }
        } else {
            tile_icon(&mut f, g, object, 245, 35);
            // Stretched, the description grows and the inscription keeps to the bottom.
            let dy = crate::panels::side_height() as i32 - 362;
            separator(&mut f, 77);
            separator(&mut f, 277 + dy);
            text(
                &mut f,
                rect(6, 32, 237, 16),
                a.value
                    .map_or("Value: ???".into(), |v| format!("Value: {}p", number(v))),
                "16-7",
                CREAM,
                1,
                false,
                None,
            );
            text(
                &mut f,
                rect(6, 52, 237, 16),
                a.burden
                    .filter(|v| *v >= 0)
                    .map_or("Unknown Burden".into(), |v| {
                        format!("{} Burden Units", number(v))
                    }),
                "16-7",
                CREAM,
                1,
                false,
                None,
            );
            rich_scroll(
                &mut f,
                rect(9, 85, 266, 192 + dy),
                super::appraisal::rich(&a),
                "15-6",
                self.scroll,
            );
            image(
                &mut f,
                0x0600126f,
                rect(4, 285 + dy, 292, 73),
                None,
                false,
                false,
            );
            if a.inscribable {
                let editable = can_inscribe(object, &a, g);
                let contents = self
                    .inscription
                    .clone()
                    .or(a.inscription.clone())
                    .unwrap_or_default();
                let scribe = if self.inscription.is_some() {
                    if contents.is_empty() {
                        ""
                    } else {
                        g.character_name().unwrap_or("")
                    }
                } else {
                    a.scribe_name.as_deref().unwrap_or("")
                };
                let editor = f.edit(
                    "inscription",
                    rect(4, 285 + dy, 292, 55),
                    contents,
                    1000,
                    true,
                    editable,
                );
                editor.background = None;
                editor.font = "15-6".into();
                editor.color = DARK;
                if scribe.is_empty()
                    && !self.inscription_focus
                    && self.inscription.as_deref().unwrap_or("").is_empty()
                {
                    text(
                        &mut f,
                        rect(6, 301 + dy, 288, 39),
                        "(inscribe here)",
                        "italic-15-6",
                        DARK,
                        1,
                        true,
                        None,
                    );
                }
                text(
                    &mut f,
                    rect(6, 342 + dy, 288, 17),
                    if scribe.is_empty() {
                        String::new()
                    } else {
                        format!("--{scribe}")
                    },
                    "italic-15-6",
                    DARK,
                    2,
                    false,
                    None,
                );
            }
        }
        f
    }
    fn event(&mut self, e: ControlEvent, ctx: &Context<'_>) -> Vec<PanelAction> {
        match e {
            ControlEvent::Pointer {
                x,
                y,
                pressed: true,
            } => {
                let dy = crate::panels::side_height() as i32 - 362;
                self.inscription_focus = rect(4, 285 + dy, 292, 55).contains(x, y);
            }
            ControlEvent::Activate(id) if id == "close" => {
                let mut actions = self.commit_inscription(ctx);
                actions.push(PanelAction::Game(UiRequest::CancelAppraisal));
                actions.push(PanelAction::Close);
                return actions;
            }
            ControlEvent::Commit { id } if id == "inscription" => {
                self.inscription_focus = false;
                return self.commit_inscription(ctx);
            }
            ControlEvent::Edit { id, text } if id == "inscription" => {
                self.inscription = Some(text);
                self.inscription_dirty = true;
            }
            ControlEvent::Scroll { id, value } if id == "description" => self.scroll = value.max(0),
            ControlEvent::Activate(id) if id.starts_with("component:") => {
                if let Some(object) = id[10..]
                    .parse::<u32>()
                    .ok()
                    .and_then(|s| ctx.game.component_object_id(s))
                {
                    return vec![PanelAction::Game(UiRequest::Select(object))];
                }
            }
            _ => {}
        }
        vec![]
    }
}

fn can_inscribe(object: ObjectId, a: &AppraisalView, g: &dyn GameView) -> bool {
    inscription_editable(
        g.inscription_mouse_facts(object),
        a.owned_by_player,
        a.viewer_is_psr,
        a.scribe_name.as_deref().unwrap_or(""),
        g.character_name().unwrap_or(""),
    )
}

// A character's portrait: three palette ranges over the face, with the eyes mirrored.
fn draw_portrait(f: &mut PanelFrame, portrait: &ClassicPortrait) {
    // The face's textures and palettes come from the classic portal. A palette it does not have
    // (a colour later than its era) leaves that range to the eye texture's own palette.
    let Some(art) = crate::art::installed() else {
        return;
    };
    let textures: Vec<_> = portrait
        .textures
        .iter()
        .map(|t| art.indexed_texture(&format!("{t:08X}")))
        .collect();
    let Some(Some(eyes)) = textures.first() else {
        return;
    };
    if textures.iter().any(Option::is_none) {
        return;
    }
    let fallback = art.palette(eyes.palette);
    let sources = portrait
        .palettes
        .map(|id| art.palette(id).or_else(|| fallback.clone()));
    let palette = portrait_palette(&sources);
    for (texture, x, y, w, h, flip_x) in [
        (portrait.textures[0], 179, 25, 25, 25, false),
        (portrait.textures[0], 204, 25, 25, 25, true),
        (portrait.textures[1], 179, 50, 50, 10, false),
        (portrait.textures[2], 179, 60, 50, 25, false),
    ] {
        if texture != 0 {
            f.screen.commands.push(Command::IndexedImage {
                did: format!("{texture:08X}"),
                palette: palette.clone(),
                x,
                y,
                width: w,
                height: h,
                clip: Some([179, 25, 231, 85]),
                flip_x,
            });
        }
    }
}
pub(super) fn portrait_palette(sources: &[Option<Vec<[u8; 4]>>; 3]) -> Vec<[u8; 4]> {
    let mut result = vec![[0, 0, 0, 255]; 256];
    for (source, start, end) in [(2, 0, 24), (1, 24, 32), (0, 32, 40)] {
        if let Some(palette) = &sources[source] {
            for (i, slot) in result.iter_mut().enumerate().take(end).skip(start) {
                if let Some(color) = palette.get(i) {
                    *slot = *color;
                }
            }
        }
    }
    result
}

/// The examined object's picture: the same tile the pack shows for it, its kind's background
/// behind the icon and its enchantments' colour in the icon's edge.
fn tile_icon(f: &mut PanelFrame, g: &dyn GameView, object: ObjectId, x: i32, y: i32) {
    let decoration = g.slot_decoration(object).or_else(|| {
        g.icon(object)
            .map(|icon| dereth_client_contract::view::SlotDecoration {
                icon_id: icon.0,
                ..Default::default()
            })
    });
    if let Some(d) = decoration.filter(|d| d.icon_id != 0 || d.is_player) {
        f.screen.commands.push(crate::Command::ItemIcon {
            recipe: crate::item_art::recipe(&d, crate::item_art::Surface::Tile),
            x,
            y,
            width: 32,
            height: 32,
            clip: None,
        });
    }
}

#[cfg(test)]
mod spell_tests {
    use super::*;
    #[derive(Debug)]
    struct Spell(&'static str);
    impl GameView for Spell {
        fn spell_examine(&self, _: u32) -> Option<dereth_client_contract::SpellExamineView> {
            use dereth_client_contract::{SpellExamineComponent, SpellExamineView};
            Some(SpellExamineView {
                name: "Void spell".into(),
                description: self.0.into(),
                school: 5,
                base_mana: -1,
                mana_mod: 2,
                duration: 119.9,
                range: 0.9144,
                components: vec![
                    None,
                    Some(SpellExamineComponent {
                        name: "Hidden".into(),
                        ..Default::default()
                    }),
                    Some(SpellExamineComponent {
                        name: "Lead Scarab".into(),
                        icon: Some(DataId(1)),
                        ..Default::default()
                    }),
                ],
                ..Default::default()
            })
        }
    }
    /// Behaviour: spell-examine.text.school-and-surviving-components
    #[test]
    fn a_classic_spell_examination_shows_void_and_only_surviving_component_text() {
        for (description, expected) in [
            ("x", "x\nCOMPONENTS:\n     Lead Scarab"),
            ("😀", "😀\n\nCOMPONENTS:\n     Lead Scarab"),
        ] {
            let game = Spell(description);
            let ctx = Context {
                now: dereth_primitives::LocalTime(0.0),
                game: &game,
                pregame: &Default::default(),
                keyboard: &Default::default(),
                settings: &Default::default(),
                map_teleport_allowed: false,
                classic: &Default::default(),
            };
            let mut p = Examine::default();
            p.set_spell(1);
            let frame = p.frame(&ctx);
            let texts: Vec<&str> = frame
                .screen
                .commands
                .iter()
                .filter_map(|c| match c {
                    crate::Command::TextBox { text, .. } | crate::Command::Text { text, .. } => {
                        Some(text.as_str())
                    }
                    _ => None,
                })
                .collect();
            assert!(texts.contains(&"School: Void Magic"));
            assert!(texts.contains(&"Mana: 0 + 2 per target"));
            assert!(texts.contains(&"Duration: 1 min."));
            assert!(texts.contains(&"Range: 1.0 yds."));
            let description = frame
                .screen
                .commands
                .iter()
                .find_map(|c| match c {
                    crate::Command::RichTextBox { runs, .. } => {
                        Some(runs.iter().map(|r| r.text.as_str()).collect::<String>())
                    }
                    _ => None,
                })
                .unwrap();
            assert_eq!(description, expected);
        }
    }
}

#[cfg(test)]
mod appraisal_tests {
    use super::*;
    use dereth_client_contract::snapshot::GameSnapshot;

    #[derive(Debug)]
    struct View {
        profile: AppraisalView,
        live: Option<(bool, u32)>,
    }
    impl GameView for View {
        fn selected_object(&self) -> Option<ObjectId> {
            Some(ObjectId(8))
        }
        fn appraisal(&self, _: ObjectId) -> Option<AppraisalView> {
            Some(self.profile.clone())
        }
        fn inscription_mouse_facts(&self, _: ObjectId) -> Option<(bool, u32)> {
            self.live
        }
        fn character_name(&self) -> Option<&str> {
            Some("Aerin")
        }
    }
    fn with_context(view: &dyn GameView, run: impl FnOnce(&Context<'_>)) {
        run(&Context {
            now: dereth_primitives::LocalTime(0.0),
            game: view,
            pregame: &Default::default(),
            keyboard: &Default::default(),
            settings: &Default::default(),
            map_teleport_allowed: false,
            classic: &Default::default(),
        });
    }

    /// Behaviour: examine.inscription.live-object-gates-editing
    #[test]
    fn a_displayed_inscription_requires_the_live_object_before_ownership_or_privilege() {
        for (live, owned, privileged, scribe, allowed) in [
            (None, true, true, "Aerin", false),
            (Some((false, 0)), true, true, "Aerin", false),
            (Some((true, 0)), false, false, "", false),
            (Some((true, 0)), true, false, "Someone", false),
            (Some((true, 0)), true, false, "aErIn", true),
            (Some((true, 0)), true, false, "", true),
            (Some((true, 0)), false, true, "Someone", true),
        ] {
            let view = View {
                profile: AppraisalView {
                    inscribable: true,
                    owned_by_player: owned,
                    viewer_is_psr: privileged,
                    scribe_name: Some(scribe.into()),
                    inscription: Some("Original".into()),
                    ..Default::default()
                },
                live,
            };
            let frozen = GameSnapshot::from_view(&view);
            for game in [&view as &dyn GameView, &frozen] {
                with_context(game, |ctx| {
                    let mut panel = Examine::default();
                    panel.set_object(ObjectId(8));
                    let frame = panel.frame(ctx);
                    let editor = frame
                        .controls
                        .iter()
                        .find(|c| c.id == "inscription")
                        .unwrap();
                    assert_eq!(
                        editor.enabled, allowed,
                        "live={live:?}, owned={owned}, privileged={privileged}"
                    );
                    assert!(
                        matches!(&editor.kind, ControlKind::Edit { text, .. } if text == "Original")
                    );
                    panel.event(
                        ControlEvent::Edit {
                            id: "inscription".into(),
                            text: "New".into(),
                        },
                        ctx,
                    );
                    let sent = panel.event(
                        ControlEvent::Commit {
                            id: "inscription".into(),
                        },
                        ctx,
                    );
                    assert_eq!(
                        sent,
                        if allowed {
                            vec![PanelAction::Game(UiRequest::SetInscription {
                                object: ObjectId(8),
                                text: "New".into(),
                            })]
                        } else {
                            vec![]
                        }
                    );
                    assert!(panel
                        .event(
                            ControlEvent::Commit {
                                id: "inscription".into()
                            },
                            ctx
                        )
                        .is_empty());
                });
            }
        }
    }

    /// Behaviour: examine.pane.template-selects-character
    #[test]
    fn a_titleless_template_uses_character_content_in_direct_and_frozen_views() {
        for (creature, template, title, character) in [
            (true, true, false, true),
            (true, false, true, true),
            (true, false, false, false),
            (false, true, true, false),
        ] {
            let view = View {
                profile: AppraisalView {
                    creature,
                    template,
                    character_title: title,
                    profession: Some("Apprentice Explorer".into()),
                    creature_display_name: Some("Creature kind".into()),
                    ..Default::default()
                },
                live: None,
            };
            let frozen = GameSnapshot::from_view(&view);
            for game in [&view as &dyn GameView, &frozen] {
                with_context(game, |ctx| {
                    let panel = Examine::default();
                    let frame = panel.frame(ctx);
                    let texts: Vec<&str> = frame
                        .screen
                        .commands
                        .iter()
                        .filter_map(|command| match command {
                            crate::Command::Text { text, .. }
                            | crate::Command::TextBox { text, .. } => Some(text.as_str()),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(texts.contains(&"Apprentice Explorer"), character);
                    assert_eq!(texts.contains(&"Non-Player Killer"), character);
                    assert_eq!(texts.contains(&"Creature kind"), creature && !character);
                });
            }
        }
    }
}
