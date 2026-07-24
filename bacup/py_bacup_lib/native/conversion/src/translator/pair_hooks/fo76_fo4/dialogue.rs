use super::*;

pub(super) const FO76_DIAL_CATEGORY_DETECTION: u8 = 4;
pub(super) const FO76_DIAL_CATEGORY_MISCELLANEOUS: u8 = 5;
pub(super) const FO4_DIAL_CATEGORY_DETECTION: u8 = 5;
pub(super) const FO4_DIAL_CATEGORY_MISCELLANEOUS: u8 = 7;
pub(super) const FO76_INFO_UNKNOWN_17: u32 = 1 << 17;
pub(super) const FO4_INFO_SAY_ONCE: u32 = 1 << 2;
pub(super) const SCEN_PLAYER_RESPONSE_SIGS: [[u8; 4]; 4] = [*b"PTOP", *b"NTOP", *b"NETO", *b"QTOP"];
pub(super) const SCEN_NPC_RESPONSE_SIGS: [[u8; 4]; 4] = [*b"NPOT", *b"NNGT", *b"NNUT", *b"NQUT"];
pub(crate) const XDI_MASTER_NAME: &str = "XDI.esm";
pub(crate) const XDI_SCENE_KEYWORD_FORM_ID: u32 = 0x000800;

pub(super) fn fo76_dial_category_to_fo4(value: u8) -> u8 {
    match value {
        FO76_DIAL_CATEGORY_DETECTION => FO4_DIAL_CATEGORY_DETECTION,
        FO76_DIAL_CATEGORY_MISCELLANEOUS => FO4_DIAL_CATEGORY_MISCELLANEOUS,
        _ => value,
    }
}

fn translate_info_unknown_17_to_say_once(value: &mut FieldValue) {
    match value {
        FieldValue::Uint(flags) if *flags & u64::from(FO76_INFO_UNKNOWN_17) != 0 => {
            *flags = (*flags & !u64::from(FO76_INFO_UNKNOWN_17)) | u64::from(FO4_INFO_SAY_ONCE);
        }
        FieldValue::Int(flags)
            if *flags >= 0 && (*flags as u64) & u64::from(FO76_INFO_UNKNOWN_17) != 0 =>
        {
            *flags = ((*flags as u64 & !u64::from(FO76_INFO_UNKNOWN_17))
                | u64::from(FO4_INFO_SAY_ONCE)) as i64;
        }
        FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
            let mut flags = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
            if flags & FO76_INFO_UNKNOWN_17 != 0 {
                flags = (flags & !FO76_INFO_UNKNOWN_17) | FO4_INFO_SAY_ONCE;
                bytes[0..4].copy_from_slice(&flags.to_le_bytes());
            }
        }
        FieldValue::List(values) => {
            for value in values {
                translate_info_unknown_17_to_say_once(value);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                translate_info_unknown_17_to_say_once(value);
            }
        }
        _ => {}
    }
}

