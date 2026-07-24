use super::*;

/// Highest FO4 condition function id currently represented in the FO4 schema.
///
/// FO76 CTDA/CTDT subrecords can carry FO76-only function ids. The FO4 CK
/// indexes its condition-function table with those ids while loading and can
/// crash before it has a chance to report a warning.
pub(super) const FO4_MAX_KNOWN_CONDITION_FUNCTION_ID: u16 = 817;
pub(super) const FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID: u16 = 276;
pub(super) const FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID: u16 = 300;
// MUST conditions run against FO4's combat target; other record types cannot
// safely approximate FO76's strongest-enemy context.
pub(super) const FO76_GET_STRONGEST_ENEMY_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 692;
pub(super) const FO4_GET_COMBAT_TARGET_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 707;
/// FO76 `GetIsCurrentLocationExact` (844, > FO4's 817 max). It takes an LCTN in
/// Parameter #1; FO4 `GetInCurrentLocation` (359) is the closest compatible gate.
pub(super) const FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID: u16 = 844;
pub(super) const FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID: u16 = 359;
/// FO76 `EditorLocationHasKeyword` (579) and FO4 `LocationHasKeyword` (562)
/// both take a KYWD in Parameter #1 and test the owning reference's location.
pub(super) const FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 579;
pub(super) const FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID: u16 = 562;
/// FO76-only `IsQuestActive` (876, > FO4's 817 max). It takes a QUST in
/// Parameter #1 and is compared `== 1`, so it maps value-identically onto FO4
/// `GetQuestRunning` (56). Remapped before the incompatibility drop so the
/// gating condition survives instead of being stripped (which would leave the
/// owning record — e.g. a loading screen — unconditionally eligible).
pub(super) const FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID: u16 = 876;
pub(super) const FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID: u16 = 56;
/// FO76 `GetIsPlayer` has no FO4 condition-function slot. FO4 expresses the
/// same predicate as `GetIsID` with Actor: Player in Parameter #1.
pub(super) const FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID: u16 = 828;
pub(super) const FO4_GET_IS_ID_CONDITION_FUNCTION_ID: u16 = 72;
pub(super) const FO4_PLAYER_ACTOR_FORM_ID: u32 = 0x0000_0007;
/// Every FO76-only condition-function id that `normalize_fo76_raw_condition_functions`
/// rewrites to an FO4 equivalent. Consumed by the
/// `drop_untranslatable_loadscreen_records` fixup so it does NOT treat a
/// remapped function as untranslatable. Keep in sync with the remaps below.
pub(crate) const FO76_REMAPPED_CONDITION_FUNCTION_IDS: &[u16] = &[
    FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID,
    FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
    FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID,
    FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID,
    FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID,
];
/// FO76-only condition ids below FO4's max id. The FO4 CK still treats these as
/// blank condition functions while loading, so the max-id guard is not enough.
/// 596 is an FO76-only function carried with a `$73808CE`-style Parameter #1 on
/// BS01 Brotherhood dialogue INFOs; xEdit renders it as `<Unknown:param>` and
/// the FO4 CK indexes its blank slot while loading (4 INFO records).
pub(super) const FO76_ONLY_CONDITION_FUNCTION_IDS_UNDER_FO4_MAX: &[u16] =
    &[2, 3, 105, 371, 579, 596, 692, 730, 737];
// CK rejects exterior CELL parameters for this COBJ condition and can crash while loading.
pub(super) const FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID: u16 = 310;
// FO76 Function 67 carries source-side function-info/base-object values that
// FO4 CK tries to resolve from its own Function Info table while loading.
pub(super) const FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID: u16 = 67;
// FO4 condition functions whose Parameter #1 is a QUST FormID
// (wbDefinitionsFO4.pas, Paramtype1: ptQuest). A FO76 QUST referenced here may
// be dropped/unconverted, leaving Parameter #1 = NULL. xEdit then reports the
// CTDA as "Parameter #1 -> Found NULL, expected QUST". The condition can't be
// retargeted (there is no surviving quest), so the whole CTDA is dropped.
pub(super) const FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS: &[u16] =
    &[56, 58, 59, 543, 629, 664];
