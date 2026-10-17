//! Pure decoding for the Tales leveled-list catalog: FO76 LVLI/LVLN, CNDF, REGN and
//! NPC_ subrecords in, catalog JSON out. Forms are named through `Keys`, so tests
//! need no plugin session.

use std::collections::BTreeSet;

use esp_authoring_core::plugin_runtime::ParsedSubrecord;
use serde_json::{Map, Value, json};

use crate::fixups::rewrite_raw_object_template_formids::{
    fo76_condition_has_reference_parameter, fo76_condition_reference_offsets, fo76_condition_references,
};

pub(crate) trait Keys {
    /// `Plugin.esm:XXXXXX` of a raw source FormID; None for an invalid master index.
    fn source(&self, raw: u32) -> Option<String>;
    /// Output FormKey of a source form that exists in the converted plugin or its masters.
    fn output(&self, raw: u32) -> Option<String>;
    fn is_faction(&self, raw: u32) -> bool;
}

#[derive(Debug, Default)]
pub(crate) struct Diagnostics {
    pub unmapped_entries: u32,
    pub unmapped_forms: BTreeSet<String>,
    pub unresolved_conditions: u32,
    pub unresolved_values: u32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RawValue {
    pub value: Option<f32>,
    pub global: Option<u32>,
    pub curve: Option<u32>,
}

impl RawValue {
    fn dynamic(&self) -> bool {
        self.global.is_some() || self.curve.is_some()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RawExtra {
    pub owner: u32,
    pub global_or_rank: u32,
    pub health: f32,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RawEntry {
    pub form: u32,
    pub level: RawValue,
    pub count: RawValue,
    pub chance_none: RawValue,
    pub extra: Option<RawExtra>,
    pub conditions: Vec<[u8; 32]>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RawList {
    pub flags: u16,
    /// DIQO: the quest whose player instance ActivePlayers/PotentialPlayers conditions read.
    pub quest: Option<u32>,
    pub chance_none: RawValue,
    pub max_count: RawValue,
    pub conditions: Vec<[u8; 32]>,
    pub entries: Vec<RawEntry>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct RawRegion {
    pub world: Option<u32>,
    pub polygons: Vec<Vec<[f32; 2]>>,
}

pub(crate) struct ListIdentity<'a> {
    pub source: &'a str,
    pub target: &'a str,
    pub editor_id: &'a str,
    pub signature: &'a str,
    pub scope: &'a str,
}

const OPERATORS: [&str; 6] = ["==", "!=", ">", ">=", "<", "<="];

// FO76's CTDA run-on values (TESConditionItem::IsTrue 0x165f340 switches on them).
const RUN_ONS: [&str; 16] = [
    "subject",
    "target",
    "reference",
    "combat_target",
    "linked_reference",
    "quest_alias",
    "package_data",
    "event_data",
    "command_target",
    "event_camera_ref",
    "my_killer",
    "active_players",
    "potential_players",
    "player_teammates",
    "target_list",
    "instance_owner",
];

fn u32_at(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().unwrap()))
}

fn finite_f32(data: &[u8], offset: usize, what: &str) -> Result<f32, String> {
    let value = u32_at(data, offset)
        .map(f32::from_bits)
        .ok_or_else(|| format!("short {what}"))?;
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("non-finite {what}"))
    }
}

fn form(data: &[u8]) -> Option<u32> {
    u32_at(data, 0).filter(|raw| *raw != 0)
}

fn condition(data: &[u8]) -> Result<[u8; 32], String> {
    data.try_into()
        .map_err(|_| format!("CTDA size {}", data.len()))
}

fn lvlo(data: &[u8]) -> Result<RawEntry, String> {
    match data.len() {
        4 => Ok(RawEntry { form: u32_at(data, 0).unwrap(), ..RawEntry::default() }),
        // Form version < 174: level u16, unused u16, form u32, count u16, chance-none u8, unused u8.
        12 => Ok(RawEntry {
            form: u32_at(data, 4).unwrap(),
            level: RawValue { value: Some(f32::from(u16::from_le_bytes([data[0], data[1]]))), ..RawValue::default() },
            count: RawValue { value: Some(f32::from(u16::from_le_bytes([data[8], data[9]]))), ..RawValue::default() },
            chance_none: RawValue { value: Some(f32::from(data[10])), ..RawValue::default() },
            ..RawEntry::default()
        }),
        size => Err(format!("LVLO size {size}")),
    }
}

pub(crate) fn editor_id(fields: &[ParsedSubrecord]) -> String {
    fields
        .iter()
        .find(|field| field.signature == "EDID")
        .map(|field| String::from_utf8_lossy(&field.data).trim_end_matches('\0').to_string())
        .unwrap_or_default()
}

pub(crate) fn decode_list(fields: &[ParsedSubrecord]) -> Result<RawList, String> {
    let mut list = RawList::default();
    for field in fields {
        let signature = field.signature.as_str();
        let data: &[u8] = field.data.as_ref();
        match signature {
            // u8 before form version 185, u16 from 185; the size tells which.
            "LVLF" => {
                list.flags = match data {
                    [low] => u16::from(*low),
                    [low, high] => u16::from_le_bytes([*low, *high]),
                    _ => return Err(format!("LVLF size {}", data.len())),
                }
            }
            // Empty in modern FO76 (form 209): a marker before LVCV/LVMV, which carry the values.
            "LVLD" => {
                if let Some(byte) = data.first() {
                    list.chance_none.value = Some(f32::from(*byte));
                }
            }
            "LVCV" => list.chance_none.value = Some(finite_f32(data, 0, signature)?),
            "LVLG" => list.chance_none.global = form(data),
            "LVCT" => list.chance_none.curve = form(data),
            "LVLM" => {
                if let Some(byte) = data.first() {
                    list.max_count.value = Some(f32::from(*byte));
                }
            }
            "LVMV" => list.max_count.value = Some(finite_f32(data, 0, signature)?),
            "LVMG" => list.max_count.global = form(data),
            "LVMT" => list.max_count.curve = form(data),
            "LVLO" => list.entries.push(lvlo(data)?),
            "DIQO" => list.quest = form(data),
            // A CTDA before the first LVLO gates the list; after it, the preceding entry.
            "CTDA" => {
                let row = condition(data)?;
                match list.entries.last_mut() {
                    Some(entry) => entry.conditions.push(row),
                    None => list.conditions.push(row),
                }
            }
            "LVLV" | "LVOG" | "LVLT" | "LVIV" | "LVIG" | "LVIT" | "LVOV" | "LVOC" | "LVOT" | "COED" => {
                let entry = list
                    .entries
                    .last_mut()
                    .ok_or_else(|| format!("{signature} before LVLO"))?;
                match signature {
                    "LVLV" => entry.level.value = Some(finite_f32(data, 0, signature)?),
                    "LVOG" => entry.level.global = form(data),
                    "LVLT" => entry.level.curve = form(data),
                    "LVIV" => entry.count.value = Some(finite_f32(data, 0, signature)?),
                    "LVIG" => entry.count.global = form(data),
                    "LVIT" => entry.count.curve = form(data),
                    "LVOV" => entry.chance_none.value = Some(finite_f32(data, 0, signature)?),
                    "LVOC" => entry.chance_none.global = form(data),
                    "LVOT" => entry.chance_none.curve = form(data),
                    _ => {
                        entry.extra = Some(RawExtra {
                            owner: u32_at(data, 0).ok_or("short COED")?,
                            global_or_rank: u32_at(data, 4).ok_or("short COED")?,
                            health: finite_f32(data, 8, "COED")?,
                        })
                    }
                }
            }
            _ => {}
        }
    }
    Ok(list)
}

pub(crate) fn decode_conditions(fields: &[ParsedSubrecord]) -> Result<Vec<[u8; 32]>, String> {
    fields
        .iter()
        .filter(|field| field.signature == "CTDA")
        .map(|field| condition(field.data.as_ref()))
        .collect()
}

pub(crate) fn decode_region(fields: &[ParsedSubrecord]) -> RawRegion {
    let mut region = RawRegion::default();
    for field in fields {
        let data: &[u8] = field.data.as_ref();
        match field.signature.as_str() {
            "WNAM" => region.world = form(data),
            // One RPLI/RPLD pair per area; RPLD holds the polygon's X/Y points.
            "RPLD" => region.polygons.push(
                data.chunks_exact(8)
                    .map(|point| {
                        [
                            f32::from_le_bytes(point[0..4].try_into().unwrap()),
                            f32::from_le_bytes(point[4..8].try_into().unwrap()),
                        ]
                    })
                    .filter(|point| point[0].is_finite() && point[1].is_finite())
                    .collect(),
            ),
            _ => {}
        }
    }
    region
}

/// The template an NPC_ takes every slot from, if any. Only such a wrapper can be replaced
/// by its template list: an NPC that keeps a slot of its own (a zero TPTA slot) or takes one
/// from another record would lose it. LCharRefugeResponder_Normal's NPCs take Traits from a
/// face list and Inventory/Factions from elsewhere; delegating them to the face list rolled
/// raider-templated face donors as hostile, naked Responders.
pub(crate) fn whole_template(fields: &[ParsedSubrecord]) -> Option<u32> {
    let tpta = fields.iter().find(|field| field.signature == "TPTA")?.data.as_ref();
    let mut slots = tpta.chunks_exact(4).map(|slot| u32::from_le_bytes(slot.try_into().unwrap()));
    let first = slots.next().filter(|raw| *raw != 0)?;
    slots.all(|raw| raw == first).then_some(first)
}

pub(crate) fn curve_json_path(fields: &[ParsedSubrecord]) -> Option<String> {
    fields
        .iter()
        .find(|field| matches!(field.signature.as_str(), "CRVE" | "JASF"))
        .map(|field| String::from_utf8_lossy(&field.data).trim_end_matches('\0').to_string())
        .filter(|path| !path.is_empty())
}

/// Lists FO4's roll can't reproduce (design 4.1 "Selection").
pub(crate) fn needs_runtime(list: &RawList) -> bool {
    const FO76_ONLY_FLAGS: u16 = 0x08 | 0x40 | 0x80 | 0x200;
    list.flags & FO76_ONLY_FLAGS != 0
        || !list.conditions.is_empty()
        || list.chance_none.dynamic()
        || list.max_count.dynamic()
        || list.entries.iter().any(|entry| {
            !entry.conditions.is_empty()
                || entry.level.dynamic()
                || entry.count.dynamic()
                || entry.chance_none.dynamic()
        })
}

pub(crate) fn conditions_of(list: &RawList) -> Vec<[u8; 32]> {
    list.conditions
        .iter()
        .chain(list.entries.iter().flat_map(|entry| entry.conditions.iter()))
        .copied()
        .collect()
}

fn value_references(value: &RawValue, out: &mut BTreeSet<u32>) {
    out.extend(value.global);
    out.extend(value.curve);
}

pub(crate) fn condition_references(rows: &[[u8; 32]], out: &mut BTreeSet<u32>) {
    for row in rows {
        out.extend(fo76_condition_references(row));
    }
}

pub(crate) fn list_references(list: &RawList, out: &mut BTreeSet<u32>) {
    value_references(&list.chance_none, out);
    value_references(&list.max_count, out);
    condition_references(&list.conditions, out);
    for entry in &list.entries {
        out.insert(entry.form);
        value_references(&entry.level, out);
        value_references(&entry.count, out);
        value_references(&entry.chance_none, out);
        condition_references(&entry.conditions, out);
        if let Some(extra) = &entry.extra {
            out.insert(extra.owner);
            out.insert(extra.global_or_rank);
        }
    }
    out.remove(&0);
}

pub(crate) fn curve_references(list: &RawList, out: &mut BTreeSet<u32>) {
    let values = [&list.chance_none, &list.max_count].into_iter().chain(
        list.entries
            .iter()
            .flat_map(|entry| [&entry.level, &entry.count, &entry.chance_none]),
    );
    out.extend(values.filter_map(|value| value.curve));
}

pub(crate) fn parameter_1_of(rows: &[[u8; 32]], function: u16, out: &mut BTreeSet<u32>) {
    parameter_of(rows, function, 12, out);
}

pub(crate) fn parameter_2_of(rows: &[[u8; 32]], function: u16, out: &mut BTreeSet<u32>) {
    parameter_of(rows, function, 16, out);
}

fn parameter_of(rows: &[[u8; 32]], function: u16, offset: usize, out: &mut BTreeSet<u32>) {
    for row in rows {
        if u16::from_le_bytes([row[8], row[9]]) == function
            && let Some(raw) = form(&row[offset..offset + 4])
        {
            out.insert(raw);
        }
    }
}

fn output_or_unresolved(
    raw: u32,
    keys: &dyn Keys,
    diagnostics: &mut Diagnostics,
    unresolved: &mut bool,
) -> Value {
    match keys.output(raw) {
        Some(key) => json!(key),
        None => {
            *unresolved = true;
            if let Some(source) = keys.source(raw) {
                diagnostics.unmapped_forms.insert(source);
            }
            Value::Null
        }
    }
}

pub(crate) fn value_json(value: &RawValue, default: f32, keys: &dyn Keys, diagnostics: &mut Diagnostics) -> Value {
    let mut out = Map::new();
    let mut unresolved = false;
    out.insert("value".into(), json!(value.value.unwrap_or(default)));
    if let Some(global) = value.global {
        out.insert("global".into(), output_or_unresolved(global, keys, diagnostics, &mut unresolved));
    }
    if let Some(curve) = value.curve {
        match keys.source(curve) {
            Some(key) => {
                out.insert("curve".into(), json!(key));
            }
            None => unresolved = true,
        }
    }
    if unresolved {
        diagnostics.unresolved_values += 1;
        out.insert("unresolved".into(), json!(true));
    }
    Value::Object(out)
}

pub(crate) fn condition_json(row: &[u8; 32], keys: &dyn Keys, diagnostics: &mut Diagnostics) -> Value {
    let flags = row[0];
    let function = u16::from_le_bytes([row[8], row[9]]);
    let form_offsets = fo76_condition_reference_offsets(row);
    let mut unresolved = false;
    let mut out = Map::new();
    out.insert("function".into(), json!(function));
    let operator = OPERATORS.get(usize::from(flags >> 5)).copied();
    unresolved |= operator.is_none();
    out.insert("op".into(), json!(operator.unwrap_or("==")));
    out.insert("or".into(), json!(flags & 0x01 != 0));
    out.insert("swap".into(), json!(flags & 0x10 != 0));
    // 0x02 alias parameters and 0x08 package-data parameters have no meaning in a list roll.
    // FO76 data sets them on parameterless functions too (LChar_MainAll's GetRandomPercent).
    let reinterpreted = if fo76_condition_has_reference_parameter(function) { flags & 0x0A } else { 0 };
    out.insert("unsupported_flags".into(), json!(reinterpreted));
    if flags & 0x04 != 0 {
        let global = u32_at(row, 4).unwrap();
        out.insert("global".into(), output_or_unresolved(global, keys, diagnostics, &mut unresolved));
    } else {
        let value = f32::from_bits(u32_at(row, 4).unwrap());
        unresolved |= !value.is_finite();
        out.insert("value".into(), json!(if value.is_finite() { value } else { 0.0 }));
    }
    let mut params = Vec::new();
    for offset in [12usize, 16] {
        let raw = u32_at(row, offset).unwrap();
        if !form_offsets.contains(&offset) {
            params.push(json!({ "raw": raw }));
        } else if raw == 0 {
            params.push(Value::Null);
        } else if let Some(source) = keys.source(raw) {
            let form = keys.output(raw);
            if form.is_none() {
                diagnostics.unmapped_forms.insert(source.clone());
            }
            params.push(json!({ "source": source, "form": form }));
        } else {
            unresolved = true;
            params.push(Value::Null);
        }
    }
    out.insert("params".into(), Value::Array(params));
    let run_on = u32_at(row, 20).unwrap();
    let run_on_name = usize::try_from(run_on).ok().and_then(|index| RUN_ONS.get(index)).copied().unwrap_or("unsupported");
    out.insert("run_on".into(), json!(run_on_name));
    if run_on == 2 {
        let reference = u32_at(row, 24).unwrap();
        out.insert("reference".into(), output_or_unresolved(reference, keys, diagnostics, &mut unresolved));
    }
    out.insert("param3".into(), json!(i32::from_le_bytes(row[28..32].try_into().unwrap())));
    if unresolved {
        diagnostics.unresolved_conditions += 1;
        out.insert("unresolved".into(), json!(true));
    }
    Value::Object(out)
}

pub(crate) fn conditions_json(rows: &[[u8; 32]], keys: &dyn Keys, diagnostics: &mut Diagnostics) -> Vec<Value> {
    rows.iter()
        .map(|row| condition_json(row, keys, diagnostics))
        .collect()
}

fn extra_json(extra: &RawExtra, keys: &dyn Keys) -> Value {
    let mut out = Map::new();
    out.insert("health".into(), json!(extra.health));
    if let Some(owner) = (extra.owner != 0).then(|| keys.output(extra.owner)).flatten() {
        out.insert("owner".into(), json!(owner));
        if keys.is_faction(extra.owner) {
            out.insert("rank".into(), json!(extra.global_or_rank as i32));
        } else if let Some(global) = (extra.global_or_rank != 0)
            .then(|| keys.output(extra.global_or_rank))
            .flatten()
        {
            out.insert("global".into(), json!(global));
        }
    }
    Value::Object(out)
}

pub(crate) fn list_json(
    identity: &ListIdentity<'_>,
    list: &RawList,
    keys: &dyn Keys,
    diagnostics: &mut Diagnostics,
) -> Value {
    let mut entries = Vec::new();
    for entry in &list.entries {
        let Some(form) = keys.output(entry.form) else {
            diagnostics.unmapped_entries += 1;
            if let Some(source) = keys.source(entry.form) {
                diagnostics.unmapped_forms.insert(source);
            }
            continue;
        };
        let mut row = json!({
            "form": form,
            "level": value_json(&entry.level, 1.0, keys, diagnostics),
            "count": value_json(&entry.count, 1.0, keys, diagnostics),
            "chance_none": value_json(&entry.chance_none, 0.0, keys, diagnostics),
            "conditions": conditions_json(&entry.conditions, keys, diagnostics),
        });
        if let Some(extra) = &entry.extra {
            row["extra"] = extra_json(extra, keys);
        }
        entries.push(row);
    }
    let mut out = json!({
        "source": identity.source,
        "target": identity.target,
        "editor_id": identity.editor_id,
        "signature": identity.signature,
        "scope": identity.scope,
        "flags": list.flags,
        "chance_none": value_json(&list.chance_none, 0.0, keys, diagnostics),
        "max_count": value_json(&list.max_count, 0.0, keys, diagnostics),
        "conditions": conditions_json(&list.conditions, keys, diagnostics),
        "entries": entries,
    });
    if let Some(quest) = list.quest.and_then(|quest| keys.source(quest)) {
        out["quest"] = json!(quest);
    }
    out
}

#[cfg(test)]
pub(crate) mod fake {
    use std::collections::HashMap;

