use super::*;
use crate::phase::animations::{
    FnvKfStageRequest, weapon_parts::attach_creature_property_sequence,
};
use crate::source_rig::CreaturePropertySequence;
use nif_core_native::model::NifValue;

pub(super) struct PropertyClip {
    source: NifFile,
    source_path: String,
    sequence_index: usize,
    playback_speed: f64,
    windows: Vec<PropertyWindow>,
    clip_name: String,
    source_float_slots: Vec<String>,
}

struct PropertyWindow {
    crop_start: f32,
    crop_end: f32,
    source_start: f64,
    source_end: f64,
    looping: bool,
}

pub(super) fn capture(
    requests: &mut Vec<PropertyClip>,
    source: &[u8],
    request: &FnvKfStageRequest<'_>,
    candidate: &MotionCandidate,
    clip_name: &str,
    looping: bool,
    source_idle: Option<&CapabilityRoleGenerator>,
) -> Result<(), String> {
    if candidate.binding.required_float_slots.is_empty() {
        return Ok(());
    }
    let source = NifFile::from_bytes(source, None).map_err(|error| error.to_string())?;
    let sequence_index = candidate.sequence_index;
    let sequence = source
        .blocks
        .iter()
        .filter(|block| block.type_name == "NiControllerSequence")
        .nth(sequence_index)
        .ok_or("property KF has no selected sequence")?;
    let number = |field| match sequence.get_field(field) {
        Some(NifValue::Float(value)) => Ok(*value),
        _ => Err(format!("property KF missing {field}")),
    };
    let duration = (number("Stop Time")? - number("Start Time")?) / number("Frequency")?;
    let playback_speed = request
        .automatic_attack
        .map(|segment| segment.playback_speed)
        .or(request.attack_playback_speed)
        .unwrap_or(1.0);
    let (start, end) = request
        .automatic_attack
        .map(|segment| {
            (
                segment.start_seconds * playback_speed,
                segment.end_seconds * playback_speed,
            )
        })
        .unwrap_or((0.0, duration));
    let mut windows = if source_idle.is_some() {
        Vec::new()
    } else {
        vec![PropertyWindow {
            crop_start: 0.0,
            crop_end: 0.0,
            source_start: start,
            source_end: end,
            looping,
        }]
    };
    if let Some(CapabilityRoleGenerator::SourceIdle {
        duration_bits,
        loop_start_bits,
        loop_end_bits,
        has_loop_markers,
        looping_min,
        looping_max,
        ..
    }) = source_idle
    {
        let duration = f32::from_bits(*duration_bits);
        let from = f32::from_bits(*loop_start_bits);
        let to = f32::from_bits(*loop_end_bits);
        let inner_loop = if *has_loop_markers {
            *looping_min == 255 && *looping_max != 0
        } else {
            looping
        };
        let mut add = |begin: f32, end: f32, loops| {
            let crop_end = duration - end;
            if begin < end
                && !windows.iter().any(|window| {
                    window.crop_start == begin
                        && window.crop_end == crop_end
                        && window.looping == loops
                })
            {
                windows.push(PropertyWindow {
                    crop_start: begin,
                    crop_end,
                    source_start: f64::from(begin),
                    source_end: f64::from(end),
                    looping: loops,
                });
            }
        };
        add(0.0, from, false);
        add(from, to, inner_loop);
        add(to, duration, false);
    }
    requests.push(PropertyClip {
        source,
        source_path: candidate.source_kf.clone(),
        sequence_index,
        playback_speed,
        windows,
        clip_name: clip_name.into(),
        source_float_slots: candidate.binding.required_float_slots.clone(),
    });
    Ok(())
}

