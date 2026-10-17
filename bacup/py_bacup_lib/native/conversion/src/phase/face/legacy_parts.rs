use super::*;
use crate::ids::SubrecordSig;
use nif_core_native::model::{NifFile, NifValue};

pub(super) fn convert_part(
    source: &Record,
    target_key: FormKey,
    race: RaceClass,
    reference_spec: &HeadPartSpec,
    source_extracted: &Path,
    target_extracted: &Path,
    mod_path: &Path,
    interner: &StringInterner,
) -> Result<(FacePartAsset, Record), String> {
    let source_model = source
        .fields
        .iter()
        .find_map(|field| {
            if field.sig.0 != *b"MODL" {
                return None;
            }
            match field.value {
                FieldValue::String(value) => interner.resolve(value),
                _ => None,
            }
        })
        .ok_or_else(|| "legacy head part has no model".to_string())?;
    let model = source_model.replace('\\', "/");
    let model = model
        .strip_prefix("Meshes/")
        .or_else(|| model.strip_prefix("meshes/"))
        .unwrap_or(&model);
    let beard = source.sig.0 == *b"HDPT";
    let mut source_nif = NifFile::load(source_extracted.join("Meshes").join(model))
        .map_err(|error| format!("load legacy head part {source_model}: {error}"))?;
    if nif_core_native::convert_file::prepare_legacy_face_part_for_fo4(&mut source_nif) == 0 {
        return Err(format!("no convertible geometry in {source_model}"));
    }
    let reference = NifFile::load(target_extracted.join(&reference_spec.model_path))
        .map_err(|error| format!("load head-part skin reference: {error}"))?;
    let schema = nif_core_native::schema::NifSchema::from_generated();
    let mut shapes = source_nif
        .blocks
        .iter()
        .filter(|block| {
            schema.is_subtype_of(&block.type_name, "BSTriShape")
                && matches!(block.get_field("Vertex Data"), Some(NifValue::Array(_)))
        })
        .map(|block| block.block_id)
        .collect::<Vec<_>>();
    let no_hat = shapes
        .iter()
        .copied()
        .filter(|id| {
            matches!(source_nif.blocks[*id].get_field("Name"), Some(NifValue::String(name))
            if !name.eq_ignore_ascii_case("Hat"))
        })
        .collect::<Vec<_>>();
    if !no_hat.is_empty() {
        shapes = no_hat;
    }
    if shapes.is_empty() {
        return Err(format!("no head-part geometry in {source_model}"));
    }
    for shape in &shapes {
        nif_core_native::processors::bake_shape_hierarchy_transform(&mut source_nif, *shape)?;
    }
    let plugin = interner
        .resolve(target_key.plugin)
        .ok_or("unresolved output plugin")?;
    let namespace = Path::new(plugin)
        .file_stem()
        .and_then(|name| name.to_str())
        .ok_or("invalid output plugin")?;
    copy_textures(&mut source_nif, source_extracted, mod_path, namespace)?;
    let name = format!("B21_LegacyHeadPart_{:06X}", target_key.local);
    let mut output = reference.clone();
    let reference_shapes = output
        .blocks
        .iter()
        .filter(|block| schema.is_subtype_of(&block.type_name, "BSTriShape"))
        .map(|block| block.block_id)
        .collect::<Vec<_>>();
    output.remove_blocks(&reference_shapes);
    let root = output
        .header
        .footer_roots
        .first()
        .copied()
        .filter(|id| *id >= 0)
        .ok_or("head-part reference has no root")? as usize;
    output.blocks[root].set_field("Name", NifValue::String("BSFaceGenNiNodeSkinned".into()));
    let mut shape_ids = Vec::new();
    for (index, source_shape) in shapes.into_iter().enumerate() {
        let shape_name = if index == 0 {
            name.clone()
        } else {
            format!("{name}:{index}")
        };
        let copied =
            append_face_part_shape(&mut output, &source_nif, source_shape, root, &shape_name)?;
        nif_core_native::skin::rig_facegen_shape_from_reference(&mut output, copied, &reference)?;
        shape_ids.push(copied);
    }
    nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut output);
    // Standalone HDPT models need dynamic tint; NPC FaceGeom bakes that color later.
    for shape in &shape_ids {
        let shader = shader_id(&output, *shape).ok_or("head part has no shader")?;
        let shader = &mut output.blocks[shader];
        let flags = shader
            .get_field("Shader Flags 1")
            .map(NifValue::as_i64)
            .unwrap_or(0) as u64;
        shader.set_field("Shader Type", NifValue::UInt(6));
        shader.set_field("Hair Tint Color", NifValue::Color3([1.0; 3]));
        shader.set_field("Shader Flags 1", NifValue::UInt(flags | (1 << 18)));
        shader.set_field("Shader Flags 1:FO4", NifValue::UInt(flags | (1 << 18)));
    }
    let relative = format!("{namespace}/HeadParts/{name}.nif");
    let path = mod_path.join("data/Meshes").join(&relative);
    std::fs::create_dir_all(path.parent().ok_or("head-part output has no parent")?)
        .map_err(|error| error.to_string())?;
    output.save(Some(path)).map_err(|error| error.to_string())?;
    let record = head_part_record(target_key, &name, &relative, race, beard, interner);
    Ok((
        FacePartAsset {
            spec: HeadPartSpec {
                plugin: plugin.to_string(),
                local: target_key.local,
                model_path: format!("Meshes/{relative}"),
                source_model_path: None,
                shape_name: name,
            },
            nif: output,
            shape_ids,
            skin_reference: None,
        },
        record,
    ))
}