    use super::Keys;

    #[derive(Default)]
    pub(crate) struct FakeKeys {
        pub outputs: HashMap<u32, String>,
        pub factions: Vec<u32>,
    }

    impl FakeKeys {
        pub(crate) fn with(outputs: &[(u32, &str)]) -> Self {
            Self { outputs: outputs.iter().map(|(raw, key)| (*raw, key.to_string())).collect(), factions: Vec::new() }
        }
    }

    impl Keys for FakeKeys {
        fn source(&self, raw: u32) -> Option<String> {
            (raw >> 24 == 0).then(|| format!("SeventySix.esm:{:06X}", raw & 0xFF_FFFF))
        }
        fn output(&self, raw: u32) -> Option<String> {
            self.outputs.get(&raw).cloned()
        }
        fn is_faction(&self, raw: u32) -> bool {
            self.factions.contains(&raw)
        }
    }
}

#[cfg(test)]
mod tests {
    use esp_authoring_core::plugin_runtime::ParsedSubrecord;

    use super::fake::FakeKeys;
    use super::*;

    fn field(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord { signature: signature.into(), data: data.into(), semantic_type: None }
    }

    fn f32_field(signature: &str, value: f32) -> ParsedSubrecord {
        field(signature, value.to_le_bytes().to_vec())
    }

