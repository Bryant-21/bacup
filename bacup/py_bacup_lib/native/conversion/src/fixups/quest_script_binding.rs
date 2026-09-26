//! Shared plumbing for fixups that bind a B21 addition script to converted quests.

use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use serde_json::{Value, json};

use crate::fixups::FixupError;
use crate::fixups::quest_script_vmad::{AttachResult, attach_script_bytes};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::{HandleRawScan, PluginSession};
use crate::source_read::form_key_to_read_str;
use crate::sym::{StringInterner, Sym};

const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

pub(crate) fn object_value(form: Option<FormKey>, interner: &StringInterner) -> Option<Value> {
    let Some(form) = form else {
        return Some(json!({"Alias": -1, "FormID": null}));
    };
    Some(json!({
        "Alias": -1,
        "FormID": {
            "reference": {
                "plugin": interner.resolve(form.plugin)?,
                "object_id": format!("{:06X}", form.local),
            },
        },
    }))
}

pub(crate) fn object_property(
    name: &str,
    form: Option<FormKey>,
    interner: &StringInterner,
) -> Option<Value> {
    Some(property(name, "Object", object_value(form, interner)?))
}

pub(crate) fn object_array_property(
    name: &str,
    forms: &[FormKey],
    interner: &StringInterner,
) -> Option<Value> {
    let values = forms
        .iter()
        .map(|form| object_value(Some(*form), interner))
        .collect::<Option<Vec<_>>>()?;
    Some(property(name, "Array of Object", json!(values)))
}

pub(crate) fn int_array_property(name: &str, values: &[i32]) -> Value {
    property(name, "Array of Int32", json!(values))
}

pub(crate) fn string_array_property(name: &str, values: &[String]) -> Value {
    property(name, "Array of String", json!(values))
}

fn property(name: &str, kind: &str, value: Value) -> Value {
    json!({
        "propertyName": name,
        "Type": kind,
        "Flags": VMAD_PROPERTY_FLAG_EDITED,
        "Value": value,
    })
}

pub(crate) fn script_vmad(
    script_name: &str,
    properties: Vec<Value>,
    target_masters: &[String],
    target_plugin: &str,
) -> Option<Vec<u8>> {
    build_vmad_bytes_from_payload(
        &json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": script_name,
                "Flags": 0,
                "Properties": properties,
            }],
        }),
        target_masters,
        target_plugin,
    )
}

/// The output form a source form translated to, when that form exists: a
/// record in the output plugin, or a record in a master the mapper resolved to.
pub(crate) fn emitted_target_form(
    session: &mut PluginSession,
    mapper: &FormKeyMapper,
    source: FormKey,
) -> Result<Option<FormKey>, FixupError> {
    let Some(target) = mapper.lookup(source) else {
        return Ok(None);
    };
    let Some(plugin) = mapper.interner.resolve(target.plugin) else {
        return Ok(None);
    };
    if plugin.eq_ignore_ascii_case(&session.target_slot().parsed.plugin_name) {
        let target_id = session.target_id();
        let exists = session
            .record_exists_in_handle(target_id, &form_key_to_read_str(&target, mapper.interner))
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        return Ok(exists.then_some(target));
    }
    Ok(session
        .target_masters()
        .iter()
        .any(|master| master.eq_ignore_ascii_case(plugin))
        .then_some(target))
}

/// Add a script binding to a quest's VMAD; `None` when the quest has no VMAD.
pub(crate) fn attach_quest_script(
    session: &mut PluginSession,
    quest: &FormKey,
    script_name: &str,
    script_vmad: &[u8],
) -> Result<Option<AttachResult>, FixupError> {
    let mut outcome = None;
    session
        .patch_all_subrecords_bytes(quest, "VMAD", |bytes| {
            if outcome.is_some() {
                outcome = Some(AttachResult::Conflict("multiple_vmad_subrecords"));
                return false;
            }
            let result = attach_script_bytes(bytes, script_name, script_vmad);
            outcome = Some(result);
            result == AttachResult::Changed
        })
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    Ok(outcome)
}

/// The conversion's source plugin, for fixups that read its records raw.
pub(crate) struct SourcePlugin {
    pub(crate) id: u64,
    pub(crate) name: String,
    masters: Vec<String>,
    sym: Sym,
}

impl SourcePlugin {
    pub(crate) fn of(session: &PluginSession, interner: &StringInterner) -> Option<Self> {
        let id = session.source_id()?;
        let slot = session.source_slot_opt()?;
        Some(Self {
            id,
            name: slot.parsed.plugin_name.clone(),
            masters: slot.parsed.header.masters.clone(),
            sym: interner.intern(&slot.parsed.plugin_name),
        })
    }

    pub(crate) fn masters(&self) -> &[String] {
        &self.masters
    }

    /// Raw FormIDs of this plugin's own records of `sig`, in object-id order.
    pub(crate) fn own_raw_form_ids(&self, scan: &HandleRawScan<'_>, sig: SigCode) -> Vec<u32> {
        let mut raw_form_ids = scan
            .raw_form_ids_of_sig(sig)
            .into_iter()
            .filter(|raw| (raw >> 24) as usize >= self.masters.len())
            .collect::<Vec<_>>();
        raw_form_ids.sort_unstable_by_key(|raw| raw & 0x00FF_FFFF);
        raw_form_ids
    }

    pub(crate) fn form_key(&self, raw_form_id: u32, interner: &StringInterner) -> Option<FormKey> {
        if raw_form_id == 0 {
            return None;
        }
        let plugin = self
            .masters
            .get((raw_form_id >> 24) as usize)
            .map_or(self.sym, |master| interner.intern(master));
        Some(FormKey {
            plugin,
            local: raw_form_id & 0x00FF_FFFF,
        })
    }

    pub(crate) fn form_key_at(
        &self,
        data: &[u8],
        offset: usize,
        interner: &StringInterner,
    ) -> Option<FormKey> {
        let raw = data.get(offset..offset.checked_add(4)?)?;
        self.form_key(u32::from_le_bytes(raw.try_into().ok()?), interner)
    }
}
