//! Fixup: strip WATR spell-slot references that do not point at SPEL records.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::source_read::form_key_to_read_str;
use crate::sym::StringInterner;

const SPELL_SIG: &str = "SPEL";
const WATER_SIG: &str = "WATR";
// FO4 WATR spell slots: XNAM = ConsumeSpell (applied when the water is drunk),
// YNAM = ContactSpell (applied while standing in it). Both accept a SPEL only.
const CONSUME_SPELL_SIG: &str = "XNAM";
const CONTACT_SPELL_SIG: &str = "YNAM";
const FO4_MASTER: &str = "Fallout4.esm";
// FO4 base-game "drinking" spells, paired with a contact/hazard spell exactly as
// Fallout4.esm's own water records pair them (ExtBloodyWater: 023F1B/024FBF,
// ExtLakeIrradiatedWater: 1FE6A3/1FE6A4).
const WATER_RADIATION_DRINKING: u32 = 0x02_4FBF;
const WATER_RADIATION_HIGH_HAZARD: u32 = 0x1F_E6A3;
const WATER_RADIATION_HIGH_DRINKING: u32 = 0x1F_E6A4;

pub struct FixWaterSpellRefsFixup;

impl Fixup for FixWaterSpellRefsFixup {
    fn name(&self) -> &'static str {
        "fix_water_spell_refs"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .target_slot()
            .parsed
            .game
            .as_deref()
            .is_some_and(|game| game.eq_ignore_ascii_case("fo4"))
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = session
            .schema()
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        let water_sig =
            SigCode::from_str(WATER_SIG).map_err(|e| FixupError::Other(e.to_string()))?;
        let water_fks = session
            .form_keys_of_sig(water_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if water_fks.is_empty() {
            return Ok(FixupReport::empty());
        }

        let target_handle_id = session.target_id();
        let output_plugin_name = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let target_master_handle_ids = config.target_master_handle_ids.clone();
        let mut changed_records = Vec::new();
        let mut stripped_refs = 0u32;
        let mut warnings = Vec::new();

        for water_fk in water_fks {
            let mut record =
                match session.record_decoded(&water_fk, target_schema.as_ref(), mapper.interner) {
                    Ok(record) => record,
                    Err(e) => {
                        let warning = mapper
                            .interner
                            .intern(&format!("fix_water_spell_refs_read_err:{e}"));
                        warnings.push(warning);
                        continue;
                    }
                };

            let removed = repair_water_spell_refs(&mut record, mapper.interner, &mut |fk| {
                record_signature_for_formkey(
                    session,
                    mapper.interner,
                    target_handle_id,
                    &output_plugin_name,
                    &target_masters,
                    &target_master_handle_ids,
                    fk,
                )
            });
            if removed > 0 {
                stripped_refs += removed;
                changed_records.push(record);
            }
        }

        let changed_record_count = changed_records.len() as u32;
        session
            .replace_records(changed_records, target_schema.as_ref(), mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        let mut report = FixupReport::empty();
        report.records_changed = changed_record_count;
        report.records_dropped = stripped_refs;
        report.warnings = warnings;
        Ok(report)
    }
}

fn record_signature_for_formkey(
    session: &mut PluginSession,
    interner: &StringInterner,
    target_handle_id: u64,
    output_plugin_name: &str,
    target_masters: &[String],
    target_master_handle_ids: &[u64],
    fk: &FormKey,
) -> Option<String> {
    if fk.local == 0 {
        return None;
    }

    let plugin = interner.resolve(fk.plugin)?;
    if plugin.eq_ignore_ascii_case(output_plugin_name) {
        let fk_str = form_key_to_read_str(fk, interner);
        return session
            .record_signature_in_handle(target_handle_id, &fk_str)
            .ok()
            .flatten();
    }

    for (master_name, handle_id) in target_masters.iter().zip(target_master_handle_ids.iter()) {
        if !plugin.eq_ignore_ascii_case(master_name) {
            continue;
        }
        let fk_str = format!("{master_name}:{:06X}", fk.local);
        return session
            .record_signature_in_handle(*handle_id, &fk_str)
            .ok()
            .flatten();
    }

    None
}

/// Paired FO4 "drinking" spell for a given ContactSpell (YNAM) value, following
/// Fallout4.esm's own water convention: high-hazard water uses the high drinking
/// spell, everything else the standard one.
fn paired_drinking_local(contact: &FormKey, interner: &StringInterner) -> u32 {
    let is_fo4_master = interner
        .resolve(contact.plugin)
        .is_some_and(|plugin| plugin.eq_ignore_ascii_case(FO4_MASTER));
    if is_fo4_master && (contact.local & 0x00FF_FFFF) == WATER_RADIATION_HIGH_HAZARD {
        WATER_RADIATION_HIGH_DRINKING
    } else {
        WATER_RADIATION_DRINKING
    }
}

/// Repair a WATR record's spell slots for FO4.
///
/// FO76 `WATR.XNAM` (ConsumeSpell) may point at an `ALCH` ingestible (e.g.
/// `WaterDirty`), which FO4's XNAM — a `SPEL`-only slot — cannot hold, so the
/// upstream type/dangle passes null or strip it and the converted water silently
/// loses its drink-on-consume effect. Instead of dropping it, repoint XNAM at the
/// FO4 drinking spell paired with the record's ContactSpell (YNAM), matching how
/// Fallout4.esm's own water records pair the two slots. When the earlier passes
/// already removed XNAM entirely, re-insert it before YNAM. `YNAM` keeps the
/// strip-if-not-`SPEL` behaviour. Returns the number of slots changed.
pub fn repair_water_spell_refs(
    record: &mut Record,
    interner: &StringInterner,
    signature_for_formkey: &mut dyn FnMut(&FormKey) -> Option<String>,
) -> u32 {
    if record.sig.as_str() != WATER_SIG {
        return 0;
    }
    let fo4_sym = interner.intern(FO4_MASTER);

    // Drinking spell paired with the ContactSpell, read before the retain pass
    // (which may strip an invalid YNAM).
    let drinking_local = record
        .fields
        .iter()
        .find(|entry| entry.sig.as_str() == CONTACT_SPELL_SIG)
        .and_then(|entry| match &entry.value {
            FieldValue::FormKey(fk) => Some(paired_drinking_local(fk, interner)),
            _ => None,
        })
        .unwrap_or(WATER_RADIATION_DRINKING);
    let drinking_fk = FieldValue::FormKey(FormKey {
        local: drinking_local,
        plugin: fo4_sym,
    });

    let mut changed = 0u32;
    let mut has_consume = false;
    record.fields.retain_mut(|entry| match entry.sig.as_str() {
        CONSUME_SPELL_SIG => {
            has_consume = true;
            let valid = matches!(
                &entry.value,
                FieldValue::FormKey(fk)
                    if fk.local != 0
                        && signature_for_formkey(fk).as_deref() == Some(SPELL_SIG)
            );
            if !valid {
                entry.value = drinking_fk.clone();
                changed += 1;
            }
            true
        }
        CONTACT_SPELL_SIG => {
            let keep = matches!(
                &entry.value,
                FieldValue::FormKey(fk)
                    if fk.local != 0
                        && signature_for_formkey(fk).as_deref() == Some(SPELL_SIG)
            );
            if !keep {
                changed += 1;
            }
            keep
        }
        _ => true,
    });

    // ConsumeSpell already removed by an earlier type/dangle pass, but the water
    // still carries a ContactSpell → restore the paired drinking spell (FO4 water
    // pairs both slots). Inserted just before YNAM to keep FO4 subrecord order.
    if !has_consume {
        if let Some(ynam_idx) = record
            .fields
            .iter()
            .position(|entry| entry.sig.as_str() == CONTACT_SPELL_SIG)
        {
            if let Ok(sig) = SubrecordSig::from_str(CONSUME_SPELL_SIG) {
                record.fields.insert(
                    ynam_idx,
                    FieldEntry {
                        sig,
                        value: drinking_fk,
                    },
                );
                changed += 1;
            }
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::{FieldEntry, RecordFlags};
    use smallvec::smallvec;

    fn fk(local: u32, plugin: &str, interner: &StringInterner) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn water_record(interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(WATER_SIG).unwrap(),
            form_key: fk(0x1000, "Output.esm", interner),
            eid: Some(interner.intern("TestWater")),
            flags: RecordFlags::empty(),
            fields: smallvec![
                field(
                    "XNAM",
                    FieldValue::FormKey(fk(0x0100, "Output.esm", interner))
                ),
                field(
                    "YNAM",
                    FieldValue::FormKey(fk(0x0200, "Output.esm", interner))
                ),
                field(
                    "INAM",
                    FieldValue::FormKey(fk(0x0300, "Output.esm", interner))
                ),
            ],
            warnings: smallvec![],
        }
    }

    fn field_fk<'a>(record: &'a Record, sig: &str) -> Option<&'a FormKey> {
        record
            .fields
            .iter()
            .find(|e| e.sig.as_str() == sig)
            .and_then(|e| match &e.value {
                FieldValue::FormKey(fk) => Some(fk),
                _ => None,
            })
    }