    fn u32_field(signature: &str, value: u32) -> ParsedSubrecord {
        field(signature, value.to_le_bytes().to_vec())
    }

    fn ctda(flags: u8, comparison: u32, function: u16, param1: u32, run_on: u32, reference: u32) -> Vec<u8> {
        let mut row = vec![0u8; 32];
        row[0] = flags;
        row[4..8].copy_from_slice(&comparison.to_le_bytes());
        row[8..10].copy_from_slice(&function.to_le_bytes());
        row[12..16].copy_from_slice(&param1.to_le_bytes());
        row[20..24].copy_from_slice(&run_on.to_le_bytes());
        row[24..28].copy_from_slice(&reference.to_le_bytes());
        row[28..32].copy_from_slice(&(-1i32).to_le_bytes());
        row
    }

    // Shape of FO76 CreatureOutfit_FeralGhoul (08EC46): first-match, golf branch gated by 579.
    fn golf_outfit() -> Vec<ParsedSubrecord> {
        vec![
            field("EDID", b"CreatureOutfit_FeralGhoul\0".to_vec()),
            field("LVLF", 0x40u16.to_le_bytes().to_vec()),
            field("LLCT", vec![2]),
            u32_field("LVLO", 0x0039_1D85),
            f32_field("LVLV", 1.0),
            f32_field("LVIV", 1.0),
            f32_field("LVOV", 0.0),
            field("CTDA", ctda(0, 1.0f32.to_bits(), 579, 0x003B_3731, 0, 0)),
            u32_field("LVLO", 0x0039_1D86),
            f32_field("LVLV", 1.0),
        ]
    }

