use std::collections::{BTreeMap, HashMap};
use std::path::Path;

use rayon::prelude::*;
use serde::Deserialize;
use serde_json::{Value, json};

use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;

use super::legacy_weapons::{source_weapon_paths, stage_actor_set};
use super::weapon_events::{
    WeaponAnimationFields, WeaponPerspective, decode_weapon_animation_fields,
    source_weapon_clip_candidates,
};

#[derive(Deserialize)]
struct SourceWeapon {
    editor_id: String,
    form_key: String,
    model: Option<String>,
    first_model: Option<String>,
    dnam_hex: String,
}

fn source_animation_fields(source: &SourceWeapon) -> WeaponAnimationFields {
    let interner = StringInterner::new();
    let bytes = source
        .dnam_hex
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect::<Vec<_>>();
    let mut record = Record::new(
        SigCode(*b"WEAP"),
        FormKey {
            plugin: interner.intern("FalloutNV.esm"),
            local: 0x800,
        },
    );
    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"DNAM"),
        value: FieldValue::Bytes(bytes.into()),
    });
    decode_weapon_animation_fields(&record, &interner).unwrap()
}

fn source_weapon_class(source: &SourceWeapon, fields: &WeaponAnimationFields) -> &'static str {
    if !(3..=9).contains(&fields.animation_type) {
        "non_ranged"
    } else if source.model.as_deref().is_none_or(str::is_empty) {
        "no_model"
    } else if matches!(
        source.editor_id.as_str(),
        "Bloatfly1HP" | "WeapNVSecuritronMissile"
    ) {
        "creature_attack"
    } else if matches!(
        source.editor_id.as_str(),
        "WeapNVDetonator" | "WeapNVDetonatorLoyal"
    ) {
        "detonator"
    } else if source.editor_id == "WeapNVBinoculars" {
        "zoom_tool"
    } else if let Err(error) = source_weapon_clip_candidates(fields, WeaponPerspective::Third) {
        eprintln!(
            "unsupported {} {}: {error}",
            source.editor_id, source.form_key
        );
        "unsupported_fields"
    } else {
        "human_ranged"
    }
}

#[test]
#[ignore = "requires extracted FNV/FO4 assets and modkit CLI corpus in temporary directory"]
fn real_source_ranged_weapon_profiles_stage_for_both_perspectives() {
    let fixture_path = std::env::temp_dir().join("b21_fnv_weapon_corpus.json");
    let corpus: Vec<SourceWeapon> =
        serde_json::from_slice(&std::fs::read(&fixture_path).unwrap()).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let source_roots = vec![root.join("extracted/fnv")];
    let paths = source_weapon_paths(&source_roots).unwrap();
    let output = tempfile::tempdir().unwrap();
    let mut profiles = BTreeMap::new();
    let mut classes = BTreeMap::<&str, usize>::new();
    for source in &corpus {
        let fields = source_animation_fields(source);
        let class = source_weapon_class(source, &fields);
        if class == "human_ranged" {
            let key = serde_json::to_string(&fields).unwrap();
            let profile = profiles.entry(key).or_insert_with(|| (fields, Vec::new()));
            profile
                .1
                .push(format!("{} {}", source.editor_id, source.form_key));
        }
        *classes.entry(class).or_default() += 1;
    }
    let profiles = profiles.into_values().collect::<Vec<_>>();
    let outcomes = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .unwrap()
        .install(|| {
            profiles
                .par_iter()
                .enumerate()
                .flat_map_iter(|(index, (fields, weapons))| {
                    [WeaponPerspective::Third, WeaponPerspective::First].map(|perspective| {
                        let set_id = format!("B21_Corpus_{index}");
                        match stage_actor_set(
                            fields,
                            perspective,
                            &set_id,
                            &source_roots,
                            &paths,
                            &root.join("extracted/fo4"),
                            output.path(),
                            &HashMap::new(),
                        ) {
                            Ok(staged) => {
                                assert!(!staged.clips.is_empty());
                                for path in &staged.clips {
                                    super::validate_staged_fo4_animation(
                                        &std::fs::read(output.path().join(path)).unwrap(),
                                    )
                                    .unwrap();
                                }
                                Ok(())
                            }
                            Err(error) => {
                                let failure = format!("{perspective:?} {weapons:?}: {error}");
                                eprintln!("staging failure: {failure}");
                                Err(failure)
                            }
                        }
                    })
                })
                .collect::<Vec<_>>()
        });
    let staged_sets = outcomes.iter().filter(|result| result.is_ok()).count();
    let failures = outcomes
        .into_iter()
        .filter_map(Result::err)
        .collect::<Vec<_>>();
    eprintln!(
        "weapon corpus: records={} classes={classes:?} distinct_profiles={} staged_sets={staged_sets} failed_sets={}",
        corpus.len(),
        profiles.len(),
        failures.len()
    );
    assert_eq!(classes.values().sum::<usize>(), corpus.len());
    assert!(!profiles.is_empty());
    assert!(
        failures.is_empty(),
        "source weapon staging corpus has failures"
    );
}

