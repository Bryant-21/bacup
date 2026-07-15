//! Fixup: rewrite raw FormIDs preserved inside byte payloads.
//!
//! OBTS contains variable arrays for template keywords, included OMODs, and
//! material-swap property rows. In the whole-plugin path those values can
//! remain raw source-local FormIDs such as `0037D0C1`, which are then written
//! as FO4 master references instead of the converted output-plugin FormID.
//!
//! NPC_ TPTA has the same failure mode: FO76 source-local template actor slots
//! must be rewritten to the converted target FormIDs before the FO4 plugin is
//! saved. It can also contain already-encoded output-plugin self references
//! when a converted NPC shares a local FormID with a FO4 template NPC; those
//! direct self slots are redirected to the record's default template.
//!
//! OMOD MODS and DATA also carry raw FormIDs in material-swap slots, attach
//! points, attach parent slots, and included-mod rows. Those rows drive the
//! power-armor selector chains and otherwise load as missing Fallout4.esm
//! objects.
//!
//! PACK PKCU and raw CTDA/CTDT condition subrecords have the same on-disk
//! hazard: source-local parameters remain `00xxxxxx` bytes until this pass can
//! rewrite them with the final target plugin index.
//!
//! REGN RDWT weather rows, RDSA sound rows, RFCT effect-art slots, and WTHR
//! image-space/sound/spell slots can also survive as raw `00xxxxxx` FormIDs,
//! which makes the FO4 CK look for those records in Fallout4.esm.
//!
//! FSTS DATA footstep arrays have the same shape: packed source-local FSTP refs.
//!
//! AMMO DNAM, HAZD DNAM, PROJ DNAM, and STAG TNAM carry byte-packed refs. When
//! source-local FO76 IDs survive there, CK resolves them as Fallout4.esm.
//!
//! Parsed FormKey fields can fail the same way: a raw source-local `00xxxxxx`
//! value is decoded as the first target master before byte-level fixups see it.
//!
//! REFR/ACHR location payloads also include current-zone cell refs (`XCZC`);
//! leaving those source-local can make CK treat the compact FormID as a pointer.
//!
//! VMAD script object properties have the same source-context raw FormID
//! encoding. Without rewriting, CK resolves copied FO76 property targets as
//! Fallout4.esm objects and reports invalid script properties.

use rustc_hash::{FxHashMap, FxHashSet};

use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::rewrite_schema_form_ids_in_subrecord;
use esp_authoring_core::plugin_runtime::{
    CompiledSchema, SchemaRecordJson, compiled_schema_for_game, schema_record_spec,
    schema_subrecord_spec,
};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;

pub struct RewriteRawObjectTemplateFormIdsFixup;

enum TargetMasterValidityCache {
    NoFirstMaster,
    FirstMaster {
        local_object_ids: FxHashSet<u32>,
        npc_template_local_object_ids: FxHashSet<u32>,
    },
}

impl TargetMasterValidityCache {
    fn from_session(
        session: &mut PluginSession,
        target_master_handles: &[(String, u64)],
        interner: &StringInterner,
    ) -> Self {
        let Some((_, handle_id)) = target_master_handles.first() else {
            return Self::NoFirstMaster;
        };
        let local_object_ids = session
            .local_object_ids_in_handle(*handle_id)
            .unwrap_or_default();
        let mut npc_template_local_object_ids = FxHashSet::default();
        for sig in ["NPC_", "LVLN"] {
            let Ok(form_keys) = session.form_keys_of_sig_in_handle(
                *handle_id,
                crate::ids::SigCode::from_str(sig).unwrap(),
                interner,
            ) else {
                continue;
            };
            npc_template_local_object_ids.extend(
                form_keys
                    .into_iter()
                    .filter_map(|fk| (fk.local != 0).then_some(fk.local & 0x00FF_FFFF)),
            );
        }
        Self::FirstMaster {
            local_object_ids,
            npc_template_local_object_ids,
        }
    }

    fn is_valid_raw_form_id(&self, raw_form_id: u32) -> bool {
        if raw_form_id == 0 || raw_form_id >> 24 != 0 {
            return true;
        }
        match self {
            Self::NoFirstMaster => true,
            Self::FirstMaster {
                local_object_ids, ..
            } => local_object_ids.contains(&(raw_form_id & 0x00FF_FFFF)),
        }
    }

    fn is_valid_npc_template_raw_form_id(&self, raw_form_id: u32) -> bool {
        if raw_form_id == 0 || raw_form_id >> 24 != 0 {
            return false;
        }
        match self {
            Self::NoFirstMaster => false,
            Self::FirstMaster {
                npc_template_local_object_ids,
                ..
            } => npc_template_local_object_ids.contains(&(raw_form_id & 0x00FF_FFFF)),
        }
    }

    #[cfg(test)]
    fn from_object_ids_for_test(ids: impl IntoIterator<Item = u32>) -> Self {
        Self::FirstMaster {
            local_object_ids: ids.into_iter().collect(),
            npc_template_local_object_ids: FxHashSet::default(),
        }
    }

    #[cfg(test)]
    fn no_first_master_for_test() -> Self {
        Self::NoFirstMaster
    }
}

const FO76_CTDA_PARAM1_FORMID_FUNCTIONS: &[u16] = &[
    1, 14, 27, 32, 42, 43, 44, 45, 47, 56, 59, 60, 62, 66, 67, 68, 69, 71, 72, 73, 74, 75, 79, 84,
    99, 105, 109, 117, 122, 129, 131, 132, 136, 142, 148, 149, 152, 161, 162, 163, 172, 180, 181,
    182, 193, 195, 197, 199, 214, 223, 228, 230, 246, 248, 250, 258, 259, 261, 262, 264, 266, 267,
    277, 278, 280, 359, 360, 362, 366, 370, 372, 373, 375, 376, 378, 398, 403, 408, 409, 410, 414,
    426, 438, 439, 444, 445, 448, 449, 450, 459, 463, 465, 477, 479, 493, 494, 497, 501, 506, 507,
    510, 511, 512, 513, 515, 516, 517, 518, 522, 523, 524, 525, 533, 534, 535, 543, 550, 552, 560,
    561, 562, 563, 565, 566, 567, 577, 579, 584, 591, 592, 595, 596, 598, 600, 601, 603, 604, 605,
    606, 608, 610, 617, 624, 625, 626, 629, 637, 639, 640, 650, 651, 652, 661, 664, 678, 682, 691,
    692, 693, 696, 697, 699, 705, 707, 713, 719, 720, 722, 736, 737, 741, 742, 749, 753, 754, 766,
    772, 773, 783, 802, 804, 806, 807, 808, 809, 810, 832, 833, 834, 835, 836, 840, 841, 844, 845,
    849, 852, 853, 854, 856, 857, 858, 859, 863, 865, 869, 870, 871, 875, 876, 881, 882, 884, 890,
    891, 895, 904, 905, 906, 907, 911, 914, 917, 920, 921, 931, 936, 937, 5001, 5004, 8003, 8005,
    8007, 10005, 10007, 10009, 10010, 10011, 10013, 10016, 12002,
];

const FO76_CTDA_PARAM2_FORMID_FUNCTIONS: &[u16] = &[
    149, 180, 181, 230, 280, 576, 577, 600, 601, 603, 604, 605, 608, 610, 650, 846,
];

impl Fixup for RewriteRawObjectTemplateFormIdsFixup {
    fn name(&self) -> &'static str {
        "rewrite_raw_object_template_formids"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_masters = session.target_masters().to_vec();
        let target_master_handles: Vec<(String, u64)> = target_masters
            .iter()
            .cloned()
            .zip(config.target_master_handle_ids.iter().copied())
            .collect();
        let target_master_validity = TargetMasterValidityCache::from_session(
            session,
            &target_master_handles,
            mapper.interner,
        );
        let target_master_handles_available = !target_master_handles.is_empty();
        let source_encoded_targets = encoded_targets_by_source_object_id(mapper, &target_masters);
        let mut encoded_targets = source_encoded_targets.clone();
        if !encoded_targets.is_empty() {
            add_output_record_targets(
                session,
                mapper.interner,
                &target_masters,
                &mut encoded_targets,
            )?;
        }
        if encoded_targets.is_empty() {
            return Ok(FixupReport::empty());
        }
        let target_record_sigs_by_encoded_form_id =
            target_record_sigs_by_encoded_form_id(session, mapper.interner, &target_masters)?;

        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let target_compiled_schema = session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .and_then(|game| compiled_schema_for_game(game).ok());
        let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let output_plugin = mapper.interner.intern(&output_plugin_name);
        let first_target_master_plugin = target_masters
            .first()
            .map(|master_name| mapper.interner.intern(master_name));
        let mut form_keys = Vec::new();
        for sig in session
            .target_signatures()
            .map_err(|e| FixupError::HandleError(e.to_string()))?
        {
            form_keys.extend(
                session
                    .form_keys_of_sig(sig, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?,
            );
        }

        let mut report = FixupReport::empty();
        let mut changed_records = Vec::new();
        let mut changed_records_by_fk = FxHashMap::default();
        for fk in form_keys {
            let mut record =
                match session.record_decoded(&fk, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let w = mapper
                            .interner
                            .intern(&format!("rewrite_raw_object_template_formids_read_err:{e}"));
                        report.warnings.push(w);
                        continue;
                    }
                };

            let schema_rewrite_changed = {
                let mut is_valid_target_master_formid =
                    |raw_form_id: u32| target_master_validity.is_valid_raw_form_id(raw_form_id);
                match target_compiled_schema.as_ref().and_then(|schema| {
                    schema_record_spec(schema.as_ref(), record.sig.as_str())
                        .map(|record_spec| (schema, record_spec))
                }) {
                    Some((schema, record_spec)) => rewrite_schema_record_raw_formids(
                        &mut record,
                        record_spec,
                        schema.as_ref(),
                        &encoded_targets,
                        &mut is_valid_target_master_formid,
                    ),
                    None => false,
                }
            };
            let decoded_rewrite_changed = {
                let mut is_valid_target_master_formid =
                    |raw_form_id: u32| target_master_validity.is_valid_raw_form_id(raw_form_id);
                rewrite_decoded_target_master_formids(
                    &mut record,
                    first_target_master_plugin,
                    output_plugin,
                    &target_masters,
                    mapper.interner,
                    &encoded_targets,
                    target_master_handles_available,
                    &mut is_valid_target_master_formid,
                )
            };
            let xmsp_rewrite_changed = rewrite_xmsp_material_swap_record(
                &mut record,
                &encoded_targets,
                output_plugin,
                &target_masters,
                mapper.interner,
            );
            let raw_rewrite_changed = {
                let mut is_valid_target_master_formid =
                    |raw_form_id: u32| target_master_validity.is_valid_raw_form_id(raw_form_id);
                let mut is_valid_target_master_npc_template_formid = |raw_form_id: u32| {
                    target_master_validity.is_valid_npc_template_raw_form_id(raw_form_id)
                };
                rewrite_record_raw_template_formids_with_master(
                    &mut record,
                    &encoded_targets,
                    &target_masters,
                    first_target_master_plugin,
                    mapper.interner,
                    &target_record_sigs_by_encoded_form_id,
                    &mut is_valid_target_master_formid,
                    &mut is_valid_target_master_npc_template_formid,
                )
            };
            let pack_template_changed = resolve_pack_template_inputs_with_session(
                session,
                &changed_records_by_fk,
                &mut record,
                target_schema.as_ref(),
                output_plugin,
                &target_masters,
                mapper.interner,
            )?;
            if schema_rewrite_changed
                || decoded_rewrite_changed
                || xmsp_rewrite_changed
                || raw_rewrite_changed
                || pack_template_changed
            {
                changed_records_by_fk.insert(record.form_key, record.clone());
                changed_records.push(record);
            }
        }

        report.records_changed = session
            .replace_records_contents(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .try_into()
            .unwrap_or(u32::MAX);

        Ok(report)
    }
}

fn add_output_record_targets(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[String],
    encoded_targets: &mut FxHashMap<u32, u32>,
) -> Result<(), FixupError> {
    let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
    let output_plugin = interner.intern(&output_plugin_name);
    for sig in session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        let form_keys = session
            .form_keys_of_sig(sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in form_keys {
            if fk.local == 0 || fk.plugin != output_plugin {
                continue;
            }
            let Some(encoded) = encode_target_form_id(fk, interner, target_masters) else {
                continue;
            };
            encoded_targets.entry(fk.local).or_insert(encoded);
        }
    }
    Ok(())
}

pub(crate) fn target_record_sigs_by_encoded_form_id(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_masters: &[String],
) -> Result<FxHashMap<u32, crate::ids::SigCode>, FixupError> {
    let mut out = FxHashMap::default();
    for sig in session
        .target_signatures()
        .map_err(|e| FixupError::HandleError(e.to_string()))?
    {
        let form_keys = session
            .form_keys_of_sig(sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        for fk in form_keys {
            let Some(encoded) = encode_target_form_id(fk, interner, target_masters) else {
                continue;
            };
            out.insert(encoded, sig);
        }
    }
    Ok(out)
}

pub(crate) fn encoded_targets_by_source_object_id(
    mapper: &FormKeyMapper,
    target_masters: &[String],
) -> FxHashMap<u32, u32> {
    let mut out = FxHashMap::default();
    for (source, target) in mapper.source_to_target_iter() {
        if let Some(encoded) = encode_target_form_id(target, mapper.interner, target_masters) {
            out.insert(source.local, encoded);
        }
    }
    out
}

pub(crate) fn encode_target_form_id(
    target: FormKey,
    interner: &StringInterner,
    target_masters: &[String],
) -> Option<u32> {
    if target.local == 0 {
        return Some(0);
    }
    let plugin_name = interner.resolve(target.plugin)?;
    let load_index = target_masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case(plugin_name))
        .unwrap_or(target_masters.len());
    if load_index > u8::MAX as usize || target.local > 0x00FF_FFFF {
        return None;
    }
    Some(((load_index as u32) << 24) | target.local)
}

fn rewrite_schema_record_raw_formids(
    record: &mut Record,
    record_spec: &SchemaRecordJson,
    schema: &CompiledSchema,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = false;
    let mut occurrences: FxHashMap<String, usize> = FxHashMap::default();
    for entry in &mut record.fields {
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        let sig = entry.sig.as_str().to_string();
        let occurrence = occurrences.entry(sig.clone()).or_insert(0);
        let sub_spec = schema_subrecord_spec(record_spec, sig.as_str(), *occurrence);
        *occurrence += 1;
        let Some(sub_spec) = sub_spec else {
            continue;
        };

        let mut rewrite_formid = |raw: u32| {
            rewrite_unresolved_target_master_formid(
                raw,
                encoded_targets,
                is_valid_target_master_formid,
            )
        };
        changed |= rewrite_schema_form_ids_in_subrecord(
            sub_spec,
            schema,
            bytes.as_mut_slice(),
            &mut rewrite_formid,
        );
    }
    changed
}

fn rewrite_decoded_target_master_formids(
    record: &mut Record,
    first_target_master_plugin: Option<crate::sym::Sym>,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_master_handles_available: bool,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let Some(first_target_master_plugin) = first_target_master_plugin else {
        return false;
    };

    record.fields.iter_mut().fold(false, |changed, entry| {
        rewrite_decoded_target_master_formid_value(
            &mut entry.value,
            first_target_master_plugin,
            output_plugin,
            target_masters,
            interner,
            encoded_targets,
            target_master_handles_available,
            is_valid_target_master_formid,
        ) | changed
    })
}

fn rewrite_decoded_target_master_formid_value(
    value: &mut FieldValue,
    first_target_master_plugin: crate::sym::Sym,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_master_handles_available: bool,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    match value {
        FieldValue::FormKey(fk)
            if fk.plugin == first_target_master_plugin
                && fk.local != 0
                && fk.local <= 0x00FF_FFFF =>
        {
            if target_master_handles_available && is_valid_target_master_formid(fk.local) {
                return false;
            }
            rewrite_decoded_mapped_formkey(
                fk,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
            )
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_decoded_target_master_formid_value(
                item,
                first_target_master_plugin,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                target_master_handles_available,
                is_valid_target_master_formid,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            rewrite_decoded_target_master_formid_value(
                item,
                first_target_master_plugin,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                target_master_handles_available,
                is_valid_target_master_formid,
            ) | changed
        }),
        _ => false,
    }
}

fn rewrite_xmsp_material_swap_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
) -> bool {
    let xmsp_sig = SubrecordSig(*b"XMSP");
    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig != xmsp_sig {
            continue;
        }
        changed |= match &mut entry.value {
            FieldValue::Bytes(bytes) => rewrite_formid_at(bytes.as_mut_slice(), 0, encoded_targets),
            FieldValue::FormKey(fk) => rewrite_decoded_mapped_formkey(
                fk,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
            ),
            _ => false,
        };
    }
    changed
}

fn rewrite_decoded_mapped_formkey(
    fk: &mut FormKey,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    if fk.local == 0 || fk.local > 0x00FF_FFFF {
        return false;
    }
    let Some(encoded) = encoded_targets.get(&fk.local).copied() else {
        return false;
    };
    let Some(target_fk) =
        form_key_from_encoded_target(encoded, output_plugin, target_masters, interner)
    else {
        return false;
    };
    if *fk == target_fk {
        return false;
    }
    *fk = target_fk;
    true
}

fn rewrite_unresolved_target_master_formid(
    raw: u32,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> Option<u32> {
    if raw == 0 || raw >> 24 != 0 || is_valid_target_master_formid(raw) {
        return None;
    }
    encoded_targets
        .get(&raw)
        .copied()
        .filter(|encoded| *encoded != raw)
}

#[cfg(test)]
fn rewrite_record_raw_template_formids(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut is_valid_target_master_npc_template_formid = |_raw_form_id: u32| false;
    rewrite_record_raw_template_formids_with_master(
        record,
        encoded_targets,
        target_masters,
        None,
        interner,
        target_record_sigs_by_encoded_form_id,
        is_valid_target_master_formid,
        &mut is_valid_target_master_npc_template_formid,
    )
}

fn rewrite_record_raw_template_formids_with_master(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    first_target_master_plugin: Option<crate::sym::Sym>,
    interner: &StringInterner,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    rewrite_object_template_record(
        record,
        encoded_targets,
        interner,
        is_valid_target_master_formid,
    ) | rewrite_omod_data_record(record, encoded_targets, is_valid_target_master_formid)
        | rewrite_npc_template_actor_record(
            record,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            target_masters,
            interner,
            is_valid_target_master_npc_template_formid,
        )
        | rewrite_pack_package_template_record(record, encoded_targets)
        | rewrite_vmad_record(record, encoded_targets)
        | rewrite_raw_condition_record(record, encoded_targets, is_valid_target_master_formid)
        | rewrite_ipds_pnam_record(record, encoded_targets)
        | rewrite_expl_data_record(record, encoded_targets, is_valid_target_master_formid)
        | rewrite_fsts_footstep_set_record(record, encoded_targets, target_masters, interner)
        | rewrite_clmt_weather_record(record, encoded_targets, target_masters, interner)
        | rewrite_regn_weather_record(record, encoded_targets, target_masters, interner)
        | rewrite_wthr_weather_record(
            record,
            encoded_targets,
            target_masters,
            interner,
            is_valid_target_master_formid,
        )
        | rewrite_regn_sound_record(record, encoded_targets)
        | sanitize_regn_references(
            record,
            encoded_targets,
            target_masters,
            interner,
            target_record_sigs_by_encoded_form_id,
            is_valid_target_master_formid,
        )
        | rewrite_rfct_effect_art_record(record, encoded_targets)
        | rewrite_destruction_stage_record(record, encoded_targets)
        | rewrite_ammo_data_record(
            record,
            encoded_targets,
            target_masters,
            interner,
            is_valid_target_master_formid,
        )
        | rewrite_hazd_data_record(record, encoded_targets, is_valid_target_master_formid)
        | rewrite_proj_data_record(record, encoded_targets, is_valid_target_master_formid)
        | rewrite_prps_record(record, encoded_targets, is_valid_target_master_formid)
        | rewrite_stag_sound_record(record, encoded_targets)
        | rewrite_placed_ref_location_record(
            record,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            is_valid_target_master_formid,
        )
        | sanitize_npc_actor_runtime_refs(
            record,
            target_record_sigs_by_encoded_form_id,
            target_masters,
            first_target_master_plugin,
            interner,
            is_valid_target_master_npc_template_formid,
        )
}

fn rewrite_object_template_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    interner: &StringInterner,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = false;
    let record_sig = record.sig;
    let obts_sig = SubrecordSig(*b"OBTS");
    for entry in &mut record.fields {
        if entry.sig != obts_sig {
            continue;
        }
        changed |= match &mut entry.value {
            FieldValue::Bytes(bytes) => rewrite_obts_bytes(
                bytes.as_mut_slice(),
                encoded_targets,
                is_valid_target_master_formid,
            ),
            _ => rewrite_decoded_object_template_properties(
                record_sig,
                &mut entry.value,
                encoded_targets,
                interner,
            ),
        };
    }
    changed
}

fn rewrite_decoded_object_template_properties(
    record_sig: crate::ids::SigCode,
    value: &mut FieldValue,
    encoded_targets: &FxHashMap<u32, u32>,
    interner: &StringInterner,
) -> bool {
    let Some(material_swap_property_id) = material_swap_property_id_for_record(record_sig) else {
        return false;
    };

    match value {
        FieldValue::Struct(fields) => rewrite_decoded_object_template_struct_properties(
            fields.as_mut_slice(),
            material_swap_property_id,
            encoded_targets,
            interner,
        ),
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_decoded_object_template_properties(record_sig, item, encoded_targets, interner)
                | changed
        }),
        _ => false,
    }
}

fn rewrite_decoded_object_template_struct_properties(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    material_swap_property_id: u16,
    encoded_targets: &FxHashMap<u32, u32>,
    interner: &StringInterner,
) -> bool {
    let Some(properties_index) = field_index_canonical(fields, "properties", interner) else {
        return false;
    };
    let FieldValue::List(properties) = &mut fields[properties_index].1 else {
        return false;
    };

    properties.iter_mut().fold(false, |changed, property| {
        rewrite_decoded_object_template_property_row(
            property,
            material_swap_property_id,
            encoded_targets,
            interner,
        ) | changed
    })
}

fn rewrite_decoded_object_template_property_row(
    property: &mut FieldValue,
    material_swap_property_id: u16,
    encoded_targets: &FxHashMap<u32, u32>,
    interner: &StringInterner,
) -> bool {
    let FieldValue::Struct(fields) = property else {
        return false;
    };
    let Some(property_id) = field_index_canonical(fields, "property", interner)
        .and_then(|index| field_value_to_u16(&fields[index].1))
    else {
        return false;
    };
    if property_id != material_swap_property_id {
        return false;
    }

    rewrite_decoded_raw_formid_field(fields, "value1", encoded_targets, interner)
        | rewrite_decoded_raw_formid_field(fields, "value2", encoded_targets, interner)
}