    #[test]
    fn conditions_stay_on_their_entry() {
        let list = decode_list(&golf_outfit()).unwrap();
        assert_eq!(list.flags, 0x40);
        assert!(list.conditions.is_empty());
        assert_eq!(list.entries.len(), 2);
        assert_eq!(list.entries[0].form, 0x0039_1D85);
        assert_eq!(list.entries[0].conditions.len(), 1);
        assert!(list.entries[1].conditions.is_empty());
        assert_eq!(list.entries[0].level.value, Some(1.0));
    }

    #[test]
    fn empty_lvld_and_lvlm_are_markers_before_the_float_values() {
        // Form version 209 (Forest_Unique_Items 929F96): LVLD is empty; LVCV/LVMV carry the values.
        let list = decode_list(&[
            field("LVLD", Vec::new()),
            field("LVLM", Vec::new()),
            f32_field("LVMV", 3.0),
            f32_field("LVCV", 25.0),
        ])
        .unwrap();
        assert_eq!(list.chance_none.value, Some(25.0));
        assert_eq!(list.max_count.value, Some(3.0));
        assert_eq!(decode_list(&[field("LVLD", vec![40])]).unwrap().chance_none.value, Some(40.0));
    }

    #[test]
    fn flags_read_one_or_two_bytes() {
        assert_eq!(decode_list(&[field("LVLF", vec![0x08])]).unwrap().flags, 0x08);
        assert_eq!(decode_list(&[field("LVLF", vec![0x00, 0x02])]).unwrap().flags, 0x200);
        assert!(decode_list(&[field("LVLF", vec![1, 2, 3])]).is_err());
    }

