//! Restore the record data that four recovered FO76 scripts depend on.
//!
//! Each repair is pinned to one record by local ID, signature and EditorID:
//!
//! - `Burn_RE_ThirstyPerson_Flee` (PACK 856BB5) gates itself on two hardcoded
//!   actor values, Confidence (0x2BD) and Variable01 (0x301). FO76 hardcoded AV
//!   references are mapped into the output plugin and then nulled as dangling,
//!   which removes both conditions and the script's `ConditionalAV`. The FO4
//!   engine registers the same AVs at the same IDs, so they are re-pointed at
//!   Fallout4.esm.
//! - `Burn_RE_Scene_ThirstyPerson` (QUST 81ACB2) loses its fragment-lane
//!   Confidence and Aggression properties the same way.
//! - `AC_MQ_Dialogue_VinRusso` (QUST 7456A7) was a FO76 quest module. Its Loc
//!   alias is an event-data fill, which a script `Start()` can never satisfy,
//!   and Actor_Vin fills from Loc. Both become Optional so the recovered parent
//!   fragments can start the quest and force Loc and Actor_Vin themselves.
//! - Four `DefaultTeachRecipeOnMenuItemRun` terminals teach recipes that FO4
//!   gates on `B21_PlanLearned_*` globals. Each terminal gains a
//!   `LearnedGlobals` array aligned with its `ValueSet` rows.

use bytes::Bytes;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::{
    ParsedRecord, ParsedSubrecord, WriteEffect, build_vmad_bytes_from_payload,
};
use serde_json::{Value, json};
use smallvec::smallvec;
use smol_str::SmolStr;

use crate::fixups::{FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::session::PluginSession;

const FLEE_PACKAGE: (u32, &str) = (0x856BB5, "Burn_RE_ThirstyPerson_Flee");
const FLEE_PACKAGE_SCRIPT: &str = "Burn_RE_FleePackageScript";
const THIRSTY_PERSON_QUEST: (u32, &str) = (0x81ACB2, "Burn_RE_Scene_ThirstyPerson");
const VIN_RUSSO_MODULE_QUEST: (u32, &str) = (0x7456A7, "AC_MQ_Dialogue_VinRusso");
const VIN_RUSSO_ALIASES: [u32; 2] = [1, 2];
const RECIPE_TERMINALS: [(u32, &str); 4] = [
    (0x518949, "EN07_CommandArchivalTerminalSchematicSub"),
    (0x3AAB79, "MTR06_SystemTerminal_Schematics"),
    (0x275E98, "MTR01_DominickSubterminal"),
    (0x4F6806, "RSVP00_Terminal_Miguel_Main"),
];
const TEACH_RECIPE_SCRIPT: &str = "DefaultTeachRecipeOnMenuItemRun";
const PLAN_LEARNED_PREFIX: &str = "B21_PlanLearned_";

const AV_AGGRESSION: u32 = 0x2BC;
const AV_CONFIDENCE: u32 = 0x2BD;
const AV_VARIABLE01: u32 = 0x301;
const FUNCTION_GET_ACTOR_VALUE: u16 = 14;
const FUNCTION_GET_GLOBAL_VALUE: u16 = 74;
const ALIAS_FLAG_OPTIONAL: u32 = 0x2;
const VMAD_PROPERTY_FLAG_EDITED: u64 = 1;

struct Context<'a> {
    masters: &'a [String],
    plugin: &'a str,
    fallout4: u32,
}

impl Context<'_> {
    fn object(&self, raw: u32) -> Option<Value> {
        let index = (raw >> 24) as usize;
        let plugin = match index.cmp(&self.masters.len()) {
            std::cmp::Ordering::Less => self.masters[index].as_str(),
            std::cmp::Ordering::Equal => self.plugin,
            std::cmp::Ordering::Greater => return None,
        };
        Some(json!({
            "Alias": -1,
            "FormID": {"reference": {"plugin": plugin, "object_id": format!("{:06X}", raw & 0x00FF_FFFF)}},
        }))
    }

    fn raw(&self, object: &Value) -> Option<u32> {
        let reference = object.get("FormID")?.get("reference")?;
        let plugin = reference.get("plugin")?.as_str()?;
        let local = u32::from_str_radix(reference.get("object_id")?.as_str()?, 16).ok()?;
        let index = if plugin.eq_ignore_ascii_case(self.plugin) {
            self.masters.len()
        } else {
            self.masters
                .iter()
                .position(|master| master.eq_ignore_ascii_case(plugin))?
        };
        Some(((index as u32) << 24) | local)
    }

    fn fallout4_av(&self, local: u32) -> u32 {
        self.fallout4 | local
    }
}