fn rewrite_decoded_raw_formid_field(
    fields: &mut [(crate::sym::Sym, FieldValue)],
    name: &str,
    encoded_targets: &FxHashMap<u32, u32>,
    interner: &StringInterner,
) -> bool {
    let Some(index) = field_index_canonical(fields, name, interner) else {
        return false;
    };
    rewrite_decoded_raw_formid_scalar(&mut fields[index].1, encoded_targets)
}

fn rewrite_decoded_raw_formid_scalar(
    value: &mut FieldValue,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    match value {
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => {
            let raw_value = *raw as u32;
            let Some(encoded) = encoded_source_local_formid(raw_value, encoded_targets) else {
                return false;
            };
            if *raw == encoded as u64 {
                return false;
            }
            *raw = encoded as u64;
            true
        }
        FieldValue::Int(raw) if *raw >= 0 && *raw <= u32::MAX as i64 => {
            let raw_value = *raw as u32;
            let Some(encoded) = encoded_source_local_formid(raw_value, encoded_targets) else {
                return false;
            };
            if *raw == encoded as i64 {
                return false;
            }
            *raw = encoded as i64;
            true
        }
        FieldValue::Bytes(bytes) => rewrite_formid_at(bytes.as_mut_slice(), 0, encoded_targets),
        _ => false,
    }
}

fn encoded_source_local_formid(raw: u32, encoded_targets: &FxHashMap<u32, u32>) -> Option<u32> {
    if raw == 0 || raw >> 24 != 0 {
        return None;
    }
    encoded_targets.get(&raw).copied()
}

fn material_swap_property_id_for_record(record_sig: crate::ids::SigCode) -> Option<u16> {
    match &record_sig.0 {
        b"WEAP" => Some(89),
        b"ARMO" | b"ARMA" => Some(13),
        b"NPC_" => Some(5),
        _ => None,
    }
}

fn field_value_to_u16(value: &FieldValue) -> Option<u16> {
    match value {
        FieldValue::Uint(value) => u16::try_from(*value).ok(),
        FieldValue::Int(value) => u16::try_from(*value).ok(),
        FieldValue::Float(value) if value.is_finite() => {
            let rounded = value.round();
            (0.0..=u16::MAX as f32)
                .contains(&rounded)
                .then_some(rounded as u16)
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
            Some(u16::from_le_bytes([bytes[0], bytes[1]]))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, candidate)| field_value_to_u16(candidate)),
        _ => None,
    }
}

fn field_index_canonical(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<usize> {
    let wanted = canonical_field_name(name);
    fields.iter().position(|(field_name, _)| {
        interner
            .resolve(*field_name)
            .is_some_and(|field_name| canonical_field_name(field_name) == wanted)
    })
}

fn canonical_field_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn rewrite_npc_template_actor_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = false;
    let self_raw = encode_target_form_id(record.form_key, interner, target_masters);
    let default_template_raw = npc_default_template_raw(
        record,
        encoded_targets,
        target_record_sigs_by_encoded_form_id,
        target_masters,
        interner,
        is_valid_target_master_npc_template_formid,
    );
    let tplt_sig = SubrecordSig(*b"TPLT");
    let tpta_sig = SubrecordSig(*b"TPTA");
    for entry in &mut record.fields {
        if entry.sig == tplt_sig {
            changed |= rewrite_npc_default_template_value(
                &mut entry.value,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_npc_template_formid,
            );
            continue;
        }
        if entry.sig != tpta_sig {
            continue;
        }
        changed |= rewrite_npc_template_actor_value(
            &mut entry.value,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            self_raw,
            default_template_raw,
            target_masters,
            interner,
            is_valid_target_master_npc_template_formid,
        );
    }
    changed
}

const FO4_HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const NPC_ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;
const NPC_TEMPLATE_TRAITS: u16 = 0x0001;
const NPC_TEMPLATE_STATS: u16 = 0x0002;
const NPC_TEMPLATE_FACTIONS: u16 = 0x0004;
const NPC_TEMPLATE_SPELL_LIST: u16 = 0x0008;
const NPC_TEMPLATE_AI_DATA: u16 = 0x0010;
const NPC_TEMPLATE_AI_PACKAGES: u16 = 0x0020;
const NPC_TEMPLATE_MODEL_ANIMATION: u16 = 0x0040;
const NPC_TEMPLATE_BASE_DATA: u16 = 0x0080;
const NPC_TEMPLATE_INVENTORY: u16 = 0x0100;
const NPC_TEMPLATE_SCRIPT: u16 = 0x0200;
const NPC_TEMPLATE_DEF_PACKAGE_LIST: u16 = 0x0400;
const NPC_TEMPLATE_ATTACK_DATA: u16 = 0x0800;
const NPC_TEMPLATE_KEYWORDS: u16 = 0x1000;

const NPC_TPTA_SLOT_TEMPLATE_FLAGS: [u16; 13] = [
    NPC_TEMPLATE_TRAITS,
    NPC_TEMPLATE_STATS,
    NPC_TEMPLATE_FACTIONS,
    NPC_TEMPLATE_SPELL_LIST,
    NPC_TEMPLATE_AI_DATA,
    NPC_TEMPLATE_AI_PACKAGES,
    NPC_TEMPLATE_MODEL_ANIMATION,
    NPC_TEMPLATE_BASE_DATA,
    NPC_TEMPLATE_INVENTORY,
    NPC_TEMPLATE_SCRIPT,
    NPC_TEMPLATE_DEF_PACKAGE_LIST,
    NPC_TEMPLATE_ATTACK_DATA,
    NPC_TEMPLATE_KEYWORDS,
];

fn sanitize_npc_actor_runtime_refs(
    record: &mut Record,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    first_target_master_plugin: Option<crate::sym::Sym>,
    interner: &StringInterner,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"NPC_" {
        return false;
    }

    let tplt_sig = SubrecordSig(*b"TPLT");
    let tpta_sig = SubrecordSig(*b"TPTA");
    let mut changed = false;
    let mut template_slots_changed = false;
    let mut retained = smallvec::SmallVec::with_capacity(record.fields.len());
    for mut entry in record.fields.drain(..) {
        if entry.sig == tplt_sig {
            if npc_template_ref_is_valid(
                &entry.value,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_formid,
            ) {
                retained.push(entry);
            } else {
                changed = true;
            }
            continue;
        }

        if entry.sig == tpta_sig
            && sanitize_npc_template_actor_value(
                &mut entry.value,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_formid,
            )
        {
            changed = true;
            template_slots_changed = true;
        }

        if matches!(entry.sig.0, sig if sig == *b"RNAM" || sig == *b"ATKR")
            && repair_npc_human_race_ref_value(
                &mut entry.value,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                first_target_master_plugin,
                interner,
            )
        {
            changed = true;
        }

        retained.push(entry);
    }
    record.fields = retained;

    if template_slots_changed && sync_npc_acbs_template_flags_from_tpta(record, interner) {
        changed = true;
    }

    changed
}

fn npc_template_ref_is_valid(
    value: &FieldValue,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let Some(raw) = encoded_formid_from_value(value, target_masters, interner) else {
        return false;
    };
    npc_template_raw_is_valid(
        raw,
        target_record_sigs_by_encoded_form_id,
        is_valid_target_master_formid,
    )
}

fn npc_template_raw_is_valid(
    raw: u32,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if raw == 0 {
        return false;
    }
    if let Some(sig) = target_record_sigs_by_encoded_form_id.get(&raw) {
        return npc_template_sig_is_valid(sig);
    }
    raw >> 24 == 0 && is_valid_target_master_formid(raw)
}

fn npc_template_actor_raw_is_valid(
    raw: u32,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if raw == 0 {
        return true;
    }
    if let Some(sig) = target_record_sigs_by_encoded_form_id.get(&raw) {
        return npc_template_sig_is_valid(sig);
    }
    raw >> 24 == 0 && is_valid_target_master_formid(raw)
}

fn npc_template_sig_is_valid(sig: &crate::ids::SigCode) -> bool {
    matches!(sig.as_str(), "NPC_" | "LVLN")
}

fn sanitize_npc_template_actor_value(
    value: &mut FieldValue,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            let mut changed = false;
            for offset in (0..bytes.len()).step_by(4) {
                let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
                    continue;
                };
                let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                if !npc_template_actor_raw_is_valid(
                    raw,
                    target_record_sigs_by_encoded_form_id,
                    is_valid_target_master_formid,
                ) {
                    chunk.copy_from_slice(&0u32.to_le_bytes());
                    changed = true;
                }
            }
            changed
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            sanitize_npc_template_actor_value(
                item,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_formid,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            sanitize_npc_template_actor_value(
                item,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_formid,
            ) | changed
        }),
        _ => {
            let Some(raw) = encoded_formid_from_value(value, target_masters, interner) else {
                return false;
            };
            if npc_template_actor_raw_is_valid(
                raw,
                target_record_sigs_by_encoded_form_id,
                is_valid_target_master_formid,
            ) {
                return false;
            }
            *value = FieldValue::Uint(0);
            true
        }
    }
}

fn repair_npc_human_race_ref_value(
    value: &mut FieldValue,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    first_target_master_plugin: Option<crate::sym::Sym>,
    interner: &StringInterner,
) -> bool {
    let Some(first_target_master_plugin) = first_target_master_plugin else {
        return false;
    };

    match value {
        FieldValue::FormKey(fk) => {
            if fk.plugin == first_target_master_plugin || fk.local != FO4_HUMAN_RACE_LOCAL {
                return false;
            }
            if encode_target_form_id(*fk, interner, target_masters).is_some_and(|raw| {
                target_record_sigs_by_encoded_form_id
                    .get(&raw)
                    .is_some_and(|sig| sig.as_str() == "RACE")
            }) {
                return false;
            }
            *fk = FormKey {
                local: FO4_HUMAN_RACE_LOCAL,
                plugin: first_target_master_plugin,
            };
            true
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => repair_raw_human_race_ref(
            bytes.as_mut_slice(),
            0,
            target_record_sigs_by_encoded_form_id,
            target_masters,
        ),
        FieldValue::Uint(raw) if (*raw & 0x00FF_FFFF) == FO4_HUMAN_RACE_LOCAL as u64 => {
            let raw32 = *raw as u32;
            if raw32 >> 24 == 0
                || target_record_sigs_by_encoded_form_id
                    .get(&raw32)
                    .is_some_and(|sig| sig.as_str() == "RACE")
            {
                return false;
            }
            *raw = FO4_HUMAN_RACE_LOCAL as u64;
            true
        }
        FieldValue::Int(raw)
            if *raw >= 0 && (*raw as u32 & 0x00FF_FFFF) == FO4_HUMAN_RACE_LOCAL =>
        {
            let raw32 = *raw as u32;
            if raw32 >> 24 == 0
                || target_record_sigs_by_encoded_form_id
                    .get(&raw32)
                    .is_some_and(|sig| sig.as_str() == "RACE")
            {
                return false;
            }
            *raw = FO4_HUMAN_RACE_LOCAL as i64;
            true
        }
        _ => false,
    }
}

fn repair_raw_human_race_ref(
    bytes: &mut [u8],
    offset: usize,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
) -> bool {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return false;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw & 0x00FF_FFFF != FO4_HUMAN_RACE_LOCAL || raw >> 24 == 0 {
        return false;
    }
    if target_record_sigs_by_encoded_form_id
        .get(&raw)
        .is_some_and(|sig| sig.as_str() == "RACE")
    {
        return false;
    }
    let master_raw = FO4_HUMAN_RACE_LOCAL;
    if target_masters.is_empty() {
        return false;
    }
    chunk.copy_from_slice(&master_raw.to_le_bytes());
    true
}

fn encoded_formid_from_value(
    value: &FieldValue,
    target_masters: &[String],
    interner: &StringInterner,
) -> Option<u32> {
    match value {
        FieldValue::FormKey(fk) => encode_target_form_id(*fk, interner, target_masters),
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => Some(*raw as u32),
        FieldValue::Int(raw) if *raw >= 0 && *raw <= u32::MAX as i64 => Some(*raw as u32),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().unwrap()))
        }
        _ => None,
    }
}

fn sync_npc_acbs_template_flags_from_tpta(record: &mut Record, interner: &StringInterner) -> bool {
    let flags = npc_tpta_template_flags(record);
    let acbs_sig = SubrecordSig(*b"ACBS");
    for entry in &mut record.fields {
        if entry.sig != acbs_sig {
            continue;
        }
        return set_npc_acbs_template_flags(&mut entry.value, flags, interner);
    }
    false
}

fn npc_tpta_template_flags(record: &Record) -> u16 {
    let tpta_sig = SubrecordSig(*b"TPTA");
    record
        .fields
        .iter()
        .find(|entry| entry.sig == tpta_sig)
        .map(|entry| template_flags_for_tpta_value(&entry.value))
        .unwrap_or(0)
}

fn template_flags_for_tpta_value(value: &FieldValue) -> u16 {
    match value {
        FieldValue::Bytes(bytes) => bytes
            .chunks_exact(4)
            .enumerate()
            .filter_map(|(slot_index, chunk)| {
                let flag = NPC_TPTA_SLOT_TEMPLATE_FLAGS
                    .get(slot_index)
                    .copied()
                    .unwrap_or(0);
                (flag != 0 && chunk != [0, 0, 0, 0]).then_some(flag)
            })
            .fold(0, |flags, flag| flags | flag),
        FieldValue::List(items) => items
            .iter()
            .enumerate()
            .filter_map(|(slot_index, item)| {
                let flag = NPC_TPTA_SLOT_TEMPLATE_FLAGS
                    .get(slot_index)
                    .copied()
                    .unwrap_or(0);
                (flag != 0 && npc_template_slot_is_nonzero(item)).then_some(flag)
            })
            .fold(0, |flags, flag| flags | flag),
        FieldValue::Struct(fields) => fields
            .iter()
            .enumerate()
            .filter_map(|(slot_index, (_, item))| {
                let flag = NPC_TPTA_SLOT_TEMPLATE_FLAGS
                    .get(slot_index)
                    .copied()
                    .unwrap_or(0);
                (flag != 0 && npc_template_slot_is_nonzero(item)).then_some(flag)
            })
            .fold(0, |flags, flag| flags | flag),
        _ => 0,
    }
}

fn npc_template_slot_is_nonzero(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => false,
        FieldValue::Bool(value) => *value,
        FieldValue::Int(value) => *value != 0,
        FieldValue::Uint(value) => *value != 0,
        FieldValue::Float(value) => *value != 0.0,
        FieldValue::String(_) => true,
        FieldValue::Bytes(bytes) => bytes.iter().any(|byte| *byte != 0),
        FieldValue::FormKey(fk) => fk.local != 0,
        FieldValue::List(items) => items.iter().any(npc_template_slot_is_nonzero),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| npc_template_slot_is_nonzero(value)),
    }
}

fn set_npc_acbs_template_flags(
    value: &mut FieldValue,
    flags: u16,
    interner: &StringInterner,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2 => {
            let old = u16::from_le_bytes(
                bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET..NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                    .try_into()
                    .unwrap(),
            );
            if old == flags {
                return false;
            }
            bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET..NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                .copy_from_slice(&flags.to_le_bytes());
            true
        }
        FieldValue::Struct(fields) => {
            let key = interner.intern("TemplateFlags");
            let new_value = FieldValue::List(template_flag_values(flags, interner));
            if let Some((_, existing)) = fields.iter_mut().find(|(field, _)| *field == key) {
                if *existing == new_value {
                    return false;
                }
                *existing = new_value;
            } else {
                fields.push((key, new_value));
            }
            true
        }
        _ => false,
    }
}

fn template_flag_values(flags: u16, interner: &StringInterner) -> Vec<FieldValue> {
    [
        (NPC_TEMPLATE_TRAITS, "Traits"),
        (NPC_TEMPLATE_STATS, "Stats"),
        (NPC_TEMPLATE_FACTIONS, "Factions"),
        (NPC_TEMPLATE_AI_DATA, "AIData"),
        (NPC_TEMPLATE_AI_PACKAGES, "AIPackages"),
        (NPC_TEMPLATE_MODEL_ANIMATION, "ModelAnimation"),
        (NPC_TEMPLATE_BASE_DATA, "BaseData"),
        (NPC_TEMPLATE_INVENTORY, "Inventory"),
        (NPC_TEMPLATE_SCRIPT, "Script"),
    ]
    .into_iter()
    .filter_map(|(flag, name)| {
        (flags & flag != 0).then(|| FieldValue::String(interner.intern(name)))
    })
    .collect()
}

fn rewrite_npc_template_actor_value(
    value: &mut FieldValue,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    self_raw: Option<u32>,
    default_template_raw: Option<u32>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            let mut changed = false;
            for offset in (0..bytes.len()).step_by(4) {
                changed |= rewrite_npc_template_formid_at(
                    bytes.as_mut_slice(),
                    offset,
                    encoded_targets,
                    target_record_sigs_by_encoded_form_id,
                    target_masters,
                    is_valid_target_master_npc_template_formid,
                );
                if let Some(self_raw) = self_raw {
                    changed |= rewrite_self_template_formid_at(
                        bytes.as_mut_slice(),
                        offset,
                        self_raw,
                        default_template_raw,
                    );
                }
            }
            changed
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_npc_template_actor_value(
                item,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                self_raw,
                default_template_raw,
                target_masters,
                interner,
                is_valid_target_master_npc_template_formid,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            rewrite_npc_template_actor_value(
                item,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                self_raw,
                default_template_raw,
                target_masters,
                interner,
                is_valid_target_master_npc_template_formid,
            ) | changed
        }),
        _ => rewrite_npc_template_actor_scalar(
            value,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            self_raw,
            default_template_raw,
            target_masters,
            interner,
            is_valid_target_master_npc_template_formid,
        ),
    }
}

fn rewrite_npc_template_actor_scalar(
    value: &mut FieldValue,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    self_raw: Option<u32>,
    default_template_raw: Option<u32>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let raw = match value {
        FieldValue::Uint(raw) if *raw <= u32::MAX as u64 => *raw as u32,
        FieldValue::Int(raw) if *raw >= 0 && *raw <= u32::MAX as i64 => *raw as u32,
        FieldValue::FormKey(fk) => {
            let Some(raw) = encode_target_form_id(*fk, interner, target_masters) else {
                return false;
            };
            raw
        }
        _ => return false,
    };

    let mut replacement = preferred_npc_template_target_raw(
        raw,
        encoded_targets,
        target_record_sigs_by_encoded_form_id,
        target_masters,
        is_valid_target_master_npc_template_formid,
    );
    if self_raw.is_some_and(|self_raw| replacement == self_raw) {
        replacement = default_template_raw
            .filter(|candidate| *candidate != 0 && Some(*candidate) != self_raw)
            .unwrap_or(0);
    }
    if replacement == raw {
        return false;
    }

    match value {
        FieldValue::Uint(raw) => *raw = replacement as u64,
        FieldValue::Int(raw) => *raw = replacement as i64,
        _ => *value = FieldValue::Uint(replacement as u64),
    }
    true
}

fn rewrite_npc_template_formid_at(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return false;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    let replacement = preferred_npc_template_target_raw(
        raw,
        encoded_targets,
        target_record_sigs_by_encoded_form_id,
        target_masters,
        is_valid_target_master_npc_template_formid,
    );
    if replacement == raw {
        return false;
    }
    chunk.copy_from_slice(&replacement.to_le_bytes());
    true
}

fn preferred_npc_template_target_raw(
    raw: u32,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> u32 {
    if raw == 0 {
        return raw;
    }
    let object_id = raw & 0x00FF_FFFF;
    if is_valid_target_master_npc_template_formid(object_id) {
        return object_id;
    }
    let output_raw = ((target_masters.len() as u32) << 24) | object_id;
    if target_record_sigs_by_encoded_form_id
        .get(&output_raw)
        .is_some_and(npc_template_sig_is_valid)
    {
        return output_raw;
    }
    encoded_source_local_formid(raw, encoded_targets).unwrap_or(raw)
}

fn rewrite_npc_default_template_value(
    value: &mut FieldValue,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => rewrite_npc_template_formid_at(
            bytes.as_mut_slice(),
            0,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            target_masters,
            is_valid_target_master_npc_template_formid,
        ),
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_npc_default_template_value(
                item,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_npc_template_formid,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            rewrite_npc_default_template_value(
                item,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                target_masters,
                interner,
                is_valid_target_master_npc_template_formid,
            ) | changed
        }),
        _ => rewrite_npc_template_actor_scalar(
            value,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            None,
            None,
            target_masters,
            interner,
            is_valid_target_master_npc_template_formid,
        ),
    }
}

fn npc_default_template_raw(
    record: &Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_npc_template_formid: &mut dyn FnMut(u32) -> bool,
) -> Option<u32> {
    let tplt_sig = SubrecordSig(*b"TPLT");
    for entry in &record.fields {
        if entry.sig != tplt_sig {
            continue;
        }
        return match &entry.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                let raw = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
                Some(preferred_npc_template_target_raw(
                    raw,
                    encoded_targets,
                    target_record_sigs_by_encoded_form_id,
                    target_masters,
                    is_valid_target_master_npc_template_formid,
                ))
            }
            FieldValue::FormKey(fk) => {
                encode_target_form_id(*fk, interner, target_masters).map(|raw| {
                    preferred_npc_template_target_raw(
                        raw,
                        encoded_targets,
                        target_record_sigs_by_encoded_form_id,
                        target_masters,
                        is_valid_target_master_npc_template_formid,
                    )
                })
            }
            FieldValue::Uint(value) if *value <= u32::MAX as u64 => {
                Some(preferred_npc_template_target_raw(
                    *value as u32,
                    encoded_targets,
                    target_record_sigs_by_encoded_form_id,
                    target_masters,
                    is_valid_target_master_npc_template_formid,
                ))
            }
            _ => None,
        };
    }
    None
}

fn rewrite_self_template_formid_at(
    bytes: &mut [u8],
    offset: usize,
    self_raw: u32,
    default_template_raw: Option<u32>,
) -> bool {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return false;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw != self_raw {
        return false;
    }
    let replacement = default_template_raw
        .filter(|candidate| *candidate != 0 && *candidate != self_raw)
        .unwrap_or(0);
    chunk.copy_from_slice(&replacement.to_le_bytes());
    true
}

fn rewrite_omod_data_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"OMOD" {
        return false;
    }

    let mut changed = false;
    let mods_sig = SubrecordSig(*b"MODS");
    let data_sig = SubrecordSig(*b"DATA");
    for entry in &mut record.fields {
        if entry.sig == mods_sig {
            if let FieldValue::Bytes(bytes) = &mut entry.value {
                changed |= rewrite_formid_at(bytes.as_mut_slice(), 0, encoded_targets);
            }
        } else if entry.sig == data_sig {
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            changed |= rewrite_omod_data_bytes(
                bytes.as_mut_slice(),
                encoded_targets,
                is_valid_target_master_formid,
            );
        };
    }
    changed
}