    #[test]
    fn conditions_before_the_first_entry_gate_the_list() {
        let list = decode_list(&[
            field("CTDA", ctda(0, 1.0f32.to_bits(), 77, 0, 0, 0)),
            u32_field("LVLO", 0x10),
            field("CTDA", ctda(0, 1.0f32.to_bits(), 72, 0x11, 0, 0)),
        ])
        .unwrap();
        assert_eq!(list.conditions.len(), 1);
        assert_eq!(list.entries[0].conditions.len(), 1);
    }

    #[test]
    fn twelve_byte_entries_carry_level_count_and_chance() {
        let mut lvlo = Vec::new();
        lvlo.extend(15u16.to_le_bytes());
        lvlo.extend([0, 0]);
        lvlo.extend(0x0042_D362u32.to_le_bytes());
        lvlo.extend(2u16.to_le_bytes());
        lvlo.extend([25, 0]);
        let list = decode_list(&[field("LVLO", lvlo)]).unwrap();
        let entry = &list.entries[0];
        assert_eq!((entry.form, entry.level.value, entry.count.value, entry.chance_none.value), (0x0042_D362, Some(15.0), Some(2.0), Some(25.0)));
    }

    #[test]
    fn values_keep_every_part_and_reject_non_finite_floats() {
        let list = decode_list(&[
            u32_field("LVLO", 0x10),
            f32_field("LVOV", 5.0),
            u32_field("LVOC", 0x20),
            u32_field("LVOT", 0x30),
        ])
        .unwrap();
        assert_eq!(list.entries[0].chance_none, RawValue { value: Some(5.0), global: Some(0x20), curve: Some(0x30) });
        assert!(decode_list(&[u32_field("LVLO", 0x10), f32_field("LVOV", f32::NAN)]).is_err());
        assert!(decode_list(&[f32_field("LVOV", 1.0)]).is_err(), "entry field before LVLO");
    }

