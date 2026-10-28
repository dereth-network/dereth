//! A convex indoor piece from a static scene. Meshes named `portal_0`, `portal_1`, and so on
//! describe its openings; every other mesh is a wall, floor or ceiling.

use std::collections::BTreeMap;
use std::path::Path;

use dereth_assets::geometry::CellStruct;
use dereth_assets::{BspNode, Environment, GfxObj, Polygon, VertexArray};
use dereth_primitives::frame::V3;
use dereth_primitives::{DataId, Frame, Vec3};

use super::read::{PartModel as _, SceneRecords as _};
use super::{add, named, read, Record};
use crate::error::{Error, Loc};
use crate::source::{Located, NewRecord, Target};

fn dot(a: Vec3, b: Vec3) -> f32 {
    a.x * b.x + a.y * b.y + a.z * b.z
}

/// Import one piece, including the surfaces used by its default look.
///
/// # Errors
/// The scene is not a convex indoor cell, or an opening is not one flat convex boundary.
pub fn records(
    root: &Path,
    path: &Path,
    at: &Loc,
    name: &str,
) -> Result<Vec<Located<NewRecord>>, Error> {
    let scene = read::scene(root, path).map_err(|e| Error::source(at, e))?;
    let mut out = Vec::new();
    let mut surfaces = scene.materials(at, name, &mut out)?;
    texture_colors(&mut out, at);
    if surfaces.len() >= 255 {
        return Err(Error::source(
            at,
            "a piece has at most 254 materials plus its opening surface",
        ));
    }
    let portal_surface =
        u16::try_from(surfaces.len()).map_err(|e| Error::source(at, e.to_string()))?;
    let portal_name = format!("{name}-openings");
    out.push(Located {
        at: at.clone(),
        item: NewRecord::Surface {
            target: Target::New(portal_name.clone()),
            surface_type: 0x11,
            texture: None,
            palette: None,
            color: Some(0xFF00_0000),
            translucency: 1.0,
            luminosity: 0.0,
            diffuse: 1.0,
        },
    });
    surfaces.push(named(&portal_name));
    let mut walls = Vec::new();
    let mut portals = BTreeMap::new();
    for part in &scene.parts {
        let model = part
            .model(Frame::default(), &surfaces)
            .map_err(|e| Error::source(at, e))?;
        if let Some(slot) = part.name.strip_prefix("portal_") {
            let slot: usize = slot
                .parse()
                .map_err(|_| Error::source(at, "portal names end in a number"))?;
            if portals.insert(slot, model).is_some() {
                return Err(Error::source(at, "each portal number appears once"));
            }
        } else {
            walls.push(model);
        }
    }
    if portals.len() > 255 {
        return Err(Error::source(at, "a piece has at most 255 openings"));
    }
    if !portals.keys().copied().eq(0..portals.len()) {
        return Err(Error::source(
            at,
            "portal numbers start at zero without gaps",
        ));
    }
    let cell = assemble(walls, portals.into_values().collect(), portal_surface)
        .map_err(|e| Error::source(at, e))?;
    add(
        &mut out,
        at,
        name.to_owned(),
        Record::Environment {
            environment: Box::new(Environment {
                id: DataId(0),
                cells: vec![cell],
            }),
            surfaces,
        },
    );
    Ok(out)
}

// Indoor walls draw textured surfaces. A plain glTF color becomes a one-pixel picture.
fn texture_colors(records: &mut Vec<Located<NewRecord>>, at: &Loc) {
    let mut extra = Vec::new();
    for record in records.iter_mut() {
        let NewRecord::Surface {
            target: Target::New(name),
            surface_type,
            texture,
            color,
            translucency,
            ..
        } = &mut record.item
        else {
            continue;
        };
        let Some(argb) = color.take() else { continue };
        add(
            &mut extra,
            at,
            name.clone(),
            Record::Image {
                width: 1,
                height: 1,
                pixels: argb.to_le_bytes().to_vec(),
            },
        );
        extra.push(Located {
            at: at.clone(),
            item: NewRecord::SurfaceTexture {
                target: Target::New(name.clone()),
                levels: vec![named(name)],
                category: 0,
            },
        });
        *surface_type = 2;
        *texture = Some(named(name));
        *translucency = 0.0;
    }
    records.extend(extra);
}

