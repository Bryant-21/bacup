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
pub(crate) const FO76_ONLY_QUST_EVENT_TYPES: [u32; 6] = [
    u32::from_le_bytes(*b"ADBO"),
    u32::from_le_bytes(*b"CBGN"),
    u32::from_le_bytes(*b"ILOC"),
    u32::from_le_bytes(*b"LCPG"),
    u32::from_le_bytes(*b"PCON"),
    u32::from_le_bytes(*b"QPMT"),
];
const PLAYER_CONNECT_AUTOSTART_FALLBACK_QUESTS: &[(u32, &str)] = &[
    (0x005E_AD3B, "bs01_mq00_breadcrumb_onconnect"),
    (0x0072_A2A7, "storm_mq01_breadcrumb_onconnect"),
    (0x007F_79A9, "burn_sq01_onconnect"),
];

/// Standing radio stations whose transmitter broadcasts from the start of the
/// game but whose quest FO76 started from a server-driven story event. FO4 has
/// no producer for those events, so without autostart the transmitter is listed
/// in the Pip-Boy and plays static forever.
///
/// Entries must own a *dedicated* transmitter REFR and a `BeginOnQuestStart`
/// broadcast scene, so that starting the quest is exactly what makes the
/// broadcast audible. Sequenced story broadcasts are deliberately excluded:
/// `BS00_Maxson*`/`BS00_Paladin*` are five quests sharing one transmitter each,
/// and auto-starting them would play five overlapping segments on one station.
const STANDING_RADIO_STATION_AUTOSTART_QUESTS: &[(u32, &str)] = &[
    (0x0069_9466, "storm_mq01_breadcrumb_radio"),
    // Overseer broadcast. Its transmitter alias fills by location-ref-type
    // (`ALFA`/`ALRT` → `AutofillGeneric02`) rather than a forced `ALFR`, so the
    // LCSR row in Vault76LocationExterior is what binds it — that survives
    // conversion intact; only the quest start is missing.
    (0x003F_BBB3, "w05_mq_101p_radio"),
    (0x0005_2DBF, "sfm04_organic_radio"),
    (0x0009_210E, "ff06_feed"),
    (0x0001_8ECF, "sfl02_track_radioquest"),
];

// Vetted and deliberately excluded, so a later sweep does not re-add them:
//   tw006                — transmitter 04DB42 is shared by three quests, and it
//                          has no BeginOnQuestStart scene: the BS00 overlap shape.
//   tw002                — carries no ALDN at all, so nothing can list it.
//   bos03, bos_radio,
//   nwot_carnivalradio   — no BeginOnQuestStart scene, so starting the quest
//                          lists a station that then broadcasts nothing.
//   rs02_beat            — owns nine scenes; auto-starting it at load would fire
//                          story content early, not just open a station.
pub(super) const MILE_CARAVAN_INTRO_QUEST_FORM_ID: u32 = 0x0076_B107;
const MILE_CARAVAN_INTRO_QUEST_EID: &str = "mile_caravanintro";
const NUKE_LAUNCH_CARD_PATROL_FORM_ID: u32 = 0x003E_133F;
const NUKE_LAUNCH_CARD_PATROL_EID: &str = "nuke_launchcardpatrol";
const NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_ID: u32 = 35;
const NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_EID: &str = "EventLocation";
const NUKE_LAUNCH_CARD_PATROL_LOCATION_REF_TYPES: &[(u32, u32)] =
    &[(1, 0x0001_F40F), (23, 0x0001_9479), (29, 0x0001_F40F)];
pub(super) const MILE_CARAVAN_CARGO_ALIAS_ID: u32 = 11;
pub(super) const MILE_CARAVAN_DEAD_BRAHMIN_ALIAS_ID: i16 = 10;
pub(super) const MILE_CARAVAN_BAD_CARGO_REF_FORM_ID: u32 = 0x0043_23B4;
pub(super) const MILE_CARAVAN_CARGO_MISC_FORM_ID: u32 = 0x0076_B147;
pub(super) const QUST_CREATE_REFERENCE_IN_ALIAS: u16 = 0x8000;
pub(super) const QUST_CREATE_REFERENCE_DEFAULT_LEVEL: u64 = 0;
pub(super) const TW043_FORM_ID: u32 = 0x0005_A243;
const TW043_EID: &str = "tw043";
pub(super) const TW043_GUARD_ALIAS_ID: u32 = 0;
pub(super) const TW043_GUARD_BASE_FORM_ID: u32 = 0x001E_7C60;
pub(super) const TW043_GUARD_POD_ALIAS_ID: i32 = 7;
pub(super) const LINK_PROTECTRON_POD_FORM_ID: u32 = 0x000F_39E3;

// QUST subrecords stripped from FO76 input before FO4 translation. These are
// normally FO76-only chunks FO4 does not accept. ALFC is admitted only inside
// a reference-alias row below; ALFE/ALFD pairs survive only when a
// producer-specific adapter proves their FO4 payload slots.
pub(super) const QUST_DROP_SIGS: &[[u8; 4]] = &[
    *b"ACBS", *b"ALFC", *b"ALFF", *b"ALSO", *b"ATTR", *b"COED", *b"DTGT", *b"ESAV", *b"ESCE",
    *b"ESCS", *b"ESDA", *b"ESRP", *b"ESRV", *b"KNAM", *b"NAM8", *b"QUCF", *b"SCCM", *b"SCFC",
    *b"SDCT", *b"SPPI", *b"SPPT", *b"TRAE", *b"VNAM",
];

