use super::*;

pub(super) const QUST_ALIAS_SIGS: &[[u8; 4]] = &[
    *b"ALST", *b"ALID", *b"FNAM", *b"ALFI", *b"ALFR", *b"ALUA", *b"ALFA", *b"KNAM", *b"ALRT",
    *b"ALEQ", *b"ALEA", *b"ALCO", *b"ALCA", *b"ALCL", *b"ALNA", *b"ALNT", *b"ALFE", *b"ALFD",
    *b"ALCC", *b"CTDA", *b"CIS1", *b"CIS2", *b"KSIZ", *b"KWDA", *b"COCT", *b"CNTO", *b"COED",
    *b"SPOR", *b"OCOR", *b"GWOR", *b"ECOR", *b"ALLA", *b"ALDN", *b"ALFV", *b"ALDI", *b"ALSP",
    *b"ALFC", *b"ALPC", *b"VTCK", *b"ALED", *b"ALLS", *b"ALFL", *b"ALCS", *b"ALMI",
];
pub(super) const QUST_ALIAS_ANCHOR_SIGS: &[[u8; 4]] = &[*b"ALST", *b"ALLS", *b"ALCS"];
pub(super) const QUST_ALIAS_OPTIONAL_FLAG: u32 = 0x2;
pub(super) const FO76_QUST_FRAGMENT_VERSION: u8 = 4;
pub(super) const QUST_OBJECTIVE_TARGET_CONDITION_SIGS: &[[u8; 4]] = &[*b"CTDA", *b"CIS1", *b"CIS2"];
const FO76_ONLY_QUST_EVENT_TYPES: [u32; 6] = [
    u32::from_le_bytes(*b"ADBO"),
    u32::from_le_bytes(*b"CBGN"),
    u32::from_le_bytes(*b"ILOC"),
    u32::from_le_bytes(*b"LCPG"),
    u32::from_le_bytes(*b"PCON"),
    u32::from_le_bytes(*b"QPMT"),
];
// QUST subrecords stripped from FO76 input before FO4 translation. Most are
// FO76-only chunks FO4 does not accept. ALFE/ALFD are FO4-known, but FO76
// event alias fills can fault FO4's ALFD resolver, so the alias row is kept
// without its event-fill data.
pub(super) const QUST_DROP_SIGS: &[[u8; 4]] = &[
    *b"ACBS", *b"ALFC", *b"ALFE", *b"ALFD", *b"ALSO", *b"ATTR", *b"COED", *b"DTGT", *b"ESAV",
    *b"ESCE", *b"ESCS", *b"ESDA", *b"ESRP", *b"ESRV", *b"KNAM", *b"NAM8", *b"QUCF", *b"SCCM",
    *b"SCFC", *b"SDCT", *b"SPPI", *b"SPPT", *b"TRAE", *b"VNAM",
];

// FO76 QUST.DATA layout variants (the `flags` field is a union on
// `record_form_version`; detected here by payload length). FO4 QUST.DNAM is a
// fixed 12-byte `struct:H,B,B,f,B,B,B,B`.
pub(super) const FO76_QUST_DATA_FLAGS64_LEN: usize = 20; // flags u64 (form_version >= 202)
pub(super) const FO76_QUST_DATA_FLAGS32_LEN: usize = 16; // flags u32 (form_version < 202)
pub(super) const FO4_QUST_DNAM_LEN: usize = 12;
pub(super) const FO76_QUST_TYPE_PUBLIC_EVENT: u8 = 6;
pub(super) const FO76_QUST_TYPE_EVENT: u8 = 8;
pub(super) const FO4_QUST_TYPE_NONE: u8 = 0;
pub(super) const FO4_QUST_TYPE_MAIN_QUEST: u8 = 1;
pub(super) const FO4_QUST_TYPE_MISCELLANEOUS: u8 = 6;
pub(super) const FO4_QUST_TYPE_SIDE_QUESTS: u8 = 7;

/// Relayout an FO76 QUST `DATA` payload into an FO4 QUST `DNAM` payload.
///
/// The low 16 flag bits are bit-identical between the two games
/// (`start_game_enabled`=1, `starts_enabled`=16, `run_once`=256,
/// `has_dialogue_data`=0x8000, …); FO76-only flag bits (>= 0x10000) are dropped
/// by the u16 truncation. `priority` and `delay_time` carry over at their FO4
/// offsets; the incompatible quest-type enum is mapped explicitly. Returns
/// `None` for an unrecognized length so the caller leaves the field untouched.
#[derive(Clone, Copy)]
pub(super) struct Fo76QuestData {
    flags: u64,
    priority: u8,
    delay_time: [u8; 4],
    quest_type: u8,
}

