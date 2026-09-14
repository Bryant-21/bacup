//! Fixup: drop `FTYP` (Forced Loc Ref Type) from base records that a QUST alias
//! creates references from (`ALCO` "Create Reference to Object").
//!
//! `TESQuest::StartUp` promotes each unloaded alias-filled reference and branches on
//! `TESObjectREFR::HasLocationRefType()`, which is true when the *base* form carries
//! the `'CFLR'` component (an `FTYP` subrecord):
//!
//! ```text
//! if (ref->formFlags & 0x4000) {                          // unloaded
//!   if (ref->HasLocationRefType(nullptr)) {
//!     loc = ref->extraList->GetLocation();
//!     loc->ForEachSpecialRef(FindUnloadedDataForSpecialRef…);   // NO null check
//!   } else if (!ref->GetLinkedRef(nullptr)) {
//!     loc = ref->extraList->GetLocation();
//!     if (loc) …                                               // null-CHECKED
//!   }
//! }
//! ```
//!
//! A reference an alias *creates* (`ALCO` + `ALCA`) at a target in an unloaded cell
//! has no `ExtraLocation`, so `GetLocation()` returns NULL and the first branch
//! dereferences it: a hard CTD at quest start (`Fallout4.exe+066E2CD`,
//! `mov r11d,[rax+0x90]`, rax=0). Example: QUST `TW043` "Activity: Patrol Duty"
//! creating `LC043PenitentiaryGuardProtectron_Patrol` at `LC043GuardPod` in an
//! unloaded interior. FO76 sets `ForcedLocRefTypes` on ~23% of its actors;
//! `Fallout4.esm` sets `FTYP` on 4 of 3015 NPC_ records, so vanilla FO4 always takes
//! the null-checked branch.
//!
//! `FTYP` is removed only from the `ALCO` targets of the output QUSTs. It is what
//! makes LCRT-based alias fills match, so bases that are only placed keep theirs.
//! Idempotent.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;

pub struct StripAliasCreatedBaseLocRefTypesFixup;

impl Fixup for StripAliasCreatedBaseLocRefTypesFixup {
    fn name(&self) -> &'static str {
        "strip_alias_created_base_loc_ref_types"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, _session: &PluginSession, _config: &FixupConfig) -> bool {
        true
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
        let interner = mapper.interner;
        let own_plugin = interner.intern(&session.target_slot().parsed.plugin_name);

        let qust_sig =
            SigCode::from_str("QUST").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let qust_fks = session
            .form_keys_of_sig(qust_sig, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut created_bases: FxHashSet<FormKey> = FxHashSet::default();
        for fk in qust_fks {
            let Ok(record) = session.record_decoded(&fk, target_schema, interner) else {
                continue;
            };
            for base_fk in alias_created_object_keys(&record) {
                if base_fk.plugin == own_plugin {
                    created_bases.insert(base_fk);
                }
            }
        }

        let mut bases: Vec<FormKey> = created_bases.into_iter().collect();
        bases.sort_by_key(|fk| fk.local);

        let mut changed_records = Vec::new();
        for base_fk in bases {
            let Ok(mut record) = session.record_decoded(&base_fk, target_schema, interner) else {
                continue;
            };
            if strip_forced_loc_ref_type(&mut record) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_alias_created_base_loc_ref_types replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

/// Every `ALCO` "Create Reference to Object" target in a QUST's alias segment.
fn alias_created_object_keys(record: &Record) -> Vec<FormKey> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.as_str() == "ALCO")
        .filter_map(|entry| match &entry.value {
            FieldValue::FormKey(fk) => Some(*fk),
            _ => None,
        })
        .collect()
}

fn strip_forced_loc_ref_type(record: &mut Record) -> bool {
    let before = record.fields.len();
    record.fields.retain(|entry| entry.sig.as_str() != "FTYP");
    record.fields.len() != before
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;
    use smallvec::{SmallVec, smallvec};

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn record(sig: &str, form_key: FormKey, fields: SmallVec<[FieldEntry; 8]>) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key,
            eid: None,
            flags: RecordFlags::empty(),
            fields,
            warnings: SmallVec::new(),
        }
    }

    /// `ALCO` is a plain `formid` codec, so it reaches fixups decoded as a
    /// `FormKey` — unlike the `array_struct:` alias subrecords (ALLA), which
    /// arrive as raw `Bytes`. Reading the wrong shape would make this pass a
    /// silent no-op.
    #[test]
    fn collects_every_alco_target_from_the_alias_segment() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let guard = FormKey {
            local: 0x1E7C60,
            plugin,
        };
        let other = FormKey {
            local: 0x1E7C61,
            plugin,
        };
        let quest = record(
            "QUST",
            FormKey {
                local: 0x05A243,
                plugin,
            },
            smallvec![
                field("ALST", FieldValue::Uint(0)),
                field("ALCO", FieldValue::FormKey(guard)),
                field("ALCA", FieldValue::Int(7)),
                field("ALED", FieldValue::Bytes(SmallVec::new())),
                field("ALST", FieldValue::Uint(1)),
                field("ALCO", FieldValue::FormKey(other)),
                field("ALED", FieldValue::Bytes(SmallVec::new())),
            ],
        );

        assert_eq!(alias_created_object_keys(&quest), vec![guard, other]);
    }

    #[test]
    fn quest_without_create_aliases_yields_nothing() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let quest = record(
            "QUST",
            FormKey {
                local: 0x05A243,
                plugin,
            },
            smallvec![
                field("ALST", FieldValue::Uint(0)),
                field(
                    "ALFR",
                    FieldValue::FormKey(FormKey {
                        local: 0x380138,
                        plugin,
                    }),
                ),
                field("ALED", FieldValue::Bytes(SmallVec::new())),
            ],
        );

        assert!(alias_created_object_keys(&quest).is_empty());
    }

    #[test]
    fn strips_ftyp_and_is_idempotent() {
        let interner = StringInterner::new();
        let plugin = interner.intern("SeventySix.esm");
        let mut npc = record(
            "NPC_",
            FormKey {
                local: 0x1E7C60,
                plugin,
            },
            smallvec![
                field("EDID", FieldValue::Bytes(SmallVec::new())),
                field(
                    "FTYP",
                    FieldValue::FormKey(FormKey {
                        local: 0x5267DB,
                        plugin,
                    }),
                ),
                field("AIDT", FieldValue::Bytes(SmallVec::new())),
            ],
        );

        assert!(strip_forced_loc_ref_type(&mut npc));
        assert!(
            !npc.fields.iter().any(|entry| entry.sig.as_str() == "FTYP"),
            "FTYP must be gone"
        );
        assert_eq!(npc.fields.len(), 2, "only FTYP may be removed");
        assert!(!strip_forced_loc_ref_type(&mut npc));
    }
}
