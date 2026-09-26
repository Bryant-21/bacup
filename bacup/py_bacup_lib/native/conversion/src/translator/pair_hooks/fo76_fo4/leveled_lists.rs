use super::*;

// `GetGlobalValue` (74, shared FO4/FO76). FO76 gates seasonal/event leveled
// entries behind globals such as `Festive_Holiday_Enabled` that stay 0 in FO4.
pub(super) const GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID: u16 = 74;
pub(super) const GET_ITEM_COUNT_CONDITION_FUNCTION_ID: u16 = 47;
pub(super) const GET_LOCKED_CONDITION_FUNCTION_ID: u16 = 5;
pub(super) const GET_LOCK_LEVEL_CONDITION_FUNCTION_ID: u16 = 65;
pub(super) const GET_LEVEL_CONDITION_FUNCTION_ID: u16 = 80;
pub(super) const HAS_PERK_CONDITION_FUNCTION_ID: u16 = 448;
// `GetRandomPercent` (77, shared FO4/FO76). FO76 `LPI_` placement lists gate the
// normal-world leaf behind a chance roll against a percentage GLOB.
pub(super) const GET_RANDOM_PERCENT_CONDITION_FUNCTION_ID: u16 = 77;
pub(super) const LEVELED_LIST_USE_ALL_FLAG: u8 = 0x04;
// GetLockLevel comparisons use GLOB references instead of literal floats in the
// live FO76 lists, so the conservative baseline cannot be derived from bytes
// without mapping these four stable source globals.
pub(super) const FO76_LOCK_LEVEL_NOVICE_GLOBAL: u32 = 0x0AEF4F;
pub(super) const FO76_LOCK_LEVEL_ADVANCED_GLOBAL: u32 = 0x0AEF50;
pub(super) const FO76_LOCK_LEVEL_EXPERT_GLOBAL: u32 = 0x0AEF51;
pub(super) const FO76_LOCK_LEVEL_MASTER_GLOBAL: u32 = 0x0AEF5D;
pub(super) const FO76_GLOWING_CREATURE_SPAWN_CONDITION_FORM: u32 = 0x8464EF;
pub(super) const FO76_GLOWING_CREATURE_VARIANT_KEYWORD: u32 = 0x2D3EBE;
pub(super) const FO76_SCORCHED_CREATURE_VARIANT_KEYWORD: u32 = 0x2D3EBF;
const FO76_SPECIAL_CREATURE_VARIANT_KEYWORDS: &[u32] = &[
    FO76_GLOWING_CREATURE_VARIANT_KEYWORD,
    FO76_SCORCHED_CREATURE_VARIANT_KEYWORD,
];
pub(super) const FO76_LEVELED_ENTRY_TAIL_SIGS: &[[u8; 4]] = &[
    *b"COED", *b"CTDA", *b"CTDT", *b"CIS1", *b"CIS2", *b"LVUD", *b"LVOV", *b"LVOC", *b"LVOT",
    *b"LVIV", *b"LVIG", *b"LVIT", *b"LVLV", *b"LVOG", *b"LVLT",
];
pub(super) const FO76_CAPS_FORM_ID: u32 = 0x00000F;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum LeveledEntryDisposition {
    Keep,
    Reject,
    Unknown,
}

#[derive(Clone, Copy, Debug)]
pub(super) struct LeveledEntrySpan {
    start: usize,
    end: usize,
}

pub(super) fn source_lvlo_reference(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    interner: &crate::sym::StringInterner,
) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(fk) => Some(*fk),
        FieldValue::Uint(value) => u32::try_from(*value)
            .ok()
            .map(|raw| source_form_key_from_raw(raw, source_plugin)),
        FieldValue::Int(value) => u32::try_from(*value)
            .ok()
            .map(|raw| source_form_key_from_raw(raw, source_plugin)),
        FieldValue::Bytes(bytes) if bytes.len() >= 12 => {
            let raw = u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]);
            Some(source_form_key_from_raw(raw, source_plugin))
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Some(source_form_key_from_raw(raw, source_plugin))
        }
        FieldValue::Struct(fields) => {
            for name in [
                "value",
                "reference",
                "reference_reference",
                "base_data_item",
                "base_data_npc",
                "base_data_spell",
                "pack_in",
                "item",
                "npc",
                "spell",
            ] {
                if let Some(candidate) = named_value(fields, name, interner)
                    .and_then(|candidate| source_lvlo_reference(candidate, source_plugin, interner))
                {
                    return Some(candidate);
                }
            }
            fields.iter().find_map(|(_, candidate)| {
                source_lvlo_reference(candidate, source_plugin, interner)
            })
        }
        FieldValue::List(items) => items
            .iter()
            .find_map(|candidate| source_lvlo_reference(candidate, source_plugin, interner)),
        _ => None,
    }
}

