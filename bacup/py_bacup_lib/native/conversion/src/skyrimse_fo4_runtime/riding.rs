use std::path::Path;

use nif_core_native::model::{NifFile, NifValue};
use serde::Serialize;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::run::ConversionRun;
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::Game;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub(crate) struct RidingRace {
    race: String,
    source_race: String,
    source_body_part_data: String,
    source_skeleton: String,
    target_skeleton: String,
    saddle_node: String,
    mount_offset: [f32; 3],
    dismount_offset: [f32; 3],
    camera_offset: [f32; 3],
}

#[derive(Serialize)]
pub(crate) struct RidingRuntimeOutput {
    version: u32,
    rider_enter_idle: String,
    rider_exit_idle: String,
    mount_keyword: String,
    rider_mount_heading_radians: f32,
    races: Vec<RidingRace>,
    #[serde(skip)]
    plugin: String,
}

impl RidingRuntimeOutput {
    pub(crate) fn write(&self, mod_path: &Path) -> Result<(), String> {
        let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime/Riding");
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(directory.join(format!("{}.json", self.plugin)), bytes)
            .map_err(|error| error.to_string())
    }
}

pub(crate) fn prepare_riding_runtime_output(
    run: &ConversionRun,
    source_data_root: &Path,
    target_data_root: &Path,
    rider_enter_idle: FormKey,
    rider_exit_idle: FormKey,
    mount_keyword: FormKey,
    rider_mount_heading_radians: f32,
) -> Result<Option<RidingRuntimeOutput>, String> {
    if run.source != Game::SkyrimSe || run.target != Game::Fo4 {
        return Ok(None);
    }
    if !rider_mount_heading_radians.is_finite() {
        return Err("Source rider entry heading must be finite".into());
    }
    let mapper = run
        .mapper_state
        .as_ref()
        .ok_or("Riding mapper is unavailable")?;
    let output_plugin = run.interner.intern(&run.config.output_plugin_name);
    let mut races = Vec::new();
    for source_key in iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"RACE"), &run.interner)
        .map_err(|error| error.to_string())?
    {
        let source = read_record_relayout_by_form_key(
            run.source_handle_id,
            &source_key,
            &run.schema_source,
            &run.interner,
            None,
        )
        .map_err(|error| error.to_string())?;
        if !is_horse_skeleton(&source, &run.interner) {
            continue;
        }
        let Some(target_key) = mapper
            .source_to_target
            .get(&source_key)
            .filter(|target| target.plugin == output_plugin)
        else {
            continue;
        };
        let Some(target) = run
            .read_target_record_if_available(*target_key)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if target.sig.0 != *b"RACE" {
            return Err(format!(
                "Horse RACE {} mapped to a non-RACE",
                source_key.format(&run.interner)
            ));
        }
        let Some(FieldValue::FormKey(body_key)) = field(&source, b"GNAM") else {
            return Err(format!(
                "Horse {} has no source BPTD",
                source_key.format(&run.interner)
            ));
        };
        let source_skeleton = skeleton_path(&source, &run.interner)?;
        let target_skeleton = skeleton_path(&target, &run.interner)?;
        let source_nif = NifFile::load(mesh_path(source_data_root, &source_skeleton))
            .map_err(|error| error.to_string())?;
        let target_nif = NifFile::load(mesh_path(target_data_root, &target_skeleton))
            .map_err(|error| error.to_string())?;
        validate_saddle_conversion(&source_nif, &target_nif)?;
        races.push(project_riding_race(
            &source,
            *body_key,
            *target_key,
            source_skeleton,
            target_skeleton,
            &run.interner,
        )?);
    }
    if races.is_empty() {
        return Ok(None);
    }
    for idle in [rider_enter_idle, rider_exit_idle] {
        if idle.plugin != output_plugin
            || run
                .read_target_record_if_available(idle)
                .map_err(|error| error.to_string())?
                .is_none_or(|record| record.sig.0 != *b"IDLE")
        {
            return Err(format!(
                "Converted riding interaction IDLE {} is unavailable",
                idle.format(&run.interner)
            ));
        }
    }
    if mount_keyword.plugin != output_plugin
        || run
            .read_target_record_if_available(mount_keyword)
            .map_err(|error| error.to_string())?
            .is_none_or(|record| record.sig.0 != *b"KYWD")
    {
        return Err("Converted riding target keyword is unavailable".into());
    }
    races.sort_by(|left, right| left.race.cmp(&right.race));
    Ok(Some(RidingRuntimeOutput {
        version: 1,
        rider_enter_idle: runtime_key(rider_enter_idle, &run.interner),
        rider_exit_idle: runtime_key(rider_exit_idle, &run.interner),
        mount_keyword: runtime_key(mount_keyword, &run.interner),
        rider_mount_heading_radians,
        races,
        plugin: run.config.output_plugin_name.clone(),
    }))
}

