use std::collections::{BTreeSet, HashMap};

use havok_native::hkx::{HkxMember, types::HkxValue};
use nif_core_native::model::{NifFile, NifValue};

fn hk_field<'a>(members: &'a [HkxMember], name: &str) -> Result<&'a HkxValue, String> {
    members
        .iter()
        .find(|m| m.name == name)
        .map(|m| &m.value)
        .ok_or_else(|| format!("Starfield family cloth missing {name}"))
}

fn hk_array(value: &HkxValue) -> Result<&[HkxValue], String> {
    match value {
        HkxValue::Array(values) => Ok(values),
        _ => Err("Starfield family cloth expected array".into()),
    }
}

fn cloth_bytes(value: &NifValue) -> Result<Vec<u8>, String> {
    let NifValue::Struct(fields) = value else {
        return Err("Starfield family cloth has invalid byte storage".into());
    };
    let bytes = match fields.get("Data") {
        Some(NifValue::Bytes(bytes)) => bytes.clone(),
        Some(NifValue::Array(values)) => values.iter().map(|v| v.as_i64() as u8).collect(),
        _ => return Err("Starfield family cloth has no bytes".into()),
    };
    let size = fields
        .get("Data Size")
        .ok_or("Starfield family cloth has no byte count")?
        .as_usize();
    bytes
        .get(..size)
        .map(|v| v.to_vec())
        .ok_or("Starfield family cloth byte count exceeds storage".into())
}

fn deformed_bones(owner: &NifFile) -> Result<BTreeSet<String>, String> {
    let mut result = BTreeSet::new();
    for block in owner
        .blocks
        .iter()
        .filter(|b| b.type_name == "BSClothExtraData")
    {
        let bytes = cloth_bytes(
            block
                .get_field("Binary Data")
                .ok_or("Starfield family cloth has no data")?,
        )?;
        let tag = havok_native::hkx::parse_tagfile(&bytes).map_err(|e| e.to_string())?;
        if tag.sdk_version != "20190200" {
            return Err("Starfield family cloth has unsupported SDK".into());
        }
        let file = tag.materialize_hkx().map_err(|e| e.to_string())?;
        let skeletons: Vec<_> = file
            .objects()
            .iter()
            .filter(|o| o.class_name == "hkaSkeleton")
            .collect();
        if skeletons.len() != 1 {
            return Err("Starfield family cloth requires one skeleton".into());
        }
        let bones = hk_array(hk_field(&skeletons[0].members, "bones")?)?;
        for deform in file
            .objects()
            .iter()
            .filter(|o| o.class_name == "hclSimpleMeshBoneDeformOperator")
        {
            for pair in hk_array(hk_field(&deform.members, "triangleBonePairs")?)? {
                let offset = hk_field(
                    pair.as_object_members()
                        .ok_or("invalid Starfield family cloth pair")?,
                    "boneOffset",
                )?;
                let offset = match offset {
                    HkxValue::U16(v) => *v as usize,
                    HkxValue::U32(v) => *v as usize,
                    _ => return Err("invalid Starfield family cloth bone offset".into()),
                };
                if offset % 64 != 0 {
                    return Err("unaligned Starfield family cloth bone offset".into());
                }
                let bone = bones
                    .get(offset / 64)
                    .ok_or("Starfield family cloth bone exceeds skeleton")?;
                let HkxValue::String { value: name, .. } = hk_field(
                    bone.as_object_members()
                        .ok_or("invalid Starfield family cloth bone")?,
                    "name",
                )?
                else {
                    return Err("Starfield family cloth bone has no name".into());
                };
                result.insert(name.clone());
            }
        }
    }
    Ok(result)
}

fn palette(nif: &NifFile) -> Result<HashMap<String, Vec<&NifValue>>, String> {
    let mut result: HashMap<String, Vec<&NifValue>> = HashMap::new();
    for shape in nif.blocks.iter().filter(|b| b.type_name == "BSGeometry") {
        let Some(NifValue::Ref(skin)) = shape.get_field("Skin") else {
            continue;
        };
        if *skin < 0 {
            continue;
        }
        let instance = nif
            .get_block(*skin as usize)
            .ok_or("Starfield family skin reference is missing")?;
        let Some(NifValue::Ref(data)) = instance.get_field("Data") else {
            return Err("Starfield family skin has no bone data".into());
        };
        let data = nif
            .get_block(*data as usize)
            .ok_or("Starfield family bone data is missing")?;
        let Some(NifValue::Array(binds)) = data.get_field("Bone List") else {
            return Err("Starfield family skin has no inverse binds".into());
        };
        let Some(NifValue::Array(extras)) = shape.get_field("Extra Data List") else {
            return Err("Starfield family skin has no palette".into());
        };
        let attaches: Vec<_> = extras
            .iter()
            .filter_map(|v| match v {
                NifValue::Ref(id) if *id >= 0 => nif.get_block(*id as usize),
                _ => None,
            })
            .filter(|b| b.type_name == "SkinAttach")
            .collect();
        if attaches.len() != 1 {
            return Err("Starfield family shape requires one SkinAttach".into());
        }
        let Some(NifValue::Array(names)) = attaches[0].get_field("Bones") else {
            return Err("Starfield family SkinAttach has no names".into());
        };
        if names.len() != binds.len() {
            return Err("Starfield family palette/bind counts differ".into());
        }
        for (name, bind) in names.iter().zip(binds) {
            let NifValue::String(name) = name else {
                return Err("Starfield family palette name is invalid".into());
            };
            result
                .entry(name.trim_end_matches('\0').to_string())
                .or_default()
                .push(bind);
        }
    }
    Ok(result)
}