fn append(va: &mut VertexArray, mut mesh: GfxObj) -> Result<Vec<Polygon>, String> {
    if va.vertices.len() + mesh.vertex_array.vertices.len() > i16::MAX as usize {
        return Err("a piece has at most 32767 vertices".into());
    }
    let offset = u16::try_from(va.vertices.len()).map_err(|e| e.to_string())?;
    for mut vertex in mesh.vertex_array.vertices {
        vertex.id = i16::try_from(va.vertices.len()).map_err(|e| e.to_string())?;
        va.vertices.push(vertex);
    }
    for poly in &mut mesh.polygons {
        for id in &mut poly.vertex_ids {
            *id += offset;
        }
    }
    Ok(mesh.polygons)
}

fn boundary(va: &VertexArray, polys: &[Polygon]) -> Result<Polygon, String> {
    let first = polys.first().ok_or("an opening needs triangles")?;
    let mut weld = BTreeMap::new();
    let mut edges: BTreeMap<(u16, u16), usize> = BTreeMap::new();
    let mut positions = Vec::new();
    for poly in polys {
        let mut ids = Vec::new();
        for id in &poly.vertex_ids {
            let p = va.vertices[usize::from(*id)].position;
            let key = [p.x, p.y, p.z].map(|n| if n == 0.0 { 0 } else { n.to_bits() });
            ids.push(*weld.entry(key).or_insert(*id));
            positions.push(p);
        }
        for i in 0..ids.len() {
            *edges.entry((ids[i], ids[(i + 1) % ids.len()])).or_default() += 1;
        }
    }
    if edges.values().any(|n| *n != 1) {
        return Err("opening triangles overlap or have inconsistent winding".into());
    }
    let outer: BTreeMap<u16, u16> = edges
        .keys()
        .filter(|(a, b)| !edges.contains_key(&(*b, *a)))
        .copied()
        .collect();
    let boundary_edges = edges
        .keys()
        .filter(|(a, b)| !edges.contains_key(&(*b, *a)))
        .count();
    if outer.len() != boundary_edges || !(3..=255).contains(&outer.len()) {
        return Err("an opening needs one boundary of 3 to 255 corners".into());
    }
    let start = *outer.keys().next().ok_or("an opening needs a boundary")?;
    let mut ids = vec![start];
    loop {
        let next = *outer
            .get(ids.last().unwrap())
            .ok_or("an opening has a broken edge")?;
        if next == start {
            break;
        }
        if ids.contains(&next) || ids.len() >= outer.len() {
            return Err("an opening has a broken boundary".into());
        }
        ids.push(next);
    }
    if ids.len() != outer.len() {
        return Err("an opening cannot have holes or separate islands".into());
    }
    let points: Vec<_> = ids
        .iter()
        .map(|id| va.vertices[usize::from(*id)].position)
        .collect();
    let normal = points[1].sub(points[0]).cross(points[2].sub(points[0]));
    if normal.mag2() < 1e-10 {
        return Err("an opening's first three corners must form a triangle".into());
    }
    let normal = normal.normalize();
    if positions
        .iter()
        .any(|p| dot(normal, p.sub(points[0])).abs() > 0.001)
    {
        return Err("an opening must be flat".into());
    }
    for i in 0..points.len() {
        let a = points[i];
        let b = points[(i + 1) % points.len()];
        let c = points[(i + 2) % points.len()];
        if dot(normal, b.sub(a).cross(c.sub(b))) < -0.0001 {
            return Err("an opening must be convex".into());
        }
    }
    let mut poly = first.clone();
    poly.num_pts = u8::try_from(ids.len()).map_err(|e| e.to_string())?;
    poly.vertex_ids = ids;
    poly.stippling = 4;
    poly.sides_type = 0;
    poly.pos_uv_indices = None;
    poly.neg_uv_indices = None;
    Ok(poly)
}

