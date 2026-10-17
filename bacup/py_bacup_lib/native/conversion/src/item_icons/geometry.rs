use std::collections::HashSet;
use std::sync::Arc;

use nif_core_native::model::{NifBlock, NifFile, NifValue};

use super::assets::{Alpha, Assets, data_name};
use super::render::{Point, Triangle, bounds};

type Matrix = [[f64; 4]; 4];
const IDENTITY: Matrix = [
    [1., 0., 0., 0.],
    [0., 1., 0., 0.],
    [0., 0., 1., 0.],
    [0., 0., 0., 1.],
];

fn number(value: Option<&NifValue>, fallback: f64) -> f64 {
    match value {
        Some(NifValue::Float(v)) => *v,
        Some(v @ (NifValue::UInt(_) | NifValue::Int(_) | NifValue::Ref(_))) => v.as_i64() as f64,
        _ => fallback,
    }
}
fn text(value: Option<&NifValue>) -> &str {
    if let Some(NifValue::String(s)) = value {
        s
    } else {
        ""
    }
}
fn array(value: Option<&NifValue>) -> &[NifValue] {
    if let Some(NifValue::Array(v)) = value {
        v
    } else {
        &[]
    }
}
fn member<'a>(value: &'a NifValue, name: &str) -> Option<&'a NifValue> {
    if let NifValue::Struct(fields) = value {
        fields.get(name)
    } else {
        None
    }
}
fn point(value: Option<&NifValue>) -> Point {
    match value {
        Some(NifValue::Vec3(v)) => v.map(f64::from),
        Some(value) => ["x", "y", "z"].map(|key| number(member(value, key), 0.)),
        None => [0.; 3],
    }
}
fn linked<'a>(nif: &'a NifFile, value: Option<&NifValue>) -> Option<&'a NifBlock> {
    let index = value?.as_i64();
    if index < 0 {
        None
    } else {
        nif.get_block(index as usize)
    }
}
fn local(block: &NifBlock) -> Matrix {
    let mut matrix = IDENTITY;
    let scale = number(block.get_field("Scale"), 1.);
    let scale = if scale == 0. { 1. } else { scale };
    for row in 0..3 {
        for col in 0..3 {
            matrix[row][col] = match block.get_field("Rotation") {
                Some(NifValue::Matrix33(values)) => values[row][col] as f64,
                Some(value) => number(
                    member(value, &format!("m{}{}", col + 1, row + 1)),
                    f64::from(row == col),
                ),
                None => f64::from(row == col),
            } * scale;
        }
    }
    let translation = point(block.get_field("Translation"));
    for row in 0..3 {
        matrix[row][3] = translation[row];
    }
    matrix
}
fn transform(parent: Matrix, child: Matrix) -> Matrix {
    std::array::from_fn(|row| {
        std::array::from_fn(|col| {
            (0..4).fold(0., |sum, i| parent[row][i].mul_add(child[i][col], sum))
        })
    })
}
fn skip_reason(nif: &NifFile, block: &NifBlock) -> Option<&'static str> {
    if number(block.get_field("Flags"), 0.) as u64 & 1 != 0 {
        return Some("hidden");
    }
    if block.type_name == "NiBillboardNode" {
        return Some("billboard");
    }
    if text(block.get_field("Name"))
        .to_lowercase()
        .starts_with("editormarker")
    {
        return Some("editor marker");
    }
    let shader = linked(nif, block.get_field("Shader Property"))?;
    if number(shader.get_field("Shader Flags 1"), 0.) as u64 & (1 << 15) != 0 {
        return Some("refraction");
    }
    if shader.type_name == "BSEffectShaderProperty"
        && [shader, block]
            .iter()
            .any(|b| number(b.get_field("Controller"), -1.) >= 0.)
    {
        return Some("animated effect");
    }
    None
}
fn alpha_test(
    nif: &NifFile,
    block: &NifBlock,
    assets: &Assets,
) -> Result<Option<(Arc<Alpha>, f64)>, String> {
    let Some(shader) = linked(nif, block.get_field("Shader Property")) else {
        return Ok(None);
    };
    let name = data_name("materials", text(shader.get_field("Name")));
    let material = if name.ends_with(".bgsm") || name.ends_with(".bgem") {
        assets.read(&name)?
    } else {
        None
    };
    let (tested, threshold, texture) = if let Some(bytes) = material {
        if name.ends_with(".bgsm") {
            let material = materials_native::bgsm::parse(&bytes).map_err(|e| e.to_string())?;
            (
                material.header.alpha_test,
                material.header.alpha_test_ref as f64,
                material.DiffuseTexture,
            )
        } else {
            let material = materials_native::bgem::parse(&bytes).map_err(|e| e.to_string())?;
            (
                material.header.alpha_test,
                material.header.alpha_test_ref as f64,
                material.BaseTexture,
            )
        }
    } else {
        let alpha = linked(nif, block.get_field("Alpha Property"));
        let tested = alpha.is_some_and(|a| number(a.get_field("Flags"), 0.) as u64 & 0x200 != 0);
        let threshold = alpha.map_or(0., |a| number(a.get_field("Threshold"), 0.));
        let textures = linked(nif, shader.get_field("Texture Set"));
        let texture = textures
            .map(|t| text(array(t.get_field("Textures")).first()))
            .unwrap_or_else(|| text(shader.get_field("Source Texture")))
            .to_string();
        (tested, threshold, texture)
    };
    if tested && !texture.is_empty() {
        return Ok(assets
            .alpha(&data_name("textures", &texture))?
            .map(|alpha| (alpha, threshold)));
    }
    Ok(None)
}