pub(super) fn remap_known_lvlo_reference(
    interner: &crate::sym::StringInterner,
    reference: FormKey,
) -> FormKey {
    if reference.local == FO76_CAPS_FORM_ID
        && interner
            .resolve(reference.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return FormKey {
            local: FO76_CAPS_FORM_ID,
            plugin: interner.intern(FO4_MASTER_NAME),
        };
    }
    reference
}

pub(super) fn raw_lvlo_u16(value: &FieldValue, offset: usize) -> Option<u16> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() < 12 {
        return None;
    }
    let end = offset.checked_add(2)?;
    let slice = bytes.get(offset..end)?;
    Some(u16::from_le_bytes([slice[0], slice[1]]))
}

pub(super) fn fo4_lvlo_value(
    interner: &crate::sym::StringInterner,
    record_sig: &[u8; 4],
    level: u16,
    reference: FormKey,
    count: u16,
    chance_none: u8,
) -> FieldValue {
    let reference_field = match record_sig {
        b"LVLN" => "npc",
        b"LVSP" => "spell",
        _ => "item",
    };
    FieldValue::Struct(vec![
        (
            interner.intern("level"),
            bytes_value(&level.max(1).to_le_bytes()),
        ),
        (interner.intern("unknown_u8_1"), bytes_value(&[0])),
        (interner.intern("unknown_u8_2"), bytes_value(&[0])),
        (
            interner.intern(reference_field),
            FieldValue::FormKey(reference),
        ),
        (interner.intern("count"), bytes_value(&count.to_le_bytes())),
        (interner.intern("chance_none"), bytes_value(&[chance_none])),
        (interner.intern("unknown_u8_6"), bytes_value(&[0])),
    ])
}

fn source_entry_chance_none(
    entry: &FieldValue,
    tail: &[FieldEntry],
    interner: &crate::sym::StringInterner,
) -> u8 {
    fn percent(value: &FieldValue) -> Option<u8> {
        let value = match value {
            FieldValue::Float(value) => *value,
            FieldValue::Uint(value) => *value as f32,
            FieldValue::Int(value) => *value as f32,
            FieldValue::Bytes(bytes) if bytes.len() == 4 => {
                f32::from_le_bytes(bytes.as_slice().try_into().ok()?)
            }
            FieldValue::Bytes(bytes) if bytes.len() == 1 => f32::from(bytes[0]),
            _ => return None,
        };
        value
            .is_finite()
            .then(|| value.round().clamp(0.0, 100.0) as u8)
    }

    tail.iter()
        .find(|field| field.sig.as_str() == "LVOV")
        .and_then(|field| percent(&field.value))
        .or_else(|| match entry {
            FieldValue::Bytes(bytes) if bytes.len() >= 12 => Some(bytes[10].min(100)),
            FieldValue::Struct(fields) => {
                named_value(fields, "chance_none", interner).and_then(percent)
            }
            _ => None,
        })
        .unwrap_or(0)
}