#[derive(Debug, Default)]
pub(crate) struct XdiDialoguePlan {
    pub info_parent_overrides: HashMap<u32, u32>,
    pub dial_info_count_overrides: HashMap<u32, u32>,
    pub scene_player_topic_fillers: HashMap<u32, FormKey>,
    pub combined_info_splits: HashMap<u32, PlayerDialogueInfoSplit>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PlayerDialogueInfoSplit {
    pub player_parent: u32,
    pub npc_parent: u32,
}

#[derive(Debug, Default)]
pub(crate) struct ScenDialogueAction {
    pub player_topics: Vec<FormKey>,
    pub npc_topics: Vec<FormKey>,
}

pub(crate) fn scen_dialogue_actions(record: &Record) -> Vec<ScenDialogueAction> {
    if record.sig.0 != *b"SCEN" {
        return Vec::new();
    }

    let source_plugin = record.form_key.plugin;
    let mut actions = Vec::new();
    let mut current: Option<ScenDialogueAction> = None;
    for entry in &record.fields {
        if entry.sig.0 == *b"ANAM" {
            if let Some(action) = current.take() {
                actions.push(action);
            }
            current = Some(ScenDialogueAction::default());
            continue;
        }
        let Some(action) = current.as_mut() else {
            continue;
        };
        let destination = match &entry.sig.0 {
            b"ESCS" => &mut action.player_topics,
            b"ESCE" => &mut action.npc_topics,
            _ => continue,
        };
        if let Some(FieldValue::FormKey(form_key)) =
            source_form_key_value(&entry.value, source_plugin)
        {
            destination.push(form_key);
        }
    }
    if let Some(action) = current {
        actions.push(action);
    }
    actions
}

pub(super) fn scene_actions_require_xdi(actions: &[ScenDialogueAction]) -> bool {
    actions
        .iter()
        .any(|action| action.player_topics.len() > SCEN_PLAYER_RESPONSE_SIGS.len())
}

pub(super) fn ensure_xdi_scene_keyword(interner: &crate::sym::StringInterner, record: &mut Record) {
    let xdi_keyword = FormKey {
        local: XDI_SCENE_KEYWORD_FORM_ID,
        plugin: interner.intern(XDI_MASTER_NAME),
    };
    if record
        .fields
        .iter()
        .any(|field| field.sig.0 == *b"KWDA" && field.value == FieldValue::FormKey(xdi_keyword))
    {
        return;
    }

    record.fields.push(FieldEntry {
        sig: SubrecordSig(*b"KWDA"),
        value: FieldValue::FormKey(xdi_keyword),
    });
    let keyword_count = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"KWDA")
        .count() as u32;
    if let Some(field) = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"KSIZ")
    {
        crate::record::write_u32_field(&mut field.value, keyword_count);
    } else {
        let kwda_index = record
            .fields
            .iter()
            .position(|field| field.sig.0 == *b"KWDA")
            .expect("XDI KWDA was just inserted");
        record.fields.insert(
            kwda_index,
            FieldEntry {
                sig: SubrecordSig(*b"KSIZ"),
                value: FieldValue::Uint(u64::from(keyword_count)),
            },
        );
    }
}