fn editor_id(record: &ParsedRecord) -> Option<&str> {
    let data = record
        .subrecords
        .iter()
        .find(|subrecord| subrecord.signature.as_str() == "EDID")?
        .data
        .as_ref();
    let end = data.iter().position(|byte| *byte == 0).unwrap_or(data.len());
    std::str::from_utf8(&data[..end]).ok()
}

fn is_target(record: &ParsedRecord, signature: &str, eid: &str) -> bool {
    record.signature.as_str() == signature && editor_id(record) == Some(eid)
}

fn read_u32(data: &[u8], offset: usize) -> Option<u32> {
    data.get(offset..offset + 4)
        .map(|bytes| u32::from_le_bytes(bytes.try_into().expect("four-byte slice")))
}

fn condition_function(data: &[u8]) -> Option<u16> {
    data.get(8..10)
        .map(|bytes| u16::from_le_bytes(bytes.try_into().expect("two-byte slice")))
}

/// FO4 CTDA: `<subject>.GetActorValue(actor_value) == 0`, AND-joined.
fn actor_value_is_zero_condition(actor_value: u32) -> Vec<u8> {
    let mut data = Vec::with_capacity(32);
    data.extend_from_slice(&[0, 0, 0, 0]);
    data.extend_from_slice(&0f32.to_le_bytes());
    data.extend_from_slice(&FUNCTION_GET_ACTOR_VALUE.to_le_bytes());
    data.extend_from_slice(&[0, 0]);
    data.extend_from_slice(&actor_value.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&(-1i32).to_le_bytes());
    data
}

/// Inserts the flee package's two conditions after PSDT. Leaves a package
/// that already carries any condition untouched.
fn restore_flee_conditions(record: &mut ParsedRecord, ctx: &Context) -> bool {
    if record
        .subrecords
        .iter()
        .any(|subrecord| subrecord.signature.as_str() == "CTDA")
    {
        return false;
    }
    let Some(schedule) = record
        .subrecords
        .iter()
        .position(|subrecord| subrecord.signature.as_str() == "PSDT")
    else {
        return false;
    };
    let conditions = [AV_CONFIDENCE, AV_VARIABLE01].map(|local| ParsedSubrecord {
        signature: SmolStr::new("CTDA"),
        data: Bytes::from(actor_value_is_zero_condition(ctx.fallout4_av(local))),
        semantic_type: None,
    });
    record
        .subrecords
        .splice(schedule + 1..schedule + 1, conditions);
    true
}

fn mark_aliases_optional(record: &mut ParsedRecord, alias_ids: &[u32]) -> bool {
    let mut current_alias = None;
    let mut changed = false;
    for subrecord in &mut record.subrecords {
        match subrecord.signature.as_str() {
            "ALST" | "ALLS" | "ALCS" => current_alias = read_u32(&subrecord.data, 0),
            "ALED" => current_alias = None,
            "FNAM" if current_alias.is_some_and(|id| alias_ids.contains(&id)) => {
                let Some(flags) = read_u32(&subrecord.data, 0) else {
                    continue;
                };
                if flags & ALIAS_FLAG_OPTIONAL == 0 {
                    let mut data = subrecord.data.to_vec();
                    data[..4].copy_from_slice(&(flags | ALIAS_FLAG_OPTIONAL).to_le_bytes());
                    subrecord.data = Bytes::from(data);
                    changed = true;
                }
            }
            _ => {}
        }
    }
    changed
}

fn script_mut<'a>(scripts: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    scripts.as_array_mut()?.iter_mut().find(|script| {
        script
            .get("ScriptName")
            .and_then(Value::as_str)
            .is_some_and(|script_name| script_name.eq_ignore_ascii_case(name))
    })
}

fn property_mut<'a>(script: &'a mut Value, name: &str) -> Option<&'a mut Value> {
    script
        .get_mut("Properties")?
        .as_array_mut()?
        .iter_mut()
        .find(|property| {
            property
                .get("propertyName")
                .and_then(Value::as_str)
                .is_some_and(|property_name| property_name.eq_ignore_ascii_case(name))
        })
}

