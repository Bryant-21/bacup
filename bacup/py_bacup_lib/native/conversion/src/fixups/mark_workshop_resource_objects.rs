//! FO76 has no settlement ratings, so its buildable workshop objects lack what FO4 counts.
//! `Workshop::IsResourceObject` counts an object only when its base carries
//! `WorkshopResourceObject` (sleepable furniture and actors excepted), a bed adds only its
//! `resourceBed`, and WorkshopParentScript only assigns settlers to objects carrying
//! WorkshopObjectScript. Each buildable bed and each object with a food, water, defense or power
//! rating gets what its vanilla FO4 counterpart carries. Beds and guard posts, the furniture,
//! also need an assigned settler, as in FO4.

use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use rustc_hash::FxHashSet;
use serde_json::json;

use crate::fixups::apply_fo76_workshop_catalog::{
    FO4_WORKSHOP_WORKBENCH_FURNITURE, FO4_WORKSHOP_WORKBENCHES, first_form_key,
};
use crate::fixups::bridge_fo76_enemy_music_keywords::{append_keywords, keyword_form_keys};
use crate::fixups::quest_script_vmad::{
    AttachResult, add_top_level_object_properties, attach_script_bytes, has_top_level_script,
};
use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::Sym;

const IS_SLEEP_FURNITURE: u32 = 0x021B18;
const WORKSHOP_WORK_OBJECT: u32 = 0x020592;
const WORKSHOP_RESOURCE_OBJECT: u32 = 0x129A8C;
const RESOURCE_BED: u32 = 0x000334;
/// PowerGenerated, Food, Water, Safety.
const RATINGS: [u32; 4] = [0x00032E, 0x000331, 0x000332, 0x000333];
const WORKSHOP_PARENT: u32 = 0x02058E;
const SCRIPT_NAME: &str = "WorkshopObjectScript";
const PARENT_PROPERTY: &str = "WorkshopParent";

pub struct MarkWorkshopResourceObjectsFixup;

impl Fixup for MarkWorkshopResourceObjectsFixup {
    fn name(&self) -> &'static str {
        "mark_workshop_resource_objects"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
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
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let interner = mapper.interner;
        let output_plugin = mapper.output_plugin_sym();
        let schema = session.schema().map_err(handle_error)?;
        let fallout4 = interner.intern("Fallout4.esm");
        let fo4 = |local| FormKey { local, plugin: fallout4 };
        let masters = session.target_masters().to_vec();
        let plugin = session.target_slot().parsed.plugin_name.clone();
        let mut report = FixupReport::empty();
        // Without Fallout4.esm as a master, nothing here can carry FO4 workshop data.
        if !masters.iter().any(|master| master.eq_ignore_ascii_case("Fallout4.esm")) {
            return Ok(report);
        }
        let av = |local| {
            encode_target_form_id(fo4(local), interner, &masters).map(|raw| Av { key: fo4(local), raw })
        };
        let resource_object =
            av(WORKSHOP_RESOURCE_OBJECT).ok_or_else(|| encode_error(WORKSHOP_RESOURCE_OBJECT))?;
        let bed_rating = av(RESOURCE_BED).ok_or_else(|| encode_error(RESOURCE_BED))?;
        let ratings = RATINGS
            .map(|local| av(local).ok_or_else(|| encode_error(local)))
            .into_iter()
            .collect::<Result<Vec<_>, _>>()?;
        let raw_parent = av(WORKSHOP_PARENT).ok_or_else(|| encode_error(WORKSHOP_PARENT))?.raw;
        let script_vmad = workshop_object_vmad(&masters, &plugin)
            .ok_or_else(|| FixupError::Other("cannot encode WorkshopObjectScript VMAD".into()))?;
        let names = PropertyNames {
            actor_value: interner.intern("properties_actor_value"),
            value: interner.intern("properties_value"),
        };

