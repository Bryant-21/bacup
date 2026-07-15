//! Target schema normalization for translated records.
//!
//! This module is the structural boundary between semantic conversion and
//! binary target writes. Fixups may still perform semantic repair, but records
//! written to a target plugin must first satisfy the target schema shape.

use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::{AuthoringSchema, FieldDef, RecordDef, SubrecordDef};
use crate::sym::{StringInterner, Sym};
use smallvec::SmallVec;
use std::collections::{HashMap, HashSet, VecDeque};

pub(crate) struct TargetRecordNormalizer<'a> {
    pub target_schema: &'a AuthoringSchema,
    pub source_record_def: Option<&'a RecordDef>,
    pub interner: Option<&'a StringInterner>,
}

pub(crate) enum TargetRecordNormalization {
    Keep(Record),
    DropUnsupportedRecord,
}

struct IndexedFieldEntry {
    original_index: usize,
    entry: FieldEntry,
}

impl<'a> TargetRecordNormalizer<'a> {
    #[cfg(test)]
    pub(crate) fn target_only(target_schema: &'a AuthoringSchema) -> Self {
        Self {
            target_schema,
            source_record_def: None,
            interner: None,
        }
    }

    pub(crate) fn target_only_with_interner(
        target_schema: &'a AuthoringSchema,
        interner: &'a StringInterner,
    ) -> Self {
        Self {
            target_schema,
            source_record_def: None,
            interner: Some(interner),
        }
    }

    pub(crate) fn normalize(&self, mut record: Record) -> TargetRecordNormalization {
        let rec_sig = record.sig;
        let rec_local = record.form_key.local;
        let Some(target_record_def) = self.target_schema.record_def(record.sig.as_str()) else {
            crate::drop_trace::trace(
                "normalize.drop_record",
                rec_sig.as_str(),
                rec_local,
                "",
                "record sig not in target schema",
            );
            return TargetRecordNormalization::DropUnsupportedRecord;
        };

        normalize_target_form_version_union_bytes(
            &mut record,
            self.target_schema,
            target_record_def,
        );
        adapt_fo76_imgs_hdr_to_fo4_hnam(&mut record, self.source_record_def, target_record_def);
        normalize_fo76_imgs_lut_paths(
            &mut record,
            self.source_record_def,
            target_record_def,
            self.interner,
        );
        normalize_translated_race_subgraph_paths(
            &mut record,
            self.source_record_def,
            target_record_def,
            self.interner,
        );
        strip_generated_additive_race_tint_tables(&mut record, self.interner);

        let supported_sigs = target_record_def
            .subrecords
            .iter()
            .filter_map(|spec| crate::ids::SubrecordSig::from_str(&spec.id).ok())
            .collect::<HashSet<_>>();
        let race_late_start = (target_record_def.id == "RACE")
            .then(|| {
                record
                    .fields
                    .iter()
                    .position(|entry| entry.sig.as_str() == "NAM0")
            })
            .flatten();
        // TERM has two target SNAM slots. FO76 writes the late Marker Parameters
        // as ZNAM, so keep that row separate from the early SNDR Looping Sound.
        let term_marker_anchor = (target_record_def.id == "TERM")
            .then(|| {
                record
                    .fields
                    .iter()
                    .rposition(|entry| entry.sig.as_str() == "XMRK")
            })
            .flatten();

        let mut fields_by_sig: HashMap<_, VecDeque<IndexedFieldEntry>> = HashMap::new();
        let mut term_marker_parameters = VecDeque::new();
        let mut race_late_entries = Vec::new();
        let original_field_count = record.fields.len();
        for (original_index, mut entry) in record.fields.drain(..).enumerate() {
            if target_record_def.id == "TERM"
                && (entry.sig.as_str() == "ZNAM"
                    || (entry.sig.as_str() == "SNAM"
                        && term_marker_anchor.is_some_and(|anchor| original_index > anchor)))
            {
                entry.sig =
                    crate::ids::SubrecordSig::from_str("ZNAM").expect("ZNAM is a valid signature");
                term_marker_parameters.push_back(IndexedFieldEntry {
                    original_index,
                    entry,
                });
                continue;
            }
            if supported_sigs.contains(&entry.sig) {
                let indexed = IndexedFieldEntry {
                    original_index,
                    entry,
                };
                if race_late_start.is_some_and(|start| original_index >= start) {
                    race_late_entries.push(indexed);
                } else {
                    fields_by_sig
                        .entry(indexed.entry.sig)
                        .or_default()
                        .push_back(indexed);
                }
            } else {
                crate::drop_trace::trace(
                    "normalize.unsupported_sig",
                    rec_sig.as_str(),
                    rec_local,
                    entry.sig.as_str(),
                    "subrecord sig absent from target-game schema",
                );
            }
        }

        let mut ordered = Vec::with_capacity(original_field_count);
        let mut schema_index = 0usize;
        while schema_index < target_record_def.subrecords.len() {
            let target_subrecord_def = &target_record_def.subrecords[schema_index];
            if let Some(scope_id) = target_subrecord_def.scope_id.as_deref() {
                let start = schema_index;
                schema_index += 1;
                while schema_index < target_record_def.subrecords.len()
                    && target_record_def.subrecords[schema_index]
                        .scope_id
                        .as_deref()
                        == Some(scope_id)
                {
                    schema_index += 1;
                }
                let segment = &target_record_def.subrecords[start..schema_index];
                if scope_id == "object_template" {
                    self.emit_object_template_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "SCEN" && scope_id == "phases" {
                    self.emit_scen_phases_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "SCEN" && scope_id == "actors" {
                    self.emit_scen_actors_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "SCEN" && scope_id == "actions" {
                    self.emit_scen_actions_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "PACK" && scope_id == "package_data" {
                    self.emit_pack_package_data_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "PACK" && scope_id == "procedure_tree" {
                    self.emit_pack_procedure_tree_segment(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if target_record_def.id == "QUST" && scope_id == "quest_dialogue_conditions"
                {
                    self.emit_qust_dialogue_conditions_segment(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if target_record_def.id == "QUST" && scope_id == "story_manager_conditions" {
                    self.emit_qust_story_manager_conditions_segment(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if target_record_def.id == "QUST" && scope_id == "objectives" {
                    self.emit_qust_objectives_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "QUST" && scope_id == "stages" {
                    self.emit_qust_stages_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "QUST" && scope_id == "aliases" {
                    self.emit_qust_alias_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "RACE" && scope_id == "body_data" {
                    self.emit_race_body_data_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if target_record_def.id == "RACE" && scope_id == "male_behavior_graph" {
                    self.emit_race_behavior_graph_segments(
                        target_record_def,
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if target_record_def.id == "RACE" && scope_id == "subgraph_data" {
                    // The FO4 RACE subgraph_data scope is [SAKD, SGNM, SAPT, STKD,
                    // SRAF]; the default emit_scoped_segment uses SAKD as a required
                    // row anchor, so FO76 creature behavior blocks that start with
                    // SGNM (no SAKD) get dropped wholesale → CK "Could not find base
                    // MT/weapon graph for race". Emit every member in source order.
                    self.emit_segment_body_in_source_order(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                        0,
                        usize::MAX,
                    );
                } else if target_record_def.id == "RACE" && scope_id == "equip_slots" {
                    self.emit_original_range_scoped_segment(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if target_record_def.id == "REGN" && scope_id == "region_data_entries" {
                    self.emit_regn_region_data_entries_segment(
                        segment,
                        target_record_def,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if scope_id == "effects" {
                    self.emit_effects_segment(
                        target_record_def,
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if scope_id == "destructible" {
                    // The destructible scope's first schema member is DEST (a
                    // once-only health/count header). The default
                    // emit_scoped_segment anchors every row on the first member,
                    // so with a singleton DEST it emits exactly ONE row —
                    // collapsing multi-stage destruction to stage 0 (one DSTD +
                    // one trailing DMDL/DMDT/DSTF) and dropping stages 1..N,
                    // including their Explosion refs. FO4 vehicles/movable
                    // statics then never catch fire or explode. Emit every
                    // destructible member in source order, which preserves each
                    // DSTD…DSTF stage group. FO76 also allows multiple alternative
                    // DMDL/DMDT model swaps inside one stage; FO4 accepts only one.
                    // Keep the first canonical model row per stage without
                    // collapsing distinct DSTD stages. (FO76-only
                    // ENLT/ENLS/AUUV members are already filtered by
                    // supported_sigs.)
                    self.emit_destructible_segment(segment, &mut fields_by_sig, &mut ordered);
                } else if is_condition_anchor_segment(segment) {
                    self.emit_original_range_scoped_segment(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else if matches!(scope_id, "body_text" | "menu_items")
                    && has_condition_string_rows(segment)
                {
                    self.emit_condition_string_scoped_segment(
                        segment,
                        &mut fields_by_sig,
                        &mut ordered,
                    );
                } else {
                    self.emit_scoped_segment(segment, &mut fields_by_sig, &mut ordered);
                }
                continue;
            }

            if is_term_marker_parameters_slot(target_record_def, target_subrecord_def) {
                self.emit_term_marker_parameters(
                    target_subrecord_def,
                    &mut term_marker_parameters,
                    &mut ordered,
                );
            } else {
                self.emit_unscoped_slot(
                    target_record_def,
                    schema_index,
                    target_subrecord_def,
                    &mut fields_by_sig,
                    &mut ordered,
                );
            }
            schema_index += 1;
        }
        self.emit_race_late_entries(target_record_def, race_late_entries, &mut ordered);

        // Any entries still in `fields_by_sig` were in the target schema but were
        // never bound to a member-list slot by the walk above (duplicate beyond
        // the schema's allowance, or a scope anchor that never matched) — they are
        // dropped here. This is the silent subrecord-loss path.
        if crate::drop_trace::enabled() {
            for deque in fields_by_sig.values() {
                for indexed in deque {
                    crate::drop_trace::trace(
                        "normalize.unconsumed",
                        rec_sig.as_str(),
                        rec_local,
                        indexed.entry.sig.as_str(),
                        "in target schema but no member-list slot consumed it",
                    );
                }
            }
        }

        record.fields = SmallVec::from_vec(ordered);
        drop_empty_imad_runtime_unsafe_subrecords(&mut record);
        coalesce_scol_duplicate_static_groups(&mut record);
        normalize_translated_cont_data(&mut record, self.source_record_def);
        normalize_translated_npc_acbs(&mut record, self.source_record_def);
        sync_ksiz_to_kwda(&mut record);
        sync_pack_pkcu_to_data_input_count(&mut record);
        pad_race_hclf_to_male_female(&mut record);

        TargetRecordNormalization::Keep(record)
    }

    fn emit_unscoped_slot(
        &self,
        target_record_def: &RecordDef,
        schema_index: usize,
        target_subrecord_def: &SubrecordDef,
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
            return;
        };
        if should_skip_qust_unscoped_condition_slot(target_record_def, sig, fields_by_sig) {
            return;
        }
        if should_skip_optional_unscoped_duplicate(
            target_record_def,
            schema_index,
            target_subrecord_def,
            fields_by_sig,
        ) {
            return;
        }
        let Some(entries) = fields_by_sig.get_mut(&sig) else {
            return;
        };

        if target_subrecord_def.multiple {
            while let Some(indexed) = entries.pop_front() {
                self.push_normalized_entry(indexed.entry, target_subrecord_def, ordered);
            }
        } else if let Some(indexed) = entries.pop_front() {
            self.push_normalized_entry(indexed.entry, target_subrecord_def, ordered);
        }
    }

    fn emit_term_marker_parameters(
        &self,
        target_subrecord_def: &SubrecordDef,
        marker_parameters: &mut VecDeque<IndexedFieldEntry>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(indexed) = marker_parameters.pop_front() else {
            return;
        };
        let mut entry = self.normalize_subrecord_entry(indexed.entry, target_subrecord_def);
        entry.sig = crate::ids::SubrecordSig::from_str("SNAM").expect("SNAM is a valid signature");
        if !drop_empty_optional_fixed_subrecord(&entry, target_subrecord_def) {
            ordered.push(entry);
        }
    }

    fn emit_race_body_data_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };
        let Some(range_start) = first_index_for_sig(fields_by_sig, anchor_sig) else {
            return;
        };
        let range_end = crate::ids::SubrecordSig::from_str("GNAM")
            .ok()
            .and_then(|sig| first_index_for_sig(fields_by_sig, sig))
            .unwrap_or(usize::MAX);
        let sigs = segment_sigs(segment).into_iter().collect::<HashSet<_>>();
        let entries = pop_all_sigs_in_original_range(fields_by_sig, &sigs, range_start, range_end);
        let def_for_sig = segment
            .iter()
            .filter_map(|def| {
                crate::ids::SubrecordSig::from_str(&def.id)
                    .ok()
                    .map(|sig| (sig, def))
            })
            .collect::<HashMap<_, _>>();
        for indexed in entries {
            if let Some(target_subrecord_def) = def_for_sig.get(&indexed.entry.sig).copied() {
                self.push_normalized_entry(indexed.entry, target_subrecord_def, ordered);
            }
        }
    }

    fn emit_race_late_entries(
        &self,
        target_record_def: &RecordDef,
        mut entries: Vec<IndexedFieldEntry>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let mut defs_for_sig = HashMap::<_, Vec<_>>::new();
        for target_subrecord_def in &target_record_def.subrecords {
            let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                continue;
            };
            defs_for_sig
                .entry(sig)
                .or_default()
                .push(target_subrecord_def);
        }
        let source_bsms_defs = self
            .source_record_def
            .into_iter()
            .flat_map(|record_def| record_def.subrecords.iter())
            .filter(|subrecord_def| subrecord_def.id == "BSMS")
            .collect::<Vec<_>>();

        entries.sort_by_key(|entry| entry.original_index);
        let mut in_bone_range_modifiers = false;
        let mut head_data_index = 0usize;
        let mut male_head = Vec::new();
        let mut female_head = Vec::new();
        let mut race_links = Vec::new();
        let mut subgraphs = Vec::new();
        let mut chatter = Vec::new();
        let mut morph_values = Vec::new();
        let mut late_metadata = Vec::new();
        let mut bone_scale_data = Vec::new();
        let mut heads = Vec::new();
        let mut icons = Vec::new();
        let mut face_morph_names = Vec::new();

        for indexed in entries {
            match &indexed.entry.sig.0 {
                b"BSMP" => in_bone_range_modifiers = false,
                b"BMMP" => in_bone_range_modifiers = true,
                _ => {}
            }
            let definition_index =
                usize::from(indexed.entry.sig.0 == *b"BSMS" && in_bone_range_modifiers);
            let Some(target_subrecord_def) = defs_for_sig
                .get(&indexed.entry.sig)
                .and_then(|definitions| {
                    definitions
                        .get(definition_index)
                        .or_else(|| definitions.first())
                })
                .copied()
            else {
                continue;
            };
            let sig = indexed.entry.sig.0;
            if sig == *b"INDX" {
                continue;
            }
            let normalized = if sig == *b"BSMS" {
                self.normalize_subrecord_entry_with_source_def(
                    indexed.entry,
                    source_bsms_defs.get(definition_index).copied(),
                    target_subrecord_def,
                )
            } else {
                self.normalize_subrecord_entry(indexed.entry, target_subrecord_def)
            };
            if drop_empty_optional_fixed_subrecord(&normalized, target_subrecord_def) {
                continue;
            }

            match &sig {
                b"NAM0" => {
                    head_data_index += 1;
                    if head_data_index == 1 {
                        male_head.push(normalized);
                    } else {
                        female_head.push(normalized);
                    }
                }
                b"FNAM" | b"RPRF" | b"AHCF" | b"FTSF" | b"DFTF" => {
                    head_data_index = head_data_index.max(2);
                    female_head.push(normalized);
                }
                b"MNAM" | b"NNAM" | b"RPRM" | b"AHCM" | b"FTSM" | b"DFTM" | b"WMAP" => {
                    if head_data_index >= 2 {
                        female_head.push(normalized);
                    } else {
                        male_head.push(normalized);
                    }
                }
                b"NAM8" | b"RNAM" | b"SRAC" => race_links.push(normalized),
                b"SADD" | b"SAKD" | b"STKD" | b"SGNM" | b"SAPT" | b"SRAF" => {
                    subgraphs.push(normalized);
                }
                b"PTOP" | b"NTOP" => chatter.push(normalized),
                b"MSID" | b"MSM0" | b"MSM1" => morph_values.push(normalized),
                b"MLSI" | b"HNAM" | b"HLTX" | b"QSTI" => late_metadata.push(normalized),
                b"BSMP" | b"BSMB" | b"BSMS" | b"BMMP" => bone_scale_data.push(normalized),
                b"HEAD" => heads.push(normalized),
                b"ICON" => icons.push(normalized),
                b"FMRI" | b"FMRN" => face_morph_names.push(normalized),
                _ if head_data_index >= 2 => female_head.push(normalized),
                _ => male_head.push(normalized),
            }
        }

        ordered.extend(male_head);
        ordered.extend(female_head);
        ordered.extend(race_links);
        ordered.extend(subgraphs);
        ordered.extend(chatter);
        ordered.extend(morph_values);
        ordered.extend(late_metadata);
        ordered.extend(bone_scale_data);
        ordered.extend(heads);
        ordered.extend(icons);
        ordered.extend(face_morph_names);
    }

    fn emit_race_behavior_graph_segments(
        &self,
        target_record_def: &RecordDef,
        male_segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some((_, female_segment)) =
            scoped_segment_by_id(target_record_def, "female_behavior_graph")
        else {
            self.emit_scoped_segment(male_segment, fields_by_sig, ordered);
            return;
        };

        let Ok(male_marker_sig) = crate::ids::SubrecordSig::from_str("MNAM") else {
            self.emit_scoped_segment(male_segment, fields_by_sig, ordered);
            return;
        };
        let Ok(female_marker_sig) = crate::ids::SubrecordSig::from_str("FNAM") else {
            self.emit_scoped_segment(male_segment, fields_by_sig, ordered);
            return;
        };

        let behavior_sigs = segment_sigs(male_segment)
            .into_iter()
            .chain(segment_sigs(female_segment))
            .collect::<HashSet<_>>();
        let range_end = first_schema_boundary_after_scope(
            target_record_def,
            "female_behavior_graph",
            &behavior_sigs,
            fields_by_sig,
        );
        let Some(first_male_marker_index) =
            fields_by_sig.get(&male_marker_sig).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.original_index < range_end)
                    .map(|entry| entry.original_index)
            })
        else {
            self.emit_scoped_segment(male_segment, fields_by_sig, ordered);
            return;
        };

        let mut entries = pop_all_sigs_in_original_range(
            fields_by_sig,
            &behavior_sigs,
            first_male_marker_index,
            range_end,
        );
        entries.sort_by_key(|entry| entry.original_index);

        let second_male_marker_index = entries
            .iter()
            .filter(|entry| {
                entry.entry.sig == male_marker_sig && entry.original_index > first_male_marker_index
            })
            .map(|entry| entry.original_index)
            .min();
        let first_female_marker_index = entries
            .iter()
            .filter(|entry| entry.entry.sig == female_marker_sig)
            .map(|entry| entry.original_index)
            .min();

        let male_payload_end =
            second_male_marker_index.unwrap_or(first_female_marker_index.unwrap_or(usize::MAX));
        let female_payload_start = match (first_female_marker_index, second_male_marker_index) {
            (Some(female), Some(second_male)) if female < second_male => female,
            (_, Some(second_male)) => second_male,
            (Some(female), None) => female,
            (None, None) => usize::MAX,
        };

        let mut male_marker = None;
        let mut female_marker = None;
        let mut male_payload = Vec::new();
        let mut female_payload = Vec::new();

        for indexed in entries {
            let index = indexed.original_index;
            let sig = indexed.entry.sig;
            if sig == male_marker_sig {
                if index == first_male_marker_index && male_marker.is_none() {
                    male_marker = Some(indexed.entry);
                }
                continue;
            }
            if sig == female_marker_sig {
                if Some(index) == first_female_marker_index && female_marker.is_none() {
                    female_marker = Some(indexed.entry);
                }
                continue;
            }

            if index > first_male_marker_index && index < male_payload_end {
                male_payload.push(indexed.entry);
            } else if index > female_payload_start {
                female_payload.push(indexed.entry);
            }
        }

        if let Some(entry) = male_marker {
            self.push_normalized_entry(entry, &male_segment[0], ordered);
        }
        for entry in male_payload {
            if let Some(target_subrecord_def) = segment_def_for_sig(male_segment, entry.sig) {
                self.push_normalized_entry(entry, target_subrecord_def, ordered);
            }
        }
        if let Some(entry) = female_marker {
            self.push_normalized_entry(entry, &female_segment[0], ordered);
        }
        for entry in female_payload {
            if let Some(target_subrecord_def) = segment_def_for_sig(female_segment, entry.sig) {
                self.push_normalized_entry(entry, target_subrecord_def, ordered);
            }
        }
    }

    fn emit_scen_phases_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(start_def) = segment.first() else {
            return;
        };
        let Some(end_index) = segment.iter().rposition(|def| def.id == "HNAM") else {
            self.emit_scoped_segment(segment, fields_by_sig, ordered);
            return;
        };
        if end_index == 0 {
            self.emit_scoped_segment(segment, fields_by_sig, ordered);
            return;
        }
        let end_def = &segment[end_index];
        let Ok(hnam_sig) = crate::ids::SubrecordSig::from_str("HNAM") else {
            return;
        };

        loop {
            let Some(start_entry) = fields_by_sig
                .get_mut(&hnam_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            let range_start = start_entry.original_index.saturating_add(1);
            let range_end = fields_by_sig
                .get(&hnam_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
                .unwrap_or(usize::MAX);

            self.push_normalized_entry(start_entry.entry, start_def, ordered);

            for (entry, target_subrecord_def) in pop_all_segment_entries_in_original_range(
                fields_by_sig,
                &segment[1..end_index],
                range_start,
                range_end,
            ) {
                self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
            }

            if range_end != usize::MAX {
                if let Some(end_entry) = pop_first_in_original_range(
                    fields_by_sig,
                    hnam_sig,
                    range_end,
                    range_end.saturating_add(1),
                ) {
                    self.push_normalized_entry(end_entry.entry, end_def, ordered);
                }
            }
        }
    }

    fn emit_scen_actors_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };
        let action_start = crate::ids::SubrecordSig::from_str("ANAM")
            .ok()
            .and_then(|sig| first_index_for_sig(fields_by_sig, sig))
            .unwrap_or(usize::MAX);

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            if anchor_index >= action_start {
                break;
            }
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            let row_end = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
                .unwrap_or(usize::MAX)
                .min(action_start);
            let row_start = anchor_index.saturating_add(1);

            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);
            self.emit_one_each_segment_entries_in_original_range(
                &segment[1..],
                fields_by_sig,
                ordered,
                row_start,
                row_end,
            );
        }
    }

    fn emit_scen_actions_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            if is_empty_marker_entry(&anchor_entry.entry) {
                self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);
                continue;
            }
            let row_end = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
                .unwrap_or(usize::MAX);
            let row_start = anchor_index.saturating_add(1);

            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);
            self.emit_segment_body_in_source_order(
                &segment[1..],
                fields_by_sig,
                ordered,
                row_start,
                row_end,
            );
        }
    }

    /// Emit the body subrecords of a scoped row preserving their original
    /// source order, dropping only sigs absent from the segment membership.
    ///
    /// The flat target schema lists several sigs (HTID, DATA, SNAM, ONAM,
    /// PNAM, VENC) twice in the SCEN actions scope at different positions; a
    /// member-list-order walk binds each source field to the first matching
    /// slot and re-sorts the body (e.g. the radio action's `DATA HTID DMAX`
    /// becomes `DATA DMAX HTID`), which xEdit rejects as out-of-order. The
    /// FO76 source already emits these in FO4-conformant order, so preserving
    /// source order is correct and dodges the duplicate-sig ambiguity.
    fn emit_segment_body_in_source_order(
        &self,
        defs: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        range_start: usize,
        range_end: usize,
    ) {
        let mut body: Vec<(usize, FieldEntry)> = Vec::new();
        let mut def_for_sig: HashMap<crate::ids::SubrecordSig, &SubrecordDef> = HashMap::new();
        for target_subrecord_def in defs {
            let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                continue;
            };
            def_for_sig.entry(sig).or_insert(target_subrecord_def);
            for entry in pop_all_in_original_range(fields_by_sig, sig, range_start, range_end) {
                body.push((entry.original_index, entry.entry));
            }
        }
        body.sort_by_key(|(index, _)| *index);
        for (_, entry) in body {
            if let Some(target_subrecord_def) = def_for_sig.get(&entry.sig).copied() {
                self.push_normalized_entry(entry, target_subrecord_def, ordered);
            }
        }
    }

    fn emit_destructible_segment(
        &self,
        defs: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let mut body = Vec::new();
        let mut def_for_sig: HashMap<crate::ids::SubrecordSig, &SubrecordDef> = HashMap::new();
        for target_subrecord_def in defs {
            let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                continue;
            };
            def_for_sig.entry(sig).or_insert(target_subrecord_def);
            body.extend(pop_all_in_original_range(fields_by_sig, sig, 0, usize::MAX));
        }
        body.sort_by_key(|entry| entry.original_index);

        let mut active_stage = false;
        let mut stage_has_model = false;
        let mut discard_model_row = false;
        for entry in body {
            let keep = match entry.entry.sig.as_str() {
                "DSTD" => {
                    active_stage = true;
                    stage_has_model = false;
                    discard_model_row = false;
                    true
                }
                "DSTF" => {
                    active_stage = false;
                    stage_has_model = false;
                    discard_model_row = false;
                    true
                }
                "DMDL" if active_stage => {
                    if stage_has_model {
                        discard_model_row = true;
                        false
                    } else {
                        stage_has_model = true;
                        discard_model_row = false;
                        true
                    }
                }
                "DMDT" | "DMDC" | "DMDS" if active_stage && discard_model_row => false,
                _ => true,
            };
            if keep {
                if let Some(target_subrecord_def) = def_for_sig.get(&entry.entry.sig).copied() {
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            }
        }
    }

    fn emit_scoped_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };

        loop {
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);

            for target_subrecord_def in &segment[1..] {
                let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                    continue;
                };
                if let Some(entry) = fields_by_sig.get_mut(&sig).and_then(VecDeque::pop_front) {
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            }
        }
    }

    fn emit_effects_segment(
        &self,
        target_record_def: &RecordDef,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };
        let effect_member_defs = segment
            .iter()
            .skip(1)
            .filter(|def| !matches!(def.id.as_str(), "CTDA" | "CTDT" | "CIS1" | "CIS2"))
            .collect::<Vec<_>>();
        let condition_defs = ["CTDA", "CTDT", "CIS1", "CIS2"]
            .into_iter()
            .filter_map(|condition_sig| {
                target_record_def
                    .subrecords
                    .iter()
                    .find(|def| def.id == condition_sig)
            })
            .collect::<Vec<_>>();

        loop {
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            let mut condition_start = anchor_entry.original_index;
            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);

            for target_subrecord_def in &effect_member_defs {
                let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                    continue;
                };
                if let Some(entry) = fields_by_sig.get_mut(&sig).and_then(VecDeque::pop_front) {
                    condition_start = condition_start.max(entry.original_index);
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            }

            let next_anchor = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index);
            let next_effect_member = effect_member_defs.iter().find_map(|def| {
                let sig = crate::ids::SubrecordSig::from_str(&def.id).ok()?;
                fields_by_sig
                    .get(&sig)
                    .and_then(VecDeque::front)
                    .map(|entry| entry.original_index)
            });
            let condition_end = next_anchor
                .filter(|next| *next > condition_start)
                .or(next_effect_member)
                .unwrap_or(usize::MAX);
            let mut conditions = Vec::new();
            for target_subrecord_def in &condition_defs {
                let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                    continue;
                };
                for entry in pop_all_in_original_range(
                    fields_by_sig,
                    sig,
                    condition_start.saturating_add(1),
                    condition_end,
                ) {
                    conditions.push((entry, *target_subrecord_def));
                }
            }
            conditions.sort_by_key(|(entry, _)| entry.original_index);
            for (entry, target_subrecord_def) in conditions {
                self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
            }
        }
    }

    /// Emit the QUST `aliases` scope.
    ///
    /// FO4's QUST alias scope is a sequence of variable-length alias structs
    /// keyed by one of three anchor sigs: ALST (reference alias), ALLS (location
    /// alias), or ALCS (collection alias). The flat schema segment lists all
    /// three variants back to back, so many child sigs (ALID, FNAM, ALFA, KNAM,
    /// CTDA, ...) appear in more than one variant.
    ///
    /// FO76 emits these alias structs in an order, and with intra-struct child
    /// orders, that FO4's forward-only xEdit cursor rejects.
    ///
    /// This walks the source alias section anchor-by-anchor: each anchor starts
    /// a new alias row that runs until the next anchor. Within a row the matching
    /// variant template (the segment slice from this anchor up to the next
    /// anchor def) drives child ordering; conditions (CTDA + trailing CIS1/CIS2)
    /// are emitted in source order. Any source sig absent from the matched
    /// variant template (FO76-only) is dropped.
    fn emit_qust_alias_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let variants = split_alias_variant_templates(segment);
        if variants.is_empty() {
            return;
        }
        let anchor_sigs: Vec<crate::ids::SubrecordSig> =
            variants.iter().map(|variant| variant.anchor_sig).collect();

        strip_illegal_reference_alias_knam_in_place(fields_by_sig, &anchor_sigs);

        // The set of valid alias IDs for this QUST: the uint32 value of every
        // ALST/ALLS/ALCS anchor. ALLA "Linked Alias" elements reference an alias
        // by this ID; FO76 stores ALLA arrays that point at phantom IDs which were
        // never defined as aliases (byte-identical in FO76 source), so xEdit
        // reports "Quest Alias [N] not found". Strip those dangling elements up
        // front so BOTH the per-row walk and the degenerate fallback drop them.
        let alias_ids = collect_alias_ids(&anchor_sigs, fields_by_sig);
        strip_dangling_alla_links_in_place(fields_by_sig, &alias_ids);

        // Degenerate layout where every alias child sig is grouped after all the
        // anchor markers (no child interleaved between two anchors). The per-row
        // ranges would all collapse, so fall back to the schema-order round-robin
        // that binds one child of each sig per anchor.
        if scoped_children_are_grouped_after_anchors(segment, fields_by_sig, anchor_sigs[0]) {
            self.emit_scoped_segment(segment, fields_by_sig, ordered);
            return;
        }

        loop {
            // The next alias row starts at the earliest remaining anchor entry.
            let Some((variant_index, anchor_index)) = anchor_sigs
                .iter()
                .enumerate()
                .filter_map(|(variant_index, sig)| {
                    fields_by_sig
                        .get(sig)
                        .and_then(VecDeque::front)
                        .map(|entry| (variant_index, entry.original_index))
                })
                .min_by_key(|(_, original_index)| *original_index)
            else {
                break;
            };
            let variant = &variants[variant_index];

            let row_start = anchor_index.saturating_add(1);
            // The row ends at the next anchor of any variant. The current anchor
            // is still queued (popped below), so scan past `anchor_index` rather
            // than only inspecting each deque's front.
            let row_end = anchor_sigs
                .iter()
                .filter_map(|sig| {
                    fields_by_sig.get(sig).and_then(|entries| {
                        entries
                            .iter()
                            .map(|entry| entry.original_index)
                            .find(|index| *index > anchor_index)
                    })
                })
                .min()
                .unwrap_or(usize::MAX);

            let Some(anchor_entry) = fields_by_sig
                .get_mut(&variant.anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            self.push_normalized_entry(anchor_entry.entry, variant.anchor_def, ordered);
            self.emit_qust_alias_row_body(variant, fields_by_sig, ordered, row_start, row_end);
        }
    }

    fn emit_qust_objectives_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };
        if fields_by_sig
            .get(&anchor_sig)
            .and_then(VecDeque::front)
            .is_some()
        {
            self.emit_qust_objective_rows(segment, fields_by_sig, ordered, anchor_def, anchor_sig);
            return;
        }

        let Some(target_subrecord_def) = segment.iter().find(|def| def.id == "QSTA") else {
            return;
        };
        let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
            return;
        };
        let end = first_qust_orphan_objective_boundary(fields_by_sig);
        for entry in pop_all_in_original_range(fields_by_sig, sig, 0, end) {
            self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
        }
    }

    fn emit_qust_objective_rows(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        anchor_def: &SubrecordDef,
        anchor_sig: crate::ids::SubrecordSig,
    ) {
        let qsta_index = segment
            .iter()
            .position(|def| def.id == "QSTA")
            .unwrap_or(segment.len());
        let top_defs = &segment[1..qsta_index];
        let target_defs = &segment[qsta_index..];
        let qsta_sig = crate::ids::SubrecordSig::from_str("QSTA").ok();

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            let row_end = first_qust_objective_boundary_after(fields_by_sig, anchor_index);
            let row_start = anchor_index.saturating_add(1);
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);
            self.emit_segment_entries_by_schema(
                top_defs,
                fields_by_sig,
                ordered,
                row_start,
                row_end,
            );

