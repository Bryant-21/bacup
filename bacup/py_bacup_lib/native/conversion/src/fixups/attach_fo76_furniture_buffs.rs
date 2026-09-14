use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use serde_json::{Value, json};

use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const SOURCE_SCRIPT: &str = "SURV_PlayerUseFurnitureScript";
const SCRIPT_NAME: &str = "B21:FurnitureBuff";

mod perks;

#[derive(Clone, Debug, PartialEq)]
struct BuffSpec {
    keyword: FormKey,
    spell: FormKey,
    wait_time: f64,
}

pub struct AttachFo76FurnitureBuffsFixup;

impl Fixup for AttachFo76FurnitureBuffsFixup {
    fn name(&self) -> &'static str {
        "attach_fo76_furniture_buffs"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = perks::repair_buff_effects(session, mapper)?;
        let Some(source) = session.source_slot_opt() else {
            return Ok(report);
        };
        let source_plugin = source.parsed.plugin_name.clone();
        let source_masters = source.parsed.header.masters.clone();
        let source_schema = session.source_schema().map_err(handle_error)?;
        let player = FormKey {
            plugin: mapper.interner.intern(&source_plugin),
            local: 7,
        };
        let Ok(player) = session.source_record_decoded(&player, &source_schema, mapper.interner)
        else {
            return Ok(report);
        };
        let Some(FieldValue::Bytes(vmad)) = field(&player, b"VMAD") else {
            return Ok(report);
        };
        let Some(payload) =
            compact_vmad_payload_json(vmad, &source_masters, &source_plugin, Some("NPC_"))
        else {
            return Err(FixupError::Other(
                "cannot decode source player furniture buffs".into(),
            ));
        };
        let target_schema = session.schema().map_err(handle_error)?;
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let mut specs = Vec::new();
        for source in source_buff_specs(&payload, mapper.interner) {
            let (Some(keyword), Some(spell)) =
                (mapper.lookup(source.keyword), mapper.lookup(source.spell))
            else {
                report.warnings.push(
                    mapper
                        .interner
                        .intern("furniture_buff:unmapped_keyword_or_spell"),
                );
                continue;
            };
            let valid = [(keyword, SigCode(*b"KYWD")), (spell, SigCode(*b"SPEL"))]
                .into_iter()
                .all(|(fk, sig)| {
                    session
                        .record_decoded(&fk, &target_schema, mapper.interner)
                        .is_ok_and(|record| record.sig == sig)
                });
            if !valid {
                report.warnings.push(
                    mapper
                        .interner
                        .intern("furniture_buff:missing_keyword_or_spell"),
                );
                continue;
            }
            let raw_keyword = encode_target_form_id(keyword, mapper.interner, &target_masters)
                .ok_or_else(|| FixupError::Other("cannot encode furniture buff keyword".into()))?;
            specs.push((
                raw_keyword,
                BuffSpec {
                    keyword,
                    spell,
                    wait_time: source.wait_time,
                },
            ));
        }
        let mut changed = Vec::new();
        for fk in session
            .form_keys_of_sig(SigCode(*b"FURN"), mapper.interner)
            .map_err(handle_error)?
        {
            let keywords = session
                .first_subrecord_bytes(&fk, "KWDA")
                .map_err(handle_error)?
                .unwrap_or_default();
            let matching: Vec<_> = specs
                .iter()
                .filter_map(|(raw, spec)| {
                    keywords
                        .chunks_exact(4)
                        .any(|bytes| bytes == raw.to_le_bytes())
                        .then_some(spec.clone())
                })
                .collect();
            if matching.is_empty() {
                continue;
            }
            let vmad = buff_vmad(&matching, &target_masters, &target_plugin, mapper.interner)
                .ok_or_else(|| FixupError::Other("cannot encode furniture buff VMAD".into()))?;
            let mut record = session
                .record_decoded(&fk, &target_schema, mapper.interner)
                .map_err(handle_error)?;
            match attach_buff_script(&mut record, &vmad) {
                AttachResult::Changed => changed.push(record),
                AttachResult::AlreadyPresent => {}
                AttachResult::Conflict(reason) => report.warnings.push(
                    mapper
                        .interner
                        .intern(&format!("furniture_buff:{:06X}:{reason}", fk.local)),
                ),
            }
        }
        let expected = changed.len();
        let replaced = session
            .replace_records_contents(changed, &target_schema, mapper.interner)
            .map_err(handle_error)?;
        if replaced != expected {
            return Err(FixupError::Other(format!(
                "furniture buffs replaced {replaced} of {expected} records"
            )));
        }
        report.records_changed += replaced as u32;
        report.diagnostics.push(mapper.interner.intern(&format!(
            "furniture_buff:types={}:attached={replaced}",
            specs.len()
        )));
        Ok(report)
    }
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn field<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *sig)
        .map(|entry| &entry.value)
}