pub(super) fn parse_fo76_qust_data(data: &[u8]) -> Option<Fo76QuestData> {
    let (flags, priority, delay_time, quest_type) = match data.len() {
        FO76_QUST_DATA_FLAGS64_LEN => (
            u64::from_le_bytes(data[0..8].try_into().ok()?),
            data[8],
            [data[12], data[13], data[14], data[15]],
            data[16],
        ),
        FO76_QUST_DATA_FLAGS32_LEN => (
            u64::from(u32::from_le_bytes(data[0..4].try_into().ok()?)),
            data[4],
            [data[8], data[9], data[10], data[11]],
            data[12],
        ),
        _ => return None,
    };
    Some(Fo76QuestData {
        flags,
        priority,
        delay_time,
        quest_type,
    })
}

pub(super) fn build_fo4_qust_dnam(source: Fo76QuestData) -> smallvec::SmallVec<[u8; 32]> {
    let mut dnam: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    dnam.resize(FO4_QUST_DNAM_LEN, 0);
    dnam[0..2].copy_from_slice(&(source.flags as u16).to_le_bytes());
    dnam[2] = source.priority;
    dnam[4..8].copy_from_slice(&source.delay_time);
    dnam[8] = fo76_qust_type_to_fo4(source.quest_type);
    dnam
}

#[cfg(test)]
pub(super) fn build_fo4_qust_dnam_from_fo76_data(
    data: &[u8],
) -> Option<smallvec::SmallVec<[u8; 32]>> {
    parse_fo76_qust_data(data).map(build_fo4_qust_dnam)
}

pub(super) fn fo76_qust_type_to_fo4(quest_type: u8) -> u8 {
    match quest_type {
        0 => FO4_QUST_TYPE_NONE,
        1 => FO4_QUST_TYPE_MAIN_QUEST,
        2 | 3 => FO4_QUST_TYPE_SIDE_QUESTS,
        7 => FO4_QUST_TYPE_MISCELLANEOUS,
        _ => FO4_QUST_TYPE_NONE,
    }
}

// FO4 QUST.DNAM flag bits (low u16 of the flags field).
pub(super) const QUST_DNAM_FLAG_START_GAME_ENABLED: u16 = 0x0001;