pub(super) fn attach(
    requests: &[PropertyClip],
    closure: &mut CreatureClosureReceipt,
    staged_root: &Path,
) -> Result<Vec<CreaturePropertySequence>, String> {
    let mut result = Vec::new();
    let mut targets = closure
        .inputs
        .iter()
        .filter(|input| {
            matches!(
                input.role,
                CreatureNifRole::Skeleton | CreatureNifRole::Body
            )
        })
        .map(|input| input.target_data_relative_path.clone())
        .collect::<Vec<_>>();
    targets.sort();
    targets.dedup();
    let mut loaded = targets
        .iter()
        .map(|target| {
            let path = staged_root.join(target.replace('/', std::path::MAIN_SEPARATOR_STR));
            NifFile::load(&path)
                .map(|nif| (target.clone(), path, nif, false))
                .map_err(|error| error.to_string())
        })
        .collect::<Result<Vec<_>, _>>()?;
    for request in requests {
        for window in &request.windows {
            let mut sequence_names = Vec::new();
            let mut unmatched = BTreeSet::new();
            let mut matched = BTreeSet::new();
            for (target, _, nif, changed) in &mut loaded {
                let key = format!(
                    "{}|{}|{}|{}|{}|{}",
                    request.source_path,
                    request.clip_name,
                    window.crop_start.to_bits(),
                    window.crop_end.to_bits(),
                    window.looping,
                    target
                );
                let name = format!("B21CP_{}", blake3::hash(key.as_bytes()).to_hex());
                let receipt = attach_creature_property_sequence(
                    nif,
                    &request.source,
                    request.sequence_index,
                    &name,
                    window.source_start,
                    Some(window.source_end),
                    request.playback_speed,
                    window.looping,
                )?;
                unmatched.extend(receipt.unmatched_part_tracks);
                matched.extend(
                    receipt.matched_tracks.iter().filter_map(|track| {
                        track.rsplit_once(':').map(|(name, _)| name.to_string())
                    }),
                );
                matched.extend(receipt.neutral_shader_tracks.iter().filter_map(|track| {
                    track
                        .strip_suffix(":verified_zero_curve")
                        .and_then(|track| track.rsplit_once(':'))
                        .map(|(name, _)| name.to_string())
                }));
                if !receipt.matched_tracks.is_empty() {
                    *changed = true;
                    sequence_names.push(name);
                }
            }
            unmatched.retain(|target| !matched.contains(target));
            if !unmatched.is_empty() {
                return Err(format!(
                    "{} property targets absent from source-owned creature NIFs: {unmatched:?}",
                    request.source_path
                ));
            }
            result.push(CreaturePropertySequence {
                clip_name: request.clip_name.clone(),
                crop_start_bits: window.crop_start.to_bits(),
                crop_end_bits: window.crop_end.to_bits(),
                looping: window.looping,
                sequence_names,
                source_float_slots: request.source_float_slots.clone(),
            });
        }
    }
    for (target, path, mut nif, changed) in loaded {
        if changed {
            nif.save(Some(path)).map_err(|error| error.to_string())?;
            refresh_staged_creature_nif_artifact(closure, staged_root, &target)
                .map_err(|error| error.to_string())?;
        }
    }
    Ok(result)
}