fn shader_id(nif: &NifFile, shape: usize) -> Option<usize> {
    match nif.blocks.get(shape)?.get_field("Shader Property")? {
        NifValue::Ref(id) if *id >= 0 => Some(*id as usize),
        _ => None,
    }
}

pub(super) fn apply_baked_color(nif: &mut NifFile, shape: usize, color: [u8; 3]) {
    let color = color.map(|component| component as f32 / 255.0);
    if let Some(shader) = shader_id(nif, shape)
        && nif.blocks[shader]
            .get_field("Shader Type")
            .map(NifValue::as_i64)
            == Some(6)
    {
        nif.blocks[shader].set_field("Hair Tint Color", NifValue::Color3(color));
        return;
    }
    let Some(NifValue::Array(vertices)) = nif.blocks[shape].get_field_mut("Vertex Data") else {
        return;
    };
    // Baked FO4 Default hair reads its tint from vertex colors.
    for vertex in vertices {
        if let NifValue::Struct(fields) = vertex
            && let Some(NifValue::Color4(tint)) = fields.get_mut("Vertex Colors")
        {
            for channel in 0..3 {
                tint[channel] *= color[channel];
            }
        }
    }
}

fn head_part_record(
    key: FormKey,
    name: &str,
    model: &str,
    race: RaceClass,
    beard: bool,
    interner: &StringInterner,
) -> Record {
    let mut record = Record::new(SigCode(*b"HDPT"), key);
    record.eid = Some(interner.intern(name));
    let female = matches!(
        race,
        RaceClass::HumanFemale | RaceClass::GhoulFemale | RaceClass::ChildFemale
    );
    record.fields = vec![
        FieldEntry {
            sig: SubrecordSig(*b"EDID"),
            value: FieldValue::String(interner.intern(name)),
        },
        FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::String(interner.intern(&model.replace('/', "\\"))),
        },
        FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Uint((if female { 4 } else { 2 }) | if beard { 16 } else { 0 }),
        },
        FieldEntry {
            sig: SubrecordSig(*b"PNAM"),
            value: FieldValue::Uint(if beard { 4 } else { 3 }),
        },
    ]
    .into();
    record
}

