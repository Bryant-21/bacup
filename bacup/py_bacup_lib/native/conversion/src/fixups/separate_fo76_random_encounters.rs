//! Fixup: make FO76's random encounters run in Appalachia, and only there.
//!
//! FO76 inherited Fallout 4's whole random-encounter skeleton — `REMainBranch`,
//! the `REEncounterType*` keywords, the `RETrigger*` bases, `REScript`,
//! `RETriggerScript`, `REParentScript` — at the same object ids and the same
//! script names, so the converted plugin reuses Fallout 4's stock scripts as
//! authored. Three things still have to be repaired.
//!
//! 1. **Worldspace.** `FO76_FO4_VANILLA_REMAP_BLOCKED_FORM_IDS` keeps FO76's
//!    `REMainBranch` and encounter-type keywords source-owned, so Appalachia
//!    gets a top-level branch beside Fallout 4's instead of nesting inside it.
//!    That stops Appalachia's encounters leaking into the Commonwealth; this
//!    pass adds the other half, `GetInWorldspace(APPALACHIA) == 1` on the event's
//!    Reference1, so a Commonwealth trigger cannot walk into the FO76 branch.
//!    It is the same shape Fallout 4 authored (`REMainBranch` gates on the
//!    Commonwealth) and the same one Far Harbor and Nuka-World use for their own
//!    top-level branches.
//!
//! 2. **`REParent`.** FO76 spells the property `RE_Parent`; Fallout 4's
//!    `REScript` declares `REParent`, mandatory. Left unrenamed it binds nothing,
//!    and every cleanup path dereferences it — `RECheckForCleanup` reads
//!    `REParent.REIgnoreForCleanup` per alias, `REAliasScript.OnLoad` calls
//!    `REParent.KillWithForce`.
//!
//! 3. **`Startup()`.** Fallout 4 calls it from each encounter's stage-10
//!    fragment; FO76's fragments were stripped server-side, so nothing does.
//!    `Startup()` is what registers the quest for `REParent`'s cleanup event, so
//!    without it a converted encounter never stops: its actors persist,
//!    `REParent`'s running-encounter budget fills permanently, and
//!    `SendStoryEventAndWait` then fails for every later encounter.
//!    `B21:RandomEncounterStartup` calls it from `OnQuestInit`.
//!
//! Idempotent: a second pass over an already-gated branch or an already-attached
//! quest leaves both byte-identical.

use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;
use smallvec::SmallVec;

use crate::fixups::quest_script_vmad::{
    AttachResult, attach_script_bytes, has_top_level_script, rename_top_level_script_property,
};
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::StringInterner;

const SOURCE_PLUGIN: &str = "SeventySix.esm";
const STARTUP_SCRIPT_NAME: &str = "B21:RandomEncounterStartup";

/// Fallout 4's `REScript`, which FO76's quests bind by the same name. Subclasses
/// keep the base name as their last namespace segment
/// (`Quests:Storm:Encounters:rescript`) and inherit `REParent`, so they match too.
const RE_QUEST_SCRIPT_NAME: &str = "rescript";
const FO76_RE_PARENT_PROPERTY: &str = "RE_Parent";
const FO4_RE_PARENT_PROPERTY: &str = "REParent";

const FO76_RE_MAIN_BRANCH_EDITOR_ID: &str = "REMainBranch";
/// `rename_fo76_target_editor_id_collision` suffixes a blocked record whose
/// EditorID collides with a master's; `REMainBranch` is one of those.
const FO76_COLLISION_RENAME_SUFFIX: &str = "fo76";
const FO76_APPALACHIA_WORLDSPACE_LOCAL: u32 = 0x0025_DA15;