pub(super) fn sync_llct_count(record: &mut Record, count: usize) {
    let count = count.min(u8::MAX as usize) as u64;
    let llct_sig = SubrecordSig(*b"LLCT");
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig == llct_sig) {
        entry.value = FieldValue::Uint(count);
    } else {
        record.fields.insert(
            0,
            FieldEntry {
                sig: llct_sig,
                value: FieldValue::Uint(count),
            },
        );
    }
}
impl Fo76Fo4Hook {
    pub(super) fn convert_fo76_leveled_list_entries(
        ctx: &crate::translator::pair_hook::PairCtx<'_>,
        record: &mut Record,
    ) {
        let interner = ctx.interner;
        let global_values = ctx.source_global_values;
        if !matches!(&record.sig.0, b"LVLI" | b"LVLN" | b"LVSP") {
            return;
        }

        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let entry_spans = Self::leveled_entry_spans(&old_fields);
        let mut keep_entries = vec![true; entry_spans.len()];
        let mut used_unknown_fallback = false;

        if record.sig.0 == *b"LVLI" {
            let lock_level_baseline = Self::leveled_condition_baseline(
                interner,
                &old_fields,
                &entry_spans,
                GET_LOCK_LEVEL_CONDITION_FUNCTION_ID,
            );
            let level_baseline = Self::leveled_condition_baseline(
                interner,
                &old_fields,
                &entry_spans,
                GET_LEVEL_CONDITION_FUNCTION_ID,
            );
            let dispositions: Vec<_> = entry_spans
                .iter()
                .map(|span| {
                    Self::leveled_entry_disposition(
                        interner,
                        &old_fields[span.start + 1..span.end],
                        lock_level_baseline,
                        level_baseline,
                        global_values,
                    )
                })
                .collect();
            keep_entries = dispositions
                .iter()
                .map(|disposition| *disposition == LeveledEntryDisposition::Keep)
                .collect();

            if !keep_entries.iter().any(|keep| *keep) {
                let unknown_entries: Vec<usize> = dispositions
                    .iter()
                    .enumerate()
                    .filter_map(|(index, disposition)| {
                        (*disposition == LeveledEntryDisposition::Unknown).then_some(index)
                    })
                    .collect();
                if let Some(fallback) = Self::conservative_leveled_fallback(
                    interner,
                    &old_fields,
                    &entry_spans,
                    &unknown_entries,
                ) {
                    keep_entries[fallback] = true;
                    used_unknown_fallback = true;
                }
            }
        } else {
            for (index, span) in entry_spans.iter().enumerate() {
                keep_entries[index] = !old_fields[span.start + 1..span.end]
                    .iter()
                    .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
                    .any(|entry| {
                        Self::condition_gates_dropped_world_state(
                            interner,
                            &entry.value,
                            global_values,
                        )
                    });
            }
            let first_match = old_fields.iter().any(|field| {
                field.sig.as_str() == "LVLF"
                    && match &field.value {
                        FieldValue::Bytes(bytes) => bytes.first().copied().map(u16::from),
                        value => field_value_to_u16(value),
                    }
                    .is_some_and(|flags| flags & 0x40 != 0)
            });
            if first_match {
                // Ordered conditional alternatives are not independent weights.
                // Until runtime conditions are supported, retain the rarest
                // surviving alternative for the same object/level/count.
                let mut alternatives = HashMap::<(FormKey, u16, u16), (usize, u8)>::new();
                for (index, span) in entry_spans.iter().enumerate() {
                    if !keep_entries[index] {
                        continue;
                    }
                    let value = &old_fields[span.start].value;
                    let Some(reference) =
                        source_lvlo_reference(value, record.form_key.plugin, interner)
                    else {
                        continue;
                    };
                    let level = raw_lvlo_u16(value, 0).unwrap_or_else(|| {
                        following_u16_value(&old_fields, span.start + 1, b"LVLV", 1)
                    });
                    let count = raw_lvlo_u16(value, 8).unwrap_or_else(|| {
                        following_u16_value(&old_fields, span.start + 1, b"LVIV", 1)
                    });
                    let chance = source_entry_chance_none(
                        value,
                        &old_fields[span.start + 1..span.end],
                        interner,
                    );
                    let key = (reference, level, count);
                    if let Some(&(previous, previous_chance)) = alternatives.get(&key) {
                        if previous_chance >= chance {
                            keep_entries[index] = false;
                            continue;
                        }
                        keep_entries[previous] = false;
                    }
                    alternatives.insert(key, (index, chance));
                }
            }
        }

        let mut retained = smallvec::SmallVec::new();
        let mut converted_count = 0usize;
        let mut field_index = 0usize;
        let mut span_index = 0usize;
        while field_index < old_fields.len() {
            if span_index < entry_spans.len() && field_index == entry_spans[span_index].start {
                let span = entry_spans[span_index];
                if keep_entries[span_index] {
                    let mut entry = old_fields[field_index].clone();
                    if let Some(reference) =
                        source_lvlo_reference(&entry.value, record.form_key.plugin, interner)
                    {
                        let reference = remap_known_lvlo_reference(interner, reference);
                        let level = raw_lvlo_u16(&entry.value, 0).unwrap_or_else(|| {
                            following_u16_value(&old_fields, field_index + 1, b"LVLV", 1)
                        });
                        let count = raw_lvlo_u16(&entry.value, 8).unwrap_or_else(|| {
                            following_u16_value(&old_fields, field_index + 1, b"LVIV", 1)
                        });
                        let chance_none = source_entry_chance_none(
                            &entry.value,
                            &old_fields[field_index + 1..span.end],
                            interner,
                        );
                        entry.value = fo4_lvlo_value(
                            interner,
                            &record.sig.0,
                            level,
                            reference,
                            count,
                            chance_none,
                        );
                        retained.push(entry);
                        retained.extend(old_fields[field_index + 1..span.end].iter().cloned());
                        converted_count += 1;
                    }
                }
                field_index = span.end;
                span_index += 1;
                continue;
            }

            retained.push(old_fields[field_index].clone());
            field_index += 1;
        }

        record.fields = retained;
        if used_unknown_fallback {
            Self::clear_leveled_list_use_all(record);
        }
        if !entry_spans.is_empty() {
            sync_llct_count(record, converted_count);
        }
    }