// FO76 QUST.DATA layout variants (the `flags` field is a union on
// `record_form_version`; detected here by payload length). FO4 QUST.DNAM is a
// fixed 12-byte `struct:H,B,B,f,B,B,B,B`.
pub(super) const FO76_QUST_DATA_FLAGS64_LEN: usize = 20; // flags u64 (form_version >= 202)
pub(super) const FO76_QUST_DATA_FLAGS32_LEN: usize = 16; // flags u32 (form_version < 202)
pub(super) const FO4_QUST_DNAM_LEN: usize = 12;
// FO76-only QUST.DATA flag bit, above the u16 FO4 DNAM flags field and so lost
// to the truncation in `build_fo4_qust_dnam_with_daily_repeat_controller`.
pub(super) const FO76_QUST_FLAG_HOLOTAPE_ONLY: u64 = 0x0008_0000;
pub(super) const FO76_QUST_TYPE_DAILY: u8 = 5;
pub(super) const FO76_QUST_TYPE_PUBLIC_EVENT: u8 = 6;
pub(super) const FO76_QUST_TYPE_EVENT: u8 = 8;
pub(super) const FO4_QUST_TYPE_NONE: u8 = 0;
pub(super) const FO4_QUST_TYPE_MAIN_QUEST: u8 = 1;
pub(super) const FO4_QUST_TYPE_MISCELLANEOUS: u8 = 6;
pub(super) const FO4_QUST_TYPE_SIDE_QUESTS: u8 = 7;
const FO76_DAILY_REPEAT_CONTROLLER_SCRIPT: &[u8] = b"DefaultDailyQuestScript";
const FO76_ENGINE_MANAGED_REPEATABLE_DAILY_QUESTS: [u32; 10] = [
    0x001E_D40A,
    0x0045_E3D2,
    0x0045_E3D3,
    0x0047_CF15,
    0x0047_CF16,
    0x0063_D33F,
    0x0063_BED4,
    0x0063_D5BD,
    0x0062_1FB7,
    0x006F_D072,
];
const FO76_REPEATABLE_PUBLIC_EVENT_QUESTS: [u32; 32] = [
    0x0058_3D14,
    0x0068_F383,
    0x007E_BDF4,
    0x0046_48C3,
    0x0073_3DB5,
    0x0004_E257,
    0x0045_4CB6,
    0x0064_31CE,
    0x0009_210E,
    0x0049_8662,
    0x007F_1E8A,
    0x0012_E67E,
    0x0009_3187,
    0x0031_1433,
    0x0004_2F7E,
    0x0062_16DA,
    0x0065_E071,
    0x006A_D506,
    0x0010_BAE1,
    0x0051_09AF,
    0x0056_2877,
    0x0069_0659,
    0x003E_271D,
    0x0063_461B,
    0x0080_BFD0,
    0x0063_4B0B,
    0x0025_C090,
    0x0004_A357,
    0x005F_E4D7,
    0x0018_7531,
    0x0065_B0A8,
    0x0003_64D0,
];
const FO76_REPEATABLE_SINGLE_PLAYER_EVENT_QUESTS: [(u32, &str); 21] = [
    (0x0013_FB17, "lookouttowerquest"),
    (0x0054_F1A4, "comp_rq_fetch"),
    (0x0056_FB76, "comp_rq_kill"),
    (0x0057_27AD, "comp_rq_rescue"),
    (0x0055_FD53, "comp_visitor"),
    (
        0x0058_215B,
        "comp_rq_fetch_specificaliases_beckett_000_saddiary",
    ),
    (
        0x0058_2164,
        "comp_rq_rescue_specificaliases_beckett_001_cultistsage",
    ),
    (0x0058_2163, "comp_rq_fetch_specificaliases_beckett_002_key"),
    (
        0x0058_2160,
        "comp_rq_kill_specificaliases_beckett_003_bronx",
    ),
    (
        0x0058_2167,
        "comp_rq_fetch_specificaliases_beckett_004_cave",
    ),
    (
        0x0058_2165,
        "comp_rq_kill_specificaliases_beckett_005_blood",
    ),
    (
        0x0058_215A,
        "comp_rq_rescue_specificaliases_beckett_006_pet",
    ),
    (0x0058_215E, "comp_rq_kill_specificaliases_beckett_007_dj"),
    (
        0x0058_216A,
        "comp_rq_rescue_specificaliases_beckett_008_missnanny",
    ),
    (
        0x0058_2168,
        "comp_rq_fetch_specificaliases_beckett_009_holotapes",
    ),
    (
        0x0058_215F,
        "comp_rq_fetch_specificaliases_beckett_010_poisonedfood",
    ),
    (0x0058_215D, "comp_rq_kill_specificaliases_beckett_011_eye"),
    (
        0x005A_272F,
        "comp_rq_kill_specificaliases_beckett_bloodeaglerandomloc",
    ),
    (
        0x005A_2730,
        "comp_rq_kill_specificaliases_beckett_bloodeagledungeon",
    ),
    (0x005A_272D, "comp_rq_fetch_specificaliases_legendaryarmor"),
    (0x005A_272E, "comp_rq_fetch_specificaliases_legendaryweapon"),
];
pub(super) const BURN_GRUNT_HUNT_FORM_ID: u32 = 0x007D_6A80;
const BURN_GRUNT_HUNT_EID: &str = "burn_bountyhunt_grunthunt";
pub(super) const BOSZ01_REPEATABLE_FORM_ID: u32 = 0x0010_D89F;
const BOSZ01_REPEATABLE_EID: &str = "bosz01";
pub(super) const FO76_QSDD_AUTO_RESTART_FLAG: u32 = 0x2;