    #[test]
    fn coed_decodes_owner_and_health() {
        let mut coed = Vec::new();
        coed.extend(0x50u32.to_le_bytes());
        coed.extend(3u32.to_le_bytes());
        coed.extend(0.5f32.to_le_bytes());
        let list = decode_list(&[u32_field("LVLO", 0x10), field("COED", coed)]).unwrap();
        assert_eq!(list.entries[0].extra, Some(RawExtra { owner: 0x50, global_or_rank: 3, health: 0.5 }));
    }

    #[test]
    fn selection_needs_fo76_flags_conditions_or_dynamic_values() {
        let plain = decode_list(&[field("LVLF", vec![0x01]), u32_field("LVLO", 0x10)]).unwrap();
        assert!(!needs_runtime(&plain));
        assert!(needs_runtime(&decode_list(&golf_outfit()).unwrap()));
        assert!(needs_runtime(&decode_list(&[field("LVLF", vec![0x08])]).unwrap()));
        assert!(needs_runtime(&decode_list(&[u32_field("LVLO", 0x10), u32_field("LVIG", 0x20)]).unwrap()));
        assert!(needs_runtime(&decode_list(&[u32_field("LVLG", 0x20)]).unwrap()));
    }

    #[test]
    fn keyword_condition_names_source_and_output() {
        let keys = FakeKeys::with(&[(0x003B_3731, "SeventySix.esm:3B3731")]);
        let mut diagnostics = Diagnostics::default();
        let row: [u8; 32] = ctda(0x01, 1.0f32.to_bits(), 579, 0x003B_3731, 0, 0).try_into().unwrap();
        let json = condition_json(&row, &keys, &mut diagnostics);
        assert_eq!(json["function"], 579);
        assert_eq!(json["op"], "==");
        assert_eq!(json["or"], true);
        assert_eq!(json["value"], 1.0);
        assert_eq!(json["params"][0]["source"], "SeventySix.esm:3B3731");
        assert_eq!(json["params"][0]["form"], "SeventySix.esm:3B3731");
        assert_eq!(json["run_on"], "subject");
        assert!(json.get("unresolved").is_none());
    }