/// Fills an Object property whose FormID was nulled; never overwrites a bound one.
fn bind_null_object(script: &mut Value, name: &str, object: Value) -> bool {
    let Some(property) = property_mut(script, name) else {
        return false;
    };
    let is_null = property
        .get("Value")
        .and_then(|value| value.get("FormID"))
        .is_some_and(Value::is_null);
    if is_null {
        property["Value"] = object;
    }
    is_null
}

fn bind_flee_package_av(vmad: &mut Value, ctx: &Context) -> bool {
    let Some(object) = ctx.object(ctx.fallout4_av(AV_VARIABLE01)) else {
        return false;
    };
    script_mut(&mut vmad["Scripts"], FLEE_PACKAGE_SCRIPT)
        .is_some_and(|script| bind_null_object(script, "ConditionalAV", object))
}

fn bind_thirsty_person_avs(vmad: &mut Value, ctx: &Context) -> bool {
    let Some(script) = vmad
        .get_mut("Script Fragments")
        .and_then(|fragments| fragments.get_mut("Script"))
    else {
        return false;
    };
    let mut changed = false;
    for (name, local) in [("Confidence", AV_CONFIDENCE), ("Aggression", AV_AGGRESSION)] {
        if let Some(object) = ctx.object(ctx.fallout4_av(local)) {
            changed |= bind_null_object(script, name, object);
        }
    }
    changed
}

fn recipe_rows(vmad: &Value, ctx: &Context) -> Option<Vec<Option<u32>>> {
    let script = vmad
        .get("Scripts")?
        .as_array()?
        .iter()
        .find(|script| {
            script
                .get("ScriptName")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(TEACH_RECIPE_SCRIPT))
        })?;
    let rows = script
        .get("Properties")?
        .as_array()?
        .iter()
        .find(|property| property.get("propertyName").and_then(Value::as_str) == Some("ValueSet"))?
        .get("Value")?
        .as_array()?;
    Some(
        rows.iter()
            .map(|row| {
                row.as_array()?
                    .iter()
                    .find(|member| {
                        member.get("memberName").and_then(Value::as_str) == Some("RecipeToTeach")
                    })
                    .and_then(|member| ctx.raw(member.get("Value")?))
            })
            .collect(),
    )
}

/// Sets `LearnedGlobals` to one entry per `ValueSet` row: the row recipe's
/// plan-learned global, or None for a recipe FO4 does not gate.
fn bind_learned_globals(vmad: &mut Value, learned: &[Option<u32>], ctx: &Context) -> bool {
    let values: Vec<Value> = learned
        .iter()
        .map(|global| {
            global
                .and_then(|raw| ctx.object(raw))
                .unwrap_or_else(|| json!({"Alias": -1, "FormID": null}))
        })
        .collect();
    let property = json!({
        "propertyName": "LearnedGlobals",
        "Type": "Array of Object",
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": values,
    });
    let Some(script) = script_mut(&mut vmad["Scripts"], TEACH_RECIPE_SCRIPT) else {
        return false;
    };
    if let Some(existing) = property_mut(script, "LearnedGlobals") {
        if *existing == property {
            return false;
        }
        *existing = property;
        return true;
    }
    let Some(properties) = script.get_mut("Properties").and_then(Value::as_array_mut) else {
        return false;
    };
    properties.push(property);
    true
}

fn plan_learned_global(session: &mut PluginSession, recipe: u32) -> Option<u32> {
    let globals: Vec<u32> = {
        let record = session.record(recipe).ok()?;
        if record.signature.as_str() != "COBJ" {
            return None;
        }
        record
            .subrecords
            .iter()
            .filter(|subrecord| subrecord.signature.as_str() == "CTDA")
            .filter(|subrecord| condition_function(&subrecord.data) == Some(FUNCTION_GET_GLOBAL_VALUE))
            .filter_map(|subrecord| read_u32(&subrecord.data, 12))
            .collect()
    };
    globals.into_iter().find(|global| {
        session.record(*global).is_ok_and(|record| {
            record.signature.as_str() == "GLOB"
                && editor_id(record).is_some_and(|eid| eid.starts_with(PLAN_LEARNED_PREFIX))
        })
    })
}

fn vmad_index(record: &ParsedRecord) -> Option<usize> {
    record
        .subrecords
        .iter()
        .position(|subrecord| subrecord.signature.as_str() == "VMAD")
}

