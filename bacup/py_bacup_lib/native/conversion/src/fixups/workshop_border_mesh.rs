//! FO4 border mesh for a converted FO76 workshop.
//!
//! FO76 draws a workshop border at runtime from the workshop REGN polygon; FO4
//! only shows a placed mesh linked to the workbench through
//! `WorkshopLinkedBuildAreaEdge`. This writes that mesh: a vertical curtain
//! around each region polygon, styled by FO4's own Sanctuary border template.
//! FO76 polygons are 2D, so the curtain spans the build volume's height
//! around the workbench instead of following terrain like vanilla borders.

use std::path::Path;

use indexmap::IndexMap;
use nif_core_native::model::{NifFile, NifValue};

pub const TEMPLATE_RELATIVE_PATH: &str = "Meshes/Workshop/WorkshopBorderSanctuary01.nif";

// Vanilla Sanctuary border texel density (UV Scale 20 on the shader).
const U_UNITS: f32 = 6548.0;
const V_UNITS: f32 = 522.0;
const DOUBLE_SIDED: i64 = 0x10;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct BorderVertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub normal: [f32; 3],
    pub bitangent: [f32; 3],
    pub alpha: u8,
}

#[derive(Debug, Default, PartialEq)]
pub struct BorderGeometry {
    pub vertices: Vec<BorderVertex>,
    pub triangles: Vec<[u16; 3]>,
}

impl BorderGeometry {
    pub fn bounds(&self) -> Option<([f32; 3], [f32; 3])> {
        let first = self.vertices.first()?.position;
        Some(self.vertices.iter().fold((first, first), |(mut min, mut max), v| {
            for axis in 0..3 {
                min[axis] = min[axis].min(v.position[axis]);
                max[axis] = max[axis].max(v.position[axis]);
            }
            (min, max)
        }))
    }
}

/// `loops` are world-space XY polygons; the curtain is built relative to
/// `origin` (the border reference's position) from `-half_height` to
/// `+half_height`, opaque in the middle half and fading out at both ends.
pub fn border_geometry(loops: &[Vec<[f32; 2]>], origin: [f32; 3], half_height: f32) -> Option<BorderGeometry> {
    let levels = [
        (-half_height, 0u8),
        (-half_height * 0.5, 127),
        (half_height * 0.5, 127),
        (half_height, 0),
    ];
    let mut geometry = BorderGeometry::default();
    for points in loops {
        let mut points: Vec<[f32; 2]> = points.clone();
        if points.len() > 1 && points.first() == points.last() {
            points.pop();
        }
        if points.len() < 3 {
            continue;
        }
        let mut distance = 0.0f32;
        for index in 0..points.len() {
            let a = points[index];
            let b = points[(index + 1) % points.len()];
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = (dx * dx + dy * dy).sqrt();
            if length <= f32::EPSILON {
                continue;
            }
            let direction = [dx / length, dy / length, 0.0];
            let normal = [direction[1], -direction[0], 0.0];
            let start = geometry.vertices.len();
            if start + 2 * levels.len() > usize::from(u16::MAX) + 1 {
                return None;
            }
            for (corner, along) in [(a, distance), (b, distance + length)] {
                for (z, alpha) in levels {
                    geometry.vertices.push(BorderVertex {
                        position: [corner[0] - origin[0], corner[1] - origin[1], z],
                        uv: [along / U_UNITS, (z + half_height) / V_UNITS],
                        normal,
                        bitangent: direction,
                        alpha,
                    });
                }
            }
            for level in 0..levels.len() - 1 {
                let a0 = (start + level) as u16;
                let b0 = a0 + levels.len() as u16;
                geometry.triangles.push([a0, a0 + 1, b0 + 1]);
                geometry.triangles.push([a0, b0 + 1, b0]);
            }
            distance += length;
        }
    }
    (!geometry.triangles.is_empty()).then_some(geometry)
}