    #[test]
    fn raw_parameters_stay_numbers() {
        let keys = FakeKeys::default();
        let mut diagnostics = Diagnostics::default();
        // 77 GetRandomPercent takes no form parameter.
        let row: [u8; 32] = ctda(0x60, 50.0f32.to_bits(), 77, 7, 0, 0).try_into().unwrap();
        let json = condition_json(&row, &keys, &mut diagnostics);
        assert_eq!(json["op"], ">=");
        assert_eq!(json["params"][0]["raw"], 7);
    }

    #[test]
    fn alias_and_package_flags_matter_only_with_a_reference_parameter() {
        let keys = FakeKeys::with(&[(0x20, "SeventySix.esm:000020")]);
        let mut diagnostics = Diagnostics::default();
        // LChar_MainAll (51C55B) entry 1: 77 GetRandomPercent under 0x87 (alias + global + OR).
        let random: [u8; 32] = ctda(0x87, 0x20, 77, 0, 0, 0).try_into().unwrap();
        assert_eq!(condition_json(&random, &keys, &mut diagnostics)["unsupported_flags"], 0);
        // 1 GetDistance: the alias flag turns its reference into an alias index.
        let distance: [u8; 32] = ctda(0x02, 1.0f32.to_bits(), 1, 3, 0, 0).try_into().unwrap();
        assert_eq!(condition_json(&distance, &keys, &mut diagnostics)["unsupported_flags"], 0x02);
        let packdata: [u8; 32] = ctda(0x08, 1.0f32.to_bits(), 72, 0x20, 0, 0).try_into().unwrap();
        assert_eq!(condition_json(&packdata, &keys, &mut diagnostics)["unsupported_flags"], 0x08);
    }

    #[test]
    fn unrepresentable_conditions_are_marked_unresolved() {
        let keys = FakeKeys::default();
        let mut diagnostics = Diagnostics::default();
        let global: [u8; 32] = ctda(0x04, 0x99, 74, 0x99, 0, 0).try_into().unwrap();
        assert_eq!(condition_json(&global, &keys, &mut diagnostics)["unresolved"], true);
        let bad_operator: [u8; 32] = ctda(0xE0, 1.0f32.to_bits(), 77, 0, 0, 0).try_into().unwrap();
        assert_eq!(condition_json(&bad_operator, &keys, &mut diagnostics)["unresolved"], true);
        let nan: [u8; 32] = ctda(0, f32::NAN.to_bits(), 77, 0, 0, 0).try_into().unwrap();
        assert_eq!(condition_json(&nan, &keys, &mut diagnostics)["unresolved"], true);
        let reference: [u8; 32] = ctda(0, 1.0f32.to_bits(), 72, 0x12, 2, 0x77).try_into().unwrap();
        let json = condition_json(&reference, &keys, &mut diagnostics);
        assert_eq!(json["run_on"], "reference");
        assert_eq!(json["unresolved"], true);
        assert_eq!(diagnostics.unresolved_conditions, 4);
    }

    #[test]
    fn unmapped_entries_are_omitted_and_counted() {
        let keys = FakeKeys::with(&[(0x0039_1D86, "SeventySix.esm:391D86"), (0x003B_3731, "SeventySix.esm:3B3731")]);
        let mut diagnostics = Diagnostics::default();
        let list = decode_list(&golf_outfit()).unwrap();
        let identity = ListIdentity {
            source: "SeventySix.esm:08EC46",
            target: "Fallout4.esm:08EC46",
            editor_id: "CreatureOutfit_FeralGhoul",
            signature: "LVLI",
            scope: "seventysix",
        };
        let json = list_json(&identity, &list, &keys, &mut diagnostics);
        assert_eq!(json["scope"], "seventysix");
        assert_eq!(json["flags"], 64);
        assert_eq!(json["entries"].as_array().unwrap().len(), 1);
        assert_eq!(json["entries"][0]["form"], "SeventySix.esm:391D86");
        assert_eq!(diagnostics.unmapped_entries, 1);
        assert!(diagnostics.unmapped_forms.contains("SeventySix.esm:391D85"));
    }

    #[test]
    fn faction_owners_carry_a_rank() {
        let mut keys = FakeKeys::with(&[(0x10, "SeventySix.esm:000010"), (0x50, "SeventySix.esm:000050")]);
        keys.factions.push(0x50);
        let mut coed = Vec::new();
        coed.extend(0x50u32.to_le_bytes());
        coed.extend(2u32.to_le_bytes());
        coed.extend(1.0f32.to_le_bytes());
        let list = decode_list(&[u32_field("LVLO", 0x10), field("COED", coed)]).unwrap();
        let identity = ListIdentity { source: "a", target: "b", editor_id: "c", signature: "LVLI", scope: "always" };
        let json = list_json(&identity, &list, &keys, &mut Diagnostics::default());
        assert_eq!(json["entries"][0]["extra"]["owner"], "SeventySix.esm:000050");
        assert_eq!(json["entries"][0]["extra"]["rank"], 2);
    }