/// Relayout an FO76 QUST `DATA` payload into an FO4 QUST `DNAM` payload.
///
/// The low 16 flag bits are bit-identical between the two games except that a
/// daily quest with FO76's repeat-controller binding, or an explicitly
/// whitelisted repeatable event, must become resettable in FO4.
/// FO76-only flag bits (>= 0x10000) are dropped by the u16 truncation.
/// `priority` and `delay_time` carry over at their FO4 offsets; the incompatible
/// quest-type enum is mapped explicitly.
/// Returns `None` for an unrecognized length so the caller leaves the field
/// untouched.
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
    build_fo4_qust_dnam_with_daily_repeat_controller(source, true, false)
}

fn build_fo4_qust_dnam_with_daily_repeat_controller(
    source: Fo76QuestData,
    has_daily_repeat_controller: bool,
    is_repeatable_event: bool,
) -> smallvec::SmallVec<[u8; 32]> {
    let mut dnam: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
    dnam.resize(FO4_QUST_DNAM_LEN, 0);
    let mut flags = source.flags as u16;
    if (source.quest_type == FO76_QUST_TYPE_DAILY && has_daily_repeat_controller)
        || is_repeatable_event
    {
        flags &= !QUST_DNAM_FLAG_RUN_ONCE;
    }
    dnam[0..2].copy_from_slice(&flags.to_le_bytes());
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
        2 | 3 | FO76_QUST_TYPE_DAILY => FO4_QUST_TYPE_SIDE_QUESTS,
        // Side quests retain each event's title and native quest-start notification.
        FO76_QUST_TYPE_PUBLIC_EVENT | FO76_QUST_TYPE_EVENT => FO4_QUST_TYPE_SIDE_QUESTS,
        7 => FO4_QUST_TYPE_MISCELLANEOUS,
        _ => FO4_QUST_TYPE_NONE,
    }
}

// FO4 QUST.DNAM flag bits (low u16 of the flags field).
pub(super) const QUST_DNAM_FLAG_START_GAME_ENABLED: u16 = 0x0001;
pub(super) const QUST_DNAM_FLAG_STARTS_ENABLED: u16 = 0x0010;
pub(super) const QUST_DNAM_FLAG_RUN_ONCE: u16 = 0x0100;

fn qust_has_daily_repeat_controller(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    let is_fo76_master_record = interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME));
    if is_fo76_master_record
        && FO76_ENGINE_MANAGED_REPEATABLE_DAILY_QUESTS
            .contains(&(record.form_key.local & 0x00FF_FFFF))
    {
        return true;
    }

    let mut vmad_fields = record.fields.iter().filter(|entry| entry.sig.0 == *b"VMAD");
    let Some(vmad) = vmad_fields.next() else {
        return false;
    };
    if vmad_fields.next().is_some() {
        return false;
    }
    let FieldValue::Bytes(bytes) = &vmad.value else {
        return false;
    };
    let Some(version) = qust_vmad_read_u16(bytes, 0) else {
        return false;
    };
    let Some(object_format) = qust_vmad_read_u16(bytes, 2) else {
        return false;
    };
    let Some(script_count) = qust_vmad_read_u16(bytes, 4) else {
        return false;
    };
    if version != FO76_VMAD_VERSION || object_format != FO76_VMAD_OBJECT_FORMAT {
        return false;
    }

    let mut offset = 6;
    for _ in 0..script_count {
        let Some(script_name) = qust_vmad_read_script(bytes, &mut offset, object_format) else {
            return false;
        };
        if script_name.eq_ignore_ascii_case(FO76_DAILY_REPEAT_CONTROLLER_SCRIPT) {
            return true;
        }
    }
    false
}

fn qust_is_repeatable_event(interner: &crate::sym::StringInterner, record: &Record) -> bool {
    let is_source_record = interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME));
    if !is_source_record {
        return false;
    }

    let local = record.form_key.local & 0x00FF_FFFF;
    let eid = qust_eid_lower(interner, record);
    FO76_REPEATABLE_PUBLIC_EVENT_QUESTS.contains(&local)
        || eid.as_deref().is_some_and(|eid| {
            FO76_REPEATABLE_SINGLE_PLAYER_EVENT_QUESTS.iter().any(
                |(expected_local, expected_eid)| local == *expected_local && eid == *expected_eid,
            )
        })
        || (local == BOSZ01_REPEATABLE_FORM_ID && eid.as_deref() == Some(BOSZ01_REPEATABLE_EID))
}

