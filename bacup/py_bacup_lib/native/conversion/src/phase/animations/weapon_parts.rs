use std::collections::{HashMap, HashSet};

use super::NifBlockExt;
use indexmap::IndexMap;
use nif_core_native::model::{NifFile, NifValue};
use serde::Serialize;

#[path = "weapon_parts_graph.rs"]
mod graph;
#[path = "legacy_weapon_parts.rs"]
mod phase;
pub use phase::ConvertLegacyWeaponPartsPhase;
#[cfg(test)]
#[path = "creature_properties_tests.rs"]
mod creature_properties_tests;
#[cfg(test)]
pub(super) use phase::stage_model_parts;

pub(super) fn attach_empty_idle(weapon: &mut NifFile) -> Result<(), String> {
    let (manager, _, _) = ensure_controller_manager(weapon)?;
    let sequence = weapon.add_block("NiControllerSequence", None);
    for (field, value) in [
        ("Name", NifValue::String("LegacyIdle".into())),
        ("Manager", NifValue::Ref(manager as i32)),
        ("Frequency", NifValue::Float(1.0)),
        ("Weight", NifValue::Float(1.0)),
        ("Start Time", NifValue::Float(0.0)),
        ("Stop Time", NifValue::Float(0.0333333)),
        ("Cycle Type", NifValue::UInt(0)),
    ] {
        weapon.blocks[sequence].set_field(field, value);
    }
    let mut sequences = array(weapon.blocks[manager].get_field("Controller Sequences")).to_vec();
    sequences.push(NifValue::Ref(sequence as i32));
    set_array(
        weapon,
        manager,
        "Num Controller Sequences",
        "Controller Sequences",
        sequences,
    );
    Ok(())
}

