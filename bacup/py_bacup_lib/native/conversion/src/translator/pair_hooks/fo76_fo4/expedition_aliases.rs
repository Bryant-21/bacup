use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum DirectStartAliasKind {
    PlayerReference,
    ModuleLocation,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DirectStartAliasRule {
    local_form_id: u32,
    editor_id: &'static str,
    anchor: [u8; 4],
    alias_id: u32,
    alias_name: &'static str,
    event_data: u32,
    kind: DirectStartAliasKind,
}

const fn player_rule(
    local_form_id: u32,
    editor_id: &'static str,
    alias_id: u32,
    alias_name: &'static str,
) -> DirectStartAliasRule {
    DirectStartAliasRule {
        local_form_id,
        editor_id,
        anchor: *b"ALST",
        alias_id,
        alias_name,
        event_data: FO76_QUEST_EVENT_REFERENCE3,
        kind: DirectStartAliasKind::PlayerReference,
    }
}

const fn module_location_rule(local_form_id: u32, editor_id: &'static str) -> DirectStartAliasRule {
    DirectStartAliasRule {
        local_form_id,
        editor_id,
        anchor: *b"ALLS",
        alias_id: 3,
        alias_name: "ModuleLocation",
        event_data: FO4_QUEST_EVENT_LOCATION1,
        kind: DirectStartAliasKind::ModuleLocation,
    }
}

const DIRECT_START_ALIAS_RULES: &[DirectStartAliasRule] = &[
    player_rule(0x006B_AA3D, "xpd_ac01_mission_tax", 19, "ExpeditionLeader"),
    player_rule(
        0x006B_231C,
        "xpd_ac02_mission_sensation",
        19,
        "ExpeditionLeader",
    ),
    player_rule(0x0062_74EC, "xpd_pitt01_mission", 19, "ExpeditionLeader"),
    player_rule(0x0064_8280, "xpd_pitt02_mission", 19, "ExpeditionLeader"),
    player_rule(0x0064_6E8B, "xpd_hubre_takephoto_camerashy", 1, "Player"),
    player_rule(
        0x0064_22F9,
        "xpd_hubre_lostandfound_chessproblem",
        1,
        "Player",
    ),
    player_rule(
        0x0064_53B9,
        "xpd_hubre_repairrobot_conversionerror",
        1,
        "Player",
    ),
    player_rule(
        0x0064_7FC1,
        "xpd_hubre_repairrobot_dogexmachina",
        1,
        "Player",
    ),
    player_rule(
        0x0063_272C,
        "xpd_hubre_settledispute_garbageday",
        1,
        "Player",
    ),
    player_rule(0x0064_5790, "xpd_hubre_lostandfound_honorroll", 1, "Player"),
    player_rule(
        0x0063_3790,
        "xpd_hubre_takephoto_hotelmakeover",
        1,
        "Player",
    ),
    player_rule(
        0x0064_9ED9,
        "xpd_hubre_takephoto_impostorsyndrome",
        1,
        "Player",
    ),
    player_rule(
        0x0064_BAD0,
        "xpd_hubre_takephoto_letterstohome",
        1,
        "Player",
    ),
    player_rule(
        0x0064_A88E,
        "xpd_hubre_settledispute_loversquarrel",
        1,
        "Player",
    ),
    player_rule(0x0064_ACD7, "xpd_hubre_repairrobot_mrmakeover", 1, "Player"),
    player_rule(0x0064_54DF, "xpd_hubre_takephoto_nosurprises", 1, "Player"),
    player_rule(
        0x0064_3EB9,
        "xpd_hubre_lostandfound_readingmaterial",
        1,
        "Player",
    ),
    player_rule(
        0x0064_A14E,
        "xpd_hubre_settledispute_responderrecruitment",
        1,
        "Player",
    ),
    player_rule(0x0064_5D90, "xpd_hubre_repairrobot_fussfungle", 1, "Player"),
    player_rule(0x0064_7A3E, "xpd_hubre_repairrobot_toothache", 1, "Player"),
    player_rule(
        0x0064_78A4,
        "xpd_hubre_settledispute_ultimateshowdown",
        1,
        "Player",
    ),
    player_rule(
        0x0064_DBEF,
        "xpd_hubre_settledispute_weaponofchoice",
        1,
        "Player",
    ),
    player_rule(0x006D_5082, "xpd_ac_dialogue_billy", 2, "Player"),
    player_rule(0x006C_3517, "xpd_ac_dialogue_sal", 2, "Player"),
    player_rule(0x006F_A1DF, "xpd_ac_dialogue_mothercharlotte", 2, "Player"),
    player_rule(0x006F_AA69, "xpd_ac_dialogue_veraciocruz", 2, "Player"),
    module_location_rule(0x0064_BC52, "xpd_module_assassination"),
    module_location_rule(0x0064_D26C, "xpd_module_freeprisoners"),
    module_location_rule(0x006B_ABB6, "xpd_module_race"),
    module_location_rule(0x0065_07B5, "xpd_module_proximitytracker"),
    module_location_rule(0x0064_DD30, "xpd_module_gatheranddeposit"),
    module_location_rule(0x0064_BC50, "xpd_module_solvelocks"),
    module_location_rule(0x0064_CD4E, "xpd_module_repelenemies"),
    module_location_rule(0x0064_EA14, "xpd_module_defendnpc"),
    module_location_rule(0x0064_CD4D, "xpd_module_carryandthrow"),
    module_location_rule(0x0064_C2D0, "xpd_module_objectdestruction"),
];

fn field_value_matches_zstring(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
    expected: &str,
) -> bool {
    match value {
        FieldValue::String(value) => interner
            .resolve(*value)
            .is_some_and(|value| value.eq_ignore_ascii_case(expected)),
        FieldValue::Bytes(bytes) => {
            let value = bytes.strip_suffix(&[0]).unwrap_or(bytes);
            value.eq_ignore_ascii_case(expected.as_bytes())
        }
        _ => false,
    }
}

fn direct_start_alias_rule(
    interner: &crate::sym::StringInterner,
    record: &Record,
) -> Option<&'static DirectStartAliasRule> {
    if record.sig.0 != *b"QUST"
        || !interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO76_MASTER_NAME))
    {
        return None;
    }

    let local_form_id = record.form_key.local & 0x00FF_FFFF;
    let editor_id = record.fields.iter().find(|entry| entry.sig.0 == *b"EDID")?;
    DIRECT_START_ALIAS_RULES.iter().find(|rule| {
        rule.local_form_id == local_form_id
            && field_value_matches_zstring(interner, &editor_id.value, rule.editor_id)
    })
}

