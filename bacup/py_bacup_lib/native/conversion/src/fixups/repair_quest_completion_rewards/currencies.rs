use super::*;
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::ids::SubrecordSig;
use crate::translator::pair_hooks::fo76_fo4::expand_source_condition_form_rows;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;

const SCRIPT: &str = "B21:CurrencyQuestRewards";
const SCRIP: u32 = 0x3F7410;
const BULLION: u32 = 0x55C1C8;
const NOTES: u32 = 0x5A5443;

#[derive(Clone, Debug)]
struct CurrencyRow {
    stage: i32,
    item: FormKey,
    count: i32,
    amount: Option<FormKey>,
    conditions: Vec<FieldEntry>,
}

#[derive(Default)]
pub(super) struct CurrencyRepair {
    pub changed: bool,
    pub rows: usize,
    pub consumed: FxHashSet<(i32, FormKey)>,
}

fn currency(form: FormKey, source_plugin: &str, interner: &StringInterner) -> bool {
    matches!(form.local, SCRIP | BULLION | NOTES)
        && interner.resolve(form.plugin) == Some(source_plugin)
}

fn reference(
    value: &FieldValue,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    source_form_key(value, masters, plugin, interner).or_else(|| match value {
        FieldValue::Uint(raw) => {
            source_raw_form_key(u32::try_from(*raw).ok()?, masters, plugin, interner)
        }
        FieldValue::Int(raw) => {
            source_raw_form_key(u32::try_from(*raw).ok()?, masters, plugin, interner)
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(_, value)| reference(value, masters, plugin, interner)),
        FieldValue::List(values) => values
            .iter()
            .find_map(|value| reference(value, masters, plugin, interner)),
        _ => None,
    })
}

fn scalar(value: &FieldValue) -> Option<f32> {
    match value {
        FieldValue::Float(value) => Some(*value),
        FieldValue::Int(value) => Some(*value as f32),
        FieldValue::Uint(value) => Some(*value as f32),
        FieldValue::Bytes(bytes) if bytes.len() == 4 => {
            Some(f32::from_le_bytes(bytes.as_slice().try_into().ok()?))
        }
        _ => None,
    }
}

// Only deterministic, single-entry currency chains are flattened. Mixed or random
// equipment lists retain their existing reward path.
fn currency_item(
    form: FormKey,
    stage: i32,
    count: i32,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
    load: &mut impl FnMut(FormKey) -> Option<Record>,
    depth: usize,
) -> Option<CurrencyRow> {
    if currency(form, plugin, interner) {
        return Some(CurrencyRow {
            stage,
            item: form,
            count,
            amount: None,
            conditions: Vec::new(),
        });
    }
    if depth >= MAX_REWARD_LIST_DEPTH {
        return None;
    }
    let record = load(form)?;
    if record.sig.0 != *b"LVLI" {
        return None;
    }
    let entries = record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"LVLO")
        .collect::<Vec<_>>();
    if entries.len() != 1 {
        return None;
    }
    for field in &record.fields {
        if matches!(&field.sig.0, b"LVCV" | b"LVOV")
            && scalar(&field.value).is_some_and(|value| value != 0.0)
        {
            return None;
        }
        if matches!(&field.sig.0, b"LVLG" | b"LVOC" | b"LVIT" | b"LVOT")
            && reference(&field.value, masters, plugin, interner).is_some()
        {
            return None;
        }
    }
    let member = reference(&entries[0].value, masters, plugin, interner)?;
    let quantity_global = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"LVIG")
        .and_then(|field| reference(&field.value, masters, plugin, interner));
    let quantity = if quantity_global.is_some() {
        1
    } else {
        let value = record
            .fields
            .iter()
            .find(|field| field.sig.0 == *b"LVIV")
            .and_then(|field| scalar(&field.value))
            .unwrap_or(1.0);
        if !value.is_finite() || value <= 0.0 || value >= i32::MAX as f32 || value.fract() != 0.0 {
            return None;
        }
        value as i32
    };
    let mut row = currency_item(
        member,
        stage,
        count.checked_mul(quantity)?,
        masters,
        plugin,
        interner,
        load,
        depth + 1,
    )?;
    if quantity_global.is_some() && row.amount.is_some() {
        return None;
    }
    row.amount = row.amount.or(quantity_global);
    row.conditions.extend(
        record
            .fields
            .iter()
            .filter(|field| matches!(&field.sig.0, b"CTDA" | b"CIS1" | b"CIS2"))
            .cloned(),
    );
    Some(row)
}