pub(crate) fn build_xdi_dialogue_plan(
    scene_records: &[Record],
    info_parent_index: &HashMap<u32, u32>,
    prompt_info_ids: &HashSet<u32>,
) -> Result<XdiDialoguePlan, String> {
    let mut parsed_scenes = Vec::new();
    let mut retained_topic_use_counts: HashMap<u32, usize> = HashMap::new();
    let mut merge_anchor_use_counts: HashMap<u32, usize> = HashMap::new();
    for scene in scene_records {
        let actions = scen_dialogue_actions(scene);
        for action in &actions {
            for topics in [&action.player_topics, &action.npc_topics] {
                for topic in topics.iter().take(SCEN_PLAYER_RESPONSE_SIGS.len()) {
                    *retained_topic_use_counts.entry(topic.local).or_default() += 1;
                }
                if topics.len() > SCEN_PLAYER_RESPONSE_SIGS.len() {
                    *merge_anchor_use_counts
                        .entry(topics[SCEN_PLAYER_RESPONSE_SIGS.len() - 1].local)
                        .or_default() += 1;
                }
            }
        }
        let requires_xdi = scene_actions_require_xdi(&actions);
        parsed_scenes.push((scene.form_key.local, actions, requires_xdi));
    }

    let mut infos_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&info, &parent) in info_parent_index {
        infos_by_parent.entry(parent).or_default().push(info);
    }

    let mut plan = XdiDialoguePlan::default();
    let mut merged_topic_parents = HashMap::new();
    for (_, actions, requires_xdi) in &parsed_scenes {
        if !requires_xdi {
            continue;
        }
        for action in actions {
            if action.player_topics.len() > SCEN_PLAYER_RESPONSE_SIGS.len() {
                merge_xdi_topic_tail(
                    &action.player_topics,
                    &retained_topic_use_counts,
                    &merge_anchor_use_counts,
                    &infos_by_parent,
                    &mut merged_topic_parents,
                    &mut plan,
                )?;
            }
            if action.npc_topics.len() > SCEN_NPC_RESPONSE_SIGS.len() {
                merge_xdi_topic_tail(
                    &action.npc_topics,
                    &retained_topic_use_counts,
                    &merge_anchor_use_counts,
                    &infos_by_parent,
                    &mut merged_topic_parents,
                    &mut plan,
                )?;
            }
        }
    }
    for (_, actions, _) in &parsed_scenes {
        for action in actions {
            for (player_topic, npc_topic) in action.player_topics.iter().zip(&action.npc_topics) {
                if infos_by_parent
                    .get(&npc_topic.local)
                    .is_some_and(|infos| !infos.is_empty())
                {
                    continue;
                }
                let player_parent = merged_topic_parents
                    .get(&player_topic.local)
                    .copied()
                    .unwrap_or(player_topic.local);
                let npc_parent = merged_topic_parents
                    .get(&npc_topic.local)
                    .copied()
                    .unwrap_or(npc_topic.local);
                for &info in infos_by_parent
                    .get(&player_topic.local)
                    .into_iter()
                    .flatten()
                    .filter(|info| prompt_info_ids.contains(info))
                {
                    let split = PlayerDialogueInfoSplit {
                        player_parent,
                        npc_parent,
                    };
                    if let Some(previous) = plan.combined_info_splits.insert(info, split) {
                        if previous != split {
                            return Err(format!(
                                "INFO {info:06X} has conflicting dialogue splits {:06X}/{:06X} and {player_parent:06X}/{npc_parent:06X}",
                                previous.player_parent, previous.npc_parent,
                            ));
                        }
                        continue;
                    }
                    if let Some(previous) = plan.info_parent_overrides.insert(info, npc_parent)
                        && previous != player_parent
                        && previous != npc_parent
                    {
                        return Err(format!(
                            "INFO {info:06X} has conflicting dialogue parents {previous:06X} and {npc_parent:06X}"
                        ));
                    }
                    let npc_count = plan
                        .dial_info_count_overrides
                        .get(&npc_parent)
                        .copied()
                        .unwrap_or_else(|| {
                            infos_by_parent.get(&npc_parent).map_or(0, Vec::len) as u32
                        })
                        .checked_add(1)
                        .ok_or_else(|| format!("DIAL {npc_parent:06X} INFO count overflow"))?;
                    plan.dial_info_count_overrides.insert(npc_parent, npc_count);
                }
            }
        }
    }
    for (scene, actions, requires_xdi) in parsed_scenes {
        if !requires_xdi {
            continue;
        }
        let needs_filler = actions.iter().any(|action| {
            !action.player_topics.is_empty()
                && action.player_topics.len() < SCEN_PLAYER_RESPONSE_SIGS.len()
        });
        if !needs_filler {
            continue;
        }
        let filler = actions
            .iter()
            .flat_map(|action| action.player_topics.iter().chain(&action.npc_topics))
            .find(|topic| {
                plan.dial_info_count_overrides
                    .get(&topic.local)
                    .copied()
                    .unwrap_or_else(|| {
                        infos_by_parent.get(&topic.local).map_or(0, Vec::len) as u32
                    })
                    == 0
            })
            .copied()
            .ok_or_else(|| {
                format!(
                    "XDI-enabled SCEN {scene:06X} has an under-four player dialogue action but no empty DIAL for slot padding"
                )
            })?;
        plan.scene_player_topic_fillers.insert(scene, filler);
    }
    Ok(plan)
}

pub(crate) fn combined_player_dialogue_info_candidates(
    scene_records: &[Record],
    info_parent_index: &HashMap<u32, u32>,
) -> HashSet<u32> {
    let mut infos_by_parent: HashMap<u32, Vec<u32>> = HashMap::new();
    for (&info, &parent) in info_parent_index {
        infos_by_parent.entry(parent).or_default().push(info);
    }

    let mut candidates = HashSet::new();
    for scene in scene_records {
        for action in scen_dialogue_actions(scene) {
            for (player_topic, npc_topic) in action.player_topics.iter().zip(&action.npc_topics) {
                if infos_by_parent
                    .get(&npc_topic.local)
                    .is_some_and(|infos| !infos.is_empty())
                {
                    continue;
                }
                candidates.extend(
                    infos_by_parent
                        .get(&player_topic.local)
                        .into_iter()
                        .flatten()
                        .copied(),
                );
            }
        }
    }
    candidates
}