#[test]
#[ignore = "requires extracted FNV/FO4 assets and modkit CLI corpus with WNAM STAT models"]
fn real_source_ranged_weapon_models_stage_parts_for_both_perspectives() {
    let fixture_path = std::env::temp_dir().join("b21_fnv_weapon_corpus.json");
    let corpus: Vec<SourceWeapon> =
        serde_json::from_slice(&std::fs::read(&fixture_path).unwrap()).unwrap();
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../..");
    let source_roots = vec![root.join("extracted/fnv")];
    let inventory = source_weapon_paths(&source_roots).unwrap();
    let output = tempfile::tempdir().unwrap();
    let meshes = output.path().join("meshes");
    let mut classes = BTreeMap::<&str, usize>::new();
    let mut jobs = Vec::new();
    for source in &corpus {
        let fields = source_animation_fields(source);
        let class = source_weapon_class(source, &fields);
        *classes.entry(class).or_default() += 1;
        if class == "human_ranged" {
            for perspective in [WeaponPerspective::Third, WeaponPerspective::First] {
                let model = match perspective {
                    WeaponPerspective::Third => source.model.as_deref(),
                    WeaponPerspective::First => source.first_model.as_deref(),
                }
                .filter(|model| !model.is_empty())
                .unwrap_or_else(|| {
                    panic!(
                        "{} {perspective:?} model missing from CLI corpus",
                        source.editor_id
                    )
                });
                jobs.push((source, fields.clone(), perspective, model));
            }
        }
    }
    let outcomes = rayon::ThreadPoolBuilder::new()
        .num_threads(8)
        .build()
        .unwrap()
        .install(|| {
            jobs.par_iter()
                .enumerate()
                .map(|(index, (source, fields, perspective, model))| {
                    let identity = format!("{} {} {perspective:?} {model}", source.editor_id, source.form_key);
                    let result = (|| -> Result<Value, String> {
                        let folder = meshes.join(index.to_string());
                        std::fs::create_dir_all(&folder).map_err(|error| error.to_string())?;
                        let converted_path = folder.join("converted.nif");
                        let converted = nif_core_native::convert_file::convert_nif_file(
                            &source_roots[0].join("meshes").join(model),
                            &converted_path,
                            "fnv",
                            "fo4",
                            None,
                            &nif_core_native::convert_file::ConvertFileOptions {
                                weapon_role: Some("gun".into()),
                                ..Default::default()
                            },
                        )
                        .map_err(|error| error.to_string())?;
                        if !converted.supported || !converted.errors.is_empty() {
                            return Err(format!("NIF conversion: {:?}", converted.errors));
                        }
                        let original = std::fs::read(&converted_path).map_err(|error| error.to_string())?;
                        let staged_path = folder.join("staged.nif");
                        let (receipt, outputs) = super::weapon_parts::stage_model_parts(
                            fields,
                            *perspective,
                            &source_roots,
                            &inventory,
                            &root.join("extracted/fo4"),
                            &meshes,
                            &converted_path,
                            &staged_path,
                            &format!("B21_CorpusParts_{index}"),
                        )?;
                        let mut nif = nif_core_native::model::NifFile::load(staged_path.clone())
                            .map_err(|error| error.to_string())?;
                        let saved = nif.to_bytes().map_err(|error| error.to_string())?;
                        std::fs::write(&staged_path, &saved).map_err(|error| error.to_string())?;
                        let reloaded = nif_core_native::model::NifFile::load(staged_path)
                            .map_err(|error| error.to_string())?;
                        let unsupported = reloaded.blocks.iter()
                            .filter(|block| !nif_core_native::fo4_block_types::is_fo4_block_type(&block.type_name))
                            .map(|block| block.type_name.as_str())
                            .collect::<Vec<_>>();
                        if !unsupported.is_empty() {
                            return Err(format!("saved NIF contains unsupported FO4 blocks: {unsupported:?}"));
                        }
                        if std::fs::read(&converted_path).map_err(|error| error.to_string())? != original {
                            return Err("part staging mutated its converted input".into());
                        }
                        for path in outputs.iter().skip(1) {
                            let bytes = std::fs::read(path).map_err(|error| error.to_string())?;
                            havok_native::hkx::HkxFile::read(&bytes).map_err(|error| error.to_string())?;
                        }
                        if receipt["has_matching_parts"] == true {
                            if outputs.len() != 4
                                || !reloaded.blocks.iter().any(|block| block.type_name == "NiControllerManager")
                                || !reloaded.blocks.iter().any(|block| block.type_name == "NiControllerSequence")
                            {
                                return Err("saved part model lost its controller or FX bundle".into());
                            }
                            let bged = reloaded.blocks.iter()
                                .find(|block| block.type_name == "BSBehaviorGraphExtraData")
                                .ok_or("saved part model lost its behavior attachment")?;
                            let Some(nif_core_native::model::NifValue::String(project)) = bged.get_field("Behaviour Graph File") else {
                                return Err("saved behavior attachment has no project path".into());
                            };
                            if !meshes.join(project.replace('\\', "/")).is_file() {
                                return Err(format!("saved behavior project does not resolve: {project}"));
                            }
                            let behavior = std::fs::read(&outputs[3]).map_err(|error| error.to_string())?;
                            let xml = havok_native::api::havok_hkx_to_xml(&behavior).map_err(|error| error.to_string())?;
                            if !xml.contains("LegacyIdle") {
                                return Err("packed FX behavior lost its idle state".into());
                            }
                        } else if outputs.len() != 1 {
                            return Err("model without matching parts emitted an FX graph".into());
                        }
                        Ok(json!({"weapon":source.editor_id,"source":source.form_key,"perspective":format!("{perspective:?}"),"model":model,"receipt":receipt}))
                    })();
                    if let Err(error) = &result {
                        eprintln!("weapon part corpus failure: {identity}: {error}");
                    }
                    result.map_err(|error| format!("{identity}: {error}"))
                })
                .collect::<Vec<_>>()
        });
    let staged_models = outcomes
        .iter()
        .filter_map(|result| result.as_ref().ok())
        .collect::<Vec<_>>();
    let with_parts = staged_models
        .iter()
        .filter(|row| row["receipt"]["has_matching_parts"] == true)
        .count();
    let failures = outcomes
        .iter()
        .filter_map(|result| result.as_ref().err())
        .collect::<Vec<_>>();
    let report = json!({"records":corpus.len(),"classes":classes,"pov_models":jobs.len(),"staged_models":staged_models.len(),"models_with_parts":with_parts,"models_without_parts":staged_models.len()-with_parts,"failures":failures,"models":staged_models});
    let report_path = std::env::temp_dir().join("b21_fnv_weapon_parts_corpus_receipt.json");
    std::fs::write(&report_path, serde_json::to_vec_pretty(&report).unwrap()).unwrap();
    eprintln!(
        "weapon part corpus: records={} classes={classes:?} pov_models={} staged_models={} with_parts={with_parts} without_parts={} failures={} report={}",
        corpus.len(),
        jobs.len(),
        staged_models.len(),
        staged_models.len() - with_parts,
        failures.len(),
        report_path.display()
    );
    assert_eq!(classes.values().sum::<usize>(), corpus.len());
    assert!(jobs.len() > 100);
    assert!(with_parts > 0);
    if !failures.is_empty() {
        let retained = output.keep();
        panic!(
            "source weapon part corpus has failures; models retained at {}",
            retained.display()
        );
    }
}
