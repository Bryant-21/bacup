use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const FO76_CAPS_LOCAL_ID: u32 = 0x00000F;
const COMPLETE_QUEST_FLAG: u8 = 0x01;
const REWARD_SCRIPT_NAME: &str = "B21:QuestRewards";
const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;
const FO76_BOOK_IS_RECIPE_FLAG: u8 = 0x20;
const MAX_REWARD_LIST_DEPTH: usize = 32;
const EXCLUDED_REWARD_POLICY: &str = "completion_xp_via_xnam,reputation,non_caps_currency,score,entitlement,random,chained,team_alternatives,recipe_or_crafting";

pub struct RepairQuestCompletionRewardsFixup;

#[derive(Clone, Debug, PartialEq, Eq)]
struct SourceRewardPayload {
    stage: i32,
    completes_quest: bool,
    xp_global: Option<FormKey>,
    caps_item: Option<FormKey>,
    caps_amount: Option<FormKey>,
    items: Vec<(FormKey, i32)>,
    excluded: ExcludedRewardServices,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TargetRewardPayload {
    xp: Vec<(i32, FormKey)>,
    caps_item: Option<FormKey>,
    caps: Vec<(i32, FormKey)>,
    items: Vec<(i32, FormKey, i32)>,
}

impl TargetRewardPayload {
    fn is_empty(&self) -> bool {
        self.xp.is_empty() && self.caps.is_empty() && self.items.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RewardStage {
    stage: i32,
    completes_quest: bool,
    rewards: Vec<FormKey>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct ExcludedRewardServices {
    xp: bool,
    non_caps_currency: bool,
    score_or_entitlement: bool,
    random: bool,
    chained: bool,
    team_alternatives: bool,
}

impl ExcludedRewardServices {
    fn merge(&mut self, other: Self) {
        self.xp |= other.xp;
        self.non_caps_currency |= other.non_caps_currency;
        self.score_or_entitlement |= other.score_or_entitlement;
        self.random |= other.random;
        self.chained |= other.chained;
        self.team_alternatives |= other.team_alternatives;
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AttachResult {
    Changed,
    AlreadyPresent,
    Conflict(&'static str),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RewardItemDisposition {
    Grantable,
    Recipe,
    Unresolved,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RewardItemFilterStats {
    recipes: u32,
    unresolved: u32,
}

impl Fixup for RepairQuestCompletionRewardsFixup {
    fn name(&self) -> &'static str {
        "repair_quest_completion_rewards"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session.source_id().is_some()
            && session
                .source_schema()
                .is_ok_and(|schema| schema.record_def("GMRW").is_some())
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let source_schema = session
            .source_schema()
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let qust_sig = SigCode::from_str("QUST")
            .map_err(|error| FixupError::SchemaError(error.to_string()))?;
        let source_slot = session
            .source_slot_opt()
            .ok_or_else(|| FixupError::HandleError("source plugin missing".into()))?;
        let source_masters = source_slot.parsed.header.masters.clone();
        let source_plugin_name = source_slot.parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();

        let target_quests = session
            .form_keys_of_sig(qust_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let target_quest_set = target_quests.iter().copied().collect::<FxHashSet<_>>();
        let source_by_target = mapper
            .source_to_target_iter()
            .filter(|(_, target)| target_quest_set.contains(target))
            .map(|(source, target)| (target, source))
            .collect::<FxHashMap<_, _>>();

        let mut attached = 0u32;
        let mut already_present = 0u32;
        let mut unresolved = 0u32;
        let mut server_only = 0u32;
        let mut policy_only = 0u32;
        let mut excluded = ExcludedRewardServices::default();
        let mut excluded_recipe_items = 0u32;
        let mut unresolved_items = 0u32;
        let mut item_disposition_cache = FxHashMap::default();
        for target_quest in target_quests {
            let Some(source_quest) = source_by_target.get(&target_quest).copied() else {
                continue;
            };
            let source_record = match session.source_record_decoded(
                &source_quest,
                source_schema.as_ref(),
                mapper.interner,
            ) {
                Ok(record) => record,
                Err(error) => {
                    warn(
                        &mut report,
                        mapper.interner,
                        target_quest,
                        &format!("source_quest_read:{error}"),
                    );
                    unresolved += 1;
                    continue;
                }
            };
            let stage_rewards = reward_stages(
                &source_record,
                &source_masters,
                &source_plugin_name,
                mapper.interner,
            );
            if stage_rewards.is_empty() {
                continue;
            }

            let mut source_payloads = Vec::new();
            let mut seen_stage_rewards = FxHashSet::default();
            let mut unreadable_reward = false;
            for stage_reward in stage_rewards {
                for reward_ref in stage_reward.rewards {
                    if !seen_stage_rewards.insert((stage_reward.stage, reward_ref)) {
                        continue;
                    }
                    let Ok(reward_record) = session.source_record_decoded(
                        &reward_ref,
                        source_schema.as_ref(),
                        mapper.interner,
                    ) else {
                        unreadable_reward = true;
                        continue;
                    };
                    if let Some(payload) = first_reward_group(
                        &reward_record,
                        stage_reward.stage,
                        stage_reward.completes_quest,
                        &source_masters,
                        &source_plugin_name,
                        mapper.interner,
                    ) {
                        excluded.merge(payload.excluded);
                        source_payloads.push(payload);
                    }
                }
            }
            if unreadable_reward {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    target_quest,
                    "reward_payload_unreadable",
                );
            }
            let mut load_source_record = |form_key| {
                session
                    .source_record_decoded(&form_key, source_schema.as_ref(), mapper.interner)
                    .ok()
            };
            let item_filter_stats = filter_source_reward_items(
                &mut source_payloads,
                &source_masters,
                &source_plugin_name,
                mapper.interner,
                &mut load_source_record,
                &mut item_disposition_cache,
            );
            excluded_recipe_items += item_filter_stats.recipes;
            unresolved_items += item_filter_stats.unresolved;
            if item_filter_stats.unresolved > 0 {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    target_quest,
                    "reward_item_type_unresolved",
                );
            }
            let has_local_source_reward = source_payloads.iter().any(|payload| {
                (!payload.completes_quest && payload.xp_global.is_some())
                    || (payload.caps_item.is_some() && payload.caps_amount.is_some())
                    || !payload.items.is_empty()
            });
            if !has_local_source_reward {
                if !unreadable_reward && item_filter_stats.unresolved == 0 {
                    if item_filter_stats.recipes > 0 {
                        policy_only += 1;
                    } else {
                        server_only += 1;
                    }
                }
                continue;
            }

            let target_payload = map_reward_payloads(source_payloads, mapper);
            if target_payload.is_empty() {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    target_quest,
                    "reward_payload_unmapped",
                );
                continue;
            }
            let Some(script_vmad) = reward_script_vmad(
                &target_payload,
                &target_masters,
                &target_plugin_name,
                mapper.interner,
            ) else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    target_quest,
                    "reward_vmad_encode_failed",
                );
                continue;
            };

            let mut outcome = None;
            let visits = session
                .patch_all_subrecords_bytes(&target_quest, "VMAD", |bytes| {
                    if outcome.is_some() {
                        outcome = Some(AttachResult::Conflict("multiple_vmad_subrecords"));
                        return false;
                    }
                    let result = attach_quest_script(bytes, REWARD_SCRIPT_NAME, &script_vmad);
                    outcome = Some(result);
                    result == AttachResult::Changed
                })
                .map_err(|error| FixupError::HandleError(error.to_string()))?;

            match outcome {
                Some(AttachResult::Changed) if visits == 1 => {
                    report.records_changed += 1;
                    attached += 1;
                }
                Some(AttachResult::AlreadyPresent) => already_present += 1,
                Some(AttachResult::Conflict(reason)) => {
                    unresolved += 1;
                    warn(&mut report, mapper.interner, target_quest, reason);
                }
                _ => {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        target_quest,
                        "quest_vmad_missing",
                    );
                }
            }
        }

        report.message = Some(mapper.interner.intern(&format!(
            "quest_completion_rewards:attached={attached};present={already_present};server_only={server_only};policy_only={policy_only};unresolved={unresolved};excluded_policy={EXCLUDED_REWARD_POLICY};excluded_seen=xp:{},non_caps_currency:{},score_or_entitlement:{},random:{},chained:{},team_alternatives:{},recipe_items:{excluded_recipe_items},unresolved_items:{unresolved_items}",
            excluded.xp,
            excluded.non_caps_currency,
            excluded.score_or_entitlement,
            excluded.random,
            excluded.chained,
            excluded.team_alternatives,
        )));
        Ok(report)
    }
}

fn warn(report: &mut FixupReport, interner: &StringInterner, quest: FormKey, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "repair_quest_completion_rewards:{:06X}:{reason}",
        quest.local
    )));
}