pub(super) fn instantiate_graph_sequences(
    rig: &CreatureManifest,
    graph: &CapabilityGraphManifest,
    closure: &mut CreatureClosureReceipt,
    staged_root: &Path,
) -> Result<(), String> {
    if rig.property_sequences.is_empty() {
        return Ok(());
    }
    let scaffold = crate::source_rig::emit_capability_scaffold(rig, graph)
        .map_err(|error| error.to_string())?;
    let core = scaffold
        .artifact(&rig.paths.core_behavior)
        .ok_or("property graph omitted core behavior")?;
    let document = roxmltree::Document::parse(&core.xml).map_err(|error| error.to_string())?;
    let mut names = rig
        .property_sequences
        .iter()
        .flat_map(|binding| &binding.sequence_names)
        .map(|name| (name.clone(), BTreeSet::new()))
        .collect::<BTreeMap<_, _>>();
    for node in document
        .descendants()
        .filter(|node| node.attribute("class") == Some("BGSGamebryoSequenceGenerator"))
    {
        let name = node
            .children()
            .find(|node| node.attribute("name") == Some("pSequence"))
            .and_then(|node| node.text())
            .ok_or("property graph sequence name is missing")?;
        let (base, _) = name
            .split_once("__")
            .ok_or("property graph sequence has no clock identity")?;
        let instances = names
            .get_mut(base)
            .ok_or_else(|| format!("property graph references undeclared sequence {base}"))?;
        if !instances.insert(name.to_string()) {
            return Err(format!("property graph aliases sequence {name}"));
        }
    }
    if names.values().any(BTreeSet::is_empty) {
        return Err("staged property sequence has no graph consumer".into());
    }
    let targets = closure
        .inputs
        .iter()
        .filter(|input| {
            matches!(
                input.role,
                CreatureNifRole::Skeleton | CreatureNifRole::Body
            )
        })
        .map(|input| input.target_data_relative_path.clone())
        .collect::<BTreeSet<_>>();
    for target in targets {
        let path = staged_root.join(target.replace('/', std::path::MAIN_SEPARATOR_STR));
        let mut nif = NifFile::load(&path).map_err(|error| error.to_string())?;
        let bases = nif
            .blocks
            .iter()
            .filter(|block| block.type_name == "NiControllerSequence")
            .filter_map(|block| match block.get_field("Name") {
                Some(NifValue::String(name)) if names.contains_key(name) => Some(name.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        if bases.is_empty() {
            continue;
        }
        for base in bases {
            let instances = names
                .remove(&base)
                .ok_or_else(|| format!("property template is attached to multiple NIFs: {base}"))?;
            crate::phase::animations::weapon_parts::instantiate_creature_property_sequence(
                &mut nif,
                &base,
                &instances.into_iter().collect::<Vec<_>>(),
            )?;
        }
        nif.save(Some(path)).map_err(|error| error.to_string())?;
        refresh_staged_creature_nif_artifact(closure, staged_root, &target)
            .map_err(|error| error.to_string())?;
    }
    if !names.is_empty() {
        return Err(format!(
            "property graph sequences have no staged NIF owner: {:?}",
            names.keys().collect::<Vec<_>>()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires B21_FNV_CREATURE_ACTION_CORPUS and its source action evidence cache"]
    fn real_securitron_property_closure_and_cropped_graph_pack_together() {
        let workspace = PathBuf::from(std::env::var("B21_FNV_CREATURE_ACTION_CORPUS").unwrap());
        let ledger: CreatureFamilyRecipeLedger = serde_json::from_slice(
            &fs::read(workspace.join("tmp/fnv_fo3_full_source_action_evidence.json")).unwrap(),
        )
        .unwrap();
        let family = ledger
            .families
            .iter()
            .find(|family| {
                family.rig.game == LegacyCreatureGame::Fnv
                    && family
                        .rig
                        .skeleton_path
                        .to_ascii_lowercase()
                        .contains("nvsecuritron")
            })
            .unwrap()
            .clone();
        let evidence = FnvFo3LiveRecipeEvidence {
            motion_families: vec![],
            motion_sets: vec![],
            movement_issues: vec![],
            graph_contracts: vec![],
            recipe_ledger: ledger,
        };
        let temp = tempfile::tempdir().unwrap();
        let prepared = prepare_live_family(
            &family,
            &evidence,
            &workspace.join("extracted"),
            temp.path(),
            &BTreeMap::new(),
            &StringInterner::new(),
        )
        .unwrap();
        assert!(!prepared.rig.property_sequences.is_empty());
        assert!(prepared.rig.animation_skeleton.float_slots.is_empty());
        assert!(
            prepared
                .rig
                .clips
                .iter()
                .all(|clip| clip.binding.declared_float_tracks == 0)
        );
        let mut attached = BTreeSet::new();
        let mut controller_types = BTreeSet::new();
        for input in &prepared.closure.inputs {
            if !matches!(
                input.role,
                CreatureNifRole::Body | CreatureNifRole::Skeleton
            ) {
                continue;
            }
            let nif = NifFile::load(
                prepared
                    .closure_staged_data_root
                    .join(&input.target_data_relative_path),
            )
            .unwrap();
            for block in &nif.blocks {
                controller_types.insert(block.type_name.clone());
                if block.type_name == "NiControllerSequence"
                    && let Some(NifValue::String(name)) = block.get_field("Name")
                {
                    assert!(
                        attached.insert(name.clone()),
                        "duplicate sequence across actor parts: {name}"
                    );
                }
            }
        }
        assert!(controller_types.contains("BSEffectShaderPropertyFloatController"));
        assert!(controller_types.contains("BSEffectShaderPropertyColorController"));
        for binding in &prepared.rig.property_sequences {
            assert!(binding.sequence_names.iter().all(|name| {
                attached
                    .iter()
                    .any(|attached| attached.starts_with(&format!("{name}__")))
            }));
        }
        let packed = crate::source_rig::pack_capability_scaffold(
            &prepared.rig,
            &prepared.graph,
            temp.path().join("packed"),
        )
        .unwrap();
        let bytes = fs::read(
            &packed
                .artifact(&prepared.rig.paths.core_behavior)
                .unwrap()
                .hkx_path,
        )
        .unwrap();
        let xml = havok_native::api::havok_hkx_to_xml(&bytes).unwrap();
        let doc = roxmltree::Document::parse(&xml).unwrap();
        let mut generated = 0;
        let mut generated_names = BTreeSet::new();
        for node in doc
            .descendants()
            .filter(|node| node.attribute("class") == Some("BGSGamebryoSequenceGenerator"))
        {
            let name = node
                .children()
                .find(|node| node.attribute("name") == Some("pSequence"))
                .and_then(|node| node.text())
                .unwrap();
            assert!(attached.contains(name));
            assert!(
                generated_names.insert(name),
                "two graph clocks share one NIF sequence: {name}"
            );
            generated += 1;
        }
        assert!(generated > 10);
        prepared.closure.validate().unwrap();
    }
}