    pub(super) fn leveled_entry_spans(fields: &[FieldEntry]) -> Vec<LeveledEntrySpan> {
        let mut spans = Vec::new();
        let mut index = 0usize;
        while index < fields.len() {
            if fields[index].sig.0 != *b"LVLO" {
                index += 1;
                continue;
            }
            let start = index;
            index += 1;
            while index < fields.len()
                && FO76_LEVELED_ENTRY_TAIL_SIGS.contains(&fields[index].sig.0)
            {
                index += 1;
            }
            spans.push(LeveledEntrySpan { start, end: index });
        }
        spans
    }

    pub(super) fn leveled_condition_baseline(
        interner: &crate::sym::StringInterner,
        fields: &[FieldEntry],
        spans: &[LeveledEntrySpan],
        function_id: u16,
    ) -> Option<f32> {
        spans
            .iter()
            .flat_map(|span| &fields[span.start + 1..span.end])
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .filter(|entry| {
                Self::condition_function_id(interner, &entry.value) == Some(function_id)
            })
            .filter_map(|entry| {
                Self::leveled_condition_threshold(interner, &entry.value, function_id)
            })
            .filter(|value| value.is_finite())
            .min_by(f32::total_cmp)
    }

    pub(super) fn leveled_entry_disposition(
        interner: &crate::sym::StringInterner,
        fields: &[FieldEntry],
        lock_level_baseline: Option<f32>,
        level_baseline: Option<f32>,
        global_values: Option<&std::collections::HashMap<u32, f32>>,
    ) -> LeveledEntryDisposition {
        let conditions: Vec<_> = fields
            .iter()
            .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CTDT"))
            .collect();
        if conditions.is_empty() {
            return LeveledEntryDisposition::Keep;
        }

        let mut disposition = LeveledEntryDisposition::Keep;
        for condition in conditions {
            match Self::leveled_condition_disposition(
                interner,
                &condition.value,
                lock_level_baseline,
                level_baseline,
                global_values,
            ) {
                LeveledEntryDisposition::Reject => return LeveledEntryDisposition::Reject,
                LeveledEntryDisposition::Unknown => disposition = LeveledEntryDisposition::Unknown,
                LeveledEntryDisposition::Keep => {}
            }
        }
        disposition
    }