fn reward_stages(
    record: &Record,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<RewardStage> {
    let mut current_stage = None;
    let mut current_stage_completes = false;
    let mut stages = Vec::<RewardStage>::new();
    for entry in &record.fields {
        match &entry.sig.0 {
            b"INDX" => {
                current_stage = field_value_stage_index(&entry.value);
                current_stage_completes = false;
            }
            b"QSDT" if current_stage.is_some() => {
                current_stage_completes = field_value_u8(&entry.value)
                    .is_some_and(|flags| flags & COMPLETE_QUEST_FLAG != 0);
            }
            b"QRWD" => {
                let Some(stage) = current_stage else {
                    continue;
                };
                let Some(reward) =
                    source_form_key(&entry.value, source_masters, source_plugin_name, interner)
                else {
                    continue;
                };
                if reward.local == 0 {
                    continue;
                }
                if let Some(existing) = stages.iter_mut().find(|entry| entry.stage == stage) {
                    existing.completes_quest |= current_stage_completes;
                    if !existing.rewards.contains(&reward) {
                        existing.rewards.push(reward);
                    }
                } else {
                    stages.push(RewardStage {
                        stage,
                        completes_quest: current_stage_completes,
                        rewards: vec![reward],
                    });
                }
            }
            _ => {}
        }
    }
    stages
}

fn first_reward_group(
    record: &Record,
    stage: i32,
    completes_quest: bool,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<SourceRewardPayload> {
    if record.sig.0 != *b"GMRW" {
        return None;
    }
    let mut caps_item = None;
    let mut caps_amount = None;
    let mut xp_global = None;
    let mut items = Vec::new();
    let mut excluded = ExcludedRewardServices::default();
    let mut groups = Vec::<Vec<&FieldEntry>>::new();
    let mut current_group = Vec::<&FieldEntry>::new();
    for entry in &record.fields {
        if entry.sig.0 == *b"ITME" {
            if !current_group.is_empty() {
                groups.push(std::mem::take(&mut current_group));
            }
            continue;
        }
        if is_reward_group_field(&entry.sig.0) {
            current_group.push(entry);
        }
    }
    if !current_group.is_empty() {
        groups.push(current_group);
    }
    let first_conditions = groups.first().map(|group| reward_group_conditions(group))?;
    for (group_index, group) in groups.into_iter().enumerate() {
        let group_conditions = reward_group_conditions(&group);
        let same_local_recipient = group_index == 0
            || (!first_conditions.is_empty() && group_conditions == first_conditions);
        if !same_local_recipient {
            excluded.team_alternatives = true;
            continue;
        }
        let mut currency = None;
        let mut currency_amount = None;
        for entry in group {
            match &entry.sig.0 {
                b"NAM7" => {
                    xp_global =
                        source_form_key(&entry.value, source_masters, source_plugin_name, interner);
                }
                b"XPCT" | b"ESRE" => excluded.xp = true,
                b"QRCO" => {
                    currency =
                        source_form_key(&entry.value, source_masters, source_plugin_name, interner);
                    if currency
                        .is_some_and(|form| !is_source_caps(form, source_plugin_name, interner))
                    {
                        excluded.non_caps_currency = true;
                    }
                }
                b"NAM8" => {
                    currency_amount =
                        source_form_key(&entry.value, source_masters, source_plugin_name, interner);
                }
                b"QSRD" => {
                    items.extend(source_reward_items(
                        &entry.value,
                        source_masters,
                        source_plugin_name,
                        interner,
                    ));
                }
                b"QRLI" | b"QRRI" => excluded.random = true,
                b"QRCX" | b"CENT" | b"NAM9" => excluded.score_or_entitlement = true,
                b"DNAM" => excluded.chained = true,
                _ => {}
            }
        }
        if currency.is_some_and(|form| is_source_caps(form, source_plugin_name, interner))
            && let Some(amount) = currency_amount
        {
            caps_item = currency;
            caps_amount = Some(amount);
        }
    }
    if caps_item.is_none() {
        caps_amount = None;
    }
    if completes_quest && xp_global.is_some() {
        excluded.xp = true;
    }
    Some(SourceRewardPayload {
        stage,
        completes_quest,
        xp_global,
        caps_item,
        caps_amount,
        items,
        excluded,
    })
}

fn reward_group_conditions(group: &[&FieldEntry]) -> Vec<FieldEntry> {
    group
        .iter()
        .filter(|entry| matches!(&entry.sig.0, b"CTDA" | b"CIS1" | b"CIS2"))
        .map(|entry| (*entry).clone())
        .collect()
}

fn is_reward_group_field(sig: &[u8; 4]) -> bool {
    matches!(
        sig,
        b"CTRG"
            | b"NAM7"
            | b"XPCT"
            | b"NAM8"
            | b"QRCO"
            | b"NAM9"
            | b"QRLI"
            | b"QRCX"
            | b"ESRE"
            | b"QRLR"
            | b"QRRI"
            | b"QSRD"
            | b"CENT"
            | b"CTDA"
            | b"CIS1"
            | b"CIS2"
            | b"DNAM"
    )
}

fn is_source_caps(form: FormKey, source_plugin_name: &str, interner: &StringInterner) -> bool {
    form.local == FO76_CAPS_LOCAL_ID && interner.resolve(form.plugin) == Some(source_plugin_name)
}

fn source_reward_items(
    value: &FieldValue,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Vec<(FormKey, i32)> {
    match value {
        FieldValue::Bytes(bytes) => bytes
            .chunks_exact(8)
            .filter_map(|row| {
                let raw = u32::from_le_bytes(row[0..4].try_into().ok()?);
                let count = i32::try_from(u32::from_le_bytes(row[4..8].try_into().ok()?)).ok()?;
                (count > 0)
                    .then(|| {
                        source_raw_form_key(raw, source_masters, source_plugin_name, interner)
                            .map(|item| (item, count))
                    })
                    .flatten()
            })
            .collect(),
        FieldValue::Struct(fields) if fields.len() >= 2 => {
            let item = source_form_key(&fields[0].1, source_masters, source_plugin_name, interner);
            let count = field_value_i32(&fields[1].1);
            match (item, count) {
                (Some(item), Some(count)) if count > 0 => vec![(item, count)],
                _ => Vec::new(),
            }
        }
        FieldValue::List(values) => values
            .iter()
            .flat_map(|value| {
                source_reward_items(value, source_masters, source_plugin_name, interner)
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn filter_source_reward_items(
    payloads: &mut [SourceRewardPayload],
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
    load_source_record: &mut impl FnMut(FormKey) -> Option<Record>,
    cache: &mut FxHashMap<FormKey, RewardItemDisposition>,
) -> RewardItemFilterStats {
    let mut stats = RewardItemFilterStats::default();
    for payload in payloads {
        payload.items.retain(|(form_key, _)| {
            let mut visiting = FxHashSet::default();
            match classify_source_reward_item(
                *form_key,
                source_masters,
                source_plugin_name,
                interner,
                load_source_record,
                cache,
                &mut visiting,
                0,
            ) {
                RewardItemDisposition::Grantable => true,
                RewardItemDisposition::Recipe => {
                    stats.recipes = stats.recipes.saturating_add(1);
                    false
                }
                RewardItemDisposition::Unresolved => {
                    stats.unresolved = stats.unresolved.saturating_add(1);
                    false
                }
            }
        });
    }
    stats
}

fn classify_source_reward_item(
    form_key: FormKey,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
    load_source_record: &mut impl FnMut(FormKey) -> Option<Record>,
    cache: &mut FxHashMap<FormKey, RewardItemDisposition>,
    visiting: &mut FxHashSet<FormKey>,
    depth: usize,
) -> RewardItemDisposition {
    if let Some(disposition) = cache.get(&form_key) {
        return *disposition;
    }
    if depth > MAX_REWARD_LIST_DEPTH || !visiting.insert(form_key) {
        return RewardItemDisposition::Unresolved;
    }

    let disposition = match load_source_record(form_key) {
        None => RewardItemDisposition::Unresolved,
        Some(record) if record.sig.0 == *b"BOOK" => {
            match source_book_is_recipe(&record, interner) {
                Some(true) => RewardItemDisposition::Recipe,
                Some(false) => RewardItemDisposition::Grantable,
                None => RewardItemDisposition::Unresolved,
            }
        }
        Some(record) if record.sig.0 == *b"LVLI" => {
            match source_leveled_item_members(&record, source_masters, source_plugin_name, interner)
            {
                Some(members) if !members.is_empty() => {
                    let mut result = RewardItemDisposition::Grantable;
                    for member in members {
                        match classify_source_reward_item(
                            member,
                            source_masters,
                            source_plugin_name,
                            interner,
                            load_source_record,
                            cache,
                            visiting,
                            depth + 1,
                        ) {
                            RewardItemDisposition::Recipe => {
                                if result == RewardItemDisposition::Grantable {
                                    result = RewardItemDisposition::Recipe;
                                }
                            }
                            RewardItemDisposition::Unresolved => {
                                result = RewardItemDisposition::Unresolved;
                            }
                            RewardItemDisposition::Grantable => {}
                        }
                    }
                    result
                }
                _ => RewardItemDisposition::Unresolved,
            }
        }
        Some(_) => RewardItemDisposition::Grantable,
    };

    visiting.remove(&form_key);
    if disposition != RewardItemDisposition::Unresolved {
        cache.insert(form_key, disposition);
    }
    disposition
}

pub(crate) fn source_book_is_recipe(record: &Record, interner: &StringInterner) -> Option<bool> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.0 != *b"DNAM" {
            return None;
        }
        match &entry.value {
            FieldValue::Bytes(bytes) => bytes
                .first()
                .map(|flags| flags & FO76_BOOK_IS_RECIPE_FLAG != 0),
            FieldValue::Struct(fields) => fields.iter().find_map(|(name, value)| {
                if !interner
                    .resolve(*name)
                    .is_some_and(|name| name.eq_ignore_ascii_case("Flags"))
                {
                    return None;
                }
                let FieldValue::List(flags) = value else {
                    return None;
                };
                Some(flags.iter().any(|flag| {
                    matches!(flag, FieldValue::String(name) if interner
                        .resolve(*name)
                        .is_some_and(|name| name.eq_ignore_ascii_case("IsRecipe")))
                }))
            }),
            _ => None,
        }
    })
}

fn source_leveled_item_members(
    record: &Record,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<Vec<FormKey>> {
    let mut members = Vec::new();
    let mut saw_entry = false;
    for entry in &record.fields {
        if !matches!(&entry.sig.0, b"LVLO" | b"LVLE") {
            continue;
        }
        saw_entry = true;
        let before = members.len();
        match &entry.value {
            FieldValue::Bytes(bytes) => {
                let raw = u32::from_le_bytes(bytes.get(4..8)?.try_into().ok()?);
                members.push(source_raw_form_key(
                    raw,
                    source_masters,
                    source_plugin_name,
                    interner,
                )?);
            }
            value => collect_nested_form_keys(value, &mut members),
        }
        if members.len() == before {
            return None;
        }
    }
    saw_entry.then_some(members)
}

fn collect_nested_form_keys(value: &FieldValue, output: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) if form_key.local != 0 => output.push(*form_key),
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_nested_form_keys(value, output);
            }
        }
        FieldValue::List(values) => {
            for value in values {
                collect_nested_form_keys(value, output);
            }
        }
        _ => {}
    }
}

fn map_reward_payloads(
    source: Vec<SourceRewardPayload>,
    mapper: &FormKeyMapper,
) -> TargetRewardPayload {
    let mut target = TargetRewardPayload {
        xp: Vec::new(),
        caps_item: None,
        caps: Vec::new(),
        items: Vec::new(),
    };
    for payload in source {
        if !payload.completes_quest
            && let Some(mapped_xp) = payload.xp_global.and_then(|xp| mapper.lookup(xp))
            && !target.xp.contains(&(payload.stage, mapped_xp))
        {
            target.xp.push((payload.stage, mapped_xp));
        }
        if let (Some(caps_item), Some(caps_amount)) = (payload.caps_item, payload.caps_amount)
            && let (Some(mapped_item), Some(mapped_amount)) =
                (mapper.lookup(caps_item), mapper.lookup(caps_amount))
        {
            target.caps_item.get_or_insert(mapped_item);
            if target.caps_item == Some(mapped_item) {
                target.caps.push((payload.stage, mapped_amount));
            }
        }
        target
            .items
            .extend(payload.items.into_iter().filter_map(|(form, count)| {
                mapper
                    .lookup(form)
                    .map(|mapped| (payload.stage, mapped, count))
            }));
    }
    target
}

fn reward_script_vmad(
    payload: &TargetRewardPayload,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let mut properties = Vec::new();
    if !payload.xp.is_empty() {
        properties.push(serde_json::json!({
            "propertyName": "XPStages",
            "Type": "Array of Int32",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": payload.xp.iter().map(|(stage, _)| *stage).collect::<Vec<_>>(),
        }));
        properties.push(serde_json::json!({
            "propertyName": "RewardXP",
            "Type": "Array of Object",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": payload
                .xp
                .iter()
                .map(|(_, amount)| object_value(*amount, interner))
                .collect::<Option<Vec<_>>>()?,
        }));
    }
    if !payload.caps.is_empty() {
        let caps_item = payload.caps_item?;
        properties.push(serde_json::json!({
            "propertyName": "CapsStages",
            "Type": "Array of Int32",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": payload.caps.iter().map(|(stage, _)| *stage).collect::<Vec<_>>(),
        }));
        properties.push(serde_json::json!({
            "propertyName": "RewardCaps",
            "Type": "Array of Object",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": payload
                .caps
                .iter()
                .map(|(_, amount)| object_value(*amount, interner))
                .collect::<Option<Vec<_>>>()?,
        }));
        properties.push(object_property("CapsItem", caps_item, interner)?);
    }
    if !payload.items.is_empty() {
        properties.push(serde_json::json!({
            "propertyName": "ItemStages",
            "Type": "Array of Int32",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": payload.items.iter().map(|(stage, _, _)| *stage).collect::<Vec<_>>(),
        }));
        let item_values = payload
            .items
            .iter()
            .map(|(_, form, _)| object_value(*form, interner))
            .collect::<Option<Vec<_>>>()?;
        let counts = payload
            .items
            .iter()
            .map(|(_, _, count)| *count)
            .collect::<Vec<_>>();
        properties.push(serde_json::json!({
            "propertyName": "RewardItems",
            "Type": "Array of Object",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": item_values,
        }));
        properties.push(serde_json::json!({
            "propertyName": "RewardCounts",
            "Type": "Array of Int32",
            "Flags": VMAD_PROPERTY_FLAG_EDITED,
            "Value": counts,
        }));
    }
    if properties.is_empty() {
        return None;
    }
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": REWARD_SCRIPT_NAME,
                "Flags": 0,
                "Properties": properties,
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn object_property(
    name: &str,
    form_key: FormKey,
    interner: &StringInterner,
) -> Option<serde_json::Value> {
    Some(serde_json::json!({
        "propertyName": name,
        "Type": "Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": object_value(form_key, interner)?,
    }))
}

fn object_value(form_key: FormKey, interner: &StringInterner) -> Option<serde_json::Value> {
    let plugin = interner.resolve(form_key.plugin)?;
    Some(serde_json::json!({
        "Alias": -1,
        "FormID": {
            "reference": {
                "plugin": plugin,
                "object_id": format!("{:06X}", form_key.local),
            },
        },
    }))
}

struct VmadReader<'a> {
    bytes: &'a [u8],
    offset: usize,
}

impl<'a> VmadReader<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, offset: 0 }
    }

    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let end = self.offset.checked_add(count)?;
        let value = self.bytes.get(self.offset..end)?;
        self.offset = end;
        Some(value)
    }

    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|bytes| bytes[0])
    }

    fn u16(&mut self) -> Option<u16> {
        Some(u16::from_le_bytes(self.take(2)?.try_into().ok()?))
    }

    fn i32(&mut self) -> Option<i32> {
        Some(i32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn string_bytes(&mut self) -> Option<&'a [u8]> {
        let length = self.u16()? as usize;
        self.take(length)
    }

    fn nonnegative_count(&mut self) -> Option<usize> {
        usize::try_from(self.i32()?).ok()
    }
}