/// True when the QUST's editorID marks it as an NPC conversation quest. FO76
/// names these `Dialogue_*` / `*_Dialogue_*` / `W05_Dialogue*` /
/// `XPD_Dialogue_*` / `NPCConversation_*`.
/// Gameplay quests are named by quest-type prefix (`RE_`, `*_MQ_`, `MTR*`,
/// `FF*`, `EN*`, `Test*`). Naming is the only signal that cleanly separates the
/// two; every structural signal also matches random encounters / events.
pub(super) fn qust_eid_lower(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> Option<String> {
    if let Some(eid) = record.eid.and_then(|sym| interner.resolve(sym)) {
        return Some(eid.to_ascii_lowercase());
    }
    for entry in &record.fields {
        if entry.sig.0 != *b"EDID" {
            continue;
        }
        return match &entry.value {
            FieldValue::String(sym) => interner.resolve(*sym).map(|s| s.to_ascii_lowercase()),
            FieldValue::Bytes(bytes) => {
                let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
                std::str::from_utf8(&bytes[..end])
                    .ok()
                    .map(|s| s.to_ascii_lowercase())
            }
            _ => None,
        };
    }
    None
}

pub(crate) fn qust_eid_is_dialogue_conversation(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    qust_eid_lower(interner, record)
        .is_some_and(|s| s.contains("dialogue") || s.contains("npcconversation"))
}

/// FO76 ships hundreds of developer test/debug/scratch quests (EditorID
/// `Test*`, `Debug*`, `zz*`/`ZZZ*`) whose scenes bind aliases to test-only world content. They are
/// never started for players in FO76 — its Story Manager / test harness drives
/// them, and that machinery is not replicated here. Auto-starting one (whether
/// via force-start or a faithfully-relayed start-game-enabled flag) makes FO4
/// try to fill those aliases, resolve a bad actor handle, and CTD on load
/// (`test_VHarbison_Dialogue_Someone`). Treat them as never-auto-run.
pub(super) fn qust_eid_is_test_or_dev(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    qust_eid_lower(interner, record)
        .is_some_and(|s| s.starts_with("test") || s.starts_with("debug") || s.starts_with("zz"))
}

pub(super) fn suppress_quest_autostart(dnam: &mut [u8]) -> Option<u16> {
    if dnam.len() < 2 {
        return None;
    }
    let mut flags = u16::from_le_bytes([dnam[0], dnam[1]]);
    if flags & QUST_DNAM_FLAG_START_GAME_ENABLED == 0 {
        return None;
    }
    let source_flags = flags;
    flags &= !QUST_DNAM_FLAG_START_GAME_ENABLED;
    dnam[0..2].copy_from_slice(&flags.to_le_bytes());
    Some(source_flags)
}

pub(super) fn mark_qust_alias_fnam_optional(value: &mut FieldValue) {
    set_u32_bits(value, QUST_ALIAS_OPTIONAL_FLAG);
}
impl Fo76Fo4Hook {
    pub(super) fn strip_qust_runtime_scopes(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"QUST" {
            return;
        }

        let player_event_consumer_alias_ids = qust_vmad_player_event_consumer_alias_ids(record);
        let remove_players_alias_ids = qust_vmad_remove_players_alias_ids(record);
        let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut after_next_alias_id = false;
        let mut after_objective_target = false;
        let mut in_objective = false;
        let mut current_alias_id = None;
        let mut current_alias_fnam_index: Option<usize> = None;
        let mut current_alias_lost_event_fill = false;
        let mut fields = record.fields.drain(..).peekable();
        while let Some(mut entry) = fields.next() {
            if entry.sig.0 == *b"ENAM"
                && field_value_to_u32(&entry.value)
                    .is_some_and(|event| FO76_ONLY_QUST_EVENT_TYPES.contains(&event))
            {
                continue;
            }
            // VMAD is retained (and FormKey-remapped by the schema-driven
            // mapper) so quest Papyrus script bindings survive; without it,
            // GetVMQuestVariable conditions cannot resolve their variable names.
            if entry.sig.0 == *b"QOBJ" {
                in_objective = true;
            }
            if entry.sig.0 == *b"ANAM" {
                after_next_alias_id = true;
                after_objective_target = false;
                in_objective = false;
                retained.push(entry);
                continue;
            }
            // FO76 uses objective-scope SNAM for StageToSet; FO4 interprets an
            // unscoped SNAM as a SWF path, so it cannot cross this boundary.
            if in_objective && entry.sig.0 == *b"SNAM" {
                after_objective_target = false;
                continue;
            }
            // The alias chain (everything after the NextAliasID anchor) is
            // retained so the FO4 alias table is rebuilt; scenes, dialogue, and
            // packages resolve their alias references against it. Runtime-unsafe
            // or FO76-only alias subrecords are dropped by QUST_DROP_SIGS.
            if after_next_alias_id && QUST_ALIAS_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                after_objective_target = false;
                if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                    current_alias_id = (entry.sig.0 == *b"ALST")
                        .then(|| field_value_to_u32(&entry.value))
                        .flatten();
                    current_alias_fnam_index = None;
                    current_alias_lost_event_fill = false;
                }
                if entry.sig.0 == *b"ALFE" {
                    let event = field_value_to_u32(&entry.value);
                    let event_data = fields.peek().and_then(|next| {
                        (next.sig.0 == *b"ALFD")
                            .then(|| field_value_to_u32(&next.value))
                            .flatten()
                    });
                    if let (Some(alias_id), Some(event), Some(event_data)) =
                        (current_alias_id, event, event_data)
                        && qust_event_alias_rewrites_to_player(
                            alias_id,
                            event,
                            event_data,
                            &player_event_consumer_alias_ids,
                            &remove_players_alias_ids,
                        )
                    {
                        fields.next();
                        retained.push(FieldEntry {
                            sig: SubrecordSig(*b"ALFR"),
                            value: FieldValue::FormKey(FormKey {
                                local: FO4_PLAYER_REF_FORM_ID,
                                plugin: interner.intern(FO4_MASTER_NAME),
                            }),
                        });
                        continue;
                    }
                }
                if QUST_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                    if matches!(&entry.sig.0, b"ALFE" | b"ALFD") {
                        current_alias_lost_event_fill = true;
                        if let Some(index) = current_alias_fnam_index {
                            if let Some(fnam) = retained.get_mut(index) {
                                mark_qust_alias_fnam_optional(&mut fnam.value);
                            }
                        }
                    }
                    continue;
                }
                if entry.sig.0 == *b"FNAM" {
                    if current_alias_lost_event_fill {
                        mark_qust_alias_fnam_optional(&mut entry.value);
                    }
                    current_alias_fnam_index = Some(retained.len());
                }
                retained.push(entry);
                continue;
            }
            if entry.sig.0 == *b"QSTA" {
                after_objective_target = true;
                continue;
            }
            if after_objective_target
                && QUST_OBJECTIVE_TARGET_CONDITION_SIGS
                    .iter()
                    .any(|sig| entry.sig.0 == *sig)
            {
                continue;
            }
            after_objective_target = false;
            if QUST_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                continue;
            }
            retained.push(entry);
        }
        drop(fields);
        record.fields = retained;
    }

    /// FO76 stores QUST quest-data in a `DATA` subrecord; FO4 stores it in
    /// `DNAM` ("General"). The translation map drops FO76 `DATA`, so without this
    /// relayout every converted QUST loses its quest-data — including the
    /// `start_game_enabled` flag and quest type — and no quest auto-starts. That
    /// leaves alias-gated dialogue unreachable (NPCs show no Talk prompt) even
    /// though the DIAL/INFO records convert fine. Renaming DATA→DNAM here (before
    /// the map drops `DATA`) lets the translator carry the synthesized DNAM
    /// through unchanged.
    pub(super) fn convert_qust_data_to_fo4_dnam(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"QUST" {
            return;
        }
        let editor_id_disable_reason = qust_editor_id_autostart_disable_reason(interner, record);
        // A QUST that already carries DNAM (a handful of FO76 records do) needs
        // no relayout, but explicit startup exclusions still apply.
        if record.fields.iter().any(|entry| entry.sig.0 == *b"DNAM") {
            let mut disabled = None;
            for entry in &mut record.fields {
                if entry.sig.0 == *b"DNAM"
                    && let FieldValue::Bytes(bytes) = &mut entry.value
                    && let Some(reason) = editor_id_disable_reason.or_else(|| {
                        bytes
                            .get(8)
                            .and_then(|&quest_type| qust_type_autostart_disable_reason(quest_type))
                    })
                    && let Some(source_flags) = suppress_quest_autostart(bytes)
                {
                    disabled = Some((reason, source_flags));
                }
            }
            if let Some((reason, source_flags)) = disabled {
                log_qust_autostart_disabled(interner, record, reason, source_flags);
            }
            return;
        }
        let dnam_sig = match SubrecordSig::from_str("DNAM") {
            Ok(s) => s,
            Err(_) => return,
        };
        let mut disabled = None;
        for entry in record.fields.iter_mut() {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            let FieldValue::Bytes(bytes) = &entry.value else {
                continue;
            };
            let Some(source_data) = parse_fo76_qust_data(bytes) else {
                return;
            };
            let mut dnam = build_fo4_qust_dnam(source_data);
            if let Some(reason) = editor_id_disable_reason
                .or_else(|| qust_type_autostart_disable_reason(source_data.quest_type))
                && let Some(source_flags) = suppress_quest_autostart(&mut dnam)
            {
                disabled = Some((reason, source_flags));
            }
            entry.sig = dnam_sig;
            entry.value = FieldValue::Bytes(dnam);
            break;
        }
        if let Some((reason, source_flags)) = disabled {
            log_qust_autostart_disabled(interner, record, reason, source_flags);
        }
    }
}