const CTDA_LEN: usize = 32;
/// Operator `Equal to` with no OR flag, so the row ANDs with the branch's
/// existing conditions instead of widening them.
const CTDA_OPERATOR_EQUAL_TO_AND: u8 = 0x00;
/// `GetInWorldspace`, Parameter #1 = WRLD FormID.
const GET_IN_WORLDSPACE_FUNCTION_ID: u16 = 310;
/// Run the condition on the story event's own data rather than on a subject.
const CTDA_RUN_ON_EVENT_DATA: u32 = 7;
/// Event-data selector `R1` (Reference1) in the trailing parameter slot, which
/// is where `RETriggerScript` puts the trigger:
/// `EncounterType.SendStoryEventAndWait(GetCurrentLocation(), Self, None, ...)`.
/// Fallout 4's own `REMainBranch` selects Reference1 the same way.
const CTDA_EVENT_DATA_REFERENCE_1: u32 = 12626;

const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;

pub struct SeparateFo76RandomEncountersFixup;

impl Fixup for SeparateFo76RandomEncountersFixup {
    fn name(&self) -> &'static str {
        "separate_fo76_random_encounters"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        let source_game = session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref());
        let target_game = session.target_slot().parsed.game.as_deref();
        source_game == Some("fo76") && target_game == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        gate_main_branch_on_appalachia(session, mapper, target_schema, &mut report)?;
        repair_encounter_quests(session, mapper, target_schema, &mut report)?;
        Ok(report)
    }
}

/// Add `GetInWorldspace(APPALACHIA) == 1` to the FO76 branch, so an encounter
/// trigger placed in the Commonwealth cannot reach Appalachia's quest nodes.
fn gate_main_branch_on_appalachia(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &AuthoringSchema,
    report: &mut FixupReport,
) -> Result<(), FixupError> {
    let source_plugin = mapper.interner.intern(SOURCE_PLUGIN);
    let Some(worldspace) = mapper.lookup(FormKey {
        local: FO76_APPALACHIA_WORLDSPACE_LOCAL,
        plugin: source_plugin,
    }) else {
        warn(report, mapper.interner, "appalachia_worldspace_unmapped");
        return Ok(());
    };
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let Some(encoded_worldspace) = encoded_target_form_id(
        worldspace,
        session.target_masters(),
        &target_plugin,
        mapper.interner,
    ) else {
        warn(report, mapper.interner, "appalachia_worldspace_unencodable");
        return Ok(());
    };

    let smbn_sig =
        SigCode::from_str("SMBN").map_err(|error| FixupError::SchemaError(error.to_string()))?;
    let form_keys = session
        .form_keys_of_sig(smbn_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;

    let mut branches = Vec::new();
    for form_key in form_keys {
        let Ok(record) = session.record_decoded(&form_key, target_schema, mapper.interner) else {
            continue;
        };
        if is_fo76_re_main_branch(&record, mapper.interner) {
            branches.push(record);
        }
    }

    match branches.len() {
        0 => {
            warn(report, mapper.interner, "re_main_branch_missing");
            return Ok(());
        }
        1 => {}
        found => {
            report.warnings.push(mapper.interner.intern(&format!(
                "separate_fo76_random_encounters:re_main_branch_ambiguous:{found}"
            )));
            return Ok(());
        }
    }

    let mut branch = branches.pop().expect("one branch");
    if !prepend_worldspace_condition(&mut branch, encoded_worldspace) {
        report.diagnostics.push(
            mapper
                .interner
                .intern("separate_fo76_random_encounters:re_main_branch_already_gated"),
        );
        return Ok(());
    }
    if session
        .replace_record_contents(branch, target_schema, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?
    {
        report.records_changed += 1;
        report.diagnostics.push(mapper.interner.intern(&format!(
            "separate_fo76_random_encounters:re_main_branch_gated:worldspace={encoded_worldspace:08X}"
        )));
    } else {
        warn(report, mapper.interner, "re_main_branch_replace_failed");
    }
    Ok(())
}

/// The branch this pass owns: FO76's own `REMainBranch`, which the remap block
/// keeps source-owned and the collision rename suffixes.
fn is_fo76_re_main_branch(record: &Record, interner: &StringInterner) -> bool {
    let Some(editor_id) = record.eid.and_then(|eid| interner.resolve(eid)) else {
        return false;
    };
    editor_id.eq_ignore_ascii_case(FO76_RE_MAIN_BRANCH_EDITOR_ID)
        || editor_id.eq_ignore_ascii_case(&format!(
            "{FO76_RE_MAIN_BRANCH_EDITOR_ID}{FO76_COLLISION_RENAME_SUFFIX}"
        ))
}

/// Returns `true` when the record was modified.
///
/// The row goes FIRST, never last. FO76's `REMainBranch` ends on ten
/// `GetEventData(K1, <encounter type>)` rows that all carry the OR flag — the
/// last one included — so an appended row joins that group: the branch would
/// then accept any event in Appalachia regardless of encounter type, and the
/// worldspace would stop gating anything. Prepended, the row ANDs with
/// everything after it.
fn prepend_worldspace_condition(record: &mut Record, encoded_worldspace: u32) -> bool {
    if already_gated(record, encoded_worldspace) {
        return false;
    }
    let at = condition_insert_position(record);
    record
        .fields
        .insert(at, worldspace_condition(encoded_worldspace));
    // A stale condition count crashes FO4's condition evaluation.
    if record.fields.iter().any(|entry| entry.sig.0 == *b"CITC") {
        record.sync_condition_count();
    }
    true
}

/// Where the leading `CTDA` belongs in an `SMBN`: ahead of the existing
/// condition block, or straight after `CITC` when the branch has none yet.
fn condition_insert_position(record: &Record) -> usize {
    if let Some(index) = record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"CTDA")
    {
        return index;
    }
    record
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"CITC")
        .map(|index| index + 1)
        .unwrap_or(record.fields.len())
}