pub(super) fn attach_behavior(weapon: &mut NifFile, project: &str) -> Result<(), String> {
    let root = weapon
        .header
        .footer_roots
        .first()
        .copied()
        .filter(|id| *id >= 0)
        .ok_or("weapon NIF has no root")? as usize;
    if weapon
        .blocks
        .iter()
        .any(|block| block.type_name == "BSBehaviorGraphExtraData")
    {
        return Err(
            "weapon already has an FX behavior; merging its moving-part graph is required".into(),
        );
    }
    let bged = weapon.add_block("BSBehaviorGraphExtraData", None);
    weapon.blocks[bged].set_field("Name", NifValue::String("BGED".into()));
    weapon.blocks[bged].set_field(
        "Behaviour Graph File",
        NifValue::String(project.replace('/', "\\")),
    );
    weapon.blocks[bged].set_field("Controls Base Skeleton", NifValue::Bool(false));
    let mut extras = array(weapon.blocks[root].get_field("Extra Data List")).to_vec();
    extras.push(NifValue::Ref(bged as i32));
    set_array(
        weapon,
        root,
        "Num Extra Data List",
        "Extra Data List",
        extras,
    );
    if let Some(block) = weapon
        .blocks
        .iter_mut()
        .find(|block| block.type_name == "BSXFlags")
    {
        let flags = match block.get_field("Integer Data") {
            Some(NifValue::UInt(value)) => *value,
            Some(NifValue::Int(value)) => *value as u64,
            _ => 0,
        };
        block.set_field("Integer Data", NifValue::UInt(flags | 1));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct WeaponPartSequenceReceipt {
    pub(crate) sequence_name: String,
    pub(crate) matched_tracks: Vec<String>,
    pub(crate) neutral_shader_tracks: Vec<String>,
    pub(crate) unmatched_part_tracks: Vec<String>,
    pub(crate) duplicate_target_names: Vec<String>,
    pub(crate) start_seconds: f64,
    pub(crate) end_seconds: f64,
    pub(crate) playback_speed: f64,
    pub(crate) looping: bool,
}

pub(crate) fn attach_weapon_part_sequence(
    weapon: &mut NifFile,
    kf: &NifFile,
    sequence_index: usize,
    output_name: &str,
    start_seconds: f64,
    end_seconds: Option<f64>,
    playback_speed: f64,
    looping: bool,
) -> Result<WeaponPartSequenceReceipt, String> {
    attach_controller_sequence(weapon, kf, sequence_index, output_name, start_seconds, end_seconds, playback_speed, looping, false, None)
}

pub(crate) fn attach_named_weapon_part_sequence(
    weapon: &mut NifFile,
    kf: &NifFile,
    output_name: &str,
    playback_speed: f64,
    names: &HashSet<String>,
) -> Result<WeaponPartSequenceReceipt, String> {
    attach_controller_sequence(weapon, kf, 0, output_name, 0.0, None, playback_speed, false, false, Some(names))
}

pub(crate) fn build_named_weapon_part_behavior(sequence: &str, triggers: Vec<String>) -> Result<Vec<u8>, String> {
    graph::build_behavior(&[
        graph::PartGraphState { sequence: "LegacyIdle".into(), triggers: vec![], exits: vec![], exit_sequence: "LegacyIdle".into() },
        graph::PartGraphState { sequence: sequence.into(), triggers, exits: vec!["end".into()], exit_sequence: "LegacyIdle".into() },
    ])
}

pub(crate) fn attach_creature_property_sequence(
    target: &mut NifFile,
    kf: &NifFile,
    sequence_index: usize,
    output_name: &str,
    start_seconds: f64,
    end_seconds: Option<f64>,
    playback_speed: f64,
    looping: bool,
) -> Result<WeaponPartSequenceReceipt, String> {
    attach_controller_sequence(target, kf, sequence_index, output_name, start_seconds, end_seconds, playback_speed, looping, true, None)
}

pub(crate) fn instantiate_creature_property_sequence(
    target: &mut NifFile,
    source_name: &str,
    output_names: &[String],
) -> Result<(), String> {
    if output_names.is_empty() || output_names.iter().collect::<HashSet<_>>().len() != output_names.len() {
        return Err("creature property sequence instances must have unique nonempty names".into());
    }
    let source = target.clone();
    let source_index = source.blocks.iter().position(|block| block.type_name == "NiControllerSequence" && block.string_field("Name").as_deref() == Some(source_name))
        .ok_or_else(|| format!("creature property sequence is absent: {source_name}"))?;
    if output_names.iter().any(|name| name.is_empty() || target.blocks.iter().enumerate().any(|(index, block)| index != source_index && block.type_name == "NiControllerSequence" && block.string_field("Name").as_deref() == Some(name))) {
        return Err("creature property sequence instance name already exists".into());
    }
    let sequence = &source.blocks[source_index];
    let manager = reference(sequence.get_field("Manager")).ok_or("creature property sequence has no manager")?;
    target.blocks[source_index].set_field("Name", NifValue::String(output_names[0].clone()));
    let mut sequences = array(target.blocks[manager].get_field("Controller Sequences")).to_vec();
    for output_name in &output_names[1..] {
        let mut copied = HashMap::new();
        let mut links = array(sequence.get_field("Controlled Blocks")).to_vec();
        for link in &mut links {
            let NifValue::Struct(fields) = link else { return Err("invalid creature controlled block".into()) };
            let interpolator = reference(fields.get("Interpolator")).ok_or("creature controlled block has no interpolator")?;
            let copied_interpolator = copy_interpolator(&source, target, interpolator, &mut copied)?;
            fields.insert("Interpolator".into(), NifValue::Ref(copied_interpolator as i32));
        }
        let output = target.add_block("NiControllerSequence", Some(sequence.fields.clone()));
        target.blocks[output].remainder = sequence.remainder.clone();
        target.blocks[output].set_field("Name", NifValue::String(output_name.clone()));
        set_array(target, output, "Num Controlled Blocks", "Controlled Blocks", links);
        sequences.push(NifValue::Ref(output as i32));
    }
    set_array(target, manager, "Num Controller Sequences", "Controller Sequences", sequences);
    Ok(())
}

fn attach_controller_sequence(
    weapon: &mut NifFile,
    kf: &NifFile,
    sequence_index: usize,
    output_name: &str,
    start_seconds: f64,
    end_seconds: Option<f64>,
    playback_speed: f64,
    looping: bool,
    creature_properties: bool,
    named_parts: Option<&HashSet<String>>,
) -> Result<WeaponPartSequenceReceipt, String> {
    let source = kf
        .blocks
        .iter()
        .filter(|block| block.type_name == "NiControllerSequence")
        .nth(sequence_index)
        .ok_or("weapon KF sequence index is missing")?;
    let source_start = source.f64_field("Start Time").unwrap_or(0.0);
    let source_stop = source
        .f64_field("Stop Time")
        .ok_or("weapon KF is missing Stop Time")?;
    let frequency = source.f64_field("Frequency").unwrap_or(1.0);
    let end_seconds = end_seconds.unwrap_or((source_stop - source_start) / frequency);
    if !frequency.is_finite()
        || frequency <= 0.0
        || !playback_speed.is_finite()
        || playback_speed <= 0.0
        || !start_seconds.is_finite()
        || !end_seconds.is_finite()
        || start_seconds < 0.0
        || end_seconds <= start_seconds
        || source_start + end_seconds * frequency > source_stop + 0.001
    {
        return Err("weapon part sequence has invalid timing".into());
    }
    let mut receipt = WeaponPartSequenceReceipt {
        sequence_name: output_name.into(),
        matched_tracks: vec![],
        neutral_shader_tracks: vec![],
        unmatched_part_tracks: vec![],
        duplicate_target_names: vec![],
        start_seconds,
        end_seconds,
        playback_speed,
        looping,
    };
    let mut targets = HashMap::<String, Vec<usize>>::new();
    for (id, block) in weapon.blocks.iter().enumerate() {
        if let Some(NifValue::String(name)) = block.get_field("Name") {
            if named_parts.map_or(creature_properties || name.starts_with("##"), |names| names.contains(name)) && block.get_field("Translation").is_some() {
                targets
                    .entry(if creature_properties || named_parts.is_some() { name.clone() } else { name.to_ascii_lowercase() })
                    .or_default()
                    .push(id);
            }
        }
    }
    let mut selected = Vec::new();
    if let Some(names) = named_parts {
        for name in names {
            let count = weapon.blocks.iter().filter(|block| block.get_field("Translation").is_some() && matches!(block.get_field("Name"), Some(NifValue::String(candidate)) if candidate.eq_ignore_ascii_case(name))).count();
            if count != 1 {
                return Err(format!("named weapon part {name:?} has missing or case-fold-colliding model targets"));
            }
        }
    }
    let mut source_identities = HashSet::new();
    for link in array(source.get_field("Controlled Blocks")) {
        let NifValue::Struct(fields) = link else {
            continue;
        };
        let Some(NifValue::String(name)) = fields.get("Node Name") else {
            continue;
        };
        if creature_properties && fields.get("Controller Type") == Some(&NifValue::String("NiTransformController".into())) {
            continue;
        }
        if named_parts.is_some_and(|names| !names.contains(name)) || (named_parts.is_none() && !creature_properties && !name.starts_with("##")) {
            continue;
        }
        if named_parts.is_some() && !matches!(fields.get("Controller Type"), Some(NifValue::String(kind)) if matches!(kind.as_str(), "NiTransformController" | "NiVisController")) {
            return Err(format!("named weapon part {name} requires a transform or visibility controller"));
        }
        let target_name = if creature_properties || named_parts.is_some() { name.clone() } else { name.to_ascii_lowercase() };
        let Some(ids) = targets.get(&target_name) else {
            receipt.unmatched_part_tracks.push(name.clone());
            continue;
        };
        if (creature_properties || named_parts.is_some()) && ids.len() != 1 {
            let kind = if creature_properties { "creature property sequence" } else { "named weapon part sequence" };
            return Err(format!("{kind} target {name:?} is ambiguous: {ids:?}"));
        }
        if creature_properties {
            let identity = ["Node Name", "Property Type", "Controller Type", "Controller ID", "Interpolator ID"].map(|field| match fields.get(field) {
                Some(NifValue::String(value)) => value.clone(),
                _ => String::new(),
            });
            if !source_identities.insert(identity.clone()) {
                return Err(format!("duplicate creature property controller identity: {identity:?}"));
            }
        }
        let controller = match fields.get("Controller Type") {
            Some(NifValue::String(kind))
                if matches!(
                    kind.as_str(),
                    "NiTransformController"
                        | "NiVisController"
                        | "NiMaterialColorController"
                        | "BSMaterialEmittanceMultController"
                        | "NiTextureTransformController"
                        | "NiAlphaController"
                        | "BSRefractionFirePeriodController"
                        | "BSRefractionStrengthController"
                ) =>
            {
                kind.as_str()
            }
            value => {
                return Err(format!(
                    "unsupported weapon part controller {name}: {value:?}"
                ));
            }
        };
        let target = *ids
            .iter()
            .max_by_key(|id| array(weapon.blocks[**id].get_field("Children")).len())
            .unwrap();
        if ids.len() > 1 && !receipt.duplicate_target_names.contains(name) {
            // FNV 9mm has an empty magazine attachment and its visible mesh sharing ##nmClip.
            receipt.duplicate_target_names.push(name.clone());
        }
        let source_interpolator = reference(fields.get("Interpolator"))
            .ok_or_else(|| format!("weapon part {name} has no interpolator"))?;
        let identity_rotation = controller == "NiTextureTransformController"
            && fields.get("Controller ID") == Some(&NifValue::String("0-0-TT_ROTATE".into()));
        if identity_rotation || controller == "BSRefractionFirePeriodController" {
            let shader =
                reference(weapon.blocks[target].get_field("Shader Property")).ok_or_else(|| {
                    format!("weapon neutral shader track {name} has no converted shader")
                })?;
            let strength_curves: Vec<_> = array(source.get_field("Controlled Blocks"))
                .iter()
                .filter_map(|link| match link {
                    NifValue::Struct(link)
                        if link.get("Node Name") == Some(&NifValue::String(name.clone()))
                            && link.get("Controller Type")
                                == Some(&NifValue::String(
                                    "BSRefractionStrengthController".into(),
                                )) =>
                    {
                        Some(reference(link.get("Interpolator")))
                    }
                    _ => None,
                })
                .collect();
            let zero_strength = !strength_curves.is_empty()
                && strength_curves
                    .iter()
                    .all(|id| id.is_some_and(|id| zero_float_curve(kf, id)));
            let baseline_neutral = if identity_rotation {
                matches!(
                    weapon.blocks[shader].type_name.as_str(),
                    "BSLightingShaderProperty" | "BSEffectShaderProperty"
                )
            } else {
                zero_strength && weapon.blocks[shader].f64_field("Refraction Strength") == Some(0.0)
            };
            if !baseline_neutral || !zero_float_curve(kf, source_interpolator) {
                return Err(format!(
                    "weapon shader track {name}:{controller} has no proven neutral FO4 lowering"
                ));
            }
            receipt
                .neutral_shader_tracks
                .push(format!("{name}:{controller}:verified_zero_curve"));
            continue;
        }
        selected.push((
            fields.clone(),
            name.clone(),
            controller.to_string(),
            target,
            source_interpolator,
        ));
    }
    if selected.is_empty() {
        return Ok(receipt);
    }
    if (creature_properties || named_parts.is_some()) && weapon.blocks.iter().any(|block| block.type_name == "NiControllerSequence" && block.string_field("Name").as_deref() == Some(output_name)) {
        let kind = if creature_properties { "creature property" } else { "named weapon part" };
        return Err(format!("duplicate {kind} sequence name: {output_name}"));
    }
    let (manager, multi, palette) = ensure_controller_manager(weapon)?;
    let mut copied = HashMap::new();
    let mut links = Vec::new();
    let mut extra_targets = array(weapon.blocks[multi].get_field("Extra Targets")).to_vec();
    let mut objects = array(weapon.blocks[palette].get_field("Objs")).to_vec();
    for (mut fields, name, controller, target, interpolator) in selected {
        let interpolator = copy_interpolator(kf, weapon, interpolator, &mut copied)?;
        let controller_id = if controller == "NiTransformController" {
            if !extra_targets
                .iter()
                .any(|value| reference(Some(value)) == Some(target))
            {
                extra_targets.push(NifValue::Ref(target as i32));
            }
            multi
        } else if controller == "NiVisController" {
            ensure_visibility_controller(weapon, target)
        } else {
            let shader =
                reference(weapon.blocks[target].get_field("Shader Property")).ok_or_else(|| {
                    format!("weapon shader track {name} has no converted shader property")
                })?;
            let (kind, variable_field, variable, blend) =
                shader_controller_binding(&fields, &weapon.blocks[shader].type_name)?;
            fields.insert(
                "Property Type".into(),
                NifValue::String(weapon.blocks[shader].type_name.clone()),
            );
            fields.insert("Controller Type".into(), NifValue::String(kind.into()));
            fields.insert(
                "Controller ID".into(),
                NifValue::String(variable.to_string()),
            );
            ensure_shader_controller(weapon, shader, kind, variable_field, variable, blend)
        };
        if controller != "NiTransformController" {
            let begin = weapon.blocks[controller_id]
                .f64_field("Start Time")
                .unwrap_or(source_start);
            let end = weapon.blocks[controller_id]
                .f64_field("Stop Time")
                .unwrap_or(source_stop);
            weapon.blocks[controller_id]
                .set_field("Start Time", NifValue::Float(begin.min(source_start)));
            weapon.blocks[controller_id]
                .set_field("Stop Time", NifValue::Float(end.max(source_stop)));
        }
        fields.insert("Interpolator".into(), NifValue::Ref(interpolator as i32));
        fields.insert("Controller".into(), NifValue::Ref(controller_id as i32));
        links.push(NifValue::Struct(fields));
        if !objects.iter().any(|value| matches!(value, NifValue::Struct(fields) if matches!(fields.get("Name"),Some(NifValue::String(existing)) if existing.eq_ignore_ascii_case(&name)))) {
            objects.push(NifValue::Struct(IndexMap::from([
                ("Name".into(), NifValue::String(name.clone())),
                ("AV Object".into(), NifValue::Ref(target as i32)),
            ])));
        }
        receipt.matched_tracks.push(format!("{name}:{controller}"));
    }
    set_array(
        weapon,
        multi,
        "Num Extra Targets",
        "Extra Targets",
        extra_targets,
    );
    set_array(weapon, palette, "Num Objs", "Objs", objects);
    let start = source_start + start_seconds * frequency;
    let stop = (source_start + end_seconds * frequency).min(source_stop);
    let text = weapon.add_block("NiTextKeyExtraData", None);
    set_array(
        weapon,
        text,
        "Num Text Keys",
        "Text Keys",
        [(start, "start"), (stop, "end")]
            .into_iter()
            .filter(|_| !creature_properties)
            .map(|(time, value)| {
                NifValue::Struct(IndexMap::from([
                    ("Time".into(), NifValue::Float(time)),
                    ("Value".into(), NifValue::String(value.into())),
                ]))
            })
            .collect(),
    );
    let sequence = weapon.add_block("NiControllerSequence", None);
    for (field, value) in [
        ("Name", NifValue::String(output_name.into())),
        ("Array Grow By", NifValue::UInt(1)),
        ("Weight", NifValue::Float(1.0)),
        ("Text Keys", NifValue::Ref(text as i32)),
        ("Cycle Type", NifValue::UInt(if looping { 0 } else { 2 })),
        ("Frequency", NifValue::Float(frequency * playback_speed)),
        ("Start Time", NifValue::Float(start)),
        ("Stop Time", NifValue::Float(stop)),
        ("Manager", NifValue::Ref(manager as i32)),
        ("Accum Root Name", NifValue::String(if creature_properties || named_parts.is_some() { "" } else { "Weapon" }.into())),
    ] {
        weapon.blocks[sequence].set_field(field, value);
    }
    set_array(
        weapon,
        sequence,
        "Num Controlled Blocks",
        "Controlled Blocks",
        links,
    );
    let mut sequences = array(weapon.blocks[manager].get_field("Controller Sequences")).to_vec();
    sequences.push(NifValue::Ref(sequence as i32));
    set_array(
        weapon,
        manager,
        "Num Controller Sequences",
        "Controller Sequences",
        sequences,
    );
    Ok(receipt)
}

fn shader_controller_binding(
    fields: &IndexMap<String, NifValue>,
    shader_type: &str,
) -> Result<(&'static str, &'static str, u64, &'static str), String> {
    let lighting = match shader_type {
        "BSLightingShaderProperty" => true,
        "BSEffectShaderProperty" => false,
        _ => return Err(format!("unsupported converted weapon shader {shader_type}")),
    };
    let string = |name| match fields.get(name) {
        Some(NifValue::String(value)) => value.as_str(),
        _ => "",
    };
    let controller = string("Controller Type");
    let identifier = string("Controller ID");
    if controller == "NiMaterialColorController" {
        let variable = match identifier {
            "SELF_ILLUM" => u64::from(lighting),
            "SPEC" if lighting => 0,
            _ => {
                return Err(format!(
                    "unsupported source weapon material color {identifier}"
                ));
            }
        };
        return Ok((
            if lighting {
                "BSLightingShaderPropertyColorController"
            } else {
                "BSEffectShaderPropertyColorController"
            },
            "Controlled Color",
            variable,
            "NiBlendPoint3Interpolator",
        ));
    }
    let variable = match controller {
        "BSMaterialEmittanceMultController" => {
            if lighting {
                11
            } else {
                0
            }
        }
        "NiAlphaController" => {
            if lighting {
                12
            } else {
                5
            }
        }
        "BSRefractionStrengthController" if lighting => 0,
        "NiTextureTransformController" => match identifier {
            "0-0-TT_TRANSLATE_U" => {
                if lighting {
                    20
                } else {
                    6
                }
            }
            "0-0-TT_TRANSLATE_V" => {
                if lighting {
                    22
                } else {
                    8
                }
            }
            "0-0-TT_SCALE_U" => {
                if lighting {
                    21
                } else {
                    7
                }
            }
            "0-0-TT_SCALE_V" => {
                if lighting {
                    23
                } else {
                    9
                }
            }
            _ => {
                return Err(format!(
                    "unsupported source weapon texture transform {identifier}"
                ));
            }
        },
        _ => {
            return Err(format!(
                "unsupported source weapon shader controller {controller}"
            ));
        }
    };
    Ok((
        if lighting {
            "BSLightingShaderPropertyFloatController"
        } else {
            "BSEffectShaderPropertyFloatController"
        },
        "Controlled Variable",
        variable,
        "NiBlendFloatInterpolator",
    ))
}

fn zero_float_curve(source: &NifFile, interpolator: usize) -> bool {
    let Some(interpolator) = source
        .blocks
        .get(interpolator)
        .filter(|block| block.type_name == "NiFloatInterpolator")
    else {
        return false;
    };
    let data = match interpolator.get_field("Data") {
        Some(NifValue::Ref(data)) if *data >= 0 => *data as usize,
        None | Some(NifValue::Ref(-1)) => return interpolator.f64_field("Value") == Some(0.0),
        _ => return false,
    };
    let Some(data) = source
        .blocks
        .get(data)
        .filter(|block| block.type_name == "NiFloatData")
    else {
        return false;
    };
    let Some(NifValue::Struct(group)) = data.get_field("Data") else {
        return false;
    };
    let interpolation = match group.get("Interpolation") {
        Some(NifValue::UInt(value)) if matches!(*value, 1 | 2 | 5) => *value,
        _ => return false,
    };
    let keys = array(group.get("Keys"));
    if keys.is_empty() || group.get("Num Keys") != Some(&NifValue::UInt(keys.len() as u64)) {
        return false;
    }
    let times: Vec<_> = keys
        .iter()
        .filter_map(|key| match key {
            NifValue::Struct(key) => match key.get("Time") {
                Some(NifValue::Float(time)) => Some(*time),
                _ => None,
            },
            _ => None,
        })
        .collect();
    if times.len() != keys.len() || !times.windows(2).all(|pair| pair[0] < pair[1]) {
        return false;
    }
    keys.iter().all(|key| {
        matches!(key, NifValue::Struct(key)
        if matches!(key.get("Time"), Some(NifValue::Float(time)) if time.is_finite())
        && key.get("Value") == Some(&NifValue::Float(0.0))
        && (interpolation != 2 || ["Forward", "Backward"].iter()
            .all(|name| key.get(*name) == Some(&NifValue::Float(0.0)))))
    })
}

fn ensure_shader_controller(
    weapon: &mut NifFile,
    target: usize,
    kind: &str,
    variable_field: &str,
    variable: u64,
    blend_kind: &str,
) -> usize {
    let existing = controller_chain(
        weapon,
        reference(weapon.blocks[target].get_field("Controller")),
    )
    .into_iter()
    .find(|id| {
        weapon.blocks[*id].type_name == kind
            && weapon.blocks[*id].get_field(variable_field) == Some(&NifValue::UInt(variable))
    });
    let controller = existing.unwrap_or_else(|| {
        let previous = weapon.blocks[target]
            .get_field("Controller")
            .cloned()
            .unwrap_or(NifValue::Ref(-1));
        let id = weapon.add_block(kind, None);
        for (field, value) in [
            ("Next Controller", previous),
            ("Target", NifValue::Ref(target as i32)),
            ("Flags", NifValue::UInt(108)),
            ("Frequency", NifValue::Float(1.0)),
            (variable_field, NifValue::UInt(variable)),
        ] {
            weapon.blocks[id].set_field(field, value);
        }
        weapon.blocks[target].set_field("Controller", NifValue::Ref(id as i32));
        id
    });
    let flags = match weapon.blocks[controller].get_field("Flags") {
        Some(NifValue::UInt(flags)) => *flags,
        _ => 76,
    };
    weapon.blocks[controller].set_field("Flags", NifValue::UInt(flags | 0x20));
    if reference(weapon.blocks[controller].get_field("Interpolator"))
        .is_none_or(|id| weapon.blocks[id].type_name != blend_kind)
    {
        let blend = weapon.add_block(blend_kind, None);
        weapon.blocks[blend].set_field("Flags", NifValue::UInt(1));
        weapon.blocks[blend].set_field("Array Size", NifValue::UInt(2));
        weapon.blocks[controller].set_field("Interpolator", NifValue::Ref(blend as i32));
    }
    controller
}

fn ensure_controller_manager(weapon: &mut NifFile) -> Result<(usize, usize, usize), String> {
    let root = weapon
        .header
        .footer_roots
        .first()
        .copied()
        .filter(|id| *id >= 0)
        .ok_or("weapon NIF has no root")? as usize;
    let manager = controller_chain(
        weapon,
        reference(weapon.blocks[root].get_field("Controller")),
    )
    .into_iter()
    .find(|id| weapon.blocks[*id].type_name == "NiControllerManager");
    let manager = manager.unwrap_or_else(|| {
        let previous = weapon.blocks[root]
            .get_field("Controller")
            .cloned()
            .unwrap_or(NifValue::Ref(-1));
        let manager = weapon.add_block("NiControllerManager", None);
        weapon.blocks[manager].set_field("Next Controller", previous);
        weapon.blocks[manager].set_field("Target", NifValue::Ref(root as i32));
        weapon.blocks[manager].set_field("Flags", NifValue::UInt(76));
        weapon.blocks[manager].set_field("Frequency", NifValue::Float(1.0));
        weapon.blocks[manager].set_field("Start Time", NifValue::Float(f32::MAX as f64));
        weapon.blocks[manager].set_field("Stop Time", NifValue::Float(-(f32::MAX as f64)));
        weapon.blocks[root].set_field("Controller", NifValue::Ref(manager as i32));
        manager
    });
    let multi = controller_chain(
        weapon,
        reference(weapon.blocks[manager].get_field("Next Controller")),
    )
    .into_iter()
    .find(|id| weapon.blocks[*id].type_name == "NiMultiTargetTransformController");
    let multi = multi.unwrap_or_else(|| {
        let previous = weapon.blocks[manager]
            .get_field("Next Controller")
            .cloned()
            .unwrap_or(NifValue::Ref(-1));
        let multi = weapon.add_block("NiMultiTargetTransformController", None);
        weapon.blocks[multi].set_field("Next Controller", previous);
        weapon.blocks[multi].set_field("Target", NifValue::Ref(root as i32));
        weapon.blocks[multi].set_field("Flags", NifValue::UInt(108));
        weapon.blocks[multi].set_field("Frequency", NifValue::Float(1.0));
        weapon.blocks[multi].set_field("Start Time", NifValue::Float(f32::MAX as f64));
        weapon.blocks[multi].set_field("Stop Time", NifValue::Float(-(f32::MAX as f64)));
        weapon.blocks[manager].set_field("Next Controller", NifValue::Ref(multi as i32));
        multi
    });
    let palette =
        reference(weapon.blocks[manager].get_field("Object Palette")).unwrap_or_else(|| {
            let palette = weapon.add_block("NiDefaultAVObjectPalette", None);
            weapon.blocks[palette].set_field("Scene", NifValue::Ref(root as i32));
            weapon.blocks[manager].set_field("Object Palette", NifValue::Ref(palette as i32));
            palette
        });
    Ok((manager, multi, palette))
}

fn ensure_visibility_controller(weapon: &mut NifFile, target: usize) -> usize {
    let existing = controller_chain(
        weapon,
        reference(weapon.blocks[target].get_field("Controller")),
    )
    .into_iter()
    .find(|id| weapon.blocks[*id].type_name == "NiVisController");
    let controller = existing.unwrap_or_else(|| {
        let previous = weapon.blocks[target]
            .get_field("Controller")
            .cloned()
            .unwrap_or(NifValue::Ref(-1));
        let id = weapon.add_block("NiVisController", None);
        weapon.blocks[id].set_field("Next Controller", previous);
        weapon.blocks[id].set_field("Target", NifValue::Ref(target as i32));
        weapon.blocks[id].set_field("Flags", NifValue::UInt(108));
        weapon.blocks[id].set_field("Frequency", NifValue::Float(1.0));
        weapon.blocks[target].set_field("Controller", NifValue::Ref(id as i32));
        id
    });
    let flags = match weapon.blocks[controller].get_field("Flags") {
        Some(NifValue::UInt(flags)) => *flags,
        _ => 76,
    };
    weapon.blocks[controller].set_field("Flags", NifValue::UInt(flags | 0x20));
    if reference(weapon.blocks[controller].get_field("Interpolator"))
        .is_none_or(|id| weapon.blocks[id].type_name != "NiBlendBoolInterpolator")
    {
        let blend = weapon.add_block("NiBlendBoolInterpolator", None);
        weapon.blocks[blend].set_field("Flags", NifValue::UInt(1));
        weapon.blocks[blend].set_field("Array Size", NifValue::UInt(2));
        weapon.blocks[controller].set_field("Interpolator", NifValue::Ref(blend as i32));
    }
    controller
}

fn copy_interpolator(
    source: &NifFile,
    target: &mut NifFile,
    id: usize,
    copied: &mut HashMap<usize, usize>,
) -> Result<usize, String> {
    if let Some(id) = copied.get(&id) {
        return Ok(*id);
    }
    let block = source
        .blocks
        .get(id)
        .ok_or("weapon KF interpolator dependency is missing")?;
    let output = target.add_block(block.type_name.clone(), Some(block.fields.clone()));
    target.blocks[output].remainder = block.remainder.clone();
    copied.insert(id, output);
    for reference in block
        .get_refs(&nif_core_native::schema::SCHEMA)
        .into_iter()
        .filter(|id| *id >= 0)
    {
        copy_interpolator(source, target, reference as usize, copied)?;
    }
    for value in target.blocks[output].fields.values_mut() {
        remap_refs(value, copied);
    }
    Ok(output)
}

fn remap_refs(value: &mut NifValue, map: &HashMap<usize, usize>) {
    match value {
        NifValue::Ref(id) if *id >= 0 => {
            if let Some(mapped) = map.get(&(*id as usize)) {
                *id = *mapped as i32;
            }
        }
        NifValue::Array(values) => {
            for value in values {
                remap_refs(value, map);
            }
        }
        NifValue::Struct(fields) => {
            for value in fields.values_mut() {
                remap_refs(value, map);
            }
        }
        _ => {}
    }
}

fn controller_chain(nif: &NifFile, mut id: Option<usize>) -> Vec<usize> {
    let mut seen = HashSet::new();
    let mut result = Vec::new();
    while let Some(current) = id.filter(|id| *id < nif.blocks.len()) {
        if !seen.insert(current) {
            break;
        }
        result.push(current);
        id = reference(nif.blocks[current].get_field("Next Controller"));
    }
    result
}

fn reference(value: Option<&NifValue>) -> Option<usize> {
    match value {
        Some(NifValue::Ref(id)) if *id >= 0 => Some(*id as usize),
        _ => None,
    }
}

fn array(value: Option<&NifValue>) -> &[NifValue] {
    match value {
        Some(NifValue::Array(values)) => values,
        _ => &[],
    }
}

fn set_array(nif: &mut NifFile, block: usize, count: &str, field: &str, values: Vec<NifValue>) {
    nif.blocks[block].set_field(count, NifValue::UInt(values.len() as u64));
    nif.blocks[block].set_field(field, NifValue::Array(values));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn zero_interpolator(nif: &mut NifFile) -> usize {
        let data = nif.add_block("NiFloatData", None);
        nif.blocks[data].set_field(
            "Data",
            NifValue::Struct(IndexMap::from([
                ("Num Keys".into(), NifValue::UInt(2)),
                ("Interpolation".into(), NifValue::UInt(2)),
                (
                    "Keys".into(),
                    NifValue::Array(
                        [0.0, 1.0]
                            .into_iter()
                            .map(|time| {
                                NifValue::Struct(IndexMap::from([
                                    ("Time".into(), NifValue::Float(time)),
                                    ("Value".into(), NifValue::Float(0.0)),
                                    ("Forward".into(), NifValue::Float(0.0)),
                                    ("Backward".into(), NifValue::Float(0.0)),
                                ]))
                            })
                            .collect(),
                    ),
                ),
            ])),
        );
        let interpolator = nif.add_block("NiFloatInterpolator", None);
        nif.blocks[interpolator].set_field("Data", NifValue::Ref(data as i32));
        interpolator
    }

    #[test]
    fn zero_curve_proof_rejects_nonzero_tangents_values_and_broken_dependencies() {
        let mut source = NifFile::new("fo4");
        let interpolator = zero_interpolator(&mut source);
        let data = reference(source.blocks[interpolator].get_field("Data")).unwrap();
        let valid = source.blocks[data].get_field("Data").unwrap().clone();
        assert!(zero_float_curve(&source, interpolator));
        for field in ["Value", "Forward", "Backward"] {
            let mut changed = valid.clone();
            let NifValue::Struct(group) = &mut changed else {
                panic!();
            };
            let NifValue::Array(keys) = group.get_mut("Keys").unwrap() else {
                panic!();
            };
            let NifValue::Struct(key) = &mut keys[0] else {
                panic!();
            };
            key.insert(field.into(), NifValue::Float(0.1));
            source.blocks[data].set_field("Data", changed);
            assert!(!zero_float_curve(&source, interpolator), "{field}");
        }
        source.blocks[data].set_field("Data", valid);
        source.blocks[interpolator].set_field("Data", NifValue::Ref(i32::MAX));
        assert!(!zero_float_curve(&source, interpolator));
        source.blocks[interpolator].set_field("Data", NifValue::Ref(-1));
        source.blocks[interpolator].set_field("Value", NifValue::Float(0.0));
        assert!(zero_float_curve(&source, interpolator));
        source.blocks[interpolator].set_field("Data", NifValue::Ref(-2));
        assert!(!zero_float_curve(&source, interpolator));
    }

    #[test]
    fn neutral_legacy_shader_channels_have_explicit_receipts_and_require_zero_baselines() {
        let mut weapon = NifFile::new("fo4");
        let shape = weapon.add_block("BSTriShape", None);
        weapon.blocks[shape].set_field("Name", NifValue::String("##Display:0".into()));
        let shader = weapon.add_block("BSLightingShaderProperty", None);
        weapon.blocks[shader].set_field("Refraction Strength", NifValue::Float(0.0));
        weapon.blocks[shape].set_field("Shader Property", NifValue::Ref(shader as i32));
        set_array(
            &mut weapon,
            0,
            "Num Children",
            "Children",
            vec![NifValue::Ref(shape as i32)],
        );
        let mut source = kf();
        let mut links = Vec::new();
        for (kind, identifier) in [
            ("NiTextureTransformController", "0-0-TT_ROTATE"),
            ("BSRefractionFirePeriodController", ""),
            ("BSRefractionStrengthController", ""),
        ] {
            let interpolator = zero_interpolator(&mut source);
            links.push(NifValue::Struct(IndexMap::from([
                ("Node Name".into(), NifValue::String("##Display:0".into())),
                ("Controller Type".into(), NifValue::String(kind.into())),
                ("Controller ID".into(), NifValue::String(identifier.into())),
                ("Interpolator".into(), NifValue::Ref(interpolator as i32)),
            ])));
        }
        let sequence = source
            .blocks
            .iter()
            .position(|block| block.type_name == "NiControllerSequence")
            .unwrap();
        set_array(
            &mut source,
            sequence,
            "Num Controlled Blocks",
            "Controlled Blocks",
            links,
        );
        let receipt = attach_weapon_part_sequence(
            &mut weapon,
            &source,
            0,
            "LegacyReload",
            0.0,
            None,
            1.0,
            false,
        )
        .unwrap();
        assert_eq!(receipt.neutral_shader_tracks.len(), 2);
        assert_eq!(receipt.matched_tracks.len(), 1);
        assert!(weapon.blocks.iter().any(|block| block.type_name
            == "BSLightingShaderPropertyFloatController"
            && block.get_field("Controlled Variable") == Some(&NifValue::UInt(0))));
        let mut missing_strength = source.clone();
        let links = array(source.blocks[sequence].get_field("Controlled Blocks"));
        set_array(
            &mut missing_strength,
            sequence,
            "Num Controlled Blocks",
            "Controlled Blocks",
            links[..2].to_vec(),
        );
        assert!(
            attach_weapon_part_sequence(
                &mut weapon,
                &missing_strength,
                0,
                "MissingStrength",
                0.0,
                None,
                1.0,
                false,
            )
            .is_err()
        );
        let mut nonzero_strength = source.clone();
        let NifValue::Struct(strength_link) = &links[2] else {
            panic!();
        };
        let strength = reference(strength_link.get("Interpolator")).unwrap();
        nonzero_strength.blocks[strength].set_field("Data", NifValue::Ref(-1));
        nonzero_strength.blocks[strength].set_field("Value", NifValue::Float(0.1));
        assert!(
            attach_weapon_part_sequence(
                &mut weapon,
                &nonzero_strength,
                0,
                "NonzeroStrength",
                0.0,
                None,
                1.0,
                false,
            )
            .is_err()
        );
        weapon.blocks[shader].set_field("Refraction Strength", NifValue::Float(0.5));
        assert!(
            attach_weapon_part_sequence(
                &mut weapon,
                &source,
                0,
                "NonNeutral",
                0.0,
                None,
                1.0,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn source_shader_tracks_bind_converted_properties_and_roundtrip_native_controllers() {
        for shader_kind in ["BSLightingShaderProperty", "BSEffectShaderProperty"] {
            let mut weapon = NifFile::new("fo4");
            let shape = weapon.add_block("BSTriShape", None);
            weapon.blocks[shape].set_field("Name", NifValue::String("##Display:0".into()));
            let shader = weapon.add_block(shader_kind, None);
            weapon.blocks[shape].set_field("Shader Property", NifValue::Ref(shader as i32));
            set_array(
                &mut weapon,
                0,
                "Num Children",
                "Children",
                vec![NifValue::Ref(shape as i32)],
            );
            let mut source = kf();
            let mut links = Vec::new();
            for (kind, identifier, point3) in [
                ("NiMaterialColorController", "SELF_ILLUM", true),
                ("BSMaterialEmittanceMultController", "", false),
                ("NiTextureTransformController", "0-0-TT_TRANSLATE_U", false),
                ("NiTextureTransformController", "0-0-TT_SCALE_V", false),
                ("NiAlphaController", "", false),
            ] {
                let interpolator = source.add_block(
                    if point3 {
                        "NiPoint3Interpolator"
                    } else {
                        "NiFloatInterpolator"
                    },
                    None,
                );
                let data = source.add_block(if point3 { "NiPosData" } else { "NiFloatData" }, None);
                source.blocks[interpolator].set_field("Data", NifValue::Ref(data as i32));
                links.push(NifValue::Struct(IndexMap::from([
                    ("Node Name".into(), NifValue::String("##Display:0".into())),
                    (
                        "Property Type".into(),
                        NifValue::String("NiMaterialProperty".into()),
                    ),
                    ("Controller Type".into(), NifValue::String(kind.into())),
                    ("Controller ID".into(), NifValue::String(identifier.into())),
                    ("Interpolator".into(), NifValue::Ref(interpolator as i32)),
                    ("Controller".into(), NifValue::Ref(-1)),
                ])));
            }
            let sequence = source
                .blocks
                .iter()
                .position(|block| block.type_name == "NiControllerSequence")
                .unwrap();
            set_array(
                &mut source,
                sequence,
                "Num Controlled Blocks",
                "Controlled Blocks",
                links,
            );
            let receipt = attach_weapon_part_sequence(
                &mut weapon,
                &source,
                0,
                "LegacyReload",
                0.0,
                None,
                1.25,
                false,
            )
            .unwrap();
            assert_eq!(receipt.matched_tracks.len(), 5);
            let bytes = weapon.to_bytes().unwrap();
            let reloaded = NifFile::from_bytes(&bytes, None).unwrap();
            let sequence = reloaded
                .blocks
                .iter()
                .find(|block| block.type_name == "NiControllerSequence")
                .unwrap();
            for link in array(sequence.get_field("Controlled Blocks")) {
                let NifValue::Struct(link) = link else {
                    panic!();
                };
                assert_eq!(link["Property Type"], NifValue::String(shader_kind.into()));
                let controller = reference(link.get("Controller")).unwrap();
                assert_eq!(
                    reference(reloaded.blocks[controller].get_field("Target")),
                    Some(shader)
                );
                assert_eq!(
                    reloaded.blocks[controller].f64_field("Start Time"),
                    Some(0.0)
                );
                assert_eq!(
                    reloaded.blocks[controller].f64_field("Stop Time"),
                    Some(1.0)
                );
                let interpolator = reference(link.get("Interpolator")).unwrap();
                assert!(reference(reloaded.blocks[interpolator].get_field("Data")).is_some());
            }
            let chain = controller_chain(
                &reloaded,
                reference(reloaded.blocks[shader].get_field("Controller")),
            );
            assert_eq!(chain.len(), 5);
            assert!(
                chain
                    .iter()
                    .all(|id| nif_core_native::fo4_block_types::is_fo4_block_type(
                        &reloaded.blocks[*id].type_name
                    ))
            );
            assert!(!reloaded.blocks.iter().any(|block| matches!(
                block.type_name.as_str(),
                "NiMaterialColorController"
                    | "BSMaterialEmittanceMultController"
                    | "NiTextureTransformController"
                    | "NiAlphaController"
            )));
        }
    }

    fn node(nif: &mut NifFile, name: &str) -> usize {
        let id = nif.add_block("NiNode", None);
        nif.blocks[id].set_field("Name", NifValue::String(name.into()));
        let mut children = array(nif.blocks[0].get_field("Children")).to_vec();
        children.push(NifValue::Ref(id as i32));
        set_array(nif, 0, "Num Children", "Children", children);
        id
    }

    fn kf() -> NifFile {
        let mut nif = NifFile::new("fo4");
        let sequence = nif.add_block("NiControllerSequence", None);
        nif.blocks[sequence].set_field("Start Time", NifValue::Float(0.0));
        nif.blocks[sequence].set_field("Stop Time", NifValue::Float(1.0));
        nif.blocks[sequence].set_field("Frequency", NifValue::Float(1.0));
        let mut links = Vec::new();
        for (name, controller, interpolator_type) in [
            (
                "Bip01 R Hand",
                "NiTransformController",
                "NiTransformInterpolator",
            ),
            (
                "##Slide",
                "NiTransformController",
                "NiTransformInterpolator",
            ),
            ("##Magazine", "NiVisController", "NiBoolInterpolator"),
            (
                "##OtherGun",
                "NiTransformController",
                "NiTransformInterpolator",
            ),
        ] {
            let interpolator = nif.add_block(interpolator_type, None);
            let data = nif.add_block(
                if interpolator_type == "NiBoolInterpolator" {
                    "NiBoolData"
                } else {
                    "NiTransformData"
                },
                None,
            );
            nif.blocks[interpolator].set_field("Data", NifValue::Ref(data as i32));
            links.push(NifValue::Struct(IndexMap::from([
                ("Node Name".into(), NifValue::String(name.into())),
                (
                    "Controller Type".into(),
                    NifValue::String(controller.into()),
                ),
                ("Interpolator".into(), NifValue::Ref(interpolator as i32)),
                ("Controller".into(), NifValue::Ref(-1)),
            ])));
        }
        set_array(
            &mut nif,
            sequence,
            "Num Controlled Blocks",
            "Controlled Blocks",
            links,
        );
        nif
    }

    #[test]
    fn exact_named_parts_reject_case_fold_collisions_before_palette_mutation() {
        let mut weapon = NifFile::new("fo4");
        node(&mut weapon,"Magazine");
        node(&mut weapon,"magazine");
        let source = kf();
        let before = weapon.blocks.len();
        let error = attach_named_weapon_part_sequence(&mut weapon,&source,"Reload",1.,&HashSet::from(["Magazine".into(),"magazine".into()])).unwrap_err();
        assert!(error.contains("case-fold"));
        assert_eq!(weapon.blocks.len(),before);
    }

    #[test]
    fn matched_part_tracks_are_preserved_with_manager_and_visibility_binding() {
        let mut weapon = NifFile::new("fo4");
        let slide = node(&mut weapon, "##Slide");
        node(&mut weapon, "##Magazine");
        let receipt = attach_weapon_part_sequence(
            &mut weapon,
            &kf(),
            0,
            "LegacyFire",
            0.1,
            None,
            1.25,
            false,
        )
        .unwrap();
        assert_eq!(receipt.matched_tracks.len(), 2);
        assert_eq!(receipt.unmatched_part_tracks, vec!["##OtherGun"]);
        assert!(
            !weapon
                .blocks
                .iter()
                .any(|block| block.get_field("Name")
                    == Some(&NifValue::String("Bip01 R Hand".into())))
        );
        let multi = weapon
            .blocks
            .iter()
            .find(|block| block.type_name == "NiMultiTargetTransformController")
            .unwrap();
        assert_eq!(
            array(multi.get_field("Extra Targets")),
            &[NifValue::Ref(slide as i32)]
        );
        let sequence = weapon
            .blocks
            .iter()
            .find(|block| block.type_name == "NiControllerSequence")
            .unwrap();
        assert_eq!(sequence.f64_field("Start Time"), Some(0.1));
        assert_eq!(sequence.f64_field("Frequency"), Some(1.25));
        let bytes = weapon.to_bytes().unwrap();
        let reloaded = NifFile::from_bytes(&bytes, None).unwrap();
        assert!(
            reloaded
                .blocks
                .iter()
                .any(|block| block.type_name == "NiBlendBoolInterpolator")
        );
        assert_eq!(
            reloaded
                .blocks
                .iter()
                .filter(|block| block.type_name == "NiControllerSequence")
                .count(),
            1
        );
    }

    #[test]
    fn additional_actions_reuse_manager_palette_and_interpolator_targets() {
        let mut weapon = NifFile::new("fo4");
        node(&mut weapon, "##Slide");
        node(&mut weapon, "##Magazine");
        let source = kf();
        for name in ["LegacyFire", "LegacyReload"] {
            attach_weapon_part_sequence(&mut weapon, &source, 0, name, 0.0, Some(0.5), 1.0, false)
                .unwrap();
        }
        for kind in [
            "NiControllerManager",
            "NiMultiTargetTransformController",
            "NiDefaultAVObjectPalette",
            "NiVisController",
            "NiBlendBoolInterpolator",
        ] {
            assert_eq!(
                weapon
                    .blocks
                    .iter()
                    .filter(|block| block.type_name == kind)
                    .count(),
                1,
                "{kind}"
            );
        }
        let palette = weapon
            .blocks
            .iter()
            .find(|block| block.type_name == "NiDefaultAVObjectPalette")
            .unwrap();
        assert_eq!(array(palette.get_field("Objs")).len(), 2);
        assert!(
            attach_weapon_part_sequence(
                &mut weapon,
                &source,
                0,
                "Invalid",
                0.6,
                Some(0.5),
                1.0,
                false
            )
            .is_err()
        );
    }

    #[test]
    fn constant_part_interpolator_survives_without_data_and_broken_dependencies_fail() {
        let mut weapon = NifFile::new("fo4");
        node(&mut weapon, "##Slide");
        let mut source = kf();
        let sequence = source
            .blocks
            .iter()
            .find(|block| block.type_name == "NiControllerSequence")
            .unwrap();
        let link=array(sequence.get_field("Controlled Blocks")).iter().find(|link|matches!(link,NifValue::Struct(fields) if fields.get("Node Name")==Some(&NifValue::String("##Slide".into())))).unwrap();
        let NifValue::Struct(fields) = link else {
            panic!()
        };
        let interpolator = reference(fields.get("Interpolator")).unwrap();
        source.blocks[interpolator].set_field("Data", NifValue::Ref(-1));
        attach_weapon_part_sequence(&mut weapon, &source, 0, "Constant", 0.0, None, 1.0, false)
            .unwrap();
        let bytes = weapon.to_bytes().unwrap();
        let reloaded = NifFile::from_bytes(&bytes, None).unwrap();
        assert!(
            reloaded
                .blocks
                .iter()
                .any(|block| block.type_name == "NiTransformInterpolator"
                    && block.get_field("Data") == Some(&NifValue::Ref(-1)))
        );
        source.blocks[interpolator].set_field("Data", NifValue::Ref(100_000));
        assert!(
            attach_weapon_part_sequence(&mut weapon, &source, 0, "Broken", 0.0, None, 1.0, false)
                .unwrap_err()
                .contains("dependency is missing")
        );
    }

    #[test]
    #[ignore = "requires extracted FNV weapon NIF and KF assets"]
    fn real_weapon_parts_survive_fo4_conversion_and_nif_save_reload() {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../../extracted/fnv");
        let temp = tempfile::tempdir().unwrap();
        for (model, attack, reload, expected) in [
            (
                "weapons/1handpistol/9mm.nif",
                "1hpattackright",
                "1hpreloadl",
                "##nmBar",
            ),
            (
                "weapons/2handrifle/huntingrifle.nif",
                "2hrattack3",
                "2hrreloada",
                "##HRBolt",
            ),
        ] {
            for perspective in ["_male", "_1stperson"] {
                let output = temp.path().join(format!("{perspective}-{}.nif", attack));
                let report = nif_core_native::convert_file::convert_nif_file(
                    &root.join("meshes").join(model),
                    &output,
                    "fnv",
                    "fo4",
                    None,
                    &nif_core_native::convert_file::ConvertFileOptions {
                        weapon_role: Some("gun".into()),
                        ..Default::default()
                    },
                )
                .unwrap();
                assert!(
                    report.supported && report.errors.is_empty(),
                    "{:?}",
                    report.errors
                );
                let mut weapon = NifFile::load(output.clone()).unwrap();
                let basis = weapon
                    .blocks
                    .iter()
                    .find(|block| {
                        block.get_field("Name")
                            == Some(&NifValue::String("LegacyWeaponBasis".into()))
                    })
                    .unwrap()
                    .get_field("Rotation")
                    .cloned();
                for (stem, name) in [(attack, "LegacyFire"), (reload, "LegacyReload")] {
                    let source = NifFile::load(
                        root.join(format!("meshes/characters/{perspective}/{stem}.kf")),
                    )
                    .unwrap();
                    let receipt = attach_weapon_part_sequence(
                        &mut weapon,
                        &source,
                        0,
                        name,
                        0.0,
                        None,
                        1.0,
                        false,
                    )
                    .unwrap();
                    assert!(!receipt.matched_tracks.is_empty(), "{perspective}/{stem}");
                    if name == "LegacyFire" {
                        assert!(
                            receipt
                                .matched_tracks
                                .iter()
                                .any(|track| track.starts_with(expected)),
                            "{:?}",
                            receipt.matched_tracks
                        );
                    }
                }
                weapon.save(Some(output.clone())).unwrap();
                let reloaded = NifFile::load(output).unwrap();
                assert_eq!(
                    reloaded
                        .blocks
                        .iter()
                        .filter(|block| block.type_name == "NiControllerSequence")
                        .count(),
                    2
                );
                assert_eq!(
                    reloaded
                        .blocks
                        .iter()
                        .find(|block| block.get_field("Name")
                            == Some(&NifValue::String("LegacyWeaponBasis".into())))
                        .unwrap()
                        .get_field("Rotation"),
                    basis.as_ref()
                );
                for sequence in reloaded
                    .blocks
                    .iter()
                    .filter(|block| block.type_name == "NiControllerSequence")
                {
                    assert!(!array(sequence.get_field("Controlled Blocks")).is_empty());
                    for link in array(sequence.get_field("Controlled Blocks")) {
                        let NifValue::Struct(fields) = link else {
                            panic!()
                        };
                        assert!(
                            reference(fields.get("Interpolator"))
                                .is_some_and(|id| id < reloaded.blocks.len())
                        );
                        assert!(
                            reference(fields.get("Controller"))
                                .is_some_and(|id| id < reloaded.blocks.len())
                        );
                    }
                }
            }
        }
    }
}