struct Card {
    points: Vec<Point>,
    uv: Vec<[f64; 2]>,
    faces: Vec<[usize; 3]>,
    vertex_alpha: Vec<f64>,
    texture: Arc<Alpha>,
    threshold: f64,
}

pub fn visible_triangles(
    nif: &NifFile,
    assets: &Assets,
) -> Result<(Vec<Triangle>, Vec<(String, String)>), String> {
    fn visit(
        nif: &NifFile,
        assets: &Assets,
        id: usize,
        parent: Matrix,
        visited: &mut HashSet<usize>,
        triangles: &mut Vec<Triangle>,
        cards: &mut Vec<Card>,
        skipped: &mut Vec<(String, String)>,
    ) -> Result<(), String> {
        if !visited.insert(id) {
            return Ok(());
        }
        let Some(block) = nif.get_block(id) else {
            return Ok(());
        };
        if let Some(reason) = skip_reason(nif, block) {
            let name = text(block.get_field("Name"));
            skipped.push((
                if name.is_empty() {
                    block.type_name.clone()
                } else {
                    name.into()
                },
                reason.into(),
            ));
            return Ok(());
        }
        let matrix = transform(parent, local(block));
        let vertices = array(block.get_field("Vertex Data"));
        let faces: Vec<[usize; 3]> = array(block.get_field("Triangles"))
            .iter()
            .map(|v| ["v1", "v2", "v3"].map(|key| number(member(v, key), 0.) as usize))
            .collect();
        if !vertices.is_empty()
            && !faces.is_empty()
            && faces.iter().flatten().all(|&v| v < vertices.len())
        {
            let points: Vec<Point> = vertices
                .iter()
                .map(|v| {
                    let [x, y, z] = point(member(v, "Vertex"));
                    std::array::from_fn(|r| {
                        z.mul_add(matrix[r][2], y.mul_add(matrix[r][1], x * matrix[r][0]))
                            + matrix[r][3]
                    })
                })
                .collect();
            let cut = if member(&vertices[0], "UV").is_some() {
                alpha_test(nif, block, assets)?
            } else {
                None
            };
            if let Some((texture, threshold)) = cut {
                let uv = vertices
                    .iter()
                    .map(|v| {
                        member(v, "UV").map_or([0.; 2], |uv| {
                            ["u", "v"].map(|key| number(member(uv, key), 0.))
                        })
                    })
                    .collect();
                let vertex_alpha = vertices
                    .iter()
                    .map(|v| {
                        member(v, "Vertex Colors")
                            .map_or(255., |color| number(member(color, "a"), 255.))
                    })
                    .collect();
                cards.push(Card {
                    points,
                    uv,
                    faces,
                    vertex_alpha,
                    texture,
                    threshold,
                });
            } else {
                triangles.extend(faces.iter().map(|face| face.map(|index| points[index])));
            }
        }
        let children = array(block.get_field("Children"));
        let children = if !children.is_empty() && block.type_name == "NiSwitchNode" {
            let index =
                number(block.get_field("Index"), 0.).min((children.len() - 1) as f64) as isize;
            let index = if index < 0 {
                children.len().saturating_sub(index.unsigned_abs())
            } else {
                index as usize
            };
            &children[index..index + 1]
        } else if block.type_name == "NiLODNode" && !children.is_empty() {
            &children[..1]
        } else {
            children
        };
        for child in children {
            if child.as_i64() >= 0 {
                visit(
                    nif,
                    assets,
                    child.as_usize(),
                    matrix,
                    visited,
                    triangles,
                    cards,
                    skipped,
                )?;
            }
        }
        Ok(())
    }
    let mut triangles = Vec::new();
    let mut cards = Vec::new();
    let mut skipped = Vec::new();
    visit(
        nif,
        assets,
        0,
        IDENTITY,
        &mut HashSet::new(),
        &mut triangles,
        &mut cards,
        &mut skipped,
    )?;
    if !cards.is_empty() {
        let (mut low, mut high) = bounds(&triangles);
        for card in &cards {
            for face in &card.faces {
                for &index in face {
                    for axis in 0..3 {
                        low[axis] = low[axis].min(card.points[index][axis]);
                        high[axis] = high[axis].max(card.points[index][axis]);
                    }
                }
            }
        }
        let mut extent: Point = std::array::from_fn(|axis| high[axis] - low[axis]);
        extent.sort_by(f64::total_cmp);
        for card in cards {
            triangles.extend(alpha_tested_triangles(
                &card.points,
                &card.uv,
                &card.faces,
                &card.texture,
                &card.vertex_alpha,
                card.threshold,
                extent[1] / 128.,
            ));
        }
    }
    Ok((triangles, skipped))
}