fn skip_struct(reader: &mut VmadReader<'_>, object_format: u16) -> Option<()> {
    let count = reader.nonnegative_count()?;
    for _ in 0..count {
        reader.string_bytes()?;
        let property_type = reader.u8()?;
        reader.u8()?;
        skip_property_value(reader, property_type, object_format)?;
    }
    Some(())
}

fn skip_property_value(
    reader: &mut VmadReader<'_>,
    property_type: u8,
    object_format: u16,
) -> Option<()> {
    match property_type {
        0 | 6 => Some(()),
        1 => {
            reader.take(8)?;
            Some(())
        }
        2 => {
            reader.string_bytes()?;
            Some(())
        }
        3 | 4 => {
            reader.take(4)?;
            Some(())
        }
        5 => {
            reader.take(1)?;
            Some(())
        }
        7 => skip_struct(reader, object_format),
        11 => {
            let count = reader.nonnegative_count()?;
            reader.take(count.checked_mul(8)?)?;
            Some(())
        }
        12 => {
            let count = reader.nonnegative_count()?;
            for _ in 0..count {
                reader.string_bytes()?;
            }
            Some(())
        }
        13 | 14 => {
            let count = reader.nonnegative_count()?;
            reader.take(count.checked_mul(4)?)?;
            Some(())
        }
        15 => {
            let count = reader.nonnegative_count()?;
            reader.take(count)?;
            Some(())
        }
        16 => {
            reader.take(4)?;
            Some(())
        }
        17 => {
            let count = reader.nonnegative_count()?;
            for _ in 0..count {
                skip_struct(reader, object_format)?;
            }
            Some(())
        }
        _ => None,
    }
}