fn copy_textures(
    nif: &mut NifFile,
    source: &Path,
    output: &Path,
    namespace: &str,
) -> Result<(), String> {
    for block in &mut nif.blocks {
        if block.type_name != "BSShaderTextureSet" {
            continue;
        }
        let Some(NifValue::Array(textures)) = block.get_field_mut("Textures") else {
            continue;
        };
        for texture in textures.iter_mut() {
            let NifValue::String(path) = texture else {
                continue;
            };
            if path.is_empty() {
                continue;
            }
            let relative = path.replace('\\', "/");
            let relative = relative
                .strip_prefix("textures/")
                .or_else(|| relative.strip_prefix("Textures/"))
                .unwrap_or(&relative);
            let target_relative = format!("{namespace}/HeadParts/{relative}");
            let destination = output.join("data/Textures").join(&target_relative);
            std::fs::create_dir_all(destination.parent().ok_or("texture has no parent")?)
                .map_err(|error| error.to_string())?;
            std::fs::copy(source.join("Textures").join(relative), &destination)
                .map_err(|error| format!("copy head-part texture {relative}: {error}"))?;
            *path = format!("textures\\{}", target_relative.replace('/', "\\"));
        }
        let Some(NifValue::String(normal)) = textures.get(1) else {
            continue;
        };
        let Some((base, suffix)) = normal.rsplit_once('_') else {
            continue;
        };
        if !suffix.eq_ignore_ascii_case("n.dds") {
            continue;
        }
        let specular = format!("{base}_s.dds");
        let normal_path = output.join("data").join(normal.replace('\\', "/"));
        let image = directxtex_native::read_dds_float_rgba_image(&normal_path)?;
        // FNV stores specular in normal alpha; FO4 reads a separate spec-gloss map.
        let converted =
            materials_native::texture_convert::gamebryo_normal_envmask_to_fo4_specgloss_pixels(
                &image.rgba,
                None,
                image.width as usize,
                image.height as usize,
                0,
                0,
                Default::default(),
            )
            .map_err(|error| error.to_string())?;
        for (path, pixels) in [
            (normal_path, converted.normal),
            (
                output.join("data").join(specular.replace('\\', "/")),
                converted.specgloss,
            ),
        ] {
            directxtex_native::write_dds_float_rgba_image(
                &path,
                image.width,
                image.height,
                &pixels,
                "BC5_UNORM",
                true,
            )?;
        }
        textures.resize_with(textures.len().max(8), || NifValue::String(String::new()));
        textures[7] = NifValue::String(specular);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converted_part_records_reference_own_models_and_correct_sex_and_type() {
        let interner = StringInterner::new();
        let key = FormKey {
            plugin: interner.intern("FNV_FO3.esm"),
            local: 0x800,
        };
        for (race, beard, flags, kind) in [
            (RaceClass::HumanFemale, false, 4, 3),
            (RaceClass::HumanMale, true, 18, 4),
        ] {
            let record = head_part_record(
                key,
                "B21_LegacyHeadPart_000800",
                "FNV_FO3/HeadParts/000800.nif",
                race,
                beard,
                &interner,
            );
            assert_eq!(record.form_key, key);
            let value = |sig| {
                &record
                    .fields
                    .iter()
                    .find(|field| field.sig.0 == sig)
                    .unwrap()
                    .value
            };
            assert_eq!(value(*b"DATA"), &FieldValue::Uint(flags));
            assert_eq!(value(*b"PNAM"), &FieldValue::Uint(kind));
            assert_eq!(
                value(*b"MODL"),
                &FieldValue::String(interner.intern("FNV_FO3\\HeadParts\\000800.nif"))
            );
        }
    }

    #[test]
    #[ignore = "requires extracted FNV and FO4 assets"]
    fn extracted_legacy_hair_and_beard_convert_with_textures_and_head_binds() {
        let extracted = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../extracted");
        let source = extracted.join("fnv");
        let target = extracted.join("fo4");
        let output = tempfile::tempdir().unwrap();
        let interner = StringInterner::new();
        let table = HairTable::load(EMBEDDED_HAIR_LOOKUP_YAML).unwrap();
        for (index, model, race, hair, beard) in [
            (
                0,
                "Characters/Hair/HairBun.NIF",
                RaceClass::HumanFemale,
                "032D37",
                false,
            ),
            (
                1,
                "Characters/Hair/HairBaseOld.NIF",
                RaceClass::HumanMale,
                "0987D9",
                false,
            ),
            (
                2,
                "Characters/Hair/BeardFullOld.NIF",
                RaceClass::HumanMale,
                "09F923",
                true,
            ),
        ] {
            let hair = HairRef {
                plugin: "FalloutNV.esm".into(),
                object_id: hair.into(),
            };
            let reference = if beard {
                target_beard_spec(&hair, race).unwrap()
            } else {
                target_hair_spec(&table.lookup(Some(&hair), race).unwrap()).unwrap()
            };
            let mut source_record = Record::new(
                SigCode(if beard { *b"HDPT" } else { *b"HAIR" }),
                FormKey {
                    plugin: interner.intern("FalloutNV.esm"),
                    local: u32::from_str_radix(&hair.object_id, 16).unwrap(),
                },
            );
            source_record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"MODL"),
                value: FieldValue::String(interner.intern(model)),
            });
            let key = FormKey {
                plugin: interner.intern("FNV_FO3.esm"),
                local: 0x800 + index,
            };
            let (asset, record) = convert_part(
                &source_record,
                key,
                race,
                &reference,
                &source,
                &target,
                output.path(),
                &interner,
            )
            .unwrap();
            assert_eq!(record.form_key, key);
            assert!(!asset.shape_ids.is_empty());
            assert!(asset.spec.plugin.eq_ignore_ascii_case("FNV_FO3.esm"));
            let saved = output.path().join("data").join(&asset.spec.model_path);
            let saved = NifFile::load(saved).unwrap();
            for block in saved
                .blocks
                .iter()
                .filter(|block| block.type_name == "BSShaderTextureSet")
            {
                if let Some(NifValue::Array(textures)) = block.get_field("Textures") {
                    for texture in textures {
                        let NifValue::String(path) = texture else {
                            continue;
                        };
                        if path.is_empty() {
                            continue;
                        }
                        let relative = path.replace('\\', "/");
                        assert!(
                            output.path().join("data").join(&relative).is_file()
                                || target.join(&relative).is_file(),
                            "missing {model} texture: {path}"
                        );
                    }
                }
            }
            let binds = nif_core_native::skeleton_repose::collect_bind_matrices_by_name(&saved);
            let reference = NifFile::load(target.join(&reference.model_path)).unwrap();
            let expected =
                nif_core_native::skeleton_repose::collect_bind_matrices_by_name(&reference);
            assert!(!binds.is_empty());
            for (name, matrix) in binds {
                let expected = expected.get(&name).unwrap();
                assert!(
                    matrix
                        .iter()
                        .flatten()
                        .zip(expected.iter().flatten())
                        .all(|(left, right)| (left - right).abs() < 0.001)
                );
            }
            let mut baked = asset.nif.clone();
            for shape in &asset.shape_ids {
                let shader = shader_id(&baked, *shape).unwrap();
                assert_eq!(
                    baked.blocks[shader]
                        .get_field("Shader Type")
                        .unwrap()
                        .as_i64(),
                    6
                );
            }
            nif_core_native::convert_file::finalize_assembled_facegeom_for_fo4(&mut baked);
            for shape in &asset.shape_ids {
                let vertex_color = |nif: &NifFile| {
                    let Some(NifValue::Array(vertices)) =
                        nif.blocks[*shape].get_field("Vertex Data")
                    else {
                        panic!("vertices")
                    };
                    let Some(NifValue::Struct(fields)) = vertices.first() else {
                        panic!("vertex")
                    };
                    let Some(NifValue::Color4(color)) = fields.get("Vertex Colors") else {
                        panic!("{model}: no baked hair colors")
                    };
                    *color
                };
                let before = vertex_color(&baked);
                apply_baked_color(&mut baked, *shape, [64, 128, 255]);
                let after = vertex_color(&baked);
                assert!((after[0] - before[0] * 64.0 / 255.0).abs() < 1e-6);
                assert!((after[1] - before[1] * 128.0 / 255.0).abs() < 1e-6);
                assert_eq!(&after[2..], &before[2..]);
            }
            for shape in &asset.shape_ids {
                let Some(NifValue::Array(vertices)) =
                    asset.nif.blocks[*shape].get_field("Vertex Data")
                else {
                    panic!("vertices");
                };
                let z = vertices
                    .iter()
                    .filter_map(|vertex| match vertex {
                        NifValue::Struct(fields) => fields.get("Vertex"),
                        _ => None,
                    })
                    .filter_map(|value| match value {
                        NifValue::Vec3(value) => Some(value[2]),
                        NifValue::Struct(fields) => match fields.get("z") {
                            Some(NifValue::Float(value)) => Some(*value as f32),
                            _ => None,
                        },
                        _ => None,
                    })
                    .collect::<Vec<_>>();
                assert!(!z.is_empty());
                assert!(z.iter().all(|z| *z > -30.0 && *z < 30.0), "{model}: {z:?}");
            }
        }
    }
}