fn qust_is_burn_grunt_hunt_autorestart(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
        && record.form_key.local & 0x00FF_FFFF == BURN_GRUNT_HUNT_FORM_ID
        && qust_eid_lower(interner, record).as_deref() == Some(BURN_GRUNT_HUNT_EID)
        && record
            .fields
            .iter()
            .any(|entry| entry.sig.0 == *b"QSDD" && qsdd_has_auto_restart(interner, &entry.value))
}

fn qsdd_has_auto_restart(interner: &crate::sym::StringInterner, value: &FieldValue) -> bool {
    match value {
        FieldValue::Struct(fields) => fields.iter().any(|(name, value)| {
            interner.resolve(*name) == Some("flags")
                && field_value_to_u32(value)
                    .is_some_and(|flags| flags & FO76_QSDD_AUTO_RESTART_FLAG != 0)
        }),
        FieldValue::Bytes(bytes) if bytes.len() >= 12 => {
            u32::from_le_bytes(bytes[8..12].try_into().unwrap()) & FO76_QSDD_AUTO_RESTART_FLAG != 0
        }
        _ => false,
    }
}

/// Lowercased QUST EditorID, from `eid` or the raw `EDID` subrecord.
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

/// True when the QUST's editorID marks it as an NPC conversation quest. FO76
/// names these `Dialogue_*` / `*_Dialogue_*` / `W05_Dialogue*` /
/// `XPD_Dialogue_*` / `NPCConversation_*`.
/// Gameplay quests are named by quest-type prefix (`RE_`, `*_MQ_`, `MTR*`,
/// `FF*`, `EN*`, `Test*`). Naming is the only signal that cleanly separates the
/// two; every structural signal also matches random encounters / events.
pub(crate) fn qust_eid_is_dialogue_conversation(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    qust_eid_lower(interner, record)
        .is_some_and(|s| s.contains("dialogue") || s.contains("npcconversation"))
}