pub(crate) fn split_fo76_combined_player_dialogue_info(
    npc_response: &mut Record,
    player_form_key: FormKey,
    interner: &crate::sym::StringInterner,
) -> Result<Record, String> {
    let prompt = npc_response
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"RNAM")
        .map(|field| field.value.clone())
        .ok_or_else(|| {
            format!(
                "INFO {:06X} was planned for dialogue splitting but has no RNAM prompt",
                npc_response.form_key.local
            )
        })?;
    let conditions = npc_response
        .fields
        .iter()
        .filter(|field| matches!(&field.sig.0, b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2"))
        .cloned()
        .collect::<Vec<_>>();
    let subtitle_priority = npc_response
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"INAM")
        .cloned();

    npc_response.fields.retain(|field| {
        !matches!(
            &field.sig.0,
            b"RNAM" | b"CTDA" | b"CTDT" | b"CIS1" | b"CIS2"
        )
    });

    let mut player = Record::new(npc_response.sig, player_form_key);
    player.flags = npc_response.flags;
    player.fields.push(FieldEntry {
        sig: SubrecordSig(*b"ENAM"),
        value: FieldValue::Bytes(SmallVec::from_slice(&[0; 4])),
    });
    let mut response_data = Vec::with_capacity(20);
    response_data.extend_from_slice(&0_u32.to_le_bytes());
    response_data.push(1);
    response_data.extend_from_slice(&0_u32.to_le_bytes());
    response_data.push(0);
    response_data.extend_from_slice(&0_u16.to_le_bytes());
    response_data.extend_from_slice(&(-1_i32).to_le_bytes());
    response_data.extend_from_slice(&(-1_i32).to_le_bytes());
    player.fields.push(FieldEntry {
        sig: SubrecordSig(*b"TRDA"),
        value: FieldValue::Bytes(SmallVec::from_vec(response_data)),
    });
    player.fields.push(FieldEntry {
        sig: SubrecordSig(*b"NAM1"),
        value: prompt,
    });
    for sig in [*b"NAM2", *b"NAM3", *b"NAM4"] {
        player.fields.push(FieldEntry {
            sig: SubrecordSig(sig),
            value: FieldValue::String(interner.intern("")),
        });
    }
    player.fields.extend(conditions);
    if let Some(subtitle_priority) = subtitle_priority {
        player.fields.push(subtitle_priority);
    }
    Ok(player)
}

pub(super) fn merge_xdi_topic_tail(
    topics: &[FormKey],
    retained_topic_use_counts: &HashMap<u32, usize>,
    merge_anchor_use_counts: &HashMap<u32, usize>,
    infos_by_parent: &HashMap<u32, Vec<u32>>,
    merged_topic_parents: &mut HashMap<u32, u32>,
    plan: &mut XdiDialoguePlan,
) -> Result<(), String> {
    let anchor = topics[SCEN_PLAYER_RESPONSE_SIGS.len() - 1].local;
    if retained_topic_use_counts
        .get(&anchor)
        .copied()
        .unwrap_or_default()
        != merge_anchor_use_counts
            .get(&anchor)
            .copied()
            .unwrap_or_default()
    {
        return Err(format!(
            "XDI merge anchor DIAL {anchor:06X} is shared in a retained SCEN slot without the same merged tail"
        ));
    }
    let mut anchor_count = plan
        .dial_info_count_overrides
        .get(&anchor)
        .copied()
        .unwrap_or_else(|| infos_by_parent.get(&anchor).map_or(0, Vec::len) as u32);
    for extra in topics.iter().skip(SCEN_PLAYER_RESPONSE_SIGS.len()) {
        if retained_topic_use_counts
            .get(&extra.local)
            .copied()
            .unwrap_or_default()
            != 0
        {
            return Err(format!(
                "XDI merged DIAL {:06X} is shared in a retained SCEN slot",
                extra.local
            ));
        }
        if let Some(previous) = merged_topic_parents.insert(extra.local, anchor) {
            if previous != anchor {
                return Err(format!(
                    "DIAL {:06X} has conflicting XDI merge parents {previous:06X} and {anchor:06X}",
                    extra.local
                ));
            }
            continue;
        }
        for &info in infos_by_parent.get(&extra.local).into_iter().flatten() {
            if let Some(previous) = plan.info_parent_overrides.insert(info, anchor)
                && previous != anchor
            {
                return Err(format!(
                    "INFO {info:06X} has conflicting XDI merge parents {previous:06X} and {anchor:06X}"
                ));
            }
            anchor_count = anchor_count
                .checked_add(1)
                .ok_or_else(|| format!("DIAL {anchor:06X} INFO count overflow"))?;
        }
        plan.dial_info_count_overrides.insert(extra.local, 0);
    }
    plan.dial_info_count_overrides.insert(anchor, anchor_count);
    Ok(())
}