fn same_bind(a: &NifValue, b: &NifValue) -> bool {
    let (NifValue::Struct(a), NifValue::Struct(b)) = (a, b) else {
        return false;
    };
    ["Translation", "Rotation", "Scale"]
        .iter()
        .all(|key| match (a.get(*key), b.get(*key)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        })
}

pub(crate) fn verify_companion_bindings(
    consumer: &NifFile,
    owner: &NifFile,
) -> Result<Vec<String>, String> {
    let custom = deformed_bones(owner)?;
    if custom.is_empty() {
        return Ok(Vec::new());
    }
    let consumer = palette(consumer)?;
    let owner = palette(owner)?;
    let mut verified = Vec::new();
    for name in custom {
        let Some(binds) = consumer.get(&name) else {
            continue;
        };
        let owner_binds = owner
            .get(&name)
            .ok_or_else(|| format!("Starfield cloth owner has no skin witness for {name}"))?;
        if binds
            .iter()
            .any(|bind| owner_binds.iter().any(|owner| !same_bind(bind, owner)))
        {
            return Err(format!(
                "Starfield cloth companion inverse bind conflicts for {name}"
            ));
        }
        verified.push(name);
    }
    Ok(verified)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn companion_bind_equality_ignores_bounds_but_rejects_one_ulp() {
        let mut fields = indexmap::IndexMap::new();
        fields.insert("Translation".into(), NifValue::Vec3([0., 0., 0.]));
        fields.insert(
            "Rotation".into(),
            NifValue::Matrix33([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]),
        );
        fields.insert("Scale".into(), NifValue::Float(1.));
        let a = NifValue::Struct(fields.clone());
        fields.insert("Bounding Sphere".into(), NifValue::Float(42.));
        assert!(same_bind(&a, &NifValue::Struct(fields.clone())));
        fields.insert(
            "Scale".into(),
            NifValue::Float(f64::from_bits(1f64.to_bits() + 1)),
        );
        assert!(!same_bind(&a, &NifValue::Struct(fields)));
    }

    #[test]
    #[ignore = "requires extracted Starfield source outfits"]
    fn real_starfield_cloth_family_inverse_binds_agree() {
        let root =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../extracted/starfield/meshes");
        let cases = [
            (
                "Clothes/outfit_gang_strikers_01/Outfit_Gang_Strikers_UpperBody_Decals_01_M.nif",
                "Clothes/outfit_gang_strikers_01/Outfit_Gang_Strikers_UpperBody_01_M.nif",
                "Cloth_Bone_021",
            ),
            (
                "Clothes/outfit_gang_strikers_01/Outfit_Gang_Strikers_UpperBody_Decal_01_F.nif",
                "Clothes/outfit_gang_strikers_01/Outfit_Gang_Strikers_UpperBody_01_F.nif",
                "Cloth_Bone_021",
            ),
            (
                "Clothes/outfit_ryujin_businesssuit_manager_02/Outfit_Ryujin_BusinessSuit_Manager_UpperBody_02_F_1st.nif",
                "Clothes/outfit_ryujin_businesssuit_manager_02/Outfit_Ryujin_BusinessSuit_Manager_Sleeves_02_F_1st.nif",
                "L_Cloth_Sleeve",
            ),
        ];
        for (consumer, owner, bone) in cases {
            let mut consumer = NifFile::load(root.join(consumer)).unwrap();
            let owner = NifFile::load(root.join(owner)).unwrap();
            assert!(
                verify_companion_bindings(&consumer, &owner)
                    .unwrap()
                    .iter()
                    .any(|name| name == bone)
            );
            let data = consumer
                .blocks
                .iter_mut()
                .find(|b| b.type_name == "BSSkin::BoneData")
                .unwrap();
            if let Some(NifValue::Array(binds)) = data.fields.get_mut("Bone List") {
                for bind in binds {
                    let NifValue::Struct(fields) = bind else {
                        panic!()
                    };
                    fields.insert("Scale".into(), NifValue::Float(2.));
                }
            }
            // The Ryujin custom bones are in its second source geometry.
            if let Some(data) = consumer
                .blocks
                .iter_mut()
                .filter(|b| b.type_name == "BSSkin::BoneData")
                .nth(1)
            {
                if let Some(NifValue::Array(binds)) = data.fields.get_mut("Bone List") {
                    for bind in binds {
                        let NifValue::Struct(fields) = bind else {
                            panic!()
                        };
                        fields.insert("Scale".into(), NifValue::Float(2.));
                    }
                }
            }
            assert!(
                verify_companion_bindings(&consumer, &owner)
                    .unwrap_err()
                    .contains("inverse bind conflicts")
            );
        }
    }
}