    fn watr(interner: &StringInterner, fields: Vec<FieldEntry>) -> Record {
        Record {
            sig: SigCode::from_str(WATER_SIG).unwrap(),
            form_key: fk(0x1000, "Output.esm", interner),
            eid: Some(interner.intern("TestWater")),
            flags: RecordFlags::empty(),
            fields: fields.into_iter().collect(),
            warnings: smallvec![],
        }
    }

    #[test]
    fn repairs_consume_spell_and_keeps_valid_contact() {
        // XNAM (ConsumeSpell) → ALCH is FO4-invalid; instead of stripping it we
        // repoint it at the paired FO4 drinking spell. YNAM (ContactSpell) → SPEL
        // is valid and left alone.
        let interner = StringInterner::new();
        let mut record = water_record(&interner);

        let changed = repair_water_spell_refs(&mut record, &interner, &mut |fk| match fk.local {
            0x0100 => Some("ALCH".to_string()),
            0x0200 => Some("SPEL".to_string()),
            _ => None,
        });

        assert_eq!(changed, 1);
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["XNAM", "YNAM", "INAM"]);
        let xnam = field_fk(&record, "XNAM").unwrap();
        assert_eq!(xnam.local, WATER_RADIATION_DRINKING);
        assert_eq!(interner.resolve(xnam.plugin), Some("Fallout4.esm"));
        assert_eq!(field_fk(&record, "YNAM").unwrap().local, 0x0200);
    }

    #[test]
    fn repairs_consume_spell_and_strips_missing_contact() {
        // XNAM unresolvable → repaired to drinking; YNAM unresolvable → stripped.
        let interner = StringInterner::new();
        let mut record = water_record(&interner);

        let changed = repair_water_spell_refs(&mut record, &interner, &mut |_| None);

        assert_eq!(changed, 2);
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["XNAM", "INAM"]);
        assert_eq!(
            field_fk(&record, "XNAM").unwrap().local,
            WATER_RADIATION_DRINKING
        );
    }

    #[test]
    fn pairs_high_hazard_contact_to_high_drinking() {
        let interner = StringInterner::new();
        let mut record = watr(
            &interner,
            vec![
                field(
                    "XNAM",
                    FieldValue::FormKey(fk(0x0100, "Output.esm", &interner)),
                ),
                field(
                    "YNAM",
                    FieldValue::FormKey(fk(WATER_RADIATION_HIGH_HAZARD, "Fallout4.esm", &interner)),
                ),
            ],
        );

        let changed = repair_water_spell_refs(&mut record, &interner, &mut |fk| match fk.local {
            WATER_RADIATION_HIGH_HAZARD => Some("SPEL".to_string()),
            _ => None,
        });

        assert_eq!(changed, 1);
        let xnam = field_fk(&record, "XNAM").unwrap();
        assert_eq!(xnam.local, WATER_RADIATION_HIGH_DRINKING);
        assert_eq!(interner.resolve(xnam.plugin), Some("Fallout4.esm"));
    }

    #[test]
    fn inserts_consume_spell_when_absent_with_valid_contact() {
        // Earlier passes removed XNAM entirely; a valid ContactSpell remains → the
        // paired drinking spell is inserted before YNAM.
        let interner = StringInterner::new();
        let mut record = watr(
            &interner,
            vec![
                field(
                    "YNAM",
                    FieldValue::FormKey(fk(0x0200, "Output.esm", &interner)),
                ),
                field(
                    "INAM",
                    FieldValue::FormKey(fk(0x0300, "Output.esm", &interner)),
                ),
            ],
        );

        let changed = repair_water_spell_refs(&mut record, &interner, &mut |fk| match fk.local {
            0x0200 => Some("SPEL".to_string()),
            _ => None,
        });

        assert_eq!(changed, 1);
        let sigs: Vec<&str> = record.fields.iter().map(|f| f.sig.as_str()).collect();
        assert_eq!(sigs, vec!["XNAM", "YNAM", "INAM"]);
        assert_eq!(
            field_fk(&record, "XNAM").unwrap().local,
            WATER_RADIATION_DRINKING
        );
    }

    #[test]
    fn keeps_valid_consume_spell() {
        let interner = StringInterner::new();
        let mut record = water_record(&interner);

        let changed = repair_water_spell_refs(&mut record, &interner, &mut |fk| match fk.local {
            0x0100 => Some("SPEL".to_string()),
            0x0200 => Some("SPEL".to_string()),
            _ => None,
        });

        assert_eq!(changed, 0);
        let xnam = field_fk(&record, "XNAM").unwrap();
        assert_eq!(xnam.local, 0x0100);
        assert_eq!(interner.resolve(xnam.plugin), Some("Output.esm"));
    }
}