fn rewrite_obts_bytes(
    bytes: &mut [u8],
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    const FIXED_HEADER_LEN: usize = 16;
    const INCLUDE_ROW_LEN: usize = 7;
    const PROPERTY_ROW_LEN: usize = 24;

    if bytes.len() < FIXED_HEADER_LEN + 2 {
        return false;
    }

    let include_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let keyword_count = bytes[15] as usize;

    let keywords_start = FIXED_HEADER_LEN;
    let Some(after_keywords) = keyword_count
        .checked_mul(4)
        .and_then(|len| keywords_start.checked_add(len))
    else {
        return false;
    };
    let Some(includes_start) = after_keywords.checked_add(2) else {
        return false;
    };
    let Some(includes_end) = include_count
        .checked_mul(INCLUDE_ROW_LEN)
        .and_then(|len| includes_start.checked_add(len))
    else {
        return false;
    };
    if bytes.len() < includes_end {
        return false;
    }
    let can_scan_properties = property_count
        .checked_mul(PROPERTY_ROW_LEN)
        .and_then(|len| includes_end.checked_add(len))
        .is_some_and(|properties_end| properties_end <= bytes.len());

    let mut changed = false;
    for index in 0..keyword_count {
        changed |= rewrite_formid_at(bytes, keywords_start + index * 4, encoded_targets);
    }
    for index in 0..include_count {
        changed |= rewrite_formid_at(
            bytes,
            includes_start + index * INCLUDE_ROW_LEN,
            encoded_targets,
        );
    }
    if can_scan_properties {
        for index in 0..property_count {
            let row_start = includes_end + index * PROPERTY_ROW_LEN;
            // Value 1 is a FormID for every value_type in {4,6}; Value 2 only for
            // the material-swap special case. (See rewrite_omod_data_bytes.)
            if property_value1_is_formid(bytes, row_start) {
                changed |= rewrite_property_value1_formid(
                    bytes,
                    row_start + 12,
                    encoded_targets,
                    is_valid_target_master_formid,
                );
            } else if is_material_swap_property(bytes, row_start) {
                changed |= rewrite_formid_at(bytes, row_start + 12, encoded_targets);
            }
            if is_material_swap_property(bytes, row_start) {
                changed |= rewrite_formid_at(bytes, row_start + 16, encoded_targets);
            }
        }
    }
    changed
}

fn is_material_swap_property(bytes: &[u8], row_start: usize) -> bool {
    let Some(property_bytes) = bytes.get(row_start + 8..row_start + 10) else {
        return false;
    };
    matches!(
        u16::from_le_bytes([property_bytes[0], property_bytes[1]]),
        5 | 13 | 89
    )
}

/// Whether an OMOD/OBTS property row's `Value 1` slot holds a FormID.
///
/// Per xEdit `wbOMODDataPropertyValue1Decider` (wbDefinitionsFO4.pas:2775),
/// `Value 1` decodes as a FormID exactly when `Value Type` (the first byte of
/// the 24-byte row) is 4 (`FormID,Int`) or 6 (`FormID,Float`). For value types
/// 0/1/2/5 it is Int/Float/Bool/Enum and must not be touched.
fn property_value1_is_formid(bytes: &[u8], row_start: usize) -> bool {
    matches!(bytes.get(row_start), Some(4) | Some(6))
}

/// Rewrite a property `Value 1` FormID at `offset`, then null any residue.
///
/// `rewrite_formid_at` rewrites the slot when the source-local id was converted
/// (present in `encoded_targets`). A FormID-typed `Value 1` that is still a
/// source-local `00xxxxxx` afterward points at a FO76-only / skipped record
/// that has no FO4 counterpart; left as-is it resolves to a non-existent
/// `Fallout4.esm` form. xEdit treats `Value 1 - FormID = NULL` as a valid empty
/// ref, so zero it. Returns true if either step changed the bytes.
fn rewrite_property_value1_formid(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = rewrite_formid_at(bytes, offset, encoded_targets);
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return changed;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    // Only a source-local (master byte 0) residue that no target master claims
    // is dangling. Already-target-encoded (master byte != 0) and valid first-master
    // ids are left untouched.
    if raw != 0 && raw >> 24 == 0 && !is_valid_target_master_formid(raw) {
        chunk.copy_from_slice(&0u32.to_le_bytes());
        changed = true;
    }
    changed
}

fn rewrite_omod_data_bytes(
    bytes: &mut [u8],
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    const HEADER_LEN: usize = 20;
    const ATTACH_POINT_OFFSET: usize = 16;
    const ATTACH_PARENT_SLOT_COUNT_LEN: usize = 4;
    const ITEM_ROW_LEN: usize = 4;
    const INCLUDE_ROW_LEN: usize = 7;
    const PROPERTY_ROW_LEN: usize = 24;

    if bytes.len() < HEADER_LEN + ATTACH_PARENT_SLOT_COUNT_LEN {
        return false;
    }

    let include_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap()) as usize;
    let property_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap()) as usize;
    let attach_parent_slot_count =
        u32::from_le_bytes(bytes[HEADER_LEN..HEADER_LEN + 4].try_into().unwrap()) as usize;
    let Some(attach_parent_slots_len) = attach_parent_slot_count.checked_mul(4) else {
        return false;
    };
    let Some(includes_len) = include_count.checked_mul(INCLUDE_ROW_LEN) else {
        return false;
    };
    let Some(properties_len) = property_count.checked_mul(PROPERTY_ROW_LEN) else {
        return false;
    };

    let attach_parent_slots_start = HEADER_LEN + ATTACH_PARENT_SLOT_COUNT_LEN;
    let Some(item_start) = attach_parent_slots_start.checked_add(attach_parent_slots_len) else {
        return false;
    };
    let Some(includes_start) = item_start.checked_add(ITEM_ROW_LEN) else {
        return false;
    };
    let Some(includes_end) = includes_start.checked_add(includes_len) else {
        return false;
    };
    let Some(properties_end) = includes_end.checked_add(properties_len) else {
        return false;
    };
    if properties_end > bytes.len() {
        return false;
    }

    let mut changed = rewrite_formid_at(bytes, ATTACH_POINT_OFFSET, encoded_targets);
    for index in 0..attach_parent_slot_count {
        changed |= rewrite_formid_at(
            bytes,
            attach_parent_slots_start + index * 4,
            encoded_targets,
        );
    }
    for index in 0..include_count {
        changed |= rewrite_formid_at(
            bytes,
            includes_start + index * INCLUDE_ROW_LEN,
            encoded_targets,
        );
    }
    for index in 0..property_count {
        let row_start = includes_end + index * PROPERTY_ROW_LEN;
        // Value 1 is a FormID for every value_type in {4,6} (xEdit Value1 decider),
        // not just the material-swap property ids. Rewrite all of them, then
        // null any source-local residue the conversion couldn't map.
        if property_value1_is_formid(bytes, row_start) {
            changed |= rewrite_property_value1_formid(
                bytes,
                row_start + 12,
                encoded_targets,
                is_valid_target_master_formid,
            );
        } else if is_material_swap_property(bytes, row_start) {
            // Material-swap rows carry a swap FormID in Value 1 regardless of
            // value_type; remap it but leave unmapped residue (do not null).
            changed |= rewrite_formid_at(bytes, row_start + 12, encoded_targets);
        }
        // Value 2 only carries a FormID for the material-swap special case (the
        // Value2 decider makes value_type 4/6 an Int/Float). Keep that branch.
        if is_material_swap_property(bytes, row_start) {
            changed |= rewrite_formid_at(bytes, row_start + 16, encoded_targets);
        }
    }
    changed
}

fn rewrite_pack_package_template_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    if record.sig.0 != *b"PACK" {
        return false;
    }

    let mut changed = false;
    let pkcu_sig = SubrecordSig(*b"PKCU");
    for entry in &mut record.fields {
        if entry.sig != pkcu_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_formid_at(bytes.as_mut_slice(), 4, encoded_targets);
    }
    changed
}

fn rewrite_vmad_record(record: &mut Record, encoded_targets: &FxHashMap<u32, u32>) -> bool {
    let mut changed = false;
    let record_sig = record.sig.0;
    let vmad_sig = SubrecordSig(*b"VMAD");
    for entry in &mut record.fields {
        if entry.sig != vmad_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_vmad_bytes(bytes.as_mut_slice(), encoded_targets, &record_sig);
    }
    changed
}

fn rewrite_vmad_bytes(
    bytes: &mut [u8],
    encoded_targets: &FxHashMap<u32, u32>,
    record_sig: &[u8; 4],
) -> bool {
    let mut rewritten = bytes.to_vec();
    let Some(changed) =
        rewrite_vmad_bytes_inner(rewritten.as_mut_slice(), encoded_targets, record_sig)
    else {
        return false;
    };
    if changed {
        bytes.copy_from_slice(&rewritten);
    }
    changed
}

fn rewrite_vmad_bytes_inner(
    bytes: &mut [u8],
    encoded_targets: &FxHashMap<u32, u32>,
    record_sig: &[u8; 4],
) -> Option<bool> {
    if bytes.len() < 6 {
        return None;
    }
    let object_format = read_u16_at(bytes, 2)?;
    let script_count = read_u16_at(bytes, 4)? as usize;
    let mut offset = 6usize;
    let mut changed = false;
    for _ in 0..script_count {
        changed |= rewrite_vmad_script_entry(bytes, &mut offset, object_format, encoded_targets)?;
    }
    if record_sig == b"QUST" && offset < bytes.len() {
        if let Some(fragment_changed) =
            rewrite_vmad_qust_after_scripts(bytes, &mut offset, object_format, encoded_targets)
        {
            changed |= fragment_changed;
        }
    }
    Some(changed)
}

fn rewrite_vmad_qust_after_scripts(
    bytes: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    encoded_targets: &FxHashMap<u32, u32>,
) -> Option<bool> {
    advance_vmad_offset(bytes, offset, 1)?;
    let fragment_count = read_u16_at(bytes, *offset)? as usize;
    advance_vmad_offset(bytes, offset, 2)?;

    let script_name_len = read_u16_at(bytes, *offset)? as usize;
    advance_vmad_offset(bytes, offset, 2)?;
    let mut changed = false;
    if script_name_len > 0 {
        advance_vmad_offset(bytes, offset, script_name_len)?;
        advance_vmad_offset(bytes, offset, 1)?;
        let property_count = read_u16_at(bytes, *offset)? as usize;
        advance_vmad_offset(bytes, offset, 2)?;
        for _ in 0..property_count {
            skip_vmad_string(bytes, offset)?;
            let property_type = *bytes.get(*offset)?;
            advance_vmad_offset(bytes, offset, 1)?;
            advance_vmad_offset(bytes, offset, 1)?;
            changed |= rewrite_vmad_property_value(
                bytes,
                offset,
                property_type,
                object_format,
                encoded_targets,
            )?;
        }
    }

    for _ in 0..fragment_count {
        advance_vmad_offset(bytes, offset, 2)?;
        advance_vmad_offset(bytes, offset, 2)?;
        advance_vmad_offset(bytes, offset, 4)?;
        advance_vmad_offset(bytes, offset, 1)?;
        skip_vmad_string(bytes, offset)?;
        skip_vmad_string(bytes, offset)?;
    }

    let alias_count = read_u16_at(bytes, *offset)? as usize;
    advance_vmad_offset(bytes, offset, 2)?;
    for _ in 0..alias_count {
        changed |= rewrite_vmad_object(bytes, offset, object_format, encoded_targets)?;
        advance_vmad_offset(bytes, offset, 2)?;
        let alias_object_format = read_u16_at(bytes, *offset)?;
        advance_vmad_offset(bytes, offset, 2)?;
        let alias_script_count = read_u16_at(bytes, *offset)? as usize;
        advance_vmad_offset(bytes, offset, 2)?;
        for _ in 0..alias_script_count {
            changed |=
                rewrite_vmad_script_entry(bytes, offset, alias_object_format, encoded_targets)?;
        }
    }
    Some(changed)
}

fn rewrite_vmad_script_entry(
    bytes: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    encoded_targets: &FxHashMap<u32, u32>,
) -> Option<bool> {
    skip_vmad_string(bytes, offset)?;
    advance_vmad_offset(bytes, offset, 1)?;
    let property_count = read_u16_at(bytes, *offset)? as usize;
    advance_vmad_offset(bytes, offset, 2)?;

    let mut changed = false;
    for _ in 0..property_count {
        skip_vmad_string(bytes, offset)?;
        let property_type = *bytes.get(*offset)?;
        advance_vmad_offset(bytes, offset, 1)?;
        advance_vmad_offset(bytes, offset, 1)?;
        changed |= rewrite_vmad_property_value(
            bytes,
            offset,
            property_type,
            object_format,
            encoded_targets,
        )?;
    }
    Some(changed)
}

fn rewrite_vmad_property_value(
    bytes: &mut [u8],
    offset: &mut usize,
    property_type: u8,
    object_format: u16,
    encoded_targets: &FxHashMap<u32, u32>,
) -> Option<bool> {
    match property_type {
        0 | 6 => Some(false),
        1 => rewrite_vmad_object(bytes, offset, object_format, encoded_targets),
        2 => {
            skip_vmad_string(bytes, offset)?;
            Some(false)
        }
        3 | 4 => {
            advance_vmad_offset(bytes, offset, 4)?;
            Some(false)
        }
        5 => {
            advance_vmad_offset(bytes, offset, 1)?;
            Some(false)
        }
        7 => rewrite_vmad_struct(bytes, offset, object_format, encoded_targets),
        11 => {
            let count = read_nonnegative_vmad_count(bytes, offset)?;
            let mut changed = false;
            for _ in 0..count {
                changed |= rewrite_vmad_object(bytes, offset, object_format, encoded_targets)?;
            }
            Some(changed)
        }
        12 => {
            let count = read_nonnegative_vmad_count(bytes, offset)?;
            for _ in 0..count {
                skip_vmad_string(bytes, offset)?;
            }
            Some(false)
        }
        13 | 14 => {
            let count = read_nonnegative_vmad_count(bytes, offset)?;
            advance_vmad_offset(bytes, offset, count.checked_mul(4)?)?;
            Some(false)
        }
        15 => {
            let count = read_nonnegative_vmad_count(bytes, offset)?;
            advance_vmad_offset(bytes, offset, count)?;
            Some(false)
        }
        16 => {
            advance_vmad_offset(bytes, offset, 4)?;
            Some(false)
        }
        17 => {
            let count = read_nonnegative_vmad_count(bytes, offset)?;
            let mut changed = false;
            for _ in 0..count {
                changed |= rewrite_vmad_struct(bytes, offset, object_format, encoded_targets)?;
            }
            Some(changed)
        }
        _ => None,
    }
}

fn rewrite_vmad_struct(
    bytes: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    encoded_targets: &FxHashMap<u32, u32>,
) -> Option<bool> {
    let member_count = read_nonnegative_vmad_count(bytes, offset)?;
    let mut changed = false;
    for _ in 0..member_count {
        skip_vmad_string(bytes, offset)?;
        let member_type = *bytes.get(*offset)?;
        advance_vmad_offset(bytes, offset, 1)?;
        advance_vmad_offset(bytes, offset, 1)?;
        changed |= rewrite_vmad_property_value(
            bytes,
            offset,
            member_type,
            object_format,
            encoded_targets,
        )?;
    }
    Some(changed)
}

fn rewrite_vmad_object(
    bytes: &mut [u8],
    offset: &mut usize,
    object_format: u16,
    encoded_targets: &FxHashMap<u32, u32>,
) -> Option<bool> {
    bytes.get(*offset..(*offset).checked_add(8)?)?;
    let formid_offset = if object_format == 2 {
        (*offset).checked_add(4)?
    } else {
        *offset
    };
    let changed = rewrite_formid_at(bytes, formid_offset, encoded_targets);
    advance_vmad_offset(bytes, offset, 8)?;
    Some(changed)
}

fn skip_vmad_string(bytes: &[u8], offset: &mut usize) -> Option<()> {
    let len = read_u16_at(bytes, *offset)? as usize;
    advance_vmad_offset(bytes, offset, 2)?;
    advance_vmad_offset(bytes, offset, len)
}

fn read_nonnegative_vmad_count(bytes: &[u8], offset: &mut usize) -> Option<usize> {
    let count = read_i32_at(bytes, *offset)?;
    advance_vmad_offset(bytes, offset, 4)?;
    if count < 0 {
        return None;
    }
    usize::try_from(count).ok()
}

fn advance_vmad_offset(bytes: &[u8], offset: &mut usize, len: usize) -> Option<()> {
    let end = (*offset).checked_add(len)?;
    bytes.get(*offset..end)?;
    *offset = end;
    Some(())
}

fn read_u16_at(bytes: &[u8], offset: usize) -> Option<u16> {
    let chunk = bytes.get(offset..offset.checked_add(2)?)?;
    Some(u16::from_le_bytes([chunk[0], chunk[1]]))
}

fn read_i32_at(bytes: &[u8], offset: usize) -> Option<i32> {
    let chunk = bytes.get(offset..offset.checked_add(4)?)?;
    Some(i32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
}

#[derive(Clone, Copy)]
struct PackPkcu {
    data_input_count: u32,
    package_template: u32,
    version: u32,
}

fn resolve_pack_template_inputs_with_session(
    session: &mut PluginSession,
    rewritten_records: &FxHashMap<FormKey, Record>,
    record: &mut Record,
    target_schema: &AuthoringSchema,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
) -> Result<bool, FixupError> {
    let Some((_, pkcu)) = read_pack_pkcu(record) else {
        return Ok(false);
    };
    if pkcu.package_template == 0 {
        return Ok(false);
    }
    let Some(template_fk) = form_key_from_encoded_target(
        pkcu.package_template,
        output_plugin,
        target_masters,
        interner,
    ) else {
        return Ok(false);
    };
    if let Some(changed) =
        resolve_pack_template_inputs_from_rewritten_record(rewritten_records, &template_fk, record)
    {
        return Ok(changed);
    }
    let template = match session.record_decoded(&template_fk, target_schema, interner) {
        Ok(template) => template,
        Err(_) => return Ok(false),
    };
    Ok(resolve_pack_template_inputs_from_template(
        record, &template,
    ))
}

fn resolve_pack_template_inputs_from_rewritten_record(
    rewritten_records: &FxHashMap<FormKey, Record>,
    template_fk: &FormKey,
    record: &mut Record,
) -> Option<bool> {
    rewritten_records
        .get(template_fk)
        .map(|template| resolve_pack_template_inputs_from_template(record, template))
}

fn resolve_pack_template_inputs_from_template(record: &mut Record, template: &Record) -> bool {
    if record.sig.0 != *b"PACK" || template.sig.0 != *b"PACK" {
        return false;
    }

    let Some((pkcu_pos, instance_pkcu)) = read_pack_pkcu(record) else {
        return false;
    };
    let Some((_, template_pkcu)) = read_pack_pkcu(template) else {
        return false;
    };
    if instance_pkcu.package_template == 0 || instance_pkcu.version >= template_pkcu.version {
        return false;
    }

    let mut existing_unams = pack_unam_values(record, pkcu_pos);
    let mut chunks_to_insert = Vec::new();
    for chunk in pack_input_chunks(template) {
        let Some(unam) = chunk.unam else {
            continue;
        };
        if existing_unams.contains(&unam) {
            continue;
        }
        existing_unams.push(unam);
        chunks_to_insert.push(chunk);
    }

    if !chunks_to_insert.is_empty() {
        insert_pack_input_chunks(record, pkcu_pos, chunks_to_insert);
    }

    let new_input_count = pack_data_input_count(record, pkcu_pos);
    if new_input_count < template_pkcu.data_input_count {
        return false;
    }
    let pkcu_changed =
        write_pack_pkcu_count_and_version(record, pkcu_pos, new_input_count, template_pkcu.version);
    copy_pack_xnam_from_template(record, template) | pkcu_changed
}

fn read_pack_pkcu(record: &Record) -> Option<(usize, PackPkcu)> {
    if record.sig.0 != *b"PACK" {
        return None;
    }
    let pkcu_sig = SubrecordSig(*b"PKCU");
    let (index, entry) = record
        .fields
        .iter()
        .enumerate()
        .find(|(_, entry)| entry.sig == pkcu_sig)?;
    let FieldValue::Bytes(bytes) = &entry.value else {
        return None;
    };
    if bytes.len() < 12 {
        return None;
    }
    Some((
        index,
        PackPkcu {
            data_input_count: u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            package_template: u32::from_le_bytes(bytes[4..8].try_into().ok()?),
            version: u32::from_le_bytes(bytes[8..12].try_into().ok()?),
        },
    ))
}

fn write_pack_pkcu_count_and_version(
    record: &mut Record,
    pkcu_pos: usize,
    data_input_count: u32,
    version: u32,
) -> bool {
    let Some(entry) = record.fields.get_mut(pkcu_pos) else {
        return false;
    };
    let FieldValue::Bytes(bytes) = &mut entry.value else {
        return false;
    };
    if bytes.len() < 12 {
        return false;
    }

    let mut changed = false;
    if bytes[0..4] != data_input_count.to_le_bytes() {
        bytes[0..4].copy_from_slice(&data_input_count.to_le_bytes());
        changed = true;
    }
    if bytes[8..12] != version.to_le_bytes() {
        bytes[8..12].copy_from_slice(&version.to_le_bytes());
        changed = true;
    }
    changed
}

struct PackInputChunk {
    unam: Option<u32>,
    data_fields: Vec<FieldEntry>,
    unam_field: Option<FieldEntry>,
}

fn pack_input_chunks(record: &Record) -> Vec<PackInputChunk> {
    let Some((pkcu_pos, _)) = read_pack_pkcu(record) else {
        return Vec::new();
    };
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    if let Some(layout) = pack_trailing_unam_layout(record, pkcu_pos, data_end_pos) {
        let mut chunks = Vec::with_capacity(layout.anam_indices.len());
        for (input_index, data_start) in layout.anam_indices.iter().copied().enumerate() {
            let data_end = layout
                .anam_indices
                .get(input_index + 1)
                .copied()
                .unwrap_or(layout.first_unam_pos);
            let unam_index = layout.unam_indices[input_index];
            let unam_field = record.fields[unam_index].clone();
            chunks.push(PackInputChunk {
                unam: pack_unam_value(&unam_field),
                data_fields: record.fields[data_start..data_end].to_vec(),
                unam_field: Some(unam_field),
            });
        }
        return chunks;
    }

    let mut chunks = Vec::new();
    let mut chunk_start = None;

    for index in pkcu_pos + 1..data_end_pos {
        let entry = &record.fields[index];
        if chunk_start.is_none() {
            if entry.sig.as_str() != "ANAM" {
                continue;
            }
            chunk_start = Some(index);
        }

        if entry.sig.as_str() != "UNAM" {
            continue;
        }

        let Some(start) = chunk_start.take() else {
            continue;
        };
        let fields = record.fields[start..=index].to_vec();
        chunks.push(PackInputChunk {
            unam: pack_unam_value(entry),
            data_fields: fields.clone(),
            unam_field: None,
        });
    }

    chunks
}

struct PackTrailingUnamLayout {
    anam_indices: Vec<usize>,
    unam_indices: Vec<usize>,
    first_unam_pos: usize,
}

fn pack_trailing_unam_layout(
    record: &Record,
    pkcu_pos: usize,
    data_end_pos: usize,
) -> Option<PackTrailingUnamLayout> {
    let mut anam_indices = Vec::new();
    let mut unam_indices = Vec::new();
    for index in pkcu_pos + 1..data_end_pos {
        match record.fields[index].sig.as_str() {
            "ANAM" => anam_indices.push(index),
            "UNAM" => unam_indices.push(index),
            _ => {}
        }
    }
    if anam_indices.is_empty() || anam_indices.len() != unam_indices.len() {
        return None;
    }
    let first_unam_pos = *unam_indices.first()?;
    if first_unam_pos <= *anam_indices.last()? {
        return None;
    }
    if !record.fields[first_unam_pos..data_end_pos]
        .iter()
        .all(|entry| entry.sig.as_str() == "UNAM")
    {
        return None;
    }
    Some(PackTrailingUnamLayout {
        anam_indices,
        unam_indices,
        first_unam_pos,
    })
}

fn insert_pack_input_chunks(
    record: &mut Record,
    pkcu_pos: usize,
    fields_to_insert: Vec<PackInputChunk>,
) {
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    let trailing_layout = if fields_to_insert
        .iter()
        .all(|chunk| chunk.unam_field.is_some())
    {
        pack_trailing_unam_layout(record, pkcu_pos, data_end_pos)
    } else {
        None
    };
    if let Some(layout) = trailing_layout {
        let data_insert_pos = layout.first_unam_pos;
        let mut offset = 0;
        for chunk in &fields_to_insert {
            for field in &chunk.data_fields {
                record
                    .fields
                    .insert(data_insert_pos + offset, field.clone());
                offset += 1;
            }
        }

        let unam_insert_pos = pack_data_end_pos(record, pkcu_pos);
        let mut offset = 0;
        for chunk in fields_to_insert {
            if let Some(field) = chunk.unam_field {
                record.fields.insert(unam_insert_pos + offset, field);
                offset += 1;
            }
        }
        return;
    }

    let insert_pos = pack_data_end_pos(record, pkcu_pos);
    let mut offset = 0;
    for chunk in fields_to_insert {
        for field in chunk.data_fields {
            record.fields.insert(insert_pos + offset, field);
            offset += 1;
        }
        if let Some(field) = chunk.unam_field {
            record.fields.insert(insert_pos + offset, field);
            offset += 1;
        }
    }
}

fn pack_unam_values(record: &Record, pkcu_pos: usize) -> Vec<u32> {
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    record.fields[pkcu_pos + 1..data_end_pos]
        .iter()
        .filter(|entry| entry.sig.as_str() == "UNAM")
        .filter_map(pack_unam_value)
        .collect()
}

fn pack_unam_value(entry: &FieldEntry) -> Option<u32> {
    if entry.sig.as_str() != "UNAM" {
        return None;
    }
    match &entry.value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[0..4].try_into().ok()?))
        }
        FieldValue::Bytes(bytes) => bytes.first().copied().map(u32::from),
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn pack_data_input_count(record: &Record, pkcu_pos: usize) -> u32 {
    let data_end_pos = pack_data_end_pos(record, pkcu_pos);
    record.fields[pkcu_pos + 1..data_end_pos]
        .iter()
        .filter(|entry| entry.sig.as_str() == "ANAM")
        .count() as u32
}