    #[test]
    fn run_ons_keep_their_fo76_names() {
        let keys = FakeKeys::default();
        let mut diagnostics = Diagnostics::default();
        for (run_on, name) in [(11, "active_players"), (12, "potential_players"), (15, "instance_owner"), (16, "unsupported")] {
            let row: [u8; 32] = ctda(0, 1.0f32.to_bits(), 47, 0, run_on, 0).try_into().unwrap();
            assert_eq!(condition_json(&row, &keys, &mut diagnostics)["run_on"], name);
        }
    }

    #[test]
    fn a_list_quest_is_kept_by_source_key() {
        // DIQO scopes ActivePlayers/PotentialPlayers to the player's instance of that quest.
        let list = decode_list(&[u32_field("DIQO", 0x0062_0F7B), u32_field("LVLO", 0x10)]).unwrap();
        assert_eq!(list.quest, Some(0x0062_0F7B));
        let identity = ListIdentity { source: "a", target: "b", editor_id: "c", signature: "LVLI", scope: "always" };
        let keys = FakeKeys::with(&[(0x10, "SeventySix.esm:000010")]);
        let json = list_json(&identity, &list, &keys, &mut Diagnostics::default());
        assert_eq!(json["quest"], "SeventySix.esm:620F7B");
        assert!(list_json(&identity, &decode_list(&[]).unwrap(), &keys, &mut Diagnostics::default()).get("quest").is_none());
    }

    #[test]
    fn parameter_2_finds_the_region_of_a_weather_check() {
        // LLS_Creature_Large: 149 GetIsCurrentWeather(Storm_NewWeather_Nuke_New 761063, StormRegion 6923E9).
        let mut row = ctda(0, 1.0f32.to_bits(), 149, 0x0076_1063, 0, 0);
        row[16..20].copy_from_slice(&0x0069_23E9u32.to_le_bytes());
        let row: [u8; 32] = row.try_into().unwrap();
        let mut regions = std::collections::BTreeSet::new();
        parameter_2_of(&[row], 149, &mut regions);
        assert_eq!(regions.into_iter().collect::<Vec<_>>(), vec![0x0069_23E9]);
    }

    #[test]
    fn regions_keep_each_polygon() {
        let mut points = Vec::new();
        for value in [0.0f32, 0.0, 4096.0, 0.0, 4096.0, 4096.0] {
            points.extend(value.to_le_bytes());
        }
        let region = decode_region(&[u32_field("WNAM", 0x25DA15), u32_field("RPLI", 0), field("RPLD", points)]);
        assert_eq!(region.world, Some(0x25DA15));
        assert_eq!(region.polygons, vec![vec![[0.0, 0.0], [4096.0, 0.0], [4096.0, 4096.0]]]);
    }

    #[test]
    fn whole_template_needs_every_slot_from_one_record() {
        let tpta = |slots: &[u32]| field("TPTA", slots.iter().flat_map(|raw| raw.to_le_bytes()).collect());
        assert_eq!(whole_template(&[tpta(&[0x51C55B; 14]), u32_field("TPLT", 0x51C55B)]), Some(0x51C55B));
        let mut own_factions = [0x6529FB; 14];
        own_factions[2] = 0;
        assert_eq!(whole_template(&[tpta(&own_factions)]), None);
        let mut other_inventory = [0x6529FB; 14];
        other_inventory[8] = 0x65322F;
        assert_eq!(whole_template(&[tpta(&other_inventory)]), None);
        assert_eq!(whole_template(&[tpta(&[0; 14]), u32_field("TPLT", 0x99)]), None);
        assert_eq!(whole_template(&[u32_field("TPLT", 0x99)]), None);
        assert_eq!(whole_template(&[]), None);
    }

    #[test]
    fn references_cover_entries_values_conditions_and_extras() {
        let list = decode_list(&golf_outfit()).unwrap();
        let mut refs = std::collections::BTreeSet::new();
        list_references(&list, &mut refs);
        assert!(refs.contains(&0x0039_1D85) && refs.contains(&0x0039_1D86) && refs.contains(&0x003B_3731));
        let mut keywords = std::collections::BTreeSet::new();
        parameter_1_of(&conditions_of(&list), 579, &mut keywords);
        assert_eq!(keywords.into_iter().collect::<Vec<_>>(), vec![0x003B_3731]);
    }
}