fn collect(
    quest: &Record,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
    load: &mut impl FnMut(FormKey) -> Option<Record>,
) -> (Vec<CurrencyRow>, FxHashSet<(i32, FormKey)>) {
    let mut rows = Vec::new();
    let mut consumed = FxHashSet::default();
    let mut stage = None;
    for (field_index, field) in quest.fields.iter().enumerate() {
        match &field.sig.0 {
            b"INDX" => stage = field_value_stage_index(&field.value),
            b"QRWD" => {
                let Some(stage) = stage else {
                    continue;
                };
                let Some(reward) = reference(&field.value, masters, plugin, interner) else {
                    continue;
                };
                let Some(reward) = load(reward).filter(|record| record.sig.0 == *b"GMRW") else {
                    continue;
                };
                let start = quest.fields[..field_index]
                    .iter()
                    .rposition(|field| matches!(&field.sig.0, b"QSDT" | b"INDX"))
                    .unwrap_or(field_index);
                let stage_conditions = quest.fields[start + 1..]
                    .iter()
                    .take_while(|field| {
                        !matches!(&field.sig.0, b"QSDT" | b"INDX" | b"QOBJ" | b"ANAM")
                    })
                    .filter(|field| matches!(&field.sig.0, b"CTDA" | b"CIS1" | b"CIS2"))
                    .cloned()
                    .collect::<Vec<_>>();
                for group in reward.fields.split(|field| field.sig.0 == *b"ITME") {
                    let conditions = group
                        .iter()
                        .filter(|field| matches!(&field.sig.0, b"CTDA" | b"CIS1" | b"CIS2"))
                        .cloned()
                        .collect::<Vec<_>>();
                    let item = group
                        .iter()
                        .find(|field| field.sig.0 == *b"QRCO")
                        .and_then(|field| reference(&field.value, masters, plugin, interner));
                    let amount = group
                        .iter()
                        .find(|field| field.sig.0 == *b"NAM8")
                        .and_then(|field| reference(&field.value, masters, plugin, interner));
                    if let (Some(item), Some(amount)) = (
                        item.filter(|item| currency(*item, plugin, interner)),
                        amount,
                    ) {
                        let mut guards = stage_conditions.clone();
                        guards.extend(conditions.iter().cloned());
                        rows.push(CurrencyRow {
                            stage,
                            item,
                            count: 1,
                            amount: Some(amount),
                            conditions: guards,
                        });
                    }
                    for item_field in group.iter().filter(|field| field.sig.0 == *b"QSRD") {
                        for (item, count) in
                            source_reward_items(&item_field.value, masters, plugin, interner)
                        {
                            if let Some(mut row) = currency_item(
                                item, stage, count, masters, plugin, interner, load, 0,
                            ) {
                                let mut guards = stage_conditions.clone();
                                guards.extend(conditions.iter().cloned());
                                guards.extend(row.conditions);
                                row.conditions = guards;
                                rows.push(row);
                                consumed.insert((stage, item));
                            }
                        }
                    }
                }
            }
            b"QOBJ" | b"ANAM" => stage = None,
            _ => {}
        }
    }
    (rows, consumed)
}