fn pack_data_end_pos(record: &Record, pkcu_pos: usize) -> usize {
    record
        .fields
        .iter()
        .enumerate()
        .skip(pkcu_pos + 1)
        .find_map(|(index, entry)| (entry.sig.as_str() == "XNAM").then_some(index))
        .unwrap_or(record.fields.len())
}

fn copy_pack_xnam_from_template(record: &mut Record, template: &Record) -> bool {
    let Some(template_xnam) = template
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "XNAM")
        .cloned()
    else {
        return false;
    };
    let Some(record_xnam) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "XNAM")
    else {
        return false;
    };
    if record_xnam.value == template_xnam.value {
        return false;
    }
    record_xnam.value = template_xnam.value;
    true
}

fn rewrite_raw_condition_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = false;
    let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    let mut dropping_condition_strings = false;
    for mut entry in record.fields.drain(..) {
        if dropping_condition_strings {
            if matches!(&entry.sig.0, b"CIS1" | b"CIS2") {
                changed = true;
                continue;
            }
            dropping_condition_strings = false;
        }
        if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
            retained.push(entry);
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            retained.push(entry);
            continue;
        };
        match rewrite_raw_condition_bytes(
            bytes.as_mut_slice(),
            encoded_targets,
            is_valid_target_master_formid,
        ) {
            RawConditionRewrite::Unchanged => {}
            RawConditionRewrite::Changed => changed = true,
            RawConditionRewrite::Drop => {
                changed = true;
                dropping_condition_strings = true;
                continue;
            }
        }
        retained.push(entry);
    }
    record.fields = retained;
    changed
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RawConditionRewrite {
    Unchanged,
    Changed,
    Drop,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum RawFormIdRewrite {
    Unchanged,
    Changed,
    Invalid,
}

fn rewrite_raw_condition_bytes(
    bytes: &mut [u8],
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> RawConditionRewrite {
    if bytes.len() < 10 {
        return RawConditionRewrite::Unchanged;
    }

    let function_id = u16::from_le_bytes([bytes[8], bytes[9]]);
    let mut changed = false;

    if bytes[0] & 0x04 != 0 {
        match rewrite_or_validate_formid_at(
            bytes,
            4,
            encoded_targets,
            is_valid_target_master_formid,
        ) {
            RawFormIdRewrite::Unchanged => {}
            RawFormIdRewrite::Changed => changed = true,
            RawFormIdRewrite::Invalid => return RawConditionRewrite::Drop,
        }
    }
    if FO76_CTDA_PARAM1_FORMID_FUNCTIONS
        .binary_search(&function_id)
        .is_ok()
    {
        match rewrite_or_validate_formid_at(
            bytes,
            12,
            encoded_targets,
            is_valid_target_master_formid,
        ) {
            RawFormIdRewrite::Unchanged => {}
            RawFormIdRewrite::Changed => changed = true,
            RawFormIdRewrite::Invalid => return RawConditionRewrite::Drop,
        }
    }
    if FO76_CTDA_PARAM2_FORMID_FUNCTIONS
        .binary_search(&function_id)
        .is_ok()
    {
        match rewrite_or_validate_formid_at(
            bytes,
            16,
            encoded_targets,
            is_valid_target_master_formid,
        ) {
            RawFormIdRewrite::Unchanged => {}
            RawFormIdRewrite::Changed => changed = true,
            RawFormIdRewrite::Invalid => return RawConditionRewrite::Drop,
        }
    }
    if bytes.len() >= 28 {
        let run_on = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        if run_on == 2 {
            match rewrite_or_validate_formid_at(
                bytes,
                24,
                encoded_targets,
                is_valid_target_master_formid,
            ) {
                RawFormIdRewrite::Unchanged => {}
                RawFormIdRewrite::Changed => changed = true,
                RawFormIdRewrite::Invalid => return RawConditionRewrite::Drop,
            }
        }
    }

    if changed {
        RawConditionRewrite::Changed
    } else {
        RawConditionRewrite::Unchanged
    }
}

fn rewrite_ipds_pnam_record(record: &mut Record, encoded_targets: &FxHashMap<u32, u32>) -> bool {
    if record.sig.0 != *b"IPDS" {
        return false;
    }

    let mut changed = false;
    let pnam_sig = SubrecordSig(*b"PNAM");
    for entry in &mut record.fields {
        if entry.sig != pnam_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.len() != 8 {
            continue;
        }
        changed |= rewrite_formid_at(bytes.as_mut_slice(), 0, encoded_targets);
        changed |= rewrite_formid_at(bytes.as_mut_slice(), 4, encoded_targets);
    }
    changed
}

fn rewrite_expl_data_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"EXPL" {
        return false;
    }

    let mut changed = false;
    let data_sig = SubrecordSig(*b"DATA");
    for entry in &mut record.fields {
        if entry.sig != data_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_or_null_formids_at_offsets(
            bytes.as_mut_slice(),
            &[0, 4, 8, 12, 16, 20, 24],
            encoded_targets,
            is_valid_target_master_formid,
        );
    }
    changed
}

fn rewrite_ammo_data_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"AMMO" {
        return false;
    }

    let mut changed = false;
    let output_plugin = record.form_key.plugin;
    let dnam_sig = SubrecordSig(*b"DNAM");
    for entry in &mut record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                changed |= rewrite_or_null_formid_at(
                    bytes.as_mut_slice(),
                    0,
                    encoded_targets,
                    is_valid_target_master_formid,
                );
            }
            value => {
                changed |= rewrite_ammo_dnam_value(
                    value,
                    output_plugin,
                    target_masters,
                    interner,
                    encoded_targets,
                    is_valid_target_master_formid,
                );
            }
        }
    }
    changed
}

fn rewrite_ammo_dnam_value(
    value: &mut FieldValue,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            if rewrite_decoded_mapped_formkey(
                fk,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
            ) {
                return true;
            }
            let Some(raw) = encode_target_form_id(*fk, interner, target_masters) else {
                return false;
            };
            if raw == 0 || raw >> 24 != 0 || is_valid_target_master_formid(raw) {
                return false;
            }
            fk.local = 0;
            true
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_ammo_dnam_value(
                item,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                is_valid_target_master_formid,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            rewrite_ammo_dnam_value(
                item,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                is_valid_target_master_formid,
            ) | changed
        }),
        _ => false,
    }
}

fn rewrite_hazd_data_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"HAZD" {
        return false;
    }

    let mut changed = false;
    let dnam_sig = SubrecordSig(*b"DNAM");
    for entry in &mut record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_or_null_formids_at_offsets(
            bytes.as_mut_slice(),
            &[24, 28, 32, 36],
            encoded_targets,
            is_valid_target_master_formid,
        );
    }
    changed
}

fn rewrite_proj_data_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"PROJ" {
        return false;
    }

    let mut changed = false;
    let dnam_sig = SubrecordSig(*b"DNAM");
    for entry in &mut record.fields {
        if entry.sig != dnam_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_or_null_formids_at_offsets(
            bytes.as_mut_slice(),
            &[16, 20, 32, 36, 52, 56, 60, 80, 84, 89],
            encoded_targets,
            is_valid_target_master_formid,
        );
    }
    changed
}

fn rewrite_fsts_footstep_set_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
) -> bool {
    if record.sig.0 != *b"FSTS" {
        return false;
    }

    let mut changed = false;
    let output_plugin = record.form_key.plugin;
    let data_sig = SubrecordSig(*b"DATA");
    for entry in &mut record.fields {
        if entry.sig != data_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                changed |= rewrite_fsts_footsteps_bytes(bytes.as_mut_slice(), encoded_targets);
            }
            value => {
                changed |= rewrite_regn_rdwt_value(
                    value,
                    output_plugin,
                    target_masters,
                    interner,
                    encoded_targets,
                );
            }
        }
    }
    changed
}

fn rewrite_fsts_footsteps_bytes(bytes: &mut [u8], encoded_targets: &FxHashMap<u32, u32>) -> bool {
    if bytes.len() % 4 != 0 {
        return false;
    }

    let mut changed = false;
    for offset in (0..bytes.len()).step_by(4) {
        changed |= rewrite_formid_at(bytes, offset, encoded_targets);
    }
    changed
}

fn rewrite_stag_sound_record(record: &mut Record, encoded_targets: &FxHashMap<u32, u32>) -> bool {
    if record.sig.0 != *b"STAG" {
        return false;
    }

    let mut changed = false;
    let tnam_sig = SubrecordSig(*b"TNAM");
    for entry in &mut record.fields {
        if entry.sig != tnam_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_formid_at(bytes.as_mut_slice(), 0, encoded_targets);
    }
    changed
}

pub(crate) fn rewrite_placed_ref_location_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if !matches!(record.sig.as_str(), "ACHR" | "REFR" | "PGRE") {
        return false;
    }

    let mut changed = false;
    let mut retained = smallvec::SmallVec::with_capacity(record.fields.len());
    for mut entry in record.fields.drain(..) {
        let remove = match &mut entry.value {
            FieldValue::Bytes(bytes) if matches!(&entry.sig.0, b"XLCN" | b"XEZN" | b"XCZC") => {
                let rewrite = rewrite_or_validate_formid_at(
                    bytes.as_mut_slice(),
                    0,
                    encoded_targets,
                    is_valid_target_master_formid,
                );
                match rewrite {
                    RawFormIdRewrite::Unchanged => !placed_ref_location_signature_matches(
                        entry.sig,
                        bytes.as_slice(),
                        target_record_sigs_by_encoded_form_id,
                    ),
                    RawFormIdRewrite::Changed => {
                        changed = true;
                        !placed_ref_location_signature_matches(
                            entry.sig,
                            bytes.as_slice(),
                            target_record_sigs_by_encoded_form_id,
                        )
                    }
                    RawFormIdRewrite::Invalid => {
                        changed = true;
                        true
                    }
                }
            }
            FieldValue::Bytes(bytes) if entry.sig.0 == *b"XLRT" => {
                let had_bytes = !bytes.is_empty();
                if rewrite_placed_ref_loc_ref_types(
                    bytes,
                    encoded_targets,
                    is_valid_target_master_formid,
                ) {
                    changed = true;
                }
                had_bytes && bytes.is_empty()
            }
            _ => false,
        };
        if remove {
            changed = true;
        } else {
            retained.push(entry);
        }
    }
    record.fields = retained;
    changed
}

fn placed_ref_location_signature_matches(
    subrecord_sig: SubrecordSig,
    bytes: &[u8],
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
) -> bool {
    let Some(chunk) = bytes.get(0..4) else {
        return true;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    // A null pointer is harmless (engine ignores it) and stripping vs keeping it
    // is not the leak this guards — keep it.
    if raw == 0 {
        return true;
    }
    match target_record_sigs_by_encoded_form_id.get(&raw) {
        Some(target_sig) => match &subrecord_sig.0 {
            b"XLCN" => target_sig.0 == *b"LCTN",
            b"XEZN" => target_sig.0 == *b"ECZN",
            _ => true,
        },
        // Unresolved/unindexed target (the index only covers the OUTPUT plugin's
        // own records, not masters). For XEZN this is still a positive defect:
        // a FO4 placed-ref XEZN whose target is anything other than an ECZN is
        // always invalid (xEdit "Found a LCTN, expected: ECZN"), and the dominant
        // FO76 case is a master-resident or converted LCTN that the output-only
        // index can't see. Treat unresolved XEZN as a non-match (strip). XLCN's
        // valid target IS a LCTN — a master-resident LCTN is legitimate there, so
        // keep XLCN conservative (unresolved ⇒ assume valid, don't strip).
        None => subrecord_sig.0 != *b"XEZN",
    }
}

fn rewrite_placed_ref_loc_ref_types(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if bytes.is_empty() || bytes.len() % 4 != 0 {
        return false;
    }

    let mut changed = false;
    let mut retained = Vec::with_capacity(bytes.len());
    for chunk in bytes.chunks(4) {
        let mut raw = [chunk[0], chunk[1], chunk[2], chunk[3]];
        match rewrite_or_validate_formid_at(
            raw.as_mut_slice(),
            0,
            encoded_targets,
            is_valid_target_master_formid,
        ) {
            RawFormIdRewrite::Unchanged => retained.extend_from_slice(&raw),
            RawFormIdRewrite::Changed => {
                changed = true;
                retained.extend_from_slice(&raw);
            }
            RawFormIdRewrite::Invalid => {
                changed = true;
            }
        }
    }

    if changed {
        bytes.clear();
        bytes.extend_from_slice(&retained);
    }
    changed
}

fn rewrite_destruction_stage_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    let mut changed = false;
    let dstd_sig = SubrecordSig(*b"DSTD");
    for entry in &mut record.fields {
        if entry.sig != dstd_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_formid_at(bytes.as_mut_slice(), 8, encoded_targets);
        changed |= rewrite_formid_at(bytes.as_mut_slice(), 12, encoded_targets);
    }
    changed
}

fn rewrite_prps_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = false;
    let prps_sig = SubrecordSig(*b"PRPS");
    for entry in &mut record.fields {
        if entry.sig != prps_sig {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        changed |= rewrite_prps_bytes(bytes, encoded_targets, is_valid_target_master_formid);
    }
    changed
}

fn rewrite_prps_bytes(
    bytes: &mut smallvec::SmallVec<[u8; 32]>,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    const PRPS_ROW_LEN: usize = 8;

    if bytes.len() % PRPS_ROW_LEN != 0 {
        return false;
    }

    let mut changed = false;
    let mut kept = Vec::with_capacity(bytes.len());
    for row in bytes.chunks_exact(PRPS_ROW_LEN) {
        let raw = u32::from_le_bytes(row[0..4].try_into().unwrap());
        if raw == 0 {
            changed = true;
            continue;
        }

        let mut rewritten_row = row.to_vec();
        if raw >> 24 == 0 {
            if let Some(encoded) = encoded_targets.get(&raw).copied() {
                if encoded != raw {
                    rewritten_row[0..4].copy_from_slice(&encoded.to_le_bytes());
                    changed = true;
                }
            } else if !is_valid_target_master_formid(raw) {
                changed = true;
                continue;
            }
        }
        kept.extend_from_slice(&rewritten_row);
    }

    if changed {
        *bytes = smallvec::SmallVec::from_vec(kept);
    }
    changed
}

fn rewrite_regn_weather_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
) -> bool {
    if record.sig.0 != *b"REGN" {
        return false;
    }

    let mut changed = false;
    let output_plugin = record.form_key.plugin;
    let rdwt_sig = SubrecordSig(*b"RDWT");
    for entry in &mut record.fields {
        if entry.sig != rdwt_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                changed |= rewrite_regn_rdwt_bytes(bytes.as_mut_slice(), encoded_targets);
            }
            value => {
                changed |= rewrite_regn_rdwt_value(
                    value,
                    output_plugin,
                    target_masters,
                    interner,
                    encoded_targets,
                );
            }
        }
    }
    changed
}

fn rewrite_clmt_weather_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
) -> bool {
    if record.sig.0 != *b"CLMT" {
        return false;
    }

    let mut changed = false;
    let output_plugin = record.form_key.plugin;
    let wlst_sig = SubrecordSig(*b"WLST");
    for entry in &mut record.fields {
        if entry.sig != wlst_sig {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) => {
                changed |= rewrite_regn_rdwt_bytes(bytes.as_mut_slice(), encoded_targets);
            }
            value => {
                changed |= rewrite_regn_rdwt_value(
                    value,
                    output_plugin,
                    target_masters,
                    interner,
                    encoded_targets,
                );
            }
        }
    }
    changed
}

fn rewrite_wthr_weather_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"WTHR" {
        return false;
    }

    let mut changed = false;
    let output_plugin = record.form_key.plugin;
    for entry in &mut record.fields {
        let sig = entry.sig.as_str();
        match &mut entry.value {
            FieldValue::Bytes(bytes) => match sig {
                "IMSP" | "WGDR" | "HNAM" => {
                    changed |= rewrite_wthr_formid_array_bytes(
                        bytes.as_mut_slice(),
                        encoded_targets,
                        is_valid_target_master_formid,
                    );
                }
                "SNAM" => {
                    changed |= rewrite_or_null_formid_at(
                        bytes.as_mut_slice(),
                        0,
                        encoded_targets,
                        is_valid_target_master_formid,
                    );
                }
                "UNAM" => {
                    changed |= rewrite_or_null_formid_at(
                        bytes.as_mut_slice(),
                        0,
                        encoded_targets,
                        is_valid_target_master_formid,
                    );
                    changed |= rewrite_or_null_formid_at(
                        bytes.as_mut_slice(),
                        8,
                        encoded_targets,
                        is_valid_target_master_formid,
                    );
                }
                _ => {}
            },
            value => {
                changed |= rewrite_regn_rdwt_value(
                    value,
                    output_plugin,
                    target_masters,
                    interner,
                    encoded_targets,
                );
            }
        }
    }
    changed
}

fn rewrite_wthr_formid_array_bytes(
    bytes: &mut [u8],
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if bytes.len() % 4 != 0 {
        return false;
    }

    let mut changed = false;
    for offset in (0..bytes.len()).step_by(4) {
        changed |= rewrite_or_null_formid_at(
            bytes,
            offset,
            encoded_targets,
            is_valid_target_master_formid,
        );
    }
    changed
}

fn rewrite_regn_sound_record(record: &mut Record, encoded_targets: &FxHashMap<u32, u32>) -> bool {
    if record.sig.0 != *b"REGN" {
        return false;
    }

    let mut changed = false;
    let rdsa_sig = SubrecordSig(*b"RDSA");
    for entry in &mut record.fields {
        if entry.sig != rdsa_sig {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            changed |= rewrite_regn_rdsa_bytes(bytes.as_mut_slice(), encoded_targets);
        }
    }
    changed
}

fn sanitize_regn_references(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
    target_masters: &[String],
    interner: &StringInterner,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if record.sig.0 != *b"REGN" {
        return false;
    }

    let mut changed = false;
    let output_plugin = record.form_key.plugin;
    let mut retained = smallvec::SmallVec::new();
    for mut entry in record.fields.drain(..) {
        let keep = match entry.sig.as_str() {
            "WNAM" => sanitize_required_regn_formkey_field(
                &mut entry.value,
                "WRLD",
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                is_valid_target_master_formid,
                &mut changed,
            ),
            "RDMO" => sanitize_optional_regn_formkey_field(
                &mut entry.value,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                is_valid_target_master_formid,
                &mut changed,
            ),
            "RDWT" => sanitize_regn_rdwt_rows(
                &mut entry.value,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                is_valid_target_master_formid,
                &mut changed,
            ),
            "RDOT" => sanitize_regn_rdot_rows(
                &mut entry.value,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
                target_record_sigs_by_encoded_form_id,
                is_valid_target_master_formid,
                &mut changed,
            ),
            _ => true,
        };
        if keep {
            retained.push(entry);
        } else {
            changed = true;
        }
    }
    record.fields = retained;
    changed |= prune_invalid_regn_layout(record, interner);
    changed
}

fn prune_invalid_regn_layout(record: &mut Record, interner: &StringInterner) -> bool {
    if record.sig.0 != *b"REGN" {
        return false;
    }

    let mut changed = false;
    if !record
        .fields
        .iter()
        .any(|entry| entry.sig.as_str() == "WNAM")
    {
        let mut retained = smallvec::SmallVec::new();
        let mut in_region_data = false;
        for entry in record.fields.drain(..) {
            let sig = entry.sig.as_str();
            if sig == "RDAT" {
                in_region_data = true;
                retained.push(entry);
                continue;
            }
            if !in_region_data && matches!(sig, "RPLI" | "RPLD" | "ANAM") {
                changed = true;
                continue;
            }
            retained.push(entry);
        }
        record.fields = retained;
    }

    changed | prune_orphan_regn_data_entries(record, interner)
}