        let mut decode_errors = 0u32;
        let mut seen = FxHashSet::default();
        let mut furniture_category = FxHashSet::default();
        let mut created = Vec::new();
        for fk in session
            .form_keys_of_sig(SigCode(*b"COBJ"), interner)
            .map_err(handle_error)?
        {
            let Ok(recipe) = session.record_decoded(&fk, &schema, interner) else {
                decode_errors += 1;
                continue;
            };
            let Some(bench) = field(&recipe, b"BNAM")
                .and_then(first_form_key)
                .filter(|bench| bench.plugin == fallout4 && FO4_WORKSHOP_WORKBENCHES.contains(&bench.local))
            else {
                continue;
            };
            // Master records already carry FO4 data; editing one here would override it.
            let Some(object) = field(&recipe, b"CNAM")
                .and_then(first_form_key)
                .filter(|object| object.plugin == output_plugin)
            else {
                continue;
            };
            if seen.insert(object) {
                created.push(object);
            }
            if bench.local == FO4_WORKSHOP_WORKBENCH_FURNITURE {
                furniture_category.insert(object);
            }
        }

        let (mut beds, mut rated, mut unsupported) = (0u32, 0u32, 0u32);
        let mut changed = Vec::new();
        for fk in created {
            let Ok(mut record) = session.record_decoded(&fk, &schema, interner) else {
                decode_errors += 1;
                continue;
            };
            let furniture = record.sig == SigCode(*b"FURN");
            let bed = furniture
                && furniture_category.contains(&fk)
                && keyword_form_keys(&record, &masters, &plugin, interner)
                    .contains(&fo4(IS_SLEEP_FURNITURE));
            let has_rating = ratings.iter().any(|rating| {
                property_value(&record, *rating, &names).is_some_and(|value| value != 0.0)
            });
            if !bed && !has_rating {
                continue;
            }
            if bed {
                beds += 1;
            } else {
                rated += 1;
            }
            let mut edited = false;
            let properties = if bed { vec![resource_object, bed_rating] } else { vec![resource_object] };
            for property in properties {
                match add_property(&mut record, property, &names) {
                    Some(added) => edited |= added,
                    None => unsupported += 1,
                }
            }
            if furniture {
                edited |= append_keywords(
                    &mut record,
                    &[fo4(WORKSHOP_WORK_OBJECT)],
                    &masters,
                    &plugin,
                    interner,
                );
            }
            match bind_workshop_object_script(&mut record, &script_vmad, raw_parent) {
                AttachResult::Changed => edited = true,
                AttachResult::AlreadyPresent => {}
                AttachResult::Conflict(_) => unsupported += 1,
            }
            if edited {
                changed.push(record);
            }
        }

        let expected = changed.len();
        let replaced = session
            .replace_records_contents(changed, &schema, interner)
            .map_err(handle_error)?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "mark_workshop_resource_objects replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced as u32;
        report.diagnostics.push(interner.intern(&format!(
            "mark_workshop_resource_objects:beds={beds}:rated={rated}:changed={replaced}"
        )));
        if decode_errors > 0 || unsupported > 0 {
            report.warnings.push(interner.intern(&format!(
                "mark_workshop_resource_objects:decode_errors={decode_errors}:unsupported={unsupported}"
            )));
        }
        Ok(report)
    }
}

#[derive(Clone, Copy)]
struct Av {
    key: FormKey,
    raw: u32,
}

struct PropertyNames {
    actor_value: Sym,
    value: Sym,
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn encode_error(local: u32) -> FixupError {
    FixupError::Other(format!("cannot encode Fallout4.esm {local:06X}"))
}

fn field<'a>(record: &'a Record, sig: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| entry.sig.0 == *sig)
        .map(|entry| &entry.value)
}