// CTDA "Run On" value (bytes [20..24]) for "Quest Alias": the condition resolves
// through an alias index against the owning quest.
pub(super) const CTDA_RUN_ON_QUEST_ALIAS: u32 = 5;
// Record types whose CTDA carries an owning quest context (xEdit resolves it
// from the record container or QNAM/PNAM-style owner field).
pub(super) const QUEST_CONTEXT_CONDITION_RECORD_SIGS: &[[u8; 4]] =
    &[*b"QUST", *b"SCEN", *b"PACK", *b"INFO", *b"DIAL"];
// GetIsAliasRef / alias-index parameter. It is valid only when xEdit can resolve
// an owning quest context and that quest has the referenced alias id.
pub(super) const FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS: &[u16] = &[566];
// FO76 nuke-zone check (849, > FO4's 817 max — always dropped as untranslatable).
// Gates leveled-list entries to nuke-irradiated zones (radiation suits, glowing
// variants) that never occur in FO4.
pub(super) const FO76_NUKE_ZONE_CONDITION_FUNCTION_ID: u16 = 849;
pub(super) const CTDA_COMPARISON_GLOBAL_FLAG: u8 = 0x04;

/// True when a CTDA function id has no FO4 equivalent: it exceeds FO4's max
/// known condition-function id (817), or it is a FO76-only id that falls under
/// that max but is still a blank slot in FO4. Keeping such a condition makes the
/// FO4 CK index a non-existent function-table entry and crash while loading.
pub(crate) fn is_fo4_incompatible_condition_function_id(function_id: u16) -> bool {
    function_id > FO4_MAX_KNOWN_CONDITION_FUNCTION_ID
        || FO76_ONLY_CONDITION_FUNCTION_IDS_UNDER_FO4_MAX.contains(&function_id)
}

