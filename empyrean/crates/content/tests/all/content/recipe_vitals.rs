//! Divergence: V463
//! Optional recipe vital requirements survive packing and live content edits.
//! Fixture: synthetic recipes and the shared SQL overlay fixture.

use crate::support::overlay_fixture::*;
use empyrean_content::import::WorldContent;
use empyrean_content::models::world::{Recipe, RecipeVitalRequirement};
use empyrean_content::pack::format::{FLAG_INDEX_SORTED, FLAG_RECIPE_VITALS};
use empyrean_content::PackError;

const EXTENSION: &[u8] = b"CREATE TABLE recipe_requirements_vital (id int unsigned NOT NULL, health int unsigned NOT NULL, stamina int unsigned NOT NULL, mana int unsigned NOT NULL, PRIMARY KEY (id), FOREIGN KEY (id) REFERENCES recipe (id) ON DELETE CASCADE); INSERT INTO recipe_requirements_vital VALUES (5,10,0,20);";

fn packed(extended: bool) -> Vec<u8> {
    let mut content = WorldContent::default();
    content.recipes.push(Recipe {
        id: 5,
        ..Recipe::default()
    });
    if extended {
        content
            .recipe_vital_requirements
            .push(RecipeVitalRequirement {
                id: 5,
                health: 10,
                stamina: 0,
                mana: 20,
            });
    }
    content.to_pack([0; 16]).unwrap().0
}

#[test]
fn optional_requirements_round_trip_without_changing_ordinary_recipe_records() {
    let plain = Pack::from_bytes(packed(false)).unwrap();
    let extended = Pack::from_bytes(packed(true)).unwrap();
    plain.verify_hash().unwrap();
    extended.verify_hash().unwrap();
    assert_eq!(plain.header().flags, FLAG_INDEX_SORTED);
    assert_eq!(plain.count(TableId::RECIPE_VITAL), 0);
    assert_eq!(
        extended.header().flags,
        FLAG_INDEX_SORTED | FLAG_RECIPE_VITALS
    );
    assert_eq!(
        plain.raw(TableId::RECIPE, 5).unwrap(),
        extended.raw(TableId::RECIPE, 5).unwrap()
    );
    let db = PackContent::new(extended);
    let requirements = db.get_recipe_vital_requirement(5).unwrap();
    assert_eq!(
        (requirements.health, requirements.stamina, requirements.mana),
        (10, 0, 20)
    );
    assert!(db.get_recipe_vital_requirement(6).is_none());
}

#[test]
fn a_pack_cannot_silently_drop_the_recipe_vital_capability() {
    for extended in [false, true] {
        let mut bytes = packed(extended);
        let wrong_flags = if extended {
            FLAG_INDEX_SORTED
        } else {
            FLAG_INDEX_SORTED | FLAG_RECIPE_VITALS
        };
        bytes[0x10..0x14].copy_from_slice(&wrong_flags.to_le_bytes());
        assert!(matches!(
            Pack::from_bytes(bytes),
            Err(PackError::HeaderField {
                field: "recipe vital table capability",
                ..
            })
        ));
    }
}

#[test]
fn vital_requirements_can_be_added_updated_and_deleted_across_overlay_restarts() {
    let dir = tmp("recipe-vitals");
    {
        let (db, overlay) = open(&dir);
        assert!(db.get_recipe_vital_requirement(5).is_none());
        overlay.import_sql(EXTENSION, "requirements.sql").unwrap();
        assert_eq!(db.get_recipe_vital_requirement(5).unwrap().health, 10);
        overlay
            .import_sql(
                b"UPDATE recipe_requirements_vital SET health=25 WHERE id=5;",
                "update.sql",
            )
            .unwrap();
        assert_eq!(db.get_recipe_vital_requirement(5).unwrap().health, 25);
    }
    let (db, overlay) = open(&dir);
    assert_eq!(db.get_recipe_vital_requirement(5).unwrap().health, 25);
    overlay
        .import_sql(
            b"DELETE FROM recipe_requirements_vital WHERE id=5;",
            "delete.sql",
        )
        .unwrap();
    assert!(db.get_recipe_vital_requirement(5).is_none());
    assert!(db.get_recipe(5).is_some());
    let connection = rusqlite::Connection::open(dir.join("overlay.sqlite")).unwrap();
    let format: String = connection
        .query_row("SELECT value FROM meta WHERE key='format'", [], |row| {
            row.get(0)
        })
        .unwrap();
    assert_eq!(
        format,
        empyrean_content::overlay::file::FORMAT_RECIPE_VITALS
    );
    assert_ne!(
        format,
        empyrean_content::overlay::file::FORMAT,
        "old readers reject the entire overlay, even after removing the requirements"
    );
}

#[test]
fn deleting_a_recipe_removes_its_vital_requirements() {
    let dir = tmp("recipe-vital-cascade");
    let (db, overlay) = open(&dir);
    overlay.import_sql(EXTENSION, "requirements.sql").unwrap();
    overlay
        .import_sql(b"DELETE FROM recipe WHERE id=5;", "delete.sql")
        .unwrap();
    assert!(db.get_recipe(5).is_none());
    assert!(db.get_recipe_vital_requirement(5).is_none());
}