fn worldspace_condition(encoded_worldspace: u32) -> FieldEntry {
    let mut bytes = vec![0u8; CTDA_LEN];
    bytes[0] = CTDA_OPERATOR_EQUAL_TO_AND;
    bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
    bytes[8..10].copy_from_slice(&GET_IN_WORLDSPACE_FUNCTION_ID.to_le_bytes());
    bytes[12..16].copy_from_slice(&encoded_worldspace.to_le_bytes());
    bytes[20..24].copy_from_slice(&CTDA_RUN_ON_EVENT_DATA.to_le_bytes());
    bytes[28..32].copy_from_slice(&CTDA_EVENT_DATA_REFERENCE_1.to_le_bytes());
    FieldEntry {
        sig: SubrecordSig(*b"CTDA"),
        value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
    }
}

fn already_gated(record: &Record, encoded_worldspace: u32) -> bool {
    record.fields.iter().any(|entry| {
        if entry.sig.0 != *b"CTDA" {
            return false;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            return false;
        };
        bytes.len() >= 16
            && u16::from_le_bytes(bytes[8..10].try_into().unwrap()) == GET_IN_WORLDSPACE_FUNCTION_ID
            && u32::from_le_bytes(bytes[12..16].try_into().unwrap()) == encoded_worldspace
    })
}