impl Fo76Fo4Hook {
    pub(super) fn drop_fo4_incompatible_conditions(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        Self::normalize_fo76_raw_condition_functions(record);
        let record_sig = record.sig.0;
        // When a CTDA is dropped, its trailing CIS1/CIS2 parameter strings must
        // be dropped with it: they immediately follow their owning condition and
        // FO4 rejects a CIS1/CIS2 that is not preceded by a CTDA (CK/xEdit report
        // it as an out-of-order subrecord, e.g. orphaned `BTXT CIS2` rows in TERM
        // body/menu condition groups).
        let mut dropping_condition_strings = false;
        record.fields.retain(|entry| match &entry.sig.0 {
            b"CTDA" | b"CTDT" => {
                let drop = Self::condition_function_id(interner, &entry.value).is_some_and(
                    |function_id| {
                        if Self::is_fo4_incompatible_condition_function_id(function_id) {
                            return true;
                        }
                        let parameter_1 =
                            Self::condition_parameter_1(interner, &entry.value).unwrap_or(0);
                        if function_id == FO76_FUNCTION_INFO_CONDITION_FUNCTION_ID
                            && parameter_1 != 0
                        {
                            return true;
                        }
                        if parameter_1 == 0
                            && FO4_QUEST_PARAMETER_1_CONDITION_FUNCTION_IDS.contains(&function_id)
                        {
                            return true;
                        }
                        if FO4_QUEST_ALIAS_PARAMETER_1_CONDITION_FUNCTION_IDS.contains(&function_id)
                            && !QUEST_CONTEXT_CONDITION_RECORD_SIGS.contains(&record_sig)
                        {
                            return true;
                        }
                        let run_on = Self::condition_run_on(interner, &entry.value).unwrap_or(0);
                        if run_on == CTDA_RUN_ON_QUEST_ALIAS
                            && !QUEST_CONTEXT_CONDITION_RECORD_SIGS.contains(&record_sig)
                        {
                            return true;
                        }
                        record_sig == *b"COBJ"
                            && function_id == FO4_COBJ_EXTERIOR_CELL_REJECTED_CONDITION_FUNCTION_ID
                            && parameter_1 != 0
                    },
                );
                dropping_condition_strings = drop;
                !drop
            }
            b"CIS1" | b"CIS2" => !dropping_condition_strings,
            _ => {
                dropping_condition_strings = false;
                true
            }
        });
        if record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
            // CTDA rows may already be stale after generic condition translation.
            record.sync_condition_count();
        }
    }

    pub(super) fn normalize_fo76_raw_condition_functions(record: &mut Record) {
        Self::normalize_fo76_get_is_player_conditions(record);
        Self::normalize_fo76_editor_location_has_keyword_conditions(record);
        let is_music_track = record.sig.0 == *b"MUST";
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            let Some(function_id) = Self::raw_condition_function_id(bytes) else {
                continue;
            };
            if function_id == FO76_IS_IN_INTERIOR_ACOUSTIC_SPACE_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_IS_IN_INTERIOR_CONDITION_FUNCTION_ID,
                );
            } else if is_music_track
                && function_id == FO76_GET_STRONGEST_ENEMY_HAS_KEYWORD_CONDITION_FUNCTION_ID
            {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_COMBAT_TARGET_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                );
            } else if function_id == FO76_GET_IS_CURRENT_LOCATION_EXACT_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_IN_CURRENT_LOCATION_CONDITION_FUNCTION_ID,
                );
            } else if function_id == FO76_IS_QUEST_ACTIVE_CONDITION_FUNCTION_ID {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_GET_QUEST_RUNNING_CONDITION_FUNCTION_ID,
                );
            }
        }
    }

    pub(super) fn normalize_fo76_editor_location_has_keyword_conditions(record: &mut Record) {
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if Self::raw_condition_function_id(bytes)
                == Some(FO76_EDITOR_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID)
            {
                Self::set_raw_condition_function_id(
                    bytes,
                    FO4_LOCATION_HAS_KEYWORD_CONDITION_FUNCTION_ID,
                );
            }
        }
    }

    /// Production `source_read` keeps `struct:` codecs, including CTDA/CTDT,
    /// as raw bytes. Synthetic structured conditions are deliberately left
    /// untouched because Parameter #1's union shape cannot be rewritten
    /// losslessly without knowing the exact authoring variant.
    pub(super) fn normalize_fo76_get_is_player_conditions(record: &mut Record) {
        for entry in &mut record.fields {
            if !matches!(&entry.sig.0, b"CTDA" | b"CTDT") {
                continue;
            }
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if Self::raw_condition_function_id(bytes)
                != Some(FO76_GET_IS_PLAYER_CONDITION_FUNCTION_ID)
            {
                continue;
            }
            Self::set_raw_condition_function_id(bytes, FO4_GET_IS_ID_CONDITION_FUNCTION_ID);
            Self::set_raw_condition_parameter_1(bytes, FO4_PLAYER_ACTOR_FORM_ID);
        }
    }

    pub(super) fn is_fo4_incompatible_condition_function_id(function_id: u16) -> bool {
        is_fo4_incompatible_condition_function_id(function_id)
    }

    pub(super) fn raw_condition_function_id(bytes: &[u8]) -> Option<u16> {
        if bytes.len() < 10 {
            return None;
        }
        Some(u16::from_le_bytes([bytes[8], bytes[9]]))
    }

    pub(super) fn set_raw_condition_function_id(bytes: &mut [u8], function_id: u16) {
        if bytes.len() >= 10 {
            bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
        }
    }

    pub(super) fn raw_condition_parameter_1(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 16 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[12], bytes[13], bytes[14], bytes[15],
        ]))
    }

    pub(super) fn set_raw_condition_parameter_1(bytes: &mut [u8], parameter_1: u32) {
        if bytes.len() >= 16 {
            bytes[12..16].copy_from_slice(&parameter_1.to_le_bytes());
        }
    }

    pub(super) fn condition_function_id(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u16> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_function_id(bytes.as_slice()),
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "Function", interner).and_then(field_value_to_u16)
            }
            _ => None,
        }
    }

    pub(super) fn condition_parameter_1(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u32> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_parameter_1(bytes.as_slice()),
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "Parameter1", interner).and_then(field_value_to_u32)
            }
            _ => None,
        }
    }

    pub(super) fn raw_condition_run_on(bytes: &[u8]) -> Option<u32> {
        if bytes.len() < 24 {
            return None;
        }
        Some(u32::from_le_bytes([
            bytes[20], bytes[21], bytes[22], bytes[23],
        ]))
    }

    pub(super) fn condition_run_on(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u32> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_run_on(bytes.as_slice()),
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "RunOn", interner).and_then(field_value_to_u32)
            }
            _ => None,
        }
    }

    /// CTDA comparison operator — the high 3 bits of the type byte (0=Equal,
    /// 1=NotEqual, 2=Greater, 3=GreaterOrEqual, 4=Less, 5=LessOrEqual).
    pub(super) fn raw_condition_operator(bytes: &[u8]) -> Option<u8> {
        bytes.first().map(|b| (b >> 5) & 0x07)
    }

    pub(super) fn condition_operator(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u8> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_operator(bytes),
            FieldValue::Struct(fields) => named_value_canonical(fields, "Type", interner)
                .and_then(field_value_to_u16)
                .map(|value| ((value as u8) >> 5) & 0x07),
            _ => None,
        }
    }

    pub(super) fn condition_uses_comparison_global(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<bool> {
        match value {
            FieldValue::Bytes(bytes) => bytes
                .first()
                .map(|value| value & CTDA_COMPARISON_GLOBAL_FLAG != 0),
            FieldValue::Struct(fields) => named_value_canonical(fields, "Type", interner)
                .and_then(field_value_to_u16)
                .map(|value| value as u8 & CTDA_COMPARISON_GLOBAL_FLAG != 0),
            _ => None,
        }
    }

    pub(super) fn raw_condition_comparison_value(bytes: &[u8]) -> Option<f32> {
        if bytes.len() < 8 || bytes[0] & CTDA_COMPARISON_GLOBAL_FLAG != 0 {
            return None;
        }
        Some(f32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]))
    }

    pub(super) fn condition_comparison_value(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<f32> {
        match value {
            FieldValue::Bytes(bytes) => Self::raw_condition_comparison_value(bytes),
            FieldValue::Struct(fields) => {
                if Self::condition_uses_comparison_global(interner, value)? {
                    return None;
                }
                named_value_canonical(fields, "ComparisonValue", interner)
                    .and_then(Self::nested_f32)
            }
            _ => None,
        }
    }

    pub(super) fn condition_comparison_global_id(
        interner: &crate::sym::StringInterner,
        value: &FieldValue,
    ) -> Option<u32> {
        if !Self::condition_uses_comparison_global(interner, value)? {
            return None;
        }
        match value {
            FieldValue::Bytes(bytes) if bytes.len() >= 8 => {
                Some(u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]) & 0x00FF_FFFF)
            }
            FieldValue::Struct(fields) => {
                named_value_canonical(fields, "ComparisonValue", interner)
                    .and_then(Self::nested_form_id)
            }
            _ => None,
        }
    }

    pub(super) fn nested_f32(value: &FieldValue) -> Option<f32> {
        match value {
            FieldValue::Float(value) => Some(*value),
            FieldValue::Uint(value) => Some(*value as f32),
            FieldValue::Int(value) => Some(*value as f32),
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                Some(f32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]))
            }
            FieldValue::List(values) => values.iter().find_map(Self::nested_f32),
            FieldValue::Struct(fields) => {
                fields.iter().find_map(|(_, value)| Self::nested_f32(value))
            }
            _ => None,
        }
    }

    pub(super) fn nested_form_id(value: &FieldValue) -> Option<u32> {
        match value {
            FieldValue::FormKey(form_key) => Some(form_key.local & 0x00FF_FFFF),
            FieldValue::Uint(value) => u32::try_from(*value).ok().map(|value| value & 0x00FF_FFFF),
            FieldValue::Int(value) => u32::try_from(*value).ok().map(|value| value & 0x00FF_FFFF),
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                Some(u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) & 0x00FF_FFFF)
            }
            FieldValue::List(values) => values.iter().find_map(Self::nested_form_id),
            FieldValue::Struct(fields) => fields
                .iter()
                .find_map(|(_, value)| Self::nested_form_id(value)),
            _ => None,
        }
    }

    pub(super) fn numeric_condition_matches(value: f32, operator: u8, comparison: f32) -> bool {
        match operator {
            0 => value == comparison,
            1 => value != comparison,
            2 => value > comparison,
            3 => value >= comparison,
            4 => value < comparison,
            5 => value <= comparison,
            _ => false,
        }
    }
}
