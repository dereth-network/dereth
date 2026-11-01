//! Interior cells' appearances, read from the install.

use auberean_assets::property::PropertyValue;
use auberean_assets::visual::Appearance;
use auberean_primitives::DataId;
use auberean_world::place::cell_look;

use crate::common::install;

/// An interior cell's appearance key picks a change set of the cells' appearance table, which
/// dresses its mesh's materials by kind: the start hall's cell `0x02980120` is a sandy canyon
/// mesh dressed as a grassy hall (its floor's kind `0x81000040` takes modifier `0x30000786`, a
/// grass picture). Every cell whose key the table holds is dressed; a cell naming no appearance
/// (`0x40000001`) or a key the table lacks keeps its mesh's own materials.
///
/// Behaviour: draw.cells.a-cells-appearance-dresses-its-mesh-by-material-kind
#[test]
fn a_cells_appearance_dresses_its_mesh_by_material_kind() {
    let r = &install().records;
    let hall = r.cell(0x02, 0x98, 0x120).expect("the start hall's cell");
    let look = cell_look(r, &hall).expect("the hall names an appearance");
    let floor: Vec<DataId> = look
        .materials
        .iter()
        .filter(|a| a.material_kind == 0x8100_0040)
        .map(|a| a.modifier)
        .collect();
    assert_eq!(floor, vec![DataId(0x3000_0786)]);
    assert_eq!(look.materials.len(), 21);

    let table: Appearance = r
        .get(r.data_id_by_enum(0x4000_0008, 0x4000_0002))
        .expect("the cells' appearance table");
    let (mut cells, mut named, mut dressed, mut expected) = (0, 0, 0, 0);
    for (x, y) in r.landblock_coordinates() {
        let Some(info) = r.landblock_info(x, y) else {
            continue;
        };
        for low in 0x100..0x100 + info.cell_count {
            let Some(c) = u16::try_from(low).ok().and_then(|l| r.cell(x, y, l)) else {
                continue;
            };
            cells += 1;
            let key = c
                .properties
                .iter()
                .flat_map(|p| p.groups.iter())
                .filter(|g| g.group == 0x4000_0010)
                .flat_map(|g| g.properties.iter())
                .find(|p| p.id == 0x4000_005D)
                .and_then(|p| match p.value {
                    PropertyValue::Enum(k) => Some(k),
                    _ => None,
                });
            if let Some(k) = key {
                named += 1;
                if k != 0x4000_0001 && table.keys.iter().any(|(t, _)| *t == k) {
                    expected += 1;
                }
            }
            if cell_look(r, &c).is_some() {
                dressed += 1;
            }
        }
    }
    println!("{cells} interior cells, {named} name an appearance, {dressed} dressed by it");
    assert!(cells > 50_000, "{cells}");
    assert!(dressed > 30_000, "{dressed}");
    assert_eq!(dressed, expected);
}
