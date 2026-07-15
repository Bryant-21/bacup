//! Drop FO76 player IDLE branches that override incompatible FO4 anchors.
//!
//! FO76 adds children beneath vanilla FO4 player-animation leaves. FO4 selects
//! those children, but its behavior graphs do not implement every event they
//! emit. In particular, `RightReleaseChargingHoldForceFire` emits
//! `attackReleaseChargingHoldForceFire`, which is absent from FO4's first-person
//! GunBehavior graph. Charged weapons then reach full charge without firing.
//!
//! FO76 also corrects FO4's `RaiderSeakRoot` typo to `RaiderSneakRoot`. EID
//! deduplication therefore keeps an empty duplicate under `ActionSneak`, which
//! swallows third-person crouch selection. The related FO76-only gun-sneak
//! branch is incompatible with the same FO4 tree.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SigCode;
use crate::session::PluginSession;

const INCOMPATIBLE_PLAYER_IDLE_EDITOR_IDS: &[&str] = &[
    "RaiderSneakRoot",
    "GunReadySneak",
    "RightReleaseChargingHoldEnd",
    "RightReleaseChargingHoldForceFire",
    "RightReleaseChargingHoldDefault",
];

pub struct DropIncompatiblePlayerIdlesFixup;

impl Fixup for DropIncompatiblePlayerIdlesFixup {
    fn name(&self) -> &'static str {
        "drop_incompatible_player_idles"
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
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let idle_sig =
            SigCode::from_str("IDLE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let idle_fks = session
            .form_keys_of_sig(idle_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut dropped_eids = Vec::new();
        for fk in idle_fks {
            let record = match session.record_decoded(&fk, &target_schema, mapper.interner) {
                Ok(record) => record,
                Err(_) => continue,
            };
            let Some(eid) = record.eid.and_then(|sym| mapper.interner.resolve(sym)) else {
                continue;
            };
            if !is_incompatible_player_idle(eid) {
                continue;
            }

            if session
                .remove_record(&fk)
                .map_err(|e| FixupError::HandleError(e.to_string()))?
            {
                dropped_eids.push(eid.to_string());
            }
        }

        let mut report = FixupReport::empty();
        report.records_dropped = dropped_eids.len().try_into().unwrap_or(u32::MAX);
        if !dropped_eids.is_empty() {
            dropped_eids.sort_unstable();
            report.warnings.push(mapper.interner.intern(&format!(
                "Dropped FO4-incompatible FO76 player IDLE branches: {dropped_eids:?}"
            )));
        }
        Ok(report)
    }
}

fn is_incompatible_player_idle(eid: &str) -> bool {
    INCOMPATIBLE_PLAYER_IDLE_EDITOR_IDS
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(eid))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record, RecordFlags};
    use crate::session::open_session;
    use crate::sym::StringInterner;
    use esp_authoring_core::plugin_runtime::plugin_handle_new_native;
    use smallvec::SmallVec;

    fn record(sig: &str, local: u32, eid: &str, interner: &StringInterner) -> Record {
        let eid = interner.intern(eid);
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: Some(eid),
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("EDID").unwrap(),
                value: FieldValue::String(eid),
            }],
            warnings: SmallVec::new(),
        }
    }

    #[test]
    fn matches_only_confirmed_incompatible_editor_ids() {
        for eid in INCOMPATIBLE_PLAYER_IDLE_EDITOR_IDS {
            assert!(is_incompatible_player_idle(eid));
            assert!(is_incompatible_player_idle(&eid.to_ascii_lowercase()));
        }
        assert!(!is_incompatible_player_idle("RightReleaseChargingHold"));
        assert!(!is_incompatible_player_idle("RaiderSeakRoot"));
        assert!(!is_incompatible_player_idle("GunReadyRoot"));
    }

    #[test]
    fn drops_five_idles_without_touching_other_records() {
        let interner = StringInterner::new();
        let source = plugin_handle_new_native("SeventySix.esm", Some("fo76")).unwrap();
        let target = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        let mut session = open_session(target, Some(source)).unwrap();
        let schema = session.schema().unwrap();

        for (index, eid) in INCOMPATIBLE_PLAYER_IDLE_EDITOR_IDS.iter().enumerate() {
            session
                .add_record(
                    record("IDLE", 0x800 + index as u32, eid, &interner),
                    schema.as_ref(),
                    &interner,
                )
                .unwrap();
        }
        session
            .add_record(
                record("IDLE", 0x900, "RightReleaseChargingHold", &interner),
                schema.as_ref(),
                &interner,
            )
            .unwrap();
        session
            .add_record(
                record("WEAP", 0x901, "GunReadySneak", &interner),
                schema.as_ref(),
                &interner,
            )
            .unwrap();

        let mut state = MapperState::new(std::iter::empty(), MapperOptions::default());
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        let report = DropIncompatiblePlayerIdlesFixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap();

        assert_eq!(report.records_dropped, 5);
        let remaining_idles = session
            .form_keys_of_sig(SigCode::from_str("IDLE").unwrap(), &interner)
            .unwrap();
        assert_eq!(remaining_idles.len(), 1);
        assert_eq!(remaining_idles[0].local, 0x900);
        let remaining_weapons = session
            .form_keys_of_sig(SigCode::from_str("WEAP").unwrap(), &interner)
            .unwrap();
        assert_eq!(remaining_weapons.len(), 1);
        assert_eq!(remaining_weapons[0].local, 0x901);
    }
}