fn prune_orphan_regn_data_entries(record: &mut Record, interner: &StringInterner) -> bool {
    let mut changed = false;
    let mut retained = smallvec::SmallVec::new();
    let mut index = 0;

    while index < record.fields.len() {
        let entry = record.fields[index].clone();
        if entry.sig.as_str() != "RDAT" {
            retained.push(entry);
            index += 1;
            continue;
        }

        let rdat_type = regn_rdat_type(&entry, interner);
        let mut data_fields = Vec::new();
        index += 1;
        while index < record.fields.len() {
            let next = record.fields[index].clone();
            if next.sig.as_str() == "RDAT" {
                break;
            }
            if !is_regn_data_payload_sig(next.sig.as_str()) {
                break;
            }
            data_fields.push(next);
            index += 1;
        }

        if rdat_type.is_some()
            && data_fields
                .iter()
                .any(|field| regn_payload_matches_rdat_type(rdat_type, field, interner))
        {
            retained.push(entry);
            retained.extend(data_fields);
        } else {
            changed = true;
        }
    }

    if changed {
        record.fields = retained;
    }
    changed
}

fn regn_rdat_type(entry: &FieldEntry, interner: &StringInterner) -> Option<u32> {
    match &entry.value {
        FieldValue::Bytes(bytes) => {
            let chunk = bytes.get(0..4)?;
            Some(u32::from_le_bytes(chunk.try_into().ok()?))
        }
        FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
            if interner.resolve(*name) != Some("type") {
                return None;
            }
            match value {
                FieldValue::Uint(value) => u32::try_from(*value).ok(),
                FieldValue::Int(value) if *value >= 0 => u32::try_from(*value).ok(),
                FieldValue::Bytes(bytes) => {
                    let chunk = bytes.get(0..4)?;
                    Some(u32::from_le_bytes(chunk.try_into().ok()?))
                }
                _ => None,
            }
        }),
        _ => None,
    }
}

fn is_regn_data_payload_sig(sig: &str) -> bool {
    matches!(
        sig,
        "ICON" | "RDMO" | "RDMP" | "RDOT" | "RDGS" | "RDWT" | "RLDM" | "ANAM" | "RDSA"
    )
}

fn regn_payload_matches_rdat_type(
    rdat_type: Option<u32>,
    entry: &FieldEntry,
    interner: &StringInterner,
) -> bool {
    if !field_value_is_non_empty(&entry.value, interner) {
        return false;
    }
    match rdat_type {
        Some(2) => entry.sig.as_str() == "RDOT",
        Some(3) => entry.sig.as_str() == "RDWT",
        Some(4) => matches!(entry.sig.as_str(), "ICON" | "RDMP"),
        Some(5) => entry.sig.as_str() == "RDOT",
        Some(6) => entry.sig.as_str() == "RDGS",
        Some(7) => matches!(entry.sig.as_str(), "RDMO" | "RDSA"),
        Some(_) | None => is_regn_data_payload_sig(entry.sig.as_str()),
    }
}

fn field_value_is_non_empty(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::None => false,
        FieldValue::Bytes(bytes) => !bytes.is_empty(),
        FieldValue::List(items) => !items.is_empty(),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| field_value_is_non_empty(value, interner)),
        FieldValue::FormKey(fk) => fk.local != 0,
        FieldValue::String(sym) => interner
            .resolve(*sym)
            .is_some_and(|value| !value.is_empty()),
        FieldValue::Bool(_) | FieldValue::Int(_) | FieldValue::Uint(_) | FieldValue::Float(_) => {
            true
        }
    }
}

fn sanitize_required_regn_formkey_field(
    value: &mut FieldValue,
    expected_sig: &str,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    changed: &mut bool,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => rewrite_and_check_regn_fk(
            fk,
            Some(expected_sig),
            output_plugin,
            target_masters,
            interner,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            is_valid_target_master_formid,
            changed,
        ),
        FieldValue::Bytes(bytes) => rewrite_and_check_regn_raw_formid(
            bytes.as_mut_slice(),
            0,
            Some(expected_sig),
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            is_valid_target_master_formid,
            changed,
        ),
        FieldValue::None => false,
        _ => true,
    }
}

fn sanitize_optional_regn_formkey_field(
    value: &mut FieldValue,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    changed: &mut bool,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => rewrite_and_check_regn_fk(
            fk,
            None,
            output_plugin,
            target_masters,
            interner,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            is_valid_target_master_formid,
            changed,
        ),
        FieldValue::Bytes(bytes) => rewrite_and_check_regn_raw_formid(
            bytes.as_mut_slice(),
            0,
            None,
            encoded_targets,
            target_record_sigs_by_encoded_form_id,
            is_valid_target_master_formid,
            changed,
        ),
        FieldValue::None => false,
        _ => true,
    }
}

fn sanitize_regn_rdwt_rows(
    value: &mut FieldValue,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    changed: &mut bool,
) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            const RDWT_ROW_LEN: usize = 12;
            if bytes.len() % RDWT_ROW_LEN != 0 {
                return true;
            }
            let mut kept = Vec::with_capacity(bytes.len());
            for row in bytes.chunks(RDWT_ROW_LEN) {
                let mut row = row.to_vec();
                let weather_valid = rewrite_and_check_regn_raw_formid(
                    row.as_mut_slice(),
                    0,
                    None,
                    encoded_targets,
                    target_record_sigs_by_encoded_form_id,
                    is_valid_target_master_formid,
                    changed,
                );
                if !weather_valid {
                    *changed = true;
                    continue;
                }
                if !rewrite_and_check_regn_raw_formid(
                    row.as_mut_slice(),
                    8,
                    None,
                    encoded_targets,
                    target_record_sigs_by_encoded_form_id,
                    is_valid_target_master_formid,
                    changed,
                ) {
                    row[8..12].copy_from_slice(&0_u32.to_le_bytes());
                    *changed = true;
                }
                kept.extend_from_slice(&row);
            }
            if kept.is_empty() {
                false
            } else {
                if kept.as_slice() != bytes.as_slice() {
                    *bytes = smallvec::SmallVec::from_vec(kept);
                    *changed = true;
                }
                true
            }
        }
        FieldValue::List(rows) => {
            let before = rows.len();
            rows.retain_mut(|row| {
                let FieldValue::Struct(fields) = row else {
                    return true;
                };
                let mut weather_ok = true;
                for (name, item) in fields {
                    let Some(field_name) = interner.resolve(*name) else {
                        continue;
                    };
                    if field_name == "WeatherTypesWeather" {
                        weather_ok = sanitize_optional_regn_formkey_field(
                            item,
                            output_plugin,
                            target_masters,
                            interner,
                            encoded_targets,
                            target_record_sigs_by_encoded_form_id,
                            is_valid_target_master_formid,
                            changed,
                        );
                    } else if field_name == "WeatherTypesGlobal"
                        && !sanitize_optional_regn_formkey_field(
                            item,
                            output_plugin,
                            target_masters,
                            interner,
                            encoded_targets,
                            target_record_sigs_by_encoded_form_id,
                            is_valid_target_master_formid,
                            changed,
                        )
                    {
                        *item = FieldValue::None;
                        *changed = true;
                    }
                }
                weather_ok
            });
            if rows.len() != before {
                *changed = true;
            }
            !rows.is_empty()
        }
        _ => true,
    }
}

fn sanitize_regn_rdot_rows(
    value: &mut FieldValue,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    changed: &mut bool,
) -> bool {
    let FieldValue::List(rows) = value else {
        return true;
    };
    let before = rows.len();
    rows.retain_mut(|row| {
        let FieldValue::Struct(fields) = row else {
            return true;
        };
        for (name, item) in fields {
            if interner.resolve(*name) == Some("object") {
                return sanitize_optional_regn_formkey_field(
                    item,
                    output_plugin,
                    target_masters,
                    interner,
                    encoded_targets,
                    target_record_sigs_by_encoded_form_id,
                    is_valid_target_master_formid,
                    changed,
                );
            }
        }
        false
    });
    if rows.len() != before {
        *changed = true;
    }
    !rows.is_empty()
}

fn rewrite_and_check_regn_fk(
    fk: &mut FormKey,
    expected_sig: Option<&str>,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    changed: &mut bool,
) -> bool {
    if fk.local == 0 {
        return false;
    }
    *changed |= rewrite_decoded_mapped_formkey(
        fk,
        output_plugin,
        target_masters,
        interner,
        encoded_targets,
    );
    let Some(encoded) = encode_target_form_id(*fk, interner, target_masters) else {
        return false;
    };
    encoded_regn_target_is_valid(
        encoded,
        expected_sig,
        target_record_sigs_by_encoded_form_id,
        is_valid_target_master_formid,
    )
}

fn rewrite_and_check_regn_raw_formid(
    bytes: &mut [u8],
    offset: usize,
    expected_sig: Option<&str>,
    encoded_targets: &FxHashMap<u32, u32>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
    changed: &mut bool,
) -> bool {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return false;
    };
    let mut raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw == 0 {
        return false;
    }
    if raw >> 24 == 0
        && let Some(encoded) = encoded_targets.get(&raw).copied()
    {
        if encoded != raw {
            raw = encoded;
            chunk.copy_from_slice(&encoded.to_le_bytes());
            *changed = true;
        }
    }
    encoded_regn_target_is_valid(
        raw,
        expected_sig,
        target_record_sigs_by_encoded_form_id,
        is_valid_target_master_formid,
    )
}

fn encoded_regn_target_is_valid(
    encoded: u32,
    expected_sig: Option<&str>,
    target_record_sigs_by_encoded_form_id: &FxHashMap<u32, crate::ids::SigCode>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    if encoded == 0 {
        return false;
    }
    if let Some(sig) = target_record_sigs_by_encoded_form_id.get(&encoded) {
        return expected_sig.is_none_or(|expected| sig.as_str() == expected);
    }
    encoded >> 24 == 0 && is_valid_target_master_formid(encoded)
}

fn rewrite_regn_rdwt_bytes(bytes: &mut [u8], encoded_targets: &FxHashMap<u32, u32>) -> bool {
    const RDWT_ROW_LEN: usize = 12;

    if bytes.len() % RDWT_ROW_LEN != 0 {
        return false;
    }

    let mut changed = false;
    for row_start in (0..bytes.len()).step_by(RDWT_ROW_LEN) {
        changed |= rewrite_formid_at(bytes, row_start, encoded_targets);
        changed |= rewrite_formid_at(bytes, row_start + 8, encoded_targets);
    }
    changed
}

fn rewrite_regn_rdsa_bytes(bytes: &mut [u8], encoded_targets: &FxHashMap<u32, u32>) -> bool {
    const RDSA_ROW_LEN: usize = 12;

    if bytes.len() % RDSA_ROW_LEN != 0 {
        return false;
    }

    let mut changed = false;
    for row_start in (0..bytes.len()).step_by(RDSA_ROW_LEN) {
        changed |= rewrite_formid_at(bytes, row_start, encoded_targets);
    }
    changed
}

fn rewrite_rfct_effect_art_record(
    record: &mut Record,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    if record.sig.0 != *b"RFCT" {
        return false;
    }

    let mut changed = false;
    let data_sig = SubrecordSig(*b"DATA");
    for entry in &mut record.fields {
        if entry.sig != data_sig {
            continue;
        }
        if let FieldValue::Bytes(bytes) = &mut entry.value {
            changed |= rewrite_formid_at(bytes.as_mut_slice(), 0, encoded_targets);
        }
    }
    changed
}

fn rewrite_regn_rdwt_value(
    value: &mut FieldValue,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    match value {
        FieldValue::FormKey(fk) => {
            let Some(encoded) = encoded_targets.get(&fk.local).copied() else {
                return false;
            };
            let Some(target_fk) =
                form_key_from_encoded_target(encoded, output_plugin, target_masters, interner)
            else {
                return false;
            };
            if *fk == target_fk {
                return false;
            }
            *fk = target_fk;
            true
        }
        FieldValue::List(items) => items.iter_mut().fold(false, |changed, item| {
            rewrite_regn_rdwt_value(
                item,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
            ) | changed
        }),
        FieldValue::Struct(fields) => fields.iter_mut().fold(false, |changed, (_, item)| {
            rewrite_regn_rdwt_value(
                item,
                output_plugin,
                target_masters,
                interner,
                encoded_targets,
            ) | changed
        }),
        _ => false,
    }
}

fn form_key_from_encoded_target(
    encoded: u32,
    output_plugin: crate::sym::Sym,
    target_masters: &[String],
    interner: &StringInterner,
) -> Option<FormKey> {
    if encoded == 0 {
        return Some(FormKey {
            local: 0,
            plugin: output_plugin,
        });
    }
    let load_index = ((encoded >> 24) & 0xFF) as usize;
    let local = encoded & 0x00FF_FFFF;
    let plugin = if load_index < target_masters.len() {
        interner.intern(&target_masters[load_index])
    } else if load_index == target_masters.len() {
        output_plugin
    } else {
        return None;
    };
    Some(FormKey { local, plugin })
}

pub(crate) fn rewrite_formid_at(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
) -> bool {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return false;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw == 0 || raw >> 24 != 0 {
        return false;
    }
    let Some(encoded) = encoded_targets.get(&raw) else {
        return false;
    };
    if *encoded == raw {
        return false;
    }
    chunk.copy_from_slice(&encoded.to_le_bytes());
    true
}