    pub(super) fn leveled_condition_disposition(
        interner: &crate::sym::StringInterner,
        condition: &FieldValue,
        lock_level_baseline: Option<f32>,
        level_baseline: Option<f32>,
        global_values: Option<&std::collections::HashMap<u32, f32>>,
    ) -> LeveledEntryDisposition {
        let Some(function_id) = Self::condition_function_id(interner, condition) else {
            return LeveledEntryDisposition::Unknown;
        };

        if function_id == FO76_NUKE_ZONE_CONDITION_FUNCTION_ID
            || function_id == HAS_PERK_CONDITION_FUNCTION_ID
            || FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS.contains(&function_id)
        {
            return LeveledEntryDisposition::Reject;
        }
        if function_id == FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID {
            return LeveledEntryDisposition::Keep;
        }
        // A chance roll is world-state independent and survives into FO4 as-is.
        if function_id == GET_RANDOM_PERCENT_CONDITION_FUNCTION_ID {
            return LeveledEntryDisposition::Keep;
        }
        // A FO76 condition *form* hides world state this pass cannot read, and it
        // is how nuke/radstorm variants are gated (e.g.
        // `Radstorm_NukaFlora_Spawn_Condition`). Its CTDA is dropped downstream as
        // untranslatable, so keeping the entry would make the special-state
        // variant unconditional.
        if function_id == FO76_CONDITION_FORM_CONDITION_FUNCTION_ID {
            return LeveledEntryDisposition::Unknown;
        }
        if Self::is_fo4_incompatible_condition_function_id(function_id) {
            return LeveledEntryDisposition::Keep;
        }

        let baseline = match function_id {
            GET_ITEM_COUNT_CONDITION_FUNCTION_ID | GET_LOCKED_CONDITION_FUNCTION_ID => Some(0.0),
            // Evaluate against what the global actually ships as. Assuming 0
            // dropped every row gated on an *enabled* global -- it emptied
            // `RA_LL_Rewards_PublicEvents_TreasuryNotes`, whose gate
            // `Gold_Treasury_Note_Loot_Enabled` is 1.
            GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID => Some(Self::condition_global_value(
                interner,
                condition,
                global_values,
            )),
            GET_LOCK_LEVEL_CONDITION_FUNCTION_ID => lock_level_baseline,
            GET_LEVEL_CONDITION_FUNCTION_ID => level_baseline,
            _ => None,
        };
        let (Some(baseline), Some(operator), Some(threshold)) = (
            baseline,
            Self::condition_operator(interner, condition),
            Self::leveled_condition_threshold(interner, condition, function_id),
        ) else {
            return LeveledEntryDisposition::Unknown;
        };

        if Self::numeric_condition_matches(baseline, operator, threshold) {
            LeveledEntryDisposition::Keep
        } else {
            LeveledEntryDisposition::Reject
        }
    }

    pub(super) fn conservative_leveled_fallback(
        interner: &crate::sym::StringInterner,
        fields: &[FieldEntry],
        spans: &[LeveledEntrySpan],
        candidates: &[usize],
    ) -> Option<usize> {
        candidates.iter().copied().min_by(|left, right| {
            let left_key = Self::leveled_fallback_key(interner, fields, spans[*left]);
            let right_key = Self::leveled_fallback_key(interner, fields, spans[*right]);
            left_key
                .0
                .cmp(&right_key.0)
                .then(left_key.1.cmp(&right_key.1))
                .then_with(|| left_key.2.total_cmp(&right_key.2))
                .then(left.cmp(right))
        })
    }

    pub(super) fn leveled_fallback_key(
        interner: &crate::sym::StringInterner,
        fields: &[FieldEntry],
        span: LeveledEntrySpan,
    ) -> (u16, u16, f32) {
        let entry = &fields[span.start];
        let level = raw_lvlo_u16(&entry.value, 0)
            .unwrap_or_else(|| following_u16_value(fields, span.start + 1, b"LVLV", 1));
        let count = raw_lvlo_u16(&entry.value, 8)
            .unwrap_or_else(|| following_u16_value(fields, span.start + 1, b"LVIV", 1));
        let lowest_threshold = fields[span.start + 1..span.end]
            .iter()
            .filter(|field| matches!(&field.sig.0, b"CTDA" | b"CTDT"))
            .filter_map(|field| {
                let function_id = Self::condition_function_id(interner, &field.value)?;
                Self::leveled_condition_threshold(interner, &field.value, function_id)
            })
            .filter(|value| value.is_finite())
            .min_by(f32::total_cmp)
            .unwrap_or(f32::INFINITY);
        (level, count, lowest_threshold)
    }

