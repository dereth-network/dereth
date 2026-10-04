//! The character-creation wizard end to end: a character built with a chosen profession, skills
//! and face produces a char-gen result that passes every index ACE's `PlayerFactory.Create`
//! dereferences unguarded; its template id is the template the character fits (exponents 2.5, 3.0,
//! 3.5 and a 0.75 floor); profession and skill clicks change only the boxes their page declares;
//! the Olthoi wizard snaps past three disabled pages in both directions; the turntable describes the
//! built character; restore is the three-field `0xF7D9`; and the part arrows wrap as retail's do.
//! Fixture: an offline headless `App` on the retail dats, driven by button messages broadcast on its
//! own element tree and compared across paired runs pixel by pixel; the result is inspected, never
//! sent.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_chargen::{Attr, CharGenState, SkillAdvancementClass};
use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::screens::chargen::{
    self, CharGenScreen, EParts, EcgProgress, PROFESSION_BUTTONS,
};

/// Fails the test when the retail dats are not where `$DERETH_TEST_DAT_DIR` says.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

/// The retail store; the test fails when it is absent, with the shared shortfall message from
/// `dereth_dat::testing::open_store_or_fail`.
fn store() -> std::sync::Arc<dereth_dat::RetailDatStore> {
    crate::common::dats()
}

fn tables() -> Option<(
    dereth_assets::tables::CharGen,
    dereth_assets::tables::SkillTable,
)> {
    use dereth_assets::Decode;
    let s = store();
    let cg_bytes = dereth_primitives::AssetSource::read(s.as_ref(), DataId(0x0E00_0002)).ok()?;
    let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes).ok()?;
    let sk_bytes = dereth_primitives::AssetSource::read(s.as_ref(), DataId(0x0E00_0004)).ok()?;
    let sk =
        dereth_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes).ok()?;
    Some((cg, sk))
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

fn character_set() -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: vec![dereth_ui::persist::CharacterIdentity {
            id: ObjectId(0x5000_0001),
            name: "+Alba".into(),
            seconds_grace_period: 0,
        }],
        num_allowed_characters: 11,
        account: "ac01".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

/// Bring an offline `App` up on the char-gen wizard, having pressed *Create Character*. A missing
/// device or shell fails the test here; every caller also `expect`s the result.
fn app_on_wizard() -> Option<App> {
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("a headless App on the software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell starts over the retail dats: {e}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first-pixel scene loads: {e}"));
    let host = app.probe_mut().host_state_mut();
    host.character_set = Some(character_set());
    host.received_set = true;
    host.world_name = Some("ACEmulator".into());
    for _ in 0..12 {
        app.frame();
        match app.ui().and_then(|u| u.flow.current_mode()) {
            Some(m) if m == mode::CHARACTER_MANAGEMENT => break,
            Some(m) if m == mode::INTRO => {
                let root = app
                    .ui()
                    .and_then(|u| u.flow.current())
                    .and_then(|s| s.roots().first().copied());
                if let (Some(root), Some(shell)) = (root, app.ui_mut()) {
                    shell.ui.broadcast_element_message(
                        root,
                        dereth_ui_screens::screens::intro::MSG_SKIP,
                        0,
                        0,
                    );
                }
            }
            _ => {}
        }
    }
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the flow did not reach the character screen"
    );
    click(&mut app, 0x1000_03A0); // Create Character
    app.frame();
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHAR_GEN),
        "Create Character queues the wizard"
    );
    app.frame();
    Some(app)
}