            let Some(qsta_sig) = qsta_sig else {
                continue;
            };
            let Some(qsta_def) = target_defs.first() else {
                continue;
            };
            loop {
                let Some(target_index) = fields_by_sig.get(&qsta_sig).and_then(|entries| {
                    entries
                        .iter()
                        .find(|entry| {
                            entry.original_index >= row_start && entry.original_index < row_end
                        })
                        .map(|entry| entry.original_index)
                }) else {
                    break;
                };
                let Some(qsta_entry) =
                    pop_first_in_original_range(fields_by_sig, qsta_sig, target_index, row_end)
                else {
                    break;
                };
                self.push_normalized_entry(qsta_entry.entry, qsta_def, ordered);
                let target_end = first_index_after(fields_by_sig, &[qsta_sig], target_index)
                    .filter(|index| *index < row_end)
                    .unwrap_or(row_end);
                self.emit_condition_entries_in_original_range(
                    target_defs,
                    fields_by_sig,
                    ordered,
                    target_index.saturating_add(1),
                    target_end,
                );
            }
        }
    }

    fn emit_qust_stages_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            let row_end = first_index_after(fields_by_sig, &[anchor_sig], anchor_index)
                .into_iter()
                .chain(first_qust_alias_anchor_after(fields_by_sig, anchor_index))
                .min()
                .unwrap_or(usize::MAX);
            let row_start = anchor_index.saturating_add(1);
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);

            self.emit_qust_stage_log_entries(segment, fields_by_sig, ordered, row_start, row_end);
        }
    }

    fn emit_qust_stage_log_entries(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        row_start: usize,
        row_end: usize,
    ) {
        let Some(log_entry_def) = segment.iter().find(|def| def.id == "QSDT") else {
            return;
        };
        let Ok(log_entry_sig) = crate::ids::SubrecordSig::from_str("QSDT") else {
            return;
        };
        let Some(log_body_start) = segment
            .iter()
            .position(|def| def.id == "QSDT")
            .map(|i| i + 1)
        else {
            return;
        };
        let log_body_defs = &segment[log_body_start..];

        loop {
            let Some(log_entry_index) = fields_by_sig.get(&log_entry_sig).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| {
                        entry.original_index >= row_start && entry.original_index < row_end
                    })
                    .map(|entry| entry.original_index)
            }) else {
                break;
            };
            let Some(log_entry) =
                pop_first_in_original_range(fields_by_sig, log_entry_sig, log_entry_index, row_end)
            else {
                break;
            };
            self.push_normalized_entry(log_entry.entry, log_entry_def, ordered);
            let log_end = first_index_after(fields_by_sig, &[log_entry_sig], log_entry_index)
                .filter(|index| *index < row_end)
                .unwrap_or(row_end);
            let log_start = log_entry_index.saturating_add(1);

            let mut emitted_conditions = false;
            for target_subrecord_def in log_body_defs {
                if matches!(
                    target_subrecord_def.id.as_str(),
                    "CTDA" | "CTDT" | "CIS1" | "CIS2"
                ) {
                    if !emitted_conditions {
                        self.emit_condition_entries_in_original_range(
                            log_body_defs,
                            fields_by_sig,
                            ordered,
                            log_start,
                            log_end,
                        );
                        emitted_conditions = true;
                    }
                    continue;
                }
                let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                    continue;
                };
                if target_subrecord_def.multiple {
                    for entry in pop_all_in_original_range(fields_by_sig, sig, log_start, log_end) {
                        self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                    }
                } else if let Some(entry) =
                    pop_first_in_original_range(fields_by_sig, sig, log_start, log_end)
                {
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            }
        }
    }

    fn emit_qust_alias_row_body(
        &self,
        variant: &AliasVariantTemplate<'_>,
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        row_start: usize,
        row_end: usize,
    ) {
        for target_subrecord_def in variant.child_defs {
            match target_subrecord_def.id.as_str() {
                // Conditions are emitted as a group in source order so each CTDA
                // keeps its trailing CIS1/CIS2 parameter strings. The first
                // CTDA/CIS1/CIS2 def in the template drives the whole group; the
                // later duplicate defs are skipped to avoid re-emitting.
                "CTDA" => self.emit_qust_alias_conditions_in_range(
                    variant.child_defs,
                    fields_by_sig,
                    ordered,
                    row_start,
                    row_end,
                ),
                "CIS1" | "CIS2" => {}
                _ => {
                    let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id)
                    else {
                        continue;
                    };
                    if target_subrecord_def.multiple {
                        for entry in
                            pop_all_in_original_range(fields_by_sig, sig, row_start, row_end)
                        {
                            self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                        }
                    } else if let Some(entry) =
                        pop_first_in_original_range(fields_by_sig, sig, row_start, row_end)
                    {
                        self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                    }
                }
            }
        }
    }

    fn emit_qust_alias_conditions_in_range(
        &self,
        child_defs: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        row_start: usize,
        row_end: usize,
    ) {
        let mut entries: Vec<(IndexedFieldEntry, &SubrecordDef)> = Vec::new();
        for target_subrecord_def in child_defs {
            if !matches!(target_subrecord_def.id.as_str(), "CTDA" | "CIS1" | "CIS2") {
                continue;
            }
            let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                continue;
            };
            for entry in pop_all_in_original_range(fields_by_sig, sig, row_start, row_end) {
                entries.push((entry, target_subrecord_def));
            }
        }
        entries.sort_by_key(|(entry, _)| entry.original_index);
        for (entry, target_subrecord_def) in entries {
            self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
        }
    }

    fn emit_original_range_scoped_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            let row_end = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
                .unwrap_or(usize::MAX);
            let row_end = condition_scope_row_end(segment, fields_by_sig, anchor_index, row_end);
            let row_start = anchor_index.saturating_add(1);

            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);

            for target_subrecord_def in &segment[1..] {
                let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                    continue;
                };
                if target_subrecord_def.multiple {
                    for entry in pop_all_in_original_range(fields_by_sig, sig, row_start, row_end) {
                        self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                    }
                } else if let Some(entry) =
                    pop_first_in_original_range(fields_by_sig, sig, row_start, row_end)
                {
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            }
        }
    }

    fn emit_regn_region_data_entries_segment(
        &self,
        segment: &[SubrecordDef],
        target_record_def: &RecordDef,
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };

        let mut regular_rows = Vec::new();
        let mut sound_rows = Vec::new();

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            let row_end = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
                .unwrap_or(usize::MAX);
            let row_start = anchor_index.saturating_add(1);

            let mut row = Vec::new();
            self.push_normalized_entry(anchor_entry.entry, anchor_def, &mut row);
            self.emit_segment_body_in_source_order(
                &segment[1..],
                fields_by_sig,
                &mut row,
                row_start,
                row_end,
            );
            if let Some(rdsa_def) = target_record_def.subrecord_def("RDSA") {
                let Ok(rdsa_sig) = crate::ids::SubrecordSig::from_str("RDSA") else {
                    continue;
                };
                for entry in pop_all_in_original_range(fields_by_sig, rdsa_sig, row_start, row_end)
                {
                    self.push_normalized_entry(entry.entry, rdsa_def, &mut row);
                }
            }

            if row.iter().any(|entry| entry.sig.as_str() == "RDSA") {
                sound_rows.push(row);
            } else {
                regular_rows.push(row);
            }
        }

        for row in regular_rows.into_iter().chain(sound_rows) {
            ordered.extend(row);
        }
    }

    fn emit_one_each_segment_entries_in_original_range(
        &self,
        defs: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        range_start: usize,
        range_end: usize,
    ) {
        for target_subrecord_def in defs {
            let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                continue;
            };
            if let Some(entry) =
                pop_first_in_original_range(fields_by_sig, sig, range_start, range_end)
            {
                self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
            }
        }
    }

    fn emit_condition_string_scoped_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };
        let condition_start = segment
            .iter()
            .position(|def| matches!(def.id.as_str(), "CTDA" | "CTDT"))
            .unwrap_or(segment.len());

        // The last anchor of a scoped condition group would otherwise sweep
        // every trailing condition up to `usize::MAX` (see `row_end` below).
        // For the `body_text` scope that swallows the menu-item conditions that
        // live after the `ISIZ` count: the last body text steals every item's
        // CTDA and the menu items convert with no conditions at all (their menu
        // conditions "moved to the body"). Bound the `body_text` scope at the
        // start of the menu-item region (ISIZ, or the first ITXT) so item
        // conditions stay with their items.
        let scope_end_bound = if anchor_def.scope_id.as_deref() == Some("body_text") {
            ["ISIZ", "ITXT"]
                .iter()
                .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
                .find_map(|sig| first_index_for_sig(fields_by_sig, sig))
                .unwrap_or(usize::MAX)
        } else {
            usize::MAX
        };

        self.emit_condition_entries_before_first_anchor(
            segment,
            fields_by_sig,
            ordered,
            anchor_sig,
            condition_start,
        );

        loop {
            let Some(anchor_index) = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
            else {
                break;
            };
            let Some(anchor_entry) = fields_by_sig
                .get_mut(&anchor_sig)
                .and_then(VecDeque::pop_front)
            else {
                break;
            };
            let row_end = fields_by_sig
                .get(&anchor_sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
                .unwrap_or(scope_end_bound)
                .min(scope_end_bound);
            let row_start = anchor_index.saturating_add(1);

            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);
            self.emit_segment_entries_by_schema(
                &segment[1..condition_start],
                fields_by_sig,
                ordered,
                row_start,
                row_end,
            );
            self.emit_condition_entries_in_original_range(
                segment,
                fields_by_sig,
                ordered,
                row_start,
                row_end,
            );
            self.emit_segment_entries_by_schema(
                &segment[condition_start..],
                fields_by_sig,
                ordered,
                row_start,
                row_end,
            );
        }
    }

    fn emit_condition_entries_before_first_anchor(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        anchor_sig: crate::ids::SubrecordSig,
        condition_start: usize,
    ) {
        if segment.first().and_then(|def| def.scope_id.as_deref()) != Some("menu_items") {
            return;
        }
        let Some(first_anchor_index) = fields_by_sig
            .get(&anchor_sig)
            .and_then(VecDeque::front)
            .map(|entry| entry.original_index)
        else {
            return;
        };
        self.emit_condition_entries_in_original_range(
            segment,
            fields_by_sig,
            ordered,
            0,
            first_anchor_index,
        );
        self.emit_segment_entries_by_schema(
            &segment[condition_start..],
            fields_by_sig,
            ordered,
            0,
            first_anchor_index,
        );
    }

    fn emit_segment_entries_by_schema(
        &self,
        defs: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        range_start: usize,
        range_end: usize,
    ) {
        for target_subrecord_def in defs {
            if matches!(
                target_subrecord_def.id.as_str(),
                "CTDA" | "CTDT" | "CIS1" | "CIS2"
            ) {
                continue;
            }
            let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                continue;
            };
            if target_subrecord_def.multiple {
                for entry in pop_all_in_original_range(fields_by_sig, sig, range_start, range_end) {
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            } else if let Some(entry) =
                pop_first_in_original_range(fields_by_sig, sig, range_start, range_end)
            {
                self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
            }
        }
    }

    fn emit_condition_entries_in_original_range(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
        range_start: usize,
        range_end: usize,
    ) {
        let condition_defs = condition_string_defs(segment);
        let mut entries = Vec::new();
        for (sig, target_subrecord_def) in condition_defs {
            for entry in pop_all_in_original_range(fields_by_sig, sig, range_start, range_end) {
                entries.push((entry, target_subrecord_def));
            }
        }
        entries.sort_by_key(|(entry, _)| entry.original_index);
        for (entry, target_subrecord_def) in entries {
            self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
        }
    }

    fn emit_qust_dialogue_conditions_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let end = first_qust_dialogue_conditions_boundary(fields_by_sig);
        self.emit_condition_entries_in_original_range(segment, fields_by_sig, ordered, 0, end);
    }

    fn emit_qust_story_manager_conditions_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let start = qust_story_manager_conditions_start(fields_by_sig);
        let end = first_qust_story_manager_conditions_boundary(fields_by_sig);
        if start < end {
            self.emit_condition_entries_in_original_range(
                segment,
                fields_by_sig,
                ordered,
                start,
                end,
            );
        }
    }

    fn emit_pack_package_data_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let package_end_index = crate::ids::SubrecordSig::from_str("XNAM")
            .ok()
            .and_then(|xnam_sig| {
                fields_by_sig
                    .get(&xnam_sig)
                    .and_then(VecDeque::front)
                    .map(|entry| entry.original_index)
            })
            .unwrap_or(usize::MAX);

        let Some(package_start_index) =
            first_segment_entry_index_before(fields_by_sig, segment, package_end_index)
        else {
            return;
        };
        for (entry, target_subrecord_def) in pop_all_segment_entries_in_original_range(
            fields_by_sig,
            segment,
            package_start_index,
            package_end_index,
        ) {
            self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
        }
    }

    fn emit_pack_procedure_tree_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(start) = first_segment_entry_index_before(fields_by_sig, segment, usize::MAX)
        else {
            return;
        };
        let end = first_pack_procedure_tree_boundary_after(fields_by_sig, start);
        for (entry, target_subrecord_def) in
            pop_all_segment_entries_in_original_range(fields_by_sig, segment, start, end)
        {
            self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
        }
    }

    fn emit_object_template_segment(
        &self,
        segment: &[SubrecordDef],
        fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let Some(anchor_def) = segment.first() else {
            return;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            return;
        };
        let Some(obts_def) = segment.iter().find(|def| def.id == "OBTS") else {
            self.emit_scoped_segment(segment, fields_by_sig, ordered);
            return;
        };
        let Ok(obts_sig) = crate::ids::SubrecordSig::from_str("OBTS") else {
            return;
        };
        if fields_by_sig
            .get(&obts_sig)
            .map(VecDeque::is_empty)
            .unwrap_or(true)
        {
            self.emit_scoped_segment(segment, fields_by_sig, ordered);
            return;
        }

        while let Some(mut anchor_entry) = fields_by_sig
            .get_mut(&anchor_sig)
            .and_then(VecDeque::pop_front)
        {
            let template_count = fields_by_sig.get(&obts_sig).map(VecDeque::len).unwrap_or(0);
            set_u32_count(&mut anchor_entry.entry.value, template_count as u32);
            self.push_normalized_entry(anchor_entry.entry, anchor_def, ordered);

            for _ in 0..template_count {
                for target_subrecord_def in
                    segment.iter().skip(1).take_while(|def| def.id != "OBTS")
                {
                    let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id)
                    else {
                        continue;
                    };
                    if let Some(entry) = fields_by_sig.get_mut(&sig).and_then(VecDeque::pop_front) {
                        self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                    }
                }

                if let Some(entry) = fields_by_sig
                    .get_mut(&obts_sig)
                    .and_then(VecDeque::pop_front)
                {
                    self.push_normalized_entry(entry.entry, obts_def, ordered);
                }
            }

            for target_subrecord_def in segment.iter().skip_while(|def| def.id != "STOP") {
                let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
                    continue;
                };
                while let Some(entry) = fields_by_sig.get_mut(&sig).and_then(VecDeque::pop_front) {
                    self.push_normalized_entry(entry.entry, target_subrecord_def, ordered);
                }
            }
        }
    }

    fn push_normalized_entry(
        &self,
        entry: FieldEntry,
        target_subrecord_def: &SubrecordDef,
        ordered: &mut Vec<FieldEntry>,
    ) {
        let entry = self.normalize_subrecord_entry(entry, target_subrecord_def);
        if !drop_fo76_qust_null_alias_display_name(
            &entry,
            self.source_record_def,
            target_subrecord_def,
        ) && !drop_empty_optional_fixed_subrecord(&entry, target_subrecord_def)
        {
            ordered.push(entry);
        }
    }

    fn normalize_subrecord_entry(
        &self,
        entry: FieldEntry,
        target_subrecord_def: &SubrecordDef,
    ) -> FieldEntry {
        let source_subrecord_def = self
            .source_record_def
            .and_then(|record_def| record_def.subrecord_def(entry.sig.as_str()));
        self.normalize_subrecord_entry_with_source_def(
            entry,
            source_subrecord_def,
            target_subrecord_def,
        )
    }

    fn normalize_subrecord_entry_with_source_def(
        &self,
        entry: FieldEntry,
        source_subrecord_def: Option<&SubrecordDef>,
        target_subrecord_def: &SubrecordDef,
    ) -> FieldEntry {
        let entry = normalize_legacy_gamebryo_subrecord_layout(
            entry,
            self.source_record_def,
            source_subrecord_def,
            self.target_schema.record_def(
                self.source_record_def
                    .map_or("", |record_def| record_def.id.as_str()),
            ),
            target_subrecord_def,
            self.interner,
        );
        normalize_field_entry(
            entry,
            source_subrecord_def,
            target_subrecord_def,
            self.interner,
        )
    }
}

fn normalize_translated_race_subgraph_paths(
    record: &mut Record,
    source_record_def: Option<&RecordDef>,
    target_record_def: &RecordDef,
    interner: Option<&StringInterner>,
) {
    if source_record_def.is_none() || target_record_def.id != "RACE" {
        return;
    }
    let Some(interner) = interner else {
        return;
    };

    for entry in &mut record.fields {
        if !matches!(entry.sig.as_str(), "SGNM" | "SAPT") {
            continue;
        }
        let FieldValue::String(sym) = entry.value else {
            continue;
        };
        let Some(path) = interner.resolve(sym) else {
            continue;
        };
        let normalized =
            path.trim_end_matches(|ch: char| ch.is_ascii_whitespace() || ch.is_ascii_control());
        if normalized != path {
            entry.value = FieldValue::String(interner.intern(normalized));
        }
    }
}

fn strip_generated_additive_race_tint_tables(
    record: &mut Record,
    interner: Option<&StringInterner>,
) {
    if record.sig.0 != *b"RACE"
        || !record.fields.iter().any(|entry| entry.sig.0 == *b"SADD")
        || !record.fields.iter().any(|entry| {
            if entry.sig.0 != *b"EDID" {
                return false;
            }
            match &entry.value {
                FieldValue::String(value) => interner
                    .and_then(|interner| interner.resolve(*value))
                    .is_some_and(is_generated_additive_race_editor_id),
                FieldValue::Bytes(value) => {
                    std::str::from_utf8(value).is_ok_and(is_generated_additive_race_editor_id)
                }
                _ => false,
            }
        })
    {
        return;
    }
    strip_unsupported_race_tint_tables(record);
}

fn is_generated_additive_race_editor_id(editor_id: &str) -> bool {
    let editor_id = editor_id.trim_matches('\0').to_ascii_lowercase();
    editor_id.contains("raceadditive") || editor_id.contains("race_additive")
}

pub(crate) fn strip_unsupported_race_tint_tables(record: &mut Record) {
    if record.sig.0 != *b"RACE" {
        return;
    }

    let mut in_tint_row = false;
    record.fields.retain(|entry| match &entry.sig.0 {
        b"TINL" => false,
        b"TTGP" | b"TETI" | b"TTEF" | b"TTET" | b"TTEB" | b"TTEC" | b"TTED" | b"TTGE" => {
            in_tint_row = true;
            false
        }
        b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2" if in_tint_row => false,
        _ => {
            in_tint_row = false;
            true
        }
    });
}

fn normalize_legacy_gamebryo_subrecord_layout(
    mut entry: FieldEntry,
    source_record_def: Option<&RecordDef>,
    source_subrecord_def: Option<&SubrecordDef>,
    target_record_def: Option<&RecordDef>,
    target_subrecord_def: &SubrecordDef,
    interner: Option<&StringInterner>,
) -> FieldEntry {
    let (Some(source_record_def), Some(source_subrecord_def), Some(target_record_def)) =
        (source_record_def, source_subrecord_def, target_record_def)
    else {
        return entry;
    };
    if source_record_def.id != target_record_def.id {
        return entry;
    }
    let layout = (
        source_record_def.id.as_str(),
        target_subrecord_def.id.as_str(),
        source_subrecord_def.codec.as_deref(),
        target_subrecord_def.codec.as_deref(),
    );

    entry.value = match (layout, entry.value) {
        (("REFR" | "ACHR", "XLKR", Some("formid"), Some("struct:I,I")), FieldValue::Bytes(raw))
            if raw.len() == 4 =>
        {
            let mut bytes = Vec::with_capacity(8);
            bytes.extend_from_slice(&0_u32.to_le_bytes());
            bytes.extend_from_slice(&raw);
            FieldValue::Bytes(SmallVec::from_vec(bytes))
        }
        (
            ("REFR" | "ACHR", "XLKR", Some("formid"), Some("struct:I,I")),
            FieldValue::FormKey(reference),
        ) if interner.is_some() => FieldValue::Struct(vec![
            (interner.unwrap().intern("keyword_ref"), FieldValue::Uint(0)),
            (
                interner.unwrap().intern("ref"),
                FieldValue::FormKey(reference),
            ),
        ]),
        (
            ("REFR" | "PGRE" | "CELL", "XOWN", Some("formid"), Some("struct:I,B,B,B,B,B,B,B,B")),
            FieldValue::Bytes(raw),
        ) if matches!(raw.len(), 4 | 5) => {
            let mut bytes = vec![0_u8; 12];
            bytes[..4].copy_from_slice(&raw[..4]);
            FieldValue::Bytes(SmallVec::from_vec(bytes))
        }
        (
            ("REFR" | "PGRE" | "CELL", "XOWN", Some("formid"), Some("struct:I,B,B,B,B,B,B,B,B")),
            FieldValue::FormKey(owner),
        ) if interner.is_some() => FieldValue::Struct(
            target_subrecord_def
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    (
                        interner.unwrap().intern(&field.id),
                        if index == 0 {
                            FieldValue::FormKey(owner)
                        } else {
                            FieldValue::Uint(0)
                        },
                    )
                })
                .collect(),
        ),
        (
            ("REFR", "XPRM", Some("struct:f,f,f,f,f,f,f,I"), Some("struct:f,f,f,f,f,f,f,I")),
            FieldValue::Bytes(raw),
        ) if raw.len() == 16 => {
            let mut bytes = Vec::with_capacity(32);
            bytes.extend_from_slice(&raw[..12]);
            for _ in 0..4 {
                bytes.extend_from_slice(&1.0_f32.to_le_bytes());
            }
            bytes.extend_from_slice(&raw[12..16]);
            FieldValue::Bytes(SmallVec::from_vec(bytes))
        }
        (("NOTE", "DATA", Some("uint8"), Some("struct:I,f")), FieldValue::Bytes(raw))
            if raw.len() == 4 =>
        {
            // FNV/FO3 DATA is a one-byte note type in a padded four-byte payload.
            FieldValue::Bytes(SmallVec::from_slice(&[raw[0], 0, 0, 0, 0, 0, 0, 0]))
        }
        (("NOTE", "DATA", Some("uint8"), Some("struct:I,f")), FieldValue::Uint(note_type))
            if interner.is_some() =>
        {
            FieldValue::Struct(vec![
                (
                    interner.unwrap().intern("value"),
                    FieldValue::Uint(note_type & 0xFF),
                ),
                (interner.unwrap().intern("weight"), FieldValue::Float(0.0)),
            ])
        }
        (_, value) => value,
    };
    entry
}

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum ScolStaticKey {
    Raw(u32),
    FormKey(crate::ids::FormKey),
}

fn coalesce_scol_duplicate_static_groups(record: &mut Record) {
    if record.sig.as_str() != "SCOL" {
        return;
    }
    let Ok(onam_sig) = crate::ids::SubrecordSig::from_str("ONAM") else {
        return;
    };
    let Ok(data_sig) = crate::ids::SubrecordSig::from_str("DATA") else {
        return;
    };

    let mut output = Vec::with_capacity(record.fields.len());
    let mut data_index_by_static: HashMap<ScolStaticKey, usize> = HashMap::new();
    let fields = record.fields.drain(..).collect::<Vec<_>>();
    let mut iter = fields.into_iter().peekable();

    while let Some(onam_entry) = iter.next() {
        if onam_entry.sig != onam_sig {
            output.push(onam_entry);
            continue;
        }

        let key = scol_static_key(&onam_entry.value);
        let data_entry = if iter.peek().is_some_and(|entry| entry.sig == data_sig) {
            iter.next()
        } else {
            None
        };

        let Some(key) = key else {
            output.push(onam_entry);
            if let Some(data_entry) = data_entry {
                output.push(data_entry);
            }
            continue;
        };

        if let Some(existing_data_index) = data_index_by_static.get(&key).copied() {
            if let Some(data_entry) = data_entry {
                append_scol_data(&mut output[existing_data_index].value, data_entry.value);
            }
            continue;
        }

        output.push(onam_entry);
        if let Some(data_entry) = data_entry {
            let data_index = output.len();
            output.push(data_entry);
            data_index_by_static.insert(key, data_index);
        }
    }

    record.fields = SmallVec::from_vec(output);
}

fn scol_static_key(value: &FieldValue) -> Option<ScolStaticKey> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => Some(ScolStaticKey::Raw(
            u32::from_le_bytes(bytes[..4].try_into().ok()?),
        )),
        FieldValue::Uint(value) => Some(ScolStaticKey::Raw(*value as u32)),
        FieldValue::Int(value) if *value >= 0 => Some(ScolStaticKey::Raw(*value as u32)),
        FieldValue::FormKey(form_key) => Some(ScolStaticKey::FormKey(*form_key)),
        _ => None,
    }
}

fn append_scol_data(target: &mut FieldValue, source: FieldValue) {
    match (target, source) {
        (FieldValue::Bytes(target), FieldValue::Bytes(source)) => {
            target.extend_from_slice(source.as_slice());
        }
        (FieldValue::List(target), FieldValue::List(mut source)) => {
            target.append(&mut source);
        }
        _ => {}
    }
}

fn drop_empty_imad_runtime_unsafe_subrecords(record: &mut Record) {
    if record.sig.as_str() != "IMAD" {
        return;
    }

    record
        .fields
        .retain(|entry| !is_empty_imad_runtime_unsafe_subrecord(entry));
}

fn is_empty_imad_runtime_unsafe_subrecord(entry: &FieldEntry) -> bool {
    matches!(entry.sig.as_str(), "NAM5" | "NAM6") && is_empty_marker_entry(entry)
}

fn drop_empty_optional_fixed_subrecord(entry: &FieldEntry, target_def: &SubrecordDef) -> bool {
    if target_def.required || fixed_size_hint(None, target_def).is_none() {
        return false;
    }
    // A null INAM (IDLE FormID) in a PACK on-begin/end/change branch is a
    // structural procedure-tree marker, not an optional payload: FO4 reads each
    // POBA/POEA/POCA branch as a fixed INAM+PDTO pair and a missing INAM yields
    // "missing procedure tree item" / "invalid chunk ID". Keep it (encodes to a
    // zeroed FormID). FO76 carries a null INAM on ~all packages; without this
    // they all decode to None here and get dropped.
    if entry.sig.as_str() == "INAM"
        && matches!(
            target_def.scope_id.as_deref(),
            Some("onbegin") | Some("onend") | Some("onchange")
        )
    {
        return false;
    }

    matches!(entry.value, FieldValue::None)
}

fn drop_fo76_qust_null_alias_display_name(
    entry: &FieldEntry,
    source_record_def: Option<&RecordDef>,
    target_def: &SubrecordDef,
) -> bool {
    if target_def.id != "ALDN"
        || target_def.scope_id.as_deref() != Some("aliases")
        || !is_fo76_qust_record_def(source_record_def)
    {
        return false;
    }

    match &entry.value {
        FieldValue::None => true,
        FieldValue::Bytes(bytes) => bytes.len() == 4 && bytes.iter().all(|byte| *byte == 0),
        FieldValue::FormKey(form_key) => form_key.local == 0,
        _ => false,
    }
}

fn is_fo76_qust_record_def(source_record_def: Option<&RecordDef>) -> bool {
    source_record_def.is_some_and(|record_def| {
        record_def.id == "QUST"
            && record_def.subrecord_def("QQSD").is_some()
            && record_def.subrecord_def("QTFS").is_some()
    })
}

/// The QUST alias-scope anchor sigs, in their FO4 schema order. Each starts a
/// distinct alias-struct variant (reference / location / collection alias).
const QUST_ALIAS_ANCHOR_SIGS: &[&str] = &["ALST", "ALLS", "ALCS"];

struct AliasVariantTemplate<'a> {
    anchor_sig: crate::ids::SubrecordSig,
    anchor_def: &'a SubrecordDef,
    child_defs: &'a [SubrecordDef],
}

/// Partition the flat QUST `aliases` segment into one template per alias-struct
/// variant. Each variant runs from its anchor def up to (but excluding) the next
/// anchor def, so each variant carries only the child sigs FO4 permits inside
/// that alias kind.
fn split_alias_variant_templates(segment: &[SubrecordDef]) -> Vec<AliasVariantTemplate<'_>> {
    let anchor_positions: Vec<usize> = segment
        .iter()
        .enumerate()
        .filter(|(_, def)| QUST_ALIAS_ANCHOR_SIGS.contains(&def.id.as_str()))
        .map(|(index, _)| index)
        .collect();

    let mut variants = Vec::with_capacity(anchor_positions.len());
    for (position, &start) in anchor_positions.iter().enumerate() {
        let end = anchor_positions
            .get(position + 1)
            .copied()
            .unwrap_or(segment.len());
        let anchor_def = &segment[start];
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            continue;
        };
        variants.push(AliasVariantTemplate {
            anchor_sig,
            anchor_def,
            child_defs: &segment[start + 1..end],
        });
    }
    variants
}

/// Gather the set of valid alias IDs for a QUST from its ALST/ALLS/ALCS anchors.
/// Each anchor decodes (codec `uint32`) to a `FieldValue::Uint`; an FO76 source
/// may also leave it as raw 4-byte `Bytes`.
fn collect_alias_ids(
    anchor_sigs: &[crate::ids::SubrecordSig],
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> HashSet<u32> {
    let mut ids = HashSet::new();
    for sig in anchor_sigs {
        let Some(entries) = fields_by_sig.get(sig) else {
            continue;
        };
        for indexed in entries {
            if let Some(id) = alias_id_value(&indexed.entry.value) {
                ids.insert(id);
            }
        }
    }
    ids
}

fn strip_illegal_reference_alias_knam_in_place(
    fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    anchor_sigs: &[crate::ids::SubrecordSig],
) {
    let Ok(alst_sig) = crate::ids::SubrecordSig::from_str("ALST") else {
        return;
    };
    let Ok(knam_sig) = crate::ids::SubrecordSig::from_str("KNAM") else {
        return;
    };

    let mut anchors = anchor_sigs
        .iter()
        .flat_map(|sig| {
            fields_by_sig
                .get(sig)
                .into_iter()
                .flatten()
                .map(move |entry| (entry.original_index, *sig))
        })
        .collect::<Vec<_>>();
    anchors.sort_by_key(|(index, _)| *index);

    let Some(entries) = fields_by_sig.get_mut(&knam_sig) else {
        return;
    };
    entries.retain(|entry| {
        anchors
            .iter()
            .rev()
            .find(|(index, _)| *index < entry.original_index)
            .is_none_or(|(_, anchor_sig)| *anchor_sig != alst_sig)
    });
}

fn alias_id_value(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Uint(v) => Some(*v as u32),
        FieldValue::Int(v) => Some(*v as u32),
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(u32::from_le_bytes(bytes[..4].try_into().ok()?))
        }
        _ => None,
    }
}

/// Strip dangling ALLA "Linked Alias" elements from every ALLA entry in the QUST
/// alias scope, dropping any ALLA subrecord that becomes empty. See
/// [`drop_dangling_alla_links`].
fn strip_dangling_alla_links_in_place(
    fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    alias_ids: &HashSet<u32>,
) {
    let Ok(alla_sig) = crate::ids::SubrecordSig::from_str("ALLA") else {
        return;
    };
    let Some(entries) = fields_by_sig.get_mut(&alla_sig) else {
        return;
    };
    entries.retain_mut(|indexed| !drop_dangling_alla_links(&mut indexed.entry.value, alias_ids));
}

