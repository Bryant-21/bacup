//! Fixup: re-anchor IDLE tree parents at the vanilla Fallout4.esm actions.
//!
//! FO4's animation system attaches per-graph IDLE trees under its OWN action
//! records (`Fallout4.esm` AACT anchors like ActionInitializeGraphToBaseState).
//! FO76 renamed several of those inherited actions (ActionInitializeGraph,
//! ActionDeathAnimation, ActionCriticalHit, ActionSheathe, …), so the
//! conversion's vanilla dedup keeps LOCAL COPIES of them and IDLE
//! `ANAM` parent/previous references stay anchored at the copies. Branches
//! rooted at a copy are unreachable from the engine's default-object anchors —
//! the creature's InitializeGraph/Sheathe/Death idles are never found and the
//! actor holds the T-pose (megasloth root cause; proven in-game: loading a
//! port whose IDLEs anchor at Fallout4.esm fixes ours by presence alone).
//!
//! For every IDLE `ANAM` slot (parent at byte 0, previous at byte 4): when the
//! reference targets the OUTPUT plugin and that object-id exists as AACT in
//! BOTH the output plugin and Fallout4.esm, rewrite the master byte to the
//! Fallout4.esm index. FO76-only actions (e.g. 52BF9C
//! ActionSimulatedGraphEventCollection, absent from FO4) stay local.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::Sym;

pub struct RemapIdleAnchorActionsFixup;

impl Fixup for RemapIdleAnchorActionsFixup {
    fn name(&self) -> &'static str {
        "remap_idle_anchor_actions"
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

        let masters = session.target_masters().to_vec();
        let Some(fo4_index) = masters
            .iter()
            .position(|name| name.eq_ignore_ascii_case("Fallout4.esm"))
        else {
            return Ok(report);
        };
        let Some(&fo4_handle) = config.target_master_handle_ids.get(fo4_index) else {
            return Ok(report);
        };
        let output_master_index = masters.len() as u32;

