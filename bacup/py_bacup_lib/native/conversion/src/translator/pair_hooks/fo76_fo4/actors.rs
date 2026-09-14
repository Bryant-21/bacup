use super::*;

pub(super) const CHINESE_STEALTH_ARMA_EDITOR_ID: &str = "AA_ArmorChineseStealth";

pub(super) const FO76_UPPER_BODY_SKIN_BIPED_MASK: u64 =
    (1 << (41 - 30)) | (1 << (42 - 30)) | (1 << (43 - 30)) | (1 << (44 - 30)) | (1 << (45 - 30));
pub(super) const FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN: u64 = 1 << 1;
pub(super) const FO4_BODY_BIPED_MASK: u64 = 1 << (33 - 30);
pub(super) const FO4_PIPBOY_BIPED_MASK: u64 = 1 << (60 - 30);
pub(super) const FO4_HUMAN_CHILD_RACE_FORM_ID: u32 = 0x11D83F;
pub(super) const FO76_ANIMS_GRIP_RIFLE_STRAIGHT_FORM_ID: u32 = 0x0464EF;
pub(super) const FO76_ANIMS_ALIEN_RIFLE_FORM_ID: u32 = 0x63E33B;

pub(super) const DESTRUCTIBLE_GROUP_SIGS: &[[u8; 4]] = &[
    *b"DEST", *b"DAMC", *b"DSTD", *b"DSTA", *b"DMDL", *b"DMDT", *b"DMDC", *b"DMDS", *b"DSTF",
    *b"HGLB", *b"ENLT", *b"ENLS", *b"AUUV",
];
const DSTD_FLAGS_OFFSET: usize = 3;
const DSTD_EXPLOSION_OFFSET: usize = 8;
const DSTD_CAP_DAMAGE: u8 = 0x01;
const DSTD_DESTROY: u8 = 0x04;
const DSTD_IGNORE_EXTERNAL_DAMAGE: u8 = 0x08;
pub(super) const RD01_ENC04_ASSASSIN_NPC_FORM_ID: u32 = 0x78BD9B;
pub(super) const CS_RAIDER_01_MELEE_FORM_ID: u32 = 0x047165;
pub(super) const CS_RAIDER_RANGED_FORM_ID: u32 = 0x03183B;
pub(super) const HONEY_BEAST_BEE_SWARM_BPTD_FORM_ID: u32 = 0x52501A;
pub(super) const HONEY_BEAST_BEE_SWARM_BPTD_EDITOR_ID: &str = "HoneyBeastBeeSwarmBodyPartData";
pub(super) const W05_ACTOR_NUKE_REACTION_SCRIPT: &str = "W05_ActorNukeReactionScript";
pub(super) const W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY: &str = "BaseOutfit";

pub(super) fn source_form_key_from_raw(raw: u32, source_plugin: crate::sym::Sym) -> FormKey {
    FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: source_plugin,
    }
}

pub(super) fn npc_perk_entry_value(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
    perk_key: crate::sym::Sym,
    rank_key: crate::sym::Sym,
) -> Option<FieldValue> {
    let (raw, rank) = match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            (raw, bytes.get(4).copied().unwrap_or(0))
        }
        FieldValue::Uint(value) => (u32::try_from(*value).ok()?, 0),
        FieldValue::Int(value) => (u32::try_from(*value).ok()?, 0),
        _ => return None,
    };

    Some(FieldValue::Struct(vec![
        (
            perk_key,
            FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin)),
        ),
        (rank_key, bytes_value(&[rank])),
    ]))
}

pub(super) fn npc_faction_value(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<FieldValue> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != 5 {
        return None;
    }
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Some(FieldValue::Struct(vec![
        (
            interner.intern("faction"),
            FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin)),
        ),
        (interner.intern("rank"), bytes_value(&bytes[4..5])),
    ]))
}

pub(super) fn npc_container_value(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<FieldValue> {
    let FieldValue::Bytes(bytes) = value else {
        return None;
    };
    if bytes.len() != 8 {
        return None;
    }
    let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
    Some(FieldValue::Struct(vec![
        (
            interner.intern("item"),
            FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin)),
        ),
        (interner.intern("count"), bytes_value(&bytes[4..8])),
    ]))
}