    pub(super) fn clear_leveled_list_use_all(record: &mut Record) {
        for entry in &mut record.fields {
            if entry.sig.0 == *b"LVLF" {
                clear_low_byte_flag(&mut entry.value, LEVELED_LIST_USE_ALL_FLAG);
            }
        }
    }

    pub(super) fn leveled_condition_threshold(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
        function_id: u16,
    ) -> Option<f32> {
        if function_id == GET_LOCK_LEVEL_CONDITION_FUNCTION_ID
            && Self::condition_uses_comparison_global(interner, value) == Some(true)
        {
            return match Self::condition_comparison_global_id(interner, value)? {
                FO76_LOCK_LEVEL_NOVICE_GLOBAL => Some(25.0),
                FO76_LOCK_LEVEL_ADVANCED_GLOBAL => Some(50.0),
                FO76_LOCK_LEVEL_EXPERT_GLOBAL => Some(75.0),
                FO76_LOCK_LEVEL_MASTER_GLOBAL => Some(100.0),
                _ => None,
            };
        }
        Self::condition_comparison_value(interner, value)
    }

    /// True when a `CTDA` gates its entry behind FO76-only world state that the
    /// target leveled-list shape cannot represent.
    /// The shipped value of the global a `GetGlobalValue` condition tests.
    /// Falls back to 0 -- FO4's default for an unknown global -- so a value the
    /// caller could not read behaves exactly as it did before.
    pub(super) fn condition_global_value(
        interner: &crate::sym::StringInterner,
        condition: &FieldValue,
        global_values: Option<&std::collections::HashMap<u32, f32>>,
    ) -> f32 {
        Self::condition_parameter_1(interner, condition)
            .and_then(|global| {
                global_values.and_then(|values| values.get(&(global & 0x00FF_FFFF)).copied())
            })
            .unwrap_or(0.0)
    }

    pub(super) fn condition_gates_dropped_world_state(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
        global_values: Option<&std::collections::HashMap<u32, f32>>,
    ) -> bool {
        let function_id = Self::condition_function_id(interner, value);
        if function_id == Some(FO76_NUKE_ZONE_CONDITION_FUNCTION_ID) {
            return true;
        }
        if function_id == Some(FO76_CONDITION_FORM_CONDITION_FUNCTION_ID) {
            return Self::condition_parameter_1(interner, value).is_some_and(|parameter| {
                parameter & 0x00FF_FFFF == FO76_GLOWING_CREATURE_SPAWN_CONDITION_FORM
            });
        }
        if matches!(
            function_id,
            Some(FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
                | Some(FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
        ) {
            return Self::condition_parameter_1(interner, value).is_some_and(|parameter| {
                FO76_SPECIAL_CREATURE_VARIANT_KEYWORDS.contains(&(parameter & 0x00FF_FFFF))
            });
        }
        let FieldValue::Bytes(bytes) = value else {
            return false;
        };
        function_id == Some(GET_GLOBAL_VALUE_CONDITION_FUNCTION_ID)
            && Self::condition_requires_global_on(
                bytes,
                Self::condition_global_value(interner, value, global_values),
            )
    }

    /// True when a `GetGlobalValue` condition cannot be satisfied at
    /// `global_value` -- the value the global ships with, defaulting to 0 (the
    /// FO4 off-state) when it could not be read. The satisfied branch is kept
    /// so the live entry survives.
    pub(super) fn condition_requires_global_on(bytes: &[u8], global_value: f32) -> bool {
        let (Some(op), Some(cmp)) = (
            Self::raw_condition_operator(bytes),
            Self::raw_condition_comparison_value(bytes),
        ) else {
            return false;
        };
        match op {
            0 => cmp != global_value, // == a value the global does not hold
            1 => cmp == global_value, // != the value it does hold
            2 => cmp >= global_value, // > a value the global cannot exceed
            3 => cmp > global_value,  // >= a value above the global
            _ => false,               // less-than variants satisfy -> keep
        }
    }
}