/// Drop ALLA "Linked Alias" array elements (`array_struct:I,i`: KYWD FormID +
/// alias-index i32, 8 bytes each) whose alias index is not a valid alias ID of
/// the owning QUST. FO76 ships ALLA arrays referencing phantom alias indices that
/// were never defined (byte-identical in the FO76 source), which FO4 rejects with
/// xEdit "Quest Alias [N] not found". ALLA reaches normalization as raw `Bytes`
/// (the generic decoder leaves `array_struct:` codecs unparsed). Returns `true`
/// when the whole subrecord becomes empty (the caller drops it).
fn drop_dangling_alla_links(value: &mut FieldValue, alias_ids: &HashSet<u32>) -> bool {
    let FieldValue::Bytes(bytes) = value else {
        return false;
    };
    if bytes.len() < 8 {
        return bytes.is_empty();
    }
    let mut kept: SmallVec<[u8; 32]> = SmallVec::new();
    for element in bytes.chunks_exact(8) {
        let alias_index = u32::from_le_bytes([element[4], element[5], element[6], element[7]]);
        if alias_ids.contains(&alias_index) {
            kept.extend_from_slice(element);
        }
    }
    *bytes = kept;
    bytes.is_empty()
}

fn has_condition_string_rows(segment: &[SubrecordDef]) -> bool {
    segment
        .iter()
        .any(|def| matches!(def.id.as_str(), "CTDA" | "CTDT"))
        && segment
            .iter()
            .any(|def| matches!(def.id.as_str(), "CIS1" | "CIS2"))
}

fn is_condition_anchor_segment(segment: &[SubrecordDef]) -> bool {
    segment
        .first()
        .is_some_and(|def| matches!(def.id.as_str(), "CTDA" | "CTDT"))
        && segment
            .iter()
            .any(|def| matches!(def.id.as_str(), "CIS1" | "CIS2"))
}

fn condition_string_defs(
    segment: &[SubrecordDef],
) -> Vec<(crate::ids::SubrecordSig, &SubrecordDef)> {
    segment
        .iter()
        .filter(|def| matches!(def.id.as_str(), "CTDA" | "CTDT" | "CIS1" | "CIS2"))
        .filter_map(|def| {
            crate::ids::SubrecordSig::from_str(&def.id)
                .ok()
                .map(|sig| (sig, def))
        })
        .collect()
}

fn condition_scope_row_end(
    segment: &[SubrecordDef],
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    anchor_index: usize,
    fallback_end: usize,
) -> usize {
    if segment.first().and_then(|def| def.scope_id.as_deref()) != Some("body_text") {
        return fallback_end;
    }

    let mut row_end = fallback_end;
    for boundary in ["ISIZ", "ITXT"] {
        let Ok(sig) = crate::ids::SubrecordSig::from_str(boundary) else {
            continue;
        };
        if let Some(boundary_index) = fields_by_sig
            .get(&sig)
            .and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.original_index > anchor_index)
            })
            .map(|entry| entry.original_index)
        {
            row_end = row_end.min(boundary_index);
        }
    }
    row_end
}

fn first_qust_dialogue_conditions_boundary(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> usize {
    [
        "NEXT", "INDX", "QSDT", "NNAM", "QSTA", "ANAM", "ALST", "ALLS", "ALCS",
    ]
    .iter()
    .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
    .filter_map(|sig| first_index_for_sig(fields_by_sig, sig))
    .min()
    .unwrap_or(usize::MAX)
}

fn should_skip_qust_unscoped_condition_slot(
    target_record_def: &RecordDef,
    sig: crate::ids::SubrecordSig,
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> bool {
    if target_record_def.id != "QUST" || !matches!(&sig.0, b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2") {
        return false;
    }
    let Some(condition_index) = first_index_for_sig(fields_by_sig, sig) else {
        return false;
    };
    condition_index >= first_qust_unscoped_condition_boundary(fields_by_sig)
}

fn first_qust_unscoped_condition_boundary(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> usize {
    [
        "INDX", "QSDT", "NNAM", "QSTA", "ANAM", "ALST", "ALLS", "ALCS",
    ]
    .iter()
    .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
    .filter_map(|sig| first_index_for_sig(fields_by_sig, sig))
    .min()
    .unwrap_or(usize::MAX)
}

fn qust_story_manager_conditions_start(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> usize {
    crate::ids::SubrecordSig::from_str("NEXT")
        .ok()
        .and_then(|sig| first_index_for_sig(fields_by_sig, sig))
        .map(|index| index.saturating_add(1))
        .unwrap_or(0)
}

fn first_qust_story_manager_conditions_boundary(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> usize {
    [
        "INDX", "QSDT", "NNAM", "QSTA", "ANAM", "ALST", "ALLS", "ALCS",
    ]
    .iter()
    .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
    .filter_map(|sig| first_index_for_sig(fields_by_sig, sig))
    .min()
    .unwrap_or(usize::MAX)
}

fn first_qust_orphan_objective_boundary(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> usize {
    ["INDX", "QSDT", "ANAM", "ALST", "ALLS", "ALCS"]
        .iter()
        .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
        .filter_map(|sig| first_index_for_sig(fields_by_sig, sig))
        .min()
        .unwrap_or(usize::MAX)
}

fn first_qust_objective_boundary_after(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    after_index: usize,
) -> usize {
    ["QOBJ", "ANAM", "ALST", "ALLS", "ALCS"]
        .iter()
        .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
        .filter_map(|sig| {
            fields_by_sig.get(&sig).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.original_index > after_index)
                    .map(|entry| entry.original_index)
            })
        })
        .min()
        .unwrap_or(usize::MAX)
}

fn first_qust_alias_anchor_after(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    after_index: usize,
) -> Option<usize> {
    let sigs: Vec<crate::ids::SubrecordSig> = QUST_ALIAS_ANCHOR_SIGS
        .iter()
        .filter_map(|sig| crate::ids::SubrecordSig::from_str(sig).ok())
        .collect();
    first_index_after(fields_by_sig, &sigs, after_index)
}

fn first_index_after(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    sigs: &[crate::ids::SubrecordSig],
    after_index: usize,
) -> Option<usize> {
    sigs.iter()
        .filter_map(|sig| {
            fields_by_sig.get(sig).and_then(|entries| {
                entries
                    .iter()
                    .find(|entry| entry.original_index > after_index)
                    .map(|entry| entry.original_index)
            })
        })
        .min()
}

fn is_term_marker_parameters_slot(
    target_record_def: &RecordDef,
    target_subrecord_def: &SubrecordDef,
) -> bool {
    target_record_def.id == "TERM"
        && target_subrecord_def.id == "SNAM"
        && target_subrecord_def
            .codec
            .as_deref()
            .is_some_and(|codec| codec.starts_with("array_struct:"))
}

fn should_skip_optional_unscoped_duplicate(
    target_record_def: &RecordDef,
    schema_index: usize,
    target_subrecord_def: &SubrecordDef,
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> bool {
    if target_subrecord_def.required
        || target_subrecord_def.multiple
        || target_subrecord_def.scope_id.is_some()
    {
        return false;
    }

    let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
        return false;
    };
    let Some(first_entry_index) = fields_by_sig
        .get(&sig)
        .and_then(VecDeque::front)
        .map(|entry| entry.original_index)
    else {
        return false;
    };

    let subrecords = &target_record_def.subrecords;
    let mut index = schema_index + 1;
    while index < subrecords.len() {
        let Some(scope_id) = subrecords[index].scope_id.as_deref() else {
            index += 1;
            continue;
        };
        let scope_start = index;
        index += 1;
        while index < subrecords.len() && subrecords[index].scope_id.as_deref() == Some(scope_id) {
            index += 1;
        }
        let segment = &subrecords[scope_start..index];
        if !segment.iter().any(|def| def.id == target_subrecord_def.id) {
            continue;
        }
        let Some(anchor_def) = segment.first() else {
            continue;
        };
        let Ok(anchor_sig) = crate::ids::SubrecordSig::from_str(&anchor_def.id) else {
            continue;
        };
        if fields_by_sig
            .get(&anchor_sig)
            .and_then(VecDeque::front)
            .is_some_and(|entry| entry.original_index < first_entry_index)
        {
            return true;
        }
    }

    false
}

fn pop_first_in_original_range(
    fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    sig: crate::ids::SubrecordSig,
    start: usize,
    end: usize,
) -> Option<IndexedFieldEntry> {
    let entries = fields_by_sig.get_mut(&sig)?;
    let pos = entries
        .iter()
        .position(|entry| entry.original_index >= start && entry.original_index < end)?;
    entries.remove(pos)
}

fn pop_all_in_original_range(
    fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    sig: crate::ids::SubrecordSig,
    start: usize,
    end: usize,
) -> Vec<IndexedFieldEntry> {
    let Some(entries) = fields_by_sig.get_mut(&sig) else {
        return Vec::new();
    };
    let mut popped = Vec::new();
    while let Some(pos) = entries
        .iter()
        .position(|entry| entry.original_index >= start && entry.original_index < end)
    {
        let Some(entry) = entries.remove(pos) else {
            break;
        };
        popped.push(entry);
    }
    popped
}

fn first_index_for_sig(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    sig: crate::ids::SubrecordSig,
) -> Option<usize> {
    fields_by_sig
        .get(&sig)
        .and_then(VecDeque::front)
        .map(|entry| entry.original_index)
}

fn scoped_segment_by_id<'a>(
    record_def: &'a RecordDef,
    scope_id: &str,
) -> Option<(usize, &'a [SubrecordDef])> {
    let mut index = 0usize;
    while index < record_def.subrecords.len() {
        if record_def.subrecords[index].scope_id.as_deref() != Some(scope_id) {
            index += 1;
            continue;
        }

        let start = index;
        index += 1;
        while index < record_def.subrecords.len()
            && record_def.subrecords[index].scope_id.as_deref() == Some(scope_id)
        {
            index += 1;
        }
        return Some((index, &record_def.subrecords[start..index]));
    }
    None
}

fn segment_sigs(segment: &[SubrecordDef]) -> Vec<crate::ids::SubrecordSig> {
    segment
        .iter()
        .filter_map(|def| crate::ids::SubrecordSig::from_str(&def.id).ok())
        .collect()
}

fn segment_def_for_sig(
    segment: &[SubrecordDef],
    sig: crate::ids::SubrecordSig,
) -> Option<&SubrecordDef> {
    segment
        .iter()
        .find(|def| crate::ids::SubrecordSig::from_str(&def.id).ok() == Some(sig))
}

fn first_schema_boundary_after_scope(
    record_def: &RecordDef,
    scope_id: &str,
    excluded_sigs: &HashSet<crate::ids::SubrecordSig>,
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
) -> usize {
    let Some((scope_end, _)) = scoped_segment_by_id(record_def, scope_id) else {
        return usize::MAX;
    };
    record_def.subrecords[scope_end..]
        .iter()
        .filter_map(|def| crate::ids::SubrecordSig::from_str(&def.id).ok())
        .filter(|sig| !excluded_sigs.contains(sig))
        .filter_map(|sig| first_index_for_sig(fields_by_sig, sig))
        .min()
        .unwrap_or(usize::MAX)
}

fn pop_all_sigs_in_original_range(
    fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    sigs: &HashSet<crate::ids::SubrecordSig>,
    start: usize,
    end: usize,
) -> Vec<IndexedFieldEntry> {
    let mut popped = Vec::new();
    for sig in sigs {
        popped.extend(pop_all_in_original_range(fields_by_sig, *sig, start, end));
    }
    popped.sort_by_key(|entry| entry.original_index);
    popped
}

fn is_empty_marker_entry(entry: &FieldEntry) -> bool {
    matches!(&entry.value, FieldValue::None)
        || matches!(&entry.value, FieldValue::Bytes(bytes) if bytes.is_empty())
}

fn scoped_children_are_grouped_after_anchors(
    segment: &[SubrecordDef],
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    anchor_sig: crate::ids::SubrecordSig,
) -> bool {
    let Some(last_anchor_index) = fields_by_sig
        .get(&anchor_sig)
        .and_then(|entries| entries.back())
        .map(|entry| entry.original_index)
    else {
        return false;
    };
    let first_child_index = segment
        .iter()
        .skip(1)
        .filter_map(|def| crate::ids::SubrecordSig::from_str(&def.id).ok())
        .filter_map(|sig| {
            fields_by_sig
                .get(&sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
        })
        .min();

    first_child_index.is_some_and(|index| index > last_anchor_index)
}

fn first_segment_entry_index_before(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    segment: &[SubrecordDef],
    end: usize,
) -> Option<usize> {
    segment
        .iter()
        .filter_map(|def| crate::ids::SubrecordSig::from_str(&def.id).ok())
        .filter_map(|sig| {
            fields_by_sig
                .get(&sig)
                .and_then(VecDeque::front)
                .map(|entry| entry.original_index)
        })
        .filter(|index| *index < end)
        .min()
}

fn pop_all_segment_entries_in_original_range<'a>(
    fields_by_sig: &mut HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    segment: &'a [SubrecordDef],
    start: usize,
    end: usize,
) -> Vec<(IndexedFieldEntry, &'a SubrecordDef)> {
    let mut popped = Vec::new();
    for target_subrecord_def in segment {
        let Ok(sig) = crate::ids::SubrecordSig::from_str(&target_subrecord_def.id) else {
            continue;
        };
        let Some(entries) = fields_by_sig.get_mut(&sig) else {
            continue;
        };
        while let Some(pos) = entries
            .iter()
            .position(|entry| entry.original_index >= start && entry.original_index < end)
        {
            let Some(entry) = entries.remove(pos) else {
                break;
            };
            popped.push((entry, target_subrecord_def));
        }
    }
    popped.sort_by_key(|(entry, _)| entry.original_index);
    popped
}

fn first_pack_procedure_tree_boundary_after(
    fields_by_sig: &HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>>,
    start: usize,
) -> usize {
    let mut end = usize::MAX;
    for boundary in ["UNAM", "BNAM", "POBA", "POEA", "POCA"] {
        let Ok(sig) = crate::ids::SubrecordSig::from_str(boundary) else {
            continue;
        };
        if let Some(index) = fields_by_sig
            .get(&sig)
            .and_then(|entries| entries.iter().find(|entry| entry.original_index > start))
            .map(|entry| entry.original_index)
        {
            end = end.min(index);
        }
    }
    end
}

fn normalize_field_entry(
    mut entry: FieldEntry,
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
    interner: Option<&StringInterner>,
) -> FieldEntry {
    entry.value = normalize_field_value(entry.value, source_def, target_def, interner);
    entry
}

fn normalize_target_form_version_union_bytes(
    record: &mut Record,
    target_schema: &AuthoringSchema,
    target_record_def: &RecordDef,
) {
    for entry in &mut record.fields {
        let Some(target_def) = target_record_def.subrecord_def(entry.sig.as_str()) else {
            continue;
        };
        let Some(target_size) = target_form_version_union_fixed_size(
            target_schema,
            target_record_def.id.as_str(),
            target_def,
        ) else {
            continue;
        };
        let FieldValue::Bytes(raw) = &mut entry.value else {
            continue;
        };
        raw.truncate(target_size);
    }
}

fn target_form_version_union_fixed_size(
    target_schema: &AuthoringSchema,
    record_sig: &str,
    target_def: &SubrecordDef,
) -> Option<usize> {
    if target_def.union_variants.len() < 2 {
        return None;
    }

    let legacy_layout = target_schema.struct_field_layout(record_sig, target_def.id.as_str());
    let target_layout = target_schema.struct_field_layout_versioned(
        record_sig,
        target_def.id.as_str(),
        Some(crate::fixups::remap_struct_internal_formids::FO4_TARGET_FORM_VERSION),
    );
    if legacy_layout.is_empty()
        || target_layout.is_empty()
        || (legacy_layout.len() == target_layout.len()
            && legacy_layout
                .iter()
                .zip(&target_layout)
                .all(|(legacy, target)| {
                    legacy.field_id == target.field_id
                        && legacy.offset == target.offset
                        && legacy.width == target.width
                }))
    {
        return None;
    }

    target_layout
        .iter()
        .map(|field| field.offset.saturating_add(field.width))
        .max()
}

fn adapt_fo76_imgs_hdr_to_fo4_hnam(
    record: &mut Record,
    source_record_def: Option<&RecordDef>,
    target_record_def: &RecordDef,
) {
    if record.sig.as_str() != "IMGS" || !is_fo76_imgs_record_def(source_record_def) {
        return;
    }
    if target_record_def.id != "IMGS"
        || !target_record_def
            .subrecords
            .iter()
            .any(|subrecord| subrecord.id == "HNAM")
    {
        return;
    }

    let Ok(hnam_sig) = crate::ids::SubrecordSig::from_str("HNAM") else {
        return;
    };

    for field in &mut record.fields {
        if !matches!(field.sig.as_str(), "ENAM" | "FNAM" | "GNAM") {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            continue;
        };
        let Some(hnam) = fo76_imgs_hdr_bytes_to_fo4_hnam(bytes.as_slice()) else {
            continue;
        };
        field.sig = hnam_sig;
        field.value = FieldValue::Bytes(SmallVec::from_vec(hnam));
    }
}

fn is_fo76_imgs_record_def(source_record_def: Option<&RecordDef>) -> bool {
    source_record_def
        .filter(|record_def| record_def.id == "IMGS")
        .and_then(|record_def| {
            record_def
                .subrecords
                .iter()
                .find(|subrecord| subrecord.id == "ENAM")
        })
        .is_some_and(|subrecord| {
            subrecord
                .fields
                .iter()
                .any(|field| field.id == "auto_exposure_max")
        })
}

fn fo76_imgs_hdr_bytes_to_fo4_hnam(raw: &[u8]) -> Option<Vec<u8>> {
    let source = decode_f32_prefix(raw);
    if source.len() < 9 {
        return None;
    }

    let read = |index: usize, fallback: f32| -> f32 {
        source
            .get(index)
            .copied()
            .filter(|value| value.is_finite())
            .unwrap_or(fallback)
    };

    let weather_percent_sunlight = read(6, 0.0).abs() > 20.0;
    let bloom_scale_floor = if weather_percent_sunlight { 0.2 } else { 0.1 };
    let sunlight_floor = if weather_percent_sunlight { 4.5 } else { 1.8 };
    let sky_floor = if weather_percent_sunlight { 2.4 } else { 1.5 };

    Some(encode_f32s(&[
        read(0, 3.0),
        read(1, 0.02),
        read(2, 0.5),
        positive_or_floor(read(3, bloom_scale_floor), bloom_scale_floor),
        positive_or_floor(read(4, 3.25), 3.25),
        positive_or_floor(read(5, 1.6), 1.6),
        fo76_scale_or_floor(read(6, sunlight_floor), sunlight_floor),
        fo76_scale_or_floor(read(7, sky_floor), sky_floor),
        read(8, 0.18),
    ]))
}

fn decode_f32_prefix(bytes: &[u8]) -> Vec<f32> {
    bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]))
        .collect()
}

fn encode_f32s(values: &[f32]) -> Vec<u8> {
    let mut out = Vec::with_capacity(values.len() * 4);
    for value in values {
        out.extend_from_slice(&value.to_le_bytes());
    }
    out
}

fn positive_or_floor(value: f32, floor: f32) -> f32 {
    if value.is_finite() && value > 0.0 {
        value
    } else {
        floor
    }
}

fn fo76_scale_or_floor(value: f32, floor: f32) -> f32 {
    let scaled = if value.abs() > 20.0 {
        value / 100.0
    } else {
        value
    };
    positive_or_floor(scaled, floor).max(floor)
}

fn normalize_fo76_imgs_lut_paths(
    record: &mut Record,
    source_record_def: Option<&RecordDef>,
    target_record_def: &RecordDef,
    interner: Option<&StringInterner>,
) {
    if record.sig.as_str() != "IMGS"
        || !source_record_def.is_some_and(|record_def| record_def.id == "IMGS")
        || target_record_def.id != "IMGS"
    {
        return;
    }
    let Some(interner) = interner else {
        return;
    };
    let Ok(tx00_sig) = crate::ids::SubrecordSig::from_str("TX00") else {
        return;
    };

    for field in &mut record.fields {
        if field.sig != tx00_sig {
            continue;
        }
        let FieldValue::String(sym) = field.value else {
            continue;
        };
        let Some(path) = interner.resolve(sym) else {
            continue;
        };
        let Some(normalized) = normalize_fo76_imgs_lut_path(path) else {
            continue;
        };
        field.value = FieldValue::String(interner.intern(&normalized));
    }
}

fn normalize_fo76_imgs_lut_path(path: &str) -> Option<String> {
    let slashes_normalized = path.replace('/', "\\");
    let stripped = strip_ascii_prefix(&slashes_normalized, "data\\textures\\")
        .or_else(|| strip_ascii_prefix(&slashes_normalized, "textures\\"))
        .unwrap_or(slashes_normalized.as_str());

    if stripped == path {
        None
    } else {
        Some(stripped.to_string())
    }
}

fn strip_ascii_prefix<'a>(value: &'a str, prefix: &str) -> Option<&'a str> {
    value
        .get(..prefix.len())
        .filter(|head| head.eq_ignore_ascii_case(prefix))
        .map(|_| &value[prefix.len()..])
}

fn normalize_field_value(
    value: FieldValue,
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
    interner: Option<&StringInterner>,
) -> FieldValue {
    match value {
        FieldValue::Bytes(raw) => FieldValue::Bytes(SmallVec::from_vec(normalize_raw_bytes(
            raw.as_slice(),
            source_def,
            target_def,
        ))),
        FieldValue::Struct(fields) => {
            if let Some(interner) = interner {
                let target_fields = selected_subrecord_union_variant(target_def)
                    .map(|variant| variant.fields.as_slice())
                    .unwrap_or_else(|| target_def.fields.as_slice());
                FieldValue::Struct(normalize_struct_pairs(
                    fields,
                    source_def.map(|def| def.fields.as_slice()),
                    target_fields,
                    interner,
                ))
            } else {
                FieldValue::Struct(fields)
            }
        }
        FieldValue::List(mut items)
            if should_collapse_list_to_singleton(source_def, target_def) =>
        {
            if items.is_empty() {
                FieldValue::List(items)
            } else {
                normalize_field_value(items.remove(0), source_def, target_def, interner)
            }
        }
        FieldValue::List(items) => {
            let Some(interner) = interner else {
                return FieldValue::List(items);
            };
            FieldValue::List(
                items
                    .into_iter()
                    .map(|item| {
                        normalize_list_item_against_fields(
                            item,
                            source_def.map(|def| def.fields.as_slice()),
                            target_def.fields.as_slice(),
                            interner,
                        )
                    })
                    .collect(),
            )
        }
        value => {
            let Some(codec) = fixed_codec_for_subrecord(source_def, target_def) else {
                return value;
            };
            match encode_fixed_scalar(&value, codec) {
                Some(bytes) => FieldValue::Bytes(SmallVec::from_vec(bytes)),
                None => value,
            }
        }
    }
}

fn should_collapse_list_to_singleton(
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
) -> bool {
    if target_def
        .codec
        .as_deref()
        .is_some_and(|codec| raw_bytes_need_variable_tail_passthrough(target_def, codec))
    {
        return false;
    }
    fixed_size_hint(source_def, target_def).is_some()
}

fn normalize_list_item_against_fields(
    item: FieldValue,
    source_fields: Option<&[FieldDef]>,
    target_fields: &[FieldDef],
    interner: &StringInterner,
) -> FieldValue {
    match item {
        FieldValue::Struct(fields) if !target_fields.is_empty() => FieldValue::Struct(
            normalize_struct_pairs(fields, source_fields, target_fields, interner),
        ),
        other => other,
    }
}

fn normalize_struct_pairs(
    fields: Vec<(Sym, FieldValue)>,
    source_fields: Option<&[FieldDef]>,
    target_fields: &[FieldDef],
    interner: &StringInterner,
) -> Vec<(Sym, FieldValue)> {
    if target_fields.is_empty() {
        return fields;
    }

    let mut seen = HashSet::new();
    let mut ordered = Vec::with_capacity(fields.len());
    for (original_index, (key, value)) in fields.into_iter().enumerate() {
        let Some((target_index, target_field)) = find_matching_field(target_fields, key, interner)
        else {
            continue;
        };
        if !seen.insert(target_index) {
            continue;
        }
        let source_field = source_fields
            .and_then(|fields| find_matching_field(fields, key, interner))
            .map(|(_, field)| field);
        let value = normalize_value_against_field(value, source_field, target_field, interner);
        ordered.push((target_index, original_index, (key, value)));
    }

    ordered.sort_by_key(|(target_index, original_index, _)| (*target_index, *original_index));
    ordered
        .into_iter()
        .map(|(_, _, pair)| pair)
        .collect::<Vec<_>>()
}

fn normalize_value_against_field(
    value: FieldValue,
    source_field: Option<&FieldDef>,
    target_field: &FieldDef,
    interner: &StringInterner,
) -> FieldValue {
    let target_field = select_union_variant(&value, target_field, interner).unwrap_or(target_field);
    match value {
        FieldValue::Bytes(raw) => FieldValue::Bytes(SmallVec::from_vec(normalize_field_bytes(
            raw.as_slice(),
            source_field,
            target_field,
        ))),
        FieldValue::Struct(fields) if !target_field.fields.is_empty() => {
            FieldValue::Struct(normalize_struct_pairs(
                fields,
                source_field.map(|field| field.fields.as_slice()),
                target_field.fields.as_slice(),
                interner,
            ))
        }
        FieldValue::List(mut items) if should_collapse_field_list_to_singleton(target_field) => {
            if items.is_empty() {
                FieldValue::List(items)
            } else {
                normalize_value_against_field(items.remove(0), source_field, target_field, interner)
            }
        }
        FieldValue::List(items) if !target_field.fields.is_empty() => FieldValue::List(
            items
                .into_iter()
                .map(|item| {
                    normalize_list_item_against_fields(
                        item,
                        source_field.map(|field| field.fields.as_slice()),
                        target_field.fields.as_slice(),
                        interner,
                    )
                })
                .collect(),
        ),
        value => {
            let Some(codec) = fixed_codec_for_field(source_field, target_field) else {
                return value;
            };
            match encode_fixed_scalar(&value, codec) {
                Some(bytes) => FieldValue::Bytes(SmallVec::from_vec(bytes)),
                None => value,
            }
        }
    }
}

fn should_collapse_field_list_to_singleton(target_field: &FieldDef) -> bool {
    target_field.fields.is_empty() && target_fixed_codec(target_field).is_some()
}

fn select_union_variant<'a>(
    value: &FieldValue,
    target_field: &'a FieldDef,
    interner: &StringInterner,
) -> Option<&'a FieldDef> {
    if target_field.union_variants.is_empty() {
        return None;
    }
    let FieldValue::Struct(fields) = value else {
        return None;
    };

    target_field
        .union_variants
        .iter()
        .enumerate()
        .filter_map(|(index, variant)| {
            let score = fields
                .iter()
                .filter(|(key, _)| find_matching_field(&variant.fields, *key, interner).is_some())
                .count();
            (score > 0).then_some((score, std::cmp::Reverse(index), variant))
        })
        .max_by_key(|(score, reverse_index, _)| (*score, *reverse_index))
        .map(|(_, _, variant)| variant)
}

fn find_matching_field<'a>(
    fields: &'a [FieldDef],
    key: Sym,
    interner: &StringInterner,
) -> Option<(usize, &'a FieldDef)> {
    let name = interner.resolve(key)?;
    let canonical_name = canonical_field_name(name);
    fields.iter().enumerate().find(|(_, field)| {
        field.id == name
            || field.display_label.as_deref() == Some(name)
            || canonical_field_name(&field.id) == canonical_name
            || field
                .display_label
                .as_deref()
                .is_some_and(|label| canonical_field_name(label) == canonical_name)
    })
}

fn canonical_field_name(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn set_u32_count(value: &mut FieldValue, expected: u32) {
    match value {
        FieldValue::Uint(n) => *n = expected as u64,
        FieldValue::Int(n) => *n = expected as i64,
        FieldValue::Struct(fields) => {
            if let Some((_, first_value)) = fields.first_mut() {
                set_u32_count(first_value, expected);
            } else {
                *value = FieldValue::Uint(expected as u64);
            }
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            bytes[..4].copy_from_slice(&expected.to_le_bytes());
        }
        FieldValue::Bytes(bytes) => {
            bytes.clear();
            bytes.extend_from_slice(&expected.to_le_bytes());
        }
        _ => *value = FieldValue::Uint(expected as u64),
    }
}

fn sync_pack_pkcu_to_data_input_count(record: &mut Record) {
    if record.sig.as_str() != "PACK" {
        return;
    }

    let Some(pkcu_pos) = record
        .fields
        .iter()
        .position(|entry| entry.sig.as_str() == "PKCU")
    else {
        return;
    };
    let end_pos = record
        .fields
        .iter()
        .enumerate()
        .skip(pkcu_pos + 1)
        .find_map(|(index, entry)| (entry.sig.as_str() == "XNAM").then_some(index))
        .unwrap_or(record.fields.len());
    let data_input_count = record.fields[pkcu_pos + 1..end_pos]
        .iter()
        .filter(|entry| entry.sig.as_str() == "ANAM")
        .count();

    set_u32_count(&mut record.fields[pkcu_pos].value, data_input_count as u32);
}

fn sync_ksiz_to_kwda(record: &mut Record) {
    let Some(expected) = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == "KWDA")
        .and_then(|entry| formid_array_item_count(&entry.value))
    else {
        return;
    };

    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "KSIZ" {
            continue;
        }
        match &mut entry.value {
            FieldValue::Uint(n) => *n = expected as u64,
            FieldValue::Int(n) => *n = expected as i64,
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                bytes[..4].copy_from_slice(&expected.to_le_bytes());
            }
            _ => {}
        }
        break;
    }
}

/// FO4 RACE `HCLF` "Default Hair Colors" is a fixed 2-element array (Male,
/// Female) — xEdit `wbArray(HCLF, ..., ['Male','Female'])` — 8 bytes. A FO76
/// source race with only the Male color produces a 1-element (4-byte) HCLF, so
/// FO4 reports "Expected 4 bytes, found 0" for the missing Female slot. Pad a
/// 1-element HCLF up to 2 by appending a NULL (`00000000`) color; NULL is in the
/// field's allowed set (`[NULL, CLFM]`). The schema models HCLF as a variable
/// `formid_array` (correct for roundtrip), so this pad lives in the emit, not
/// the schema. Only the exactly-1 case is padded; 0 or 2+ are left as-is.
fn pad_race_hclf_to_male_female(record: &mut Record) {
    if record.sig.as_str() != "RACE" {
        return;
    }
    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "HCLF" {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                bytes.extend_from_slice(&0u32.to_le_bytes());
            }
            FieldValue::List(items) if items.len() == 1 => {
                items.push(FieldValue::Uint(0));
            }
            _ => {}
        }
    }
}

const NPC_ACBS_TEMPLATE_FLAGS_OFFSET: usize = 14;
const NPC_ACBS_FLAG_ESSENTIAL: u32 = 0x0000_0002;
const NPC_ACBS_FLAG_UNKNOWN_25: u32 = 0x0200_0000;
const NPC_ACBS_FLAG_IS_GHOST: u32 = 0x2000_0000;
const NPC_ACBS_FLAG_INVULNERABLE: u32 = 0x8000_0000;
const NPC_ACBS_FLAGS_STRIPPED_FOR_FO4: u32 = NPC_ACBS_FLAG_IS_GHOST | NPC_ACBS_FLAG_INVULNERABLE;
const NPC_TEMPLATE_TRAITS: u16 = 0x0001;
const NPC_TEMPLATE_STATS: u16 = 0x0002;
const NPC_TEMPLATE_FACTIONS: u16 = 0x0004;
const NPC_TEMPLATE_AI_DATA: u16 = 0x0010;
const NPC_TEMPLATE_AI_PACKAGES: u16 = 0x0020;
const NPC_TEMPLATE_MODEL_ANIMATION: u16 = 0x0040;
const NPC_TEMPLATE_BASE_DATA: u16 = 0x0080;
const NPC_TEMPLATE_INVENTORY: u16 = 0x0100;
const NPC_TEMPLATE_SCRIPT: u16 = 0x0200;
const NPC_TPTA_SLOT_TEMPLATE_FLAGS: [u16; 13] = [
    NPC_TEMPLATE_TRAITS,
    NPC_TEMPLATE_STATS,
    NPC_TEMPLATE_FACTIONS,
    0,
    NPC_TEMPLATE_AI_DATA,
    NPC_TEMPLATE_AI_PACKAGES,
    NPC_TEMPLATE_MODEL_ANIMATION,
    NPC_TEMPLATE_BASE_DATA,
    NPC_TEMPLATE_INVENTORY,
    NPC_TEMPLATE_SCRIPT,
    0,
    0,
    0,
];

const FO4_CONT_DATA_FLAGS: u8 = 0x07;

fn normalize_translated_cont_data(record: &mut Record, source_record_def: Option<&RecordDef>) {
    if record.sig.as_str() != "CONT" || !source_record_def.is_some_and(|def| def.id == "CONT") {
        return;
    }

    let Some(data) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.as_str() == "DATA")
    else {
        return;
    };

    retain_cont_data_flags_supported_by_fo4(&mut data.value);
}