fn mapped_condition(
    field: &FieldEntry,
    owner: FormKey,
    source_masters: &[String],
    source_plugin: &str,
    target_masters: &[String],
    mapper: &FormKeyMapper,
) -> Result<FieldEntry, String> {
    if field.sig.0 != *b"CTDA" {
        return Ok(field.clone());
    }
    let FieldValue::Bytes(raw) = &field.value else {
        return Err("currency_condition_not_raw".into());
    };
    if raw.len() != 32 {
        return Err("currency_condition_wrong_size".into());
    }
    let mut bytes = lower_currency_quest_predicate(raw);
    let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
    let mapped = |source: u32| -> Result<u32, String> {
        source_raw_form_key(source, source_masters, source_plugin, mapper.interner)
            .and_then(|form| mapper.lookup(form))
            .and_then(|form| encode_target_form_id(form, mapper.interner, target_masters))
            .ok_or_else(|| format!("currency_condition_unmapped:{source:08X}"))
    };
    if bytes[0] & 0x04 != 0 {
        let value = mapped(u32::from_le_bytes(bytes[4..8].try_into().unwrap()))?;
        bytes[4..8].copy_from_slice(&value.to_le_bytes());
    }
    match function {
        837 => {
            if !target_masters
                .iter()
                .any(|master| master.eq_ignore_ascii_case("Fallout4.esm"))
            {
                return Err("currency_player_master_missing".into());
            }
            let vanilla = mapper.interner.intern("Fallout4.esm");
            let player = encode_target_form_id(
                FormKey {
                    plugin: vanilla,
                    local: 7,
                },
                mapper.interner,
                target_masters,
            )
            .ok_or("currency_player_unmapped")?;
            let player_ref = encode_target_form_id(
                FormKey {
                    plugin: vanilla,
                    local: 0x14,
                },
                mapper.interner,
                target_masters,
            )
            .ok_or("currency_player_ref_unmapped")?;
            bytes[8..12].copy_from_slice(&72u32.to_le_bytes());
            bytes[12..16].copy_from_slice(&player.to_le_bytes());
            bytes[16..20].fill(0);
            bytes[20..24].copy_from_slice(&2u32.to_le_bytes());
            bytes[24..28].copy_from_slice(&player_ref.to_le_bytes());
            bytes[28..32].copy_from_slice(&u32::MAX.to_le_bytes());
        }
        59 if u32::from_le_bytes(bytes[16..20].try_into().unwrap()) == 0 => {
            let stage = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
            if stage > u16::MAX as u32 {
                return Err("currency_stage_condition_ambiguous".into());
            }
            let quest = encode_target_form_id(owner, mapper.interner, target_masters)
                .ok_or("currency_owner_unmapped")?;
            bytes[12..16].copy_from_slice(&quest.to_le_bytes());
            bytes[16..20].copy_from_slice(&stage.to_le_bytes());
        }
        14 | 56 | 59 | 74 | 543 | 629 => {
            let value = mapped(u32::from_le_bytes(bytes[12..16].try_into().unwrap()))?;
            bytes[12..16].copy_from_slice(&value.to_le_bytes());
        }
        _ => return Err(format!("currency_condition_unsupported:{function}")),
    }
    if function != 837 && u32::from_le_bytes(bytes[20..24].try_into().unwrap()) == 2 {
        let value = mapped(u32::from_le_bytes(bytes[24..28].try_into().unwrap()))?;
        bytes[24..28].copy_from_slice(&value.to_le_bytes());
    }
    Ok(FieldEntry {
        sig: field.sig,
        value: FieldValue::Bytes(bytes.into()),
    })
}

