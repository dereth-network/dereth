//! Behaviour: none (tooling: deleting a pack's own weenie in the weenie editor over the retail dats)
//! A pack's own weenie that nothing names is deleted from the Definition pane's menu or the
//! editor's, as one step that undo takes back; one the world still names is not, and the pane
//! lists what names it. The server's weenies offer no Delete.
//! Fixture: the sample world over Holtburg, whose guide is placed in Holtburg.

use lyceum_editor::app::{Drill, EditorApp, Tab};
use lyceum_foundry::edit::{Op, Weenie};
use lyceum_foundry::script::Action as Edit;

use crate::{click, editor, gpu_lock, sample, shots};

/// A lamp of the pack's that nothing names.
const LAMP: &str = "/* A lamp. */
DELETE FROM `weenie` WHERE `class_Id` = {{wcid:lamp}};

INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES ({{wcid:lamp}}, 'sample-lamp', 1, '2026-10-25 00:00:00') /* Generic */;

INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:lamp}}, 1, 'Brass Lamp') /* Name */;

INSERT INTO `weenie_properties_d_i_d` (`object_Id`, `type`, `value`)
VALUES ({{wcid:lamp}}, 1, 0x020007DD) /* Setup */;
";

/// Weenie `w` shown in the Definition pane, brought forward.
fn define(ed: &mut EditorApp, w: &Weenie) {
    ed.panels.weenie = Some(w.clone());
    ed.show_tab(Tab::Definition);
    for _ in 0..3 {
        assert!(ed.frame());
    }
}

/// The Definition pane's menu opened and its Delete clicked.
fn delete_from_definition(ed: &mut EditorApp) {
    ed.definition.delete_button = None;
    let menu = ed.definition.menu_button.expect("Definition's menu");
    click(ed, (menu.center().x, menu.center().y));
    ed.frame();
    let delete = ed.definition.delete_button.expect("its Delete");
    click(ed, (delete.center().x, delete.center().y));
    ed.frame();
}

#[test]
fn a_packs_weenie_nothing_names_is_deleted_as_one_step_and_one_still_placed_is_not() {
    let _gpu = gpu_lock();
    let (_dir, world) = sample("editor-delete-weenie");
    let mut ed = editor(&world);
    ed.wait_for_names().unwrap();
    let lamp = Weenie::Symbol("holtburg:lamp".into());
    let (file, _) = ed.session.new_weenie_file("lamp").unwrap();
    ed.edit(&Edit::Op(Op::WriteSql {
        file: file.clone(),
        text: Some(LAMP.into()),
    }))
    .unwrap();
    // Deleted from Definition's menu: its file goes, as one step.
    define(&mut ed, &lamp);
    let steps = ed.session.history().0;
    delete_from_definition(&mut ed);
    assert_eq!(ed.session.history().0, steps + 1, "{}", ed.panels.message);
    assert!(ed.session.weenie_file(&lamp).is_none());
    assert!(ed.session.sql_text(&file).is_none());
    assert!(!ed.weenies.iter().any(|e| e.weenie == lamp));
    assert_eq!(ed.panels.weenie, None, "the pane lets go of it");
    // Undone, it is back.
    ed.edit(&Edit::Undo).unwrap();
    assert!(ed.session.weenie_file(&lamp).is_some());
    // From the editor's menu, which closes once it shows nothing.
    ed.edit_weenie(&lamp, None);
    for _ in 0..3 {
        assert!(ed.frame());
    }
    let menu = ed.editor.menu_button.expect("the editor's menu");
    click(&mut ed, (menu.center().x, menu.center().y));
    ed.frame();
    let delete = ed.editor.delete_button.expect("its Delete");
    click(&mut ed, (delete.center().x, delete.center().y));
    ed.frame();
    assert!(ed.session.weenie_file(&lamp).is_none());
    assert!(!ed.editor.open);
    // The guide stands in Holtburg: refused, and the pane lists where.
    let guide = Weenie::Symbol("holtburg:guide".into());
    define(&mut ed, &guide);
    let steps = ed.session.history().0;
    delete_from_definition(&mut ed);
    assert_eq!(ed.session.history().0, steps, "nothing changed");
    assert!(ed.session.weenie_file(&guide).is_some());
    assert!(
        ed.panels.message.contains("still used in 1 place(s)"),
        "{}",
        ed.panels.message
    );
    assert_eq!(ed.definition.drill_of(&guide), Some(Drill::UsedBy));
    for _ in 0..2 {
        assert!(ed.frame());
    }
    ed.capture(&shots("delete-weenie").join("still-used.png"))
        .unwrap();
    // A server's weenie offers no Delete.
    define(&mut ed, &Weenie::Class(1));
    ed.definition.delete_button = None;
    let menu = ed.definition.menu_button.expect("Definition's menu");
    click(&mut ed, (menu.center().x, menu.center().y));
    ed.frame();
    assert!(ed.definition.delete_button.is_none());
}