fn named_value<'a>(items: &'a Value, name_key: &str, name: &str) -> Option<&'a Value> {
    items
        .as_array()?
        .iter()
        .find(|item| {
            item.get(name_key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        })?
        .get("Value")
}

fn object_form(value: &Value, interner: &StringInterner) -> Option<FormKey> {
    let reference = value.get("FormID")?.get("reference")?;
    Some(FormKey {
        plugin: interner.intern(reference.get("plugin")?.as_str()?),
        local: u32::from_str_radix(reference.get("object_id")?.as_str()?, 16).ok()?,
    })
}

fn source_buff_specs(payload: &Value, interner: &StringInterner) -> Vec<BuffSpec> {
    let Some(script) = payload
        .get("Scripts")
        .and_then(Value::as_array)
        .and_then(|scripts| {
            scripts.iter().find(|script| {
                script
                    .get("ScriptName")
                    .and_then(Value::as_str)
                    .is_some_and(|name| name.eq_ignore_ascii_case(SOURCE_SCRIPT))
            })
        })
    else {
        return Vec::new();
    };
    let Some(rows) = script
        .get("Properties")
        .and_then(|props| named_value(props, "propertyName", "FurnitureTypeData"))
        .and_then(Value::as_array)
    else {
        return Vec::new();
    };
    rows.iter()
        .filter_map(|row| {
            let member = |name| named_value(row, "memberName", name);
            // Beds also require disease, healing and companion logic owned by FO4's sleep system.
            if member("IsDiseaseRisk").and_then(Value::as_bool) == Some(true)
                || [
                    "SpellToCast_RomanticAllyPresent",
                    "SpellToCast_InfatuatedAllyPresent",
                ]
                .iter()
                .any(|name| {
                    member(name)
                        .and_then(|value| object_form(value, interner))
                        .is_some()
                })
            {
                return None;
            }
            // FurnitureTypeDatum.WaitTime defaults to 30 in the source script declarations.
            let wait_time = member("WaitTime").and_then(Value::as_f64).unwrap_or(30.0);
            if !wait_time.is_finite() || wait_time <= 0.0 {
                return None;
            }
            Some(BuffSpec {
                keyword: object_form(member("FurnitureTypeKeyword")?, interner)?,
                spell: object_form(member("SpellToCast")?, interner)?,
                wait_time,
            })
        })
        .collect()
}

fn buff_vmad(
    specs: &[BuffSpec],
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let rows = specs.iter().map(|spec| Some(json!([
        {"memberName": "BuffSpell", "Type": "Object", "Flags": 1, "Value": {
            "Alias": -1, "FormID": {"reference": {
                "plugin": interner.resolve(spec.spell.plugin)?, "object_id": format!("{:06X}", spec.spell.local)
            }}
        }},
        {"memberName": "WaitTime", "Type": "Float", "Flags": 1, "Value": spec.wait_time}
    ]))).collect::<Option<Vec<_>>>()?;
    build_vmad_bytes_from_payload(
        &json!({
            "Version": 6, "Object Format": 2,
            "Scripts": [{"ScriptName": SCRIPT_NAME, "Properties": [{
                "propertyName": "Buffs", "Type": "Array of Struct", "Flags": 1, "Value": rows
            }]}]
        }),
        masters,
        plugin,
    )
}

fn attach_buff_script(record: &mut Record, vmad: &[u8]) -> AttachResult {
    if let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"VMAD")
    {
        let FieldValue::Bytes(bytes) = &mut entry.value else {
            return AttachResult::Conflict("existing_vmad_not_bytes");
        };
        let mut updated = bytes.to_vec();
        let result = attach_script_bytes(&mut updated, SCRIPT_NAME, vmad);
        if result == AttachResult::Changed {
            *bytes = updated.into();
        }
        return result;
    }
    record.fields.insert(
        0,
        FieldEntry {
            sig: SubrecordSig(*b"VMAD"),
            value: FieldValue::Bytes(vmad.into()),
        },
    );
    AttachResult::Changed
}

#[cfg(test)]
mod tests;