pub(crate) fn qust_uses_player_connect_autostart_fallback(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> bool {
    qust_autostart_fallback_flags(interner, record) != 0
}

/// The DNAM flag bits an allowlisted quest must gain to start on its own.
///
/// Standing radio stations need `StartsEnabled` alongside `StartGameEnabled`:
/// every vanilla station pairs them (`DN067_Radio` = 0x0011, `DN125_Radio` =
/// 0x0111 with RunOnce), and `StartGameEnabled` alone starts the quest without
/// enabling it, so the transmitter alias never goes on air.
fn qust_autostart_fallback_flags(interner: &crate::sym::StringInterner, record: &Record) -> u16 {
    if qust_matches_autostart_fallback(interner, record, STANDING_RADIO_STATION_AUTOSTART_QUESTS) {
        QUST_DNAM_FLAG_START_GAME_ENABLED | QUST_DNAM_FLAG_STARTS_ENABLED
    } else if qust_matches_autostart_fallback(
        interner,
        record,
        PLAYER_CONNECT_AUTOSTART_FALLBACK_QUESTS,
    ) {
        QUST_DNAM_FLAG_START_GAME_ENABLED
    } else {
        0
    }
}

fn qust_matches_autostart_fallback(
    interner: &crate::sym::StringInterner,
    record: &Record,
    allowlist: &[(u32, &str)],
) -> bool {
    if !interner
        .resolve(record.form_key.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return false;
    }
    let Some(editor_id) = qust_eid_lower(interner, record) else {
        return false;
    };
    let local = record.form_key.local & 0x00FF_FFFF;
    allowlist
        .iter()
        .any(|&(form_id, expected_editor_id)| local == form_id && editor_id == expected_editor_id)
}

fn force_qust_autostart_flags(record: &mut Record, force_flags: u16) {
    for entry in &mut record.fields {
        if entry.sig.0 != *b"DNAM" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            continue;
        };
        if bytes.len() < 2 {
            continue;
        }
        let flags = u16::from_le_bytes([bytes[0], bytes[1]]) | force_flags;
        bytes[0..2].copy_from_slice(&flags.to_le_bytes());
    }
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

/// True when a QUST alias `ALFR` (Forced Reference) carries a null FormID.
///
/// FO76 authors a script-filled reference alias as "fill type = Forced
/// Reference, reference = NULL". FO4 encodes the same intent by emitting **no**
/// fill subrecord at all — `Fallout4.esm` contains 0 null `ALFR` values across
/// its 11,573 QUST aliases, and 2,787 reference aliases with no fill subrecord
/// (1,972 of them non-Optional). Carrying FO76's null `ALFR` through makes FO4
/// attempt a forced-reference fill that can never succeed; on a non-Optional
/// alias that aborts quest start, so every quest object the quest owns stays
/// unprotected (and the quest never runs at all).
fn qust_alias_forced_reference_is_null(value: &FieldValue) -> bool {
    match value {
        FieldValue::FormKey(form_key) => form_key.local & 0x00FF_FFFF == 0,
        FieldValue::Bytes(bytes) => {
            bytes.len() >= 4 && u32::from_le_bytes(bytes[..4].try_into().expect("four bytes")) == 0
        }
        FieldValue::Uint(value) => *value as u32 & 0x00FF_FFFF == 0,
        FieldValue::None => true,
        _ => false,
    }
}

fn field_value_matches_zstring(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
    expected: &str,
) -> bool {
    match value {
        FieldValue::String(value) => interner.resolve(*value) == Some(expected),
        FieldValue::Bytes(bytes) => {
            bytes.as_slice() == expected.as_bytes()
                || bytes
                    .strip_suffix(&[0])
                    .is_some_and(|value| value == expected.as_bytes())
        }
        _ => false,
    }
}

impl Fo76Fo4Hook {
    pub(crate) fn fo76_only_qust_event_types() -> &'static [u32; 6] {
        &FO76_ONLY_QUST_EVENT_TYPES
    }

    pub(super) fn strip_qust_runtime_scopes(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"QUST" {
            return;
        }

        Self::adapt_nuke_launch_card_patrol_location_ref_types(interner, record);
        Self::repair_mile_caravan_intro_cargo_alias(interner, record);
        Self::remove_tw043_guard_startup_link(interner, record);
        strip_w05_mqs_205_shutdown_script_binding(interner, record);
        strip_rd01_enc02_dead_topic_bindings(interner, record);
        let player_event_consumer_alias_ids = qust_vmad_player_event_consumer_alias_ids(record);
        let remove_players_alias_ids = qust_vmad_remove_players_alias_ids(record);
        let reference1_event_alias_ids = qust_reference1_event_alias_ids(record);
        let mut preserved_event_alias_ids = qust_preserved_event_alias_ids(interner, record);
        preserved_event_alias_ids.extend(qust_companion_event_alias_ids(interner, record));
        let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut after_next_alias_id = false;
        let mut in_objective = false;
        let mut current_alias_id = None;
        let mut current_alias_is_location = false;
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
                in_objective = false;
                retained.push(entry);
                continue;
            }
            // FO76 uses objective-scope SNAM for StageToSet; FO4 interprets an
            // unscoped SNAM as a SWF path, so it cannot cross this boundary.
            if in_objective && entry.sig.0 == *b"SNAM" {
                continue;
            }
            // The alias chain (everything after the NextAliasID anchor) is
            // retained so the FO4 alias table is rebuilt; scenes, dialogue, and
            // packages resolve their alias references against it. Runtime-unsafe
            // or FO76-only alias subrecords are dropped by QUST_DROP_SIGS.
            if after_next_alias_id && QUST_ALIAS_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                if QUST_ALIAS_ANCHOR_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                    current_alias_is_location = entry.sig.0 == *b"ALLS";
                    current_alias_id = (entry.sig.0 == *b"ALST")
                        .then(|| field_value_to_u32(&entry.value))
                        .flatten();
                    current_alias_fnam_index = None;
                    current_alias_lost_event_fill = false;
                }
                if entry.sig.0 == *b"ALED" {
                    current_alias_id = None;
                    current_alias_is_location = false;
                    current_alias_fnam_index = None;
                    current_alias_lost_event_fill = false;
                    retained.push(entry);
                    continue;
                }
                if entry.sig.0 == *b"ALFE" {
                    let event = field_value_to_u32(&entry.value);
                    let event_data = fields
                        .peek()
                        .filter(|next| next.sig.0 == *b"ALFD")
                        .and_then(|next| field_value_to_u32(&next.value));
                    if current_alias_is_location
                        && event.zip(event_data).is_some_and(|(event, event_data)| {
                            qust_location_event_alias_is_proven(event, event_data)
                        })
                    {
                        let event_data_entry = fields.next().expect("validated ALFD event data");
                        retained.push(entry);
                        retained.push(event_data_entry);
                        continue;
                    }
                    if let (Some(alias_id), Some(event), Some(event_data)) =
                        (current_alias_id, event, event_data)
                        && qust_event_alias_rewrites_to_reference1(
                            alias_id,
                            event,
                            event_data,
                            &reference1_event_alias_ids,
                        )
                    {
                        let mut event_data_entry =
                            fields.next().expect("validated ALFD event data");
                        set_u32_count(&mut event_data_entry.value, FO4_QUEST_EVENT_REFERENCE1);
                        retained.push(entry);
                        retained.push(event_data_entry);
                        continue;
                    }
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
                    if let (Some(alias_id), Some(event), Some(event_data)) =
                        (current_alias_id, event, event_data)
                        && qust_event_alias_is_proven(
                            alias_id,
                            event,
                            event_data,
                            &preserved_event_alias_ids,
                        )
                    {
                        let event_data_entry = fields.next().expect("validated ALFD event data");
                        retained.push(entry);
                        retained.push(event_data_entry);
                        continue;
                    }
                    if event_data.is_some() {
                        fields.next();
                    }
                    current_alias_lost_event_fill = true;
                    if let Some(index) = current_alias_fnam_index
                        && let Some(fnam) = retained.get_mut(index)
                    {
                        mark_qust_alias_fnam_optional(&mut fnam.value);
                    }
                    continue;
                }
                if entry.sig.0 == *b"ALFD" {
                    current_alias_lost_event_fill = true;
                    if let Some(index) = current_alias_fnam_index
                        && let Some(fnam) = retained.get_mut(index)
                    {
                        mark_qust_alias_fnam_optional(&mut fnam.value);
                    }
                    continue;
                }
                if entry.sig.0 == *b"ALFC"
                    && current_alias_id.is_some()
                    && matches!(&entry.value, FieldValue::FormKey(faction) if faction.local & 0x00FF_FFFF != 0)
                {
                    retained.push(entry);
                    continue;
                }
                // FO76's "script-filled" reference alias is `ALFR = NULL`; FO4
                // spells that as no fill subrecord at all. Left in place, FO4
                // runs a forced-reference fill that always fails — fatal to
                // quest start on a non-Optional alias.
                if entry.sig.0 == *b"ALFR" && qust_alias_forced_reference_is_null(&entry.value) {
                    continue;
                }
                if QUST_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
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
            if QUST_DROP_SIGS.iter().any(|sig| entry.sig.0 == *sig) {
                continue;
            }
            retained.push(entry);
        }
        drop(fields);
        record.fields = retained;
    }

    fn adapt_nuke_launch_card_patrol_location_ref_types(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.form_key.local != NUKE_LAUNCH_CARD_PATROL_FORM_ID
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
            || qust_eid_lower(interner, record).as_deref() != Some(NUKE_LAUNCH_CARD_PATROL_EID)
        {
            return;
        }

        let Some(next_alias_index) = record.fields.iter().position(|entry| {
            entry.sig.0 == *b"ANAM"
                && field_value_to_u32(&entry.value)
                    == Some(NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_ID)
        }) else {
            return;
        };
        if record.fields.iter().any(|entry| {
            entry.sig.0 == *b"ALLS"
                && field_value_to_u32(&entry.value)
                    == Some(NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_ID)
        }) {
            return;
        }

        let mut alias_ranges = Vec::new();
        let mut alias_start = None;
        for (index, entry) in record.fields.iter().enumerate() {
            if entry.sig.0 == *b"ALST" {
                alias_start = Some(index);
            } else if entry.sig.0 == *b"ALED"
                && let Some(start) = alias_start.take()
            {
                alias_ranges.push((start, index + 1));
            }
        }

        let mut replacements = Vec::new();
        for (start, end) in alias_ranges {
            let Some(alias_id) = field_value_to_u32(&record.fields[start].value) else {
                continue;
            };
            let Some(&(_, expected_ref_type)) = NUKE_LAUNCH_CARD_PATROL_LOCATION_REF_TYPES
                .iter()
                .find(|&&(expected_alias_id, _)| expected_alias_id == alias_id)
            else {
                continue;
            };
            if record.fields[start + 1..end].iter().any(|entry| {
                matches!(
                    entry.sig.0,
                    [b'A', b'L', b'F', b'A'] | [b'A', b'L', b'R', b'T']
                )
            }) {
                return;
            }
            let mut alff_indices =
                (start + 1..end).filter(|&index| record.fields[index].sig.0 == *b"ALFF");
            let Some(alff_index) = alff_indices.next() else {
                return;
            };
            if alff_indices.next().is_some() {
                return;
            }
            let ref_type = match record.fields[alff_index].value {
                FieldValue::FormKey(form_key) => form_key.local & 0x00FF_FFFF,
                ref value => field_value_to_u32(value).unwrap_or_default() & 0x00FF_FFFF,
            };
            if ref_type != expected_ref_type {
                return;
            }
            replacements.push((alff_index, record.fields[alff_index].value.clone()));
        }
        if replacements.len() != NUKE_LAUNCH_CARD_PATROL_LOCATION_REF_TYPES.len() {
            return;
        }

        for (alff_index, ref_type) in replacements.into_iter().rev() {
            record.fields[alff_index] = FieldEntry {
                sig: SubrecordSig(*b"ALFA"),
                value: FieldValue::Uint(u64::from(NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_ID)),
            };
            record.fields.insert(
                alff_index + 1,
                FieldEntry {
                    sig: SubrecordSig(*b"ALRT"),
                    value: ref_type,
                },
            );
        }

        set_u32_count(
            &mut record.fields[next_alias_index].value,
            NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_ID + 1,
        );
        for entry in [
            FieldEntry {
                sig: SubrecordSig(*b"ALLS"),
                value: FieldValue::Uint(u64::from(NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_ID)),
            },
            FieldEntry {
                sig: SubrecordSig(*b"ALID"),
                value: FieldValue::String(
                    interner.intern(NUKE_LAUNCH_CARD_PATROL_EVENT_LOCATION_ALIAS_EID),
                ),
            },
            FieldEntry {
                sig: SubrecordSig(*b"FNAM"),
                value: FieldValue::Uint(0),
            },
            FieldEntry {
                sig: SubrecordSig(*b"ALFE"),
                value: FieldValue::Uint(u64::from(FO76_QUEST_EVENT_SCPT)),
            },
            FieldEntry {
                sig: SubrecordSig(*b"ALFD"),
                value: FieldValue::Uint(u64::from(FO4_QUEST_EVENT_LOCATION1)),
            },
            FieldEntry {
                sig: SubrecordSig(*b"ALED"),
                value: FieldValue::None,
            },
        ]
        .into_iter()
        .rev()
        {
            record.fields.insert(next_alias_index + 1, entry);
        }
    }

    pub(super) fn repair_mile_caravan_intro_cargo_alias(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"QUST"
            || record.form_key.local != MILE_CARAVAN_INTRO_QUEST_FORM_ID
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
            || qust_eid_lower(interner, record).as_deref() != Some(MILE_CARAVAN_INTRO_QUEST_EID)
        {
            return;
        }

        let Some(alias_scope_start) = record
            .fields
            .iter()
            .position(|entry| entry.sig.0 == *b"ANAM")
            .map(|index| index + 1)
        else {
            return;
        };
        let mut cargo_alias_start = None;
        for (index, entry) in record.fields.iter().enumerate().skip(alias_scope_start) {
            if entry.sig.0 == *b"ALST"
                && field_value_to_u32(&entry.value) == Some(MILE_CARAVAN_CARGO_ALIAS_ID)
            {
                if cargo_alias_start.replace(index).is_some() {
                    return;
                }
            }
        }
        let Some(cargo_alias_start) = cargo_alias_start else {
            return;
        };
        let mut cargo_alias_end = None;
        for (index, entry) in record.fields.iter().enumerate().skip(cargo_alias_start + 1) {
            if entry.sig.0 == *b"ALED" {
                cargo_alias_end = Some(index);
                break;
            }
            if QUST_ALIAS_ANCHOR_SIGS
                .iter()
                .any(|anchor| entry.sig.0 == *anchor)
            {
                return;
            }
        }
        let Some(cargo_alias_end) = cargo_alias_end else {
            return;
        };
        let cargo_alias = &record.fields[cargo_alias_start + 1..cargo_alias_end];
        let mut alias_names = cargo_alias.iter().filter(|entry| entry.sig.0 == *b"ALID");
        if !alias_names
            .next()
            .is_some_and(|entry| field_value_matches_zstring(interner, &entry.value, "Cargo"))
            || alias_names.next().is_some()
            || cargo_alias
                .iter()
                .any(|entry| matches!(&entry.sig.0, b"ALCO" | b"ALCA" | b"ALCL"))
        {
            return;
        }
        let mut bad_forced_refs = cargo_alias.iter().enumerate().filter(|(_, entry)| {
            entry.sig.0 == *b"ALFR"
                && matches!(
                    &entry.value,
                    FieldValue::FormKey(form_key)
                        if form_key.local == MILE_CARAVAN_BAD_CARGO_REF_FORM_ID
                            && form_key.plugin == record.form_key.plugin
                )
        });
        let Some((bad_forced_ref_offset, _)) = bad_forced_refs.next() else {
            return;
        };
        if bad_forced_refs.next().is_some()
            || cargo_alias
                .iter()
                .filter(|entry| entry.sig.0 == *b"ALFR")
                .count()
                != 1
        {
            return;
        }

        let bad_forced_ref_index = cargo_alias_start + 1 + bad_forced_ref_offset;
        let mut create_in_dead_brahmin = smallvec::SmallVec::new();
        create_in_dead_brahmin.extend_from_slice(&MILE_CARAVAN_DEAD_BRAHMIN_ALIAS_ID.to_le_bytes());
        create_in_dead_brahmin.extend_from_slice(&QUST_CREATE_REFERENCE_IN_ALIAS.to_le_bytes());
        record.fields.remove(bad_forced_ref_index);
        record.fields.insert(
            bad_forced_ref_index,
            FieldEntry {
                sig: SubrecordSig(*b"ALCL"),
                value: FieldValue::Uint(QUST_CREATE_REFERENCE_DEFAULT_LEVEL),
            },
        );
        record.fields.insert(
            bad_forced_ref_index,
            FieldEntry {
                sig: SubrecordSig(*b"ALCA"),
                value: FieldValue::Bytes(create_in_dead_brahmin),
            },
        );
        record.fields.insert(
            bad_forced_ref_index,
            FieldEntry {
                sig: SubrecordSig(*b"ALCO"),
                value: FieldValue::FormKey(FormKey {
                    local: MILE_CARAVAN_CARGO_MISC_FORM_ID,
                    plugin: record.form_key.plugin,
                }),
            },
        );
    }

    pub(super) fn remove_tw043_guard_startup_link(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"QUST"
            || record.form_key.local != TW043_FORM_ID
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
            || qust_eid_lower(interner, record).as_deref() != Some(TW043_EID)
        {
            return;
        }

        let mut guard_alias_starts =
            record
                .fields
                .iter()
                .enumerate()
                .filter_map(|(index, entry)| {
                    (entry.sig.0 == *b"ALST"
                        && field_value_to_u32(&entry.value) == Some(TW043_GUARD_ALIAS_ID))
                    .then_some(index)
                });
        let Some(guard_alias_start) = guard_alias_starts.next() else {
            return;
        };
        if guard_alias_starts.next().is_some() {
            return;
        }

        let Some(guard_alias_end) = record.fields[guard_alias_start + 1..]
            .iter()
            .position(|entry| entry.sig.0 == *b"ALED")
            .map(|offset| guard_alias_start + 1 + offset)
        else {
            return;
        };
        if record.fields[guard_alias_start + 1..guard_alias_end]
            .iter()
            .any(|entry| {
                QUST_ALIAS_ANCHOR_SIGS
                    .iter()
                    .any(|anchor| entry.sig.0 == *anchor)
            })
        {
            return;
        }

        let guard_alias = &record.fields[guard_alias_start + 1..guard_alias_end];
        let mut alias_names = guard_alias.iter().filter(|entry| entry.sig.0 == *b"ALID");
        if !alias_names
            .next()
            .is_some_and(|entry| field_value_matches_zstring(interner, &entry.value, "TW043Guard"))
            || alias_names.next().is_some()
        {
            return;
        }
        let mut base_objects = guard_alias.iter().filter(|entry| entry.sig.0 == *b"ALCO");
        if !base_objects.next().is_some_and(|entry| {
            matches!(
                &entry.value,
                FieldValue::FormKey(form_key)
                    if form_key.local == TW043_GUARD_BASE_FORM_ID
                        && form_key.plugin == record.form_key.plugin
            )
        }) || base_objects.next().is_some()
        {
            return;
        }

        let mut linked_aliases = guard_alias
            .iter()
            .enumerate()
            .filter(|(_, entry)| entry.sig.0 == *b"ALLA");
        let Some((linked_alias_offset, linked_alias)) = linked_aliases.next() else {
            return;
        };
        if linked_aliases.next().is_some() {
            return;
        }
        let FieldValue::Bytes(payload) = &linked_alias.value else {
            return;
        };
        if payload.len() != 8
            || u32::from_le_bytes(payload[0..4].try_into().unwrap()) != LINK_PROTECTRON_POD_FORM_ID
            || i32::from_le_bytes(payload[4..8].try_into().unwrap()) != TW043_GUARD_POD_ALIAS_ID
        {
            return;
        }

        record
            .fields
            .remove(guard_alias_start + 1 + linked_alias_offset);
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
        let has_daily_repeat_controller = qust_has_daily_repeat_controller(interner, record);
        let is_repeatable_event = qust_is_repeatable_event(interner, record);
        let is_burn_grunt_hunt_autorestart = qust_is_burn_grunt_hunt_autorestart(interner, record);
        let has_displayed_objectives = qust_has_displayed_objectives(record);
        let force_autostart_flags = qust_autostart_fallback_flags(interner, record);
        // A QUST that already carries DNAM needs no relayout, but repeatability
        // and explicit startup exclusions still apply.
        if record.fields.iter().any(|entry| entry.sig.0 == *b"DNAM") {
            let mut disabled = None;
            for entry in &mut record.fields {
                if entry.sig.0 != *b"DNAM" {
                    continue;
                }
                let FieldValue::Bytes(bytes) = &mut entry.value else {
                    continue;
                };
                if (has_daily_repeat_controller
                    || is_repeatable_event
                    || is_burn_grunt_hunt_autorestart)
                    && bytes.len() >= 2
                {
                    let flags = u16::from_le_bytes([bytes[0], bytes[1]]) & !QUST_DNAM_FLAG_RUN_ONCE;
                    bytes[0..2].copy_from_slice(&flags.to_le_bytes());
                }
                if let Some(reason) = editor_id_disable_reason.or_else(|| {
                    bytes
                        .get(8)
                        .and_then(|&quest_type| qust_type_autostart_disable_reason(quest_type))
                }) && let Some(source_flags) = suppress_quest_autostart(bytes)
                {
                    disabled = Some((reason, source_flags));
                }
            }
            if let Some((reason, source_flags)) = disabled {
                log_qust_autostart_disabled(interner, record, reason, source_flags);
            }
            if force_autostart_flags != 0 {
                force_qust_autostart_flags(record, force_autostart_flags);
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
            let mut dnam = build_fo4_qust_dnam_with_daily_repeat_controller(
                source_data,
                has_daily_repeat_controller,
                is_repeatable_event || is_burn_grunt_hunt_autorestart,
            );
            if let Some(reason) = editor_id_disable_reason
                .or_else(|| qust_type_autostart_disable_reason(source_data.quest_type))
                .or_else(|| {
                    qust_flags_autostart_disable_reason(source_data.flags, has_displayed_objectives)
                })
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
        if force_autostart_flags != 0 {
            force_qust_autostart_flags(record, force_autostart_flags);
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
    if editor_id
        .as_deref()
        .is_some_and(|editor_id| editor_id.starts_with("en"))
        && !qust_eid_is_dialogue_conversation(interner, record)
    {
        return Some("event_editor_id_prefix_en");
    }
    None
}

/// FO76's `holotape_only` keeps a quest out of the Pip-Boy list while it still
/// runs, so most flagged quests are invisible holotape/dialogue containers that
/// must keep auto-starting or their tapes go dead. FO4 has no equivalent flag,
/// and the bit is dropped by the u16 DNAM truncation. A quest that claims to be
/// unlisted yet also carries displayable objectives contradicts itself — those
/// objectives can only be a dev/QA harness, and in FO4 they surface as a live,
/// untitled Pip-Boy entry. Suppress startup for exactly that case.
fn qust_flags_autostart_disable_reason(
    flags: u64,
    has_displayed_objectives: bool,
) -> Option<&'static str> {
    (flags & FO76_QUST_FLAG_HOLOTAPE_ONLY != 0 && has_displayed_objectives)
        .then_some("holotape_only_with_objectives")
}

fn qust_has_displayed_objectives(record: &Record) -> bool {
    record.fields.iter().any(|entry| entry.sig.0 == *b"QOBJ")
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