fn rewrite_or_validate_formid_at(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> RawFormIdRewrite {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return RawFormIdRewrite::Unchanged;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw == 0 || raw >> 24 != 0 {
        return RawFormIdRewrite::Unchanged;
    }
    if let Some(encoded) = encoded_targets.get(&raw) {
        if *encoded == raw {
            return RawFormIdRewrite::Unchanged;
        }
        chunk.copy_from_slice(&encoded.to_le_bytes());
        return RawFormIdRewrite::Changed;
    }
    if is_valid_target_master_formid(raw) {
        return RawFormIdRewrite::Unchanged;
    }
    RawFormIdRewrite::Invalid
}

fn rewrite_or_null_formid_at(
    bytes: &mut [u8],
    offset: usize,
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let Some(chunk) = bytes.get_mut(offset..offset + 4) else {
        return false;
    };
    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
    if raw == 0 || raw >> 24 != 0 {
        return false;
    }
    if let Some(encoded) = encoded_targets.get(&raw) {
        if *encoded == raw {
            return false;
        }
        chunk.copy_from_slice(&encoded.to_le_bytes());
        return true;
    }
    if is_valid_target_master_formid(raw) {
        return false;
    }
    chunk.copy_from_slice(&0_u32.to_le_bytes());
    true
}

fn rewrite_or_null_formids_at_offsets(
    bytes: &mut [u8],
    offsets: &[usize],
    encoded_targets: &FxHashMap<u32, u32>,
    is_valid_target_master_formid: &mut dyn FnMut(u32) -> bool,
) -> bool {
    let mut changed = false;
    for offset in offsets {
        changed |= rewrite_or_null_formid_at(
            bytes,
            *offset,
            encoded_targets,
            is_valid_target_master_formid,
        );
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode};
    use crate::record::FieldEntry;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    fn target_map() -> FxHashMap<u32, u32> {
        let mut map = FxHashMap::default();
        map.insert(0x00000024, 0x07000814);
        map.insert(0x00093BC8, 0x07093BC8);
        map.insert(0x0001DB4C, 0x0701DB4C);
        map.insert(0x0037D0C1, 0x0737D0C1);
        map.insert(0x005B2FFF, 0x075B2FFF);
        map.insert(0x005070, 0x07005070);
        map.insert(0x005071, 0x07005071);
        map.insert(0x00554A9E, 0x07554A9E);
        map.insert(0x005DC207, 0x075DC207);
        map.insert(0x005F6DC1, 0x075F6DC1);
        map.insert(0x00685532, 0x07685532);
        map.insert(0x00405ED4, 0x07405ED4);
        map.insert(0x00196CCB, 0x07196CCB);
        map.insert(0x0048CC2A, 0x0748CC2A);
        map.insert(0x004398AE, 0x074398AE);
        map.insert(0x007EBA4B, 0x077EBA4B);
        map.insert(0x007EE02B, 0x077EE02B);
        map.insert(0x0079C797, 0x0779C797);
        map.insert(0x0031F006, 0x0731F006);
        map.insert(0x003CFFF3, 0x073CFFF3);
        map.insert(0x0043F37E, 0x0743F37E);
        map.insert(0x004E6157, 0x074E6157);
        map.insert(0x0064ADB6, 0x0764ADB6);
        map.insert(0x0067AC0D, 0x0767AC0D);
        map.insert(0x007A85B0, 0x077A85B0);
        map.insert(0x002CD2E2, 0x072CD2E2);
        map.insert(0x00262208, 0x07262208);
        map.insert(0x00358545, 0x00003956);
        map
    }

    fn target_masters() -> Vec<String> {
        vec![
            "Fallout4.esm".to_string(),
            "DLCRobot.esm".to_string(),
            "DLCworkshop01.esm".to_string(),
            "DLCCoast.esm".to_string(),
            "DLCworkshop02.esm".to_string(),
            "DLCworkshop03.esm".to_string(),
            "DLCNukaWorld.esm".to_string(),
        ]
    }

    fn target_record_sigs() -> FxHashMap<u32, SigCode> {
        FxHashMap::default()
    }

    #[test]
    fn target_master_validity_cache_matches_legacy_policy() {
        let cache = TargetMasterValidityCache::from_object_ids_for_test([0x0037D0C1, 0x000002DC]);

        assert!(cache.is_valid_raw_form_id(0));
        assert!(cache.is_valid_raw_form_id(0x01000001));
        assert!(cache.is_valid_raw_form_id(0x0037D0C1));
        assert!(cache.is_valid_raw_form_id(0x000002DC));
        assert!(!cache.is_valid_raw_form_id(0x005B2FFF));
    }

    #[test]
    fn target_master_validity_cache_without_first_master_keeps_legacy_valid_behavior() {
        let cache = TargetMasterValidityCache::no_first_master_for_test();

        assert!(cache.is_valid_raw_form_id(0x0037D0C1));
        assert!(cache.is_valid_raw_form_id(0));
        assert!(cache.is_valid_raw_form_id(0x01000001));
    }

    fn schema_field(id: &str, kind: &str) -> esp_authoring_core::plugin_runtime::SchemaFieldJson {
        esp_authoring_core::plugin_runtime::SchemaFieldJson {
            id: id.to_string(),
            kind: kind.to_string(),
            ..Default::default()
        }
    }

    fn schema_subrecord(
        id: &str,
        codec: &str,
        fields: Vec<esp_authoring_core::plugin_runtime::SchemaFieldJson>,
    ) -> esp_authoring_core::plugin_runtime::SchemaSubrecordJson {
        esp_authoring_core::plugin_runtime::SchemaSubrecordJson {
            id: id.to_string(),
            kind: "parsed".to_string(),
            codec: Some(codec.to_string()),
            fields,
            ..Default::default()
        }
    }

    fn empty_compiled_schema() -> CompiledSchema {
        CompiledSchema {
            records: std::collections::HashMap::new(),
            enums: std::collections::HashMap::new(),
        }
    }

    fn rewrite_record_for_test(record: &mut Record, interner: &StringInterner) -> bool {
        let mut is_valid_target_master_formid = |_raw_form_id: u32| true;
        let mut is_valid_target_master_npc_template_formid = |_raw_form_id: u32| false;
        rewrite_record_raw_template_formids_with_master(
            record,
            &target_map(),
            &target_masters(),
            Some(interner.intern("Fallout4.esm")),
            interner,
            &target_record_sigs(),
            &mut is_valid_target_master_formid,
            &mut is_valid_target_master_npc_template_formid,
        )
    }

    fn make_record_with_obts(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey::parse("79AC42@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("OBTS").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_record_with_tpta(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse("5C3C50@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPTA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_record_with_tplt_and_tpta(
        interner: &StringInterner,
        form_key: &str,
        default_template: &str,
        raw: Vec<u8>,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse(form_key, interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPLT").unwrap(),
            value: FieldValue::FormKey(FormKey::parse(default_template, interner).unwrap()),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPTA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_record_with_tplt_and_struct_tpta(
        interner: &StringInterner,
        form_key: &str,
        default_template: &str,
        slots: Vec<FieldValue>,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse(form_key, interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPLT").unwrap(),
            value: FieldValue::FormKey(FormKey::parse(default_template, interner).unwrap()),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TPTA").unwrap(),
            value: FieldValue::Struct(
                slots
                    .into_iter()
                    .enumerate()
                    .map(|(index, value)| (interner.intern(&format!("slot_{index}")), value))
                    .collect(),
            ),
        });
        record
    }

    fn make_record_with_omod_data(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("OMOD").unwrap(),
            FormKey::parse("1CFE4B@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_record_with_omod_mods(raw: u32) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("OMOD").unwrap(),
            FormKey::parse("1CFE4B@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODS").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw.to_le_bytes().to_vec())),
        });
        record
    }

    fn make_mgef_record_with_data(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("MGEF").unwrap(),
            FormKey::parse("11082D@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_pack_record(sub_sig: &str, raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("PACK").unwrap(),
            FormKey::parse("4273B2@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sub_sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn raw_field(sig: &str, raw: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        }
    }

    fn make_npc_record_with_vmad(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse("72972E@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(raw_field("VMAD", raw));
        record
    }

    fn make_qust_record_with_vmad(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("QUST").unwrap(),
            FormKey::parse("405E15@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(raw_field("VMAD", raw));
        record
    }

    fn vmad_string(value: &str) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&(value.len() as u16).to_le_bytes());
        raw.extend_from_slice(value.as_bytes());
        raw
    }

    fn vmad_object(raw_form_id: u32) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u16.to_le_bytes());
        raw.extend_from_slice(&(-1_i16).to_le_bytes());
        raw.extend_from_slice(&raw_form_id.to_le_bytes());
        raw
    }

    fn vmad_property(name: &str, property_type: u8, value: Vec<u8>) -> Vec<u8> {
        let mut raw = vmad_string(name);
        raw.push(property_type);
        raw.push(0);
        raw.extend(value);
        raw
    }

    fn vmad_script(script_name: &str, properties: Vec<Vec<u8>>) -> Vec<u8> {
        let mut raw = vmad_string(script_name);
        raw.push(0);
        raw.extend_from_slice(&(properties.len() as u16).to_le_bytes());
        for property in properties {
            raw.extend(property);
        }
        raw
    }

    fn vmad_payload(scripts: Vec<Vec<u8>>) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&6_u16.to_le_bytes());
        raw.extend_from_slice(&2_u16.to_le_bytes());
        raw.extend_from_slice(&(scripts.len() as u16).to_le_bytes());
        for script in scripts {
            raw.extend(script);
        }
        raw
    }

    fn vmad_qust_payload(fragment_properties: Vec<Vec<u8>>) -> Vec<u8> {
        let mut raw = vmad_payload(Vec::new());
        raw.push(4);
        raw.extend_from_slice(&0_u16.to_le_bytes());
        raw.extend(vmad_script(
            "Fragments:Quests:QF_W05_MQ_001P_Wayward_Lacey_00405E15",
            fragment_properties,
        ));
        raw.extend_from_slice(&0_u16.to_le_bytes());
        raw
    }

    fn make_pack_record_with_fields(fields: Vec<FieldEntry>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("PACK").unwrap(),
            FormKey::parse("3F069B@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields = fields.into_iter().collect();
        record
    }

    fn pkcu(data_input_count: u32, package_template: u32, version: u32) -> FieldEntry {
        let mut raw = Vec::new();
        raw.extend_from_slice(&data_input_count.to_le_bytes());
        raw.extend_from_slice(&package_template.to_le_bytes());
        raw.extend_from_slice(&version.to_le_bytes());
        raw_field("PKCU", raw)
    }

    fn pack_bool_input(unam: u8) -> Vec<FieldEntry> {
        vec![
            raw_field("ANAM", b"Bool\0".to_vec()),
            raw_field("CNAM", vec![0]),
            raw_field("UNAM", vec![unam]),
        ]
    }

    fn pack_bool_data_input() -> Vec<FieldEntry> {
        vec![
            raw_field("ANAM", b"Bool\0".to_vec()),
            raw_field("CNAM", vec![0]),
        ]
    }

    fn pack_unam(unam: u8) -> FieldEntry {
        raw_field("UNAM", vec![unam])
    }

    fn make_ipds_record(pnam_rows: &[(u32, u32)]) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("IPDS").unwrap(),
            FormKey::parse("04BD9F@SeventySix.esm", &mut interner).unwrap(),
        );
        for (material, impact) in pnam_rows {
            let mut raw = Vec::with_capacity(8);
            raw.extend_from_slice(&material.to_le_bytes());
            raw.extend_from_slice(&impact.to_le_bytes());
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("PNAM").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(raw)),
            });
        }
        record
    }

    fn make_expl_record(data_formids: &[u32]) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("EXPL").unwrap(),
            FormKey::parse("5DB5CD@SeventySix.esm", &mut interner).unwrap(),
        );
        let mut raw = Vec::new();
        for formid in data_formids {
            raw.extend_from_slice(&formid.to_le_bytes());
        }
        raw.resize(84, 0);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_hazd_record(effect: u32, light: u32, impact: u32, sound: u32) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("HAZD").unwrap(),
            FormKey::parse("64ADB5@SeventySix.esm", &mut interner).unwrap(),
        );
        let mut raw = vec![0u8; 52];
        raw[24..28].copy_from_slice(&effect.to_le_bytes());
        raw[28..32].copy_from_slice(&light.to_le_bytes());
        raw[32..36].copy_from_slice(&impact.to_le_bytes());
        raw[36..40].copy_from_slice(&sound.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_ammo_record_with_dnam_raw(projectile: u32) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("AMMO").unwrap(),
            FormKey::parse("64A567@SeventySix.esm", &mut interner).unwrap(),
        );
        let mut raw = vec![0u8; 16];
        raw[0..4].copy_from_slice(&projectile.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_ammo_record_with_structured_dnam(
        interner: &StringInterner,
        projectile: FormKey,
    ) -> Record {
        let mut record = Record::new(
            SigCode::from_str("AMMO").unwrap(),
            FormKey::parse("64A567@SeventySix.esm", interner).unwrap(),
        );
        let projectile_sym = interner.intern("Projectile");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Struct(vec![(projectile_sym, FieldValue::FormKey(projectile))]),
        });
        record
    }

    fn make_proj_record(
        muzzle_flash_light: u32,
        explosion: u32,
        active_sound: u32,
        default_weapon: u32,
    ) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("PROJ").unwrap(),
            FormKey::parse("52EEFC@SeventySix.esm", &mut interner).unwrap(),
        );
        let mut raw = vec![0u8; 93];
        raw[20..24].copy_from_slice(&muzzle_flash_light.to_le_bytes());
        raw[32..36].copy_from_slice(&explosion.to_le_bytes());
        raw[36..40].copy_from_slice(&active_sound.to_le_bytes());
        raw[60..64].copy_from_slice(&default_weapon.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_fsts_record_with_data(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("FSTS").unwrap(),
            FormKey::parse("00506F@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_cont_record(sub_sig: &str, raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("CONT").unwrap(),
            FormKey::parse("88854E@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sub_sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_regn_record_with_rdwt(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REGN").unwrap(),
            FormKey::parse("842D5C@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_regn_record_with_rdsa(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REGN").unwrap(),
            FormKey::parse("0138C1@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDSA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_rfct_record_with_data(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("RFCT").unwrap(),
            FormKey::parse("4721DE@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_regn_record_with_structured_rdwt(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("REGN").unwrap(),
            FormKey::parse("842D5C@SeventySix.esm", interner).unwrap(),
        );
        let weather_key = interner.intern("WeatherTypesWeather");
        let chance_key = interner.intern("WeatherTypesChance");
        let global_key = interner.intern("WeatherTypesGlobal");
        let weather_fk = FormKey::parse("7EBA4B@Fallout4.esm", interner).unwrap();
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![
                (weather_key, FieldValue::FormKey(weather_fk)),
                (chance_key, FieldValue::Uint(95)),
                (global_key, FieldValue::Uint(0)),
            ])]),
        });
        record
    }

    fn make_regn_record_for_output(interner: &StringInterner) -> Record {
        Record::new(
            SigCode::from_str("REGN").unwrap(),
            FormKey::parse("842D5C@TestOut.esp", interner).unwrap(),
        )
    }

    fn regn_rdat(type_id: u32) -> FieldEntry {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&type_id.to_le_bytes());
        bytes.extend_from_slice(&[0, 50, 0, 0]);
        FieldEntry {
            sig: SubrecordSig::from_str("RDAT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn make_clmt_record_with_wlst(raw: Vec<u8>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("CLMT").unwrap(),
            FormKey::parse("000727@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WLST").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        record
    }

    fn make_clmt_record_with_structured_wlst(interner: &StringInterner) -> Record {
        let mut record = Record::new(
            SigCode::from_str("CLMT").unwrap(),
            FormKey::parse("000727@SeventySix.esm", interner).unwrap(),
        );
        let weather_key = interner.intern("WeatherTypesWeather");
        let chance_key = interner.intern("WeatherTypesChance");
        let global_key = interner.intern("WeatherTypesGlobal");
        let weather_fk = FormKey::parse("4398AE@Fallout4.esm", interner).unwrap();
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WLST").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![
                (weather_key, FieldValue::FormKey(weather_fk)),
                (chance_key, FieldValue::Uint(100)),
                (global_key, FieldValue::Uint(0)),
            ])]),
        });
        record
    }

    fn make_wthr_record(fields: Vec<(&str, Vec<u8>)>) -> Record {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("WTHR").unwrap(),
            FormKey::parse("2DB892@SeventySix.esm", &mut interner).unwrap(),
        );
        for (sig, raw) in fields {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(raw)),
            });
        }
        record
    }

    fn formid_words(words: &[u32]) -> Vec<u8> {
        let mut raw = Vec::new();
        for word in words {
            raw.extend_from_slice(&word.to_le_bytes());
        }
        raw
    }

    fn obts(include_mods: &[u32], keywords: &[u32], property_value: Option<u32>) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&(include_mods.len() as u32).to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&[0, 0, 0, 0]);
        raw.extend_from_slice(&(-1_i16).to_le_bytes());
        raw.push(1);
        raw.push(keywords.len() as u8);
        for keyword in keywords {
            raw.extend_from_slice(&keyword.to_le_bytes());
        }
        raw.push(0);
        raw.push(0);
        for form_id in include_mods {
            raw.extend_from_slice(&form_id.to_le_bytes());
            raw.extend_from_slice(&[0, 0, 1]);
        }
        if let Some(value) = property_value {
            raw.extend_from_slice(&[0; 11]);
            raw.extend_from_slice(&value.to_le_bytes());
            raw.extend_from_slice(&[0; 8]);
        }
        raw
    }

    fn tpta(slots: &[u32]) -> Vec<u8> {
        let mut raw = Vec::new();
        for slot in slots {
            raw.extend_from_slice(&slot.to_le_bytes());
        }
        raw
    }

    fn omod_data(
        attach_point: u32,
        attach_parent_slots: &[u32],
        include_mods: &[u32],
        property_value: Option<u32>,
    ) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&(include_mods.len() as u32).to_le_bytes());
        raw.extend_from_slice(&(property_value.is_some() as u32).to_le_bytes());
        raw.extend_from_slice(&[0, 0]);
        raw.extend_from_slice(&u32::from_le_bytes(*b"ARMO").to_le_bytes());
        raw.extend_from_slice(&[0, 0]);
        raw.extend_from_slice(&attach_point.to_le_bytes());
        raw.extend_from_slice(&(attach_parent_slots.len() as u32).to_le_bytes());
        for slot in attach_parent_slots {
            raw.extend_from_slice(&slot.to_le_bytes());
        }
        raw.extend_from_slice(&0_u32.to_le_bytes());
        for form_id in include_mods {
            raw.extend_from_slice(&form_id.to_le_bytes());
            raw.extend_from_slice(&[0, 0, 1]);
        }
        if let Some(value) = property_value {
            raw.extend_from_slice(&[0, 0, 0, 0]);
            raw.extend_from_slice(&[0, 0, 0, 0]);
            raw.extend_from_slice(&0_u16.to_le_bytes());
            raw.extend_from_slice(&[0, 0]);
            raw.extend_from_slice(&value.to_le_bytes());
            raw.extend_from_slice(&0_u32.to_le_bytes());
            raw.extend_from_slice(&0_f32.to_le_bytes());
        }
        raw
    }

    fn omod_data_with_property(property_id: u16, value_1: u32, value_2: u32) -> Vec<u8> {
        omod_data_with_typed_property(0, property_id, value_1, value_2)
    }

    fn omod_data_with_typed_property(
        value_type: u8,
        property_id: u16,
        value_1: u32,
        value_2: u32,
    ) -> Vec<u8> {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&1_u32.to_le_bytes());
        raw.extend_from_slice(&[0, 0]);
        raw.extend_from_slice(&u32::from_le_bytes(*b"ARMO").to_le_bytes());
        raw.extend_from_slice(&[0, 0]);
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        // Property row begins here (24 bytes): value_type @ +0.
        raw.push(value_type);
        raw.extend_from_slice(&[0, 0, 0]);
        raw.extend_from_slice(&[0, 0, 0, 0]);
        raw.extend_from_slice(&property_id.to_le_bytes());
        raw.extend_from_slice(&[0, 0]);
        raw.extend_from_slice(&value_1.to_le_bytes());
        raw.extend_from_slice(&value_2.to_le_bytes());
        raw.extend_from_slice(&0_f32.to_le_bytes());
        raw
    }

    fn ctda(function_id: u16, flags: u8, param1: u32, param2: u32) -> Vec<u8> {
        let mut raw = vec![0_u8; 32];
        raw[0] = flags;
        raw[4..8].copy_from_slice(&0x00685532_u32.to_le_bytes());
        raw[8..10].copy_from_slice(&function_id.to_le_bytes());
        raw[12..16].copy_from_slice(&param1.to_le_bytes());
        raw[16..20].copy_from_slice(&param2.to_le_bytes());
        raw[28..32].copy_from_slice(&(-1_i32).to_le_bytes());
        raw
    }

    #[test]
    fn rewrites_obts_included_mod_formids() {
        let mut record = make_record_with_obts(obts(&[0x0037D0C1, 0x0079C797], &[], None));
        let interner = StringInterner::new();

        assert!(rewrite_object_template_record(
            &mut record,
            &target_map(),
            &interner,
            &mut |_raw: u32| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
            0x0737D0C1
        );
        assert_eq!(
            u32::from_le_bytes(bytes[25..29].try_into().unwrap()),
            0x0779C797
        );
    }

    #[test]
    fn rewrites_obts_keyword_formids_and_moves_include_offset() {
        let mut record = make_record_with_obts(obts(&[0x0037D0C1], &[0x00685532], None));
        let interner = StringInterner::new();

        assert!(rewrite_object_template_record(
            &mut record,
            &target_map(),
            &interner,
            &mut |_raw: u32| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            0x07685532
        );
        assert_eq!(
            u32::from_le_bytes(bytes[22..26].try_into().unwrap()),
            0x0737D0C1
        );
    }

    #[test]
    fn leaves_unmapped_master_refs_and_property_values_unchanged() {
        let mut record = make_record_with_obts(obts(&[0x000A2D77], &[], Some(0x0037D0C1)));
        let interner = StringInterner::new();

        assert!(!rewrite_object_template_record(
            &mut record,
            &target_map(),
            &interner,
            &mut |_raw: u32| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[18..22].try_into().unwrap()),
            0x000A2D77
        );
        assert_eq!(
            u32::from_le_bytes(bytes[36..40].try_into().unwrap()),
            0x0037D0C1
        );
    }

    #[test]
    fn rewrites_decoded_obts_material_swap_property_formids() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey::parse("79AC42@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("OBTS").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("property_count"), FieldValue::Uint(1)),
                (
                    interner.intern("properties"),
                    FieldValue::List(vec![FieldValue::Struct(vec![
                        (interner.intern("value_type"), FieldValue::Uint(4)),
                        (interner.intern("function_type"), FieldValue::Uint(2)),
                        (interner.intern("property"), FieldValue::Uint(89)),
                        (interner.intern("value_1"), FieldValue::Uint(0x00685532)),
                        (interner.intern("value_2"), FieldValue::Uint(3)),
                        (interner.intern("step"), FieldValue::Float(0.0)),
                    ])]),
                ),
            ]),
        });

        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Struct(obts_fields) = &record.fields[0].value else {
            panic!("expected OBTS struct");
        };
        let properties_index =
            field_index_canonical(obts_fields, "properties", &interner).expect("properties");
        let FieldValue::List(properties) = &obts_fields[properties_index].1 else {
            panic!("expected properties list");
        };
        let FieldValue::Struct(property_fields) = &properties[0] else {
            panic!("expected property row struct");
        };
        let value_1_index =
            field_index_canonical(property_fields, "value_1", &interner).expect("value_1");
        let value_2_index =
            field_index_canonical(property_fields, "value_2", &interner).expect("value_2");
        assert_eq!(
            property_fields[value_1_index].1,
            FieldValue::Uint(0x07685532)
        );
        assert_eq!(property_fields[value_2_index].1, FieldValue::Uint(3));
    }

    #[test]
    fn leaves_decoded_obts_non_material_property_values_unchanged() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey::parse("79AC42@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("OBTS").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("properties"),
                FieldValue::List(vec![FieldValue::Struct(vec![
                    (interner.intern("property"), FieldValue::Uint(31)),
                    (interner.intern("value_1"), FieldValue::Uint(0x00685532)),
                ])]),
            )]),
        });

        assert!(!rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Struct(obts_fields) = &record.fields[0].value else {
            panic!("expected OBTS struct");
        };
        let properties_index =
            field_index_canonical(obts_fields, "properties", &interner).expect("properties");
        let FieldValue::List(properties) = &obts_fields[properties_index].1 else {
            panic!("expected properties list");
        };
        let FieldValue::Struct(property_fields) = &properties[0] else {
            panic!("expected property row struct");
        };
        let value_1_index =
            field_index_canonical(property_fields, "value_1", &interner).expect("value_1");
        assert_eq!(
            property_fields[value_1_index].1,
            FieldValue::Uint(0x00685532)
        );
    }

    #[test]
    fn rewrites_npc_template_actor_formids() {
        let mut record = make_record_with_tpta(tpta(&[
            0x001DFE13, 0x00000000, 0x005F6DC1, 0x005DC207, 0x005B2FFF,
        ]));
        let interner = StringInterner::new();

        assert!(rewrite_npc_template_actor_record(
            &mut record,
            &target_map(),
            &FxHashMap::default(),
            &target_masters(),
            &interner,
            &mut |_| false,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        let slots: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert_eq!(
            slots,
            vec![0x001DFE13, 0x00000000, 0x075F6DC1, 0x075DC207, 0x075B2FFF]
        );
    }

    #[test]
    fn npc_template_flags_include_every_fo4_tpta_slot() {
        let mut bytes = Vec::new();
        for slot in 1_u32..=13 {
            bytes.extend_from_slice(&slot.to_le_bytes());
        }

        assert_eq!(
            template_flags_for_tpta_value(&FieldValue::Bytes(SmallVec::from_vec(bytes))),
            0x1FFF
        );
    }

    #[test]
    fn redirects_npc_template_actor_self_slots_to_default_template() {
        let interner = StringInterner::new();
        let mut record = make_record_with_tplt_and_tpta(
            &interner,
            "03D628@SeventySix.esm",
            "03D628@Fallout4.esm",
            tpta(&[0x0703D628, 0x00000000, 0x07591317, 0x0703D628]),
        );

        assert!(rewrite_npc_template_actor_record(
            &mut record,
            &target_map(),
            &FxHashMap::default(),
            &target_masters(),
            &interner,
            &mut |_| false,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[1].value else {
            panic!("expected raw bytes");
        };
        let slots: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert_eq!(slots, vec![0x0003D628, 0x00000000, 0x07591317, 0x0003D628]);
    }

    #[test]
    fn redirects_struct_npc_template_actor_self_slots_to_default_template() {
        let interner = StringInterner::new();
        let mut record = make_record_with_tplt_and_struct_tpta(
            &interner,
            "03D628@SeventySix.esm",
            "03D628@Fallout4.esm",
            vec![
                FieldValue::FormKey(FormKey::parse("03D628@SeventySix.esm", &interner).unwrap()),
                FieldValue::Uint(0),
                FieldValue::Uint(0x005F6DC1),
                FieldValue::Uint(0x0703D628),
            ],
        );

        assert!(rewrite_npc_template_actor_record(
            &mut record,
            &target_map(),
            &FxHashMap::default(),
            &target_masters(),
            &interner,
            &mut |_| false,
        ));
        let FieldValue::Struct(fields) = &record.fields[1].value else {
            panic!("expected struct TPTA");
        };
        assert_eq!(fields[0].1, FieldValue::Uint(0x0003D628));
        assert_eq!(fields[1].1, FieldValue::Uint(0));
        assert_eq!(fields[2].1, FieldValue::Uint(0x075F6DC1));
        assert_eq!(fields[3].1, FieldValue::Uint(0x0003D628));
    }

    #[test]
    fn rewrites_struct_npc_template_actor_master_formkeys_to_emitted_source_records() {
        let interner = StringInterner::new();
        let mut record = make_record_with_tplt_and_struct_tpta(
            &interner,
            "1423A8@SeventySix.esm",
            "01DB4C@SeventySix.esm",
            vec![
                FieldValue::FormKey(FormKey::parse("01DB4C@Fallout4.esm", &interner).unwrap()),
                FieldValue::FormKey(FormKey::parse("01DB4C@Fallout4.esm", &interner).unwrap()),
            ],
        );

        let mut sigs = FxHashMap::default();
        sigs.insert(0x0701DB4C, SigCode::from_str("NPC_").unwrap());

        assert!(rewrite_npc_template_actor_record(
            &mut record,
            &target_map(),
            &sigs,
            &target_masters(),
            &interner,
            &mut |_| false,
        ));
        let FieldValue::Struct(fields) = &record.fields[1].value else {
            panic!("expected struct TPTA");
        };
        assert_eq!(fields[0].1, FieldValue::Uint(0x0701DB4C));
        assert_eq!(fields[1].1, FieldValue::Uint(0x0701DB4C));
    }

    #[test]
    fn npc_templates_prefer_valid_first_master_templates_over_output_templates() {
        let interner = StringInterner::new();
        let mut record = make_record_with_tplt_and_struct_tpta(
            &interner,
            "1423A8@B21_Test.esp",
            "01DB4C@B21_Test.esp",
            vec![
                FieldValue::FormKey(FormKey::parse("01DB4C@B21_Test.esp", &interner).unwrap()),
                FieldValue::Uint(0x0701DB4C),
            ],
        );

        let mut sigs = FxHashMap::default();
        sigs.insert(0x0701DB4C, SigCode::from_str("NPC_").unwrap());

        assert!(rewrite_npc_template_actor_record(
            &mut record,
            &target_map(),
            &sigs,
            &target_masters(),
            &interner,
            &mut |raw| raw == 0x0001DB4C,
        ));

        assert_eq!(record.fields[0].value, FieldValue::Uint(0x0001DB4C));
        let FieldValue::Struct(fields) = &record.fields[1].value else {
            panic!("expected struct TPTA");
        };
        assert_eq!(fields[0].1, FieldValue::Uint(0x0001DB4C));
        assert_eq!(fields[1].1, FieldValue::Uint(0x0001DB4C));
    }

    #[test]
    fn sanitizes_npc_template_refs_that_do_not_resolve_to_actor_templates() {
        let interner = StringInterner::new();
        let plugin = "B21_Test.esp";
        let output_npc = FormKey::parse(&format!("5858E8@{plugin}"), &interner).unwrap();
        let output_lvln = FormKey::parse(&format!("59763E@{plugin}"), &interner).unwrap();
        let bad_master = FormKey::parse("59763E@Fallout4.esm", &interner).unwrap();

        let mut record = make_record_with_tplt_and_struct_tpta(
            &interner,
            &format!("5858E7@{plugin}"),
            &format!("59763E@{plugin}"),
            vec![
                FieldValue::FormKey(bad_master),
                FieldValue::FormKey(output_npc),
                FieldValue::FormKey(output_lvln),
            ],
        );
        let mut acbs = vec![0u8; 20];
        acbs[NPC_ACBS_TEMPLATE_FLAGS_OFFSET..NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&0x01FF_u16.to_le_bytes());
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig::from_str("ACBS").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(acbs)),
            },
        );

        let masters = target_masters();
        let mut sigs = FxHashMap::default();
        sigs.insert(
            encode_target_form_id(output_npc, &interner, &masters).unwrap(),
            SigCode::from_str("NPC_").unwrap(),
        );
        sigs.insert(
            encode_target_form_id(output_lvln, &interner, &masters).unwrap(),
            SigCode::from_str("LVLN").unwrap(),
        );

        assert!(sanitize_npc_actor_runtime_refs(
            &mut record,
            &sigs,
            &masters,
            Some(interner.intern("Fallout4.esm")),
            &interner,
            &mut |_| false,
        ));

        assert!(
            record
                .fields
                .iter()
                .any(|entry| entry.sig.as_str() == "TPLT"),
            "LVLN default template should be preserved when the LVLN exists"
        );
        let tpta = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "TPTA")
            .expect("TPTA remains");
        let FieldValue::Struct(fields) = &tpta.value else {
            panic!("expected struct TPTA");
        };
        assert_eq!(fields[0].1, FieldValue::Uint(0));
        assert_eq!(fields[1].1, FieldValue::FormKey(output_npc));
        assert_eq!(fields[2].1, FieldValue::FormKey(output_lvln));

        let acbs = record
            .fields
            .iter()
            .find(|entry| entry.sig.as_str() == "ACBS")
            .expect("ACBS remains");
        let FieldValue::Bytes(bytes) = &acbs.value else {
            panic!("expected ACBS bytes");
        };
        assert_eq!(
            u16::from_le_bytes(
                bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET..NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                    .try_into()
                    .unwrap()
            ),
            NPC_TEMPLATE_STATS | NPC_TEMPLATE_FACTIONS
        );
    }

    #[test]
    fn repairs_dangling_output_human_race_refs_to_fo4_master() {
        let interner = StringInterner::new();
        let plugin = "B21_Test.esp";
        let mut record = Record::new(
            SigCode::from_str("NPC_").unwrap(),
            FormKey::parse(&format!("5858E7@{plugin}"), &interner).unwrap(),
        );
        for sig in ["RNAM", "ATKR"] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str(sig).unwrap(),
                value: FieldValue::FormKey(
                    FormKey::parse(&format!("013746@{plugin}"), &interner).unwrap(),
                ),
            });
        }

        assert!(sanitize_npc_actor_runtime_refs(
            &mut record,
            &FxHashMap::default(),
            &target_masters(),
            Some(interner.intern("Fallout4.esm")),
            &interner,
            &mut |_| true,
        ));

        for entry in &record.fields {
            let FieldValue::FormKey(fk) = &entry.value else {
                panic!("expected race FormKey");
            };
            assert_eq!(fk.local, FO4_HUMAN_RACE_LOCAL);
            assert_eq!(interner.resolve(fk.plugin), Some("Fallout4.esm"));
        }
    }

    #[test]
    fn rewrites_pack_pkcu_package_template_formid() {
        let mut pkcu = Vec::new();
        pkcu.extend_from_slice(&7_u32.to_le_bytes());
        pkcu.extend_from_slice(&0x005F6DC1_u32.to_le_bytes());
        pkcu.extend_from_slice(&2_u32.to_le_bytes());
        let mut record = make_pack_record("PKCU", pkcu);

        assert!(rewrite_pack_package_template_record(
            &mut record,
            &target_map()
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x075F6DC1
        );
    }

    #[test]
    fn leaves_vanilla_pack_pkcu_package_template_unchanged() {
        let mut pkcu = Vec::new();
        pkcu.extend_from_slice(&7_u32.to_le_bytes());
        pkcu.extend_from_slice(&0x00002CE0_u32.to_le_bytes());
        pkcu.extend_from_slice(&2_u32.to_le_bytes());
        let mut record = make_pack_record("PKCU", pkcu);

        assert!(!rewrite_pack_package_template_record(
            &mut record,
            &target_map()
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x00002CE0
        );
    }

    #[test]
    fn resolves_stale_pack_template_inputs_from_newer_template() {
        let mut instance_fields = vec![pkcu(4, 0x07002CB0, 1)];
        for unam in [1, 3, 5, 7] {
            instance_fields.extend(pack_bool_input(unam));
        }
        instance_fields.push(raw_field("XNAM", Vec::new()));
        let mut record = make_pack_record_with_fields(instance_fields);

        let mut template_fields = vec![pkcu(5, 0, 2)];
        for unam in [1, 3, 5, 7, 9] {
            template_fields.extend(pack_bool_input(unam));
        }
        template_fields.push(raw_field("XNAM", Vec::new()));
        let template = make_pack_record_with_fields(template_fields);

        assert!(resolve_pack_template_inputs_from_template(
            &mut record,
            &template
        ));
        let (pkcu_pos, resolved_pkcu) = read_pack_pkcu(&record).expect("PKCU");
        assert_eq!(resolved_pkcu.data_input_count, 5);
        assert_eq!(resolved_pkcu.package_template, 0x07002CB0);
        assert_eq!(resolved_pkcu.version, 2);
        assert_eq!(pack_unam_values(&record, pkcu_pos), vec![1, 3, 5, 7, 9]);

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        let xnam_pos = sigs.iter().position(|sig| *sig == "XNAM").expect("XNAM");
        assert_eq!(&sigs[xnam_pos - 3..xnam_pos], ["ANAM", "CNAM", "UNAM"]);
    }

    #[test]
    fn resolves_stale_pack_template_inputs_with_trailing_unam_indices() {
        let mut instance_fields = vec![pkcu(4, 0x07002CB0, 1)];
        instance_fields.push(raw_field("ANAM", b"Location\0".to_vec()));
        instance_fields.push(raw_field("PLDT", vec![0; 16]));
        for _ in 0..3 {
            instance_fields.extend(pack_bool_data_input());
        }
        for unam in [1, 3, 5, 7] {
            instance_fields.push(pack_unam(unam));
        }
        instance_fields.push(raw_field("XNAM", vec![0x08]));
        let mut record = make_pack_record_with_fields(instance_fields);

        let mut template_fields = vec![pkcu(5, 0, 2)];
        template_fields.push(raw_field("ANAM", b"Location\0".to_vec()));
        template_fields.push(raw_field("PLDT", vec![0; 16]));
        for _ in 0..4 {
            template_fields.extend(pack_bool_data_input());
        }
        for unam in [1, 3, 5, 7, 9] {
            template_fields.push(pack_unam(unam));
        }
        template_fields.push(raw_field("XNAM", vec![0x0A]));
        let template = make_pack_record_with_fields(template_fields);

        assert!(resolve_pack_template_inputs_from_template(
            &mut record,
            &template
        ));
        let (pkcu_pos, resolved_pkcu) = read_pack_pkcu(&record).expect("PKCU");
        assert_eq!(resolved_pkcu.data_input_count, 5);
        assert_eq!(resolved_pkcu.package_template, 0x07002CB0);
        assert_eq!(resolved_pkcu.version, 2);
        assert_eq!(pack_unam_values(&record, pkcu_pos), vec![1, 3, 5, 7, 9]);

        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        let first_unam_pos = sigs.iter().position(|sig| *sig == "UNAM").expect("UNAM");
        let xnam_pos = sigs.iter().position(|sig| *sig == "XNAM").expect("XNAM");
        assert_eq!(
            sigs[pkcu_pos + 1..first_unam_pos]
                .iter()
                .filter(|sig| **sig == "ANAM")
                .count(),
            5
        );
        assert!(
            sigs[first_unam_pos..xnam_pos]
                .iter()
                .all(|sig| *sig == "UNAM")
        );
        let FieldValue::Bytes(bytes) = &record.fields[xnam_pos].value else {
            panic!("expected XNAM bytes");
        };
        assert_eq!(bytes.as_slice(), &[0x0A]);
    }

    #[test]
    fn rewrites_vmad_object_property_formids() {
        let raw_source = 0x00685532_u32;
        let raw_target = 0x07685532_u32;
        let vmad = vmad_payload(vec![vmad_script(
            "W05_ActorNukeReactionScript",
            vec![
                vmad_property("W05_NPCNukeEquipDelayMax", 1, vmad_object(raw_source)),
                vmad_property("SetOpenAnim", 2, vmad_string("JumpOn01")),
            ],
        )]);
        let mut record = make_npc_record_with_vmad(vmad);

        assert!(rewrite_record_for_test(&mut record, &StringInterner::new()));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected VMAD bytes");
        };
        assert!(
            bytes
                .windows(4)
                .any(|window| window == raw_target.to_le_bytes().as_slice())
        );
        assert!(
            !bytes
                .windows(4)
                .any(|window| window == raw_source.to_le_bytes().as_slice())
        );
    }

    #[test]
    fn rewrites_nested_vmad_object_property_formids() {
        let raw_source = 0x0048CC2A_u32;
        let raw_target = 0x0748CC2A_u32;
        let mut struct_payload = Vec::new();
        struct_payload.extend_from_slice(&1_i32.to_le_bytes());
        struct_payload.extend(vmad_property("NestedObject", 1, vmad_object(raw_source)));
        let mut array_payload = Vec::new();
        array_payload.extend_from_slice(&1_i32.to_le_bytes());
        array_payload.extend(vmad_object(raw_source));
        let vmad = vmad_payload(vec![vmad_script(
            "NestedScript",
            vec![
                vmad_property("StructProperty", 7, struct_payload),
                vmad_property("ArrayProperty", 11, array_payload),
            ],
        )]);
        let mut record = make_npc_record_with_vmad(vmad);

        assert!(rewrite_record_for_test(&mut record, &StringInterner::new()));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected VMAD bytes");
        };
        let rewritten_count = bytes
            .windows(4)
            .filter(|window| *window == raw_target.to_le_bytes().as_slice())
            .count();
        assert_eq!(rewritten_count, 2);
    }

    #[test]
    fn rewrites_qust_vmad_fragment_script_property_formids() {
        let raw_source = 0x00405ED4_u32;
        let raw_target = 0x07405ED4_u32;
        let vmad = vmad_qust_payload(vec![vmad_property(
            "W05_MQ_001P_Wayward_LaceyIsela_Checkpoint",
            1,
            vmad_object(raw_source),
        )]);
        let mut record = make_qust_record_with_vmad(vmad);

        assert!(rewrite_record_for_test(&mut record, &StringInterner::new()));

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected VMAD bytes");
        };
        assert!(
            bytes
                .windows(4)
                .any(|window| window == raw_target.to_le_bytes().as_slice())
        );
        assert!(
            !bytes
                .windows(4)
                .any(|window| window == raw_source.to_le_bytes().as_slice())
        );
    }

    #[test]
    fn resolves_pack_template_inputs_from_prior_rewritten_template() {
        let interner = StringInterner::new();
        let template_fk = FormKey::parse("5F6DC1@SeventySix.esm", &interner).unwrap();

        let mut template_fields = vec![pkcu(2, 0, 2)];
        template_fields.extend(pack_bool_input(1));
        template_fields.push(raw_field("ANAM", b"Condition\0".to_vec()));
        template_fields.push(raw_field("CTDA", ctda(14, 0, 0x00685532, 0)));
        template_fields.push(raw_field("UNAM", vec![9]));
        template_fields.push(raw_field("XNAM", Vec::new()));
        let mut rewritten_template = make_pack_record_with_fields(template_fields);
        rewritten_template.form_key = template_fk;
        assert!(rewrite_raw_condition_record(
            &mut rewritten_template,
            &target_map(),
            &mut |_| true,
        ));

        let mut instance_fields = vec![pkcu(1, 0x075F6DC1, 1)];
        instance_fields.extend(pack_bool_input(1));
        instance_fields.push(raw_field("XNAM", Vec::new()));
        let mut instance = make_pack_record_with_fields(instance_fields);

        let mut rewritten_records = FxHashMap::default();
        rewritten_records.insert(template_fk, rewritten_template);
        assert_eq!(
            resolve_pack_template_inputs_from_rewritten_record(
                &rewritten_records,
                &template_fk,
                &mut instance,
            ),
            Some(true)
        );

        let (pkcu_pos, resolved_pkcu) = read_pack_pkcu(&instance).expect("PKCU");
        assert_eq!(resolved_pkcu.data_input_count, 2);
        assert_eq!(resolved_pkcu.version, 2);
        assert_eq!(pack_unam_values(&instance, pkcu_pos), vec![1, 9]);
        let copied_ctda = instance
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "CTDA")
            .expect("copied CTDA");
        let FieldValue::Bytes(bytes) = &copied_ctda.value else {
            panic!("expected CTDA bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x07685532
        );
    }

    #[test]
    fn leaves_pack_template_inputs_when_instance_version_is_newer() {
        let mut instance_fields = vec![pkcu(4, 0x07002CB0, 3)];
        for unam in [1, 3, 5, 7] {
            instance_fields.extend(pack_bool_input(unam));
        }
        instance_fields.push(raw_field("XNAM", Vec::new()));
        let mut record = make_pack_record_with_fields(instance_fields);

        let mut template_fields = vec![pkcu(5, 0, 2)];
        for unam in [1, 3, 5, 7, 9] {
            template_fields.extend(pack_bool_input(unam));
        }
        template_fields.push(raw_field("XNAM", Vec::new()));
        let template = make_pack_record_with_fields(template_fields);

        assert!(!resolve_pack_template_inputs_from_template(
            &mut record,
            &template
        ));
        let (pkcu_pos, resolved_pkcu) = read_pack_pkcu(&record).expect("PKCU");
        assert_eq!(resolved_pkcu.data_input_count, 4);
        assert_eq!(resolved_pkcu.version, 3);
        assert_eq!(pack_unam_values(&record, pkcu_pos), vec![1, 3, 5, 7]);
    }

    #[test]
    fn rewrites_raw_ctda_source_local_formid_parameters() {
        let mut record = make_pack_record("CTDA", ctda(14, 0, 0x00685532, 0));

        assert!(rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x07685532
        );
    }

    #[test]
    fn schema_rewrite_rewrites_byte_formids_for_any_record_signature() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("TERM").unwrap(),
            FormKey::parse("00ABCD@SeventySix.esm", &mut interner).unwrap(),
        );
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x0037D0C1_u32.to_le_bytes());
        raw.extend_from_slice(&0x005DC207_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });
        let record_spec = SchemaRecordJson {
            id: "TERM".to_string(),
            subrecords: vec![schema_subrecord(
                "XNAM",
                "struct:I,I",
                vec![
                    schema_field("schema_ref", "formid"),
                    schema_field("numeric_value", "uint32"),
                ],
            )],
            ..Default::default()
        };
        let mut is_valid_target_master_formid = |_| false;

        assert!(rewrite_schema_record_raw_formids(
            &mut record,
            &record_spec,
            &empty_compiled_schema(),
            &target_map(),
            &mut is_valid_target_master_formid,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0737D0C1
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x005DC207
        );
    }

    #[test]
    fn schema_rewrite_leaves_valid_target_master_raw_formids() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("TERM").unwrap(),
            FormKey::parse("00ABCD@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x0037D0C1_u32.to_le_bytes().to_vec())),
        });
        let record_spec = SchemaRecordJson {
            id: "TERM".to_string(),
            subrecords: vec![schema_subrecord(
                "XNAM",
                "formid",
                vec![schema_field("schema_ref", "formid")],
            )],
            ..Default::default()
        };
        let mut is_valid_target_master_formid = |raw| raw == 0x0037D0C1;

        assert!(!rewrite_schema_record_raw_formids(
            &mut record,
            &record_spec,
            &empty_compiled_schema(),
            &target_map(),
            &mut is_valid_target_master_formid,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0037D0C1
        );
    }

    #[test]
    fn rewrites_raw_xmsp_material_swap_formid() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                local: 0x07397881,
                plugin: output,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMSP").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x00093BC8_u32.to_le_bytes().to_vec())),
        });

        assert!(rewrite_xmsp_material_swap_record(
            &mut record,
            &target_map(),
            output,
            &target_masters(),
            &interner,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x07093BC8
        );
    }

    #[test]
    fn rewrites_raw_xmsp_material_swap_from_output_record_target() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let mut encoded_targets = FxHashMap::default();
        encoded_targets.insert(0x00114118, 0x07114118);
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                local: 0x07397881,
                plugin: output,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMSP").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x00114118_u32.to_le_bytes().to_vec())),
        });

        assert!(rewrite_xmsp_material_swap_record(
            &mut record,
            &encoded_targets,
            output,
            &target_masters(),
            &interner,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x07114118
        );
    }

    #[test]
    fn decoded_rewrite_rewrites_invalid_first_target_master_xmsp_formkey() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("SeventySix.esm");
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey {
                local: 0x07389BF9,
                plugin: output,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMSP").unwrap(),
            value: FieldValue::FormKey(FormKey {
                local: 0x00093BC8,
                plugin: fallout4,
            }),
        });
        let mut is_valid_target_master_formid = |_| true;

        assert!(rewrite_decoded_target_master_formids(
            &mut record,
            Some(fallout4),
            output,
            &target_masters(),
            &interner,
            &target_map(),
            false,
            &mut is_valid_target_master_formid,
        ));
        let FieldValue::FormKey(fk) = &record.fields[0].value else {
            panic!("expected formkey");
        };
        assert_eq!(
            *fk,
            FormKey {
                local: 0x00093BC8,
                plugin: output,
            }
        );
    }

    #[test]
    fn decoded_rewrite_walks_nested_values_and_leaves_valid_target_master_refs() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("SeventySix.esm");
        let nested = interner.intern("nested");
        let mut record = Record::new(
            SigCode::from_str("TERM").unwrap(),
            FormKey {
                local: 0x0000ABCD,
                plugin: output,
            },
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XNAM").unwrap(),
            value: FieldValue::Struct(vec![(
                nested,
                FieldValue::List(vec![
                    FieldValue::FormKey(FormKey {
                        local: 0x00093BC8,
                        plugin: fallout4,
                    }),
                    FieldValue::FormKey(FormKey {
                        local: 0x0037D0C1,
                        plugin: fallout4,
                    }),
                ]),
            )]),
        });
        let mut is_valid_target_master_formid = |raw| raw == 0x0037D0C1;

        assert!(rewrite_decoded_target_master_formids(
            &mut record,
            Some(fallout4),
            output,
            &target_masters(),
            &interner,
            &target_map(),
            true,
            &mut is_valid_target_master_formid,
        ));
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected struct");
        };
        let FieldValue::List(items) = &fields[0].1 else {
            panic!("expected list");
        };
        assert_eq!(
            items[0],
            FieldValue::FormKey(FormKey {
                local: 0x00093BC8,
                plugin: output,
            })
        );
        assert_eq!(
            items[1],
            FieldValue::FormKey(FormKey {
                local: 0x0037D0C1,
                plugin: fallout4,
            })
        );
    }

    #[test]
    fn rewrites_raw_ctda_fo4_formid_parameter_function() {
        let mut record = make_pack_record("CTDA", ctda(506, 0, 0x00685532, 0));

        assert!(rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x07685532
        );
    }

    #[test]
    fn rewrites_raw_ctda_global_comparison_and_param2_refs() {
        let mut record = make_pack_record("CTDA", ctda(605, 0x04, 0, 0x005B2FFF));

        assert!(rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x07685532
        );
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            0x075B2FFF
        );
    }

    #[test]
    fn leaves_raw_ctda_non_formid_parameters_unchanged() {
        let mut record = make_pack_record("CTDA", ctda(289, 0, 0x00685532, 0));

        assert!(!rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x00685532
        );
    }

    #[test]
    fn drops_raw_ctda_unmapped_source_local_formid_parameter() {
        let mut record = make_pack_record("CTDA", ctda(67, 0, 0x0036DABA, 0));

        assert!(rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| false,
        ));
        assert!(
            record
                .fields
                .iter()
                .all(|field| field.sig.as_str() != "CTDA"),
            "unmapped source-local condition parameter should drop the condition"
        );
    }

    #[test]
    fn keeps_raw_ctda_valid_target_master_formid_parameter() {
        let mut record = make_pack_record("CTDA", ctda(67, 0, 0x00002CE0, 0));

        assert!(!rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| true,
        ));
        assert_eq!(record.fields[0].sig.as_str(), "CTDA");
    }

    #[test]
    fn drops_condition_script_strings_with_invalid_raw_ctda() {
        let mut record = make_pack_record("CTDA", ctda(67, 0, 0x0036DABA, 0));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CIS1").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b"ScriptName\0".to_vec())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("CIS2").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(b"VariableName\0".to_vec())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("EDID").unwrap(),
            value: FieldValue::None,
        });

        assert!(rewrite_raw_condition_record(
            &mut record,
            &target_map(),
            &mut |_| false,
        ));
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["EDID"]);
    }

    #[test]
    fn leaves_mgef_data_archetype_unchanged() {
        let mut data = vec![0_u8; 152];
        data[64..68].copy_from_slice(&36_u32.to_le_bytes());
        let mut record = make_mgef_record_with_data(data);

        let interner = StringInterner::new();
        assert!(!rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[64..68].try_into().unwrap()), 36);
    }

    #[test]
    fn rewrites_ipds_pnam_material_and_impact_formids() {
        let mut record = make_ipds_record(&[(0x0037D0C1, 0x005F6DC1), (0x00002CE0, 0x005DC207)]);

        let interner = StringInterner::new();
        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(first) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(first[0..4].try_into().unwrap()),
            0x0737D0C1
        );
        assert_eq!(
            u32::from_le_bytes(first[4..8].try_into().unwrap()),
            0x075F6DC1
        );
        let FieldValue::Bytes(second) = &record.fields[1].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(second[0..4].try_into().unwrap()),
            0x00002CE0
        );
        assert_eq!(
            u32::from_le_bytes(second[4..8].try_into().unwrap()),
            0x075DC207
        );
    }

    #[test]
    fn rewrites_expl_data_formid_slots_and_nulls_invalid_target_master_refs() {
        let mut record = make_expl_record(&[
            0x000D8F99, 0x0006928F, 0, 0x0001F5B0, 0x00594762, 0x005566D8, 0x0067AC0D,
        ]);
        let mut map = target_map();
        map.insert(0x0006928F, 0x0706928F);
        map.insert(0x0001F5B0, 0x0701F5B0);
        let interner = StringInterner::new();
        let mut valid_master_formids = |raw: u32| raw == 0x000D8F99;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &map,
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x000D8F99
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x0706928F
        );
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x0701F5B0
        );
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(bytes[20..24].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            0x0767AC0D
        );
    }

    #[test]
    fn rewrites_ammo_dnam_projectile_raw_formid() {
        let mut record = make_ammo_record_with_dnam_raw(0x0011202B);
        let mut map = target_map();
        map.insert(0x0011202B, 0x0711202B);
        let interner = StringInterner::new();

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &map,
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0711202B
        );
    }

    #[test]
    fn rewrites_decoded_ammo_dnam_projectile_from_first_master_to_output_plugin() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let output = interner.intern("SeventySix.esm");
        let mut record = make_ammo_record_with_structured_dnam(
            &interner,
            FormKey {
                local: 0x0011202B,
                plugin: fallout4,
            },
        );
        let mut map = target_map();
        map.insert(0x0011202B, 0x0711202B);

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &map,
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut |_| true,
        ));
        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("expected structured DNAM");
        };
        assert_eq!(
            fields[0].1,
            FieldValue::FormKey(FormKey {
                local: 0x0011202B,
                plugin: output,
            })
        );
    }

    #[test]
    fn rewrites_hazd_dnam_formid_slots() {
        let mut record = make_hazd_record(0x0064ADB6, 0x003CFFF3, 0, 0x004E6157);
        let interner = StringInterner::new();

        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            0x0764ADB6
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            0x073CFFF3
        );
        assert_eq!(u32::from_le_bytes(bytes[32..36].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(bytes[36..40].try_into().unwrap()),
            0x074E6157
        );
    }

    #[test]
    fn rewrites_proj_dnam_formid_slots() {
        let mut record = make_proj_record(0x0031F006, 0x007A85B0, 0x0043F37E, 0x00004117);
        let mut map = target_map();
        map.insert(0x00004117, 0x07004117);
        let interner = StringInterner::new();

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &map,
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
            0x0731F006
        );
        assert_eq!(
            u32::from_le_bytes(bytes[32..36].try_into().unwrap()),
            0x077A85B0
        );
        assert_eq!(
            u32::from_le_bytes(bytes[36..40].try_into().unwrap()),
            0x0743F37E
        );
        assert_eq!(
            u32::from_le_bytes(bytes[60..64].try_into().unwrap()),
            0x07004117
        );
    }

    #[test]
    fn rewrites_fsts_data_footstep_formids() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x00005071_u32.to_le_bytes());
        raw.extend_from_slice(&0x00005070_u32.to_le_bytes());
        raw.extend_from_slice(&0x00005071_u32.to_le_bytes());
        raw.extend_from_slice(&0x00005070_u32.to_le_bytes());
        let mut record = make_fsts_record_with_data(raw);

        let interner = StringInterner::new();
        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x07005071
        );
        assert_eq!(
            u32::from_le_bytes(bytes[4..8].try_into().unwrap()),
            0x07005070
        );
    }

    #[test]
    fn rewrites_dstd_stage_explosion_formid() {
        let mut raw = vec![0_u8; 20];
        raw[8..12].copy_from_slice(&0x007EBA4B_u32.to_le_bytes());
        let mut record = make_cont_record("DSTD", raw);

        assert!(rewrite_destruction_stage_record(&mut record, &target_map()));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x077EBA4B
        );
    }

    #[test]
    fn rewrites_prps_actor_value_formids_and_drops_invalid_rows() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x007EBA4B_u32.to_le_bytes());
        raw.extend_from_slice(&1.0_f32.to_le_bytes());
        raw.extend_from_slice(&0x000002DC_u32.to_le_bytes());
        raw.extend_from_slice(&0.4_f32.to_le_bytes());
        raw.extend_from_slice(&0x0000038A_u32.to_le_bytes());
        raw.extend_from_slice(&1.0_f32.to_le_bytes());
        let mut record = make_cont_record("PRPS", raw);

        let mut is_valid_target_master_formid = |raw_form_id| raw_form_id == 0x000002DC;
        assert!(rewrite_prps_record(
            &mut record,
            &target_map(),
            &mut is_valid_target_master_formid,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(bytes.len(), 16);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x077EBA4B
        );
        assert_eq!(
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            0x000002DC
        );
    }

    #[test]
    fn rewrites_regn_rdwt_weather_formids_in_raw_bytes() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x007EBA4B_u32.to_le_bytes());
        raw.extend_from_slice(&95_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0x007EE02B_u32.to_le_bytes());
        raw.extend_from_slice(&5_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        let mut record = make_regn_record_with_rdwt(raw);

        let interner = StringInterner::new();
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x077EBA4B, SigCode::from_str("WTHR").unwrap());
        target_sigs.insert(0x077EE02B, SigCode::from_str("WTHR").unwrap());
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut |_| true,
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x077EBA4B
        );
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x077EE02B
        );
    }

    #[test]
    fn rewrites_regn_rdsa_sound_formids_in_raw_bytes() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x00196CCB_u32.to_le_bytes());
        raw.extend_from_slice(&15_u32.to_le_bytes());
        raw.extend_from_slice(&0.02f32.to_le_bytes());
        raw.extend_from_slice(&0x00000000_u32.to_le_bytes());
        raw.extend_from_slice(&15_u32.to_le_bytes());
        raw.extend_from_slice(&0.04f32.to_le_bytes());
        let mut record = make_regn_record_with_rdsa(raw);

        let interner = StringInterner::new();
        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x07196CCB
        );
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x00000000
        );
    }

    #[test]
    fn rewrites_stag_tnam_sound_formid_in_raw_bytes() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x00554A9E_u32.to_le_bytes());
        raw.extend_from_slice(b"NPCCatIdleSittingToStanding\0");
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("STAG").unwrap(),
            FormKey::parse("54822C@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(raw)),
        });

        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x07554A9E
        );
        assert_eq!(&bytes[4..], b"NPCCatIdleSittingToStanding\0");
    }

    #[test]
    fn rewrites_placed_ref_location_and_loc_ref_type_formids() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("ACHR").unwrap(),
            FormKey::parse("59646D@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XLCN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x002CD2E2_u32.to_le_bytes().to_vec())),
        });
        let mut xlrt = Vec::new();
        xlrt.extend_from_slice(&0x00003956_u32.to_le_bytes());
        xlrt.extend_from_slice(&0x00358545_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XLRT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(xlrt)),
        });
        let mut valid_master_formids = |raw: u32| raw == 0x00003956;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));

        let FieldValue::Bytes(xlcn) = &record.fields[0].value else {
            panic!("expected XLCN bytes");
        };
        assert_eq!(
            u32::from_le_bytes(xlcn[0..4].try_into().unwrap()),
            0x072CD2E2
        );

        let FieldValue::Bytes(xlrt) = &record.fields[1].value else {
            panic!("expected XLRT bytes");
        };
        assert_eq!(xlrt.len(), 8);
        assert_eq!(
            u32::from_le_bytes(xlrt[0..4].try_into().unwrap()),
            0x00003956
        );
        assert_eq!(
            u32::from_le_bytes(xlrt[4..8].try_into().unwrap()),
            0x00003956
        );
    }

    #[test]
    fn prunes_placed_ref_encounter_zone_when_target_is_location() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey::parse("595C33@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XEZN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x002CD2E2_u32.to_le_bytes().to_vec())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XLCN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x002CD2E2_u32.to_le_bytes().to_vec())),
        });
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x072CD2E2, SigCode::from_str("LCTN").unwrap());
        let mut valid_master_formids = |_raw: u32| false;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut valid_master_formids,
        ));

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "XLCN");
        let FieldValue::Bytes(xlcn) = &record.fields[0].value else {
            panic!("expected XLCN bytes");
        };
        assert_eq!(
            u32::from_le_bytes(xlcn[0..4].try_into().unwrap()),
            0x072CD2E2
        );
    }

    #[test]
    fn keeps_placed_ref_encounter_zone_when_target_is_encounter_zone() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey::parse("595C33@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XEZN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x002CD2E2_u32.to_le_bytes().to_vec())),
        });
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x072CD2E2, SigCode::from_str("ECZN").unwrap());
        let mut valid_master_formids = |_raw: u32| false;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut valid_master_formids,
        ));

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "XEZN");
        let FieldValue::Bytes(xezn) = &record.fields[0].value else {
            panic!("expected XEZN bytes");
        };
        assert_eq!(
            u32::from_le_bytes(xezn[0..4].try_into().unwrap()),
            0x072CD2E2
        );
    }

    #[test]
    fn prunes_placed_ref_encounter_zone_when_target_is_unindexed_master() {
        // Regression (#1): the output-only encoded-sig index does NOT include
        // master-resident records, so a placed XEZN that resolves to a LCTN
        // living in a master (Fallout4.esm/DLC) is unindexed. A non-ECZN XEZN is
        // always invalid for FO4 placed refs, so an unresolved XEZN must still be
        // stripped (it formerly leaked through as "kept").
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey::parse("595C33@SeventySix.esm", &mut interner).unwrap(),
        );
        // Already master-encoded (master byte != 0) so the raw rewrite leaves it
        // Unchanged and the type check runs; valid-master predicate confirms it
        // points at a real master record, but the sig index can't see it.
        let master_eczn_candidate = 0x01AB_CDEFu32;
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XEZN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(
                master_eczn_candidate.to_le_bytes().to_vec(),
            )),
        });
        let mut valid_master_formids = |raw: u32| raw == master_eczn_candidate;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(), // empty — master target is unindexed
            &mut valid_master_formids,
        ));

        assert!(
            record.fields.is_empty(),
            "unindexed (master-resident) XEZN target must be stripped"
        );
    }

    #[test]
    fn keeps_placed_ref_location_when_target_is_unindexed_master() {
        // Companion to the above: XLCN's valid target IS a LCTN, and a
        // master-resident LCTN is legitimate. An unindexed XLCN must stay
        // conservative (kept), not get swept up by the XEZN strip policy.
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey::parse("595C33@SeventySix.esm", &mut interner).unwrap(),
        );
        let master_lctn = 0x01AB_CDEFu32;
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XLCN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(master_lctn.to_le_bytes().to_vec())),
        });
        let mut valid_master_formids = |raw: u32| raw == master_lctn;

        let _ = rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        );

        assert_eq!(record.fields.len(), 1, "unindexed master XLCN must be kept");
        assert_eq!(record.fields[0].sig.as_str(), "XLCN");
    }

    #[test]
    fn prunes_placed_grenade_encounter_zone_when_target_is_location() {
        // PGRE (placed projectile/mine) carries XEZN→LCTN in FO76 just like
        // REFR/ACHR; FO4 requires ECZN, so the field must be stripped.
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("PGRE").unwrap(),
            FormKey::parse("3C0663@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XEZN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x002CD2E2_u32.to_le_bytes().to_vec())),
        });
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x072CD2E2, SigCode::from_str("LCTN").unwrap());
        let mut valid_master_formids = |_raw: u32| false;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut valid_master_formids,
        ));

        assert!(
            record.fields.is_empty(),
            "PGRE XEZN pointing at a LCTN must be stripped"
        );
    }

    #[test]
    fn rewrites_placed_ref_current_zone_cell_formid() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey::parse("55AE1C@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XCZC").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x00262208_u32.to_le_bytes().to_vec())),
        });
        let mut valid_master_formids = |_raw: u32| false;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));

        let FieldValue::Bytes(xczc) = &record.fields[0].value else {
            panic!("expected XCZC bytes");
        };
        assert_eq!(
            u32::from_le_bytes(xczc[0..4].try_into().unwrap()),
            0x07262208
        );
    }

    #[test]
    fn prunes_unmapped_placed_ref_location_formids() {
        let mut interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("REFR").unwrap(),
            FormKey::parse("59646D@SeventySix.esm", &mut interner).unwrap(),
        );
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XEZN").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x00BEEF01_u32.to_le_bytes().to_vec())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XLRT").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(0x00BEEF01_u32.to_le_bytes().to_vec())),
        });
        let mut valid_master_formids = |_raw: u32| false;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn rewrites_rfct_effect_art_formid_in_raw_bytes() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x0048CC2A_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&6_u32.to_le_bytes());
        let mut record = make_rfct_record_with_data(raw);

        let interner = StringInterner::new();
        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0748CC2A
        );
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 6);
    }

    #[test]
    fn rewrites_regn_rdwt_weather_formids_in_structured_rows() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_with_structured_rdwt(&mut interner);

        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x077EBA4B, SigCode::from_str("WTHR").unwrap());
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut |_| true,
        ));
        let FieldValue::List(rows) = &record.fields[0].value else {
            panic!("expected RDWT rows");
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected RDWT row struct");
        };
        let FieldValue::FormKey(weather_fk) = &fields[0].1 else {
            panic!("expected weather FormKey");
        };
        assert_eq!(weather_fk.local, 0x007EBA4B);
        assert_eq!(interner.resolve(weather_fk.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn drops_regn_dangling_worldspace_reference() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WNAM").unwrap(),
            value: FieldValue::FormKey(
                FormKey::parse("00DC6D@TestOut.esp", &mut interner).unwrap(),
            ),
        });

        let mut valid_master_formids = |_raw: u32| false;
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn keeps_regn_worldspace_reference_when_output_wrld_exists() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WNAM").unwrap(),
            value: FieldValue::FormKey(
                FormKey::parse("25DA15@TestOut.esp", &mut interner).unwrap(),
            ),
        });
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x0725DA15, SigCode::from_str("WRLD").unwrap());

        let mut valid_master_formids = |_raw: u32| false;
        assert!(!rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut valid_master_formids,
        ));
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn drops_regn_dangling_music_reference() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDMO").unwrap(),
            value: FieldValue::FormKey(
                FormKey::parse("1096F7@TestOut.esp", &mut interner).unwrap(),
            ),
        });

        let mut valid_master_formids = |_raw: u32| false;
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn drops_orphan_regn_data_entries_after_payloads_are_removed() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        record.fields.push(regn_rdat(2));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDOT").unwrap(),
            value: FieldValue::List(Vec::new()),
        });
        record.fields.push(regn_rdat(3));
        record.fields.push(regn_rdat(7));

        let mut valid_master_formids = |_raw: u32| false;
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn keeps_regn_data_entry_with_matching_weather_payload() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        let weather_key = interner.intern("WeatherTypesWeather");
        let chance_key = interner.intern("WeatherTypesChance");
        let global_key = interner.intern("WeatherTypesGlobal");
        record.fields.push(regn_rdat(3));
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::List(vec![FieldValue::Struct(vec![
                (
                    weather_key,
                    FieldValue::FormKey(
                        FormKey::parse("4398AE@TestOut.esp", &mut interner).unwrap(),
                    ),
                ),
                (chance_key, FieldValue::Uint(100)),
                (global_key, FieldValue::None),
            ])]),
        });
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x074398AE, SigCode::from_str("WTHR").unwrap());

        let mut valid_master_formids = |_raw: u32| false;
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut valid_master_formids,
        ));
        let sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect();
        assert_eq!(sigs, vec!["RDAT", "RDWT"]);
    }

    #[test]
    fn drops_area_geometry_when_regn_worldspace_is_removed() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("WNAM").unwrap(),
            value: FieldValue::FormKey(
                FormKey::parse("00DC6D@TestOut.esp", &mut interner).unwrap(),
            ),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RPLI").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(1024_u32.to_le_bytes().to_vec())),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RPLD").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(vec![0; 32])),
        });
        record.fields.push(regn_rdat(3));

        let mut valid_master_formids = |_raw: u32| false;
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn drops_regn_rdwt_rows_with_dangling_weather_refs() {
        let mut interner = StringInterner::new();
        let mut record = make_regn_record_for_output(&mut interner);
        let weather_key = interner.intern("WeatherTypesWeather");
        let chance_key = interner.intern("WeatherTypesChance");
        let global_key = interner.intern("WeatherTypesGlobal");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("RDWT").unwrap(),
            value: FieldValue::List(vec![
                FieldValue::Struct(vec![
                    (
                        weather_key,
                        FieldValue::FormKey(
                            FormKey::parse("4398AE@TestOut.esp", &mut interner).unwrap(),
                        ),
                    ),
                    (chance_key, FieldValue::Uint(65)),
                    (global_key, FieldValue::None),
                ]),
                FieldValue::Struct(vec![
                    (
                        weather_key,
                        FieldValue::FormKey(
                            FormKey::parse("00DC6D@TestOut.esp", &mut interner).unwrap(),
                        ),
                    ),
                    (chance_key, FieldValue::Uint(35)),
                    (global_key, FieldValue::None),
                ]),
            ]),
        });
        let mut target_sigs = target_record_sigs();
        target_sigs.insert(0x074398AE, SigCode::from_str("WTHR").unwrap());

        let mut valid_master_formids = |_raw: u32| false;
        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_sigs,
            &mut valid_master_formids,
        ));
        let FieldValue::List(rows) = &record.fields[0].value else {
            panic!("expected RDWT rows");
        };
        assert_eq!(rows.len(), 1);
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected RDWT row struct");
        };
        let FieldValue::FormKey(weather_fk) = &fields[0].1 else {
            panic!("expected weather FormKey");
        };
        assert_eq!(weather_fk.local, 0x004398AE);
    }

    #[test]
    fn rewrites_clmt_wlst_weather_formids_in_raw_bytes() {
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x004398AE_u32.to_le_bytes());
        raw.extend_from_slice(&100_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        let mut record = make_clmt_record_with_wlst(raw);

        let interner = StringInterner::new();
        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x074398AE
        );
    }

    #[test]
    fn rewrites_clmt_wlst_weather_formids_in_structured_rows() {
        let mut interner = StringInterner::new();
        let mut record = make_clmt_record_with_structured_wlst(&mut interner);

        assert!(rewrite_record_for_test(&mut record, &interner));
        let FieldValue::List(rows) = &record.fields[0].value else {
            panic!("expected WLST rows");
        };
        let FieldValue::Struct(fields) = &rows[0] else {
            panic!("expected WLST row struct");
        };
        let FieldValue::FormKey(weather_fk) = &fields[0].1 else {
            panic!("expected weather FormKey");
        };
        assert_eq!(weather_fk.local, 0x004398AE);
        assert_eq!(interner.resolve(weather_fk.plugin), Some("SeventySix.esm"));
    }

    #[test]
    fn rewrites_wthr_raw_imagespace_sound_and_spell_formids() {
        let mut record = make_wthr_record(vec![
            ("SNAM", formid_words(&[0x00196CCB, 3])),
            (
                "IMSP",
                formid_words(&[
                    0x007EBA4B, 0x004398AE, 0, 0x00002CE0, 0x005B2FFF, 0x005DC207, 0x005F6DC1,
                    0x00685532,
                ]),
            ),
            ("UNAM", formid_words(&[0x005B2FFF, 0, 0x007EE02B, 0, 0, 0])),
        ]);
        let interner = StringInterner::new();
        let mut valid_master_formids = |raw: u32| raw == 0x00002CE0;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));

        let FieldValue::Bytes(snam) = &record.fields[0].value else {
            panic!("expected SNAM bytes");
        };
        assert_eq!(
            u32::from_le_bytes(snam[0..4].try_into().unwrap()),
            0x07196CCB
        );
        assert_eq!(u32::from_le_bytes(snam[4..8].try_into().unwrap()), 3);

        let FieldValue::Bytes(imsp) = &record.fields[1].value else {
            panic!("expected IMSP bytes");
        };
        assert_eq!(
            u32::from_le_bytes(imsp[0..4].try_into().unwrap()),
            0x077EBA4B
        );
        assert_eq!(
            u32::from_le_bytes(imsp[4..8].try_into().unwrap()),
            0x074398AE
        );
        assert_eq!(
            u32::from_le_bytes(imsp[12..16].try_into().unwrap()),
            0x00002CE0
        );

        let FieldValue::Bytes(unam) = &record.fields[2].value else {
            panic!("expected UNAM bytes");
        };
        assert_eq!(
            u32::from_le_bytes(unam[0..4].try_into().unwrap()),
            0x075B2FFF
        );
        assert_eq!(
            u32::from_le_bytes(unam[8..12].try_into().unwrap()),
            0x077EE02B
        );
    }

    #[test]
    fn nulls_wthr_unmapped_raw_weather_formids() {
        let mut record = make_wthr_record(vec![
            ("SNAM", formid_words(&[0x0000038A, 1])),
            ("UNAM", formid_words(&[0x0000038A, 0, 0x0000038A, 0, 0, 0])),
        ]);
        let interner = StringInterner::new();
        let mut valid_master_formids = |_raw: u32| false;

        assert!(rewrite_record_raw_template_formids(
            &mut record,
            &target_map(),
            &target_masters(),
            &interner,
            &target_record_sigs(),
            &mut valid_master_formids,
        ));

        let FieldValue::Bytes(snam) = &record.fields[0].value else {
            panic!("expected SNAM bytes");
        };
        assert_eq!(u32::from_le_bytes(snam[0..4].try_into().unwrap()), 0);

        let FieldValue::Bytes(unam) = &record.fields[1].value else {
            panic!("expected UNAM bytes");
        };
        assert_eq!(u32::from_le_bytes(unam[0..4].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(unam[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn rewrites_omod_data_attach_slots_and_included_mods() {
        let mut record = make_record_with_omod_data(omod_data(
            0x00685532,
            &[0x0037D0C1, 0x00000000],
            &[0x005F6DC1, 0x005DC207],
            Some(0x005B2FFF),
        ));

        assert!(rewrite_omod_data_record(
            &mut record,
            &target_map(),
            &mut |_raw: u32| true
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[16..20].try_into().unwrap()),
            0x07685532
        );
        assert_eq!(
            u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
            0x00000002
        );
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            0x0737D0C1
        );
        assert_eq!(
            u32::from_le_bytes(bytes[28..32].try_into().unwrap()),
            0x00000000
        );
        assert_eq!(
            u32::from_le_bytes(bytes[36..40].try_into().unwrap()),
            0x075F6DC1
        );
        assert_eq!(
            u32::from_le_bytes(bytes[43..47].try_into().unwrap()),
            0x075DC207
        );
        assert_eq!(
            u32::from_le_bytes(bytes[62..66].try_into().unwrap()),
            0x005B2FFF
        );
    }

    #[test]
    fn rewrites_omod_mods_material_swap_formid() {
        let mut record = make_record_with_omod_mods(0x0037D0C1);

        assert!(rewrite_omod_data_record(
            &mut record,
            &target_map(),
            &mut |_raw: u32| true
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0737D0C1
        );
    }

    #[test]
    fn rewrites_omod_material_swap_property_formids() {
        let mut record =
            make_record_with_omod_data(omod_data_with_property(13, 0x0037D0C1, 0x005DC207));

        assert!(rewrite_omod_data_record(
            &mut record,
            &target_map(),
            &mut |_raw: u32| true
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            0x0737D0C1
        );
        assert_eq!(
            u32::from_le_bytes(bytes[44..48].try_into().unwrap()),
            0x075DC207
        );
    }

    #[test]
    fn leaves_non_material_omod_property_values_unchanged() {
        let mut record =
            make_record_with_omod_data(omod_data_with_property(3, 0x0037D0C1, 0x005DC207));

        assert!(!rewrite_omod_data_record(
            &mut record,
            &target_map(),
            &mut |_raw: u32| true
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            0x0037D0C1
        );
        assert_eq!(
            u32::from_le_bytes(bytes[44..48].try_into().unwrap()),
            0x005DC207
        );
    }

    #[test]
    fn rewrites_formid_typed_property_value1_for_non_material_property() {
        // value_type 4 (FormID,Int), property 3 (AddKeyword) — the AddKeyword
        // case that left source-local ids dangling before the value_type gate.
        let mut record =
            make_record_with_omod_data(omod_data_with_typed_property(4, 3, 0x0037D0C1, 2));

        assert!(rewrite_omod_data_record(
            &mut record,
            &target_map(),
            &mut |_raw: u32| true
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        // Value 1 remapped to the target-encoded FormID.
        assert_eq!(
            u32::from_le_bytes(bytes[40..44].try_into().unwrap()),
            0x0737D0C1
        );
        // Value 2 is an Int for value_type 4 — must stay untouched.
        assert_eq!(u32::from_le_bytes(bytes[44..48].try_into().unwrap()), 2);
    }

    #[test]
    fn nulls_formid_typed_property_value1_residue_when_unconverted() {
        // value_type 6 (FormID,Float), source-local id absent from the map and
        // not a valid target-master id → must be nulled rather than left to
        // resolve as a non-existent Fallout4.esm form.
        let mut record =
            make_record_with_omod_data(omod_data_with_typed_property(6, 3, 0x00ABCDEF, 0));

        assert!(rewrite_omod_data_record(
            &mut record,
            &target_map(),
            &mut |_raw: u32| false
        ));
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected raw bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 0);
    }
}