/// Broadcast a real button-click message on the element tree: `(element, 1, 7, 0)`.
fn click(app: &mut App, id: u32) {
    let shell = app.ui_mut().expect("shell");
    let root = shell.flow.current().expect("a screen").roots()[0];
    let h = shell
        .ui
        .get_child_recursive(root, ElementId(id))
        .unwrap_or_else(|| panic!("element {id:#010X} is not in the tree"));
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

/// The same, on a handle that had to be found inside a particular parent.
fn click_handle(app: &mut App, h: ElemHandle) {
    let shell = app.ui_mut().expect("shell");
    shell
        .ui
        .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
}

fn find(app: &App, parent: ElemHandle, id: ElementId) -> Option<ElemHandle> {
    app.ui().expect("shell").ui.get_child_recursive(parent, id)
}

fn root(app: &App) -> ElemHandle {
    app.ui()
        .expect("shell")
        .flow
        .current()
        .expect("a screen")
        .roots()[0]
}

fn wizard(app: &mut App) -> &mut CharGenScreen {
    let shell = app.ui_mut().expect("shell");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<CharGenScreen>().expect("the wizard")
}

/// Drive the whole wizard the way a player does: heritage, profession, a skill, a face, a town, a
/// name. Returns the app with the choices made and **nothing sent**.
fn build_a_character(app: &mut App) {
    // Heritage: Aluvian.
    click(app, 0x1000_03BF);
    app.frame();

    // Profession: Bow Hunter (`0x100003DA`, template 1).
    click(app, 0x1000_03F0); // the profession tab
    app.frame();
    click(app, 0x1000_03DA);
    app.frame();

    // Skills: train one more skill through the row's own `+` button.
    click(app, 0x1000_03F1); // the skills tab
    app.frame();
    let row = {
        let w = wizard(app);
        w.skill_rows
            .iter()
            .position(|r| r.level == SkillAdvancementClass::Untrained && r.train_cost <= 4)
            .and_then(|i| w.skill_rows[i].element)
            .expect("an affordable untrained skill with a row")
    };
    let plus = find(app, row, chargen::skills_page::ROW_INCREASE).expect("the + button");
    click_handle(app, plus);
    app.frame();

    // Appearance: a gender, a hair style, an eye strip and a hair colour.
    click(app, 0x1000_03F2); // the appearance tab
    app.frame();
    // `0x100003A8` is `GENDER_MALE`: it yields gender 1, "Male".
    click(app, chargen::appearance::GENDER_MALE.0);
    app.frame();
    let hair_row = find(app, root(app), ElementId(0x1000_03AF)).expect("the hair row");
    click_handle(app, hair_row);
    let next = find(app, hair_row, chargen::appearance::ARROW_NEXT).expect("the hair > arrow");
    click_handle(app, next);
    click_handle(app, next);
    app.frame();
    let eye_row = find(app, root(app), ElementId(0x1000_03B0)).expect("the eyes row");
    click_handle(app, eye_row);
    let next = find(app, eye_row, chargen::appearance::ARROW_NEXT).expect("the eyes > arrow");
    click_handle(app, next);
    app.frame();
    click_handle(app, hair_row);
    app.frame();
    click(app, chargen::appearance::COLOR_SPOTS[1].0);
    app.frame();

    // Town: Holtburg. Name: typed into the summary page's box.
    click(app, 0x1000_040D);
    click(app, 0x1000_03F4); // the summary tab
    app.frame();
    {
        let shell = app.ui_mut().expect("shell");
        let r = shell.flow.current().expect("a screen").roots()[0];
        let name_box = shell
            .ui
            .get_child_recursive(r, chargen::NAME_FIELD)
            .expect("the name box");
        if let Some(t) = shell.ui.text_element_mut(name_box) {
            t.set_text("Tarinell");
        }
        shell
            .ui
            .broadcast_element_message(name_box, dereth_ui::MessageId(0x44), 0, 0);
    }
    app.frame();
}

// -------------------------------------------------------------------------------------------
// 1. The result ACE would accept — every index it dereferences unguarded
// -------------------------------------------------------------------------------------------

/// Behaviour: chargen.finish.a-built-character-passes-every-check-ace-makes
///
/// Oracle: `PlayerFactory.Create`, read line by line, **including the appearance half**. Every one of these is an unguarded `List<T>[i]` in ACE: a value out of
/// range throws inside `InboundMessageManager` and leaves the client without a response.
/// These index checks therefore require every selected entry to be in range.
#[test]
fn a_character_with_a_chosen_profession_skills_and_face_passes_every_check_ace_makes() {
    let _gpu = gpu_lock();
    have_dats();
    let (cg, skills) = tables().expect("the retail character-generation tables");
    let mut app = app_on_wizard().expect("an app on the character-creation wizard");
    build_a_character(&mut app);

    // The choices really were made.
    {
        let w = wizard(&mut app);
        assert_eq!(w.state.heritage_group, 1, "Aluvian");
        assert_eq!(
            w.state.template, 1,
            "Bow Hunter -- and it is the fitted template"
        );
        assert_eq!(w.state.gender, 1);
        // The wizard opens on a randomized appearance, so the indices are not fixed. The arrows'
        // own arithmetic is asserted in `the_part_arrows_wrap_the_way_the_client_wraps_them`;
        // what *this* test is about is ACE's contract, which is that every index is **in range**.
        let n_hair = w.choices[EParts::Hair.choice_index()].num;
        let n_eyes = w.choices[EParts::Eyes.choice_index()].num;
        assert!(
            (0..n_hair).contains(&w.state.hair_style),
            "hair style {} is not in 0..{n_hair}",
            w.state.hair_style
        );
        assert!(
            (0..n_eyes).contains(&w.state.eyes_strip),
            "eye strip {} is not in 0..{n_eyes}",
            w.state.eyes_strip
        );
        assert_eq!(w.state.hair_color, 1);
        assert_eq!(w.state.start_area, 0, "Holtburg");
        assert!(
            w.state
                .skill_levels
                .iter()
                .filter(|s| **s == SkillAdvancementClass::Specialized)
                .count()
                >= 4,
            "the template specialised its four primary skills"
        );
    }

    // Finish, through the credit dialog if there is one. Nothing is sent to a server: with no
    // link the action is taken off the shell here and inspected.
    let host = app.host_state().clone();
    let actions = {
        let shell = app.ui_mut().expect("shell");
        let s = shell.flow.current_mut().expect("a screen");
        let any: &mut dyn std::any::Any = &mut **s;
        let w = any.downcast_mut::<CharGenScreen>().expect("the wizard");
        if !w.do_finish(true) {
            w.close_dialog(true);
        }
        shell.frame(
            dereth_primitives::LocalTime(2.0),
            &host,
            &mut dereth_ui::NullInputPump,
        );
        shell.take_chargen_actions()
    };
    let [chargen::CharGenAction::SendCharGenResult(result)] = actions.as_slice() else {
        panic!("the wizard did not ask to create anything: {actions:?}");
    };
    let mut msg = dereth_client::app::chargen_result_to_wire(result);
    msg.checksum_value = msg.checksum();

    // ---- `PlayerFactory.Create`, in its own order -----------------------------------------
    let hg = cg
        .heritage_groups
        .get(&msg.heritage_group)
        .expect("HeritageGroups[Heritage] -- unguarded in ACE");
    let sx = hg
        .sexes
        .get(&msg.gender)
        .expect("heritageGroup.Genders[Gender] -- unguarded");
    let ix = |v: i32| usize::try_from(v).ok();
    assert!(
        ix(msg.hair_style).is_some_and(|i| i < sx.hair_styles.len()),
        "HairStyleList[{}] of {} -- unguarded",
        msg.hair_style,
        sx.hair_styles.len()
    );
    assert!(
        ix(msg.eyes_strip).is_some_and(|i| i < sx.eye_strips.len()),
        "EyeStripList[{}] of {}",
        msg.eyes_strip,
        sx.eye_strips.len()
    );
    assert!(
        ix(msg.nose_strip).is_some_and(|i| i < sx.nose_strips.len()),
        "NoseStripList[{}] of {}",
        msg.nose_strip,
        sx.nose_strips.len()
    );
    assert!(
        ix(msg.mouth_strip).is_some_and(|i| i < sx.mouth_strips.len()),
        "MouthStripList[{}] of {}",
        msg.mouth_strip,
        sx.mouth_strips.len()
    );
    assert!(
        ix(msg.hair_color).is_some_and(|i| i < sx.hair_colors.len()),
        "HairColorList[{}] of {}",
        msg.hair_color,
        sx.hair_colors.len()
    );
    assert!(
        ix(msg.eye_color).is_some_and(|i| i < sx.eye_colors.len()),
        "EyeColorList[{}] of {}",
        msg.eye_color,
        sx.eye_colors.len()
    );
    // "No headgear is max UINT"; anything else is `HeadgearList[i]`.
    if msg.headgear_style != -1 {
        assert!(
            ix(msg.headgear_style).is_some_and(|i| i < sx.headgear.len()),
            "HeadgearList[{}] of {}",
            msg.headgear_style,
            sx.headgear.len()
        );
    }
    for (v, list, what) in [
        (msg.shirt_style, sx.shirts.len(), "ShirtList"),
        (msg.trousers_style, sx.pants.len(), "PantsList"),
        (msg.footwear_style, sx.footwear.len(), "FootwearList"),
    ] {
        assert!(ix(v).is_some_and(|i| i < list), "{what}[{v}] of {list}");
    }
    // `PaletteSet.GetPaletteID(hue)` takes a double in 0..=1.
    for (s, what) in [
        (msg.skin_shade, "SkinHue"),
        (msg.hair_shade, "HairHue"),
        (msg.headgear_shade, "HeadgearHue"),
        (msg.shirt_shade, "ShirtHue"),
        (msg.trousers_shade, "PantsHue"),
        (msg.footwear_shade, "FootwearHue"),
    ] {
        assert!((0.0..=1.0).contains(&s), "{what} = {s} is outside 0..=1");
    }
    // ACE indexes `heritageGroup.Templates[characterCreateInfo.TemplateOption]` directly.
    assert!(
        ix(msg.template_num).is_some_and(|i| i < hg.templates.len()),
        "Templates[{}] of {} -- ACE throws on this index",
        msg.template_num,
        hg.templates.len()
    );
    // The attribute and skill halves.
    let attrs = [
        msg.strength,
        msg.endurance,
        msg.coordination,
        msg.quickness,
        msg.focus,
        msg.self_,
    ];
    for a in attrs {
        assert!(
            (10..=100).contains(&a),
            "attribute {a} is outside ACE's 10..=100"
        );
    }
    let total: i32 = attrs.iter().sum();
    assert!(
        total <= i32::try_from(hg.attribute_credits).unwrap(),
        "{total} attribute points of {}",
        hg.attribute_credits
    );
    assert_eq!(
        msg.skill_advancement_classes.len(),
        55,
        "ClientServerSkillsMismatch otherwise"
    );
    let mut used = 0i32;
    let mut trained = 0;
    for (i, sac) in msg.skill_advancement_classes.iter().enumerate() {
        if *sac == 0 || *sac == 1 {
            continue;
        }
        trained += 1;
        let id = u32::try_from(i).unwrap();
        assert!(
            skills.skills.contains_key(&id),
            "skill {id} is not in SkillBaseHash"
        );
        let (t, s) = CharGenState::skill_costs(&cg, &skills, msg.heritage_group, id);
        used += if *sac == 3 { s } else { t };
    }
    assert!(
        trained >= 8,
        "only {trained} skills are trained or specialised; no choice was made"
    );
    assert!(
        used <= i32::try_from(hg.skill_credits).unwrap(),
        "{used} skill credits of {}",
        hg.skill_credits
    );
    assert!(
        used > 0,
        "the build spent nothing -- the skills page did nothing"
    );

    // And the client's own checksum round-trips.
    let bytes = {
        let m = dereth_protocol::login::CharacterSendCharGenResult {
            account: "ac01".into(),
            result: msg.clone(),
        };
        let mut out =
            <dereth_protocol::login::CharacterSendCharGenResult as dereth_protocol::Message>::OPCODE
                .0
                .to_le_bytes()
                .to_vec();
        out.extend(dereth_protocol::write_body(&m).expect("encodes"));
        out
    };
    let back = dereth_protocol::read_body::<dereth_protocol::login::CharacterSendCharGenResult>(
        &bytes[4..],
    )
    .expect("0xF656 round-trips");
    assert_eq!(back.result, msg);
    assert_eq!(back.result.checksum_value, back.result.checksum());
    eprintln!(
        "built: heritage {} gender {} template {} hair {} eyes {} colour {} skills used {used}",
        msg.heritage_group,
        msg.gender,
        msg.template_num,
        msg.hair_style,
        msg.eyes_strip,
        msg.hair_color
    );
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 2. Template fitting, evaluated against the retail table
// -------------------------------------------------------------------------------------------

/// Oracle: the retail template-fit calculation. Its three `pow` exponents are 2.5, 3.0
/// and 3.5, with a 0.75 floor.
///
/// **This is what the wire's template index actually is**, and it is why the client never sends the
/// constructor's -1 in a real session: with a heritage and a gender set, the function assigns 0 or
/// a real index every time it runs. Both profession-page and summary-page updates evaluate it.
///
/// Evaluated, not restated: applying template `n` makes every one of the three terms exactly 1 —
/// `min(char, tmpl) == tmpl`, every normal skill trained, every primary specialised — so the score
/// is 1.0 and `pow` cannot change it, while the untouched default build scores under the floor.
#[test]
fn the_template_the_wire_carries_is_the_one_the_character_fits() {
    have_dats();
    let (cg, skills) = tables().expect("the retail character-generation tables");
    let mut s = CharGenState::default();
    s.reset(&cg, &skills);
    s.set_heritage_group(&cg, &skills, 1);
    s.set_gender(&cg, 1);
    s.reset_skill_levels(&cg, &skills);

    // The default build -- six fifties and the heritage's free skills -- fits nothing.
    assert_eq!(
        Attr::ALL.map(|a| s.get(a)),
        [50; 6],
        "reset applies the default template"
    );
    s.fit_template_to_character(&cg);
    assert_eq!(
        s.template, 0,
        "under the 0.75 floor, so the Custom profession"
    );

    // Every real template reports itself once it has been applied.
    let n = cg.heritage_groups[&1].templates.len();
    assert_eq!(n, 7);
    for want in 1..i32::try_from(n).unwrap() {
        let mut s = CharGenState::default();
        s.reset(&cg, &skills);
        s.set_heritage_group(&cg, &skills, 1);
        s.set_gender(&cg, 1);
        s.set_template(&cg, &skills, want, true);
        let applied = s.template;
        s.fit_template_to_character(&cg);
        assert_eq!(
            s.template, want,
            "template {want} did not fit itself (applied {applied})"
        );
        // And it is a build ACE takes: the attribute total is the template's own.
        let total: i32 = Attr::ALL.iter().map(|a| s.get(*a)).sum();
        assert!(total <= 330, "template {want} spends {total} of 330");
        assert!(
            s.remaining_skill_credits >= 0,
            "template {want} overspends its skill credits"
        );
    }

    // A build one point away from a template still fits it: the ratio is 329/330 raised to 2.5.
    let mut s = CharGenState::default();
    s.reset(&cg, &skills);
    s.set_heritage_group(&cg, &skills, 1);
    s.set_gender(&cg, 1);
    s.set_template(&cg, &skills, 5, true);
    s.set_attribute(Attr::Focus, s.get(Attr::Focus) - 1);
    s.fit_template_to_character(&cg);
    assert_eq!(s.template, 5, "one point off Wayfarer is still Wayfarer");
}

// -------------------------------------------------------------------------------------------
// 3. The pages are reachable and change the frame — the differential
// -------------------------------------------------------------------------------------------

/// Every pixel inside any of a set of elements' own screen boxes.
fn declared_mask(app: &App, ids: &[ElementId], fb: (u32, u32)) -> Vec<bool> {
    let r = root(app);
    let ui = &app.ui().expect("shell").ui;
    let mut mask = vec![false; (fb.0 * fb.1) as usize];
    let mut boxes: Vec<dereth_ui::Box2D> = Vec::new();
    for id in ids {
        // Every match, not the first: six sliders share one child id.
        let mut stack = vec![r];
        while let Some(h) = stack.pop() {
            if ui.node(h).map(dereth_ui::element::ElementNode::element_id) == Some(*id) {
                boxes.push(ui.screen_box(h));
            }
            stack.extend(ui.children(h));
        }
    }
    for b in boxes {
        // `Box2D` is **inclusive** (horizontal validity is `x1 >= x0`, and width is
        // `x1 - x0 + 1`), so the last row and column of every declared box belong to it. Iterating
        // them exclusively would leave a one-pixel border of every declared box *undeclared*.
        for y in b.y0.max(0)..=b.y1.min(fb.1 as i32 - 1) {
            for x in b.x0.max(0)..=b.x1.min(fb.0 as i32 - 1) {
                mask[(y as u32 * fb.0 + x as u32) as usize] = true;
            }
        }
    }
    mask
}

/// Every pixel a glyph run claims from the shipped font metrics (pen plus the font's offsets, glyph
/// width by maximum character height), restricted to the draw commands the predicate keeps.
fn glyph_mask(
    store: &dereth_dat::RetailDatStore,
    list: &[dereth_ui::UiDrawCmd],
    fb: (u32, u32),
    keep: &dyn Fn(&dereth_ui::UiDrawCmd) -> bool,
) -> Vec<bool> {
    let mut fonts: std::collections::BTreeMap<DataId, dereth_render::font::Font> =
        std::collections::BTreeMap::new();
    let mut mask = vec![false; (fb.0 * fb.1) as usize];
    for cmd in list.iter().filter(|c| keep(c)) {
        let Some(clip) = dereth_client::ui_draw::visible_box(cmd, fb) else {
            continue;
        };
        for g in &cmd.glyphs {
            let font = fonts.entry(g.font).or_insert_with(|| {
                dereth_client::ui_draw::load_font(store, g.font).expect("a font")
            });
            let Some(d) = font.get_char_desc(g.ch) else {
                continue;
            };
            if d.width == 0 {
                continue;
            }
            let x0 = g.x + i32::from(d.horizontal_offset_before);
            let y0 = g.y + i32::from(d.vertical_offset_before);
            let x1 = x0 + i32::from(d.width) - 1;
            let y1 = y0 + i32::try_from(font.max_char_height).unwrap_or(0) - 1;
            for y in y0.max(clip.1)..=y1.min(clip.3) {
                for x in x0.max(clip.0)..=x1.min(clip.2) {
                    if x >= 0 && y >= 0 && x < fb.0 as i32 && y < fb.1 as i32 {
                        mask[(y as u32 * fb.0 + x as u32) as usize] = true;
                    }
                }
            }
        }
    }
    mask
}

/// Oracle: the frame itself -- two runs of the **same** screen from the
/// same dats, differing only in whether the click under test happened, and every changed pixel
/// inside a rectangle the UI declared.
///
/// The click is the Bow Hunter profession button. What may change is the seven profession buttons
/// (one loses its highlight, one gains it), the six attribute value boxes, the four read-outs and
/// the blurb — nothing else on the page. **The test fails if the handler is removed**: the two runs
/// would then be identical and `inside` would be 0.
#[test]
fn choosing_a_profession_changes_only_the_boxes_the_profession_page_declares() {
    let _gpu = gpu_lock();
    have_dats();
    type Shot = (u32, u32, Vec<u8>);
    let run = |pick: bool| -> Option<(Shot, Vec<bool>)> {
        let mut app = app_on_wizard()?;
        click(&mut app, 0x1000_03BF); // Aluvian
        app.frame();
        click(&mut app, 0x1000_03F0); // the profession tab
        app.frame();
        if pick {
            // Randomization applies a profession on the way in, so the button is chosen at run
            // time as the first one that is **not** the profession already applied.
            let current = wizard(&mut app).state.template;
            let (id, _, _) = *PROFESSION_BUTTONS
                .iter()
                .find(|(_, t, _)| *t != current && *t != 0)
                .expect("some profession other than the rolled one");
            click(&mut app, id.0);
            app.frame();
        }
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        let mut ids: Vec<ElementId> = PROFESSION_BUTTONS.iter().map(|(i, _, _)| *i).collect();
        ids.push(chargen::slider::VALUE);
        // Attribute updates write *two* places: the value box and the slider's float property
        // 0x86, set to `value * 0.01`, which moves the slider's thumb. It is one of the boxes the
        // page declares.
        ids.push(chargen::slider::SCROLL);
        ids.push(chargen::PROFESSION_DESC);
        for (f, t) in chargen::PROFESSION_READOUTS {
            ids.push(f);
            ids.push(t);
        }
        let mask = declared_mask(&app, &ids, (w, h));
        app.shutdown();
        Some(((w, h, bgra), mask))
    };
    let ((w, h, before), _) = run(false).expect("a rendered frame: retail dats and a WARP device");
    let ((w2, h2, after), mask) =
        run(true).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!((w, h), (w2, h2));

    let (mut inside, mut outside) = (0usize, 0usize);
    for i in 0..mask.len() {
        if before[i * 4..i * 4 + 4] != after[i * 4..i * 4 + 4] {
            if mask[i] {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside every box the page declares"
    );
    assert!(
        inside > 200,
        "only {inside} pixels changed -- the profession click did nothing"
    );
    eprintln!("profession click: {inside} pixels changed, all inside the page's own boxes");
}

/// Behaviour: chargen.skills.training-one-moves-its-row-under-the-trained-heading-and-re-prices-the-rest
///
/// The same shape for the skills page: one `+` click on one row.
///
/// What may change is the skills list and the credit meter, and nothing else.
///
/// Updating a skill removes its row and reinserts it in sorted order, so the trained row *leaves*
/// the "Useable Untrained Skills" group and arrives in "Trained Skills", and every row between the
/// two moves by one: the row's own rectangle is in a different place in the two runs.
#[test]
fn training_a_skill_changes_only_the_skills_list_and_the_credit_meter() {
    let _gpu = gpu_lock();
    have_dats();
    type Shot = (u32, u32, Vec<u8>);
    /// The tight mask (the row and the meter) and the wide one (the whole list box).
    type Masks = (Vec<bool>, Vec<bool>);
    let run = |train: bool| -> Option<(Shot, Masks, dereth_ui::region::Box2D)> {
        let mut app = app_on_wizard()?;
        click(&mut app, 0x1000_03BF);
        app.frame();
        // Randomization applies a profession on the way in and spends the skill budget on it,
        // so there may be nothing left to train and the `+` would correctly refuse an
        // unaffordable skill increase. Choosing **Custom** (template 0, `0x100003D9`) is the
        // player's own way back to an unspent budget.
        click(&mut app, 0x1000_03F0); // the profession tab
        app.frame();
        click(&mut app, 0x1000_03D9); // Custom
        app.frame();
        click(&mut app, 0x1000_03F1); // the skills tab
        app.frame();
        let (row, i) = {
            let w = wizard(&mut app);
            let i = w
                .skill_rows
                .iter()
                // Choosing a heritage also applies the selected profession, which spends most of
                // the budget, so the bound is the remaining budget itself, which the
                // skill-increase action checks.
                .position(|r| {
                    r.level == SkillAdvancementClass::Untrained
                        && r.train_cost <= w.state.remaining_skill_credits
                })?;
            (w.skill_rows[i].element?, i)
        };
        if train {
            let plus = find(&app, row, chargen::skills_page::ROW_INCREASE)?;
            click_handle(&mut app, plus);
            app.frame();
            assert_eq!(
                wizard(&mut app).skill_rows[i].level,
                SkillAdvancementClass::Trained,
                "the + button did not train the row"
            );
        }
        let _ = i;
        let list = app.ui_draw_list().to_vec();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        // The row's own rectangle and the credit meter's field, and inside them the **glyph**
        // rectangles the shipped `Font` metrics predict.
        //
        // The element box alone is not the declaration for text: a glyph is placed at the pen plus
        // the font's vertical offset and is its maximum character height tall. On this 25-pixel
        // row that reaches one pixel past the row's own rectangle.
        let rowbox = { app.ui().expect("shell").ui.screen_box(row) };
        let meter = find(&app, root(&app), chargen::skills_page::CREDITS_FIELD)
            .map(|f| app.ui().expect("shell").ui.screen_box(f));
        let keep = move |c: &dereth_ui::UiDrawCmd| {
            let b = c.screen;
            let in_row =
                b.x0 >= rowbox.x0 && b.x0 < rowbox.x1 && b.y0 >= rowbox.y0 && b.y0 < rowbox.y1;
            let in_meter =
                meter.is_some_and(|m| b.x0 >= m.x0 && b.x0 < m.x1 && b.y0 >= m.y0 && b.y0 < m.y1);
            in_row || in_meter
        };
        let store = store();
        let mut tight = glyph_mask(store.as_ref(), &list, (w, h), &keep);
        // Plus the two element rectangles themselves, for whatever fill they carry.
        //
        // `Box2D` is **inclusive** — width is `x1 - x0 + 1` (and height uses the same rule)
        // — so the ranges are inclusive too; exclusive ranges would leave the last column and the
        // last row of every declared box outside every mask.
        for b in [Some(rowbox), meter].into_iter().flatten() {
            for y in b.y0.max(0)..=b.y1.min(h as i32 - 1) {
                for x in b.x0.max(0)..=b.x1.min(w as i32 - 1) {
                    tight[(y as u32 * w + x as u32) as usize] = true;
                }
            }
        }
        // The outer claim: the list box's own rectangle and the meter's, and nothing else on the
        // page. A glyph composed into a 25-pixel row reaches one pixel past it — the shipped rows
        // are shorter than the font is tall and the list box does not clip them to the row
        // — so the row rectangle alone cannot be the whole declaration, and the list box is the
        // element that does declare where its rows may draw.
        let mut wide = vec![false; (w * h) as usize];
        for b in [
            find(&app, root(&app), chargen::skills_page::LIST)
                .map(|l| app.ui().expect("shell").ui.screen_box(l)),
            meter,
        ]
        .into_iter()
        .flatten()
        {
            for y in b.y0.max(0)..=b.y1.min(h as i32 - 1) {
                for x in b.x0.max(0)..=b.x1.min(w as i32 - 1) {
                    wide[(y as u32 * w + x as u32) as usize] = true;
                }
            }
        }
        app.shutdown();
        Some(((w, h, bgra), (tight, wide), rowbox))
    };
    let ((w, h, before), _, row_before) =
        run(false).expect("a rendered frame: retail dats and a WARP device");
    let ((w2, h2, after), (tight, wide), row_after) =
        run(true).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!((w, h), (w2, h2));
    let (mut in_row, mut in_list, mut outside) = (0usize, 0usize, 0usize);
    for i in 0..tight.len() {
        if before[i * 4..i * 4 + 4] != after[i * 4..i * 4 + 4] {
            if tight[i] {
                in_row += 1;
            } else if wide[i] {
                in_list += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside the skills list and the credit meter altogether"
    );
    assert!(
        in_row > 20,
        "only {in_row} pixels changed -- the + button did nothing on screen"
    );
    // The rest of the change is inside the list box and comes from sorted reinsertion:
    // the row is in a different place in the two runs, and the rows it passed moved with it.
    assert_ne!(
        (row_before.y0, row_before.y1),
        (row_after.y0, row_after.y1),
        "the trained row did not move -- the skill-row update re-files it under \"Trained Skills\""
    );
    eprintln!(
        "skill + click: {in_row} pixels changed inside the trained row and the meter, {in_list} \
         elsewhere in the list box as it re-orders, 0 anywhere else; the row moved from y={} to \
         y={}",
        row_before.y0, row_after.y0
    );
}

// -------------------------------------------------------------------------------------------
// 4. The Olthoi wizard skips three pages
// -------------------------------------------------------------------------------------------

/// Oracle: the retail direction-dependent page snap and the three disabled tabs. Backward
/// navigation sends Profession/Skills to Heritage and Town to Appearance; forward navigation
/// sends Profession/Skills to Appearance and Town to Summary.
#[test]
fn an_olthoi_character_walks_heritage_appearance_summary_in_both_directions() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_wizard().expect("an app on the character-creation wizard");
    click(&mut app, 0x1000_05C7); // Olthoi
    app.frame();
    assert_eq!(wizard(&mut app).state.heritage_group, 12);

    // Forwards.
    let mut seen = vec![wizard(&mut app).progress];
    for _ in 0..3 {
        click(&mut app, 0x1000_03C7); // the right arrow
        app.frame();
        seen.push(wizard(&mut app).progress);
    }
    assert_eq!(
        seen,
        vec![
            EcgProgress::Hertage,
            EcgProgress::Appearance,
            EcgProgress::Summary,
            EcgProgress::Summary
        ],
        "forwards, the three disabled pages are skipped"
    );

    // Backwards.
    let mut seen = vec![wizard(&mut app).progress];
    for _ in 0..3 {
        click(&mut app, 0x1000_03C6); // the left arrow
        app.frame();
        seen.push(wizard(&mut app).progress);
    }
    assert_eq!(
        seen,
        vec![
            EcgProgress::Summary,
            EcgProgress::Appearance,
            EcgProgress::Hertage,
            EcgProgress::Hertage
        ],
        "backwards likewise"
    );

    // An ordinary heritage is linear again.
    click(&mut app, 0x1000_03BF);
    app.frame();
    click(&mut app, 0x1000_03C7);
    app.frame();
    assert_eq!(wizard(&mut app).progress, EcgProgress::Profession);
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 5. The turntable's model
// -------------------------------------------------------------------------------------------

/// Oracle: the retail three preview animation-enum sets, camera choices and turntable
/// arithmetic, including a **3.0**-second rotation period stored as a double.
///
/// What this asserts is the **description** the host reads: the setup the heritage names, the
/// animations, the camera the zoom buttons ask for, and the turntable's own heading arithmetic
/// including its two non-modulo wraps. The preview's pixels are covered by the
/// `login::chargen_preview_space` tests.
#[test]
fn the_turntable_describes_the_character_the_wizard_has_built() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_wizard().expect("an app on the character-creation wizard");
    click(&mut app, 0x1000_03BF); // Aluvian
    app.frame();
    click(&mut app, 0x1000_03F2); // the appearance page
    app.frame();
    {
        let w = wizard(&mut app);
        assert_eq!(w.view3d.viewport, Some(chargen::appearance::VIEWPORT));
        // **Use the selected sex's setup, not the stored fallback.** The fallback is used only
        // when no heritage or gender is chosen. With both selected, the client reads the sex's
        // setup from the heritage table. The heritage-level setup is `0x02000054` (the generic
        // human) for **all thirteen** heritages, while a human male is `0x02000001`.
        assert_eq!(
            w.view3d.setup,
            w.state
                .get_setup_id(&w.tables.as_ref().expect("the tables").chargen),
            "the preview is built from the selected character setup"
        );
        assert_eq!(
            w.view3d.setup,
            dereth_primitives::DataId(0x0200_0001),
            "Aluvian male"
        );
        assert_ne!(w.view3d.setup.0, 0);
        assert_eq!(w.view3d.animation_enum, chargen::ANIM_ENUMS_DEFAULT.0);
        // **Zoomed IN, not out.** Initialization seeds the zoom-in camera position and then
        // selects the face controls, which zoom in again. The page opens on a head-and-shoulders
        // view, matching the retail frame.
        assert_eq!(w.view3d.camera_position, chargen::zoomed_in_camera(1));
    }
    // The viewport element really is in the tree and has the viewport element type.
    let vp = find(&app, root(&app), chargen::appearance::VIEWPORT).expect("the viewport");
    let b = app.ui().expect("shell").ui.screen_box(vp);
    assert!(
        b.width() > 100 && b.height() > 100,
        "the preview box is {}x{}",
        b.width(),
        b.height()
    );

    // Zoom.
    click(&mut app, chargen::appearance::ZOOM_IN.0);
    app.frame();
    assert_eq!(
        wizard(&mut app).view3d.camera_position,
        chargen::zoomed_in_camera(1)
    );
    click(&mut app, chargen::appearance::ZOOM_OUT.0);
    app.frame();
    assert_eq!(
        wizard(&mut app).view3d.camera_position,
        chargen::zoomed_out_camera(1)
    );

    // The turntable: three seconds is a full turn, pressing the same button again stops it.
    click(&mut app, chargen::appearance::ROTATE_CW.0);
    app.frame();
    {
        let w = wizard(&mut app);
        assert!(w.view3d.rotating);
        // Half a turn, measured as a **delta**. The page opens at heading **180**
        // (initialization sets and applies 180.0, because 0 faces +Y -- the direction the camera
        // looks along -- and would show the model's back), and the host delivers the appearance
        // page's message-3 tick every frame, so the heading has already moved by the frame's own
        // `dt` before this line. The absolute value is therefore not the invariant; the half-turn
        // is.
        let before = w.view3d.heading;
        w.tick_preview(1.5);
        let want = if before + 180.0 > 360.0 {
            before - 180.0
        } else {
            before + 180.0
        };
        assert!(
            (w.view3d.heading - want).abs() < 1e-3,
            "half of three seconds is half a turn: {before} -> {} , wanted {want}",
            w.view3d.heading
        );
        w.tick_preview(1.5);
        assert!(w.view3d.heading <= 360.0);
    }
    click(&mut app, chargen::appearance::ROTATE_CW.0);
    app.frame();
    assert!(
        !wizard(&mut app).view3d.rotating,
        "the same button again stops the turntable"
    );

    // Olthoi swaps the whole animation set and the camera.
    click(&mut app, 0x1000_03F0);
    app.frame();
    click(&mut app, 0x1000_03EF); // back to heritage
    app.frame();
    click(&mut app, 0x1000_05C8); // OlthoiAcid
    app.frame();
    click(&mut app, 0x1000_03F2);
    app.frame();
    let w = wizard(&mut app);
    assert_eq!(w.view3d.animation_enum, chargen::ANIM_ENUMS_OLTHOI_ACID.0);
    assert_eq!(w.view3d.anim_array_enums, chargen::ANIM_ENUMS_OLTHOI_ACID.2);
    // Zoomed in again: re-entering the page selects Face and applies its zoomed-in camera.
    assert_eq!(
        w.view3d.camera_position,
        chargen::zoomed_in_camera(dereth_chargen::HERITAGE_OLTHOI_ACID)
    );
    app.shutdown();
}

// -------------------------------------------------------------------------------------------
// 6. Restore reaches the wire
// -------------------------------------------------------------------------------------------

/// Oracle: the retail restore request contains the id and **two empty strings**, as does
/// `dereth_protocol`'s own `0xF7D9` transcription.
#[test]
fn restore_character_is_the_documented_three_field_message() {
    use dereth_protocol::Message as _;
    let m = dereth_protocol::admin::AdminSendAdminRestoreCharacter {
        iid: ObjectId(0x5000_0002),
        restored_char_name: String::new(),
        account_to_restore_to: String::new(),
    };
    let body = dereth_protocol::write_body(&m).expect("encodes");
    assert_eq!(
        dereth_protocol::admin::AdminSendAdminRestoreCharacter::OPCODE,
        dereth_protocol::Opcode(0xF7D9)
    );
    // `[guid][""][""]`: four bytes of id then two empty packed strings.
    assert_eq!(&body[..4], &0x5000_0002u32.to_le_bytes());
    let back =
        dereth_protocol::read_body::<dereth_protocol::admin::AdminSendAdminRestoreCharacter>(&body)
            .expect("round-trips");
    assert_eq!(back, m);
}

// -------------------------------------------------------------------------------------------
// 7. The appearance model is the client's
// -------------------------------------------------------------------------------------------

/// Behaviour: chargen.appearance.the-part-arrows-wrap-the-way-the-client-wraps-them
///
/// Oracle: gender-specific appearance constraints, part-choice setup and arrow wrapping,
/// checked against the retail table.
///
/// The headgear arm is the one that differs from the other eight — its floor is **-1**, "no hat",
/// and it wraps off the top back to -1 — and it is the reason ACE's `HeadgearStyle == uint.MaxValue`
/// branch is reachable at all.
#[test]
fn the_part_arrows_wrap_the_way_the_client_wraps_them() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_wizard().expect("an app on the character-creation wizard");
    click(&mut app, 0x1000_03BF);
    app.frame();
    click(&mut app, 0x1000_03F2);
    app.frame();

    let hair_row = find(&app, root(&app), ElementId(0x1000_03AF)).expect("hair row");
    click_handle(&mut app, hair_row);
    let prev = find(&app, hair_row, chargen::appearance::ARROW_PREV).expect("hair <");
    // One step back off **0** wraps to the last style, not to -1.
    //
    // The wizard opens on a randomized hair style, so walk down to 0 first, asserting the
    // ordinary decrement on every step, and then the wrap.
    let n = wizard(&mut app).choices[EParts::Hair.choice_index()].num;
    assert!(n > 1, "the heritage offers {n} hair styles");
    let start = wizard(&mut app).state.hair_style;
    assert!(
        start >= 0,
        "the wizard opened on a rolled hair style, not on -1"
    );
    for i in 0..start {
        click_handle(&mut app, prev);
        app.frame();
        assert_eq!(
            wizard(&mut app).state.hair_style,
            start - i - 1,
            "one step back"
        );
    }
    click_handle(&mut app, prev);
    app.frame();
    assert_eq!(
        wizard(&mut app).state.hair_style,
        n - 1,
        "hair wraps to the last style"
    );

    // Headgear does not: it wraps to -1.
    click(&mut app, chargen::appearance::TAB_CLOTHES.0);
    app.frame();
    let hat_row = find(&app, root(&app), ElementId(0x1000_03B5)).expect("headgear row");
    click_handle(&mut app, hat_row);
    let prev = find(&app, hat_row, chargen::appearance::ARROW_PREV).expect("headgear <");
    let start = wizard(&mut app).state.headgear_style;
    let n = wizard(&mut app).choices[EParts::Headgear.choice_index()].num;
    let mut seen = vec![start];
    for _ in 0..=n {
        click_handle(&mut app, prev);
        app.frame();
        seen.push(wizard(&mut app).state.headgear_style);
    }
    assert!(
        seen.contains(&-1),
        "headgear never reached -1 (no hat): {seen:?}"
    );
    assert!(
        seen.iter().all(|v| *v >= -1 && *v < n),
        "headgear left its range: {seen:?}"
    );
    app.shutdown();
}