fn lower_currency_quest_predicate(raw: &[u8]) -> Vec<u8> {
    use crate::translator::pair_hooks::fo76_fo4::Fo76Fo4Hook;
    if raw.len() != 32 {
        return raw.to_vec();
    }
    let mut bytes = raw.to_vec();
    match u16::from_le_bytes(bytes[8..10].try_into().unwrap()) {
        // Currency reward eligibility uses the player's one local quest instance.
        5004 => bytes[8..12].copy_from_slice(&56u32.to_le_bytes()),
        917 => bytes[8..12].copy_from_slice(&59u32.to_le_bytes()),
        857 => {
            let mut value = FieldValue::Bytes(bytes.clone().into());
            if Fo76Fo4Hook::lower_quest_completion_count_condition(&mut value) == Some(true) {
                return field_bytes(&value).unwrap().to_vec();
            }
        }
        _ => {}
    }
    bytes
}

fn append_items(target: &mut Record, rows: &[CurrencyRow]) -> Result<Vec<i32>, String> {
    let mut indices = vec![-1; rows.len()];
    let old = std::mem::take(&mut target.fields);
    let mut stage = None;
    let mut item_index = 0;
    for field in old {
        if matches!(&field.sig.0, b"INDX" | b"QOBJ" | b"ANAM") {
            if let Some(stage) = stage {
                for (index, row) in rows
                    .iter()
                    .enumerate()
                    .filter(|(_, row)| row.stage == stage)
                {
                    target.fields.push(FieldEntry {
                        sig: SubrecordSig(*b"QSDT"),
                        value: FieldValue::Uint(0),
                    });
                    target.fields.extend(row.conditions.iter().cloned());
                    indices[index] = item_index;
                    item_index += 1;
                }
            }
            stage = if field.sig.0 == *b"INDX" {
                field_value_stage_index(&field.value)
            } else {
                None
            };
            item_index = 0;
        }
        if field.sig.0 == *b"QSDT" {
            item_index += 1;
        }
        target.fields.push(field);
    }
    if let Some(stage) = stage {
        for (index, row) in rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.stage == stage)
        {
            target.fields.push(FieldEntry {
                sig: SubrecordSig(*b"QSDT"),
                value: FieldValue::Uint(0),
            });
            target.fields.extend(row.conditions.iter().cloned());
            indices[index] = item_index;
            item_index += 1;
        }
    }
    if indices.iter().any(|index| *index < 0) {
        return Err("currency_reward_stage_missing".into());
    }
    Ok(indices)
}

pub(super) fn repair(
    source: &Record,
    target_key: FormKey,
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
) -> Result<CurrencyRepair, String> {
    let source_schema = session.source_schema().map_err(|error| error.to_string())?;
    let source_slot = session.source_slot_opt().ok_or("currency_source_missing")?;
    let source_masters = source_slot.parsed.header.masters.clone();
    let source_plugin = source_slot.parsed.plugin_name.clone();
    let (rows, consumed) = collect(
        source,
        &source_masters,
        &source_plugin,
        mapper.interner,
        &mut |form| {
            session
                .source_record_decoded(&form, &source_schema, mapper.interner)
                .ok()
        },
    );
    attach_rows(
        rows,
        consumed,
        target_key,
        session,
        mapper,
        &source_masters,
        &source_plugin,
    )
}