pub fn write_border_nif(
    template: &NifFile,
    geometry: &BorderGeometry,
    name: &str,
    out_path: &Path,
) -> Result<(), String> {
    let mut nif = template.clone();
    let shape_id = nif
        .blocks
        .iter()
        .position(|block| block.type_name == "BSTriShape")
        .ok_or("border template has no BSTriShape")?;
    let shape = &nif.blocks[shape_id];
    let Some(NifValue::Array(template_vertices)) = shape.get_field("Vertex Data") else {
        return Err("border template has no vertex data".into());
    };
    let vertex_template = template_vertices.first().cloned().ok_or("empty template vertex data")?;
    let template_vertex_count = template_vertices.len() as i64;
    let template_triangle_count = match shape.get_field("Triangles") {
        Some(NifValue::Array(triangles)) => triangles.len() as i64,
        _ => 0,
    };
    let data_size = shape.get_field("Data Size").map_or(0, NifValue::as_i64);
    let vertex_size = (data_size - template_triangle_count * 6) / template_vertex_count.max(1);
    let shader_id = shape.get_field("Shader Property").map(NifValue::as_i64);

    let vertices = geometry
        .vertices
        .iter()
        .map(|vertex| border_vertex_value(&vertex_template, vertex))
        .collect::<Vec<_>>();
    let triangles = geometry
        .triangles
        .iter()
        .map(|[v1, v2, v3]| {
            NifValue::Struct(IndexMap::from([
                ("v1".to_string(), NifValue::UInt(u64::from(*v1))),
                ("v2".to_string(), NifValue::UInt(u64::from(*v2))),
                ("v3".to_string(), NifValue::UInt(u64::from(*v3))),
            ]))
        })
        .collect::<Vec<_>>();
    let (min, max) = geometry.bounds().ok_or("empty border geometry")?;
    let center: [f32; 3] = std::array::from_fn(|axis| (min[axis] + max[axis]) * 0.5);
    let radius = (0..3)
        .map(|axis| (max[axis] - center[axis]).powi(2))
        .sum::<f32>()
        .sqrt();

    if let Some(root) = nif.blocks.get_mut(0) {
        root.set_field("Name", NifValue::String(name.to_string()));
    }
    let shape = &mut nif.blocks[shape_id];
    shape.set_field("Name", NifValue::String(format!("{name}:0")));
    shape.set_field("Num Vertices", NifValue::UInt(vertices.len() as u64));
    shape.set_field("Num Triangles", NifValue::UInt(triangles.len() as u64));
    shape.set_field(
        "Data Size",
        NifValue::UInt((vertices.len() as i64 * vertex_size + triangles.len() as i64 * 6) as u64),
    );
    shape.set_field("Vertex Data", NifValue::Array(vertices));
    shape.set_field("Triangles", NifValue::Array(triangles));
    shape.set_field(
        "Bounding Sphere",
        NifValue::Struct(IndexMap::from([
            ("Center".to_string(), xyz(center)),
            ("Radius".to_string(), NifValue::Float(f64::from(radius))),
        ])),
    );
    // The curtain is seen from inside and outside the workshop.
    if let Some(shader) = shader_id
        .and_then(|id| usize::try_from(id).ok())
        .and_then(|id| nif.blocks.get_mut(id))
    {
        if let Some(flags) = shader.get_field_mut("Shader Flags 2") {
            *flags = NifValue::UInt((flags.as_i64() | DOUBLE_SIDED) as u64);
        }
    }

    if let Some(parent) = out_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create {}: {e}", parent.display()))?;
    }
    nif.save(Some(out_path.to_path_buf()))
        .map_err(|e| format!("save {}: {e}", out_path.display()))
}