fn skeleton_path(record: &Record, interner: &StringInterner) -> Result<String, String> {
    let mut paths = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"ANAM")
        .filter_map(|entry| match &entry.value {
            FieldValue::String(value) => {
                interner.resolve(*value).map(|path| path.replace('\\', "/"))
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    paths.sort();
    paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    let [path] = paths.as_slice() else {
        return Err("Riding requires one exact source-owned skeleton per race".into());
    };
    if path.starts_with('/') || path.contains(':') || path.split('/').any(|part| part == "..") {
        return Err("Riding skeleton is not a data-relative mesh path".into());
    }
    Ok(path.clone())
}

fn mesh_path(data_root: &Path, path: &str) -> std::path::PathBuf {
    data_root
        .join("meshes")
        .join(path.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn saddle_signature(nif: &NifFile) -> Result<Vec<(String, [NifValue; 3])>, String> {
    let nodes = nif.blocks.iter().filter(|block| {
        matches!(block.get_field("Name"), Some(NifValue::String(name)) if name == "SaddleBone")
    }).collect::<Vec<_>>();
    let [node] = nodes.as_slice() else {
        return Err("Horse skeleton requires one SaddleBone".into());
    };
    let mut node = *node;
    let mut chain = Vec::new();
    let mut seen = std::collections::HashSet::new();
    loop {
        if !seen.insert(node.block_id) {
            return Err("Saddle parent chain is cyclic".into());
        }
        let Some(NifValue::String(name)) = node.get_field("Name") else {
            return Err("Saddle parent chain has an unnamed node".into());
        };
        let mut transform = [NifValue::Null, NifValue::Null, NifValue::Null];
        for (index, field) in ["Translation", "Rotation", "Scale"].iter().enumerate() {
            transform[index] = node
                .get_field(field)
                .ok_or_else(|| format!("{name} lacks {field}"))?
                .clone();
        }
        chain.push((name.clone(), transform));
        let parents = nif
            .blocks
            .iter()
            .filter(|block| match block.get_field("Children") {
                Some(NifValue::Array(children)) => children
                    .iter()
                    .any(|child| matches!(child, NifValue::Ref(id) if *id == node.block_id as i32)),
                _ => false,
            })
            .collect::<Vec<_>>();
        match parents.as_slice() {
            [] if chain.len() > 1 => return Ok(chain),
            [parent] => node = parent,
            _ => return Err("Horse SaddleBone requires one complete scene parent chain".into()),
        }
    }
}

fn validate_saddle_conversion(source: &NifFile, target: &NifFile) -> Result<(), String> {
    if target.header.bs_version != 130 || saddle_signature(source)? != saddle_signature(target)? {
        return Err("Converted horse changed the source saddle attachment or is not FO4".into());
    }
    Ok(())
}

fn is_horse_skeleton(record: &Record, interner: &StringInterner) -> bool {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"ANAM")
        .any(|entry| {
            let FieldValue::String(value) = &entry.value else {
                return false;
            };
            interner.resolve(*value).is_some_and(|path| {
                path.replace('\\', "/")
                    .split('/')
                    .any(|part| part.eq_ignore_ascii_case("horse"))
            })
        })
}

fn project_riding_race(
    source: &Record,
    body: FormKey,
    target: FormKey,
    source_skeleton: String,
    target_skeleton: String,
    interner: &StringInterner,
) -> Result<RidingRace, String> {
    let data = field(source, b"DATA").ok_or("Horse RACE has no DATA")?;
    let vector = |prefix: &str, byte_offset: usize| -> Result<[f32; 3], String> {
        let mut result = [0.0; 3];
        for (index, axis) in ["x", "y", "z"].into_iter().enumerate() {
            let name = format!("mount_data_{prefix}_{axis}");
            let value = match data {
                FieldValue::Struct(fields) => fields
                    .iter()
                    .find_map(|(key, value)| {
                        (interner
                            .resolve(*key)
                            .is_some_and(|key| normalize(key) == normalize(&name)))
                        .then_some(value)
                        .and_then(|value| match value {
                            FieldValue::Float(value) => Some(*value),
                            _ => None,
                        })
                    })
                    .ok_or_else(|| format!("Horse RACE.DATA lacks source {name}"))?,
                // Skyrim's generated RACE schema places the nine mount floats after flags_2.
                FieldValue::Bytes(bytes) if bytes.len() == 164 => {
                    let offset = byte_offset + index * 4;
                    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
                }
                _ => return Err("Horse RACE.DATA has an unsupported source layout".into()),
            };
            if !value.is_finite() {
                return Err(format!("Horse RACE.DATA has non-finite {name}"));
            }
            result[index] = value;
        }
        Ok(result)
    };
    Ok(RidingRace {
        race: runtime_key(target, interner),
        source_race: runtime_key(source.form_key, interner),
        source_body_part_data: runtime_key(body, interner),
        source_skeleton,
        target_skeleton,
        saddle_node: "SaddleBone".into(),
        mount_offset: vector("mount_offset", 128)?,
        dismount_offset: vector("dismount_offset", 140)?,
        camera_offset: vector("mount_camera_offset", 152)?,
    })
}

fn runtime_key(key: FormKey, interner: &StringInterner) -> String {
    format!(
        "{:06X}:{}",
        key.local,
        interner.resolve(key.plugin).unwrap()
    )
}

fn field<'a>(record: &'a Record, signature: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| &entry.sig.0 == signature)
        .map(|entry| &entry.value)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::FieldEntry;

    fn fixture(interner: &StringInterner) -> (Record, FormKey, FormKey) {
        let key = |local, plugin: &str| FormKey {
            local,
            plugin: interner.intern(plugin),
        };
        let mut race = Record::new(SigCode(*b"RACE"), key(0x131FD, "Skyrim.esm"));
        let values = [-63.479, 0.0, 0.0, -50.0, 0.0, 65.0, 0.0, -300.0, 0.0];
        let mut fields = Vec::new();
        for (group, prefix) in ["mount_offset", "dismount_offset", "mount_camera_offset"]
            .iter()
            .enumerate()
        {
            for (axis, suffix) in ["x", "y", "z"].iter().enumerate() {
                fields.push((
                    interner.intern(&format!("mount_data_{prefix}_{suffix}")),
                    FieldValue::Float(values[group * 3 + axis]),
                ));
            }
        }
        race.fields.push(FieldEntry {
            sig: SubrecordSig(*b"DATA"),
            value: FieldValue::Struct(fields),
        });
        race.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ANAM"),
            value: FieldValue::String(
                interner.intern("Actors\\Horse\\Character Assets\\skeleton.nif"),
            ),
        });
        (
            race,
            key(0x60716, "Skyrim.esm"),
            key(0x10123, "Converted.esm"),
        )
    }

    #[test]
    fn riding_projection_preserves_source_saddle_and_offsets() {
        let interner = StringInterner::new();
        let (race, body, target) = fixture(&interner);
        assert!(is_horse_skeleton(&race, &interner));
        let value = project_riding_race(
            &race,
            body,
            target,
            "source.nif".into(),
            "target.nif".into(),
            &interner,
        )
        .unwrap();
        assert_eq!(value.mount_offset, [-63.479, 0.0, 0.0]);
        assert_eq!(value.dismount_offset, [-50.0, 0.0, 65.0]);
        assert_eq!(value.camera_offset, [0.0, -300.0, 0.0]);
        assert_eq!(value.race, "010123:Converted.esm");
        assert_eq!(value.saddle_node, "SaddleBone");
    }

    #[test]
    fn riding_projection_reads_official_horse_raw_data() {
        let interner = StringInterner::new();
        let (mut race, body, target) = fixture(&interner);
        let bytes = "FF00FF00FF00FF00FF00FF00FF0000000000803F0000803F0000803F0000803F0889500000007A43000000000000C84200C07944000060400000803F0000803F01000000FFFFFFFFFFFFFFFF00000000FFFFFFFF0000803F0000000000002041000020410000C0420000000000000000000000000000803E0000A040000000007FEA7DC20000000000000000000048C2000000000000824200000000000096C300000000";
        race.fields[0].value = FieldValue::Bytes(
            bytes
                .as_bytes()
                .chunks_exact(2)
                .map(|chunk| u8::from_str_radix(std::str::from_utf8(chunk).unwrap(), 16).unwrap())
                .collect(),
        );
        let value = project_riding_race(
            &race,
            body,
            target,
            "source.nif".into(),
            "target.nif".into(),
            &interner,
        )
        .unwrap();
        assert_eq!(value.mount_offset, [-63.479, 0.0, 0.0]);
        assert_eq!(value.dismount_offset, [-50.0, 0.0, 65.0]);
        assert_eq!(value.camera_offset, [0.0, -300.0, 0.0]);
        let FieldValue::Bytes(bytes) = &mut race.fields[0].value else {
            panic!()
        };
        bytes.pop();
        assert!(
            project_riding_race(
                &race,
                body,
                target,
                "source.nif".into(),
                "target.nif".into(),
                &interner
            )
            .is_err()
        );
    }

    #[test]
    fn riding_projection_requires_complete_offset_evidence() {
        let interner = StringInterner::new();
        let (mut race, body, target) = fixture(&interner);
        let FieldValue::Struct(data) = &mut race.fields[0].value else {
            panic!()
        };
        data.pop();
        assert!(
            project_riding_race(
                &race,
                body,
                target,
                "source.nif".into(),
                "target.nif".into(),
                &interner
            )
            .is_err()
        );
    }

    #[test]
    fn riding_saddle_requires_preserved_source_parent_and_transform() {
        use nif_core_native::model::NifBlock;
        let mut source = NifFile::new("skyrimse");
        source.blocks.clear();
        let mut parent = NifBlock::new(0, "NiNode");
        parent.set_field("Name", NifValue::String("HorseSpine2".into()));
        parent.set_field("Children", NifValue::Array(vec![NifValue::Ref(1)]));
        parent.set_field("Translation", NifValue::Vec3([0.0; 3]));
        parent.set_field(
            "Rotation",
            NifValue::Matrix33([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        );
        parent.set_field("Scale", NifValue::Float(1.0));
        let mut saddle = NifBlock::new(1, "NiNode");
        saddle.set_field("Name", NifValue::String("SaddleBone".into()));
        saddle.set_field("Translation", NifValue::Vec3([0.0, 2.0, 4.0]));
        saddle.set_field(
            "Rotation",
            NifValue::Matrix33([[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]]),
        );
        saddle.set_field("Scale", NifValue::Float(1.0));
        source.blocks.extend([parent, saddle]);
        let mut target = NifFile::new("fo4");
        target.blocks = source.blocks.clone();
        validate_saddle_conversion(&source, &target).unwrap();
        target.blocks[1].set_field("Translation", NifValue::Vec3([0.0, 0.0, 0.0]));
        assert!(validate_saddle_conversion(&source, &target).is_err());
        target.blocks = source.blocks.clone();
        target.blocks[0].set_field("Name", NifValue::String("Root".into()));
        assert!(validate_saddle_conversion(&source, &target).is_err());
        target.blocks = source.blocks.clone();
        target.blocks[0].set_field("Translation", NifValue::Vec3([0.0, 0.0, 10.0]));
        assert!(validate_saddle_conversion(&source, &target).is_err());
        target.blocks.clear();
        assert!(validate_saddle_conversion(&source, &target).is_err());
    }

    #[test]
    #[ignore = "requires an extracted Skyrim horse skeleton"]
    fn source_horse_saddle_is_a_real_scene_node() {
        let path = std::env::var("MODKIT_RIDING_SOURCE_NIF").unwrap();
        let source = NifFile::load(path).unwrap();
        let chain = saddle_signature(&source).unwrap();
        assert_eq!(chain[1].0, "HorseSpine2");
        assert!(matches!(chain[0].1[0], NifValue::Vec3(_)));
    }
}