pub(super) fn source_form_key_value(
    value: &FieldValue,
    source_plugin: crate::sym::Sym,
) -> Option<FieldValue> {
    match value {
        FieldValue::FormKey(_) => Some(value.clone()),
        FieldValue::Uint(value) => u32::try_from(*value)
            .ok()
            .map(|raw| FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin))),
        FieldValue::Int(value) => u32::try_from(*value)
            .ok()
            .map(|raw| FieldValue::FormKey(source_form_key_from_raw(raw, source_plugin))),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            let raw = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
            Some(FieldValue::FormKey(source_form_key_from_raw(
                raw,
                source_plugin,
            )))
        }
        _ => None,
    }
}

pub(super) fn destructible_header_health(
    interner: &crate::sym::StringInterner,
    value: &FieldValue,
) -> Option<i64> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            Some(i32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64)
        }
        FieldValue::Struct(fields) => named_value_canonical(fields, "health", interner)
            .and_then(field_value_to_i64)
            .or_else(|| {
                named_value_canonical(fields, "header", interner)
                    .and_then(|header| destructible_header_health(interner, header))
            }),
        _ => field_value_to_i64(value),
    }
}

impl Fo76Fo4Hook {
    pub(super) fn ensure_w05_nuke_reaction_base_outfit(ctx: &PairCtx<'_>, record: &mut Record) {
        if record.sig.0 != *b"NPC_" {
            return;
        }

        let mut default_outfit_fields =
            record.fields.iter().filter(|entry| entry.sig.0 == *b"DOFT");
        let Some(default_outfit_field) = default_outfit_fields.next() else {
            return;
        };
        if default_outfit_fields.next().is_some() {
            return;
        }
        let Some(default_outfit) =
            source_form_key_value(&default_outfit_field.value, record.form_key.plugin).and_then(
                |value| match value {
                    FieldValue::FormKey(form_key) if form_key.local != 0 => Some(form_key),
                    _ => None,
                },
            )
        else {
            return;
        };
        let Some(default_outfit_plugin) = ctx.interner.resolve(default_outfit.plugin) else {
            return;
        };

        let mut vmad_fields = record
            .fields
            .iter_mut()
            .filter(|entry| entry.sig.0 == *b"VMAD");
        let Some(vmad_field) = vmad_fields.next() else {
            return;
        };
        if vmad_fields.next().is_some() {
            return;
        }
        let FieldValue::Bytes(bytes) = &mut vmad_field.value else {
            return;
        };
        if !vmad_contains_name(bytes, W05_ACTOR_NUKE_REACTION_SCRIPT) {
            return;
        }

        let Some(mut payload) = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(
            bytes,
            ctx.target_master_names,
            FO76_MASTER_NAME,
            Some("NPC_"),
        ) else {
            return;
        };
        if build_vmad_bytes_from_payload(&payload, ctx.target_master_names, FO76_MASTER_NAME)
            .as_deref()
            != Some(bytes.as_slice())
        {
            return;
        }

        let Some(scripts) = payload
            .get_mut("Scripts")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        let matching_scripts = scripts
            .iter()
            .enumerate()
            .filter(|(_, script)| {
                script
                    .get("ScriptName")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case(W05_ACTOR_NUKE_REACTION_SCRIPT))
            })
            .map(|(index, _)| index)
            .collect::<Vec<_>>();
        let [script_index] = matching_scripts.as_slice() else {
            return;
        };
        let Some(properties) = scripts[*script_index]
            .get_mut("Properties")
            .and_then(serde_json::Value::as_array_mut)
        else {
            return;
        };
        if properties.iter().any(|property| {
            property
                .get("propertyName")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY))
        }) {
            return;
        }

        properties.push(serde_json::json!({
            "propertyName": W05_ACTOR_NUKE_BASE_OUTFIT_PROPERTY,
            "Type": "Object",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": {
                "Alias": -1,
                "FormID": {
                    "reference": {
                        "plugin": default_outfit_plugin,
                        "object_id": format!("{:06X}", default_outfit.local),
                    },
                },
            },
        }));
        let Some(encoded) =
            build_vmad_bytes_from_payload(&payload, ctx.target_master_names, FO76_MASTER_NAME)
        else {
            return;
        };
        *bytes = SmallVec::from_vec(encoded);
    }

    pub(super) fn strip_bee_swarm_ant_limb_replacements(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"BPTD"
            || record.form_key.local != HONEY_BEAST_BEE_SWARM_BPTD_FORM_ID
            || record.eid.and_then(|sym| interner.resolve(sym))
                != Some(HONEY_BEAST_BEE_SWARM_BPTD_EDITOR_ID)
        {
            return;
        }

        record.fields.retain(|entry| entry.sig.0 != *b"NAM1");
    }

    pub(super) fn arma_has_upper_body_skin(record: &Record) -> bool {
        record.sig.0 == *b"ARMA"
            && record.fields.iter().any(|entry| {
                entry.sig.0 == *b"XFLG"
                    && match entry.value {
                        FieldValue::Uint(flags) => flags & FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN != 0,
                        FieldValue::Int(flags) if flags >= 0 => {
                            (flags as u64) & FO76_ARMA_XFLG_HAS_UPPER_BODY_SKIN != 0
                        }
                        _ => false,
                    }
            })
    }

    pub(super) fn normalize_arma_upper_body_skin_slots(record: &mut Record) {
        if !Self::arma_has_upper_body_skin(record) {
            return;
        }
        let Some(value) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .map(|entry| &mut entry.value)
        else {
            return;
        };
        Self::remove_biped_slots(value, FO76_UPPER_BODY_SKIN_BIPED_MASK);
    }

    pub(super) fn normalize_child_underarmor_slots(record: &mut Record) {
        if record.sig.0 != *b"ARMO"
            || !record.fields.iter().any(|entry| {
                entry.sig.0 == *b"RNAM"
                    && matches!(
                        &entry.value,
                        FieldValue::FormKey(race) if race.local == FO4_HUMAN_CHILD_RACE_FORM_ID
                    )
            })
        {
            return;
        }

        let Some(value) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .map(|entry| &mut entry.value)
        else {
            return;
        };

        match value {
            FieldValue::Uint(mask)
                if *mask != 0 && *mask & !FO76_UPPER_BODY_SKIN_BIPED_MASK == 0 =>
            {
                *mask = FO4_BODY_BIPED_MASK;
            }
            FieldValue::Int(mask)
                if *mask > 0 && (*mask as u64) & !FO76_UPPER_BODY_SKIN_BIPED_MASK == 0 =>
            {
                *mask = FO4_BODY_BIPED_MASK as i64;
            }
            _ => {}
        }
    }

    pub(super) fn normalize_chinese_stealth_arma_pipboy_slot(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"ARMA"
            || !record.fields.iter().any(|entry| {
                if entry.sig.0 != *b"EDID" {
                    return false;
                }
                match &entry.value {
                    FieldValue::String(value) => interner
                        .resolve(*value)
                        .is_some_and(|value| value == CHINESE_STEALTH_ARMA_EDITOR_ID),
                    FieldValue::Bytes(value) => {
                        std::str::from_utf8(value).ok().is_some_and(|value| {
                            value.trim_end_matches('\0') == CHINESE_STEALTH_ARMA_EDITOR_ID
                        })
                    }
                    _ => false,
                }
            })
        {
            return;
        }
        if let Some(value) = record
            .fields
            .iter_mut()
            .find(|entry| entry.sig.0 == *b"BOD2")
            .map(|entry| &mut entry.value)
        {
            Self::remove_biped_slots(value, FO4_PIPBOY_BIPED_MASK);
        }
    }

    pub(super) fn remove_biped_slots(value: &mut FieldValue, slots: u64) {
        match value {
            FieldValue::Uint(mask) => *mask &= !slots,
            FieldValue::Int(mask) if *mask >= 0 => *mask &= !(slots as i64),
            _ => {}
        }
    }

    pub(super) fn normalize_npc_perk_entries(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"NPC_" {
            return;
        }

        let source_plugin = record.form_key.plugin;
        let perk_key = interner.intern("Perk");
        let rank_key = interner.intern("Rank");
        for entry in &mut record.fields {
            if entry.sig.0 != *b"PRKR" {
                continue;
            }
            if let Some(value) =
                npc_perk_entry_value(&entry.value, source_plugin, perk_key, rank_key)
            {
                entry.value = value;
            }
        }
    }

    pub(super) fn normalize_npc_raw_form_refs(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"NPC_" {
            return;
        }

        for entry in &mut record.fields {
            match &entry.sig.0 {
                b"SNAM" => {
                    if let Some(value) =
                        npc_faction_value(interner, &entry.value, record.form_key.plugin)
                    {
                        entry.value = value;
                    }
                }
                b"CNTO" => {
                    if let Some(value) =
                        npc_container_value(interner, &entry.value, record.form_key.plugin)
                    {
                        entry.value = value;
                    }
                }
                b"INAM" => {
                    if let Some(value) = source_form_key_value(&entry.value, record.form_key.plugin)
                    {
                        entry.value = value;
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn normalize_rd01_assassin_combat_style(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"NPC_" || record.form_key.local != RD01_ENC04_ASSASSIN_NPC_FORM_ID {
            return;
        }
        if record.eid.and_then(|sym| interner.resolve(sym)) != Some("RD01_Enc04_Assassin") {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"ZNAM" {
                continue;
            }
            if let FieldValue::FormKey(fk) = &mut entry.value {
                if fk.local == CS_RAIDER_01_MELEE_FORM_ID {
                    fk.local = CS_RAIDER_RANGED_FORM_ID;
                }
            }
        }
    }

    pub(super) fn normalize_cont_raw_form_refs(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"CONT" && record.sig.0 != *b"FURN" {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 == *b"CNTO"
                && let Some(value) =
                    npc_container_value(interner, &entry.value, record.form_key.plugin)
            {
                entry.value = value;
            }
        }
    }

    pub(super) fn strip_zero_health_cont_destructibles(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"CONT" {
            return;
        }

        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained = smallvec::SmallVec::new();
        let mut dropping_zero_health_group = false;

        for entry in old_fields {
            if entry.sig.0 == *b"DEST" {
                dropping_zero_health_group = destructible_header_health(interner, &entry.value)
                    .is_some_and(|health| health == 0);
                if dropping_zero_health_group {
                    continue;
                }
            } else if dropping_zero_health_group {
                if DESTRUCTIBLE_GROUP_SIGS.contains(&entry.sig.0) {
                    continue;
                }
                dropping_zero_health_group = false;
            }

            retained.push(entry);
        }

        record.fields = retained;
    }

    pub(super) fn normalize_explosive_mstt_destruction_flags(record: &mut Record) {
        if record.sig.0 != *b"MSTT" {
            return;
        }

        let has_explosive_destroy_stage = record.fields.iter().any(|entry| {
            let FieldValue::Bytes(bytes) = &entry.value else {
                return false;
            };
            entry.sig.0 == *b"DSTD"
                && bytes.len() >= DSTD_EXPLOSION_OFFSET + 4
                && bytes[DSTD_FLAGS_OFFSET] & (DSTD_CAP_DAMAGE | DSTD_DESTROY)
                    == DSTD_CAP_DAMAGE | DSTD_DESTROY
                && bytes[DSTD_EXPLOSION_OFFSET..DSTD_EXPLOSION_OFFSET + 4]
                    .iter()
                    .any(|byte| *byte != 0)
        });
        let has_terminal_ignore_external_damage = record.fields.iter().any(|entry| {
            let FieldValue::Bytes(bytes) = &entry.value else {
                return false;
            };
            entry.sig.0 == *b"DSTD"
                && bytes.len() > DSTD_FLAGS_OFFSET
                && bytes[0] == 0
                && bytes[DSTD_FLAGS_OFFSET] & DSTD_IGNORE_EXTERNAL_DAMAGE != 0
        });
        if !has_explosive_destroy_stage || !has_terminal_ignore_external_damage {
            return;
        }

        for entry in &mut record.fields {
            let FieldValue::Bytes(bytes) = &mut entry.value else {
                continue;
            };
            if entry.sig.0 != *b"DSTD" || bytes.len() <= DSTD_FLAGS_OFFSET {
                continue;
            }

            let is_terminal_stage = bytes[0] == 0;
            let flags = &mut bytes[DSTD_FLAGS_OFFSET];
            if *flags & (DSTD_CAP_DAMAGE | DSTD_DESTROY) == DSTD_CAP_DAMAGE | DSTD_DESTROY {
                *flags &= !DSTD_CAP_DAMAGE;
            }
            if is_terminal_stage {
                *flags &= !DSTD_IGNORE_EXTERNAL_DAMAGE;
            }
        }
    }
}