fn qust_editor_id_autostart_disable_reason(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> Option<&'static str> {
    let editor_id = qust_eid_lower(interner, record);
    if editor_id.as_deref() == Some("cb_highschoolpasystem_radioscenes") {
        return Some("explicit_high_school_pa_exclusion");
    }
    if qust_eid_is_test_or_dev(interner, record) {
        return Some("test_or_dev_editor_id");
    }
    // EN is FO76's event namespace; CB is a regional prefix (Cranberry Bog).
    if editor_id.is_some_and(|editor_id| editor_id.starts_with("en")) {
        return Some("event_editor_id_prefix_en");
    }
    None
}

fn qust_type_autostart_disable_reason(quest_type: u8) -> Option<&'static str> {
    match quest_type {
        FO76_QUST_TYPE_PUBLIC_EVENT => Some("quest_type_public_event"),
        FO76_QUST_TYPE_EVENT => Some("quest_type_event"),
        _ => None,
    }
}

fn log_qust_autostart_disabled(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
    reason: &str,
    source_flags: u16,
) {
    let plugin = interner
        .resolve(record.form_key.plugin)
        .unwrap_or("<unresolved>");
    let editor_id = qust_eid_lower(interner, record).unwrap_or_else(|| "<none>".to_string());
    let warning = interner.intern(&format!(
        "qust_start_game_disabled:form={plugin}:{:06X};editor_id={editor_id};reason={reason};source_flags=0x{source_flags:04X}",
        record.form_key.local,
    ));
    record.warnings.push(warning);
}
