//! Uniform scaling of a Havok 2014 FO4 physics-system packfile.
//!
//! Retail precombines keep the placed scale on the clone node and pre-scale
//! the collision, because Havok bodies carry no scale. Every scaled quantity
//! here is a plain float parameter: quantized vertices and tree nodes are
//! encoded relative to domains that scale with them, and W lanes (packed
//! vertex indices, flags) are never touched.

use havok_native::hkx::descriptors::DescriptorRegistry;
use havok_native::hkx::types::HkxValue;
use havok_native::hkx::{HkxMember, read_packfile, write_hkx};

use crate::error::{PrevisError, Result};

const CONVEX_SHAPES: [&str; 4] = ["hknpConvexPolytopeShape", "hknpCapsuleShape", "hknpSphereShape", "hknpConvexShape"];
const UNSCALED: [&str; 3] = ["hkRefCountedProperties", "hknpBSMaterialProperties", "hknpShapeMassProperties"];

fn member<'a>(members: &'a mut [HkxMember], name: &str) -> Result<&'a mut HkxValue> {
    members
        .iter_mut()
        .find(|m| m.name == name)
        .map(|m| &mut m.value)
        .ok_or_else(|| PrevisError::invalid(format!("packfile object has no member {name}")))
}

fn fields(value: &mut HkxValue) -> Result<&mut Vec<HkxMember>> {
    match value {
        HkxValue::Object(members) => Ok(members),
        _ => Err(PrevisError::invalid("packfile member is not a struct")),
    }
}

fn items(value: &mut HkxValue) -> Result<&mut Vec<HkxValue>> {
    match value {
        HkxValue::Array(items) => Ok(items),
        _ => Err(PrevisError::invalid("packfile member is not an array")),
    }
}

fn scale_f32(value: &mut HkxValue, s: f32) -> Result<()> {
    match value {
        HkxValue::F32(v) => Ok(*v *= s),
        _ => Err(PrevisError::invalid("packfile member is not a float")),
    }
}

/// Scales the listed lanes of a float vector (`F32List` or an array of `F32`).
fn scale_lanes(value: &mut HkxValue, lanes: std::ops::Range<usize>, s: f32) -> Result<()> {
    match value {
        HkxValue::F32List(v) if v.len() >= lanes.end => {
            v[lanes].iter_mut().for_each(|x| *x *= s);
            Ok(())
        }
        HkxValue::Array(v) if v.len() >= lanes.end => v[lanes].iter_mut().try_for_each(|x| scale_f32(x, s)),
        _ => Err(PrevisError::invalid("packfile vector is too short")),
    }
}

fn scale_aabb(value: &mut HkxValue, s: f32) -> Result<()> {
    let aabb = fields(value)?;
    scale_lanes(member(aabb, "min")?, 0..3, s)?;
    scale_lanes(member(aabb, "max")?, 0..3, s)
}

fn empty_or_sentinel_simd_tree(value: &mut HkxValue) -> Result<bool> {
    let nodes = items(member(fields(value)?, "nodes")?)?;
    Ok(nodes.iter_mut().all(|node| {
        fields(node)
            .ok()
            .and_then(|n| match n.iter().find(|m| m.name == "lx").map(|m| &m.value) {
                // The unused tree holds the empty-AABB sentinel 0x7F7FFFEE.
                Some(HkxValue::F32List(v)) => Some(v.iter().all(|&x| x > 3.4e38)),
                _ => None,
            })
            .unwrap_or(false)
    }))
}

fn scale_object(class: &str, members: &mut [HkxMember], s: f32) -> Result<()> {
    match class {
        "hknpPhysicsSystemData" => {
            if !items(member(members, "constraintCinfos")?)?.is_empty() {
                return Err(PrevisError::unsupported("scaled collision with constraints is not yet supported"));
            }
            for body in items(member(members, "bodyCinfos")?)? {
                scale_lanes(member(fields(body)?, "position")?, 0..3, s)?;
            }
        }
        "hknpCompressedMeshShape" | "hknpDynamicCompoundShape" => {
            scale_f32(member(members, "convexRadius")?, s)?;
            if class == "hknpDynamicCompoundShape" {
                let instances = fields(member(members, "instances")?)?;
                for instance in items(member(instances, "elements")?)? {
                    scale_lanes(member(fields(instance)?, "transform")?, 12..15, s)?;
                }
                scale_aabb(member(members, "aabb")?, s)?;
            }
        }
        "hknpCompressedMeshShapeData" => {
            if !empty_or_sentinel_simd_tree(member(members, "simdTree")?)? {
                return Err(PrevisError::unsupported("scaled compressed mesh with a SIMD tree is not yet supported"));
            }
            let tree = fields(member(members, "meshTree")?)?;
            scale_aabb(member(tree, "domain")?, s)?;
            for section in items(member(tree, "sections")?)? {
                let section = fields(section)?;
                scale_aabb(member(section, "domain")?, s)?;
                scale_lanes(member(section, "codecParms")?, 0..6, s)?;
            }
        }
        "hknpDynamicCompoundShapeData" => {
            let tree = fields(member(members, "aabbTree")?)?;
            for node in items(member(tree, "nodes")?)? {
                scale_aabb(member(fields(node)?, "aabb")?, s)?;
            }
        }
        convex if CONVEX_SHAPES.contains(&convex) => {
            scale_f32(member(members, "convexRadius")?, s)?;
            for vertex in items(member(members, "vertices")?)? {
                scale_lanes(vertex, 0..3, s)?;
            }
            if let Ok(planes) = member(members, "planes") {
                for plane in items(planes)? {
                    scale_lanes(plane, 3..4, s)?;
                }
            }
            if convex == "hknpCapsuleShape" {
                scale_lanes(member(members, "a")?, 0..3, s)?;
                scale_lanes(member(members, "b")?, 0..3, s)?;
            }
        }
        other if UNSCALED.contains(&other) => {}
        other => return Err(PrevisError::unsupported(format!("scaling {other} collision is not yet supported"))),
    }
    Ok(())
}

/// The packfile uniformly scaled by `s`.
pub fn scale_packfile(blob: &[u8], s: f32) -> Result<Vec<u8>> {
    if !(s.is_finite() && s > 0.0) {
        return Err(PrevisError::invalid(format!("collision scale {s} is not finite and positive")));
    }
    let mut hkx = read_packfile(blob).map_err(|e| PrevisError::invalid(format!("collision packfile: {e}")))?;
    let mut registry = DescriptorRegistry::for_contents_version("hk_2014.1.0-r1");
    // Only edit through the typed model when it reproduces the source exactly.
    if write_hkx(&hkx, &mut registry) != blob {
        return Err(PrevisError::unsupported("collision packfile does not round-trip exactly"));
    }
    if s == 1.0 {
        return Ok(blob.to_vec());
    }
    for object in hkx.objects_mut() {
        let class = object.class_name.clone();
        scale_object(&class, &mut object.members, s)?;
    }
    Ok(write_hkx(&hkx, &mut registry))
}