/// Rename `RE_Parent` and attach the startup caller on every converted
/// encounter quest.
fn repair_encounter_quests(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    target_schema: &AuthoringSchema,
    report: &mut FixupReport,
) -> Result<(), FixupError> {
    let target_plugin = session.target_slot().parsed.plugin_name.clone();
    let Some(startup_vmad) = startup_vmad(session.target_masters(), &target_plugin) else {
        return Err(FixupError::SchemaError(
            "random encounter startup VMAD encode failed".into(),
        ));
    };
    let vmad_sig = SubrecordSig::from_str("VMAD")
        .map_err(|error| FixupError::SchemaError(error.to_string()))?;
    let qust_sig =
        SigCode::from_str("QUST").map_err(|error| FixupError::SchemaError(error.to_string()))?;
    let form_keys = session
        .form_keys_of_sig(qust_sig, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;

    let mut stats = QuestStats::default();
    let mut changed_records = Vec::new();
    let mut conflicts: Vec<&'static str> = Vec::new();
    for form_key in form_keys {
        let Ok(mut record) = session.record_decoded(&form_key, target_schema, mapper.interner)
        else {
            continue;
        };
        let Some(vmad) = record.fields.iter().position(|entry| entry.sig == vmad_sig) else {
            continue;
        };
        let FieldValue::Bytes(bytes) = &record.fields[vmad].value else {
            continue;
        };
        if has_top_level_script(bytes, &script_is_encounter_script) != Some(true) {
            continue;
        }
        stats.encounter_quests += 1;

        let mut patched = bytes.to_vec();
        let renamed = rename_top_level_script_property(
            &mut patched,
            &script_is_encounter_script,
            FO76_RE_PARENT_PROPERTY,
            FO4_RE_PARENT_PROPERTY,
        );
        match renamed {
            Some(count) => stats.re_parent_renamed += count,
            None => {
                conflicts.push("vmad_unwalkable");
                continue;
            }
        }
        let attached = attach_script_bytes(&mut patched, STARTUP_SCRIPT_NAME, &startup_vmad);
        match attached {
            AttachResult::Changed => stats.startup_attached += 1,
            AttachResult::AlreadyPresent => stats.startup_already_present += 1,
            AttachResult::Conflict(reason) => {
                conflicts.push(reason);
                continue;
            }
        }
        if patched.as_slice() == bytes.as_slice() {
            continue;
        }
        record.fields[vmad].value = FieldValue::Bytes(patched.into());
        changed_records.push(record);
    }

    report.diagnostics.push(mapper.interner.intern(&format!(
        "separate_fo76_random_encounters: encounter_quests={} re_parent_renamed={} \
         startup_attached={} startup_already_present={} conflicts={}",
        stats.encounter_quests,
        stats.re_parent_renamed,
        stats.startup_attached,
        stats.startup_already_present,
        conflicts.len(),
    )));
    for reason in dedup_conflicts(conflicts) {
        warn(report, mapper.interner, &reason);
    }

    let expected = changed_records.len();
    if expected == 0 {
        return Ok(());
    }
    let replaced = session
        .replace_records_contents(changed_records, target_schema, mapper.interner)
        .map_err(|error| FixupError::HandleError(error.to_string()))?;
    if replaced != expected {
        return Err(FixupError::HandleError(format!(
            "separate_fo76_random_encounters replaced {replaced} of {expected} expected records"
        )));
    }
    report.records_changed += u32::try_from(replaced).unwrap_or(u32::MAX);
    Ok(())
}

#[derive(Default)]
struct QuestStats {
    encounter_quests: usize,
    re_parent_renamed: usize,
    startup_attached: usize,
    startup_already_present: usize,
}

/// `rescript` itself, or a subclass whose namespaced name ends in it. Both
/// inherit `REParent` and `Startup()` from Fallout 4's `REScript`.
fn script_is_encounter_script(script_name: &[u8]) -> bool {
    let Ok(name) = std::str::from_utf8(script_name) else {
        return false;
    };
    name.eq_ignore_ascii_case(RE_QUEST_SCRIPT_NAME)
        || name
            .rsplit(':')
            .next()
            .is_some_and(|leaf| leaf.eq_ignore_ascii_case(RE_QUEST_SCRIPT_NAME))
}

fn startup_vmad(target_masters: &[String], target_plugin: &str) -> Option<Vec<u8>> {
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": STARTUP_SCRIPT_NAME,
                "Flags": 0,
                "Properties": [],
            }],
        }),
        target_masters,
        target_plugin,
    )
}