fn assemble(
    walls: Vec<GfxObj>,
    openings: Vec<GfxObj>,
    portal_surface: u16,
) -> Result<CellStruct, String> {
    let mut va = VertexArray {
        vertex_type: 1,
        vertices: Vec::new(),
    };
    let mut polys = Vec::new();
    for wall in walls {
        polys.extend(append(&mut va, wall)?);
    }
    if polys.len() < 4 {
        return Err("a piece needs walls, a floor and a ceiling".into());
    }
    let wall_count = polys.len();
    let mut portals = Vec::new();
    for opening in openings {
        let triangles = append(&mut va, opening)?;
        portals.push(u16::try_from(polys.len()).map_err(|e| e.to_string())?);
        let mut portal = boundary(&va, &triangles)?;
        portal.pos_surface = portal_surface;
        portal.neg_surface = u16::MAX;
        polys.push(portal);
    }
    if polys.len() > i16::MAX as usize {
        return Err("a piece has at most 32767 polygons".into());
    }
    for (i, poly) in polys.iter_mut().enumerate() {
        poly.poly_id = i16::try_from(i).map_err(|e| e.to_string())?;
        let p: Vec<_> = poly
            .vertex_ids
            .iter()
            .map(|v| va.vertices[usize::from(*v)].position)
            .collect();
        let n = p[1].sub(p[0]).cross(p[2].sub(p[0])).normalize();
        if va
            .vertices
            .iter()
            .any(|v| dot(n, v.position.sub(p[0])) < -0.001)
        {
            return Err("piece polygons must face inward and bound a convex cell".into());
        }
    }
    let key = |id: u16| {
        let p = va.vertices[usize::from(id)].position;
        [p.x, p.y, p.z].map(|n| if n == 0.0 { 0 } else { n.to_bits() })
    };
    let mut edges = BTreeMap::new();
    for poly in &polys {
        for i in 0..poly.vertex_ids.len() {
            let edge = (
                key(poly.vertex_ids[i]),
                key(poly.vertex_ids[(i + 1) % poly.vertex_ids.len()]),
            );
            *edges.entry(edge).or_insert(0usize) += 1;
        }
    }
    if edges
        .iter()
        .any(|((a, b), count)| *count != 1 || edges.get(&(*b, *a)) != Some(&1))
    {
        return Err(
            "walls and portal boundaries must form a closed cell with matching edges".into(),
        );
    }
    let physics_polygons = polys[..wall_count].to_vec();
    let physics_bsp = crate::bsp::physics(&va, &physics_polygons, false);
    let cell_bsp = crate::bsp::cell(&va, &polys);
    let mut drawing = crate::bsp::drawing(&va, &polys);
    for i in 0..drawing.nodes.len() {
        let pairs: Vec<_> = portals
            .iter()
            .enumerate()
            .filter_map(|(slot, poly)| {
                let poly = i16::try_from(*poly).ok()?;
                drawing.nodes[i]
                    .in_polys
                    .contains(&poly)
                    .then_some((poly, i16::try_from(slot).ok()?))
            })
            .collect();
        if pairs.is_empty() {
            continue;
        }
        for (poly, _) in &pairs {
            drawing.nodes[i].in_polys.retain(|p| p != poly);
        }
        drawing.nodes[i].tag = 0x504F_5254;
        drawing.nodes[i].in_portals = pairs;
        for positive in [true, false] {
            let missing = if positive {
                drawing.nodes[i].pos_child.is_none()
            } else {
                drawing.nodes[i].neg_child.is_none()
            };
            if missing {
                let index = u32::try_from(drawing.nodes.len()).map_err(|e| e.to_string())?;
                drawing.nodes.push(BspNode {
                    tag: 0x4C45_4146,
                    leaf_index: Some(0),
                    ..BspNode::default()
                });
                if positive {
                    drawing.nodes[i].pos_child = Some(index);
                } else {
                    drawing.nodes[i].neg_child = Some(index);
                }
            }
        }
    }
    Ok(CellStruct {
        cellstruct_id: 0,
        vertex_array: va,
        polygons: polys,
        portals,
        cell_bsp,
        physics_polygons,
        physics_bsp,
        drawing_bsp: Some(drawing),
    })
}