fn top_level_script_layout<'a>(
    bytes: &'a [u8],
    script_name: &str,
) -> Option<(u16, usize, Vec<&'a [u8]>)> {
    let mut reader = VmadReader::new(bytes);
    let version = reader.u16()?;
    let object_format = reader.u16()?;
    if version != VMAD_VERSION || object_format != VMAD_OBJECT_FORMAT {
        return None;
    }
    let script_count = reader.u16()?;
    let mut matching = Vec::new();
    for _ in 0..script_count {
        let start = reader.offset;
        let name = reader.string_bytes()?;
        reader.u8()?;
        let property_count = reader.u16()?;
        for _ in 0..property_count {
            reader.string_bytes()?;
            let property_type = reader.u8()?;
            reader.u8()?;
            skip_property_value(&mut reader, property_type, object_format)?;
        }
        if name == script_name.as_bytes() {
            matching.push(&bytes[start..reader.offset]);
        }
    }
    Some((script_count, reader.offset, matching))
}

fn attach_quest_script(
    existing: &mut Vec<u8>,
    script_name: &str,
    script_vmad: &[u8],
) -> AttachResult {
    if script_vmad.len() < 6 {
        return AttachResult::Conflict("generated_vmad_invalid");
    }
    let Some((script_count, insert_at, matching)) = top_level_script_layout(existing, script_name)
    else {
        return AttachResult::Conflict("unsupported_or_malformed_vmad");
    };
    match matching.as_slice() {
        [] => {}
        [existing_script] if *existing_script == &script_vmad[6..] => {
            return AttachResult::AlreadyPresent;
        }
        [_] => return AttachResult::Conflict("same_script_different_binding"),
        _ => return AttachResult::Conflict("duplicate_script_binding"),
    }
    let Some(next_count) = script_count.checked_add(1) else {
        return AttachResult::Conflict("script_count_overflow");
    };
    existing[4..6].copy_from_slice(&next_count.to_le_bytes());
    existing.splice(insert_at..insert_at, script_vmad[6..].iter().copied());
    AttachResult::Changed
}

fn source_form_key(
    value: &FieldValue,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if let FieldValue::FormKey(form_key) = value {
        return (form_key.local != 0).then_some(*form_key);
    }
    let bytes = field_bytes(value)?;
    let raw = u32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?);
    source_raw_form_key(raw, source_masters, source_plugin_name, interner)
}

fn source_raw_form_key(
    raw: u32,
    source_masters: &[String],
    source_plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let plugin = source_masters
        .get((raw >> 24) as usize)
        .map(String::as_str)
        .unwrap_or(source_plugin_name);
    Some(FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin),
    })
}

fn field_bytes(value: &FieldValue) -> Option<&[u8]> {
    match value {
        FieldValue::Bytes(bytes) => Some(bytes.as_slice()),
        _ => None,
    }
}

fn field_value_i32(value: &FieldValue) -> Option<i32> {
    match value {
        FieldValue::Uint(value) => i32::try_from(*value).ok(),
        FieldValue::Int(value) => i32::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => Some(i32::from_le_bytes(bytes.get(0..4)?.try_into().ok()?)),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| field_value_i32(value)),
        _ => None,
    }
}

fn field_value_u8(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => bytes.first().copied(),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| field_value_u8(value)),
        _ => None,
    }
}

fn field_value_stage_index(value: &FieldValue) -> Option<i32> {
    match value {
        FieldValue::Bytes(bytes) => {
            Some(u16::from_le_bytes(bytes.get(0..2)?.try_into().ok()?) as i32)
        }
        FieldValue::Struct(fields) => fields
            .first()
            .and_then(|(_, value)| field_value_stage_index(value)),
        _ => field_value_i32(value),
    }
}