fn encoded_target_form_id(
    form_key: FormKey,
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<u32> {
    if form_key.local == 0 || form_key.local > 0x00FF_FFFF {
        return None;
    }
    let plugin = interner.resolve(form_key.plugin)?;
    let master_index = if plugin.eq_ignore_ascii_case(target_plugin) {
        target_masters.len()
    } else {
        target_masters
            .iter()
            .position(|master| master.eq_ignore_ascii_case(plugin))?
    };
    (master_index <= u8::MAX as usize).then(|| ((master_index as u32) << 24) | form_key.local)
}

fn dedup_conflicts(conflicts: Vec<&'static str>) -> Vec<String> {
    let mut counts: Vec<(&'static str, usize)> = Vec::new();
    for reason in conflicts {
        match counts.iter_mut().find(|(name, _)| *name == reason) {
            Some((_, count)) => *count += 1,
            None => counts.push((reason, 1)),
        }
    }
    counts
        .into_iter()
        .map(|(reason, count)| format!("{reason}:{count}"))
        .collect()
}

fn warn(report: &mut FixupReport, interner: &StringInterner, reason: &str) {
    report
        .warnings
        .push(interner.intern(&format!("separate_fo76_random_encounters:{reason}")));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RecordFlags;

    const OBJECT_PROPERTY: u8 = 1;
    const BOOL_PROPERTY: u8 = 5;

    /// One top-level script entry: name, then its properties.
    fn script(name: &str, properties: &[(&str, u8, &[u8])]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(name.as_bytes());
        bytes.push(0);
        bytes.extend_from_slice(&(properties.len() as u16).to_le_bytes());
        for (property_name, property_type, value) in properties {
            bytes.extend_from_slice(&(property_name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(property_name.as_bytes());
            bytes.push(*property_type);
            bytes.push(1);
            bytes.extend_from_slice(value);
        }
        bytes
    }

    fn vmad(scripts: &[Vec<u8>]) -> Vec<u8> {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&VMAD_VERSION.to_le_bytes());
        bytes.extend_from_slice(&VMAD_OBJECT_FORMAT.to_le_bytes());
        bytes.extend_from_slice(&(scripts.len() as u16).to_le_bytes());
        for entry in scripts {
            bytes.extend_from_slice(entry);
        }
        bytes
    }

    fn re_quest_vmad(script_name: &str, parent_property: &str) -> Vec<u8> {
        vmad(&[script(
            script_name,
            &[
                (parent_property, OBJECT_PROPERTY, &[0u8; 8]),
                ("StopQuestWhenAliasesDead", BOOL_PROPERTY, &[1u8]),
            ],
        )])
    }

    fn smbn(editor_id: Option<&str>, fields: Vec<FieldEntry>) -> (Record, StringInterner) {
        let interner = StringInterner::new();
        let record = Record {
            sig: SigCode::from_str("SMBN").unwrap(),
            form_key: FormKey {
                local: 0x0002_7DE0,
                plugin: interner.intern(SOURCE_PLUGIN),
            },
            eid: editor_id.map(|id| interner.intern(id)),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: SmallVec::new(),
        };
        (record, interner)
    }

    fn field(sig: &[u8; 4], bytes: Vec<u8>) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value: FieldValue::Bytes(SmallVec::from_vec(bytes)),
        }
    }

    /// `RE_Parent` is one byte longer than `REParent`, so the length prefix has
    /// to shrink with the name and every following property stay readable.
    #[test]
    fn re_parent_rename_rewrites_length_prefix_only_for_walkable_family_scripts() {
        let mut bytes = re_quest_vmad("rescript", FO76_RE_PARENT_PROPERTY);
        let original_len = bytes.len();

        let renamed = rename_top_level_script_property(
            &mut bytes,
            &script_is_encounter_script,
            FO76_RE_PARENT_PROPERTY,
            FO4_RE_PARENT_PROPERTY,
        );

        assert_eq!(renamed, Some(1));
        assert_eq!(bytes.len(), original_len - 1, "one byte shorter");
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains(FO4_RE_PARENT_PROPERTY));
        assert!(!text.contains(FO76_RE_PARENT_PROPERTY));
        // Still walkable, which is only true if the prefix was rewritten too.
        assert_eq!(
            has_top_level_script(&bytes, &script_is_encounter_script),
            Some(true)
        );
        assert!(text.contains("StopQuestWhenAliasesDead"));

        let mut bytes = re_quest_vmad("retriggerscript", FO76_RE_PARENT_PROPERTY);
        let before = bytes.clone();

        let renamed = rename_top_level_script_property(
            &mut bytes,
            &script_is_encounter_script,
            FO76_RE_PARENT_PROPERTY,
            FO4_RE_PARENT_PROPERTY,
        );

        assert_eq!(renamed, Some(0));
        assert_eq!(bytes, before, "untouched");

        let mut bytes = re_quest_vmad("rescript", FO76_RE_PARENT_PROPERTY);
        bytes.truncate(bytes.len() - 4);

        assert_eq!(
            rename_top_level_script_property(
                &mut bytes,
                &script_is_encounter_script,
                FO76_RE_PARENT_PROPERTY,
                FO4_RE_PARENT_PROPERTY,
            ),
            None
        );
    }

    /// The family is `REScript` and its subclasses. `RETriggerScript` and
    /// `REAliasScript` already spell `REParent` correctly and must not be handed
    /// `Startup()`, which only exists on `REScript`.
    #[test]
    fn encounter_script_and_main_branch_name_matching() {
        for name in [
            "rescript",
            "REScript",
            "Quests:Storm:Encounters:rescript",
            "Quests:Burn:Encounters:REScript",
        ] {
            assert!(
                script_is_encounter_script(name.as_bytes()),
                "expected {name} in the family"
            );
        }
        for name in [
            "retriggerscript",
            "realiasscript",
            "reparentscript",
            "RE_SceneSM04Script",
            "DefaultQuestRemovePlayersScript",
        ] {
            assert!(
                !script_is_encounter_script(name.as_bytes()),
                "expected {name} outside the family"
            );
        }

        for editor_id in ["REMainBranch", "REMainBranchfo76", "remainbranchFO76"] {
            let (record, interner) = smbn(Some(editor_id), Vec::new());
            assert!(
                is_fo76_re_main_branch(&record, &interner),
                "expected {editor_id} to match"
            );
        }
        for editor_id in [
            "REMainBranchNukaWorld",
            "DLC03REMainBranch",
            "RENormalBranch",
        ] {
            let (record, interner) = smbn(Some(editor_id), Vec::new());
            assert!(
                !is_fo76_re_main_branch(&record, &interner),
                "expected {editor_id} not to match"
            );
        }
        let (record, interner) = smbn(None, Vec::new());
        assert!(!is_fo76_re_main_branch(&record, &interner));
    }

    /// FO76's `REMainBranch` ends on an OR group whose LAST row still carries
    /// the OR flag, so the gate has to land ahead of the block. Appended, it
    /// would be OR'd into the encounter-type group and gate nothing.
    #[test]
    fn appalachia_gate_row_shape_and_placement() {
        let or_row = |function_id: u16| {
            let mut bytes = vec![0u8; CTDA_LEN];
            bytes[0] = 0x01;
            bytes[8..10].copy_from_slice(&function_id.to_le_bytes());
            field(b"CTDA", bytes)
        };
        let (mut branch, _interner) = smbn(
            Some("REMainBranchfo76"),
            vec![
                field(b"CITC", 2u32.to_le_bytes().to_vec()),
                or_row(576),
                or_row(576),
                field(b"DNAM", Vec::new()),
            ],
        );

        assert!(prepend_worldspace_condition(&mut branch, 0x0825_DA15));
        let sigs: Vec<String> = branch
            .fields
            .iter()
            .map(|entry| String::from_utf8_lossy(&entry.sig.0).into_owned())
            .collect();
        assert_eq!(sigs, vec!["CITC", "CTDA", "CTDA", "CTDA", "DNAM"]);
        let FieldValue::Bytes(first) = &branch.fields[1].value else {
            panic!("CTDA is bytes");
        };
        assert_eq!(
            u16::from_le_bytes(first[8..10].try_into().unwrap()),
            GET_IN_WORLDSPACE_FUNCTION_ID,
            "the gate is the first condition, not the last"
        );
        assert_eq!(first[0] & 0x01, 0, "the gate ANDs with the group after it");
        let FieldValue::Bytes(citc) = &branch.fields[0].value else {
            panic!("CITC is bytes");
        };
        assert_eq!(u32::from_le_bytes(citc[..4].try_into().unwrap()), 3);

        let before = branch.fields.clone();
        assert!(
            !prepend_worldspace_condition(&mut branch, 0x0825_DA15),
            "second pass is a no-op"
        );
        assert_eq!(branch.fields, before);

        let (mut branch, _interner) = smbn(
            Some("REMainBranchfo76"),
            vec![
                field(b"SNAM", vec![0u8; 4]),
                field(b"CITC", 0u32.to_le_bytes().to_vec()),
                field(b"DNAM", Vec::new()),
            ],
        );

        assert!(prepend_worldspace_condition(&mut branch, 0x0825_DA15));
        let sigs: Vec<String> = branch
            .fields
            .iter()
            .map(|entry| String::from_utf8_lossy(&entry.sig.0).into_owned())
            .collect();
        assert_eq!(sigs, vec!["SNAM", "CITC", "CTDA", "DNAM"]);

        let entry = worldspace_condition(0x0825_DA15);
        let FieldValue::Bytes(bytes) = &entry.value else {
            panic!("CTDA is bytes");
        };

        assert_eq!(entry.sig.0, *b"CTDA");
        assert_eq!(bytes.len(), CTDA_LEN);
        assert_eq!(bytes[0], CTDA_OPERATOR_EQUAL_TO_AND);
        assert_eq!(f32::from_le_bytes(bytes[4..8].try_into().unwrap()), 1.0);
        assert_eq!(
            u16::from_le_bytes(bytes[8..10].try_into().unwrap()),
            GET_IN_WORLDSPACE_FUNCTION_ID
        );
        assert_eq!(
            u32::from_le_bytes(bytes[12..16].try_into().unwrap()),
            0x0825_DA15
        );
        assert_eq!(
            u32::from_le_bytes(bytes[20..24].try_into().unwrap()),
            CTDA_RUN_ON_EVENT_DATA
        );
        // "R1" little-endian in the trailing event-data parameter slot.
        assert_eq!(&bytes[28..32], &[0x52, 0x31, 0x00, 0x00]);
    }

    /// A record in the output plugin indexes one past the last master; the
    /// count is read from the plugin being written, never hardcoded.
    #[test]
    fn output_plugin_records_encode_one_past_the_last_master() {
        let interner = StringInterner::new();
        let masters: Vec<String> = ["Fallout4.esm", "DLCRobot.esm", "DLCCoast.esm"]
            .iter()
            .map(|name| (*name).to_owned())
            .collect();
        let own = FormKey {
            local: 0x0025_DA15,
            plugin: interner.intern("SeventySix.esm"),
        };
        assert_eq!(
            encoded_target_form_id(own, &masters, "SeventySix.esm", &interner),
            Some(0x0325_DA15)
        );

        let master_owned = FormKey {
            local: 0x0000_003C,
            plugin: interner.intern("DLCCoast.esm"),
        };
        assert_eq!(
            encoded_target_form_id(master_owned, &masters, "SeventySix.esm", &interner),
            Some(0x0200_003C)
        );

        let unlisted = FormKey {
            local: 0x0000_0001,
            plugin: interner.intern("NotLoaded.esm"),
        };
        assert_eq!(
            encoded_target_form_id(unlisted, &masters, "SeventySix.esm", &interner),
            None
        );
    }

    /// Attaching is idempotent, so a quest converted twice keeps one binding.
    #[test]
    fn startup_attach_is_idempotent() {
        let startup = startup_vmad(&[], "SeventySix.esm").expect("VMAD");
        let mut bytes = re_quest_vmad("rescript", FO4_RE_PARENT_PROPERTY);

        assert_eq!(
            attach_script_bytes(&mut bytes, STARTUP_SCRIPT_NAME, &startup),
            AttachResult::Changed
        );
        let after_first = bytes.clone();
        assert_eq!(
            attach_script_bytes(&mut bytes, STARTUP_SCRIPT_NAME, &startup),
            AttachResult::AlreadyPresent
        );
        assert_eq!(bytes, after_first);
        assert_eq!(
            has_top_level_script(&bytes, &script_is_encounter_script),
            Some(true),
            "the encounter script survives the attach"
        );

        let bytes = startup_vmad(&[], "SeventySix.esm").expect("VMAD");
        let text = String::from_utf8_lossy(&bytes);

        assert!(text.contains(STARTUP_SCRIPT_NAME));
        assert_eq!(
            has_top_level_script(&bytes, &|name| name == STARTUP_SCRIPT_NAME.as_bytes()),
            Some(true)
        );
        assert!(!text.contains(FO4_RE_PARENT_PROPERTY));
    }
}