        let aact_sig =
            SigCode::from_str("AACT").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let idle_sig =
            SigCode::from_str("IDLE").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let fo4_aact_objids: FxHashSet<u32> = session
            .form_keys_of_sig_in_handle(fo4_handle, aact_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .map(|fk| fk.local & 0x00FF_FFFF)
            .collect();
        let output_aact_objids: FxHashSet<u32> = session
            .form_keys_of_sig(aact_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?
            .into_iter()
            .map(|fk| fk.local & 0x00FF_FFFF)
            .collect();
        let remappable: FxHashSet<u32> = output_aact_objids
            .intersection(&fo4_aact_objids)
            .copied()
            .collect();
        if remappable.is_empty() {
            return Ok(report);
        }

        let output_plugin_sym = mapper.output_plugin_sym();
        let fo4_plugin_sym = mapper.interner.intern(&masters[fo4_index]);

        let ctx = AnchorRemapCtx {
            remappable,
            output_master_index,
            fo4_master_index: fo4_index as u32,
            output_plugin_sym,
            fo4_plugin_sym,
        };

        let idle_fks = session
            .form_keys_of_sig(idle_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut changed_records = Vec::new();
        for fk in idle_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(_) => continue,
            };
            if remap_idle_anchor_refs(&mut record, &ctx) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "remap_idle_anchor_actions replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

pub(crate) struct AnchorRemapCtx {
    /// Object-ids present as AACT in BOTH the output plugin and Fallout4.esm.
    pub remappable: FxHashSet<u32>,
    pub output_master_index: u32,
    pub fo4_master_index: u32,
    pub output_plugin_sym: Sym,
    pub fo4_plugin_sym: Sym,
}

/// Rewrite the master of ANAM parent/previous references that target output-
/// plugin copies of vanilla actions. Returns true when anything changed.
pub(crate) fn remap_idle_anchor_refs(record: &mut Record, ctx: &AnchorRemapCtx) -> bool {
    let Ok(anam_sig) = SubrecordSig::from_str("ANAM") else {
        return false;
    };
    let mut changed = false;
    for entry in &mut record.fields {
        if entry.sig != anam_sig {
            continue;
        }
        changed |= remap_value(&mut entry.value, ctx);
    }
    changed
}

fn remap_value(value: &mut FieldValue, ctx: &AnchorRemapCtx) -> bool {
    match value {
        FieldValue::Bytes(bytes) => {
            let mut changed = false;
            let mut offset = 0usize;
            while offset + 4 <= bytes.len() {
                let raw = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                if let Some(fixed) = remap_raw(raw, ctx) {
                    bytes[offset..offset + 4].copy_from_slice(&fixed.to_le_bytes());
                    changed = true;
                }
                offset += 4;
            }
            changed
        }
        FieldValue::FormKey(fk) => {
            if fk.plugin == ctx.output_plugin_sym && ctx.remappable.contains(&(fk.local & 0x00FF_FFFF))
            {
                fk.plugin = ctx.fo4_plugin_sym;
                true
            } else {
                false
            }
        }
        FieldValue::List(items) => {
            let mut changed = false;
            for item in items {
                changed |= remap_value(item, ctx);
            }
            changed
        }
        FieldValue::Struct(members) => {
            let mut changed = false;
            for (_, v) in members {
                changed |= remap_value(v, ctx);
            }
            changed
        }
        _ => false,
    }
}

fn remap_raw(raw: u32, ctx: &AnchorRemapCtx) -> Option<u32> {
    if raw == 0 {
        return None;
    }
    let master_index = raw >> 24;
    let object_id = raw & 0x00FF_FFFF;
    if master_index != ctx.output_master_index || !ctx.remappable.contains(&object_id) {
        return None;
    }
    Some((ctx.fo4_master_index << 24) | object_id)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::record::{FieldEntry, RecordFlags};
    use crate::sym::StringInterner;

    fn ctx(interner: &StringInterner) -> AnchorRemapCtx {
        AnchorRemapCtx {
            remappable: [0x02FFA9u32, 0x05704C, 0x05DD59, 0x02CBA4, 0x0489ED, 0x046BAF]
                .into_iter()
                .collect(),
            output_master_index: 2, // e.g. [Fallout4.esm, DLCRobot.esm] + output
            fo4_master_index: 0,
            output_plugin_sym: interner.intern("SeventySix.esm"),
            fo4_plugin_sym: interner.intern("Fallout4.esm"),
        }
    }

    fn idle_with_anam(bytes: Vec<u8>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("IDLE").unwrap(),
            form_key: FormKey {
                local: 0x123456,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: vec![FieldEntry {
                sig: SubrecordSig::from_str("ANAM").unwrap(),
                value: FieldValue::Bytes(bytes.into_iter().collect()),
            }]
            .into_iter()
            .collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn anam(parent: u32, previous: u32) -> Vec<u8> {
        let mut v = parent.to_le_bytes().to_vec();
        v.extend_from_slice(&previous.to_le_bytes());
        v
    }

    #[test]
    fn remaps_local_vanilla_action_anchor_to_fallout4() {
        let interner = StringInterner::new();
        // parent = output copy of ActionInitializeGraph, previous = in-set idle
        let mut rec = idle_with_anam(anam(0x0202FFA9, 0x02_0321D5), &interner);
        assert!(remap_idle_anchor_refs(&mut rec, &ctx(&interner)));
        let FieldValue::Bytes(b) = &rec.fields[0].value else {
            panic!("expected bytes");
        };
        assert_eq!(
            u32::from_le_bytes(b[0..4].try_into().unwrap()),
            0x0002FFA9,
            "parent must re-anchor at Fallout4.esm"
        );
        assert_eq!(
            u32::from_le_bytes(b[4..8].try_into().unwrap()),
            0x02_0321D5,
            "non-action previous ref must stay local"
        );
    }

    #[test]
    fn fo76_only_actions_and_foreign_masters_stay_untouched() {
        let interner = StringInterner::new();
        // parent = FO76-only ActionSimulatedGraphEventCollection (not remappable),
        // previous = already-vanilla anchor.
        let mut rec = idle_with_anam(anam(0x02_52BF9C, 0x00_0959F8), &interner);
        assert!(!remap_idle_anchor_refs(&mut rec, &ctx(&interner)));
    }

    #[test]
    fn null_and_short_values_are_ignored() {
        let interner = StringInterner::new();
        let mut rec = idle_with_anam(anam(0, 0), &interner);
        assert!(!remap_idle_anchor_refs(&mut rec, &ctx(&interner)));
        let mut short = idle_with_anam(vec![0x01, 0x02], &interner);
        assert!(!remap_idle_anchor_refs(&mut short, &ctx(&interner)));
    }

    #[test]
    fn formkey_leaves_remap_by_plugin() {
        let interner = StringInterner::new();
        let c = ctx(&interner);
        let mut rec = idle_with_anam(vec![], &interner);
        rec.fields[0].value = FieldValue::Struct(vec![
            (
                interner.intern("Parent"),
                FieldValue::FormKey(FormKey {
                    local: 0x02FFA9,
                    plugin: interner.intern("SeventySix.esm"),
                }),
            ),
            (
                interner.intern("Previous"),
                FieldValue::FormKey(FormKey {
                    local: 0x52BF9C,
                    plugin: interner.intern("SeventySix.esm"),
                }),
            ),
        ]);
        assert!(remap_idle_anchor_refs(&mut rec, &c));
        let FieldValue::Struct(members) = &rec.fields[0].value else {
            panic!("expected struct");
        };
        let FieldValue::FormKey(parent) = &members[0].1 else {
            panic!("expected fk");
        };
        let FieldValue::FormKey(previous) = &members[1].1 else {
            panic!("expected fk");
        };
        assert_eq!(parent.plugin, c.fo4_plugin_sym);
        assert_eq!(previous.plugin, c.output_plugin_sym, "FO76-only action stays local");
    }
}