fn decode_vmad(record: &ParsedRecord, ctx: &Context) -> Option<(usize, Value)> {
    let index = vmad_index(record)?;
    let payload = compact_vmad_payload_json(
        &record.subrecords[index].data,
        ctx.masters,
        ctx.plugin,
        Some(record.signature.as_str()),
    )?;
    if payload.get("tail_hex").is_some() {
        return None;
    }
    Some((index, payload))
}

fn encode_vmad(record: &mut ParsedRecord, index: usize, payload: &Value, ctx: &Context) -> bool {
    let Some(bytes) = build_vmad_bytes_from_payload(payload, ctx.masters, ctx.plugin) else {
        return false;
    };
    record.subrecords[index].data = Bytes::from(bytes);
    true
}

fn commit(session: &mut PluginSession, raw: u32, report: &mut FixupReport) {
    session.record_effect(WriteEffect::RecordContents {
        form_ids: smallvec![raw],
    });
    report.records_changed = report.records_changed.saturating_add(1);
}

pub fn repair_fo76_record_dependencies(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    _config: &FixupConfig,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let masters = session.target_masters().to_vec();
    let own = masters.len() as u32;
    if own > u8::MAX as u32 {
        return Ok(report);
    }
    let Some(fallout4_index) = masters
        .iter()
        .position(|master| master.eq_ignore_ascii_case("Fallout4.esm"))
    else {
        return Ok(report);
    };
    let output_plugin = mapper.output_plugin_sym();
    let Some(plugin) = mapper.interner.resolve(output_plugin).map(str::to_string) else {
        return Ok(report);
    };
    let ctx = Context {
        masters: &masters,
        plugin: &plugin,
        fallout4: (fallout4_index as u32) << 24,
    };
    let own = own << 24;
    let warn = |report: &mut FixupReport, local: u32, reason: &str| {
        report.warnings.push(
            mapper
                .interner
                .intern(&format!("repair_fo76_record_dependencies:{local:06X}:{reason}")),
        );
    };

    let raw = own | FLEE_PACKAGE.0;
    if let Ok(record) = session.record_mut(raw)
        && is_target(record, "PACK", FLEE_PACKAGE.1)
    {
        let mut changed = restore_flee_conditions(record, &ctx);
        if let Some((index, mut vmad)) = decode_vmad(record, &ctx)
            && bind_flee_package_av(&mut vmad, &ctx)
        {
            changed |= encode_vmad(record, index, &vmad, &ctx);
        }
        if changed {
            record.raw_payload = None;
            commit(session, raw, &mut report);
        }
    } else {
        warn(&mut report, FLEE_PACKAGE.0, "missing_or_mismatched");
    }

    let raw = own | THIRSTY_PERSON_QUEST.0;
    if let Ok(record) = session.record_mut(raw)
        && is_target(record, "QUST", THIRSTY_PERSON_QUEST.1)
    {
        if let Some((index, mut vmad)) = decode_vmad(record, &ctx)
            && bind_thirsty_person_avs(&mut vmad, &ctx)
            && encode_vmad(record, index, &vmad, &ctx)
        {
            record.raw_payload = None;
            commit(session, raw, &mut report);
        }
    } else {
        warn(&mut report, THIRSTY_PERSON_QUEST.0, "missing_or_mismatched");
    }

    let raw = own | VIN_RUSSO_MODULE_QUEST.0;
    if let Ok(record) = session.record_mut(raw)
        && is_target(record, "QUST", VIN_RUSSO_MODULE_QUEST.1)
    {
        if mark_aliases_optional(record, &VIN_RUSSO_ALIASES) {
            record.raw_payload = None;
            commit(session, raw, &mut report);
        }
    } else {
        warn(&mut report, VIN_RUSSO_MODULE_QUEST.0, "missing_or_mismatched");
    }

    for (local, eid) in RECIPE_TERMINALS {
        let raw = own | local;
        let rows = match session.record(raw) {
            Ok(record) if is_target(record, "TERM", eid) => {
                decode_vmad(record, &ctx).and_then(|(_, vmad)| recipe_rows(&vmad, &ctx))
            }
            _ => None,
        };
        let Some(rows) = rows else {
            warn(&mut report, local, "missing_or_mismatched");
            continue;
        };
        let learned: Vec<Option<u32>> = rows
            .into_iter()
            .map(|recipe| recipe.and_then(|recipe| plan_learned_global(session, recipe)))
            .collect();
        let record = session
            .record_mut(raw)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if let Some((index, mut vmad)) = decode_vmad(record, &ctx)
            && bind_learned_globals(&mut vmad, &learned, &ctx)
            && encode_vmad(record, index, &vmad, &ctx)
        {
            record.raw_payload = None;
            commit(session, raw, &mut report);
        }
    }

    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    const PLUGIN: &str = "SeventySix.esm";

    fn masters() -> Vec<String> {
        vec!["Fallout4.esm".to_string()]
    }

    fn ctx(masters: &[String]) -> Context<'_> {
        Context {
            masters,
            plugin: PLUGIN,
            fallout4: 0,
        }
    }

    fn subrecord(signature: &str, data: Vec<u8>) -> ParsedSubrecord {
        ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::from(data),
            semantic_type: None,
        }
    }

    fn zstring(value: &str) -> Vec<u8> {
        let mut data = value.as_bytes().to_vec();
        data.push(0);
        data
    }

    fn record(signature: &str, eid: &str, subrecords: Vec<ParsedSubrecord>) -> ParsedRecord {
        let mut all = vec![subrecord("EDID", zstring(eid))];
        all.extend(subrecords);
        ParsedRecord {
            signature: SmolStr::new(signature),
            form_id: 0x0100_0000,
            flags: 0,
            version_control: 0,
            form_version: Some(131),
            version2: None,
            subrecords: all,
            raw_payload: Some(Bytes::from_static(b"stale")),
            parse_error: None,
        }
    }

    fn null_object() -> Value {
        json!({"Alias": -1, "FormID": null})
    }

    fn object(plugin: &str, id: &str) -> Value {
        json!({"Alias": -1, "FormID": {"reference": {"plugin": plugin, "object_id": id}}})
    }

    fn object_property(name: &str, value: Value) -> Value {
        json!({"propertyName": name, "Type": "Object", "Flags": 1, "Value": value})
    }

    #[test]
    fn flee_package_gets_both_actor_value_conditions_after_its_schedule() {
        let masters = masters();
        let mut package = record(
            "PACK",
            FLEE_PACKAGE.1,
            vec![
                subrecord("PKDT", vec![0; 12]),
                subrecord("PSDT", vec![0; 12]),
                subrecord("PKCU", vec![0; 12]),
            ],
        );

        assert!(restore_flee_conditions(&mut package, &ctx(&masters)));
        let signatures: Vec<&str> = package.subrecords.iter().map(|s| s.signature.as_str()).collect();
        assert_eq!(signatures, ["EDID", "PKDT", "PSDT", "CTDA", "CTDA", "PKCU"]);
        let conditions: Vec<&[u8]> = package.subrecords[3..5].iter().map(|s| s.data.as_ref()).collect();
        for (condition, actor_value) in conditions.iter().zip([0x2BD, 0x301]) {
            assert_eq!(condition.len(), 32);
            assert_eq!(condition_function(condition), Some(FUNCTION_GET_ACTOR_VALUE));
            assert_eq!(read_u32(condition, 12), Some(actor_value));
            assert_eq!(&condition[4..8], &0f32.to_le_bytes());
            assert_eq!(read_u32(condition, 28), Some(u32::MAX));
        }

        assert!(!restore_flee_conditions(&mut package, &ctx(&masters)));
    }

    #[test]
    fn flee_package_condition_av_rebinds_only_when_nulled() {
        let masters = masters();
        let mut vmad = json!({"Scripts": [{
            "ScriptName": FLEE_PACKAGE_SCRIPT,
            "Properties": [object_property("ConditionalAV", null_object())],
        }]});
        assert!(bind_flee_package_av(&mut vmad, &ctx(&masters)));
        assert_eq!(
            vmad["Scripts"][0]["Properties"][0]["Value"],
            object("Fallout4.esm", "000301"),
        );
        assert!(!bind_flee_package_av(&mut vmad, &ctx(&masters)));
    }

    #[test]
    fn thirsty_person_fragment_avs_bind_to_fallout4_confidence_and_aggression() {
        let masters = masters();
        let bound = object(PLUGIN, "81ACB2");
        let mut vmad = json!({"Scripts": [], "Script Fragments": {"Script": {
            "ScriptName": "Fragments:Quests:QF_Burn_RE_Scene_ThirstyPers_0081ACB2",
            "Properties": [
                object_property("Aggression", null_object()),
                object_property("Confidence", null_object()),
                object_property("PlayerEnemyFaction", bound.clone()),
            ],
        }}});

        assert!(bind_thirsty_person_avs(&mut vmad, &ctx(&masters)));
        let properties = &vmad["Script Fragments"]["Script"]["Properties"];
        assert_eq!(properties[0]["Value"], object("Fallout4.esm", "0002BC"));
        assert_eq!(properties[1]["Value"], object("Fallout4.esm", "0002BD"));
        assert_eq!(properties[2]["Value"], bound);
    }

    #[test]
    fn vin_russo_module_aliases_become_optional_without_touching_other_flags() {
        let mut quest = record(
            "QUST",
            VIN_RUSSO_MODULE_QUEST.1,
            vec![
                subrecord("ALLS", 1u32.to_le_bytes().to_vec()),
                subrecord("FNAM", 0u32.to_le_bytes().to_vec()),
                subrecord("ALED", vec![]),
                subrecord("ALST", 2u32.to_le_bytes().to_vec()),
                subrecord("FNAM", 0x0001_0040u32.to_le_bytes().to_vec()),
                subrecord("ALED", vec![]),
                subrecord("ALST", 3u32.to_le_bytes().to_vec()),
                subrecord("FNAM", 0u32.to_le_bytes().to_vec()),
                subrecord("ALED", vec![]),
            ],
        );

        assert!(mark_aliases_optional(&mut quest, &VIN_RUSSO_ALIASES));
        let flags: Vec<u32> = quest
            .subrecords
            .iter()
            .filter(|s| s.signature.as_str() == "FNAM")
            .filter_map(|s| read_u32(&s.data, 0))
            .collect();
        assert_eq!(flags, [0x2, 0x0001_0042, 0]);
        assert!(!mark_aliases_optional(&mut quest, &VIN_RUSSO_ALIASES));
    }

    #[test]
    fn recipe_terminal_learned_globals_align_with_value_set_rows() {
        let masters = masters();
        let row = |recipe: Value| {
            json!([
                {"memberName": "iMenuItemTarget", "Type": "Int32", "Flags": 1, "Value": 2},
                {"memberName": "RecipeToTeach", "Type": "Object", "Flags": 1, "Value": recipe},
            ])
        };
        let mut vmad = json!({"Scripts": [{
            "ScriptName": TEACH_RECIPE_SCRIPT,
            "Properties": [{
                "propertyName": "ValueSet",
                "Type": "Array of Struct",
                "Flags": 1,
                "Value": [
                    row(object(PLUGIN, "3C343B")),
                    row(object(PLUGIN, "3C3442")),
                    row(object("Fallout4.esm", "05A0D1")),
                ],
            }],
        }]});

        let ctx = ctx(&masters);
        assert_eq!(
            recipe_rows(&vmad, &ctx).unwrap(),
            [Some(0x013C_343B), Some(0x013C_3442), Some(0x0005_A0D1)],
        );
        let learned = [None, Some(0x01B0_7BA3), None];
        assert!(bind_learned_globals(&mut vmad, &learned, &ctx));
        let property = &vmad["Scripts"][0]["Properties"][1];
        assert_eq!(property["propertyName"], "LearnedGlobals");
        assert_eq!(property["Type"], "Array of Object");
        assert_eq!(
            property["Value"],
            json!([null_object(), object(PLUGIN, "B07BA3"), null_object()]),
        );
        assert!(!bind_learned_globals(&mut vmad, &learned, &ctx));
    }

    #[test]
    fn learned_globals_vmad_round_trips_through_the_binary_encoder() {
        let masters = masters();
        let ctx = ctx(&masters);
        let mut vmad = json!({
            "Version": 6,
            "Object Format": 2,
            "Scripts": [{
                "ScriptName": TEACH_RECIPE_SCRIPT,
                "Flags": 0,
                "Properties": [{"propertyName": "DejaChannel", "Type": "String", "Flags": 1, "Value": "MTR06"}],
            }],
        });
        assert!(bind_learned_globals(&mut vmad, &[Some(0x01B0_7BA3), None], &ctx));
        let bytes = build_vmad_bytes_from_payload(&vmad, &masters, PLUGIN).unwrap();
        let decoded = compact_vmad_payload_json(&bytes, &masters, PLUGIN, Some("TERM")).unwrap();
        assert_eq!(
            decoded["Scripts"][0]["Properties"][1]["Value"],
            json!([object(PLUGIN, "B07BA3"), null_object()]),
        );
    }
}