#[cfg(test)]
mod tests {
    use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
    use smallvec::{SmallVec, smallvec};

    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("SeventySix.esm"),
        }
    }

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    fn record(
        interner: &StringInterner,
        sig: &[u8; 4],
        local: u32,
        fields: Vec<FieldEntry>,
    ) -> Record {
        Record {
            sig: SigCode(*sig),
            form_key: fk(interner, local),
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    fn raw_form(local: u32) -> FieldValue {
        FieldValue::Bytes(SmallVec::from_slice(&local.to_le_bytes()))
    }

    fn raw_item(local: u32, count: u32) -> FieldValue {
        let mut row = Vec::new();
        row.extend_from_slice(&local.to_le_bytes());
        row.extend_from_slice(&count.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(row))
    }

    fn raw_stage(stage: u16, trailing: [u8; 2]) -> FieldValue {
        let [low, high] = stage.to_le_bytes();
        FieldValue::Bytes(SmallVec::from_slice(&[low, high, trailing[0], trailing[1]]))
    }

    fn raw_active_player_condition(comparison: f32) -> FieldValue {
        let mut bytes = [0u8; 32];
        bytes[4..8].copy_from_slice(&comparison.to_le_bytes());
        bytes[8..12].copy_from_slice(&[0x45, 0x03, 0x00, 0x00]);
        bytes[28..32].copy_from_slice(&[0xFF; 4]);
        FieldValue::Bytes(SmallVec::from_slice(&bytes))
    }

    fn reward_record(
        interner: &StringInterner,
        form: FormKey,
        xp: FormKey,
        caps: Option<(FormKey, FormKey)>,
        item: Option<(FormKey, u32)>,
    ) -> Record {
        let mut fields = vec![field(b"NAM7", raw_form(xp.local))];
        if let Some((caps_item, caps_amount)) = caps {
            fields.push(field(b"QRCO", raw_form(caps_item.local)));
            fields.push(field(b"NAM8", raw_form(caps_amount.local)));
        }
        if let Some((reward_item, count)) = item {
            fields.push(field(b"QSRD", raw_item(reward_item.local, count)));
        }
        record(interner, b"GMRW", form.local, fields)
    }

    fn reward_quest(
        interner: &StringInterner,
        local: u32,
        rows: &[(u16, bool, FormKey)],
        trailing: [u8; 2],
    ) -> Record {
        record(
            interner,
            b"QUST",
            local,
            rows.iter()
                .flat_map(|(stage, completes, reward)| {
                    [
                        field(b"INDX", raw_stage(*stage, trailing)),
                        field(b"QSDT", FieldValue::Uint(u64::from(*completes))),
                        field(b"QRWD", raw_form(reward.local)),
                    ]
                })
                .collect(),
        )
    }

    fn book_record(interner: &StringInterner, local: u32, is_recipe: bool) -> Record {
        let flags = if is_recipe {
            FO76_BOOK_IS_RECIPE_FLAG
        } else {
            0x04
        };
        let mut data = vec![0; 13];
        data[0] = flags;
        record(
            interner,
            b"BOOK",
            local,
            vec![field(b"DNAM", FieldValue::Bytes(data.into()))],
        )
    }

    fn leveled_item_record(interner: &StringInterner, local: u32, members: &[FormKey]) -> Record {
        record(
            interner,
            b"LVLI",
            local,
            members
                .iter()
                .map(|member| {
                    field(
                        b"LVLO",
                        FieldValue::Struct(vec![(
                            interner.intern("Reference"),
                            FieldValue::FormKey(*member),
                        )]),
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn reward_item_filter_excludes_recipe_books_and_recipe_bearing_lists() {
        let interner = StringInterner::new();
        let mtr02_recipe = fk(&interner, 0x4E487F);
        let nested_recipe = fk(&interner, 0x2B8BBF);
        let personal_recipe_list = fk(&interner, 0x43470C);
        let inner_recipe_list = fk(&interner, 0x434700);
        let usable_list = fk(&interner, 0x710001);
        let usable_list_member = fk(&interner, 0x017DEA);
        let usable_item = fk(&interner, 0x023736);
        let ordinary_book = fk(&interner, 0x454115);
        let unreadable_item = fk(&interner, 0x00DEAD);

        let records = FxHashMap::from_iter([
            (
                mtr02_recipe,
                book_record(&interner, mtr02_recipe.local, true),
            ),
            (
                nested_recipe,
                book_record(&interner, nested_recipe.local, true),
            ),
            (
                personal_recipe_list,
                leveled_item_record(&interner, personal_recipe_list.local, &[inner_recipe_list]),
            ),
            (
                inner_recipe_list,
                leveled_item_record(&interner, inner_recipe_list.local, &[nested_recipe]),
            ),
            (
                usable_list,
                leveled_item_record(&interner, usable_list.local, &[usable_list_member]),
            ),
            (
                usable_list_member,
                record(&interner, b"ALCH", usable_list_member.local, vec![]),
            ),
            (
                usable_item,
                record(&interner, b"ALCH", usable_item.local, vec![]),
            ),
            (
                ordinary_book,
                book_record(&interner, ordinary_book.local, false),
            ),
        ]);
        let mut payloads = vec![SourceRewardPayload {
            stage: 9000,
            completes_quest: true,
            xp_global: Some(fk(&interner, 0x109C23)),
            caps_item: Some(fk(&interner, FO76_CAPS_LOCAL_ID)),
            caps_amount: Some(fk(&interner, 0x2ABE9C)),
            items: vec![
                (mtr02_recipe, 1),
                (personal_recipe_list, 1),
                (usable_list, 1),
                (usable_item, 2),
                (ordinary_book, 1),
                (unreadable_item, 1),
            ],
            excluded: ExcludedRewardServices::default(),
        }];
        let mut load = |form_key| records.get(&form_key).cloned();
        let mut cache = FxHashMap::default();

        let stats = filter_source_reward_items(
            &mut payloads,
            &[],
            "SeventySix.esm",
            &interner,
            &mut load,
            &mut cache,
        );

        assert_eq!(
            stats,
            RewardItemFilterStats {
                recipes: 2,
                unresolved: 1,
            }
        );
        assert_eq!(
            payloads[0].items,
            vec![(usable_list, 1), (usable_item, 2), (ordinary_book, 1)]
        );
        assert_eq!(payloads[0].xp_global, Some(fk(&interner, 0x109C23)));
        assert_eq!(payloads[0].caps_amount, Some(fk(&interner, 0x2ABE9C)));
    }

    #[test]
    fn reward_item_filter_fails_closed_on_leveled_list_cycles() {
        let interner = StringInterner::new();
        let first = fk(&interner, 0x100001);
        let second = fk(&interner, 0x100002);
        let records = FxHashMap::from_iter([
            (
                first,
                leveled_item_record(&interner, first.local, &[second]),
            ),
            (
                second,
                leveled_item_record(&interner, second.local, &[first]),
            ),
        ]);
        let mut load = |form_key| records.get(&form_key).cloned();
        let mut cache = FxHashMap::default();
        let mut visiting = FxHashSet::default();

        assert_eq!(
            classify_source_reward_item(
                first,
                &[],
                "SeventySix.esm",
                &interner,
                &mut load,
                &mut cache,
                &mut visiting,
                0,
            ),
            RewardItemDisposition::Unresolved
        );
        assert!(cache.is_empty());
        assert!(visiting.is_empty());
    }

    #[test]
    fn recipe_list_with_an_unreadable_member_remains_unresolved() {
        let interner = StringInterner::new();
        let list = fk(&interner, 0x100010);
        let recipe = fk(&interner, 0x4E487F);
        let unreadable = fk(&interner, 0x100011);
        let records = FxHashMap::from_iter([
            (
                list,
                leveled_item_record(&interner, list.local, &[recipe, unreadable]),
            ),
            (recipe, book_record(&interner, recipe.local, true)),
        ]);
        let mut load = |form_key| records.get(&form_key).cloned();
        let mut cache = FxHashMap::default();
        let mut visiting = FxHashSet::default();

        assert_eq!(
            classify_source_reward_item(
                list,
                &[],
                "SeventySix.esm",
                &interner,
                &mut load,
                &mut cache,
                &mut visiting,
                0,
            ),
            RewardItemDisposition::Unresolved
        );
        assert!(!cache.contains_key(&list));
        assert!(visiting.is_empty());
    }

    #[test]
    fn reward_item_filter_fails_closed_on_excessively_deep_leveled_lists() {
        let interner = StringInterner::new();
        let forms = (0..=MAX_REWARD_LIST_DEPTH + 1)
            .map(|index| fk(&interner, 0x110000 + index as u32))
            .collect::<Vec<_>>();
        let mut records = FxHashMap::default();
        for pair in forms.windows(2) {
            records.insert(
                pair[0],
                leveled_item_record(&interner, pair[0].local, &[pair[1]]),
            );
        }
        let terminal = *forms.last().expect("deep reward list has a terminal item");
        records.insert(terminal, record(&interner, b"ALCH", terminal.local, vec![]));

        let mut load = |form_key| records.get(&form_key).cloned();
        let mut cache = FxHashMap::default();
        let mut visiting = FxHashSet::default();

        assert_eq!(
            classify_source_reward_item(
                forms[0],
                &[],
                "SeventySix.esm",
                &interner,
                &mut load,
                &mut cache,
                &mut visiting,
                0,
            ),
            RewardItemDisposition::Unresolved
        );
        assert!(cache.is_empty());
        assert!(visiting.is_empty());
        let mut visiting = FxHashSet::default();
        assert_eq!(
            classify_source_reward_item(
                forms[1],
                &[],
                "SeventySix.esm",
                &interner,
                &mut load,
                &mut cache,
                &mut visiting,
                0,
            ),
            RewardItemDisposition::Grantable
        );
    }

    #[test]
    fn source_leveled_item_members_decode_raw_reference_at_lvlo_offset_four() {
        let interner = StringInterner::new();
        let member = fk(&interner, 0x2C3EE7);
        let mut bytes = vec![1, 0, 0, 0];
        bytes.extend_from_slice(&member.local.to_le_bytes());
        bytes.extend_from_slice(&[1, 0, 0, 0]);
        let list = record(
            &interner,
            b"LVLI",
            0x331CAD,
            vec![field(b"LVLO", FieldValue::Bytes(bytes.into()))],
        );

        assert_eq!(
            source_leveled_item_members(&list, &[], "SeventySix.esm", &interner),
            Some(vec![member])
        );
    }

    #[test]
    fn reward_stages_include_midquest_and_completion_qrwd_but_ignore_dnam() {
        let interner = StringInterner::new();
        let midquest = fk(&interner, 0x100);
        let completion = fk(&interner, 0x200);
        let quest = record(
            &interner,
            b"QUST",
            1,
            vec![
                field(b"INDX", FieldValue::Uint(100)),
                field(b"QSDT", FieldValue::Uint(0)),
                field(b"QRWD", raw_form(midquest.local)),
                field(b"QRWD", raw_form(midquest.local)),
                field(b"INDX", FieldValue::Uint(9000)),
                field(b"QSDT", FieldValue::Uint(1)),
                field(b"DNAM", FieldValue::FormKey(midquest)),
                field(b"QRWD", raw_form(completion.local)),
            ],
        );

        assert_eq!(
            reward_stages(&quest, &[], "SeventySix.esm", &interner),
            vec![
                RewardStage {
                    stage: 100,
                    completes_quest: false,
                    rewards: vec![midquest],
                },
                RewardStage {
                    stage: 9000,
                    completes_quest: true,
                    rewards: vec![completion],
                },
            ]
        );
    }

    #[test]
    fn reward_stages_decode_raw_u16_indx_stage_prefix() {
        let interner = StringInterner::new();
        let reward = fk(&interner, 0x200);
        let quest = record(
            &interner,
            b"QUST",
            1,
            vec![
                field(
                    b"INDX",
                    FieldValue::Bytes(SmallVec::from_slice(&[0x64, 0x00, 0x00, 0x20])),
                ),
                field(b"QSDT", FieldValue::Uint(0)),
                field(b"QRWD", raw_form(reward.local)),
            ],
        );

        assert_eq!(
            reward_stages(&quest, &[], "SeventySix.esm", &interner),
            vec![RewardStage {
                stage: 100,
                completes_quest: false,
                rewards: vec![reward],
            }]
        );
    }

    #[test]
    fn wastelanders_midquest_reward_stages_are_not_completion_gated() {
        let interner = StringInterner::new();
        let expected = [
            (805, 0x6313B6),
            (199, 0x6313CF),
            (699, 0x6313D0),
            (801, 0x6313BD),
            (1302, 0x6313BF),
            (701, 0x6313C7),
            (5211, 0x6313C6),
        ];
        let fields = expected
            .iter()
            .flat_map(|(stage, reward)| {
                [
                    field(b"INDX", FieldValue::Uint(*stage as u64)),
                    field(b"QSDT", FieldValue::Uint(0)),
                    field(b"QRWD", raw_form(*reward)),
                ]
            })
            .collect();
        let quest = record(&interner, b"QUST", 1, fields);

        assert_eq!(
            reward_stages(&quest, &[], "SeventySix.esm", &interner),
            expected
                .iter()
                .map(|(stage, reward)| RewardStage {
                    stage: *stage,
                    completes_quest: false,
                    rewards: vec![fk(&interner, *reward)],
                })
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn field_value_i32_preserves_decoded_numeric_variants() {
        assert_eq!(field_value_i32(&FieldValue::Uint(9000)), Some(9000));
        assert_eq!(field_value_i32(&FieldValue::Int(-1)), Some(-1));
        assert_eq!(
            field_value_i32(&FieldValue::Bytes(SmallVec::from_slice(&[
                0x00, 0x00, 0x01, 0x00,
            ]))),
            Some(65536)
        );
        assert_eq!(field_value_i32(&FieldValue::Uint(u64::MAX)), None);
        assert_eq!(field_value_i32(&FieldValue::Int(i64::MAX)), None);
    }

    #[test]
    fn enclave_noncompletion_xp_rows_are_preserved_without_completion_duplicates() {
        let mut interner = StringInterner::new();
        let xp_50 = fk(&interner, 0x500001);
        let xp_1000 = fk(&interner, 0x500002);
        let completion_xp = fk(&interner, 0x500003);
        let caps_item = fk(&interner, FO76_CAPS_LOCAL_ID);
        let caps_61 = fk(&interner, 0x500004);
        let caps_300 = fk(&interner, 0x500005);
        let bunker_item = fk(&interner, 0x500006);
        let completion_item = fk(&interner, 0x500007);
        let en02_stage_361_item = fk(&interner, 0x3D5818);
        let reward_forms = [
            fk(&interner, 0x600061),
            fk(&interner, 0x600110),
            fk(&interner, 0x600250),
            fk(&interner, 0x600260),
            fk(&interner, 0x600300),
            fk(&interner, 0x600261),
            fk(&interner, 0x600361),
        ];
        let reward_records = FxHashMap::from_iter([
            (
                reward_forms[0],
                reward_record(
                    &interner,
                    reward_forms[0],
                    xp_50,
                    Some((caps_item, caps_61)),
                    None,
                ),
            ),
            (
                reward_forms[1],
                reward_record(&interner, reward_forms[1], xp_50, None, None),
            ),
            (
                reward_forms[2],
                reward_record(
                    &interner,
                    reward_forms[2],
                    xp_1000,
                    None,
                    Some((bunker_item, 2)),
                ),
            ),
            (
                reward_forms[3],
                reward_record(&interner, reward_forms[3], xp_1000, None, None),
            ),
            (
                reward_forms[4],
                reward_record(
                    &interner,
                    reward_forms[4],
                    completion_xp,
                    Some((caps_item, caps_300)),
                    Some((completion_item, 1)),
                ),
            ),
            (
                reward_forms[5],
                reward_record(&interner, reward_forms[5], xp_50, None, None),
            ),
            (
                reward_forms[6],
                reward_record(
                    &interner,
                    reward_forms[6],
                    xp_50,
                    None,
                    Some((en02_stage_361_item, 1)),
                ),
            ),
        ]);
        let bunker = reward_quest(
            &interner,
            0x0714FE,
            &[
                (61_u16, false, reward_forms[0]),
                (110, false, reward_forms[1]),
                (250, false, reward_forms[2]),
                (260, false, reward_forms[3]),
                (300, true, reward_forms[4]),
            ],
            [0xA5, 0x5A],
        );
        let en02 = reward_quest(
            &interner,
            0x0293A3,
            &[
                (261_u16, false, reward_forms[5]),
                (361, false, reward_forms[6]),
            ],
            [0xC3, 0x3C],
        );
        let mut source_payloads = Vec::new();
        for quest in [&bunker, &en02] {
            for stage_reward in reward_stages(quest, &[], "SeventySix.esm", &interner) {
                for reward in stage_reward.rewards {
                    source_payloads.push(
                        first_reward_group(
                            &reward_records[&reward],
                            stage_reward.stage,
                            stage_reward.completes_quest,
                            &[],
                            "SeventySix.esm",
                            &interner,
                        )
                        .unwrap(),
                    );
                }
            }
        }
        source_payloads.push(SourceRewardPayload {
            stage: 61,
            completes_quest: false,
            xp_global: Some(xp_50),
            caps_item: None,
            caps_amount: None,
            items: Vec::new(),
            excluded: ExcludedRewardServices::default(),
        });

        let mapped_forms = [
            xp_50,
            xp_1000,
            completion_xp,
            caps_item,
            caps_61,
            caps_300,
            bunker_item,
            completion_item,
            en02_stage_361_item,
        ];
        let mut mapper = FormKeyMapper::new(
            [],
            MapperOptions {
                output_plugin_name: "SeventySix.esm".to_string(),
                preserve_source_ids: true,
                ..MapperOptions::default()
            },
            &mut interner,
        );
        for form in mapped_forms {
            mapper.add_mapping(form, form);
        }
        let target = map_reward_payloads(source_payloads, &mapper);
        let xp_values = FxHashMap::from_iter([(xp_50, 50), (xp_1000, 1000)]);

        assert_eq!(
            target
                .xp
                .iter()
                .map(|(stage, global)| (*stage, xp_values[global]))
                .collect::<Vec<_>>(),
            vec![
                (61, 50),
                (110, 50),
                (250, 1000),
                (260, 1000),
                (261, 50),
                (361, 50)
            ]
        );
        assert!(!target.xp.iter().any(|(stage, _)| *stage == 300));
        assert_eq!(target.caps_item, Some(caps_item));
        assert_eq!(target.caps, vec![(61, caps_61), (300, caps_300)]);
        assert_eq!(
            target.items,
            vec![
                (250, bunker_item, 2),
                (300, completion_item, 1),
                (361, en02_stage_361_item, 1),
            ]
        );
    }

    #[test]
    fn raw_indx_stage_round_trips_through_attached_reward_vmad() {
        let interner = StringInterner::new();
        let reward = fk(&interner, 0x200);
        let quest = record(
            &interner,
            b"QUST",
            1,
            vec![
                field(
                    b"INDX",
                    FieldValue::Bytes(SmallVec::from_slice(&[0x28, 0x23, 0x00, 0x74])),
                ),
                field(b"QSDT", FieldValue::Uint(0)),
                field(b"QRWD", raw_form(reward.local)),
            ],
        );
        let stage = reward_stages(&quest, &[], "SeventySix.esm", &interner)[0].stage;
        let payload = TargetRewardPayload {
            xp: vec![(stage, fk(&interner, 0x556D52))],
            caps_item: Some(fk(&interner, 0x00000F)),
            caps: vec![(stage, fk(&interner, 0x5480BB))],
            items: vec![(stage, fk(&interner, 0x023736), 4)],
        };
        let reward_vmad = reward_script_vmad(&payload, &[], "SeventySix.esm", &interner).unwrap();
        let mut quest_vmad = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": "ExistingQuestScript",
                    "Flags": 0,
                    "Properties": [],
                }],
            }),
            &[],
            "SeventySix.esm",
        )
        .unwrap();

        assert_eq!(
            attach_quest_script(&mut quest_vmad, REWARD_SCRIPT_NAME, &reward_vmad),
            AttachResult::Changed
        );
        let decoded = compact_vmad_payload_json(&quest_vmad, &[], "SeventySix.esm", None).unwrap();
        let reward_script = decoded["Scripts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|script| script["ScriptName"] == REWARD_SCRIPT_NAME)
            .unwrap();
        let properties = reward_script["Properties"].as_array().unwrap();

        for property_name in ["XPStages", "CapsStages", "ItemStages"] {
            let property = properties
                .iter()
                .find(|property| property["propertyName"] == property_name)
                .unwrap();
            assert_eq!(property["Type"], "Array of Int32");
            assert_eq!(property["Value"], serde_json::json!([9000]));
            assert_ne!(property["Value"], serde_json::json!([0x7400_2328_u32]));
        }
        let xp_stages = properties
            .iter()
            .find(|property| property["propertyName"] == "XPStages")
            .unwrap()["Value"]
            .as_array()
            .unwrap();
        let reward_xp = properties
            .iter()
            .find(|property| property["propertyName"] == "RewardXP")
            .unwrap();
        assert_eq!(reward_xp["Type"], "Array of Object");
        assert_eq!(
            reward_xp["Value"].as_array().unwrap().len(),
            xp_stages.len()
        );
    }

    #[test]
    fn first_reward_group_keeps_caps_and_item_rows_before_itme() {
        let interner = StringInterner::new();
        let reward = record(
            &interner,
            b"GMRW",
            2,
            vec![
                field(b"NAM7", raw_form(0x556D52)),
                field(b"QRCO", raw_form(0x00000F)),
                field(b"NAM8", raw_form(0x5480BB)),
                field(b"QSRD", raw_item(0x023736, 4)),
                field(b"ITME", FieldValue::Bytes(SmallVec::new())),
                field(b"NAM8", raw_form(0x5480BC)),
                field(b"QSRD", raw_item(0x437005, 1)),
            ],
        );

        let payload =
            first_reward_group(&reward, 9000, true, &[], "SeventySix.esm", &interner).unwrap();
        assert_eq!(payload.stage, 9000);
        assert!(payload.completes_quest);
        assert_eq!(payload.xp_global, Some(fk(&interner, 0x556D52)));
        assert_eq!(payload.caps_item, Some(fk(&interner, 0x00000F)));
        assert_eq!(payload.caps_amount, Some(fk(&interner, 0x5480BB)));
        assert_eq!(payload.items, vec![(fk(&interner, 0x023736), 4)]);
        assert!(payload.excluded.xp);
        assert!(payload.excluded.team_alternatives);
    }

    #[test]
    fn rsvp02_group_two_caps_share_the_local_player_condition() {
        let interner = StringInterner::new();
        let reward = record(
            &interner,
            b"GMRW",
            0x6312E8,
            vec![
                field(b"XPCT", FieldValue::Float(0.0)),
                field(b"QRCO", raw_form(FO76_CAPS_LOCAL_ID)),
                field(b"QRLR", FieldValue::Uint(1)),
                field(b"QSRD", raw_item(0x4349F8, 4)),
                field(b"QSRD", raw_item(0x3CC4A3, 2)),
                field(b"QSRD", raw_item(0x3CA669, 2)),
                field(b"QSRD", raw_item(0x4E4881, 1)),
                field(b"CTDA", raw_active_player_condition(1.0)),
                field(b"ITME", FieldValue::None),
                field(b"NAM7", raw_form(0x3BD561)),
                field(b"XPCT", FieldValue::Float(0.0)),
                field(b"QRCO", raw_form(FO76_CAPS_LOCAL_ID)),
                field(b"NAM8", raw_form(0x3BE8C0)),
                field(b"QRLR", FieldValue::Uint(1)),
                field(b"CTDA", raw_active_player_condition(1.0)),
                field(b"ITME", FieldValue::None),
                field(b"NAM7", raw_form(0x5354B7)),
                field(b"XPCT", FieldValue::Float(0.0)),
                field(b"QRCO", raw_form(FO76_CAPS_LOCAL_ID)),
                field(b"NAM8", raw_form(0x5354B8)),
                field(b"QRLR", FieldValue::Uint(1)),
                field(b"CTDA", raw_active_player_condition(0.0)),
                field(b"ITME", FieldValue::None),
            ],
        );

        let payload =
            first_reward_group(&reward, 9000, true, &[], "SeventySix.esm", &interner).unwrap();
        assert_eq!(payload.xp_global, Some(fk(&interner, 0x3BD561)));
        assert_eq!(payload.caps_item, Some(fk(&interner, FO76_CAPS_LOCAL_ID)));
        assert_eq!(payload.caps_amount, Some(fk(&interner, 0x3BE8C0)));
        assert_eq!(
            payload.items,
            vec![
                (fk(&interner, 0x4349F8), 4),
                (fk(&interner, 0x3CC4A3), 2),
                (fk(&interner, 0x3CA669), 2),
                (fk(&interner, 0x4E4881), 1),
            ]
        );
        assert!(payload.excluded.xp);
        assert!(payload.excluded.team_alternatives);
    }

    #[test]
    fn mq_overseer_non_caps_group_does_not_overwrite_caps_amount() {
        let interner = StringInterner::new();
        let reward = record(
            &interner,
            b"GMRW",
            0x631275,
            vec![
                field(b"NAM7", raw_form(0x59B7D2)),
                field(b"QRCO", raw_form(FO76_CAPS_LOCAL_ID)),
                field(b"NAM8", raw_form(0x59B7D1)),
                field(b"QSRD", raw_item(0x43B776, 1)),
                field(b"CTDA", raw_active_player_condition(1.0)),
                field(b"ITME", FieldValue::None),
                field(b"QRCO", raw_form(0x3F7410)),
                field(b"NAM8", raw_form(0x59B7D3)),
                field(b"CTDA", raw_active_player_condition(1.0)),
                field(b"ITME", FieldValue::None),
            ],
        );

        let payload =
            first_reward_group(&reward, 9000, true, &[], "SeventySix.esm", &interner).unwrap();
        assert_eq!(payload.xp_global, Some(fk(&interner, 0x59B7D2)));
        assert_eq!(payload.caps_item, Some(fk(&interner, FO76_CAPS_LOCAL_ID)));
        assert_eq!(payload.caps_amount, Some(fk(&interner, 0x59B7D1)));
        assert_eq!(payload.items, vec![(fk(&interner, 0x43B776), 1)]);
        assert!(payload.excluded.xp);
        assert!(payload.excluded.non_caps_currency);
        assert!(!payload.excluded.team_alternatives);
    }

    #[test]
    fn rewarded_item_struct_decodes_without_raw_byte_assumptions() {
        let interner = StringInterner::new();
        let item_name = interner.intern("item");
        let count_name = interner.intern("count");
        let value = FieldValue::Struct(vec![
            (item_name, FieldValue::FormKey(fk(&interner, 0x564313))),
            (
                count_name,
                FieldValue::Bytes(SmallVec::from_slice(&[0x00, 0x00, 0x01, 0x00])),
            ),
        ]);

        assert_eq!(
            source_reward_items(&value, &[], "SeventySix.esm", &interner),
            vec![(fk(&interner, 0x564313), 65536)]
        );
    }

    #[test]
    fn settler_completion_payloads_6313d1_through_6313d4_are_exact() {
        let interner = StringInterner::new();
        let fixtures = [
            (0x6313D1, 0x5576E0, 0x564313, false, false),
            (0x6313D2, 0x5576DC, 0x564314, false, true),
            (0x6313D3, 0x5576D8, 0x599A7D, false, true),
            (0x6313D4, 0x5723EC, 0x599A7B, true, true),
        ];

        for (reward_id, caps_global, item, has_random_service, has_team_alternative) in fixtures {
            let mut fields = vec![
                field(b"NAM7", raw_form(0x5576DE)),
                field(b"QRCO", raw_form(FO76_CAPS_LOCAL_ID)),
                field(b"NAM8", raw_form(caps_global)),
            ];
            if has_random_service {
                fields.push(field(b"QRLI", raw_form(0x417C4B)));
            }
            fields.push(field(b"QSRD", raw_item(item, 1)));
            fields.push(field(b"ITME", FieldValue::None));
            if has_team_alternative {
                fields.push(field(b"ESRE", FieldValue::Float(0.0)));
                fields.push(field(
                    b"QRCO",
                    raw_form(if reward_id == 0x6313D4 {
                        0x538F6D
                    } else {
                        FO76_CAPS_LOCAL_ID
                    }),
                ));
                if reward_id == 0x6313D2 {
                    fields.push(field(b"NAM8", raw_form(0x421202)));
                }
                if reward_id >= 0x6313D3 {
                    fields.push(field(b"QRLI", raw_form(0x417C42)));
                }
                fields.push(field(b"ITME", FieldValue::None));
            }
            let reward = record(&interner, b"GMRW", reward_id, fields);

            let payload =
                first_reward_group(&reward, 9000, true, &[], "SeventySix.esm", &interner).unwrap();
            assert_eq!(payload.stage, 9000, "{reward_id:06X}");
            assert_eq!(
                payload.caps_item,
                Some(fk(&interner, FO76_CAPS_LOCAL_ID)),
                "{reward_id:06X}"
            );
            assert_eq!(
                payload.caps_amount,
                Some(fk(&interner, caps_global)),
                "{reward_id:06X}"
            );
            assert_eq!(
                payload.items,
                vec![(fk(&interner, item), 1)],
                "{reward_id:06X}"
            );
            assert_eq!(
                payload.excluded.random, has_random_service,
                "{reward_id:06X}"
            );
            assert_eq!(
                payload.excluded.team_alternatives, has_team_alternative,
                "{reward_id:06X}"
            );
        }
    }

    #[test]
    fn local_policy_rejects_non_caps_currency_and_server_services() {
        let interner = StringInterner::new();
        let reward = record(
            &interner,
            b"GMRW",
            0x6313D4,
            vec![
                field(b"NAM7", raw_form(0x5576D2)),
                field(b"QRCO", raw_form(0x538F6D)),
                field(b"NAM8", raw_form(0x5723EC)),
                field(b"QRCX", raw_form(0x100)),
                field(b"CENT", raw_form(0x101)),
                field(b"QRLI", raw_form(0x417C42)),
                field(b"DNAM", raw_form(0x6313D3)),
                field(b"ITME", FieldValue::None),
            ],
        );

        let payload =
            first_reward_group(&reward, 9000, true, &[], "SeventySix.esm", &interner).unwrap();
        assert_eq!(payload.caps_item, None);
        assert_eq!(payload.caps_amount, None);
        assert!(payload.excluded.xp);
        assert!(payload.excluded.non_caps_currency);
        assert!(payload.excluded.score_or_entitlement);
        assert!(payload.excluded.random);
        assert!(payload.excluded.chained);
        assert!(EXCLUDED_REWARD_POLICY.contains("reputation"));
    }

    #[test]
    fn quest_script_attachment_preserves_fragment_suffix_and_is_idempotent() {
        let interner = StringInterner::new();
        let existing = build_vmad_bytes_from_payload(
            &serde_json::json!({
                "Version": VMAD_VERSION,
                "Object Format": VMAD_OBJECT_FORMAT,
                "Scripts": [{
                    "ScriptName": "ExistingQuestScript",
                    "Flags": 0,
                    "Properties": [],
                }],
            }),
            &[],
            "SeventySix.esm",
        )
        .unwrap();
        let payload = TargetRewardPayload {
            xp: vec![(805, fk(&interner, 0x556D52))],
            caps_item: Some(fk(&interner, 0x00000F)),
            caps: vec![
                (805, fk(&interner, 0x59F4EC)),
                (9000, fk(&interner, 0x5480BB)),
            ],
            items: vec![(9000, fk(&interner, 0x023736), 4)],
        };
        let reward_vmad = reward_script_vmad(&payload, &[], "SeventySix.esm", &interner).unwrap();
        for property_name in [
            "XPStages",
            "RewardXP",
            "CapsStages",
            "RewardCaps",
            "CapsItem",
            "ItemStages",
            "RewardItems",
            "RewardCounts",
        ] {
            assert!(
                reward_vmad
                    .windows(property_name.len())
                    .any(|bytes| bytes == property_name.as_bytes()),
                "missing {property_name}"
            );
        }
        assert!(
            !reward_vmad
                .windows("CompletionStage".len())
                .any(|bytes| bytes == b"CompletionStage")
        );
        let suffix = [1, 0, 2, 0, 3, 0, 4, 0];
        let mut quest_vmad = existing;
        quest_vmad.extend_from_slice(&suffix);

        assert_eq!(
            attach_quest_script(&mut quest_vmad, REWARD_SCRIPT_NAME, &reward_vmad),
            AttachResult::Changed
        );
        assert!(quest_vmad.ends_with(&suffix));
        assert_eq!(u16::from_le_bytes(quest_vmad[4..6].try_into().unwrap()), 2);
        assert_eq!(
            attach_quest_script(&mut quest_vmad, REWARD_SCRIPT_NAME, &reward_vmad),
            AttachResult::AlreadyPresent
        );
    }
}