fn border_vertex_value(template: &NifValue, vertex: &BorderVertex) -> NifValue {
    let mut value = template.clone();
    let NifValue::Struct(fields) = &mut value else {
        return value;
    };
    fields.insert("Vertex".into(), xyz(vertex.position));
    fields.insert(
        "UV".into(),
        NifValue::Struct(IndexMap::from([
            ("u".to_string(), NifValue::Float(f64::from(vertex.uv[0]))),
            ("v".to_string(), NifValue::Float(f64::from(vertex.uv[1]))),
        ])),
    );
    fields.insert("Normal".into(), xyz(vertex.normal));
    fields.insert("Tangent".into(), xyz([0.0, 0.0, 1.0]));
    for (axis, key) in ["Bitangent X", "Bitangent Y", "Bitangent Z"].into_iter().enumerate() {
        fields.insert(key.into(), NifValue::Float(f64::from(vertex.bitangent[axis])));
    }
    let green = if vertex.alpha > 0 { 255 } else { 0 };
    fields.insert(
        "Vertex Colors".into(),
        NifValue::Struct(IndexMap::from([
            ("r".to_string(), NifValue::UInt(0)),
            ("g".to_string(), NifValue::UInt(green)),
            ("b".to_string(), NifValue::UInt(0)),
            ("a".to_string(), NifValue::UInt(u64::from(vertex.alpha))),
        ])),
    );
    value
}

fn xyz(values: [f32; 3]) -> NifValue {
    NifValue::Struct(IndexMap::from([
        ("x".to_string(), NifValue::Float(f64::from(values[0]))),
        ("y".to_string(), NifValue::Float(f64::from(values[1]))),
        ("z".to_string(), NifValue::Float(f64::from(values[2]))),
    ]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn curtain_follows_each_polygon_edge_around_the_origin() {
        let square = vec![[0.0, 0.0], [100.0, 0.0], [100.0, 100.0], [0.0, 100.0], [0.0, 0.0]];
        let geometry = border_geometry(&[square], [50.0, 50.0, 1000.0], 400.0).unwrap();

        assert_eq!(geometry.vertices.len(), 4 * 8);
        assert_eq!(geometry.triangles.len(), 4 * 6);
        assert_eq!(geometry.bounds(), Some(([-50.0, -50.0, -400.0], [50.0, 50.0, 400.0])));
        let first = &geometry.vertices[..8];
        assert_eq!(first.iter().map(|v| v.alpha).collect::<Vec<_>>(), [0, 127, 127, 0, 0, 127, 127, 0]);
        assert_eq!(first[0].normal, [0.0, -1.0, 0.0]);
        assert_eq!(first[4].uv[0], 100.0 / U_UNITS);
        assert_eq!(geometry.vertices[8].uv[0], 100.0 / U_UNITS);
        assert!(geometry.triangles.iter().flatten().all(|&i| usize::from(i) < geometry.vertices.len()));
    }

    #[test]
    fn degenerate_polygons_produce_no_geometry() {
        assert_eq!(border_geometry(&[vec![[0.0, 0.0], [1.0, 0.0]]], [0.0; 3], 10.0), None);
        assert_eq!(border_geometry(&[], [0.0; 3], 10.0), None);
    }

    #[test]
    fn writes_a_readable_double_sided_nif_from_the_fo4_template() {
        let template_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../../../extracted/fo4")
            .join(TEMPLATE_RELATIVE_PATH);
        let Ok(template) = NifFile::load(template_path) else {
            eprintln!("skipping: FO4 border template not extracted");
            return;
        };
        let square = vec![[0.0, 0.0], [4000.0, 0.0], [4000.0, 3000.0], [0.0, 3000.0]];
        let geometry = border_geometry(&[square], [2000.0, 1500.0, 0.0], 2048.0).unwrap();
        let dir = tempfile::tempdir().unwrap();
        let out = dir.path().join("Border.nif");
        write_border_nif(&template, &geometry, "TestBorder", &out).unwrap();

        let written = NifFile::load(out).unwrap();
        let shape = written.blocks.iter().find(|b| b.type_name == "BSTriShape").unwrap();
        let Some(NifValue::Array(vertices)) = shape.get_field("Vertex Data") else {
            panic!("vertex data");
        };
        assert_eq!(vertices.len(), geometry.vertices.len());
        assert_eq!(shape.get_field("Num Triangles").map(NifValue::as_i64), Some(24));
        let shader = written
            .blocks
            .iter()
            .find(|b| b.type_name == "BSEffectShaderProperty")
            .unwrap();
        let flags = shader.get_field("Shader Flags 2").map(NifValue::as_i64).unwrap();
        assert_eq!(flags & DOUBLE_SIDED, DOUBLE_SIDED);
    }
}