fn retain_cont_data_flags_supported_by_fo4(value: &mut FieldValue) {
    match value {
        FieldValue::Uint(flags) => *flags &= u64::from(FO4_CONT_DATA_FLAGS),
        FieldValue::Int(flags) => *flags &= i64::from(FO4_CONT_DATA_FLAGS),
        FieldValue::Bytes(bytes) if !bytes.is_empty() => bytes[0] &= FO4_CONT_DATA_FLAGS,
        FieldValue::Struct(fields) => {
            if let Some((_, flags)) = fields.first_mut() {
                retain_cont_data_flags_supported_by_fo4(flags);
            }
        }
        _ => {}
    }
}

fn normalize_translated_npc_acbs(record: &mut Record, source_record_def: Option<&RecordDef>) {
    if record.sig.as_str() != "NPC_" || !source_record_def.is_some_and(|def| def.id == "NPC_") {
        return;
    }

    let clear_essential = is_w05_leveled_denizen_lite_ally(record);
    let mut template_flags = 0;
    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "TPTA" {
            continue;
        }
        template_flags = sanitize_npc_tpta_value(&mut entry.value);
        break;
    }

    for entry in record.fields.iter_mut() {
        if entry.sig.as_str() != "ACBS" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.len() < NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2 {
            continue;
        }

        if bytes.len() >= 4 {
            let mut flags = u32::from_le_bytes(bytes[..4].try_into().unwrap());
            flags &= !NPC_ACBS_FLAGS_STRIPPED_FOR_FO4;
            if clear_essential {
                flags &= !NPC_ACBS_FLAG_ESSENTIAL;
            }
            bytes[..4].copy_from_slice(&flags.to_le_bytes());
        }
        bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET..NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
            .copy_from_slice(&template_flags.to_le_bytes());
        break;
    }
}

fn is_w05_leveled_denizen_lite_ally(record: &Record) -> bool {
    let Some(edid) = record.fields.iter().find_map(|field| {
        if field.sig.as_str() != "EDID" {
            return None;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            return None;
        };
        let end = bytes
            .iter()
            .position(|byte| *byte == 0)
            .unwrap_or(bytes.len());
        Some(&bytes[..end])
    }) else {
        return false;
    };

    edid.starts_with(b"W05_LvlDenizen_") && edid.ends_with(b"_or_LiteAlly")
}

fn template_flags_for_npc_tpta(bytes: &[u8]) -> u16 {
    let mut flags = 0_u16;
    for (slot_index, flag) in NPC_TPTA_SLOT_TEMPLATE_FLAGS.iter().enumerate() {
        if *flag == 0 {
            continue;
        }
        let offset = slot_index * 4;
        let Some(raw) = bytes.get(offset..offset + 4) else {
            continue;
        };
        if raw != [0, 0, 0, 0] {
            flags |= *flag;
        }
    }
    flags
}

fn sanitize_npc_tpta_value(value: &mut FieldValue) -> u16 {
    match value {
        FieldValue::Bytes(bytes) => {
            zero_unsupported_npc_tpta_slots(bytes.as_mut_slice());
            template_flags_for_npc_tpta(bytes.as_slice())
        }
        FieldValue::Struct(fields) => {
            sanitize_npc_tpta_field_values(fields.iter_mut().map(|(_, value)| value).enumerate());
            template_flags_for_npc_tpta_field_values(fields.iter().map(|(_, value)| value))
        }
        FieldValue::List(items) => {
            sanitize_npc_tpta_field_values(items.iter_mut().enumerate());
            template_flags_for_npc_tpta_field_values(items.iter())
        }
        _ => 0,
    }
}

fn sanitize_npc_tpta_field_values<'a, I>(values: I)
where
    I: IntoIterator<Item = (usize, &'a mut FieldValue)>,
{
    for (slot_index, value) in values {
        let flag = NPC_TPTA_SLOT_TEMPLATE_FLAGS
            .get(slot_index)
            .copied()
            .unwrap_or(0);
        if flag == 0 {
            *value = FieldValue::Uint(0);
        }
    }
}

fn template_flags_for_npc_tpta_field_values<'a, I>(values: I) -> u16
where
    I: IntoIterator<Item = &'a FieldValue>,
{
    let mut flags = 0_u16;
    for (slot_index, value) in values.into_iter().enumerate() {
        let Some(flag) = NPC_TPTA_SLOT_TEMPLATE_FLAGS.get(slot_index) else {
            continue;
        };
        if *flag != 0 && npc_tpta_field_value_is_nonzero(value) {
            flags |= *flag;
        }
    }
    flags
}

fn npc_tpta_field_value_is_nonzero(value: &FieldValue) -> bool {
    match value {
        FieldValue::None => false,
        FieldValue::Bool(value) => *value,
        FieldValue::Int(value) => *value != 0,
        FieldValue::Uint(value) => *value != 0,
        FieldValue::Float(value) => *value != 0.0,
        FieldValue::String(_) => true,
        FieldValue::Bytes(bytes) => bytes.iter().any(|byte| *byte != 0),
        FieldValue::FormKey(form_key) => form_key.local != 0,
        FieldValue::List(items) => items.iter().any(npc_tpta_field_value_is_nonzero),
        FieldValue::Struct(fields) => fields
            .iter()
            .any(|(_, value)| npc_tpta_field_value_is_nonzero(value)),
    }
}

fn zero_unsupported_npc_tpta_slots(bytes: &mut [u8]) {
    for (slot_index, flag) in NPC_TPTA_SLOT_TEMPLATE_FLAGS.iter().enumerate() {
        if *flag != 0 {
            continue;
        }
        let offset = slot_index * 4;
        let Some(raw) = bytes.get_mut(offset..offset + 4) else {
            continue;
        };
        raw.fill(0);
    }
}

fn formid_array_item_count(value: &FieldValue) -> Option<u32> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() % 4 == 0 => Some((bytes.len() / 4) as u32),
        FieldValue::List(items) => Some(items.len() as u32),
        _ => None,
    }
}

fn normalize_raw_bytes(
    raw: &[u8],
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
) -> Vec<u8> {
    if let Some(normalized) = normalize_fo76_magic_efit_for_fo4(raw, source_def, target_def) {
        return normalized;
    }

    if let Some(normalized) = normalize_fo76_alch_enit_for_fo4(raw, source_def, target_def) {
        return normalized;
    }

    if let Some(normalized) = normalize_fo76_qust_qsta_for_fo4(raw, source_def, target_def) {
        return normalized;
    }

    if let Some(normalized) = normalize_fo76_rfct_data_for_fo4(raw, source_def, target_def) {
        return normalized;
    }

    if let Some(target_codec) = target_def.codec.as_deref() {
        if raw_bytes_need_variable_tail_passthrough(target_def, target_codec) {
            return raw.to_vec();
        }

        if let Some(target_row_size) = array_struct_row_size(target_codec) {
            return normalize_array_struct_bytes(raw, source_def, target_row_size);
        }
    }

    // Heterogeneous-size unions (e.g. SNDR.BNAM: 6-byte `values` struct vs
    // 4-byte `base_descriptor` formid). `fixed_size_hint` picks ONE variant's
    // size (subrecord_union_fixed_size selects via .rev(), returning the
    // smaller formid size for SNDR.BNAM); the clamp below would then truncate a
    // valid 6-byte payload to 4 and drop the trailing Static Attenuation u16.
    // If the raw length is within the variants' max width, leave it unchanged;
    // only the genuine over-long case (raw > max variant size) is clamped.
    let union_sizes = subrecord_union_variant_sizes(target_def);
    if union_sizes.len() > 1 && union_sizes.iter().any(|&s| s != union_sizes[0]) {
        let max = union_sizes.iter().copied().max().unwrap_or(0);
        if raw.len() <= max {
            return raw.to_vec();
        }
        return raw[..max].to_vec();
    }

    if let Some(expected_size) = fixed_size_hint(source_def, target_def) {
        if raw.len() > expected_size {
            return raw[..expected_size].to_vec();
        }
    }

    raw.to_vec()
}

fn normalize_fo76_magic_efit_for_fo4(
    raw: &[u8],
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
) -> Option<Vec<u8>> {
    let source_def = source_def?;
    if target_def.id != "EFIT"
        || !subrecord_fields_are(target_def, &["magnitude", "area", "duration"])
    {
        return None;
    }

    match raw.len() {
        12 if subrecord_fields_are(source_def, &["effect_id", "magnitude", "area", "duration"]) => {
            let mut normalized = Vec::with_capacity(12);
            normalized.extend_from_slice(&raw[4..8]);
            normalized.extend_from_slice(&0_u32.to_le_bytes());
            normalized.extend_from_slice(&raw[8..12]);
            Some(normalized)
        }
        16.. => Some(raw[4..16].to_vec()),
        _ => None,
    }
}

const FO76_ALCH_ENIT_FLAGS_SHARED_WITH_FO4: u32 = 0x0003_0003;

fn normalize_fo76_alch_enit_for_fo4(
    raw: &[u8],
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
) -> Option<Vec<u8>> {
    if target_def.id != "ENIT"
        || !subrecord_fields_are(
            source_def?,
            &[
                "value",
                "flags",
                "addiction",
                "addiction_chance",
                "sound_consume",
                "health",
                "spoiled",
                "is_canned",
                "canned_item_base",
            ],
        )
        || !subrecord_fields_are(
            target_def,
            &[
                "value",
                "flags",
                "addiction",
                "addiction_chance",
                "sound_consume",
            ],
        )
        || raw.len() < 20
    {
        return None;
    }

    let mut normalized = raw[..20].to_vec();
    let flags = u32::from_le_bytes(normalized[4..8].try_into().ok()?);
    normalized[4..8].copy_from_slice(&(flags & FO76_ALCH_ENIT_FLAGS_SHARED_WITH_FO4).to_le_bytes());
    Some(normalized)
}

fn normalize_fo76_qust_qsta_for_fo4(
    raw: &[u8],
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
) -> Option<Vec<u8>> {
    if target_def.id != "QSTA"
        || !subrecord_fields_are(source_def?, &["alias", "target_flags", "keyword", "radius"])
        || !subrecord_fields_are(target_def, &["alias", "flags", "keyword"])
        || raw.len() < 14
    {
        return None;
    }

    let mut normalized = Vec::with_capacity(12);
    normalized.extend_from_slice(&raw[0..4]);
    let target_flags = u16::from_le_bytes([raw[4], raw[5]]) as u32;
    normalized.extend_from_slice(&target_flags.to_le_bytes());
    normalized.extend_from_slice(&raw[6..10]);
    Some(normalized)
}

fn normalize_fo76_rfct_data_for_fo4(
    raw: &[u8],
    source_def: Option<&SubrecordDef>,
    target_def: &SubrecordDef,
) -> Option<Vec<u8>> {
    if target_def.id != "DATA"
        || !subrecord_fields_are(source_def?, &["effect_art", "unused", "flags"])
        || !subrecord_fields_are(target_def, &["effect_art", "shader", "flags"])
    {
        return None;
    }

    match raw.len() {
        4 => {
            let mut normalized = Vec::with_capacity(12);
            normalized.extend_from_slice(&raw[0..4]);
            normalized.extend_from_slice(&0_u32.to_le_bytes());
            normalized.extend_from_slice(&0_u32.to_le_bytes());
            Some(normalized)
        }
        8 => {
            let mut normalized = Vec::with_capacity(12);
            normalized.extend_from_slice(&raw[0..4]);
            normalized.extend_from_slice(&0_u32.to_le_bytes());
            normalized.extend_from_slice(&raw[4..8]);
            Some(normalized)
        }
        _ => None,
    }
}

fn subrecord_fields_are(subrecord_def: &SubrecordDef, expected: &[&str]) -> bool {
    subrecord_def.fields.len() == expected.len()
        && subrecord_def
            .fields
            .iter()
            .zip(expected.iter())
            .all(|(field, expected)| field.id == *expected)
}

fn normalize_field_bytes(
    raw: &[u8],
    source_field: Option<&FieldDef>,
    target_field: &FieldDef,
) -> Vec<u8> {
    if let Some(target_codec) = field_codec(target_field) {
        if field_bytes_need_variable_tail_passthrough(target_field, target_codec) {
            return raw.to_vec();
        }

        if let Some(target_row_size) = array_struct_row_size(target_codec) {
            return normalize_array_struct_field_bytes(raw, source_field, target_row_size);
        }
    }

    // Heterogeneous-size union field — see normalize_raw_bytes for rationale.
    let union_sizes = field_union_variant_sizes(target_field);
    if union_sizes.len() > 1 && union_sizes.iter().any(|&s| s != union_sizes[0]) {
        let max = union_sizes.iter().copied().max().unwrap_or(0);
        if raw.len() <= max {
            return raw.to_vec();
        }
        return raw[..max].to_vec();
    }

    if let Some(expected_size) = field_fixed_size_hint(source_field, target_field) {
        if raw.len() > expected_size {
            return raw[..expected_size].to_vec();
        }
    }

    raw.to_vec()
}

/// Fixed byte size of each subrecord-level union variant with a computable
/// fixed codec size. Empty for non-union subrecords. Used to avoid truncating
/// a raw union payload that is already a legal variant width (see SNDR.BNAM).
fn subrecord_union_variant_sizes(target_def: &SubrecordDef) -> smallvec::SmallVec<[usize; 4]> {
    target_def
        .union_variants
        .iter()
        .filter_map(|variant| field_codec(variant).and_then(fixed_size_for_codec))
        .collect()
}

/// Field-level twin of `subrecord_union_variant_sizes`.
fn field_union_variant_sizes(target_field: &FieldDef) -> smallvec::SmallVec<[usize; 4]> {
    target_field
        .union_variants
        .iter()
        .filter_map(|variant| field_codec(variant).and_then(fixed_size_for_codec))
        .collect()
}

fn fixed_size_hint(source_def: Option<&SubrecordDef>, target_def: &SubrecordDef) -> Option<usize> {
    target_def
        .codec
        .as_deref()
        .and_then(fixed_size_for_codec)
        .or_else(|| subrecord_union_fixed_size(target_def))
        .or_else(|| {
            source_fixed_codec_for_raw_target(source_def, target_def).and_then(fixed_size_for_codec)
        })
}

fn selected_subrecord_union_variant(target_def: &SubrecordDef) -> Option<&FieldDef> {
    target_def.union_variants.iter().rev().find(|variant| {
        field_codec(variant)
            .and_then(fixed_size_for_codec)
            .is_some()
            || !variant.fields.is_empty()
    })
}

fn subrecord_union_fixed_size(target_def: &SubrecordDef) -> Option<usize> {
    selected_subrecord_union_variant(target_def)
        .and_then(field_codec)
        .and_then(fixed_size_for_codec)
}

fn source_fixed_codec_for_raw_target<'a>(
    source_def: Option<&'a SubrecordDef>,
    target_def: &SubrecordDef,
) -> Option<&'a str> {
    if target_def.kind != "raw" && target_def.codec.is_some() {
        return None;
    }
    source_def
        .and_then(|def| def.codec.as_deref())
        .filter(|codec| fixed_size_for_codec(codec).is_some())
}

fn fixed_codec_for_subrecord<'a>(
    source_def: Option<&'a SubrecordDef>,
    target_def: &'a SubrecordDef,
) -> Option<&'a str> {
    target_def
        .codec
        .as_deref()
        .filter(|codec| fixed_size_for_codec(codec).is_some())
        .or_else(|| source_fixed_codec_for_raw_target(source_def, target_def))
}

fn field_fixed_size_hint(
    source_field: Option<&FieldDef>,
    target_field: &FieldDef,
) -> Option<usize> {
    target_fixed_codec(target_field)
        .and_then(fixed_size_for_codec)
        .or_else(|| {
            source_fixed_codec_for_raw_target_field(source_field, target_field)
                .and_then(fixed_size_for_codec)
        })
}

fn fixed_codec_for_field<'a>(
    source_field: Option<&'a FieldDef>,
    target_field: &'a FieldDef,
) -> Option<&'a str> {
    target_fixed_codec(target_field)
        .or_else(|| source_fixed_codec_for_raw_target_field(source_field, target_field))
}

fn target_fixed_codec(field: &FieldDef) -> Option<&str> {
    field_codec(field).filter(|codec| fixed_size_for_codec(codec).is_some())
}

fn source_fixed_codec_for_raw_target_field<'a>(
    source_field: Option<&'a FieldDef>,
    target_field: &FieldDef,
) -> Option<&'a str> {
    if target_field.kind != "raw" && target_field.codec.is_some() {
        return None;
    }
    source_field
        .and_then(field_codec)
        .filter(|codec| fixed_size_for_codec(codec).is_some())
}

fn field_codec(field: &FieldDef) -> Option<&str> {
    field
        .codec
        .as_deref()
        .or_else(|| (!field.kind.is_empty()).then_some(field.kind.as_str()))
}

fn field_bytes_need_variable_tail_passthrough(field: &FieldDef, codec: &str) -> bool {
    let Some(struct_field_count) = structured_codec_field_count(codec) else {
        return false;
    };

    !field.fields.is_empty() && field.fields.len() != struct_field_count
}

fn encode_fixed_scalar(value: &FieldValue, codec: &str) -> Option<Vec<u8>> {
    match value {
        FieldValue::None if fixed_size_for_codec(codec) == Some(0) => Some(Vec::new()),
        FieldValue::Bool(value) => encode_fixed_uint(u64::from(*value), codec),
        FieldValue::Int(value) => encode_fixed_int(*value, codec),
        FieldValue::Uint(value) => encode_fixed_uint(*value, codec),
        FieldValue::Float(value) => match codec {
            "f32" | "float" | "float32" => Some(value.to_le_bytes().to_vec()),
            "f64" | "double" => Some((*value as f64).to_le_bytes().to_vec()),
            _ => None,
        },
        _ => None,
    }
}

fn encode_fixed_int(value: i64, codec: &str) -> Option<Vec<u8>> {
    match codec {
        "i8" | "int8" => Some(vec![(value as i8) as u8]),
        "u8" | "uint8" => Some(vec![value as u8]),
        "i16" | "int16" => Some((value as i16).to_le_bytes().to_vec()),
        "u16" | "uint16" => Some((value as u16).to_le_bytes().to_vec()),
        "i32" | "int32" => Some((value as i32).to_le_bytes().to_vec()),
        "u32" | "uint32" | "formid" | "form_id" => Some((value as u32).to_le_bytes().to_vec()),
        "i64" | "int64" => Some(value.to_le_bytes().to_vec()),
        "u64" | "uint64" => Some((value as u64).to_le_bytes().to_vec()),
        _ => None,
    }
}

fn encode_fixed_uint(value: u64, codec: &str) -> Option<Vec<u8>> {
    match codec {
        "i8" | "int8" => Some(vec![(value as i8) as u8]),
        "u8" | "uint8" => Some(vec![value as u8]),
        "i16" | "int16" => Some((value as i16).to_le_bytes().to_vec()),
        "u16" | "uint16" => Some((value as u16).to_le_bytes().to_vec()),
        "i32" | "int32" => Some((value as i32).to_le_bytes().to_vec()),
        "u32" | "uint32" | "formid" | "form_id" => Some((value as u32).to_le_bytes().to_vec()),
        "i64" | "int64" => Some((value as i64).to_le_bytes().to_vec()),
        "u64" | "uint64" => Some(value.to_le_bytes().to_vec()),
        _ => None,
    }
}

fn normalize_array_struct_bytes(
    raw: &[u8],
    source_def: Option<&SubrecordDef>,
    target_row_size: usize,
) -> Vec<u8> {
    if target_row_size == 0 || raw.is_empty() {
        return raw.to_vec();
    }

    let source_row_size = source_def
        .and_then(|def| def.codec.as_deref())
        .and_then(array_struct_row_size);

    if let Some(source_row_size) = source_row_size {
        if source_row_size > target_row_size && raw.len() % source_row_size == 0 {
            if source_row_size == 12 && target_row_size == 8 && looks_like_formid_float_rows(raw) {
                return raw.to_vec();
            }
            let mut projected = Vec::with_capacity(raw.len() / source_row_size * target_row_size);
            for source_row in raw.chunks_exact(source_row_size) {
                projected.extend_from_slice(&source_row[..target_row_size]);
            }
            return projected;
        }
    }

    let remainder = raw.len() % target_row_size;
    if remainder != 0 {
        return raw[..raw.len() - remainder].to_vec();
    }

    raw.to_vec()
}

fn normalize_array_struct_field_bytes(
    raw: &[u8],
    source_field: Option<&FieldDef>,
    target_row_size: usize,
) -> Vec<u8> {
    if target_row_size == 0 || raw.is_empty() {
        return raw.to_vec();
    }

    let source_row_size = source_field
        .and_then(field_codec)
        .and_then(array_struct_row_size);

    if let Some(source_row_size) = source_row_size {
        if source_row_size > target_row_size && raw.len() % source_row_size == 0 {
            if source_row_size == 12 && target_row_size == 8 && looks_like_formid_float_rows(raw) {
                return raw.to_vec();
            }
            let mut projected = Vec::with_capacity(raw.len() / source_row_size * target_row_size);
            for source_row in raw.chunks_exact(source_row_size) {
                projected.extend_from_slice(&source_row[..target_row_size]);
            }
            return projected;
        }
    }

    let remainder = raw.len() % target_row_size;
    if remainder != 0 {
        return raw[..raw.len() - remainder].to_vec();
    }

    raw.to_vec()
}

fn raw_bytes_need_variable_tail_passthrough(target_def: &SubrecordDef, codec: &str) -> bool {
    if matches!(target_def.id.as_str(), "NVNM" | "VMAD") {
        return true;
    }

    let Some(struct_field_count) = structured_codec_field_count(codec) else {
        return false;
    };

    target_def.fields.len() != struct_field_count
}

fn structured_codec_field_count(codec: &str) -> Option<usize> {
    codec
        .strip_prefix("struct:")
        .or_else(|| codec.strip_prefix("array_struct:"))
        .map(|body| {
            body.split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .count()
        })
}

fn array_struct_row_size(codec: &str) -> Option<usize> {
    codec
        .strip_prefix("array_struct:")
        .and_then(fixed_size_for_struct_codec)
}

fn looks_like_formid_float_rows(raw: &[u8]) -> bool {
    if raw.is_empty() || raw.len() % 8 != 0 {
        return false;
    }

    raw.chunks_exact(8).all(|row| {
        let value = f32::from_le_bytes(row[4..8].try_into().expect("row value is four bytes"));
        value.is_finite()
            && (value == 0.0 || (value.abs() >= 0.000_001 && value.abs() <= 1_000_000.0))
    })
}

pub(crate) fn fixed_size_for_codec(codec: &str) -> Option<usize> {
    match codec {
        "bool" | "u8" | "i8" | "int8" | "uint8" => Some(1),
        "u16" | "i16" | "int16" | "uint16" => Some(2),
        "u32" | "i32" | "int32" | "uint32" | "f32" | "float" | "float32" | "formid" | "form_id" => {
            Some(4)
        }
        "u64" | "i64" | "int64" | "uint64" | "f64" | "double" => Some(8),
        "empty" => Some(0),
        "zstring" | "lstring" | "bytes" | "raw" | "string" | "lenstring16" | "omod_data"
        | "formid_array" => None,
        other => other
            .strip_prefix("struct:")
            .and_then(fixed_size_for_struct_codec),
    }
}