fn attach_rows(
    mut rows: Vec<CurrencyRow>,
    consumed: FxHashSet<(i32, FormKey)>,
    target_key: FormKey,
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    source_masters: &[String],
    source_plugin: &str,
) -> Result<CurrencyRepair, String> {
    if rows.is_empty() {
        return Ok(CurrencyRepair::default());
    }
    let target_schema = session.schema().map_err(|error| error.to_string())?;
    let target_masters = session.target_masters().to_vec();
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let mut target = session
        .record_decoded(&target_key, &target_schema, mapper.interner)
        .map_err(|error| error.to_string())?;
    let vmad = target
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"VMAD")
        .and_then(|field| field_bytes(&field.value))
        .ok_or("currency_vmad_missing")?;
    let (_, _, existing) = top_level_script_layout(vmad, SCRIPT).ok_or("currency_vmad_invalid")?;
    let existing_indices = match existing.len() {
        0 => None,
        1 => {
            let payload = compact_vmad_payload_json(vmad, &target_masters, &target_plugin, None)
                .ok_or("currency_vmad_decode")?;
            let value = payload["Scripts"]
                .as_array()
                .and_then(|scripts| scripts.iter().find(|s| s["ScriptName"] == SCRIPT))
                .and_then(|script| script["Properties"].as_array())
                .and_then(|props| {
                    props
                        .iter()
                        .find(|p| p["propertyName"] == "RewardStageItems")
                })
                .ok_or("currency_existing_indices_missing")?;
            let indices: Vec<i32> = serde_json::from_value(value["Value"].clone())
                .map_err(|_| "currency_existing_indices_invalid")?;
            if indices.len() != rows.len() {
                return Err("currency_existing_row_count_changed".into());
            }
            Some(indices)
        }
        _ => return Err("currency_duplicate_script".into()),
    };
    let limits = rows
        .iter()
        .map(|row| {
            if row.item.local == BULLION {
                10000
            } else {
                i32::MAX
            }
        })
        .collect::<Vec<_>>();
    for row in &mut rows {
        row.item = mapper.lookup(row.item).ok_or("currency_item_unmapped")?;
        if let Some(amount) = row.amount {
            row.amount = Some(mapper.lookup(amount).ok_or("currency_amount_unmapped")?);
        }
        if row.conditions.iter().any(|field| {
            field.sig.0 == *b"CTDA"
                && field_bytes(&field.value)
                    .is_some_and(|bytes| bytes.get(8..10) == Some(&875u16.to_le_bytes()))
        }) {
            if row.conditions.iter().any(|field| field.sig.0 != *b"CTDA") {
                return Err("currency_condition_form_with_parameter_strings".into());
            }
            let raw = row
                .conditions
                .iter()
                .map(|field| field_bytes(&field.value).map(|bytes| bytes.to_vec()))
                .collect::<Option<Vec<_>>>()
                .ok_or("currency_condition_not_raw")?;
            row.conditions =
                expand_source_condition_form_rows(&raw, lower_currency_quest_predicate)?
                    .into_iter()
                    .map(|bytes| FieldEntry {
                        sig: SubrecordSig(*b"CTDA"),
                        value: FieldValue::Bytes(bytes.into()),
                    })
                    .collect();
        }
        row.conditions = row
            .conditions
            .iter()
            .map(|field| {
                mapped_condition(
                    field,
                    target_key,
                    source_masters,
                    source_plugin,
                    &target_masters,
                    mapper,
                )
            })
            .collect::<Result<_, _>>()?;
    }
    let indices = match existing_indices {
        Some(indices) => indices,
        None => append_items(&mut target, &rows)?,
    };
    let null = serde_json::json!({"Alias": -1, "FormID": null});
    let properties = serde_json::json!([
        {"propertyName":"RewardStages","Type":"Array of Int32","Flags":1,"Value":rows.iter().map(|row|row.stage).collect::<Vec<_>>()},
        {"propertyName":"RewardStageItems","Type":"Array of Int32","Flags":1,"Value":indices},
        {"propertyName":"RewardItems","Type":"Array of Object","Flags":1,"Value":rows.iter().map(|row|object_value(row.item,mapper.interner)).collect::<Option<Vec<_>>>().ok_or("currency_item_encode")?},
        {"propertyName":"RewardAmounts","Type":"Array of Object","Flags":1,"Value":rows.iter().map(|row|row.amount.and_then(|form|object_value(form,mapper.interner)).unwrap_or_else(||null.clone())).collect::<Vec<_>>()},
        {"propertyName":"RewardMultipliers","Type":"Array of Int32","Flags":1,"Value":rows.iter().map(|row|row.count).collect::<Vec<_>>()},
        {"propertyName":"HoldingLimits","Type":"Array of Int32","Flags":1,"Value":limits}
    ]);
    let script = build_vmad_bytes_from_payload(&serde_json::json!({"Version":6,"Object Format":2,"Scripts":[{"ScriptName":SCRIPT,"Flags":0,"Properties":properties}]}), &target_masters, &target_plugin).ok_or("currency_vmad_encode")?;
    let vmad = target
        .fields
        .iter_mut()
        .find(|field| field.sig.0 == *b"VMAD")
        .unwrap();
    let mut bytes = field_bytes(&vmad.value)
        .ok_or("currency_vmad_not_raw")?
        .to_vec();
    match attach_quest_script(&mut bytes, SCRIPT, &script) {
        AttachResult::AlreadyPresent => {
            return Ok(CurrencyRepair {
                changed: false,
                rows: rows.len(),
                consumed,
            });
        }
        AttachResult::Changed => {}
        AttachResult::Conflict(reason) => return Err(format!("currency_vmad_attach:{reason}")),
    }
    vmad.value = FieldValue::Bytes(bytes.into());
    session
        .replace_record_contents(target, &target_schema, mapper.interner)
        .map_err(|error| error.to_string())?;
    Ok(CurrencyRepair {
        changed: true,
        rows: rows.len(),
        consumed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_close_native, plugin_handle_new_native,
    };

    fn fk(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            plugin: interner.intern("SeventySix.esm"),
            local,
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
        let mut record = Record::new(SigCode(*sig), fk(interner, local));
        record.fields = fields.into_iter().collect();
        record
    }

    fn cond(function: u16, param: u32, comparison: f32) -> FieldEntry {
        let mut bytes = vec![0; 32];
        bytes[4..8].copy_from_slice(&comparison.to_le_bytes());
        bytes[8..10].copy_from_slice(&function.to_le_bytes());
        bytes[12..16].copy_from_slice(&param.to_le_bytes());
        bytes[28..32].fill(0xff);
        field(b"CTDA", FieldValue::Bytes(bytes.into()))
    }

    fn fixture(interner: &StringInterner) -> (Record, Vec<Record>) {
        let quest = record(
            interner,
            b"QUST",
            0x548761,
            vec![
                field(b"INDX", FieldValue::Uint(2000)),
                field(b"QSDT", FieldValue::Uint(0)),
                field(b"QRWD", FieldValue::FormKey(fk(interner, 0x63139D))),
                cond(59, 900, 1.0),
            ],
        );
        let mut item = 0x5A544Eu32.to_le_bytes().to_vec();
        item.extend_from_slice(&1u32.to_le_bytes());
        let reward = record(
            interner,
            b"GMRW",
            0x63139D,
            vec![
                field(b"QRCO", FieldValue::FormKey(fk(interner, 15))),
                field(b"ITME", FieldValue::None),
                field(b"QRCO", FieldValue::FormKey(fk(interner, SCRIP))),
                field(b"NAM8", FieldValue::FormKey(fk(interner, 0x5930F1))),
                field(b"ITME", FieldValue::None),
                field(b"QSRD", FieldValue::Bytes(item.into())),
                field(b"ITME", FieldValue::None),
            ],
        );
        let list = record(
            interner,
            b"LVLI",
            0x5A544E,
            vec![
                field(b"LVLO", FieldValue::Uint(0x5A5880)),
                field(b"LVIV", FieldValue::Float(0.0)),
                field(b"LVIG", FieldValue::FormKey(fk(interner, 0x5A587E))),
            ],
        );
        let nested = record(
            interner,
            b"LVLI",
            0x5A5880,
            vec![
                field(b"LVLO", FieldValue::Uint(NOTES as u64)),
                field(b"LVIV", FieldValue::Float(1.0)),
                cond(74, 0x5A543C, 1.0),
            ],
        );
        (quest, vec![reward, list, nested])
    }

    #[test]
    fn collects_guaranteed_currency_rows_and_appends_items() {
        let interner = StringInterner::new();
        let (quest, records) = fixture(&interner);
        let (rows, consumed) = collect(&quest, &[], "SeventySix.esm", &interner, &mut |key| {
            records.iter().find(|r| r.form_key == key).cloned()
        });
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].item.local, SCRIP);
        assert_eq!(rows[0].amount.unwrap().local, 0x5930F1);
        assert_eq!(rows[0].conditions, vec![cond(59, 900, 1.0)]);
        assert_eq!(rows[1].item.local, NOTES);
        assert_eq!(rows[1].amount.unwrap().local, 0x5A587E);
        assert_eq!(rows[1].count, 1);
        assert_eq!(
            rows[1].conditions,
            vec![cond(59, 900, 1.0), cond(74, 0x5A543C, 1.0)]
        );
        assert_eq!(
            consumed,
            FxHashSet::from_iter([(2000, fk(&interner, 0x5A544E))])
        );

        let (mut target, records) = fixture(&interner);
        let (rows, _) = collect(&target, &[], "SeventySix.esm", &interner, &mut |key| {
            records.iter().find(|r| r.form_key == key).cloned()
        });
        target.fields.push(field(b"QSDT", FieldValue::Uint(1)));
        target.fields.push(field(b"ANAM", FieldValue::Uint(5)));
        target.fields.push(field(b"ALST", FieldValue::Uint(3)));
        let original = target.fields.clone();
        assert_eq!(append_items(&mut target, &rows).unwrap(), vec![2, 3]);
        assert_eq!(
            &target.fields[..original.len() - 2],
            &original[..original.len() - 2]
        );
        assert_eq!(
            &target.fields[target.fields.len() - 2..],
            &original[original.len() - 2..]
        );

        let (_, mut records) = fixture(&interner);
        records[1]
            .fields
            .push(field(b"LVCV", FieldValue::Float(50.0)));
        assert!(
            currency_item(
                fk(&interner, 0x5A544E),
                1,
                1,
                &[],
                "SeventySix.esm",
                &interner,
                &mut |key| records.iter().find(|r| r.form_key == key).cloned(),
                0
            )
            .is_none()
        );
        let cycle = record(
            &interner,
            b"LVLI",
            0x800,
            vec![field(b"LVLO", FieldValue::Uint(0x800))],
        );
        assert!(
            currency_item(
                cycle.form_key,
                1,
                1,
                &[],
                "SeventySix.esm",
                &interner,
                &mut |_| Some(cycle.clone()),
                0
            )
            .is_none()
        );
    }

    #[test]
    fn conditions_remap_forms_preserve_aliases_and_lower_local_owner() {
        let mut interner = StringInterner::new();
        let quest = fk(&interner, 0x548761);
        let globals = [0x5930F1, 0x5A543C];
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut interner);
        for id in globals {
            let key = fk(mapper.interner, id);
            mapper.add_mapping(key, key);
        }
        let masters = vec!["Fallout4.esm".into()];
        let lower = |field: &FieldEntry| {
            mapped_condition(field, quest, &[], "SeventySix.esm", &masters, &mapper).unwrap()
        };
        let stage = lower(&cond(59, 900, 1.0));
        let bytes = field_bytes(&stage.value).unwrap();
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x01548761
        );
        assert_eq!(u32::from_le_bytes(bytes[16..20].try_into().unwrap()), 900);
        for comparison in [0.0f32, 1.0] {
            let owner = lower(&cond(837, 0, comparison));
            let bytes = field_bytes(&owner.value).unwrap();
            assert_eq!(&bytes[4..8], &comparison.to_le_bytes());
            assert_eq!(bytes[8], 72);
            assert_eq!(bytes[12], 7);
            assert_eq!(bytes[20], 2);
            assert_eq!(bytes[24], 0x14);
        }
        let mut alias = cond(14, 0x5930F1, 1.0);
        if let FieldValue::Bytes(bytes) = &mut alias.value {
            bytes[20] = 5;
            bytes[24] = 3;
        }
        let mapped = lower(&alias);
        let bytes = field_bytes(&mapped.value).unwrap();
        assert_eq!(bytes[15], 1);
        assert_eq!(bytes[20], 5);
        assert_eq!(bytes[24], 3);
        assert!(
            mapped_condition(
                &cond(999, 0, 1.0),
                quest,
                &[],
                "SeventySix.esm",
                &masters,
                &mapper
            )
            .is_err()
        );
        assert!(
            mapped_condition(
                &cond(74, 0x800, 1.0),
                quest,
                &[],
                "SeventySix.esm",
                &masters,
                &mapper
            )
            .is_err()
        );
    }

    #[test]
    fn actual_session_writes_aligned_properties_and_only_attaches_once() {
        let mut interner = StringInterner::new();
        let (mut quest, mut source_records) = fixture(&interner);
        quest.fields.retain(|field| field.sig.0 != *b"CTDA");
        source_records[2]
            .fields
            .retain(|field| field.sig.0 != *b"CTDA");
        let (mut rows, consumed) = collect(&quest, &[], "SeventySix.esm", &interner, &mut |key| {
            source_records.iter().find(|r| r.form_key == key).cloned()
        });
        rows.push(CurrencyRow {
            stage: 2000,
            item: fk(&interner, SCRIP),
            count: 3,
            amount: None,
            conditions: vec![],
        });
        let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        let vmad = build_vmad_bytes_from_payload(
            &serde_json::json!({"Version":6,"Object Format":2,"Scripts":[]}),
            &[],
            "SeventySix.esm",
        )
        .unwrap();
        let mut target_quest = quest.clone();
        target_quest.fields.retain(|field| field.sig.0 != *b"QRWD");
        target_quest
            .fields
            .insert(0, field(b"VMAD", FieldValue::Bytes(vmad.into())));
        let mut mapper = FormKeyMapper::new([], MapperOptions::default(), &mut interner);
        for id in [SCRIP, NOTES, 0x5930F1, 0x5A587E] {
            let key = fk(mapper.interner, id);
            mapper.add_mapping(key, key);
        }
        {
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            session
                .add_record(target_quest, &schema, mapper.interner)
                .unwrap();
            let result = attach_rows(
                rows.clone(),
                consumed.clone(),
                quest.form_key,
                &mut session,
                &mapper,
                &[],
                "SeventySix.esm",
            )
            .unwrap();
            assert!(result.changed);
            assert_eq!(result.rows, 3);
            let saved = session
                .record_decoded(&quest.form_key, &schema, mapper.interner)
                .unwrap();
            let bytes = field_bytes(
                &saved
                    .fields
                    .iter()
                    .find(|f| f.sig.0 == *b"VMAD")
                    .unwrap()
                    .value,
            )
            .unwrap();
            let decoded = compact_vmad_payload_json(bytes, &[], "SeventySix.esm", None).unwrap();
            for property in decoded["Scripts"][0]["Properties"].as_array().unwrap() {
                assert_eq!(property["Value"].as_array().unwrap().len(), 3, "{property}");
            }
            let again = attach_rows(
                rows.clone(),
                consumed.clone(),
                quest.form_key,
                &mut session,
                &mapper,
                &[],
                "SeventySix.esm",
            )
            .unwrap();
            assert!(!again.changed);
            assert_eq!(
                session
                    .record_decoded(&quest.form_key, &schema, mapper.interner)
                    .unwrap()
                    .fields,
                saved.fields
            );
            rows[0].count += 1;
            assert!(
                attach_rows(
                    rows,
                    consumed,
                    quest.form_key,
                    &mut session,
                    &mapper,
                    &[],
                    "SeventySix.esm"
                )
                .is_err()
            );
        }
        assert!(plugin_handle_close_native(target));
    }
}