fn alias_end(fields: &[FieldEntry], start: usize) -> Option<usize> {
    fields[start + 1..]
        .iter()
        .position(|entry| entry.sig.0 == *b"ALED")
        .map(|offset| start + 1 + offset + 1)
}

fn alias_field_index(
    fields: &[FieldEntry],
    range: std::ops::Range<usize>,
    sig: [u8; 4],
) -> Option<usize> {
    range.clone().find(|&index| fields[index].sig.0 == sig)
}

fn adapt_player_alias(
    interner: &crate::sym::StringInterner,
    record: &mut Record,
    start: usize,
    end: usize,
) {
    let player_plugin = interner.intern(FO4_MASTER_NAME);
    let already_forced = record.fields[start..end].iter().any(|entry| {
        entry.sig.0 == *b"ALFR"
            && matches!(
                entry.value,
                FieldValue::FormKey(form_key)
                    if form_key.local & 0x00FF_FFFF == FO4_PLAYER_REF_FORM_ID
                        && form_key.plugin == player_plugin
            )
    });
    if already_forced
        && !record.fields[start..end]
            .iter()
            .any(|entry| entry.sig.0 == *b"ALFE" || entry.sig.0 == *b"ALFD")
    {
        return;
    }

    let Some(event_index) = alias_field_index(&record.fields, start..end, *b"ALFE") else {
        return;
    };
    let Some(event_data_index) = alias_field_index(&record.fields, start..end, *b"ALFD") else {
        return;
    };
    if event_data_index != event_index + 1
        || field_value_to_u32(&record.fields[event_index].value) != Some(FO76_QUEST_EVENT_SCPT)
        || field_value_to_u32(&record.fields[event_data_index].value)
            != Some(FO76_QUEST_EVENT_REFERENCE3)
    {
        return;
    }

    record.fields[event_index] = FieldEntry {
        sig: SubrecordSig(*b"ALFR"),
        value: FieldValue::FormKey(FormKey {
            local: FO4_PLAYER_REF_FORM_ID,
            plugin: player_plugin,
        }),
    };
    record.fields.remove(event_data_index);
}

fn adapt_module_location_alias(record: &mut Record, start: usize, end: usize) {
    let Some(flags_index) = alias_field_index(&record.fields, start..end, *b"FNAM") else {
        return;
    };
    let Some(flags) = field_value_to_u32(&record.fields[flags_index].value) else {
        return;
    };
    let Some(event_index) = alias_field_index(&record.fields, start..end, *b"ALFE") else {
        return;
    };
    let Some(event_data_index) = alias_field_index(&record.fields, start..end, *b"ALFD") else {
        return;
    };
    if flags & QUST_ALIAS_OPTIONAL_FLAG != 0
        || event_data_index != event_index + 1
        || field_value_to_u32(&record.fields[event_index].value) != Some(FO76_QUEST_EVENT_SCPT)
        || field_value_to_u32(&record.fields[event_data_index].value)
            != Some(FO4_QUEST_EVENT_LOCATION1)
    {
        return;
    }

    mark_qust_alias_fnam_optional(&mut record.fields[flags_index].value);
}

impl Fo76Fo4Hook {
    pub(super) fn adapt_direct_start_quest_aliases(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        let Some(rule) = direct_start_alias_rule(interner, record).copied() else {
            return;
        };

        let Some(start) = record.fields.iter().position(|entry| {
            entry.sig.0 == rule.anchor && field_value_to_u32(&entry.value) == Some(rule.alias_id)
        }) else {
            return;
        };
        let Some(end) = alias_end(&record.fields, start) else {
            return;
        };
        let Some(name_index) = alias_field_index(&record.fields, start..end, *b"ALID") else {
            return;
        };
        if !field_value_matches_zstring(interner, &record.fields[name_index].value, rule.alias_name)
        {
            return;
        }
        let Some(event_data_index) = alias_field_index(&record.fields, start..end, *b"ALFD") else {
            if rule.kind == DirectStartAliasKind::PlayerReference {
                adapt_player_alias(interner, record, start, end);
            }
            return;
        };
        if field_value_to_u32(&record.fields[event_data_index].value) != Some(rule.event_data) {
            return;
        }

        match rule.kind {
            DirectStartAliasKind::PlayerReference => {
                adapt_player_alias(interner, record, start, end)
            }
            DirectStartAliasKind::ModuleLocation => adapt_module_location_alias(record, start, end),
        }
    }
}
