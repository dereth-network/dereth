//! A static scene, read by the world builders' shared reader (`dereth-gltf`), made into this
//! game's records: its parts into models, its materials into pictures, textures and surfaces.

use dereth_assets::{GfxObj, Polygon, SwVertex, VertexArray};
use dereth_primitives::{DataId, Frame, Vec3};
use glam::Mat3;

use super::{add, named, Record};
use crate::error::{Error, Loc};
use crate::source::{Located, NewRecord, RecordRef, Target};

use dereth_gltf::{matrix, vector};
pub(super) use dereth_gltf::{scene, Part, Scene};

/// A part made into a model: its triangles in the frame `placement`, with `surfaces` its
/// materials' surfaces.
pub(super) trait PartModel {
    fn model(&self, placement: Frame, surfaces: &[RecordRef]) -> Result<GfxObj, String>;
}

impl PartModel for Part {
    fn model(&self, placement: Frame, surfaces: &[RecordRef]) -> Result<GfxObj, String> {
        let transform = matrix(placement).inverse() * self.transform;
        let normal_matrix = Mat3::from_mat4(transform).inverse().transpose();
        let mirrored = transform.determinant() < 0.0;
        let mut va = VertexArray {
            vertex_type: 1,
            vertices: Vec::new(),
        };
        let mut polygons = Vec::new();
        for p in &self.primitives {
            if va.vertices.len() + p.positions.len() > i16::MAX as usize
                || polygons.len() + p.indices.len() / 3 > i16::MAX as usize
            {
                return Err(format!(
                    "{} exceeds 32767 vertices or triangles; split the mesh",
                    self.name
                ));
            }
            let offset = u16::try_from(va.vertices.len()).map_err(|e| e.to_string())?;
            for (i, position) in p.positions.iter().enumerate() {
                let normal = normal_matrix.mul_vec3(p.normals[i]).normalize_or_zero();
                let position = transform.transform_point3(*position);
                if !position.is_finite() || !normal.is_finite() || normal.length_squared() < 1e-12 {
                    return Err(
                        "transformed vertices and normals must be finite and nonzero".into(),
                    );
                }
                va.vertices.push(SwVertex {
                    id: i16::try_from(va.vertices.len()).map_err(|e| e.to_string())?,
                    position: vector(position),
                    normal: vector(normal),
                    uvs: vec![(p.uvs[i][0], p.uvs[i][1])],
                });
            }
            for triangle in p.indices.as_chunks::<3>().0 {
                let mut ids = triangle
                    .iter()
                    .map(|v| {
                        u16::try_from(*v)
                            .map(|v| v + offset)
                            .map_err(|e| e.to_string())
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if mirrored {
                    ids.swap(1, 2);
                }
                let material = u16::try_from(p.material).map_err(|e| e.to_string())?;
                polygons.push(Polygon {
                    poly_id: i16::try_from(polygons.len()).map_err(|e| e.to_string())?,
                    num_pts: 3,
                    stippling: 1,
                    sides_type: i32::from(p.double_sided),
                    pos_surface: material,
                    neg_surface: material,
                    vertex_ids: ids,
                    pos_uv_indices: Some(vec![0; 3]),
                    neg_uv_indices: p.double_sided.then(|| vec![0; 3]),
                });
            }
        }
        let drawing = crate::bsp::drawing(&va, &polygons);
        let center = drawing
            .nodes
            .first()
            .and_then(|n| n.sphere)
            .map_or(Vec3::ZERO, |s| s.center);
        Ok(GfxObj {
            id: DataId(0),
            flags: 2,
            surfaces: vec![DataId(0); surfaces.len()],
            vertex_array: va,
            physics_polygons: Vec::new(),
            physics_bsp: None,
            sort_center: center,
            polygons,
            drawing_bsp: Some(drawing),
            did_degrade: None,
        })
    }
}

/// A scene's materials made into records: a picture and a texture for each one with a picture, and
/// a surface for every one; answers the surfaces, one a material.
pub(super) trait SceneRecords {
    fn materials(
        &self,
        at: &Loc,
        name: &str,
        out: &mut Vec<Located<NewRecord>>,
    ) -> Result<Vec<RecordRef>, Error>;
}

impl SceneRecords for Scene {
    fn materials(
        &self,
        at: &Loc,
        name: &str,
        out: &mut Vec<Located<NewRecord>>,
    ) -> Result<Vec<RecordRef>, Error> {
        let mut surfaces = Vec::new();
        for (i, material) in self.materials.iter().enumerate() {
            let sym = format!("{name}-material-{i}");
            if material
                .color
                .iter()
                .any(|c| !c.is_finite() || !(0.0..=1.0).contains(c))
            {
                return Err(Error::source(
                    at,
                    "base color components must be between zero and one",
                ));
            }
            let textured = material.image.is_some();
            if let Some(image) = &material.image {
                let (width, height, pixels) =
                    dereth_gltf::base_color_pixels(image, material.color, material.opaque)
                        .map_err(|s| Error::source(at, s))?;
                add(
                    out,
                    at,
                    sym.clone(),
                    Record::Image {
                        width,
                        height,
                        pixels,
                    },
                );
                out.push(Located {
                    at: at.clone(),
                    item: NewRecord::SurfaceTexture {
                        target: Target::New(sym.clone()),
                        levels: vec![named(&sym)],
                        category: 0,
                    },
                });
            }
            let [r, g, b] =
                [0, 1, 2].map(|i| dereth_gltf::channel(dereth_gltf::to_srgb(material.color[i])));
            let a = dereth_gltf::channel(material.color[3]);
            out.push(Located {
                at: at.clone(),
                item: NewRecord::Surface {
                    target: Target::New(sym.clone()),
                    surface_type: if textured { 2 } else { 1 },
                    texture: textured.then(|| named(&sym)),
                    palette: None,
                    color: (!textured).then_some(u32::from_be_bytes([a, r, g, b])),
                    translucency: if textured {
                        0.0
                    } else {
                        1.0 - material.color[3]
                    },
                    luminosity: 0.0,
                    diffuse: 1.0,
                },
            });
            surfaces.push(named(&sym));
        }
        Ok(surfaces)
    }
}