fn fixed_size_for_struct_codec(codec: &str) -> Option<usize> {
    let mut total = 0usize;
    for part in codec
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
    {
        total += match part {
            "B" | "b" | "u8" | "i8" | "uint8" | "int8" => 1,
            "H" | "h" | "u16" | "i16" | "uint16" | "int16" => 2,
            "I" | "i" | "f" | "u32" | "i32" | "uint32" | "int32" | "float" | "float32"
            | "formid" | "form_id" => 4,
            "Q" | "q" | "d" | "u64" | "i64" | "uint64" | "int64" | "double" => 8,
            _ => return None,
        };
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;

    fn record(sig: &str, fields: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey::parse("000800@Test.esm", interner).unwrap(),
            eid: None,
            flags: RecordFlags::empty(),
            fields: SmallVec::from_vec(fields),
            warnings: SmallVec::new(),
        }
    }

    fn bytes_field(sig: &str, bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    fn uint_field(sig: &str, value: u64) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Uint(value),
        }
    }

    fn formkey_field(sig: &str, value: FormKey) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::FormKey(value),
        }
    }

    fn string_field(sig: &str, value: &str, interner: &StringInterner) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::String(interner.intern(value)),
        }
    }

    #[test]
    fn translated_race_subgraph_paths_strip_trailing_ascii_controls_only() {
        let interner = StringInterner::new();
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let target_race = target_schema.record_def("RACE").expect("fo4 RACE");
        let source_race = source_schema.record_def("RACE").expect("fo76 RACE");
        let mut race = record(
            "RACE",
            vec![
                string_field(
                    "SGNM",
                    "Actors\\Character\\Behaviors\\GunBehavior.hkx",
                    &interner,
                ),
                string_field("SAPT", "Actors\\Scorched\\Animations\\Mouth\r", &interner),
                string_field(
                    "SAPT",
                    "Actors\\Character\\Animations\\Weapon\\GripRifleAssault\\\r",
                    &interner,
                ),
                string_field("EDID", "ScorchedRace\r", &interner),
            ],
            &interner,
        );

        normalize_translated_race_subgraph_paths(
            &mut race,
            Some(source_race),
            target_race,
            Some(&interner),
        );

        let values: Vec<&str> = race
            .fields
            .iter()
            .filter_map(|entry| match entry.value {
                FieldValue::String(sym) => interner.resolve(sym),
                _ => None,
            })
            .collect();
        assert_eq!(
            values,
            vec![
                "Actors\\Character\\Behaviors\\GunBehavior.hkx",
                "Actors\\Scorched\\Animations\\Mouth",
                "Actors\\Character\\Animations\\Weapon\\GripRifleAssault\\",
                "ScorchedRace\r",
            ]
        );
    }

    fn none_field(sig: &str) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::None,
        }
    }

    fn struct_field(sig: &str, fields: Vec<(Sym, FieldValue)>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value: FieldValue::Struct(fields),
        }
    }

    fn field_def(id: &str) -> FieldDef {
        FieldDef {
            id: id.to_string(),
            kind: String::new(),
            codec: None,
            fields: Vec::new(),
            union_variants: Vec::new(),
            display_label: None,
        }
    }

    fn efit_def(codec: &str, fields: &[&str]) -> SubrecordDef {
        SubrecordDef {
            id: "EFIT".to_string(),
            kind: "parsed".to_string(),
            codec: Some(codec.to_string()),
            fields: fields.iter().map(|field| field_def(field)).collect(),
            union_variants: Vec::new(),
            multiple: true,
            scope_id: Some("effects".to_string()),
            required: false,
            localized: false,
        }
    }

    fn enit_def(codec: &str, fields: &[&str]) -> SubrecordDef {
        SubrecordDef {
            id: "ENIT".to_string(),
            kind: "parsed".to_string(),
            codec: Some(codec.to_string()),
            fields: fields.iter().map(|field| field_def(field)).collect(),
            union_variants: Vec::new(),
            multiple: false,
            scope_id: None,
            required: false,
            localized: false,
        }
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .collect()
    }

    fn ctda_field(function: u16) -> FieldEntry {
        let mut bytes = vec![0u8; 32];
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes_field("CTDA", bytes)
    }

    fn condition_sequence(record: &Record) -> Vec<String> {
        record
            .fields
            .iter()
            .filter_map(|field| match field.sig.as_str() {
                "CTDA" => match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() >= 10 => {
                        Some(format!("CTDA:{}", u16::from_le_bytes([bytes[8], bytes[9]])))
                    }
                    _ => Some("CTDA:?".to_string()),
                },
                "BTXT" | "CIS1" | "CIS2" | "ITXT" => Some(field.sig.as_str().to_string()),
                _ => None,
            })
            .collect()
    }

    fn cursor_order_errors(record: &Record) -> Vec<String> {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let record_def = target_schema
            .record_def(record.sig.as_str())
            .expect("target record def");
        let mut cursor = 0usize;
        let mut errors = Vec::new();

        for field in &record.fields {
            let sig = field.sig.as_str();
            let forward_pos = record_def
                .subrecords
                .get(cursor..)
                .and_then(|tail| tail.iter().position(|spec| spec.id == sig))
                .map(|offset| cursor + offset);

            if let Some(pos) = forward_pos {
                if !record_def.subrecords[pos].multiple {
                    cursor = pos + 1;
                } else {
                    cursor = pos;
                }
            } else {
                errors.push(sig.to_string());
            }
        }

        errors
    }

    fn assert_cursor_accepts(record: &Record) {
        let errors = cursor_order_errors(record);
        assert!(
            errors.is_empty(),
            "record order should satisfy target cursor, errors={errors:?}, sigs={:?}",
            sigs(record)
        );
    }

    fn assert_race_outer_cursor_accepts(record: &Record) {
        let mut outer = record.clone();
        outer.fields.retain(|entry| {
            !matches!(
                &entry.sig.0,
                b"SAKD" | b"STKD" | b"SGNM" | b"SAPT" | b"SRAF" | b"FMRI" | b"FMRN"
            )
        });
        assert_cursor_accepts(&outer);
    }

    fn sigs_from<'a>(record: &'a Record, start_sig: &str) -> Vec<&'a str> {
        let all = sigs(record);
        let start = all
            .iter()
            .position(|sig| *sig == start_sig)
            .expect("start sig should be present");
        all[start..].to_vec()
    }

    fn race_fields_before_behavior_graph(editor_id: &[u8]) -> Vec<FieldEntry> {
        vec![
            bytes_field("EDID", editor_id.to_vec()),
            bytes_field("MNAM", Vec::new()),
            bytes_field(
                "ANAM",
                b"actors\\Character\\CharacterAssets\\skeleton.nif\0".to_vec(),
            ),
            bytes_field("MODT", vec![1]),
            bytes_field("FNAM", Vec::new()),
            bytes_field(
                "ANAM",
                b"actors\\Character\\CharacterAssets\\skeleton.nif\0".to_vec(),
            ),
            bytes_field("MODT", vec![2]),
            bytes_field("NAM1", Vec::new()),
            bytes_field("MNAM", Vec::new()),
            bytes_field("INDX", vec![0; 4]),
            bytes_field(
                "MODL",
                b"Actors\\Character\\UpperBodyHumanMale.egt\0".to_vec(),
            ),
            bytes_field("MODT", vec![3]),
            bytes_field("FNAM", Vec::new()),
            bytes_field("INDX", vec![0; 4]),
            bytes_field(
                "MODL",
                b"Actors\\Character\\UpperBodyHumanFemale.egt\0".to_vec(),
            ),
            bytes_field("MODT", vec![4]),
            bytes_field("GNAM", vec![0; 4]),
        ]
    }

    fn normalize_target_only(record: Record) -> TargetRecordNormalization {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        TargetRecordNormalizer::target_only(&target_schema).normalize(record)
    }

    fn regn_rdat_bytes(type_id: u32) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&type_id.to_le_bytes());
        bytes.extend_from_slice(&[0, 50, 0, 0]);
        bytes
    }

    #[test]
    fn moves_regn_sound_rows_after_scoped_region_data() {
        let interner = StringInterner::new();
        let record = record(
            "REGN",
            vec![
                bytes_field("RDAT", regn_rdat_bytes(2)),
                bytes_field("RDOT", vec![1]),
                bytes_field("RDAT", regn_rdat_bytes(7)),
                bytes_field("RDMO", 0x071096F7_u32.to_le_bytes().to_vec()),
                bytes_field("RDSA", {
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(&0x07519C69_u32.to_le_bytes());
                    bytes.extend_from_slice(&1_u32.to_le_bytes());
                    bytes.extend_from_slice(&0.05_f32.to_le_bytes());
                    bytes
                }),
                bytes_field("RDAT", regn_rdat_bytes(3)),
                bytes_field("RDWT", {
                    let mut bytes = Vec::new();
                    bytes.extend_from_slice(&0x074398AE_u32.to_le_bytes());
                    bytes.extend_from_slice(&150_u32.to_le_bytes());
                    bytes.extend_from_slice(&0_u32.to_le_bytes());
                    bytes
                }),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("REGN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec!["RDAT", "RDOT", "RDAT", "RDWT", "RDAT", "RDMO", "RDSA"]
        );
    }

    #[test]
    fn normalizes_skyrim_weather_whiterun_region_data_to_fo4_order() {
        let interner = StringInterner::new();
        let record = record(
            "REGN",
            vec![
                bytes_field("RDAT", regn_rdat_bytes(7)),
                bytes_field("RDSA", vec![0; 84]),
                bytes_field("RDAT", regn_rdat_bytes(4)),
                bytes_field("RDMP", b"Whiterun\0".to_vec()),
                bytes_field("RDAT", regn_rdat_bytes(3)),
                bytes_field("RDWT", vec![0; 72]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("REGN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec!["RDAT", "RDMP", "RDAT", "RDWT", "RDAT", "RDSA"]
        );
    }

    #[test]
    fn pads_short_race_hclf_to_two_elements() {
        let interner = StringInterner::new();
        // 1-element HCLF (Male only, 4 bytes) — FishermanRace shape.
        let mut rec = record(
            "RACE",
            vec![bytes_field("HCLF", 0x0A0439u32.to_le_bytes().to_vec())],
            &interner,
        );
        pad_race_hclf_to_male_female(&mut rec);
        let hclf = rec
            .fields
            .iter()
            .find(|f| f.sig.as_str() == "HCLF")
            .unwrap();
        let FieldValue::Bytes(b) = &hclf.value else {
            panic!()
        };
        assert_eq!(b.len(), 8, "padded to 2 formids (Male,Female)");
        assert_eq!(&b[0..4], &0x0A0439u32.to_le_bytes(), "Male color preserved");
        assert_eq!(&b[4..8], &0u32.to_le_bytes(), "Female slot is NULL");
    }

    #[test]
    fn leaves_full_and_empty_race_hclf_untouched() {
        let interner = StringInterner::new();
        // 2-element HCLF — already correct, untouched.
        let mut full = record(
            "RACE",
            vec![bytes_field("HCLF", vec![1, 2, 3, 4, 5, 6, 7, 8])],
            &interner,
        );
        pad_race_hclf_to_male_female(&mut full);
        let FieldValue::Bytes(b) = &full.fields[0].value else {
            panic!()
        };
        assert_eq!(b.len(), 8, "2-element HCLF unchanged");

        // HCLF on a non-RACE record — untouched even if 4 bytes.
        let mut other = record(
            "NPC_",
            vec![bytes_field("HCLF", vec![1, 2, 3, 4])],
            &interner,
        );
        pad_race_hclf_to_male_female(&mut other);
        let FieldValue::Bytes(b) = &other.fields[0].value else {
            panic!()
        };
        assert_eq!(b.len(), 4, "non-RACE HCLF not padded");
    }

    fn normalize_target_only_with_interner(
        record: Record,
        interner: &StringInterner,
    ) -> TargetRecordNormalization {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        TargetRecordNormalizer::target_only_with_interner(&target_schema, interner)
            .normalize(record)
    }

    fn normalize_from_fo76_source(
        record: Record,
        source_sig: &str,
        interner: &StringInterner,
    ) -> TargetRecordNormalization {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def(source_sig),
            interner: Some(interner),
        }
        .normalize(record)
    }

    fn terminal_marker_parameters(offset_x: f32, offset_y: f32, offset_z: f32) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(24);
        bytes.extend_from_slice(&offset_x.to_le_bytes());
        bytes.extend_from_slice(&offset_y.to_le_bytes());
        bytes.extend_from_slice(&offset_z.to_le_bytes());
        bytes.extend_from_slice(&0.0_f32.to_le_bytes());
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&[0xff, 1, 0, 0]);
        bytes
    }

    #[test]
    fn routes_fo76_term_marker_parameters_to_second_snam_slot() {
        let interner = StringInterner::new();
        let cases = [
            (
                "improvised",
                Some("Furniture\\Terminals\\TerminalOnImprovised.nif\0"),
                terminal_marker_parameters(-3.336, -67.322, 0.0),
            ),
            (
                "wall",
                Some("Furniture\\Terminals\\TerminalWall01.nif\0"),
                terminal_marker_parameters(0.0, -86.0, 0.0),
            ),
            ("submenu", None, terminal_marker_parameters(0.0, 0.0, 0.0)),
        ];

        for (case, model, marker_parameters) in cases {
            let mut fields = Vec::new();
            if let Some(model) = model {
                fields.push(bytes_field("MODL", model.as_bytes().to_vec()));
            }
            fields.push(bytes_field("FNAM", 0_u16.to_le_bytes().to_vec()));
            fields.push(bytes_field(
                "XMRK",
                b"Markers\\MarkerDeskTerminal01.nif\0".to_vec(),
            ));
            fields.push(bytes_field("ZNAM", marker_parameters.clone()));
            let record = record("TERM", fields, &interner);

            let TargetRecordNormalization::Keep(record) =
                normalize_from_fo76_source(record, "TERM", &interner)
            else {
                panic!("{case} terminal should be supported");
            };

            let snam_positions = record
                .fields
                .iter()
                .enumerate()
                .filter_map(|(index, field)| (field.sig.as_str() == "SNAM").then_some(index))
                .collect::<Vec<_>>();
            assert_eq!(snam_positions.len(), 1, "{case} must not gain a fake sound");
            let xmrk_position = record
                .fields
                .iter()
                .position(|field| field.sig.as_str() == "XMRK")
                .expect("marker model should remain");
            assert!(
                snam_positions[0] > xmrk_position,
                "{case} marker parameters must use the second SNAM slot"
            );
            let FieldValue::Bytes(bytes) = &record.fields[snam_positions[0]].value else {
                panic!("{case} marker parameters should remain bytes");
            };
            assert_eq!(bytes.as_slice(), marker_parameters.as_slice());
        }
    }

    #[test]
    fn preserves_term_looping_sound_before_marker_parameters() {
        let interner = StringInterner::new();
        let sound = FormKey::parse("012345@Fallout4.esm", &interner).unwrap();
        let marker_parameters = terminal_marker_parameters(-3.336, -67.322, 0.0);
        let record = record(
            "TERM",
            vec![
                formkey_field("SNAM", sound),
                bytes_field("FNAM", 0_u16.to_le_bytes().to_vec()),
                bytes_field("XMRK", b"Markers\\MarkerDeskTerminal01.nif\0".to_vec()),
                bytes_field("ZNAM", marker_parameters.clone()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "TERM", &interner)
        else {
            panic!("terminal should be supported");
        };

        let snam = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "SNAM")
            .collect::<Vec<_>>();
        assert_eq!(snam.len(), 2);
        assert_eq!(snam[0].value, FieldValue::FormKey(sound));
        let FieldValue::Bytes(bytes) = &snam[1].value else {
            panic!("second SNAM should be marker parameters");
        };
        assert_eq!(bytes.as_slice(), marker_parameters.as_slice());
        assert!(
            record
                .fields
                .iter()
                .position(|field| std::ptr::eq(field, snam[0]))
                < record
                    .fields
                    .iter()
                    .position(|field| field.sig.as_str() == "FNAM")
        );
        assert!(
            record
                .fields
                .iter()
                .position(|field| std::ptr::eq(field, snam[1]))
                > record
                    .fields
                    .iter()
                    .position(|field| field.sig.as_str() == "XMRK")
        );
    }

    #[test]
    fn term_marker_parameter_normalization_is_idempotent() {
        let interner = StringInterner::new();
        let marker_parameters = terminal_marker_parameters(0.0, -86.0, 0.0);
        let record = record(
            "TERM",
            vec![
                bytes_field("FNAM", 0_u16.to_le_bytes().to_vec()),
                bytes_field("XMRK", b"Markers\\MarkerWallTerminal3rdP.nif\0".to_vec()),
                bytes_field("ZNAM", marker_parameters.clone()),
            ],
            &interner,
        );
        let TargetRecordNormalization::Keep(once) =
            normalize_from_fo76_source(record, "TERM", &interner)
        else {
            panic!("terminal should be supported");
        };
        let TargetRecordNormalization::Keep(twice) =
            normalize_from_fo76_source(once.clone(), "TERM", &interner)
        else {
            panic!("terminal should remain supported");
        };

        assert_eq!(twice.fields, once.fields);
    }

    fn normalize_from_source_game(
        record: Record,
        source_game: &str,
        interner: &StringInterner,
    ) -> Record {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game(source_game).expect("source schema");
        let source_record_def = source_schema.record_def(record.sig.as_str());
        let TargetRecordNormalization::Keep(record) = (TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def,
            interner: Some(interner),
        })
        .normalize(record) else {
            panic!("record should be supported");
        };
        record
    }

    fn raw_field_bytes<'a>(record: &'a Record, sig: &str) -> &'a [u8] {
        let field = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == sig)
            .expect("field should remain");
        let FieldValue::Bytes(bytes) = &field.value else {
            panic!("field should be raw bytes");
        };
        bytes.as_slice()
    }

    fn parsed_record_by_sig<'a>(
        items: &'a [esp_authoring_core::plugin_runtime::ParsedItem],
        sig: &str,
    ) -> Option<&'a esp_authoring_core::plugin_runtime::ParsedRecord> {
        for item in items {
            match item {
                esp_authoring_core::plugin_runtime::ParsedItem::Record(record)
                    if record.signature.as_str() == sig =>
                {
                    return Some(record);
                }
                esp_authoring_core::plugin_runtime::ParsedItem::Group(group) => {
                    if let Some(record) = parsed_record_by_sig(&group.children, sig) {
                        return Some(record);
                    }
                }
                _ => {}
            }
        }
        None
    }

    #[test]
    fn fo76_idlm_idle_animation_scope_survives_translation_and_normalization() {
        let interner = StringInterner::new();
        let mut record = Record::new(
            SigCode::from_str("IDLM").unwrap(),
            FormKey::parse("4F6D07@SeventySix.esm", &interner).unwrap(),
        );
        record.fields.extend([
            uint_field("IDLF", 4),
            uint_field("IDLC", 1),
            FieldEntry {
                sig: SubrecordSig::from_str("IDLT").unwrap(),
                value: FieldValue::Float(0.0),
            },
            FieldEntry {
                sig: SubrecordSig::from_str("IDLA").unwrap(),
                value: FieldValue::List(vec![FieldValue::FormKey(
                    FormKey::parse("512DA1@SeventySix.esm", &interner).unwrap(),
                )]),
            },
        ]);

        let translator = crate::translator::Translator::new(
            crate::translator::Game::Fo76,
            crate::translator::Game::Fo4,
        )
        .unwrap();
        let translated = match translator.translate(&record, &interner) {
            crate::translator::TranslateResult::Translated(record) => record,
            other => panic!("expected translated IDLM, got {other:?}"),
        };
        let TargetRecordNormalization::Keep(normalized) =
            normalize_from_fo76_source(translated, "IDLM", &interner)
        else {
            panic!("IDLM should be supported");
        };

        assert_eq!(sigs(&normalized), vec!["IDLF", "IDLC", "IDLT", "IDLA"]);
    }

    fn f32_bytes(values: &[f32]) -> Vec<u8> {
        let mut out = Vec::with_capacity(values.len() * 4);
        for value in values {
            out.extend_from_slice(&value.to_le_bytes());
        }
        out
    }

    #[test]
    fn drops_record_without_target_schema() {
        let interner = StringInterner::new();
        let record = record("ATXO", Vec::new(), &interner);

        let result = normalize_target_only(record);

        assert!(matches!(
            result,
            TargetRecordNormalization::DropUnsupportedRecord
        ));
    }

    #[test]
    fn converts_fo76_imgs_enam_to_fo4_hdr_for_clear_weather() {
        let interner = StringInterner::new();
        let record = record(
            "IMGS",
            vec![bytes_field(
                "ENAM",
                f32_bytes(&[
                    2.0, 0.02, 0.5, 0.0, 6.0, 2.0, 300.0, 100.0, 0.18, 1.0, 11.2, 1.0, 1.0,
                ]),
            )],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "IMGS", &interner)
        else {
            panic!("IMGS should be supported");
        };

        assert_eq!(sigs(&record), vec!["HNAM"]);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("HNAM should be raw bytes");
        };
        let values = decode_f32_prefix(bytes.as_slice());
        assert_eq!(values.len(), 9);
        assert_eq!(values[3], 0.2);
        assert_eq!(values[4], 6.0);
        assert_eq!(values[5], 2.0);
        assert_eq!(values[6], 4.5);
        assert_eq!(values[7], 2.4);
        assert!((values[8] - 0.18).abs() < 0.0001);
    }

    #[test]
    fn converts_fo76_imgs_enam_to_fo4_hdr_without_zeroing_interiors() {
        let interner = StringInterner::new();
        let record = record(
            "IMGS",
            vec![bytes_field(
                "ENAM",
                f32_bytes(&[
                    8.0, 0.02, 0.5, 0.0, 4.5, 2.5, 0.0, 100.0, 0.18, 0.3, 11.2, 2.0, 1.0,
                ]),
            )],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "IMGS", &interner)
        else {
            panic!("IMGS should be supported");
        };

        assert_eq!(sigs(&record), vec!["HNAM"]);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("HNAM should be raw bytes");
        };
        let values = decode_f32_prefix(bytes.as_slice());
        assert_eq!(values.len(), 9);
        assert_eq!(values[3], 0.1);
        assert_eq!(values[4], 4.5);
        assert_eq!(values[5], 2.5);
        assert_eq!(values[6], 1.8);
        assert_eq!(values[7], 1.5);
        assert!((values[8] - 0.18).abs() < 0.0001);
    }

    #[test]
    fn converts_fo76_imgs_fnam_to_fo4_hdr() {
        let interner = StringInterner::new();
        let record = record(
            "IMGS",
            vec![bytes_field(
                "FNAM",
                f32_bytes(&[2.0, 0.02, 0.5, 0.2, 16.0, 0.0, 1.0, 1.0, 0.18, 0.15, 11.2]),
            )],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "IMGS", &interner)
        else {
            panic!("IMGS should be supported");
        };

        assert_eq!(sigs(&record), vec!["HNAM"]);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("HNAM should be raw bytes");
        };
        let values = decode_f32_prefix(bytes.as_slice());
        assert_eq!(values.len(), 9);
        assert_eq!(values[0], 2.0);
        assert_eq!(values[4], 16.0);
    }

    #[test]
    fn converts_fo76_imgs_gnam_to_fo4_hdr() {
        let interner = StringInterner::new();
        let record = record(
            "IMGS",
            vec![bytes_field(
                "GNAM",
                f32_bytes(&[5.0, 0.025, 0.65, 0.25, 16.0, 0.0, 2.0, 2.0, 0.2]),
            )],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "IMGS", &interner)
        else {
            panic!("IMGS should be supported");
        };

        assert_eq!(sigs(&record), vec!["HNAM"]);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("HNAM should be raw bytes");
        };
        let values = decode_f32_prefix(bytes.as_slice());
        assert_eq!(values.len(), 9);
        assert_eq!(values[0], 5.0);
        assert_eq!(values[4], 16.0);
    }

    #[test]
    fn strips_fo76_imgs_lut_data_textures_prefix() {
        let interner = StringInterner::new();
        let record = record(
            "IMGS",
            vec![string_field(
                "TX00",
                "Data\\Textures\\Effects\\LUTs\\newlut_clear_dawnl02.dds",
                &interner,
            )],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "IMGS", &interner)
        else {
            panic!("IMGS should be supported");
        };

        assert_eq!(sigs(&record), vec!["TX00"]);
        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("TX00 should stay a string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Effects\\LUTs\\newlut_clear_dawnl02.dds")
        );
    }

    #[test]
    fn drops_subrecords_not_in_target_schema() {
        let interner = StringInterner::new();
        let record = record(
            "STAT",
            vec![
                bytes_field("EDID", b"Test\0".to_vec()),
                bytes_field("DEFL", vec![1, 2, 3, 4]),
                bytes_field("MODL", b"Model.nif\0".to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("STAT should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "MODL"]);
    }

    #[test]
    fn fo4_keym_schema_keeps_name_preview_and_sounds() {
        let interner = StringInterner::new();
        let record = record(
            "KEYM",
            vec![
                bytes_field("ZNAM", 0x0059_5D2C_u32.to_le_bytes().to_vec()),
                bytes_field("FULL", b"Congressional Access Card\0".to_vec()),
                bytes_field("YNAM", 0x0059_5D2B_u32.to_le_bytes().to_vec()),
                bytes_field("PTRN", 0x0024_8895_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("KEYM should be supported");
        };

        assert_eq!(sigs(&record), vec!["PTRN", "FULL", "YNAM", "ZNAM"]);
    }

    #[test]
    fn normalizes_fo76_cont_data_flags_and_preserves_weight() {
        let interner = StringInterner::new();
        let weight = 42.5_f32.to_le_bytes();
        let mut data = vec![0x3f];
        data.extend_from_slice(&weight);
        let record = record("CONT", vec![bytes_field("DATA", data)], &interner);

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "CONT", &interner)
        else {
            panic!("CONT should be supported");
        };

        let data = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DATA")
            .expect("CONT.DATA should remain");
        let FieldValue::Bytes(bytes) = &data.value else {
            panic!("CONT.DATA should remain raw bytes");
        };
        assert_eq!(bytes[0], FO4_CONT_DATA_FLAGS);
        assert_eq!(&bytes[1..], &weight);
    }

    #[test]
    fn reorders_fields_to_target_schema_order() {
        let interner = StringInterner::new();
        let record = record(
            "WEAP",
            vec![
                bytes_field("EDID", b"Weapon\0".to_vec()),
                bytes_field("CRDT", vec![0; 16]),
                bytes_field("INAM", vec![0; 4]),
                bytes_field("DNAM", vec![0; 4]),
                bytes_field("FNAM", vec![0; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("WEAP should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "DNAM", "FNAM", "CRDT", "INAM"]);
    }

    #[test]
    fn drops_empty_imad_runtime_unsafe_subrecords() {
        let interner = StringInterner::new();
        let record = record(
            "IMAD",
            vec![
                bytes_field("EDID", b"ImageSpace\0".to_vec()),
                bytes_field("DNAM", vec![0; 252]),
                bytes_field("WNAM", vec![1; 8]),
                bytes_field("NAM5", Vec::new()),
                bytes_field("NAM6", Vec::new()),
                bytes_field("NAM4", vec![2; 8]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("IMAD should be supported");
        };

        let sigs = sigs(&record);
        assert!(!sigs.contains(&"NAM5"));
        assert!(!sigs.contains(&"NAM6"));
        assert!(sigs.contains(&"NAM4"));
    }

    #[test]
    fn preserves_navm_mnam_nested_counted_triangle_tail() {
        let interner = StringInterner::new();
        let mut mnam = Vec::new();
        mnam.extend_from_slice(&0x0010_9A00_u32.to_le_bytes());
        mnam.extend_from_slice(&14_u16.to_le_bytes());
        for triangle in 0..14_u16 {
            mnam.extend_from_slice(&triangle.to_le_bytes());
        }
        assert_eq!(mnam.len(), 34);

        let record = record("NAVM", vec![bytes_field("MNAM", mnam.clone())], &interner);

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "NAVM", &interner)
        else {
            panic!("NAVM should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("MNAM should remain raw bytes");
        };
        assert_eq!(bytes.as_slice(), mnam.as_slice());
    }

    #[test]
    fn reorders_note_obnd_to_ck_slot() {
        let interner = StringInterner::new();
        let record = record(
            "NOTE",
            vec![
                bytes_field("EDID", b"Note\0".to_vec()),
                bytes_field("PTRN", vec![0; 4]),
                bytes_field("FULL", vec![0; 4]),
                bytes_field("MODL", b"model.nif\0".to_vec()),
                bytes_field("MODT", vec![1, 2, 3]),
                bytes_field("DNAM", vec![3]),
                bytes_field("DATA", vec![0; 8]),
                bytes_field("SNAM", vec![0; 4]),
                bytes_field("OBND", vec![0; 12]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("NOTE should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "OBND", "PTRN", "FULL", "MODL", "MODT", "DNAM", "DATA", "SNAM"
            ]
        );
    }

    #[test]
    fn keeps_lvli_lvlo_block_before_onam() {
        let interner = StringInterner::new();
        let record = record(
            "LVLI",
            vec![
                bytes_field("EDID", b"List\0".to_vec()),
                bytes_field("OBND", vec![0; 12]),
                bytes_field("LLCT", vec![2]),
                bytes_field("ONAM", vec![0; 4]),
                bytes_field("LVLO", vec![0; 12]),
                bytes_field("LVLO", vec![1; 12]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("LVLI should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec!["EDID", "OBND", "LLCT", "LVLO", "LVLO", "ONAM"]
        );
    }

    #[test]
    fn keeps_pack_package_data_after_pkcu_and_syncs_count() {
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("PKCU", vec![0; 12]),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("CNAM", vec![1]),
                bytes_field("PDTO", vec![0; 8]),
                bytes_field("UNAM", vec![0]),
                bytes_field("ANAM", b"Int\0".to_vec()),
                bytes_field("CNAM", 7_u32.to_le_bytes().to_vec()),
                bytes_field("UNAM", vec![1]),
                bytes_field("XNAM", Vec::new()),
                bytes_field("POBA", Vec::new()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "PKCU", "ANAM", "CNAM", "PDTO", "UNAM", "ANAM", "CNAM", "UNAM", "XNAM",
                "POBA"
            ]
        );
        let pkcu = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "PKCU")
            .expect("PKCU");
        match &pkcu.value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &2_u32.to_le_bytes()),
            other => panic!("expected PKCU bytes, got {other:?}"),
        }
    }

    #[test]
    fn keeps_condition_strings_with_original_ctda_row() {
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("CTDA", vec![0; 32]),
                bytes_field("CTDA", vec![1; 32]),
                bytes_field("CIS1", b"Default2StateActivator\0".to_vec()),
                bytes_field("CIS2", b"::IsOpen_var\0".to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        let condition_sigs: Vec<&str> = record
            .fields
            .iter()
            .map(|field| field.sig.as_str())
            .filter(|sig| matches!(*sig, "CTDA" | "CIS1" | "CIS2"))
            .collect();
        assert_eq!(condition_sigs, vec!["CTDA", "CTDA", "CIS1", "CIS2"]);
    }

    #[test]
    fn keeps_terminal_menu_condition_strings_with_original_ctda_row() {
        let interner = StringInterner::new();
        let record = record(
            "TERM",
            vec![
                bytes_field("EDID", b"Terminal\0".to_vec()),
                bytes_field("PNAM", vec![0; 4]),
                bytes_field("BSIZ", 2_u32.to_le_bytes().to_vec()),
                bytes_field("BTXT", 11_u32.to_le_bytes().to_vec()),
                ctda_field(46),
                bytes_field("BTXT", 12_u32.to_le_bytes().to_vec()),
                ctda_field(46),
                bytes_field("ISIZ", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ITXT", 1_u32.to_le_bytes().to_vec()),
                bytes_field("RNAM", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", vec![8]),
                bytes_field("UNAM", 3_u32.to_le_bytes().to_vec()),
                ctda_field(660),
                bytes_field("CIS1", b"Default2StateActivator\0".to_vec()),
                bytes_field("CIS2", b"::OpenState_var\0".to_vec()),
                ctda_field(46),
                bytes_field("ITXT", 4_u32.to_le_bytes().to_vec()),
                bytes_field("RNAM", 5_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", vec![8]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("TERM should be supported");
        };

        let sequence = condition_sequence(&record);
        assert!(
            sequence
                .windows(4)
                .any(|window| window == ["CTDA:660", "CIS1", "CIS2", "CTDA:46"]),
            "CIS1/CIS2 should stay with the CTDA 660 row, got {sequence:?}"
        );
        assert!(
            !sequence
                .windows(3)
                .any(|window| window == ["CTDA:46", "CIS1", "CIS2"]),
            "body-text CTDA rows should not consume menu-item strings, got {sequence:?}"
        );
    }

    #[test]
    fn last_body_text_does_not_absorb_menu_item_conditions() {
        // Regression: a TERM whose LAST body text has zero conditions (a
        // fallback "error" text) followed by menu items that DO have conditions.
        // The last body-text anchor previously swept every trailing CTDA up to
        // usize::MAX, moving all menu-item conditions onto the body text and
        // leaving the items with none (the "conditions moved to the body" bug).
        let interner = StringInterner::new();
        let record = record(
            "TERM",
            vec![
                bytes_field("EDID", b"Terminal\0".to_vec()),
                bytes_field("BSIZ", 2_u32.to_le_bytes().to_vec()),
                bytes_field("BTXT", 11_u32.to_le_bytes().to_vec()),
                ctda_field(100), // body text 1 condition
                bytes_field("BTXT", 12_u32.to_le_bytes().to_vec()), // last body text: NO conditions
                bytes_field("ISIZ", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ITXT", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", vec![8]),
                ctda_field(200), // menu item 1 condition
                bytes_field("ITXT", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", vec![8]),
                ctda_field(300), // menu item 2 condition
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("TERM should be supported");
        };

        let sequence = condition_sequence(&record);
        assert_eq!(
            sequence,
            vec![
                "BTXT", "CTDA:100", "BTXT", "ITXT", "CTDA:200", "ITXT", "CTDA:300",
            ],
            "each condition must stay with its own body text / menu item, got {sequence:?}",
        );
    }

    #[test]
    fn keeps_pack_package_data_rows_without_cnam_and_syncs_count() {
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("PKCU", 9_u32.to_le_bytes().repeat(3)),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("PDTO", vec![0; 8]),
                bytes_field("PTDA", vec![0; 12]),
                bytes_field("UNAM", vec![0]),
                bytes_field("XNAM", Vec::new()),
                bytes_field("POBA", Vec::new()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "PKCU", "ANAM", "PDTO", "PTDA", "UNAM", "XNAM", "POBA"
            ]
        );
        let pkcu = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "PKCU")
            .expect("PKCU");
        match &pkcu.value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &1_u32.to_le_bytes()),
            other => panic!("expected PKCU bytes, got {other:?}"),
        }
    }

    #[test]
    fn keeps_null_inam_in_pack_procedure_tree_branches() {
        // FO76 carries a null INAM (IDLE FK) on each POBA/POEA/POCA branch; it
        // decodes to FieldValue::None. It MUST survive — dropping it yields FO4
        // "missing procedure tree item". Verifies the structural-marker guard.
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("PKCU", vec![0; 12]),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("UNAM", vec![0]),
                bytes_field("XNAM", Vec::new()),
                bytes_field("POBA", Vec::new()),
                none_field("INAM"),
                bytes_field("PDTO", vec![0; 8]),
                bytes_field("POEA", Vec::new()),
                none_field("INAM"),
                bytes_field("PDTO", vec![0; 8]),
                bytes_field("POCA", Vec::new()),
                none_field("INAM"),
                bytes_field("PDTO", vec![0; 8]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        let s = sigs(&record);
        assert_eq!(
            s.iter().filter(|sig| **sig == "INAM").count(),
            3,
            "all three branch INAMs must be retained, got {s:?}"
        );
        for window in s.windows(2) {
            if matches!(window[0], "POBA" | "POEA" | "POCA") {
                assert_eq!(
                    window[1], "INAM",
                    "each branch marker must be followed by INAM, got {s:?}"
                );
            }
        }
    }

    #[test]
    fn keeps_pack_package_data_cnam_row_local() {
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("PKCU", vec![0; 12]),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("PDTO", vec![0; 8]),
                bytes_field("UNAM", vec![0]),
                bytes_field("ANAM", b"Int\0".to_vec()),
                bytes_field("CNAM", 7_u32.to_le_bytes().to_vec()),
                bytes_field("PDTO", vec![1; 8]),
                bytes_field("UNAM", vec![1]),
                bytes_field("XNAM", Vec::new()),
                bytes_field("ANAM", b"Root\0".to_vec()),
                bytes_field("POBA", Vec::new()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "PKCU", "ANAM", "PDTO", "UNAM", "ANAM", "CNAM", "PDTO", "UNAM", "XNAM",
                "ANAM", "POBA"
            ]
        );
        let pkcu = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "PKCU")
            .expect("PKCU");
        match &pkcu.value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &2_u32.to_le_bytes()),
            other => panic!("expected PKCU bytes, got {other:?}"),
        }
    }

    #[test]
    fn preserves_pack_package_data_trailing_indices_before_xnam() {
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("PKCU", vec![0; 12]),
                bytes_field("ANAM", b"SingleRef\0".to_vec()),
                bytes_field("PTDA", vec![0; 12]),
                bytes_field("ANAM", b"Float\0".to_vec()),
                bytes_field("CNAM", 0_f32.to_le_bytes().to_vec()),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("CNAM", vec![0]),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("CNAM", vec![0]),
                bytes_field("ANAM", b"TargetSelector\0".to_vec()),
                bytes_field("PTDA", vec![0; 12]),
                bytes_field("ANAM", b"Bool\0".to_vec()),
                bytes_field("CNAM", vec![1]),
                bytes_field("UNAM", vec![0]),
                bytes_field("UNAM", vec![1]),
                bytes_field("UNAM", vec![2]),
                bytes_field("UNAM", vec![4]),
                bytes_field("UNAM", vec![6]),
                bytes_field("UNAM", vec![8]),
                bytes_field("XNAM", vec![9]),
                bytes_field("POBA", Vec::new()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "PKCU", "ANAM", "PTDA", "ANAM", "CNAM", "ANAM", "CNAM", "ANAM", "CNAM",
                "ANAM", "PTDA", "ANAM", "CNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM", "UNAM",
                "XNAM", "POBA"
            ]
        );
        let pkcu = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "PKCU")
            .expect("PKCU");
        match &pkcu.value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &6_u32.to_le_bytes()),
            other => panic!("expected PKCU bytes, got {other:?}"),
        }
    }

    #[test]
    fn preserves_pack_procedure_tree_repeatable_nodes_in_original_order() {
        let interner = StringInterner::new();
        let record = record(
            "PACK",
            vec![
                bytes_field("EDID", b"Package\0".to_vec()),
                bytes_field("PKCU", vec![0; 12]),
                bytes_field("XNAM", vec![0]),
                bytes_field("ANAM", b"Procedure\0".to_vec()),
                bytes_field("PRCB", vec![0; 8]),
                bytes_field("PNAM", b"Travel\0".to_vec()),
                bytes_field("FNAM", vec![0; 4]),
                bytes_field("PNAM", b"Patrol\0".to_vec()),
                bytes_field("FNAM", vec![1; 4]),
                bytes_field("PKC2", vec![2]),
                bytes_field("PFO2", vec![3; 16]),
                bytes_field("UNAM", vec![0]),
                bytes_field("BNAM", b"Input\0".to_vec()),
                bytes_field("PNAM", vec![4; 4]),
                bytes_field("POBA", Vec::new()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("PACK should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "PKCU", "XNAM", "ANAM", "PRCB", "PNAM", "FNAM", "PNAM", "FNAM", "PKC2",
                "PFO2", "UNAM", "BNAM", "PNAM", "POBA"
            ]
        );
        let pnam_payloads: Vec<Vec<u8>> = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "PNAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.to_vec(),
                other => panic!("expected raw PNAM bytes, got {other:?}"),
            })
            .collect();
        assert_eq!(
            pnam_payloads,
            vec![b"Travel\0".to_vec(), b"Patrol\0".to_vec(), vec![4; 4]]
        );
    }

    #[test]
    fn trims_subrecord_union_bytes_to_target_variant_size() {
        let interner = StringInterner::new();
        let original = vec![0xA5; 158];
        let record = record(
            "EFSH",
            vec![bytes_field("DNAM", original.clone())],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("EFSH should be supported");
        };

        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let expected_size = target_schema
            .record_def("EFSH")
            .and_then(|record_def| record_def.subrecord_def("DNAM"))
            .and_then(|subrecord_def| fixed_size_hint(None, subrecord_def))
            .expect("EFSH.DNAM fixed union size");
        let dnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DNAM")
            .expect("DNAM");
        match &dnam.value {
            FieldValue::Bytes(bytes) => {
                assert_eq!(expected_size, 157);
                assert_eq!(bytes.len(), expected_size);
                assert_eq!(bytes.as_slice(), &original[..expected_size]);
            }
            other => panic!("expected DNAM bytes, got {other:?}"),
        }
    }

    #[test]
    fn keeps_target_sized_record_form_version_union_bytes_idempotently() {
        let interner = StringInterner::new();
        let original = (0..157).map(|index| index as u8).collect::<Vec<_>>();
        let record = record(
            "EFSH",
            vec![bytes_field("DNAM", original.clone())],
            &interner,
        );

        let TargetRecordNormalization::Keep(first) = normalize_target_only(record) else {
            panic!("EFSH should be supported");
        };
        let TargetRecordNormalization::Keep(second) = normalize_target_only(first.clone()) else {
            panic!("EFSH should remain supported");
        };

        let bytes = |record: &Record| {
            record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "DNAM")
                .and_then(|field| match &field.value {
                    FieldValue::Bytes(bytes) => Some(bytes.to_vec()),
                    _ => None,
                })
                .expect("raw DNAM bytes")
        };
        assert_eq!(bytes(&first), original);
        assert_eq!(bytes(&second), original);
    }

    #[test]
    fn keeps_non_version_heterogeneous_union_variant_width() {
        let interner = StringInterner::new();
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let target_record_def = target_schema.record_def("SNDR").expect("SNDR schema");
        let bnam_def = target_record_def.subrecord_def("BNAM").expect("SNDR.BNAM");
        assert_eq!(subrecord_union_variant_sizes(bnam_def).as_slice(), &[6, 4]);
        assert_eq!(
            target_form_version_union_fixed_size(&target_schema, "SNDR", bnam_def),
            None
        );

        let original = vec![1, 2, 3, 4, 5, 6];
        let mut record = record(
            "SNDR",
            vec![bytes_field("BNAM", original.clone())],
            &interner,
        );
        normalize_target_form_version_union_bytes(&mut record, &target_schema, target_record_def);
        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("BNAM should remain raw bytes");
        };
        assert_eq!(bytes.as_slice(), original);
        assert_eq!(normalize_raw_bytes(bytes, None, bnam_def), original);
    }

    #[test]
    fn drops_empty_optional_fixed_size_subrecord() {
        let interner = StringInterner::new();
        let record = record("REFR", vec![none_field("INAM")], &interner);

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("REFR should be supported");
        };

        assert!(sigs(&record).is_empty());
    }

    #[test]
    fn assigns_duplicate_weap_full_to_distinct_schema_slots() {
        let interner = StringInterner::new();
        let record = record(
            "WEAP",
            vec![
                bytes_field("EDID", b"Weapon\0".to_vec()),
                bytes_field("FULL", b"Weapon Name\0".to_vec()),
                bytes_field("FULL", b"Template Name\0".to_vec()),
                bytes_field("KSIZ", 1_u32.to_le_bytes().to_vec()),
                bytes_field("KWDA", 0x0000_1234_u32.to_le_bytes().to_vec()),
                bytes_field("DESC", b"Description\0".to_vec()),
                bytes_field("INRD", vec![0; 4]),
                bytes_field("APPR", vec![0; 4]),
                bytes_field("OBTE", vec![0; 4]),
                bytes_field("OBTF", vec![0; 4]),
                bytes_field("OBTS", vec![0; 4]),
                bytes_field("STOP", vec![0; 4]),
                bytes_field("MOD4", b"Model.nif\0".to_vec()),
                bytes_field("MO4T", vec![0; 4]),
                bytes_field("DNAM", vec![0; 132]),
                bytes_field("FNAM", vec![0; 41]),
                bytes_field("CRDT", vec![0; 16]),
                bytes_field("INAM", vec![0; 4]),
                bytes_field("MASE", vec![0; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("WEAP should be supported");
        };

        let sigs = sigs(&record);
        let full_positions = sigs
            .iter()
            .enumerate()
            .filter_map(|(index, sig)| (*sig == "FULL").then_some(index))
            .collect::<Vec<_>>();
        let ksiz_pos = sigs.iter().position(|sig| *sig == "KSIZ").unwrap();
        let obts_pos = sigs.iter().position(|sig| *sig == "OBTS").unwrap();
        assert_eq!(full_positions.len(), 2);
        assert!(full_positions[0] < ksiz_pos);
        assert!(ksiz_pos < full_positions[1]);
        assert!(full_positions[1] < obts_pos);
        assert_cursor_accepts(&record);
    }

    #[test]
    fn preserves_object_template_rows_and_syncs_count() {
        let interner = StringInterner::new();
        let mut fields = vec![
            bytes_field("EDID", b"Armor\0".to_vec()),
            bytes_field("FULL", b"Armor Name\0".to_vec()),
            bytes_field("OBTE", 99_u32.to_le_bytes().to_vec()),
        ];
        for index in 0..5 {
            fields.push(bytes_field("OBTF", Vec::new()));
            fields.push(bytes_field(
                "FULL",
                format!("Template {index}\0").into_bytes(),
            ));
            fields.push(bytes_field("OBTS", vec![index as u8; 36]));
        }
        fields.push(bytes_field("STOP", Vec::new()));
        let record = record("ARMO", fields, &interner);

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("ARMO should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "FULL", "OBTE", "OBTF", "FULL", "OBTS", "OBTF", "FULL", "OBTS", "OBTF",
                "FULL", "OBTS", "OBTF", "FULL", "OBTS", "OBTF", "FULL", "OBTS", "STOP"
            ]
        );
        let obte = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "OBTE")
            .expect("OBTE");
        match &obte.value {
            FieldValue::Bytes(bytes) => assert_eq!(&bytes[..4], &5_u32.to_le_bytes()),
            other => panic!("expected OBTE bytes, got {other:?}"),
        }
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "OBTS")
                .count(),
            5
        );
    }

    #[test]
    fn assigns_repeated_race_model_sections_to_later_duplicate_slots() {
        let interner = StringInterner::new();
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"Race\0".to_vec()),
                bytes_field("MNAM", b"male\0".to_vec()),
                bytes_field("ANAM", b"male1\0".to_vec()),
                bytes_field("ANAM", b"male2\0".to_vec()),
                bytes_field("MODT", vec![1]),
                bytes_field("MODT", vec![2]),
                bytes_field("FNAM", b"face\0".to_vec()),
                bytes_field("MTNM", b"morph\0".to_vec()),
                bytes_field("VTCK", vec![0; 4]),
                bytes_field("TINL", vec![0; 4]),
                bytes_field("PNAM", vec![0; 4]),
                bytes_field("UNAM", vec![0; 4]),
                bytes_field("ATKD", vec![0; 44]),
                bytes_field("ATKE", vec![0; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "MNAM", "ANAM", "MODT", "FNAM", "ANAM", "MODT", "MTNM", "VTCK", "TINL",
                "PNAM", "UNAM", "ATKD", "ATKE"
            ]
        );
        assert_cursor_accepts(&record);
    }

    #[test]
    fn preserves_race_head_rows_and_alternating_face_morph_names() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"FishermanRace\0".to_vec()),
                none_field("NAM0"),
                none_field("MNAM"),
                formkey_field(
                    "HEAD",
                    FormKey {
                        local: 0x0010_0001,
                        plugin: output,
                    },
                ),
                none_field("NAM0"),
                none_field("FNAM"),
                formkey_field(
                    "HEAD",
                    FormKey {
                        local: 0x0010_0002,
                        plugin: output,
                    },
                ),
                bytes_field("FMRI", 10_u32.to_le_bytes().to_vec()),
                string_field("FMRN", "NoseShort", &interner),
                bytes_field("FMRI", 11_u32.to_le_bytes().to_vec()),
                string_field("FMRN", "NoseLong", &interner),
                bytes_field("PTOP", 1.0_f32.to_le_bytes().to_vec()),
                bytes_field("NTOP", 2.0_f32.to_le_bytes().to_vec()),
                bytes_field("MSID", 1_u32.to_le_bytes().to_vec()),
                string_field("MSM0", "NoseDown", &interner),
                string_field("MSM1", "NoseUp", &interner),
                bytes_field("MLSI", 1_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "RACE", &interner)
        else {
            panic!("RACE should be supported");
        };
        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("RACE should survive final target normalization");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "NAM0", "MNAM", "NAM0", "FNAM", "PTOP", "NTOP", "MSID", "MSM0", "MSM1",
                "MLSI", "HEAD", "HEAD", "FMRI", "FMRN", "FMRI", "FMRN",
            ]
        );
        assert_race_outer_cursor_accepts(&record);
    }

    #[test]
    fn canonicalizes_fisherman_and_shielded_style_race_late_rows_idempotently() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"ShieldedSuperMutantRace\0".to_vec()),
                none_field("NAM0"),
                none_field("MNAM"),
                bytes_field("INDX", 0_u32.to_le_bytes().to_vec()),
                formkey_field(
                    "HEAD",
                    FormKey {
                        local: 0x0010_0001,
                        plugin: output,
                    },
                ),
                bytes_field("FTSM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("DFTM", 2_u32.to_le_bytes().to_vec()),
                string_field("WMAP", "Textures\\Actors\\MaleWrinkle.dds", &interner),
                none_field("NAM0"),
                none_field("FNAM"),
                bytes_field("INDX", 1_u32.to_le_bytes().to_vec()),
                formkey_field(
                    "HEAD",
                    FormKey {
                        local: 0x0010_0002,
                        plugin: output,
                    },
                ),
                string_field("SGNM", "Actors\\Supermutant\\Behavior.hkx", &interner),
                string_field("SAPT", "Actors\\Supermutant\\Animations", &interner),
                bytes_field("SRAF", vec![0; 4]),
                bytes_field("PTOP", 1.0_f32.to_le_bytes().to_vec()),
                bytes_field("NTOP", 2.0_f32.to_le_bytes().to_vec()),
                bytes_field("MSID", 1_u32.to_le_bytes().to_vec()),
                string_field("MSM0", "NoseDown", &interner),
                string_field("MSM1", "NoseUp", &interner),
                bytes_field("MLSI", 1_u32.to_le_bytes().to_vec()),
                uint_field("BSMP", 0),
                string_field("BSMB", "MaleScale", &interner),
                bytes_field("BSMS", vec![0; 36]),
                uint_field("BMMP", 0),
                string_field("BSMB", "MaleRange", &interner),
                bytes_field("BSMS", vec![0; 16]),
                bytes_field("FMRI", 10_u32.to_le_bytes().to_vec()),
                string_field("FMRN", "NoseShort", &interner),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(first) =
            normalize_from_fo76_source(record, "RACE", &interner)
        else {
            panic!("RACE should survive");
        };
        let TargetRecordNormalization::Keep(second) =
            normalize_target_only_with_interner(first.clone(), &interner)
        else {
            panic!("RACE should remain stable");
        };

        assert_eq!(
            sigs(&first),
            vec![
                "EDID", "NAM0", "MNAM", "FTSM", "DFTM", "WMAP", "NAM0", "FNAM", "SGNM", "SAPT",
                "SRAF", "PTOP", "NTOP", "MSID", "MSM0", "MSM1", "MLSI", "BSMP", "BSMB", "BSMS",
                "BMMP", "BSMB", "BSMS", "HEAD", "HEAD", "FMRI", "FMRN",
            ]
        );
        assert_eq!(second.fields, first.fields);
        assert_race_outer_cursor_accepts(&first);
    }

    #[test]
    fn preserves_race_bsms_scale_and_range_layouts_across_gender_sets() {
        let interner = StringInterner::new();
        let male_scale = f32_bytes(&[1.0, 1.1, 1.2, 1.3, 1.4, 1.5, 1.6, 1.7, 1.8]);
        let male_range = f32_bytes(&[-0.1, -0.2, 0.3, 0.4]);
        let female_scale = f32_bytes(&[2.0, 2.1, 2.2, 2.3, 2.4, 2.5, 2.6, 2.7, 2.8]);
        let female_range = f32_bytes(&[-0.5, -0.6, 0.7, 0.8]);
        let record = record(
            "RACE",
            vec![
                none_field("NAM0"),
                uint_field("BSMP", 0),
                string_field("BSMB", "MaleScale", &interner),
                bytes_field("BSMS", male_scale.clone()),
                uint_field("BMMP", 0),
                string_field("BSMB", "MaleRange", &interner),
                bytes_field("BSMS", male_range.clone()),
                uint_field("BSMP", 1),
                string_field("BSMB", "FemaleScale", &interner),
                bytes_field("BSMS", female_scale.clone()),
                uint_field("BMMP", 1),
                string_field("BSMB", "FemaleRange", &interner),
                bytes_field("BSMS", female_range.clone()),
            ],
            &interner,
        );
        let expected_payloads = [male_scale, male_range, female_scale, female_range];
        let assert_layouts = |record: &Record| {
            assert_eq!(
                sigs(record),
                vec![
                    "NAM0", "BSMP", "BSMB", "BSMS", "BMMP", "BSMB", "BSMS", "BSMP", "BSMB", "BSMS",
                    "BMMP", "BSMB", "BSMS",
                ]
            );
            let payloads = record
                .fields
                .iter()
                .filter_map(|entry| {
                    if entry.sig.as_str() != "BSMS" {
                        return None;
                    }
                    let FieldValue::Bytes(bytes) = &entry.value else {
                        panic!("BSMS should remain a raw fixed-layout payload");
                    };
                    Some(bytes.as_slice())
                })
                .collect::<Vec<_>>();
            assert_eq!(payloads.len(), 4);
            for (payload, expected) in payloads.into_iter().zip(&expected_payloads) {
                assert_eq!(payload.len(), expected.len());
                assert_eq!(payload, expected.as_slice());
            }
        };

        let TargetRecordNormalization::Keep(first) =
            normalize_from_fo76_source(record, "RACE", &interner)
        else {
            panic!("RACE should be supported");
        };
        assert_layouts(&first);

        let TargetRecordNormalization::Keep(second) =
            normalize_target_only_with_interner(first, &interner)
        else {
            panic!("RACE should survive final target normalization");
        };
        assert_layouts(&second);
    }

    #[test]
    fn strips_generated_additive_tints_but_preserves_ck_morph_rows() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let mut fields = vec![
            bytes_field("EDID", b"PowerArmorRace_additive\0".to_vec()),
            bytes_field("TINL", 1_u32.to_le_bytes().to_vec()),
            bytes_field("TTGP", vec![0; 4]),
            bytes_field("TETI", vec![0; 4]),
            bytes_field("TTEF", vec![0; 4]),
            ctda_field(14),
            bytes_field("CIS1", b"TintParameter\0".to_vec()),
            bytes_field("CIS2", b"TintParameter2\0".to_vec()),
            bytes_field("TTET", b"Textures\\Tint.dds\0".to_vec()),
            bytes_field("TTEB", 0_u32.to_le_bytes().to_vec()),
            bytes_field("TTEC", vec![0; 14]),
            bytes_field("TTED", 1.0_f32.to_le_bytes().to_vec()),
            bytes_field("TTGE", 0_u32.to_le_bytes().to_vec()),
        ];
        for (name, mask, slider) in [(b"Body\0".as_slice(), 1_u16, 7_u32), (b"Face\0", 2, 8)] {
            fields.extend([
                bytes_field("MPGN", name.to_vec()),
                bytes_field("MPPC", 1_u32.to_le_bytes().to_vec()),
                bytes_field("MPPK", mask.to_le_bytes().to_vec()),
                bytes_field("MPGS", slider.to_le_bytes().to_vec()),
            ]);
        }
        fields.extend([
            none_field("NAM0"),
            formkey_field(
                "SADD",
                FormKey {
                    local: 0x0001_A009,
                    plugin: interner.intern("Fallout4.esm"),
                },
            ),
            formkey_field(
                "STKD",
                FormKey {
                    local: 0x0002_0005,
                    plugin: output,
                },
            ),
            string_field("SGNM", "Actors\\Supermutant\\Behavior.hkx", &interner),
            string_field("SAPT", "Actors\\Supermutant\\Animations", &interner),
            bytes_field("SRAF", vec![0; 4]),
        ]);

        let record = record("RACE", fields, &interner);
        let TargetRecordNormalization::Keep(first) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("generated additive RACE should survive");
        };
        let TargetRecordNormalization::Keep(second) =
            normalize_target_only_with_interner(first.clone(), &interner)
        else {
            panic!("generated additive RACE should remain stable");
        };

        assert_eq!(
            sigs(&first),
            vec![
                "EDID", "MPGN", "MPPC", "MPPK", "MPGS", "MPGN", "MPPC", "MPPK", "MPGS", "NAM0",
                "SADD", "STKD", "SGNM", "SAPT", "SRAF",
            ]
        );
        assert_eq!(second.fields, first.fields);
        assert!(first.fields.iter().all(|entry| !matches!(
            &entry.sig.0,
            b"TINL"
                | b"TTGP"
                | b"TETI"
                | b"TTEF"
                | b"TTET"
                | b"TTEB"
                | b"TTEC"
                | b"TTED"
                | b"TTGE"
                | b"CTDA"
                | b"CIS1"
                | b"CIS2"
        )));
    }

    #[test]
    fn snallygaster_tint_cleanup_preserves_live_non_tint_families() {
        let interner = StringInterner::new();
        let seventy_six = interner.intern("SeventySix.esm");
        let mut record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"SnallyGasterRace\0".to_vec()),
                string_field(
                    "ANAM",
                    "Actors\\Snallygaster\\CharacterAssets\\Skeleton.nif",
                    &interner,
                ),
                bytes_field("TINL", 1139_u32.to_le_bytes().to_vec()),
                bytes_field("ATKD", vec![0; 44]),
                string_field("ATKE", "meleeStart_5", &interner),
                string_field("ATKT", "Leaping Power Attack", &interner),
                none_field("NAM1"),
                none_field("MNAM"),
                bytes_field("INDX", 0_u32.to_le_bytes().to_vec()),
                string_field(
                    "MODL",
                    "Actors\\Character\\UpperBodyHumanMale.egt",
                    &interner,
                ),
                none_field("FNAM"),
                none_field("NAM3"),
                none_field("MNAM"),
                string_field(
                    "MODL",
                    "Actors\\Snallygaster\\SnallygasterProject.hkx",
                    &interner,
                ),
                string_field("NAME", "C_Head", &interner),
                string_field("PHTN", "Aah", &interner),
                bytes_field("PHWT", vec![0; 64]),
                bytes_field("MLSI", 1_107_087_360_u32.to_le_bytes().to_vec()),
                formkey_field(
                    "STKD",
                    FormKey {
                        local: 0x00C301,
                        plugin: seventy_six,
                    },
                ),
                string_field(
                    "SGNM",
                    "Actors\\Snallygaster\\Behaviors\\SnallygasterCoreBehavior.hkx",
                    &interner,
                ),
                string_field("SAPT", "Actors\\Snallygaster\\Animations", &interner),
                bytes_field("SRAF", vec![1, 0, 0, 0]),
                formkey_field(
                    "SAKD",
                    FormKey {
                        local: 0x030B01,
                        plugin: seventy_six,
                    },
                ),
            ],
            &interner,
        );
        record.form_key = FormKey {
            local: 0x00D191,
            plugin: seventy_six,
        };
        let expected = record
            .fields
            .iter()
            .filter(|entry| entry.sig.0 != *b"TINL")
            .cloned()
            .collect::<SmallVec<[FieldEntry; 8]>>();

        strip_unsupported_race_tint_tables(&mut record);

        assert_eq!(record.form_key.local, 0x00D191);
        assert_eq!(record.fields, expected);
    }

    #[test]
    fn places_appended_additive_subgraphs_before_race_morph_values() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"SuperMutantRaceAdditivePluginPort\0".to_vec()),
                none_field("NAM1"),
                none_field("MNAM"),
                bytes_field("INDX", 0_u32.to_le_bytes().to_vec()),
                none_field("FNAM"),
                bytes_field("INDX", 1_u32.to_le_bytes().to_vec()),
                formkey_field(
                    "GNAM",
                    FormKey {
                        local: 0x0002_0001,
                        plugin: output,
                    },
                ),
                none_field("NAM3"),
                none_field("MNAM"),
                string_field("MODL", "male.nif", &interner),
                none_field("FNAM"),
                string_field("MODL", "female.nif", &interner),
                bytes_field("NAM4", 0_u32.to_le_bytes().to_vec()),
                formkey_field(
                    "CNAM",
                    FormKey {
                        local: 0x0002_0002,
                        plugin: output,
                    },
                ),
                none_field("NAM0"),
                none_field("MNAM"),
                formkey_field(
                    "HEAD",
                    FormKey {
                        local: 0x0002_0003,
                        plugin: output,
                    },
                ),
                bytes_field("PTOP", 1.0_f32.to_le_bytes().to_vec()),
                bytes_field("NTOP", 2.0_f32.to_le_bytes().to_vec()),
                bytes_field("MSID", 1_u32.to_le_bytes().to_vec()),
                string_field("MSM0", "NoseDown", &interner),
                string_field("MSM1", "NoseUp", &interner),
                bytes_field("MLSI", 1_u32.to_le_bytes().to_vec()),
                formkey_field(
                    "SADD",
                    FormKey {
                        local: 0x0001_A009,
                        plugin: interner.intern("Fallout4.esm"),
                    },
                ),
                formkey_field(
                    "SAKD",
                    FormKey {
                        local: 0x0002_0004,
                        plugin: output,
                    },
                ),
                formkey_field(
                    "STKD",
                    FormKey {
                        local: 0x0002_0005,
                        plugin: output,
                    },
                ),
                string_field("SGNM", "Actors\\Supermutant\\Behavior.hkx", &interner),
                string_field("SAPT", "Actors\\Supermutant\\Animations", &interner),
                bytes_field("SRAF", vec![0; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("RACE should be supported");
        };
        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("RACE should survive final target normalization");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "NAM1", "MNAM", "INDX", "FNAM", "INDX", "GNAM", "NAM3", "MNAM", "MODL",
                "FNAM", "MODL", "NAM4", "CNAM", "NAM0", "MNAM", "SADD", "SAKD", "STKD", "SGNM",
                "SAPT", "SRAF", "PTOP", "NTOP", "MSID", "MSM0", "MSM1", "MLSI", "HEAD",
            ]
        );
        assert_race_outer_cursor_accepts(&record);
    }

    #[test]
    fn places_appended_race_subgraphs_before_reduced_schema_late_members_idempotently() {
        let interner = StringInterner::new();
        let output = interner.intern("SeventySix.esm");
        let fallout4 = interner.intern("Fallout4.esm");
        let cases = vec![
            (
                "HEAD",
                vec![formkey_field(
                    "HEAD",
                    FormKey {
                        local: 0x0002_0003,
                        plugin: output,
                    },
                )],
            ),
            (
                "ICON",
                vec![string_field("ICON", "Textures\\Race.dds", &interner)],
            ),
            (
                "FMRI",
                vec![
                    bytes_field("FMRI", 10_u32.to_le_bytes().to_vec()),
                    string_field("FMRN", "NoseShort", &interner),
                ],
            ),
            (
                "MSM0",
                vec![
                    string_field("MSM0", "NoseDown", &interner),
                    string_field("MSM1", "NoseUp", &interner),
                ],
            ),
            (
                "BSMB",
                vec![
                    string_field("BSMB", "MaleScale", &interner),
                    bytes_field("BSMS", vec![0; 36]),
                ],
            ),
        ];

        for (boundary_sig, schema_late_fields) in cases {
            let mut fields = vec![
                bytes_field("EDID", b"ReducedRace\0".to_vec()),
                none_field("NAM0"),
            ];
            let schema_late_sigs = schema_late_fields
                .iter()
                .map(|entry| entry.sig.as_str())
                .collect::<Vec<_>>();
            fields.extend(schema_late_fields.clone());
            fields.extend([
                formkey_field(
                    "SADD",
                    FormKey {
                        local: 0x0001_A009,
                        plugin: fallout4,
                    },
                ),
                formkey_field(
                    "SAKD",
                    FormKey {
                        local: 0x0002_0004,
                        plugin: output,
                    },
                ),
                formkey_field(
                    "STKD",
                    FormKey {
                        local: 0x0002_0005,
                        plugin: output,
                    },
                ),
                string_field("SGNM", "Actors\\Reduced\\Behavior.hkx", &interner),
                string_field("SAPT", "Actors\\Reduced\\Animations", &interner),
                bytes_field("SRAF", vec![0; 4]),
            ]);
            let record = record("RACE", fields, &interner);

            let TargetRecordNormalization::Keep(once) =
                normalize_target_only_with_interner(record, &interner)
            else {
                panic!("reduced RACE should be supported");
            };
            let TargetRecordNormalization::Keep(twice) =
                normalize_target_only_with_interner(once.clone(), &interner)
            else {
                panic!("reduced RACE should remain supported");
            };

            let mut expected = vec![
                "EDID", "NAM0", "SADD", "SAKD", "STKD", "SGNM", "SAPT", "SRAF",
            ];
            expected.extend(schema_late_sigs);
            assert_eq!(sigs(&once), expected, "failed boundary {boundary_sig}");
            assert_eq!(twice.fields, once.fields, "failed boundary {boundary_sig}");
        }
    }

    #[test]
    fn npc_late_fields_remain_after_duplicate_full_slots() {
        let interner = StringInterner::new();
        let record = record(
            "NPC_",
            vec![
                bytes_field("EDID", b"Npc\0".to_vec()),
                bytes_field("FULL", b"Template Name\0".to_vec()),
                bytes_field("FULL", b"NPC Name\0".to_vec()),
                bytes_field("OBTE", vec![0; 4]),
                bytes_field("SHRT", b"Short\0".to_vec()),
                bytes_field("DATA", vec![0]),
                bytes_field("DNAM", vec![0; 4]),
                bytes_field("HCLF", vec![0; 4]),
                bytes_field("ZNAM", vec![0; 4]),
                bytes_field("NAM5", vec![0; 4]),
                bytes_field("NAM6", vec![0; 4]),
                bytes_field("NAM4", vec![0; 4]),
                bytes_field("MWGT", vec![0; 4]),
                bytes_field("NAM8", vec![0; 4]),
                bytes_field("DOFT", vec![0; 4]),
                bytes_field("DPLT", vec![0; 4]),
                bytes_field("MRSV", vec![0; 4]),
                bytes_field("FMRI", vec![0; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("NPC_ should be supported");
        };

        assert_cursor_accepts(&record);
        let sigs = sigs(&record);
        let second_full_pos = sigs
            .iter()
            .enumerate()
            .filter_map(|(index, sig)| (*sig == "FULL").then_some(index))
            .nth(1)
            .unwrap();
        let shrt_pos = sigs.iter().position(|sig| *sig == "SHRT").unwrap();
        assert!(second_full_pos < shrt_pos);
    }

    #[test]
    fn npc_unscoped_full_survives_when_object_template_anchor_absent() {
        let interner = StringInterner::new();
        let record = record(
            "NPC_",
            vec![
                bytes_field("EDID", b"Npc\0".to_vec()),
                bytes_field("FULL", b"NPC Name\0".to_vec()),
                bytes_field("AIDT", vec![0; 20]),
                bytes_field("DATA", vec![0]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("NPC_ should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "AIDT", "FULL", "DATA"]);
    }

    #[test]
    fn translated_npc_acbs_template_flags_match_ck_safe_tpta_slots() {
        let interner = StringInterner::new();
        let mut acbs = vec![0u8; 20];
        acbs[..4].copy_from_slice(&0x18_u32.to_le_bytes());
        acbs[14..16].copy_from_slice(&0x1F28_u16.to_le_bytes());
        let mut tpta = Vec::new();
        for slot in [
            0,
            0,
            0,
            0x00157C5F_u32,
            0,
            0x00157C5F,
            0,
            0,
            0x00157C5F,
            0x00157C5F,
            0x00157C5F,
            0x00157C5F,
            0x00157C5F,
        ] {
            tpta.extend_from_slice(&slot.to_le_bytes());
        }
        let record = record(
            "NPC_",
            vec![
                bytes_field("EDID", b"EncReaperBot\0".to_vec()),
                bytes_field("ACBS", acbs),
                bytes_field("TPLT", 0x00157C5F_u32.to_le_bytes().to_vec()),
                bytes_field("TPTA", tpta),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "NPC_", &interner)
        else {
            panic!("NPC_ should be supported");
        };
        let acbs = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ACBS")
            .expect("ACBS should remain");
        let FieldValue::Bytes(bytes) = &acbs.value else {
            panic!("ACBS should be bytes");
        };
        let flags = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        let template_flags = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
        assert_eq!(flags, 0x18);
        // Inventory (slot 8) is carried in the full-plugin path; the cell-slice
        // crash-avoidance strip lives in a !is_whole_plugin fixup instead.
        assert_eq!(
            template_flags,
            NPC_TEMPLATE_AI_PACKAGES | NPC_TEMPLATE_INVENTORY | NPC_TEMPLATE_SCRIPT
        );
        let tpta = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "TPTA")
            .expect("TPTA should remain");
        let FieldValue::Bytes(bytes) = &tpta.value else {
            panic!("TPTA should be bytes");
        };
        let slots: Vec<u32> = bytes
            .chunks_exact(4)
            .map(|chunk| u32::from_le_bytes(chunk.try_into().unwrap()))
            .collect();
        assert_eq!(
            slots,
            vec![
                0, 0, 0, 0, 0, 0x00157C5F, 0, 0, 0x00157C5F, 0x00157C5F, 0, 0, 0,
            ]
        );
    }

    #[test]
    fn translated_npc_struct_tpta_inventory_slot_is_preserved() {
        let interner = StringInterner::new();
        let template = FormKey::parse("157C5F@Fallout4.esm", &interner).unwrap();
        let mut acbs = vec![0u8; 20];
        acbs[..4].copy_from_slice(&0x18_u32.to_le_bytes());
        acbs[14..16].copy_from_slice(&0x1F28_u16.to_le_bytes());
        let tpta_names = [
            "traits",
            "stats",
            "factions",
            "spell_list",
            "ai_data",
            "ai_packages",
            "model_animation",
            "base_data",
            "inventory",
            "script",
            "def_package_list",
            "attack_data",
            "keywords",
        ];
        let mut tpta = Vec::new();
        for (slot_index, name) in tpta_names.iter().enumerate() {
            let value = if matches!(slot_index, 5 | 8 | 9 | 10) {
                FieldValue::FormKey(template)
            } else {
                FieldValue::Uint(0)
            };
            tpta.push((interner.intern(name), value));
        }
        let record = record(
            "NPC_",
            vec![
                bytes_field("EDID", b"EncReaperBot\0".to_vec()),
                bytes_field("ACBS", acbs),
                struct_field("TPTA", tpta),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "NPC_", &interner)
        else {
            panic!("NPC_ should be supported");
        };
        let acbs = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ACBS")
            .expect("ACBS should remain");
        let FieldValue::Bytes(bytes) = &acbs.value else {
            panic!("ACBS should be bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 0x18);
        assert_eq!(
            u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
            NPC_TEMPLATE_AI_PACKAGES | NPC_TEMPLATE_INVENTORY | NPC_TEMPLATE_SCRIPT
        );
        let tpta = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "TPTA")
            .expect("TPTA should remain");
        let FieldValue::Struct(fields) = &tpta.value else {
            panic!("TPTA should remain structured before target write");
        };
        assert!(matches!(&fields[5].1, FieldValue::FormKey(_)));
        // Inventory slot is carried in the full-plugin path.
        assert!(matches!(&fields[8].1, FieldValue::FormKey(_)));
        assert!(matches!(&fields[9].1, FieldValue::FormKey(_)));
        assert_eq!(fields[10].1, FieldValue::Uint(0));
    }

    #[test]
    fn translated_w05_denizen_acbs_strips_essential_ghost_and_invulnerable() {
        let interner = StringInterner::new();
        let mut acbs = vec![0u8; 20];
        let source_flags = 0x18_u32
            | NPC_ACBS_FLAG_ESSENTIAL
            | NPC_ACBS_FLAG_UNKNOWN_25
            | NPC_ACBS_FLAG_IS_GHOST
            | NPC_ACBS_FLAG_INVULNERABLE;
        acbs[..4].copy_from_slice(&source_flags.to_le_bytes());
        acbs[14..16].copy_from_slice(&0x1F28_u16.to_le_bytes());
        let record = record(
            "NPC_",
            vec![
                bytes_field(
                    "EDID",
                    b"W05_LvlDenizen_RaiderIntimidatorM_or_LiteAlly\0".to_vec(),
                ),
                bytes_field("ACBS", acbs),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "NPC_", &interner)
        else {
            panic!("NPC_ should be supported");
        };
        let acbs = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ACBS")
            .expect("ACBS should remain");
        let FieldValue::Bytes(bytes) = &acbs.value else {
            panic!("ACBS should be bytes");
        };
        let flags = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        assert_eq!(flags & NPC_ACBS_FLAG_ESSENTIAL, 0);
        assert_eq!(flags & NPC_ACBS_FLAG_UNKNOWN_25, NPC_ACBS_FLAG_UNKNOWN_25);
        assert_eq!(flags & NPC_ACBS_FLAG_IS_GHOST, 0);
        assert_eq!(flags & NPC_ACBS_FLAG_INVULNERABLE, 0);
    }

    #[test]
    fn translated_non_denizen_npc_acbs_keeps_essential() {
        let interner = StringInterner::new();
        let mut acbs = vec![0u8; 20];
        let source_flags = 0x18_u32
            | NPC_ACBS_FLAG_ESSENTIAL
            | NPC_ACBS_FLAG_IS_GHOST
            | NPC_ACBS_FLAG_INVULNERABLE;
        acbs[..4].copy_from_slice(&source_flags.to_le_bytes());
        let record = record(
            "NPC_",
            vec![
                bytes_field("EDID", b"W05_COMP_Actor_Lite_RaiderPunk_Initial\0".to_vec()),
                bytes_field("ACBS", acbs),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "NPC_", &interner)
        else {
            panic!("NPC_ should be supported");
        };
        let acbs = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ACBS")
            .expect("ACBS should remain");
        let FieldValue::Bytes(bytes) = &acbs.value else {
            panic!("ACBS should be bytes");
        };
        let flags = u32::from_le_bytes(bytes[..4].try_into().unwrap());
        assert_eq!(flags & NPC_ACBS_FLAG_ESSENTIAL, NPC_ACBS_FLAG_ESSENTIAL);
        assert_eq!(flags & NPC_ACBS_FLAG_IS_GHOST, 0);
        assert_eq!(flags & NPC_ACBS_FLAG_INVULNERABLE, 0);
    }

    #[test]
    fn target_only_npc_acbs_flags_are_not_rewritten() {
        let interner = StringInterner::new();
        let mut acbs = vec![0u8; 20];
        acbs[..4].copy_from_slice(&0x18_u32.to_le_bytes());
        acbs[14..16].copy_from_slice(&0x1F28_u16.to_le_bytes());
        let record = record(
            "NPC_",
            vec![
                bytes_field("EDID", b"TargetOnlyNpc\0".to_vec()),
                bytes_field("ACBS", acbs),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("NPC_ should be supported");
        };
        let acbs = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "ACBS")
            .expect("ACBS should remain");
        let FieldValue::Bytes(bytes) = &acbs.value else {
            panic!("ACBS should be bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 0x18);
        assert_eq!(
            u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
            0x1F28
        );
    }

    #[test]
    fn interleaves_scoped_scol_parts() {
        let interner = StringInterner::new();
        let record = record(
            "SCOL",
            vec![
                bytes_field("EDID", b"Scol\0".to_vec()),
                bytes_field("ONAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ONAM", 2_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", vec![1; 28]),
                bytes_field("DATA", vec![2; 28]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCOL should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "ONAM", "DATA", "ONAM", "DATA"]);
    }

    #[test]
    fn coalesces_duplicate_scol_static_groups() {
        let interner = StringInterner::new();
        let record = record(
            "SCOL",
            vec![
                bytes_field("EDID", b"Scol\0".to_vec()),
                bytes_field("ONAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", vec![1; 28]),
                bytes_field("ONAM", 2_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", vec![2; 28]),
                bytes_field("ONAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", vec![3; 56]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCOL should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "ONAM", "DATA", "ONAM", "DATA"]);
        let data_payloads = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "DATA")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.as_slice(),
                other => panic!("expected DATA bytes, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(data_payloads[0].len(), 84);
        assert_eq!(&data_payloads[0][..28], &[1; 28]);
        assert_eq!(&data_payloads[0][28..], &[3; 56]);
        assert_eq!(data_payloads[1], &[2; 28]);
    }

    #[test]
    fn interleaves_scoped_spell_effects() {
        let interner = StringInterner::new();
        let record = record(
            "SPEL",
            vec![
                bytes_field("EDID", b"Spell\0".to_vec()),
                bytes_field("EFID", 1_u32.to_le_bytes().to_vec()),
                bytes_field("EFID", 2_u32.to_le_bytes().to_vec()),
                bytes_field("EFIT", vec![1; 12]),
                bytes_field("EFIT", vec![2; 12]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SPEL should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "EFID", "EFIT", "EFID", "EFIT"]);
    }

    #[test]
    fn interleaves_enchantment_conditions_with_their_effects() {
        let interner = StringInterner::new();
        let record = record(
            "ENCH",
            vec![
                bytes_field("EDID", b"ChineseStealthArmor\0".to_vec()),
                bytes_field("EFID", 1_u32.to_le_bytes().to_vec()),
                bytes_field("EFIT", vec![1; 12]),
                ctda_field(286),
                ctda_field(580),
                ctda_field(46),
                bytes_field("EFID", 2_u32.to_le_bytes().to_vec()),
                bytes_field("EFIT", vec![2; 12]),
                ctda_field(286),
                ctda_field(580),
                ctda_field(46),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("ENCH should be supported");
        };

        assert_eq!(
            condition_sequence(&record),
            vec![
                "CTDA:286", "CTDA:580", "CTDA:46", "CTDA:286", "CTDA:580", "CTDA:46",
            ]
        );
        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "EFID", "EFIT", "CTDA", "CTDA", "CTDA", "EFID", "EFIT", "CTDA", "CTDA",
                "CTDA",
            ]
        );
    }

    #[test]
    fn fo76_magic_efit_drops_effect_id_for_fo4_layout() {
        let source_def = efit_def(
            "struct:I,f,I,I",
            &["effect_id", "magnitude", "area", "duration"],
        );
        let target_def = efit_def("struct:f,I,I", &["magnitude", "area", "duration"]);
        let mut raw = Vec::new();
        raw.extend_from_slice(&1_u32.to_le_bytes());
        raw.extend_from_slice(&10.0_f32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&5_u32.to_le_bytes());

        let normalized = normalize_raw_bytes(&raw, Some(&source_def), &target_def);

        assert_eq!(normalized.len(), 12);
        assert_eq!(
            f32::from_le_bytes(normalized[0..4].try_into().unwrap()),
            10.0
        );
        assert_eq!(u32::from_le_bytes(normalized[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(normalized[8..12].try_into().unwrap()), 5);
    }

    #[test]
    fn fo76_enchantment_efit_drops_leading_effect_id_with_actual_source_schema() {
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&99.0_f32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        let record = record(
            "ENCH",
            vec![
                bytes_field("EFID", 1_u32.to_le_bytes().to_vec()),
                bytes_field("EFIT", raw),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "ENCH", &interner)
        else {
            panic!("ENCH should be supported");
        };
        let efit = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "EFIT")
            .expect("EFIT");
        let FieldValue::Bytes(bytes) = &efit.value else {
            panic!("expected raw EFIT bytes");
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(f32::from_le_bytes(bytes[0..4].try_into().unwrap()), 99.0);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn fo76_alch_efit_without_area_inserts_zero_for_fo4_layout() {
        let source_def = efit_def(
            "struct:I,f,I,I",
            &["effect_id", "magnitude", "area", "duration"],
        );
        let target_def = efit_def("struct:f,I,I", &["magnitude", "area", "duration"]);
        let mut raw = Vec::new();
        raw.extend_from_slice(&3_u32.to_le_bytes());
        raw.extend_from_slice(&4.0_f32.to_le_bytes());
        raw.extend_from_slice(&20_u32.to_le_bytes());

        let normalized = normalize_raw_bytes(&raw, Some(&source_def), &target_def);

        assert_eq!(normalized.len(), 12);
        assert_eq!(
            f32::from_le_bytes(normalized[0..4].try_into().unwrap()),
            4.0
        );
        assert_eq!(u32::from_le_bytes(normalized[4..8].try_into().unwrap()), 0);
        assert_eq!(
            u32::from_le_bytes(normalized[8..12].try_into().unwrap()),
            20
        );
    }

    #[test]
    fn fo76_alch_enit_keeps_only_flags_shared_with_fo4() {
        let source_def = enit_def(
            "struct:i,I,I,f,I,I,I,B,I",
            &[
                "value",
                "flags",
                "addiction",
                "addiction_chance",
                "sound_consume",
                "health",
                "spoiled",
                "is_canned",
                "canned_item_base",
            ],
        );
        let target_def = enit_def(
            "struct:i,I,I,f,I",
            &[
                "value",
                "flags",
                "addiction",
                "addiction_chance",
                "sound_consume",
            ],
        );
        let mut raw = Vec::new();
        raw.extend_from_slice(&60_i32.to_le_bytes());
        raw.extend_from_slice(&0x0001_0019_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0.0_f32.to_le_bytes());
        raw.extend_from_slice(&0x0002_BAF2_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.push(0);
        raw.extend_from_slice(&0_u32.to_le_bytes());

        let normalized = normalize_raw_bytes(&raw, Some(&source_def), &target_def);

        assert_eq!(normalized.len(), 20);
        assert_eq!(i32::from_le_bytes(normalized[0..4].try_into().unwrap()), 60);
        assert_eq!(
            u32::from_le_bytes(normalized[4..8].try_into().unwrap()),
            0x0001_0001
        );
        assert_eq!(
            u32::from_le_bytes(normalized[16..20].try_into().unwrap()),
            0x0002_BAF2
        );
    }

    #[test]
    fn scen_terminal_fields_survive_when_action_anchor_absent() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"Scene\0".to_vec()),
                bytes_field("PNAM", vec![1; 4]),
                bytes_field("INAM", vec![2; 4]),
                bytes_field("FNAM", vec![3; 4]),
                bytes_field("VNAM", vec![4; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "FNAM", "PNAM", "INAM", "VNAM"]);
    }

    #[test]
    fn scen_phase_scope_does_not_steal_action_fnam() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"Scene\0".to_vec()),
                uint_field("FNAM", 0x5024),
                bytes_field("HNAM", Vec::new()),
                bytes_field("WNAM", 0x015e_u32.to_le_bytes().to_vec()),
                bytes_field("HNAM", Vec::new()),
                bytes_field("ANAM", 3_u16.to_le_bytes().to_vec()),
                bytes_field("INAM", 1_u32.to_le_bytes().to_vec()),
                uint_field("FNAM", 0x0020_0000),
                bytes_field("SNAM", 0_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "FNAM", "HNAM", "WNAM", "HNAM", "ANAM", "INAM", "FNAM", "SNAM",
            ]
        );
        let fnam_lengths = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "FNAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.len(),
                other => panic!("FNAM should be fixed-width bytes, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(fnam_lengths, vec![4, 4]);
    }

    #[test]
    fn scen_phase_fnam_scalar_uses_target_u16_width() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"Scene\0".to_vec()),
                uint_field("FNAM", 0x5024),
                bytes_field("HNAM", Vec::new()),
                bytes_field("WNAM", 0x015e_u32.to_le_bytes().to_vec()),
                uint_field("FNAM", 0x2000),
                bytes_field("HNAM", Vec::new()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        let fnams = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "FNAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.to_vec(),
                other => panic!("FNAM should be fixed-width bytes, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(fnams[0].as_slice(), &0x5024_u32.to_le_bytes());
        assert_eq!(fnams[1].as_slice(), &0x2000_u16.to_le_bytes());
    }

    #[test]
    fn scen_actor_scope_does_not_steal_action_aliases() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"Scene\0".to_vec()),
                uint_field("FNAM", 0x5024),
                bytes_field("ALID", 7_i32.to_le_bytes().to_vec()),
                bytes_field("LNAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DNAM", 10_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", 3_u16.to_le_bytes().to_vec()),
                bytes_field("NAM0", b"\0".to_vec()),
                bytes_field("ALID", 8_i32.to_le_bytes().to_vec()),
                bytes_field("INAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", Vec::new()),
                bytes_field("PNAM", 0x073F_BC0D_u32.to_le_bytes().to_vec()),
                bytes_field("INAM", 1_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "FNAM", "ALID", "LNAM", "DNAM", "ANAM", "NAM0", "ALID", "INAM", "ANAM",
                "PNAM", "INAM",
            ]
        );
        let aliases = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ALID")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => i32::from_le_bytes(bytes[..4].try_into().unwrap()),
                other => panic!("ALID should be bytes, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(aliases, vec![7, 8]);
    }

    #[test]
    fn scen_actions_use_empty_anam_as_end_marker() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"Scene\0".to_vec()),
                uint_field("FNAM", 0x5024),
                bytes_field("ALID", 1_i32.to_le_bytes().to_vec()),
                bytes_field("LNAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DNAM", 10_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", 3_u16.to_le_bytes().to_vec()),
                bytes_field("ALID", 1_i32.to_le_bytes().to_vec()),
                bytes_field("INAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("DTGT", 1_i32.to_le_bytes().to_vec()),
                bytes_field("ANAM", Vec::new()),
                bytes_field("ANAM", 0_u16.to_le_bytes().to_vec()),
                bytes_field("ALID", 1_i32.to_le_bytes().to_vec()),
                bytes_field("INAM", 2_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", 0x0756_D327_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", Vec::new()),
                bytes_field("PNAM", 0x073F_BC0D_u32.to_le_bytes().to_vec()),
                bytes_field("INAM", 2_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "FNAM", "ALID", "LNAM", "DNAM", "ANAM", "ALID", "INAM", "DTGT", "ANAM",
                "ANAM", "ALID", "INAM", "DATA", "ANAM", "PNAM", "INAM",
            ]
        );
        let action_markers = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ANAM")
            .map(|field| match &field.value {
                FieldValue::Bytes(bytes) => bytes.to_vec(),
                other => panic!("ANAM should be bytes, got {other:?}"),
            })
            .collect::<Vec<_>>();
        assert_eq!(
            action_markers,
            vec![
                3_u16.to_le_bytes().to_vec(),
                Vec::new(),
                0_u16.to_le_bytes().to_vec(),
                Vec::new(),
            ]
        );
        let pnam = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "PNAM")
            .expect("terminal parent quest PNAM should remain unscoped");
        let FieldValue::Bytes(bytes) = &pnam.value else {
            panic!("PNAM should be bytes");
        };
        assert_eq!(
            u32::from_le_bytes(bytes[..4].try_into().unwrap()),
            0x073F_BC0D
        );
    }

    /// BURN_SQ01_Radio_Message-shaped regression: a radio (ANAM type 6) action
    /// body arrives in source order `DATA HTID DMAX`, which is the canonical
    /// FO4 order. The flat actions scope lists HTID twice (Player Headtracking
    /// before DMAX, Play Sound after the topic block), so a member-list walk
    /// re-sorts the body to `DATA DMAX HTID` and xEdit rejects HTID as out of
    /// order. The body must be emitted in its source order.
    #[test]
    fn scen_radio_action_preserves_data_htid_dmax_order() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"BURN_SQ01_Radio_Message\0".to_vec()),
                uint_field("FNAM", 0x5024),
                none_field("HNAM"),
                bytes_field("NAM0", b"\0".to_vec()),
                none_field("NEXT"),
                none_field("NEXT"),
                bytes_field("WNAM", 0x015e_u32.to_le_bytes().to_vec()),
                none_field("HNAM"),
                bytes_field("ALID", 0_i32.to_le_bytes().to_vec()),
                bytes_field("LNAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DNAM", 0_u32.to_le_bytes().to_vec()),
                // radio action: DATA HTID DMAX is the correct FO4 order
                bytes_field("ANAM", 6_u16.to_le_bytes().to_vec()),
                bytes_field("NAM0", b"\0".to_vec()),
                bytes_field("ALID", 0_i32.to_le_bytes().to_vec()),
                bytes_field("INAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("SNAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ENAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", 0x077F_79AD_u32.to_le_bytes().to_vec()),
                bytes_field("HTID", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DMAX", 0x0043_570f_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", Vec::new()),
                // record-level tail
                bytes_field("PNAM", 0x077F_79AB_u32.to_le_bytes().to_vec()),
                bytes_field("INAM", 6_u32.to_le_bytes().to_vec()),
                bytes_field("VNAM", vec![3; 16]),
                bytes_field("XNAM", 0_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "FNAM", "HNAM", "NAM0", "NEXT", "NEXT", "WNAM", "HNAM", "ALID", "LNAM",
                "DNAM", "ANAM", "NAM0", "ALID", "INAM", "SNAM", "ENAM", "DATA", "HTID", "DMAX",
                "ANAM", "PNAM", "INAM", "VNAM", "XNAM",
            ]
        );
    }

    /// Source-order preservation must cover every duplicate-sig actions-scope
    /// member, not only HTID. The flat schema lists DATA/HTID/DMAX/ONAM (and
    /// SNAM) at multiple ranks. Action 0 keeps the radio order
    /// `DATA HTID DMAX ONAM`; action 1 keeps a repeated `SNAM ... SNAM`.
    #[test]
    fn scen_actions_preserve_all_duplicate_sig_members_in_source_order() {
        let interner = StringInterner::new();
        let record = record(
            "SCEN",
            vec![
                bytes_field("EDID", b"Scene\0".to_vec()),
                uint_field("FNAM", 0x5024),
                bytes_field("ALID", 0_i32.to_le_bytes().to_vec()),
                bytes_field("LNAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DNAM", 0_u32.to_le_bytes().to_vec()),
                // action 0: DATA HTID DMAX ONAM (radio + trailing ONAM)
                bytes_field("ANAM", 6_u16.to_le_bytes().to_vec()),
                bytes_field("NAM0", b"\0".to_vec()),
                bytes_field("ALID", 0_i32.to_le_bytes().to_vec()),
                bytes_field("INAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("SNAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("ENAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DATA", 0x0700_1111_u32.to_le_bytes().to_vec()),
                bytes_field("HTID", 0_u32.to_le_bytes().to_vec()),
                bytes_field("DMAX", 0_u32.to_le_bytes().to_vec()),
                bytes_field("ONAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", Vec::new()),
                // action 1: repeated SNAM within one action must both survive
                bytes_field("ANAM", 2_u16.to_le_bytes().to_vec()),
                bytes_field("NAM0", b"\0".to_vec()),
                bytes_field("ALID", 0_i32.to_le_bytes().to_vec()),
                bytes_field("INAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("SNAM", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ENAM", 0_u32.to_le_bytes().to_vec()),
                bytes_field("SNAM", 0x4000_0000_u32.to_le_bytes().to_vec()),
                bytes_field("ANAM", Vec::new()),
                bytes_field("PNAM", 0x0700_2222_u32.to_le_bytes().to_vec()),
                bytes_field("INAM", 0_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("SCEN should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "FNAM", "ALID", "LNAM", "DNAM", //
                "ANAM", "NAM0", "ALID", "INAM", "SNAM", "ENAM", "DATA", "HTID", "DMAX", "ONAM",
                "ANAM", //
                "ANAM", "NAM0", "ALID", "INAM", "SNAM", "ENAM", "SNAM", "ANAM", //
                "PNAM", "INAM",
            ]
        );
    }

    #[test]
    fn drops_orphan_quest_stage_children_without_index_anchor() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("NEXT", 1_u16.to_le_bytes().to_vec()),
                bytes_field("QSDT", vec![1]),
                bytes_field("QSDT", vec![2]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "NEXT"]);
    }

    #[test]
    fn preserves_qust_stage_log_entry_rows() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                none_field("NEXT"),
                bytes_field("INDX", 100_u16.to_le_bytes().to_vec()),
                bytes_field("QSDT", vec![0]),
                ctda_field(58),
                bytes_field("CIS2", b"::VarA\0".to_vec()),
                bytes_field("NAM2", b"first log\0".to_vec()),
                bytes_field("QSDT", vec![0]),
                ctda_field(59),
                bytes_field("NAM2", b"second log\0".to_vec()),
                bytes_field("INDX", 200_u16.to_le_bytes().to_vec()),
                bytes_field("QSDT", vec![0]),
                bytes_field("NAM2", b"next stage\0".to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "NEXT", //
                "INDX", "QSDT", "CTDA", "CIS2", "NAM2", "QSDT", "CTDA", "NAM2", //
                "INDX", "QSDT", "NAM2",
            ]
        );
    }

    #[test]
    fn drops_qust_objective_conditions_without_target_rows() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("QOBJ", 100_u16.to_le_bytes().to_vec()),
                uint_field("FNAM", 0),
                string_field("NNAM", "Objective A", &interner),
                ctda_field(58),
                bytes_field("CIS2", b"::ObjectiveA\0".to_vec()),
                bytes_field("QOBJ", 200_u16.to_le_bytes().to_vec()),
                uint_field("FNAM", 0),
                string_field("NNAM", "Objective B", &interner),
                ctda_field(59),
                bytes_field("ANAM", 1_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "QOBJ", "FNAM", "NNAM", "QOBJ", "FNAM", "NNAM", "ANAM",
            ]
        );
    }

    #[test]
    fn preserves_qust_objective_target_condition_rows() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("QOBJ", 100_u16.to_le_bytes().to_vec()),
                uint_field("FNAM", 0),
                string_field("NNAM", "Objective", &interner),
                bytes_field("QSTA", vec![0; 12]),
                ctda_field(58),
                bytes_field("CIS2", b"::TargetA\0".to_vec()),
                bytes_field("QSTA", vec![1; 12]),
                ctda_field(59),
                bytes_field("ANAM", 1_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "QOBJ", "FNAM", "NNAM", "QSTA", "CTDA", "CIS2", "QSTA", "CTDA", "ANAM",
            ]
        );
    }

    #[test]
    fn interleaves_quest_alias_scope() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALST", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ALDN", 11_u32.to_le_bytes().to_vec()),
                bytes_field("ALDN", 12_u32.to_le_bytes().to_vec()),
                bytes_field("ALPC", 21_u32.to_le_bytes().to_vec()),
                bytes_field("ALPC", 22_u32.to_le_bytes().to_vec()),
                bytes_field("VTCK", 31_u32.to_le_bytes().to_vec()),
                bytes_field("VTCK", 32_u32.to_le_bytes().to_vec()),
                bytes_field("ALCS", 41_u32.to_le_bytes().to_vec()),
                bytes_field("ALMI", vec![51]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "ALST", "ALDN", "ALPC", "VTCK", "ALCS", "ALMI", "ALST", "ALDN", "ALPC",
                "VTCK"
            ]
        );
    }

    #[test]
    fn preserves_interleaved_quest_alias_children_in_original_rows() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALDN", 11_u32.to_le_bytes().to_vec()),
                bytes_field("ALST", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ALPC", 22_u32.to_le_bytes().to_vec()),
                bytes_field("ALST", 3_u32.to_le_bytes().to_vec()),
                bytes_field("ALNT", 33_u32.to_le_bytes().to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec!["EDID", "ALST", "ALDN", "ALST", "ALPC", "ALST", "ALNT"]
        );
    }

    #[test]
    fn fo76_qust_alias_drops_null_display_name_but_keeps_null_voice_types() {
        let interner = StringInterner::new();
        let display_name = FormKey {
            plugin: interner.intern("SeventySix.esm"),
            local: 0x0012_3456,
        };
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                none_field("ALDN"),
                none_field("VTCK"),
                none_field("ALED"),
                bytes_field("ALST", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ALDN", vec![0; 4]),
                none_field("VTCK"),
                none_field("ALED"),
                bytes_field("ALST", 3_u32.to_le_bytes().to_vec()),
                formkey_field("ALDN", display_name),
                none_field("VTCK"),
                none_field("ALED"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "QUST", &interner)
        else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "ALST", "VTCK", "ALED", "ALST", "VTCK", "ALED", "ALST", "ALDN", "VTCK",
                "ALED"
            ]
        );
        let retained_display_names = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "ALDN")
            .collect::<Vec<_>>();
        assert_eq!(retained_display_names.len(), 1);
        assert!(matches!(
            retained_display_names[0].value,
            FieldValue::FormKey(form_key) if form_key == display_name
        ));
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "VTCK")
                .count(),
            3
        );
    }

    #[test]
    fn reorders_fo76_ordered_quest_alias_blocks_to_fo4_canonical() {
        // FO76 emits alias subrecords in an order FO4 rejects (e.g. FNAM/ALRT
        // before ALID). It can also put location-only KNAM in an ALST reference
        // alias. xEdit walks the alias scope with a forward-only cursor and flags
        // every backward jump as "out of order". This record interleaves a
        // reference alias (ALST), a location alias (ALLS), and a collection
        // alias (ALCS), each with children emitted in FO76 order.
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("ANAM", 3_u32.to_le_bytes().to_vec()),
                // Reference alias, FO76 child order: ALED last is fine, but
                // ALRT/ALFA emitted before ALID/FNAM is out of FO4 order.
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALRT", 7_u32.to_le_bytes().to_vec()),
                bytes_field("ALFA", 9_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"RefAlias\0".to_vec()),
                bytes_field("FNAM", 0x10_u32.to_le_bytes().to_vec()),
                bytes_field("KNAM", 0x20_u32.to_le_bytes().to_vec()),
                none_field("ALED"),
                // Location alias.
                bytes_field("ALLS", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ALFA", 0x30_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"LocAlias\0".to_vec()),
                bytes_field("FNAM", 0x40_u32.to_le_bytes().to_vec()),
                bytes_field("KNAM", 0x50_u32.to_le_bytes().to_vec()),
                none_field("ALED"),
                // Collection alias.
                bytes_field("ALCS", 4_u32.to_le_bytes().to_vec()),
                bytes_field("ALMI", vec![5]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        // xEdit resets its alias-struct cursor at each ALST/ALLS/ALCS anchor, so
        // the per-block intra-order is what it validates; assert the exact output.
        // (The flat `assert_cursor_accepts` helper does not model the anchor
        // reset across the three variants, so it is not used for interleaved
        // anchors.)
        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "ANAM", //
                "ALST", "ALID", "FNAM", "ALFA", "ALRT", "ALED", //
                "ALLS", "ALID", "FNAM", "ALFA", "KNAM", "ALED", //
                "ALCS", "ALMI",
            ]
        );
        let retained_knam = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "KNAM")
            .collect::<Vec<_>>();
        assert_eq!(retained_knam.len(), 1);
        assert!(matches!(
            &retained_knam[0].value,
            FieldValue::Bytes(bytes) if bytes.as_slice() == 0x50_u32.to_le_bytes()
        ));
    }

    #[test]
    fn reorders_quest_alias_with_location_alias_between_reference_aliases() {
        // FO76 interleaves reference (ALST) and location (ALLS) aliases; each
        // alias's children must stay grouped with its own anchor rather than
        // being swallowed into the next same-variant anchor's row.
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("ANAM", 4_u32.to_le_bytes().to_vec()),
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"Ref1\0".to_vec()),
                bytes_field("FNAM", 0x10_u32.to_le_bytes().to_vec()),
                none_field("ALED"),
                bytes_field("ALLS", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"Loc\0".to_vec()),
                bytes_field("FNAM", 0x20_u32.to_le_bytes().to_vec()),
                bytes_field("ALFA", 0x21_u32.to_le_bytes().to_vec()),
                none_field("ALED"),
                bytes_field("ALST", 3_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"Ref2\0".to_vec()),
                bytes_field("FNAM", 0x30_u32.to_le_bytes().to_vec()),
                none_field("ALED"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "ANAM", //
                "ALST", "ALID", "FNAM", "ALED", //
                "ALLS", "ALID", "FNAM", "ALFA", "ALED", //
                "ALST", "ALID", "FNAM", "ALED",
            ]
        );
    }

    #[test]
    fn reorders_quest_alias_body_children_to_fo4_schema_order() {
        // FO76 emits alias body children in an order FO4 rejects (ALFA/ALCC
        // before ALID/FNAM) and may carry location-only KNAM in an ALST row.
        // Rebuild the legal children and drop that illegal reference-alias KNAM.
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("ANAM", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALCC", 7_i32.to_le_bytes().to_vec()),
                bytes_field("ALFA", 9_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"A1\0".to_vec()),
                bytes_field("KNAM", 0x20_u32.to_le_bytes().to_vec()),
                bytes_field("FNAM", 0x10_u32.to_le_bytes().to_vec()),
                none_field("ALED"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_cursor_accepts(&record);
        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "ANAM", //
                "ALST", "ALID", "FNAM", "ALFA", "ALCC", "ALED",
            ]
        );
    }

    #[test]
    fn qust_dialogue_conditions_do_not_steal_alias_conditions() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                ctda_field(58),
                bytes_field("CIS1", b"TopLevel\0".to_vec()),
                none_field("NEXT"),
                bytes_field("INDX", 10_u16.to_le_bytes().to_vec()),
                bytes_field("QSDT", vec![0]),
                bytes_field("ANAM", 2_u32.to_le_bytes().to_vec()),
                bytes_field("ALST", 1_u32.to_le_bytes().to_vec()),
                bytes_field("ALID", b"Owner\0".to_vec()),
                bytes_field("FNAM", 0_u32.to_le_bytes().to_vec()),
                ctda_field(566),
                bytes_field("CIS1", b"AliasCondition\0".to_vec()),
                none_field("ALED"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_cursor_accepts(&record);
        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "CTDA", "CIS1", "NEXT", "INDX", "QSDT", "ANAM", "ALST", "ALID", "FNAM",
                "CTDA", "CIS1", "ALED",
            ]
        );
    }

    #[test]
    fn drops_duplicate_singleton_subrecords() {
        let interner = StringInterner::new();
        let record = record(
            "QUST",
            vec![
                bytes_field("EDID", b"Quest\0".to_vec()),
                bytes_field("NEXT", vec![1, 0, 0, 0]),
                bytes_field("NEXT", vec![2, 0, 0, 0]),
                bytes_field("ANAM", vec![0]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("QUST should be supported");
        };

        assert_eq!(sigs(&record), vec!["EDID", "NEXT", "ANAM"]);
    }

    #[test]
    fn syncs_keyword_count_to_keyword_array_length() {
        let interner = StringInterner::new();
        let mut kwda = Vec::new();
        for raw in [0x0000_1234_u32, 0x0000_5678, 0x0000_9ABC] {
            kwda.extend_from_slice(&raw.to_le_bytes());
        }
        let record = record(
            "WEAP",
            vec![
                bytes_field("EDID", b"Weapon\0".to_vec()),
                bytes_field("KSIZ", 1_u32.to_le_bytes().to_vec()),
                bytes_field("KWDA", kwda),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("WEAP should be supported");
        };

        let ksiz = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "KSIZ")
            .expect("KSIZ should remain");
        let FieldValue::Bytes(bytes) = &ksiz.value else {
            panic!("KSIZ should be bytes");
        };
        assert_eq!(u32::from_le_bytes(bytes[..4].try_into().unwrap()), 3);
    }

    #[test]
    fn truncates_raw_fixed_struct_to_target_length() {
        let interner = StringInterner::new();
        let record = record(
            "RACE",
            vec![bytes_field("ATKD", (0..48).collect())],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.len(), 44);
        assert_eq!(bytes.as_slice(), &(0..44).collect::<Vec<u8>>());
    }

    #[test]
    fn normalizes_fo76_rfct_data_flags_into_fo4_slot() {
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&0x0048_CC2A_u32.to_le_bytes());
        raw.extend_from_slice(&0x0000_0006_u32.to_le_bytes());
        let record = record("RFCT", vec![bytes_field("DATA", raw)], &interner);

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "RFCT", &interner)
        else {
            panic!("RFCT should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(
            u32::from_le_bytes(bytes[0..4].try_into().unwrap()),
            0x0048_CC2A
        );
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 0);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 6);
    }

    #[test]
    fn normalizes_fo76_qust_target_without_treating_radius_as_keyword() {
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        raw.extend_from_slice(&3_i32.to_le_bytes());
        raw.extend_from_slice(&512_u16.to_le_bytes());
        raw.extend_from_slice(&0_u32.to_le_bytes());
        raw.extend_from_slice(&3000_f32.to_le_bytes());
        let record = record("QUST", vec![bytes_field("QSTA", raw)], &interner);

        let TargetRecordNormalization::Keep(record) =
            normalize_from_fo76_source(record, "QUST", &interner)
        else {
            panic!("QUST should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.len(), 12);
        assert_eq!(i32::from_le_bytes(bytes[0..4].try_into().unwrap()), 3);
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 512);
        assert_eq!(u32::from_le_bytes(bytes[8..12].try_into().unwrap()), 0);
    }

    #[test]
    fn projects_fo76_race_prps_rows_to_fo4_row_shape() {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let source_race = source_schema.record_def("RACE").expect("fo76 RACE");
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        for row in 0..3_u32 {
            raw.extend_from_slice(&(0x0000_1000 + row).to_le_bytes());
            raw.extend_from_slice(&(row as f32 + 0.5).to_le_bytes());
            raw.extend_from_slice(&(0x0000_2000 + row).to_le_bytes());
        }
        let record = record("RACE", vec![bytes_field("PRPS", raw)], &interner);

        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: Some(source_race),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("RACE should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.len(), 24);
        assert_eq!(&bytes[0..4], &0x0000_1000_u32.to_le_bytes());
        assert_eq!(&bytes[8..12], &0x0000_1001_u32.to_le_bytes());
        assert_eq!(&bytes[16..20], &0x0000_1002_u32.to_le_bytes());
    }

    #[test]
    fn preserves_fo76_cont_prps_payload_when_already_fo4_row_shape() {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let source_cont = source_schema.record_def("CONT").expect("fo76 CONT");
        let interner = StringInterner::new();
        let mut raw = Vec::new();
        for (actor_value, value) in [
            (0x0033_D522_u32, 2.0_f32),
            (0x0000_038A, 1.0),
            (0x0022_33CB, 1.0),
            (0x007F_97E6, 1.0),
            (0x0080_82A5, 1.0),
            (0x0000_02DC, 10.0),
        ] {
            raw.extend_from_slice(&actor_value.to_le_bytes());
            raw.extend_from_slice(&value.to_le_bytes());
        }
        let record = record("CONT", vec![bytes_field("PRPS", raw.clone())], &interner);

        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: Some(source_cont),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("CONT should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(bytes.as_slice(), raw.as_slice());
    }

    #[test]
    fn collapses_source_array_to_target_singleton_value() {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let source_acti = source_schema.record_def("ACTI").expect("fo76 ACTI");
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let record = record(
            "ACTI",
            vec![FieldEntry {
                sig: SubrecordSig::from_str("FTYP").unwrap(),
                value: FieldValue::List(vec![
                    FieldValue::FormKey(FormKey {
                        plugin: fallout4,
                        local: 0x0000_1000,
                    }),
                    FieldValue::FormKey(FormKey {
                        plugin: fallout4,
                        local: 0x0000_2000,
                    }),
                ]),
            }],
            &interner,
        );

        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: Some(source_acti),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("ACTI should be supported");
        };

        assert_eq!(record.fields.len(), 1);
        let FieldValue::FormKey(form_key) = &record.fields[0].value else {
            panic!("FTYP should collapse to one target FormKey");
        };
        assert_eq!(form_key.local, 0x0000_1000);
    }

    #[test]
    fn raw_target_uses_source_fixed_size_for_scalar_values() {
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let source_schema = AuthoringSchema::for_game("fo76").expect("fo76 schema");
        let source_tact = source_schema.record_def("TACT").expect("fo76 TACT");
        let interner = StringInterner::new();
        let record = record("TACT", vec![uint_field("FNAM", 0x1234)], &interner);

        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: Some(source_tact),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("TACT should be supported");
        };

        let FieldValue::Bytes(bytes) = &record.fields[0].value else {
            panic!("FNAM should become fixed-width bytes");
        };
        assert_eq!(bytes.as_slice(), &[0x34, 0x12]);
    }

    #[test]
    fn normalizes_nested_struct_fields_to_target_order_and_sizes() {
        let interner = StringInterner::new();
        let record = record(
            "ARMA",
            vec![struct_field(
                "DNAM",
                vec![
                    (interner.intern("fo76_only"), FieldValue::Uint(99)),
                    (interner.intern("Weapon Adjust"), FieldValue::Float(1.25)),
                    (interner.intern("unknown_u8_7"), FieldValue::Uint(8)),
                    (
                        interner.intern("detection_sound_value"),
                        FieldValue::Uint(7),
                    ),
                    (interner.intern("unknown_u8_5"), FieldValue::Uint(6)),
                    (interner.intern("unknown_u8_4"), FieldValue::Uint(5)),
                    (interner.intern("weight_slider_female"), FieldValue::Uint(4)),
                    (interner.intern("weight_slider_male"), FieldValue::Uint(3)),
                    (interner.intern("female_priority"), FieldValue::Uint(2)),
                    (interner.intern("male_priority"), FieldValue::Uint(1)),
                ],
            )],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("ARMA should be supported");
        };

        let FieldValue::Struct(fields) = &record.fields[0].value else {
            panic!("DNAM should remain a struct");
        };
        let names = fields
            .iter()
            .map(|(key, _)| interner.resolve(*key).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec![
                "male_priority",
                "female_priority",
                "weight_slider_male",
                "weight_slider_female",
                "unknown_u8_4",
                "unknown_u8_5",
                "detection_sound_value",
                "unknown_u8_7",
                "Weapon Adjust",
            ]
        );
        let encoded_size: usize = fields
            .iter()
            .map(|(_, value)| match value {
                FieldValue::Bytes(bytes) => bytes.len(),
                other => panic!("expected nested fixed field bytes, got {other:?}"),
            })
            .sum();
        assert_eq!(encoded_size, 12);
    }

    #[test]
    fn normalizes_nested_list_struct_items() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let damage_type = FormKey {
            plugin: fallout4,
            local: 0x0000_1234,
        };
        let record = record(
            "ARMO",
            vec![
                bytes_field("DEST", vec![0; 8]),
                FieldEntry {
                    sig: SubrecordSig::from_str("DAMC").unwrap(),
                    value: FieldValue::List(vec![FieldValue::Struct(vec![
                        (interner.intern("extra"), FieldValue::Uint(99)),
                        (interner.intern("Resistances Value"), FieldValue::Uint(50)),
                        (
                            interner.intern("resistances_damage_type"),
                            FieldValue::FormKey(damage_type),
                        ),
                    ])]),
                },
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("ARMO should be supported");
        };

        let damc = record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DAMC")
            .expect("DAMC should remain when scoped anchor is present");
        let FieldValue::List(items) = &damc.value else {
            panic!("DAMC should remain a list");
        };
        let FieldValue::Struct(fields) = &items[0] else {
            panic!("DAMC row should remain a struct");
        };
        let names = fields
            .iter()
            .map(|(key, _)| interner.resolve(*key).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["resistances_damage_type", "Resistances Value"]);
        assert!(matches!(&fields[0].1, FieldValue::FormKey(_)));
        let FieldValue::Bytes(bytes) = &fields[1].1 else {
            panic!("resistance value should be fixed-width bytes");
        };
        assert_eq!(bytes.as_slice(), &50_u32.to_le_bytes());
    }

    #[test]
    fn preserves_all_destruction_stages() {
        // Regression: VaultTecVan01 (MSTT:081107) — the FO76 van carries five
        // destruction stages (DEST header + 5×DSTD, one bearing DMDL/DMDT), two
        // of them with Explosion refs. The default scoped emit anchored rows on
        // DEST (a singleton) and collapsed all five stages to one, dropping the
        // Explosion stages so the vehicle never caught fire or exploded.
        let interner = StringInterner::new();
        let record = record(
            "MSTT",
            vec![
                bytes_field("DEST", vec![0; 8]),
                bytes_field("DSTD", vec![98, 0, 1, 0, 2, 0, 0, 0]),
                none_field("DSTF"),
                bytes_field("DSTD", vec![85, 1, 2, 0, 8, 0, 0, 0]),
                none_field("DSTF"),
                bytes_field("DSTD", vec![75, 2, 3, 0, 24, 0, 0, 0]),
                none_field("DSTF"),
                bytes_field("DSTD", vec![50, 3, 4, 0, 1, 0, 0, 0]),
                string_field(
                    "DMDL",
                    "Vehicles\\Automotive\\VaultTecVan01Hulk.nif",
                    &interner,
                ),
                bytes_field("DMDT", vec![0; 12]),
                none_field("DSTF"),
                bytes_field("DSTD", vec![0, 4, 5, 0, 0, 0, 0, 0]),
                none_field("DSTF"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("MSTT should be supported");
        };

        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(
            sigs.iter().filter(|s| **s == "DSTD").count(),
            5,
            "all five destruction stages must survive, got {sigs:?}"
        );
        assert_eq!(
            sigs.iter().filter(|s| **s == "DSTF").count(),
            5,
            "each stage keeps its end marker, got {sigs:?}"
        );
        assert_eq!(
            sigs,
            vec![
                "DEST", "DSTD", "DSTF", "DSTD", "DSTF", "DSTD", "DSTF", "DSTD", "DMDL", "DMDT",
                "DSTF", "DSTD", "DSTF",
            ],
            "stage grouping is preserved in source order"
        );
    }

    #[test]
    fn drops_complete_alternative_destruction_model_rows_idempotently() {
        let interner = StringInterner::new();
        let record = record(
            "MSTT",
            vec![
                bytes_field("DEST", vec![0; 8]),
                bytes_field("DSTD", vec![0; 24]),
                string_field("DSTA", "Destroy", &interner),
                string_field("DMDL", "canonical.nif", &interner),
                bytes_field("DMDT", vec![1; 12]),
                bytes_field("DMDC", 1.0_f32.to_le_bytes().to_vec()),
                bytes_field("DMDS", 0x1234_u32.to_le_bytes().to_vec()),
                bytes_field("ENLT", vec![255; 4]),
                bytes_field("ENLS", 1.0_f32.to_le_bytes().to_vec()),
                bytes_field("AUUV", vec![0; 32]),
                string_field("DMDL", "alternative02.nif", &interner),
                bytes_field("DMDT", vec![2; 12]),
                bytes_field("DMDC", 2.0_f32.to_le_bytes().to_vec()),
                bytes_field("DMDS", 0x5678_u32.to_le_bytes().to_vec()),
                bytes_field("ENLT", vec![255; 4]),
                string_field("DMDL", "alternative03.nif", &interner),
                bytes_field("DMDT", vec![3; 12]),
                bytes_field("DMDC", 3.0_f32.to_le_bytes().to_vec()),
                bytes_field("DMDS", 0x9abc_u32.to_le_bytes().to_vec()),
                none_field("DSTF"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(once) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("MSTT should be supported");
        };
        let TargetRecordNormalization::Keep(twice) =
            normalize_target_only_with_interner(once.clone(), &interner)
        else {
            panic!("normalized MSTT should remain supported");
        };

        assert_eq!(
            sigs(&once),
            vec![
                "DEST", "DSTD", "DSTA", "DMDL", "DMDT", "DMDC", "DMDS", "DSTF"
            ]
        );
        assert_eq!(twice.fields, once.fields);
        let dmdl = once
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "DMDL")
            .expect("canonical DMDL");
        let FieldValue::String(path) = dmdl.value else {
            panic!("DMDL should be a string");
        };
        assert_eq!(interner.resolve(path), Some("canonical.nif"));
    }

    #[test]
    fn keeps_first_model_in_each_distinct_destruction_stage() {
        let interner = StringInterner::new();
        let record = record(
            "MSTT",
            vec![
                bytes_field("DEST", vec![0; 8]),
                bytes_field("DSTD", vec![0; 24]),
                string_field("DMDL", "stage1.nif", &interner),
                bytes_field("DMDT", vec![1; 12]),
                string_field("DMDL", "stage1_alt.nif", &interner),
                bytes_field("DMDT", vec![2; 12]),
                none_field("DSTF"),
                bytes_field("DSTD", vec![0; 24]),
                string_field("DMDL", "stage2.nif", &interner),
                bytes_field("DMDT", vec![3; 12]),
                string_field("DMDL", "stage2_alt.nif", &interner),
                bytes_field("DMDT", vec![4; 12]),
                none_field("DSTF"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("MSTT should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "DEST", "DSTD", "DMDL", "DMDT", "DSTF", "DSTD", "DMDL", "DMDT", "DSTF"
            ]
        );
        let paths = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "DMDL")
            .map(|field| {
                let FieldValue::String(path) = field.value else {
                    panic!("DMDL should be a string");
                };
                interner.resolve(path).unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(paths, ["stage1.nif", "stage2.nif"]);
    }

    #[test]
    fn leaves_valid_single_model_destruction_stage_unchanged() {
        let interner = StringInterner::new();
        let record = record(
            "MSTT",
            vec![
                bytes_field("DEST", vec![0; 8]),
                bytes_field("DSTD", vec![0; 24]),
                string_field("DSTA", "Destroy", &interner),
                string_field("DMDL", "only.nif", &interner),
                bytes_field("DMDT", vec![1; 12]),
                bytes_field("DMDC", 1.0_f32.to_le_bytes().to_vec()),
                bytes_field("DMDS", 0x1234_u32.to_le_bytes().to_vec()),
                none_field("DSTF"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("MSTT should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "DEST", "DSTD", "DSTA", "DMDL", "DMDT", "DMDC", "DMDS", "DSTF"
            ]
        );
    }

    #[test]
    fn does_not_cross_pair_dmdt_when_canonical_model_lacks_one() {
        let interner = StringInterner::new();
        let record = record(
            "MSTT",
            vec![
                bytes_field("DEST", vec![0; 8]),
                bytes_field("DSTD", vec![0; 24]),
                string_field("DMDL", "canonical_without_dmdt.nif", &interner),
                string_field("DMDL", "alternative.nif", &interner),
                bytes_field("DMDT", vec![2; 12]),
                none_field("DSTF"),
                bytes_field("DSTD", vec![0; 24]),
                string_field("DMDL", "next_stage.nif", &interner),
                bytes_field("DMDT", vec![3; 12]),
                none_field("DSTF"),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) =
            normalize_target_only_with_interner(record, &interner)
        else {
            panic!("MSTT should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "DEST", "DSTD", "DMDL", "DSTF", "DSTD", "DMDL", "DMDT", "DSTF"
            ]
        );
        let paths = record
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "DMDL")
            .map(|field| {
                let FieldValue::String(path) = field.value else {
                    panic!("DMDL should be a string");
                };
                interner.resolve(path).unwrap().to_string()
            })
            .collect::<Vec<_>>();
        assert_eq!(paths, ["canonical_without_dmdt.nif", "next_stage.nif"]);
    }

    fn alla_bytes(rows: &[(u32, i32)]) -> FieldValue {
        let mut bytes = Vec::new();
        for (keyword, alias_index) in rows {
            bytes.extend_from_slice(&keyword.to_le_bytes());
            bytes.extend_from_slice(&alias_index.to_le_bytes());
        }
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn alla_rows(value: &FieldValue) -> Vec<(u32, i32)> {
        let FieldValue::Bytes(bytes) = value else {
            panic!("ALLA must be bytes");
        };
        bytes
            .chunks_exact(8)
            .map(|c| {
                (
                    u32::from_le_bytes([c[0], c[1], c[2], c[3]]),
                    i32::from_le_bytes([c[4], c[5], c[6], c[7]]),
                )
            })
            .collect()
    }

    #[test]
    fn drops_only_dangling_alla_linked_aliases() {
        let alias_ids: HashSet<u32> = [0, 1, 2, 19].into_iter().collect();
        // Mirrors RE_TravelKMK03 (QUST:0464D5): keep refs 1 and 19, drop the
        // phantom 8..13 the FO76 source carries.
        let mut value = alla_bytes(&[
            (0x0002_FD66, 1),
            (0x0002_FD66, 8),
            (0x0002_FD66, 9),
            (0x0002_FD66, 19),
            (0x0002_FD66, 13),
        ]);
        let now_empty = drop_dangling_alla_links(&mut value, &alias_ids);
        assert!(!now_empty);
        assert_eq!(alla_rows(&value), vec![(0x0002_FD66, 1), (0x0002_FD66, 19)]);
    }

    #[test]
    fn drops_whole_alla_when_every_link_is_dangling() {
        let alias_ids: HashSet<u32> = [0, 1, 2].into_iter().collect();
        let mut value = alla_bytes(&[(0, 8), (0, 9), (0, 13)]);
        assert!(
            drop_dangling_alla_links(&mut value, &alias_ids),
            "all links dangling → subrecord becomes empty and is dropped"
        );
    }

    #[test]
    fn keeps_alla_when_all_links_valid() {
        let alias_ids: HashSet<u32> = [0, 1, 2].into_iter().collect();
        let mut value = alla_bytes(&[(0x1234, 0), (0, 2)]);
        let before = alla_rows(&value);
        assert!(!drop_dangling_alla_links(&mut value, &alias_ids));
        assert_eq!(alla_rows(&value), before, "byte-identical when all valid");
    }

    #[test]
    fn collect_alias_ids_reads_uint_and_byte_anchors() {
        let interner = StringInterner::new();
        let mut fields_by_sig: HashMap<crate::ids::SubrecordSig, VecDeque<IndexedFieldEntry>> =
            HashMap::new();
        let mut push = |sig: &str, value: FieldValue, idx: usize| {
            let s = crate::ids::SubrecordSig::from_str(sig).unwrap();
            fields_by_sig
                .entry(s)
                .or_default()
                .push_back(IndexedFieldEntry {
                    original_index: idx,
                    entry: FieldEntry {
                        sig: crate::ids::SubrecordSig::from_str(sig).unwrap(),
                        value,
                    },
                });
        };
        push("ALST", FieldValue::Uint(0), 0);
        push("ALST", FieldValue::Uint(2), 1);
        push(
            "ALLS",
            FieldValue::Bytes(SmallVec::from_slice(&7u32.to_le_bytes())),
            2,
        );
        push("ALCS", FieldValue::Int(19), 3);

        let anchors: Vec<_> = ["ALST", "ALLS", "ALCS"]
            .iter()
            .map(|s| crate::ids::SubrecordSig::from_str(s).unwrap())
            .collect();
        let ids = collect_alias_ids(&anchors, &fields_by_sig);
        assert_eq!(ids, [0, 2, 7, 19].into_iter().collect());
    }

    // Creature RACE behavior subgraph block that starts with SGNM (no SAKD
    // anchor) must SURVIVE normalize. A SAKD-anchored walk would drop the whole
    // block (FO76 creature races like ScorchTongueBody → CK "Could not find base
    // MT/weapon graph").
    #[test]
    fn subgraph_data_block_without_sakd_anchor_survives() {
        let interner = StringInterner::new();
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"CreatureRace\0".to_vec()),
                bytes_field(
                    "SGNM",
                    b"Actors\\X\\Behaviors\\XCoreBehavior.hkx\0".to_vec(),
                ),
                bytes_field("SAPT", b"Actors\\X\\Animations\0".to_vec()),
                bytes_field("SRAF", vec![0u8; 4]),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        let s = sigs(&record);
        assert!(s.contains(&"SGNM"), "SGNM must survive, got {s:?}");
        assert!(s.contains(&"SAPT"), "SAPT must survive, got {s:?}");
    }

    // A subgraph block WITH the SAKD anchor must also be preserved.
    #[test]
    fn subgraph_data_block_with_sakd_anchor_preserved() {
        let interner = StringInterner::new();
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"CreatureRace\0".to_vec()),
                bytes_field("SAKD", vec![0u8; 4]),
                bytes_field(
                    "SGNM",
                    b"Actors\\X\\Behaviors\\XCoreBehavior.hkx\0".to_vec(),
                ),
                bytes_field("SAPT", b"Actors\\X\\Animations\0".to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        let s = sigs(&record);
        assert!(s.contains(&"SAKD"), "SAKD must survive, got {s:?}");
        assert!(s.contains(&"SGNM"), "SGNM must survive, got {s:?}");
        assert!(s.contains(&"SAPT"), "SAPT must survive, got {s:?}");
    }

    #[test]
    fn race_equip_slots_do_not_shift_nodes_after_slot_without_node() {
        let interner = StringInterner::new();
        let record = record(
            "RACE",
            vec![
                bytes_field("EDID", b"CreatureRace\0".to_vec()),
                uint_field("QNAM", 0x0001_3F42),
                bytes_field("ZNAM", b"Weapon\0".to_vec()),
                uint_field("QNAM", 0x0001_3F43),
                bytes_field("ZNAM", b"WeaponLeft\0".to_vec()),
                uint_field("QNAM", 0x0010_FFFB),
                bytes_field("ZNAM", b"Weapon\0".to_vec()),
                uint_field("QNAM", 0x0004_334F),
                uint_field("QNAM", 0x0010_FFFA),
                bytes_field("ZNAM", b"WeaponLeft\0".to_vec()),
                uint_field("QNAM", 0x0013_6255),
                bytes_field("ZNAM", b"Head\0".to_vec()),
                uint_field("QNAM", 0x0004_9BE1),
                bytes_field("ZNAM", b"RToe2\0".to_vec()),
            ],
            &interner,
        );

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        assert_eq!(
            sigs(&record),
            vec![
                "EDID", "QNAM", "ZNAM", "QNAM", "ZNAM", "QNAM", "ZNAM", "QNAM", "QNAM", "ZNAM",
                "QNAM", "ZNAM", "QNAM", "ZNAM"
            ]
        );
        let nodes: Vec<_> = record
            .fields
            .iter()
            .filter_map(|field| match (&field.sig.as_str(), &field.value) {
                (&"ZNAM", FieldValue::Bytes(bytes)) => {
                    Some(String::from_utf8_lossy(bytes.as_slice()).into_owned())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            nodes,
            vec![
                "Weapon\0",
                "WeaponLeft\0",
                "Weapon\0",
                "WeaponLeft\0",
                "Head\0",
                "RToe2\0"
            ]
        );
    }

    #[test]
    fn race_behavior_graph_repairs_fo76_marker_tail_to_fo4_pair() {
        let interner = StringInterner::new();
        let mut fields = race_fields_before_behavior_graph(b"FishermanRace\0");
        fields.extend([
            bytes_field("NAM3", Vec::new()),
            bytes_field("MNAM", Vec::new()),
            bytes_field("MODL", b"actors\\Character\\RaiderProject.hkx\0".to_vec()),
            bytes_field("MODT", vec![5]),
            bytes_field("MNAM", Vec::new()),
            bytes_field("MODL", b"actors\\Character\\RaiderProject.hkx\0".to_vec()),
            bytes_field("MODT", vec![6]),
            bytes_field("FNAM", Vec::new()),
            bytes_field("FNAM", Vec::new()),
            bytes_field("NAM4", vec![0; 4]),
        ]);
        let record = record("RACE", fields, &interner);

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        assert_eq!(
            sigs_from(&record, "GNAM"),
            vec![
                "GNAM", "NAM3", "MNAM", "MODL", "MODT", "FNAM", "MODL", "MODT", "NAM4"
            ]
        );
        assert_cursor_accepts(&record);
    }

    #[test]
    fn race_behavior_graph_collapses_empty_duplicate_markers() {
        let interner = StringInterner::new();
        let mut fields = race_fields_before_behavior_graph(b"SuperMutantRustKingRace\0");
        fields.extend([
            bytes_field("NAM3", Vec::new()),
            bytes_field("MNAM", Vec::new()),
            bytes_field("MNAM", Vec::new()),
            bytes_field("FNAM", Vec::new()),
            bytes_field("FNAM", Vec::new()),
            bytes_field("NAM4", vec![0; 4]),
        ]);
        let record = record("RACE", fields, &interner);

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        assert_eq!(
            sigs_from(&record, "GNAM"),
            vec!["GNAM", "NAM3", "MNAM", "FNAM", "NAM4"]
        );
        assert_cursor_accepts(&record);
    }

    #[test]
    fn race_behavior_graph_preserves_valid_fo4_pair() {
        let interner = StringInterner::new();
        let mut fields = race_fields_before_behavior_graph(b"HumanRace\0");
        fields.extend([
            bytes_field("NAM3", Vec::new()),
            bytes_field("MNAM", Vec::new()),
            bytes_field("MODL", b"actors\\Character\\RaiderProject.hkx\0".to_vec()),
            bytes_field("MODT", vec![5]),
            bytes_field("FNAM", Vec::new()),
            bytes_field("MODL", b"actors\\Character\\RaiderProject.hkx\0".to_vec()),
            bytes_field("MODT", vec![6]),
            bytes_field("NAM4", vec![0; 4]),
        ]);
        let record = record("RACE", fields, &interner);

        let TargetRecordNormalization::Keep(record) = normalize_target_only(record) else {
            panic!("RACE should be supported");
        };

        assert_eq!(
            sigs_from(&record, "GNAM"),
            vec![
                "GNAM", "NAM3", "MNAM", "MODL", "MODT", "FNAM", "MODL", "MODT", "NAM4"
            ]
        );
        assert_cursor_accepts(&record);
    }

    #[test]
    fn legacy_xlkr_normalizes_exact_bytes_for_fnv_and_fo3() {
        let interner = StringInterner::new();
        let linked_ref = 0x0714_851E_u32.to_le_bytes();
        let mut expected = Vec::from(0_u32.to_le_bytes());
        expected.extend_from_slice(&linked_ref);

        for source_game in ["fnv", "fo3"] {
            for record_sig in ["REFR", "ACHR"] {
                let normalized = normalize_from_source_game(
                    record(
                        record_sig,
                        vec![bytes_field("XLKR", linked_ref.to_vec())],
                        &interner,
                    ),
                    source_game,
                    &interner,
                );
                assert_eq!(raw_field_bytes(&normalized, "XLKR"), expected);

                let normalized_again =
                    normalize_from_source_game(normalized, source_game, &interner);
                assert_eq!(raw_field_bytes(&normalized_again, "XLKR"), expected);
            }
        }
    }

    #[test]
    fn legacy_xown_normalizes_four_and_five_byte_payloads_for_fnv_and_fo3() {
        let interner = StringInterner::new();
        let owner = 0x0001_2345_u32.to_le_bytes();
        let mut expected = vec![0_u8; 12];
        expected[..4].copy_from_slice(&owner);

        for source_game in ["fnv", "fo3"] {
            for record_sig in ["REFR", "PGRE", "CELL"] {
                for raw in [owner.to_vec(), [owner.as_slice(), &[0]].concat()] {
                    let normalized = normalize_from_source_game(
                        record(record_sig, vec![bytes_field("XOWN", raw)], &interner),
                        source_game,
                        &interner,
                    );
                    assert_eq!(raw_field_bytes(&normalized, "XOWN"), expected);

                    let normalized_again =
                        normalize_from_source_game(normalized, source_game, &interner);
                    assert_eq!(raw_field_bytes(&normalized_again, "XOWN"), expected);
                }
            }
        }
    }

    #[test]
    fn fo76_refr_xprm_relayouts_short_payload_and_preserves_full_payload() {
        let interner = StringInterner::new();
        let mut short = Vec::new();
        short.extend_from_slice(&4096.0_f32.to_le_bytes());
        short.extend_from_slice(&2048.0_f32.to_le_bytes());
        short.extend_from_slice(&1024.0_f32.to_le_bytes());
        short.extend_from_slice(&1_u32.to_le_bytes());

        let mut expected = short[..12].to_vec();
        for _ in 0..4 {
            expected.extend_from_slice(&1.0_f32.to_le_bytes());
        }
        expected.extend_from_slice(&short[12..16]);

        let normalized = normalize_from_source_game(
            record("REFR", vec![bytes_field("XPRM", short)], &interner),
            "fo76",
            &interner,
        );
        assert_eq!(raw_field_bytes(&normalized, "XPRM"), expected);

        let normalized_again = normalize_from_source_game(normalized, "fo76", &interner);
        assert_eq!(raw_field_bytes(&normalized_again, "XPRM"), expected);

        let mut full = Vec::new();
        for value in [32.0_f32, 64.0, 96.0, 0.25, 0.5, 0.75, 0.875] {
            full.extend_from_slice(&value.to_le_bytes());
        }
        full.extend_from_slice(&2_u32.to_le_bytes());
        let normalized_full = normalize_from_source_game(
            record("REFR", vec![bytes_field("XPRM", full.clone())], &interner),
            "fo76",
            &interner,
        );
        assert_eq!(raw_field_bytes(&normalized_full, "XPRM"), full);
    }

    #[test]
    fn legacy_note_data_appends_zero_weight_for_fnv_and_fo3() {
        let interner = StringInterner::new();
        let padded_type = [3, 0xAA, 0xBB, 0xCC];
        let expected = [3, 0, 0, 0, 0, 0, 0, 0];

        for source_game in ["fnv", "fo3"] {
            let normalized = normalize_from_source_game(
                record(
                    "NOTE",
                    vec![bytes_field("DATA", padded_type.to_vec())],
                    &interner,
                ),
                source_game,
                &interner,
            );
            assert_eq!(raw_field_bytes(&normalized, "DATA"), expected);

            let normalized_again = normalize_from_source_game(normalized, source_game, &interner);
            assert_eq!(raw_field_bytes(&normalized_again, "DATA"), expected);
        }
    }

    #[test]
    fn legacy_width_normalization_is_source_schema_gated() {
        let interner = StringInterner::new();
        for (record_sig, field_sig) in [
            ("REFR", "XLKR"),
            ("ACHR", "XLKR"),
            ("REFR", "XOWN"),
            ("PGRE", "XOWN"),
            ("CELL", "XOWN"),
            ("NOTE", "DATA"),
        ] {
            let raw = 0x1234_5678_u32.to_le_bytes().to_vec();
            let TargetRecordNormalization::Keep(target_only) = normalize_target_only(record(
                record_sig,
                vec![bytes_field(field_sig, raw.clone())],
                &interner,
            )) else {
                panic!("record should be supported");
            };
            assert_eq!(raw_field_bytes(&target_only, field_sig), raw);

            let fo76 = normalize_from_source_game(
                record(
                    record_sig,
                    vec![bytes_field(field_sig, raw.clone())],
                    &interner,
                ),
                "fo76",
                &interner,
            );
            assert_eq!(raw_field_bytes(&fo76, field_sig), raw);
        }
    }

    #[test]
    fn legacy_widths_validate_after_fo4_save_and_reload() {
        let interner = StringInterner::new();
        let target_schema = AuthoringSchema::for_game("fo4").expect("fo4 schema");
        let plugin_name = "LegacyWidths.esp";
        let source_plugin = interner.intern("FNV_FO3_Merged.esm");
        let output_plugin = interner.intern(plugin_name);
        let source_refs =
            [0x0010_01, 0x0010_02, 0x0010_03, 0x0010_04, 0x0010_05].map(|local| FormKey {
                plugin: source_plugin,
                local,
            });
        let target_refs =
            [0x0011_11, 0x0022_22, 0x0033_33, 0x0044_44, 0x0055_55].map(|local| FormKey {
                plugin: output_plugin,
                local,
            });
        let mut mapper = crate::formkey_mapper::FormKeyMapper::new(
            [],
            crate::formkey_mapper::MapperOptions {
                output_plugin_name: plugin_name.to_string(),
                ..Default::default()
            },
            &interner,
        );
        for (source, target) in source_refs.into_iter().zip(target_refs) {
            mapper.add_mapping(source, target);
        }

        let source_records = [
            (
                "fnv",
                record(
                    "REFR",
                    vec![
                        formkey_field("XLKR", source_refs[0]),
                        formkey_field("XOWN", source_refs[1]),
                    ],
                    &interner,
                ),
            ),
            (
                "fo3",
                record(
                    "ACHR",
                    vec![formkey_field("XLKR", source_refs[2])],
                    &interner,
                ),
            ),
            (
                "fnv",
                record(
                    "PGRE",
                    vec![formkey_field("XOWN", source_refs[3])],
                    &interner,
                ),
            ),
            (
                "fo3",
                record(
                    "CELL",
                    vec![formkey_field("XOWN", source_refs[4])],
                    &interner,
                ),
            ),
            (
                "fnv",
                record("NOTE", vec![uint_field("DATA", 0xCCBB_AA03)], &interner),
            ),
        ];
        let mut normalized_records = Vec::new();
        for (index, (source_game, mut source_record)) in source_records.into_iter().enumerate() {
            mapper
                .rewrite_record(&mut source_record)
                .expect("map source references");
            let mut normalized = normalize_from_source_game(source_record, source_game, &interner);
            normalized.form_key.plugin = interner.intern(plugin_name);
            normalized.form_key.local = 0x0008_00 + index as u32;
            normalized_records.push(normalized);
        }

        let handle =
            esp_authoring_core::plugin_runtime::plugin_handle_new_native(plugin_name, Some("fo4"))
                .expect("new plugin handle");
        {
            let mut store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap();
            let slot = store.get_mut(&handle).expect("plugin slot");
            for record in normalized_records {
                crate::target_write::add_record_in_slot(slot, record, &target_schema, &interner)
                    .expect("encode normalized record");
            }
        }

        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "target_normalize_legacy_widths_{}_{}.esp",
            std::process::id(),
            unique
        ));
        esp_authoring_core::plugin_runtime::plugin_handle_save_no_py(
            handle,
            path.to_str().unwrap(),
        )
        .expect("save plugin");
        assert!(esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle));

        let reloaded = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            path.to_str().unwrap(),
            Some("fo4"),
            None,
            None,
            true,
        )
        .expect("reload plugin");
        {
            let store = esp_authoring_core::plugin_runtime::plugin_handle_store_ref()
                .lock()
                .unwrap();
            let slot = store.get(&reloaded).expect("reloaded slot");
            let mut xlkr_refr = vec![0_u8; 4];
            xlkr_refr.extend_from_slice(&target_refs[0].local.to_le_bytes());
            let mut xlkr_achr = vec![0_u8; 4];
            xlkr_achr.extend_from_slice(&target_refs[2].local.to_le_bytes());
            let mut xown_refr = target_refs[1].local.to_le_bytes().to_vec();
            xown_refr.extend_from_slice(&[0; 8]);
            let mut xown_pgre = target_refs[3].local.to_le_bytes().to_vec();
            xown_pgre.extend_from_slice(&[0; 8]);
            let mut xown_cell = target_refs[4].local.to_le_bytes().to_vec();
            xown_cell.extend_from_slice(&[0; 8]);
            let expected = [
                ("REFR", "XLKR", xlkr_refr),
                ("REFR", "XOWN", xown_refr),
                ("ACHR", "XLKR", xlkr_achr),
                ("PGRE", "XOWN", xown_pgre),
                ("CELL", "XOWN", xown_cell),
                ("NOTE", "DATA", vec![3, 0, 0, 0, 0, 0, 0, 0]),
            ];
            for (record_sig, field_sig, expected_bytes) in expected {
                let record = parsed_record_by_sig(&slot.parsed.root_items, record_sig)
                    .expect("reloaded record");
                assert!(record.parse_error.is_none());
                let subrecord = record
                    .subrecords
                    .iter()
                    .find(|subrecord| subrecord.signature.as_str() == field_sig)
                    .expect("reloaded subrecord");
                assert_eq!(subrecord.data.as_ref(), expected_bytes);

                let target_def = target_schema
                    .record_def(record_sig)
                    .and_then(|record_def| record_def.subrecord_def(field_sig))
                    .expect("target subrecord schema");
                assert_eq!(
                    target_def.codec.as_deref().and_then(fixed_size_for_codec),
                    Some(expected_bytes.len())
                );
            }
        }
        assert!(esp_authoring_core::plugin_runtime::plugin_handle_close_native(reloaded));
        std::fs::remove_file(path).expect("remove test plugin");
    }
}