pub(crate) fn apply_xdi_dial_info_count(record: &mut Record, count: u32) -> Result<(), String> {
    let field = record
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"TIFC")
        .ok_or_else(|| format!("DIAL {:06X} has no TIFC field", record.form_key.local))?;
    crate::record::write_u32_field(&mut field.value, count);
    Ok(())
}

pub(crate) fn apply_xdi_scene_player_padding(record: &mut Record, filler: FormKey) {
    if record.sig.0 != *b"SCEN" {
        return;
    }

    let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
    let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
    let mut action_row = Vec::new();
    let mut in_action_row = false;
    for entry in old_fields {
        if entry.sig.0 == *b"ANAM" {
            if in_action_row {
                push_xdi_padded_scen_action(action_row, filler, &mut retained);
                action_row = Vec::new();
            }
            in_action_row = true;
        }
        if in_action_row {
            action_row.push(entry);
        } else {
            retained.push(entry);
        }
    }
    if in_action_row {
        push_xdi_padded_scen_action(action_row, filler, &mut retained);
    }
    record.fields = retained;
}

pub(super) fn push_xdi_padded_scen_action(
    mut row: Vec<FieldEntry>,
    filler: FormKey,
    retained: &mut smallvec::SmallVec<[FieldEntry; 8]>,
) {
    let player_topic_count = row
        .iter()
        .filter(|field| SCEN_PLAYER_RESPONSE_SIGS.contains(&field.sig.0))
        .count();
    if player_topic_count == 0 || player_topic_count >= SCEN_PLAYER_RESPONSE_SIGS.len() {
        retained.extend(row);
        return;
    }

    let insert_at = row
        .iter()
        .position(|field| SCEN_NPC_RESPONSE_SIGS.contains(&field.sig.0) || field.sig.0 == *b"DTGT")
        .unwrap_or(row.len());
    let tail = row.split_off(insert_at);
    retained.extend(row);
    for sig in SCEN_PLAYER_RESPONSE_SIGS.iter().skip(player_topic_count) {
        retained.push(FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::FormKey(filler),
        });
    }
    retained.extend(tail);
}