pub fn alpha_tested_triangles(
    points: &[Point],
    uv: &[[f64; 2]],
    faces: &[[usize; 3]],
    texture: &Alpha,
    vertex_alpha: &[f64],
    threshold: f64,
    pixel: f64,
) -> Vec<Triangle> {
    fn reach<const N: usize>(corners: [[f64; N]; 3]) -> f64 {
        (0..3)
            .map(|i| {
                (0..N)
                    .map(|a| (corners[i][a] - corners[(i + 2) % 3][a]).powi(2))
                    .sum::<f64>()
                    .sqrt()
            })
            .fold(0., f64::max)
    }
    let splits: Vec<usize> = faces
        .iter()
        .map(|face| {
            let texels = reach(face.map(|i| {
                [
                    uv[i][0] * texture.width as f64,
                    uv[i][1] * texture.height as f64,
                ]
            })) / 4.;
            let span = if pixel > 0. {
                reach(face.map(|i| points[i])) / (pixel / 2.)
            } else {
                f64::INFINITY
            };
            texels.min(span).ceil().clamp(1., 24.) as usize
        })
        .collect();
    let mut kept = Vec::new();
    for split in 1..=24 {
        let mut pieces = Vec::new();
        for i in 0..split {
            for j in 0..split - i {
                pieces.push([(i, j), (i + 1, j), (i, j + 1)]);
                if i + j < split - 1 {
                    pieces.push([(i + 1, j), (i + 1, j + 1), (i, j + 1)]);
                }
            }
        }
        for (face, &n) in faces.iter().zip(&splits) {
            if n != split {
                continue;
            }
            for piece in &pieces {
                let spread = |values: [f64; 3]| {
                    piece.map(|(a, b)| {
                        values[0]
                            + a as f64 / split as f64 * (values[1] - values[0])
                            + b as f64 / split as f64 * (values[2] - values[0])
                    })
                };
                let mean = |values: [f64; 3]| {
                    let v = spread(values);
                    (v[0] + v[1] + v[2]) / 3.
                };
                let x = (mean(face.map(|i| uv[i][0])) * texture.width as f64).floor() as i64;
                let y = (mean(face.map(|i| uv[i][1])) * texture.height as f64).floor() as i64;
                let alpha =
                    texture.values[y.rem_euclid(texture.height as i64) as usize * texture.width
                        + x.rem_euclid(texture.width as i64) as usize] as f64
                        * mean(face.map(|i| vertex_alpha[i]))
                        / 255.;
                if alpha > threshold {
                    let xyz: [Point; 3] =
                        std::array::from_fn(|axis| spread(face.map(|i| points[i][axis])));
                    kept.push(std::array::from_fn(|corner| {
                        std::array::from_fn(|axis| xyz[axis][corner])
                    }));
                }
            }
        }
    }
    kept
}