fn workshop_object_vmad(masters: &[String], plugin: &str) -> Option<Vec<u8>> {
    build_vmad_bytes_from_payload(
        &json!({
            "Version": 6, "Object Format": 2,
            "Scripts": [{"ScriptName": SCRIPT_NAME, "Properties": [{
                "propertyName": PARENT_PROPERTY, "Type": "Object", "Flags": 1,
                "Value": {"Alias": -1, "FormID": {"reference": {
                    "plugin": "Fallout4.esm", "object_id": format!("{WORKSHOP_PARENT:06X}")
                }}}
            }]}]
        }),
        masters,
        plugin,
    )
}

/// The property sheet decodes as raw rows of form id + float on FURN and as structured rows on
/// other types; both are read and extended.
fn property_value(record: &Record, av: Av, names: &PropertyNames) -> Option<f32> {
    match field(record, b"PRPS")? {
        FieldValue::Bytes(rows) => rows
            .chunks_exact(8)
            .find(|row| row[..4] == av.raw.to_le_bytes())
            .map(|row| f32::from_le_bytes([row[4], row[5], row[6], row[7]])),
        FieldValue::List(rows) => rows.iter().find_map(|row| {
            let FieldValue::Struct(fields) = row else {
                return None;
            };
            let member = |name: Sym| fields.iter().find(|(field, _)| *field == name).map(|(_, value)| value);
            (member(names.actor_value) == Some(&FieldValue::FormKey(av.key))).then(|| {
                match member(names.value) {
                    Some(FieldValue::Float(value)) => *value,
                    _ => 0.0,
                }
            })
        }),
        _ => None,
    }
}

/// Adds `av` at 1 unless the sheet already has it; `None` for a sheet in neither decoded shape.
fn add_property(record: &mut Record, av: Av, names: &PropertyNames) -> Option<bool> {
    if property_value(record, av, names).is_some() {
        return Some(false);
    }
    if field(record, b"PRPS").is_none() {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"PRPS"),
            value: FieldValue::Bytes(Default::default()),
        });
    }
    match &mut record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"PRPS")?
        .value
    {
        FieldValue::Bytes(rows) if rows.len() % 8 == 0 => {
            rows.extend(av.raw.to_le_bytes());
            rows.extend(1.0f32.to_le_bytes());
        }
        FieldValue::List(rows) => rows.push(FieldValue::Struct(vec![
            (names.actor_value, FieldValue::FormKey(av.key)),
            (names.value, FieldValue::Float(1.0)),
        ])),
        _ => return None,
    }
    Some(true)
}

fn bind_workshop_object_script(
    record: &mut Record,
    script_vmad: &[u8],
    raw_parent: u32,
) -> AttachResult {
    let Some(entry) = record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"VMAD")
    else {
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig(*b"VMAD"),
                value: FieldValue::Bytes(script_vmad.into()),
            },
        );
        return AttachResult::Changed;
    };
    let FieldValue::Bytes(bytes) = &mut entry.value else {
        return AttachResult::Conflict("existing_vmad_not_bytes");
    };
    let mut updated = bytes.to_vec();
    let result = match has_top_level_script(&updated, &|name| {
        name.eq_ignore_ascii_case(SCRIPT_NAME.as_bytes())
    }) {
        None => return AttachResult::Conflict("unsupported_or_malformed_vmad"),
        Some(false) => attach_script_bytes(&mut updated, SCRIPT_NAME, script_vmad),
        // FO76 bindings arrive without WorkshopParent; an existing one makes the add refuse.
        Some(true) => match add_top_level_object_properties(&mut updated, &mut |name| {
            Some(if name.eq_ignore_ascii_case(SCRIPT_NAME) {
                vec![(PARENT_PROPERTY.to_string(), raw_parent)]
            } else {
                Vec::new()
            })
        }) {
            Some(()) => AttachResult::Changed,
            None => AttachResult::AlreadyPresent,
        },
    };
    if result == AttachResult::Changed {
        *bytes = updated.into();
    }
    result
}

#[cfg(test)]
mod tests;