pub(super) fn push_scen_action_with_fo4_choices(
    source_plugin: crate::sym::Sym,
    row: Vec<FieldEntry>,
    retained: &mut smallvec::SmallVec<[FieldEntry; 8]>,
) {
    let mut stripped: Vec<FieldEntry> = Vec::with_capacity(row.len());
    let mut player_topics: Vec<FieldValue> = Vec::new();
    let mut npc_topics: Vec<FieldValue> = Vec::new();

    for entry in row {
        match &entry.sig.0 {
            b"ESCS" => {
                if let Some(value) = source_form_key_value(&entry.value, source_plugin) {
                    player_topics.push(value);
                }
            }
            b"ESCE" => {
                if let Some(value) = source_form_key_value(&entry.value, source_plugin) {
                    npc_topics.push(value);
                }
            }
            _ => stripped.push(entry),
        }
    }

    if player_topics.is_empty() {
        retained.extend(stripped);
        return;
    }

    player_topics.truncate(SCEN_PLAYER_RESPONSE_SIGS.len());
    npc_topics.truncate(SCEN_NPC_RESPONSE_SIGS.len());

    let insert_at = stripped
        .iter()
        .position(|entry| entry.sig.0 == *b"DTGT")
        .unwrap_or(stripped.len());

    for entry in stripped.drain(..insert_at) {
        retained.push(entry);
    }
    for (index, value) in player_topics.into_iter().enumerate() {
        retained.push(FieldEntry {
            sig: SubrecordSig(SCEN_PLAYER_RESPONSE_SIGS[index]),
            value,
        });
    }
    for (index, value) in npc_topics.into_iter().enumerate() {
        retained.push(FieldEntry {
            sig: SubrecordSig(SCEN_NPC_RESPONSE_SIGS[index]),
            value,
        });
    }
    retained.extend(stripped);
}
impl Fo76Fo4Hook {
    pub(super) fn normalize_info_response_flags(record: &mut Record) {
        if record.sig.0 != *b"INFO" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 == *b"ENAM" {
                translate_info_unknown_17_to_say_once(&mut entry.value);
            }
        }
    }

    pub(super) fn normalize_scen_headtracking_aliases(record: &mut Record) {
        if record.sig.0 != *b"SCEN" {
            return;
        }

        let mut saw_topic = false;
        let mut saw_looping_max = false;
        for entry in &mut record.fields {
            match &entry.sig.0 {
                b"ANAM" => {
                    saw_topic = false;
                    saw_looping_max = false;
                }
                b"DATA" => saw_topic = true,
                b"DMAX" => saw_looping_max = true,
                b"HTID" if !(saw_topic && !saw_looping_max) => {
                    if let FieldValue::FormKey(form_key) = entry.value {
                        entry.value = FieldValue::Bytes(SmallVec::from_slice(
                            &(form_key.local & 0x00FF_FFFF).to_le_bytes(),
                        ));
                    }
                }
                _ => {}
            }
        }
    }

    pub(super) fn normalize_scen_player_dialogue_choices(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"SCEN" {
            return;
        }

        let actions = scen_dialogue_actions(record);
        let xdi_enabled = scene_actions_require_xdi(&actions);
        let source_plugin = record.form_key.plugin;
        let old_fields: Vec<FieldEntry> = record.fields.drain(..).collect();
        let mut retained: smallvec::SmallVec<[FieldEntry; 8]> = smallvec::SmallVec::new();
        let mut action_row: Vec<FieldEntry> = Vec::new();
        let mut in_action_row = false;

        for entry in old_fields {
            if entry.sig.0 == *b"ANAM" {
                if in_action_row {
                    push_scen_action_with_fo4_choices(source_plugin, action_row, &mut retained);
                    action_row = Vec::new();
                }
                in_action_row = true;
            }

            if in_action_row {
                action_row.push(entry);
            } else {
                retained.push(entry);
            }
        }

        if in_action_row {
            push_scen_action_with_fo4_choices(source_plugin, action_row, &mut retained);
        }

        record.fields = retained;
        if xdi_enabled {
            ensure_xdi_scene_keyword(interner, record);
        }
    }

    pub(crate) fn normalize_dial_data_category(
        interner: &crate::sym::StringInterner,
        record: &mut Record,
    ) {
        if record.sig.0 != *b"DIAL" {
            return;
        }
        for entry in &mut record.fields {
            if entry.sig.0 != *b"DATA" {
                continue;
            }
            Self::normalize_dial_data_category_value(interner, &mut entry.value);
        }
    }

    pub(super) fn normalize_dial_data_category_value(
        interner: &crate::sym::StringInterner,
        value: &mut FieldValue,
    ) {
        match value {
            FieldValue::Bytes(bytes) if bytes.len() >= 2 => {
                bytes[1] = fo76_dial_category_to_fo4(bytes[1]);
            }
            FieldValue::Struct(fields) => {
                for (name, field_value) in fields {
                    if Self::struct_field_name_is(interner, *name, "category") {
                        normalize_u8_field_value(field_value, fo76_dial_category_to_fo4);
                        return;
                    }
                }
            }
            _ => {}
        }
    }
}
