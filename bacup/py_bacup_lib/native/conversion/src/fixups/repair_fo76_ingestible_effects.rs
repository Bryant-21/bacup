//! Repair verified FO76 ingestibles whose magic-effect contract is incomplete in FO4.

use rustc_hash::FxHashSet;

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

const SOURCE_MASTER: &str = "SeventySix.esm";
const TARGET_MASTER: &str = "Fallout4.esm";
const FO4_STIMPAK_LOCAL: u32 = 0x0002_3736;
const FO4_STIMPAK_HEALTH_EFFECT_LOCAL: u32 = 0x0021_DDB8;

const VERIFIED_STIMPAKS: &[(u32, &str, RepairMode)] = &[
    (0x0002_3736, "Stimpak", RepairMode::SupportEffects),
    (0x0041_557C, "StimGas", RepairMode::FullEffects),
    (0x0030_78C2, "StimpakDiluted", RepairMode::SupportEffects),
    (0x0011_7DF9, "SuperStimpak", RepairMode::SupportEffects),
];

#[derive(Clone, Copy)]
enum RepairMode {
    SupportEffects,
    FullEffects,
}

pub struct RepairFo76IngestibleEffectsFixup;

impl Fixup for RepairFo76IngestibleEffectsFixup {
    fn name(&self) -> &'static str {
        "repair_fo76_ingestible_effects"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .target_masters()
            .iter()
            .any(|master| master.eq_ignore_ascii_case(TARGET_MASTER))
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
        let alch_sig = SigCode::from_str("ALCH").map_err(FixupError::SchemaError)?;
        let form_keys = session
            .form_keys_of_sig(alch_sig, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if !form_keys.iter().any(|form_key| {
            VERIFIED_STIMPAKS
                .iter()
                .any(|(local, _, _)| form_key.local == *local)
        }) {
            return Ok(report);
        }

        let Some(fallout4_handle) = session
            .target_masters()
            .iter()
            .zip(config.target_master_handle_ids.iter().copied())
            .find_map(|(master, handle)| {
                master.eq_ignore_ascii_case(TARGET_MASTER).then_some(handle)
            })
        else {
            return Ok(report);
        };
        let fallout4_sym = mapper.interner.intern(TARGET_MASTER);
        let vanilla_stimpak = session
            .record_decoded_in_handle(
                fallout4_handle,
                &FormKey {
                    plugin: fallout4_sym,
                    local: FO4_STIMPAK_LOCAL,
                },
                target_schema,
                mapper.interner,
            )
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        let vanilla_effects = effect_groups(&vanilla_stimpak, mapper.interner);
        if vanilla_effects.is_empty() {
            return Err(FixupError::Other(
                "Fallout4.esm Stimpak has no decodable effects".into(),
            ));
        }

        let mut changed_records = Vec::new();
        for form_key in form_keys {
            let Some((_, expected_eid, mode)) = VERIFIED_STIMPAKS
                .iter()
                .find(|(local, _, _)| form_key.local == *local)
            else {
                continue;
            };
            let mut record = match session.record_decoded(&form_key, target_schema, mapper.interner)
            {
                Ok(record) => record,
                Err(_) => continue,
            };
            if !is_verified_stimpak(&record, expected_eid, mapper.interner) {
                continue;
            }
            if repair_stimpak_effects(
                &mut record,
                *mode,
                &vanilla_effects,
                fallout4_sym,
                mapper.interner,
            ) {
                changed_records.push(record);
            }
        }

        if changed_records.is_empty() {
            return Ok(report);
        }
        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|error| FixupError::HandleError(error.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "repair_fo76_ingestible_effects replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn is_verified_stimpak(record: &Record, expected_eid: &str, interner: &StringInterner) -> bool {
    record.sig.as_str() == "ALCH"
        && interner
            .resolve(record.form_key.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(SOURCE_MASTER))
        && record.eid.and_then(|eid| interner.resolve(eid)) == Some(expected_eid)
}

fn repair_stimpak_effects(
    record: &mut Record,
    mode: RepairMode,
    vanilla_effects: &[(u32, Vec<FieldEntry>)],
    fallout4_sym: Sym,
    interner: &StringInterner,
) -> bool {
    let mut changed = false;
    for field in &mut record.fields {
        if field.sig.as_str() != "EFID" {
            continue;
        }
        let FieldValue::FormKey(effect) = &mut field.value else {
            continue;
        };
        if effect.local == FO4_STIMPAK_HEALTH_EFFECT_LOCAL
            && interner
                .resolve(effect.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(SOURCE_MASTER))
        {
            effect.plugin = fallout4_sym;
            changed = true;
        }
    }

    let mut present = effect_form_keys(record)
        .into_iter()
        .collect::<FxHashSet<_>>();
    for (effect_local, fields) in vanilla_effects {
        if matches!(mode, RepairMode::SupportEffects)
            && *effect_local == FO4_STIMPAK_HEALTH_EFFECT_LOCAL
        {
            continue;
        }
        if !present.insert((fallout4_sym, *effect_local)) {
            continue;
        }
        record.fields.extend(fields.iter().cloned());
        changed = true;
    }
    changed
}

fn effect_groups(record: &Record, interner: &StringInterner) -> Vec<(u32, Vec<FieldEntry>)> {
    let mut groups = Vec::new();
    let mut index = 0;
    while index < record.fields.len() {
        if record.fields[index].sig.as_str() != "EFID" {
            index += 1;
            continue;
        }
        let end = record.fields[index + 1..]
            .iter()
            .position(|field| field.sig.as_str() == "EFID")
            .map(|offset| index + 1 + offset)
            .unwrap_or(record.fields.len());
        if let Some((_, local)) = effect_form_key(&record.fields[index].value) {
            let mut fields = record.fields[index..end].to_vec();
            fields[0].value = FieldValue::FormKey(FormKey {
                plugin: interner.intern(TARGET_MASTER),
                local,
            });
            groups.push((local, fields));
        }
        index = end;
    }
    groups
}

fn effect_form_keys(record: &Record) -> Vec<(Sym, u32)> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "EFID")
        .filter_map(|field| effect_form_key(&field.value))
        .collect()
}

fn effect_form_key(value: &FieldValue) -> Option<(Sym, u32)> {
    match value {
        FieldValue::FormKey(form_key) => Some((form_key.plugin, form_key.local)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use smallvec::SmallVec;

    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::RecordFlags;

    fn field(sig: &str, value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        }
    }

    fn effect(plugin: Sym, local: u32, marker: u8) -> Vec<FieldEntry> {
        vec![
            field("EFID", FieldValue::FormKey(FormKey { plugin, local })),
            field(
                "EFIT",
                FieldValue::Bytes(SmallVec::from_slice(&[marker; 12])),
            ),
        ]
    }

    fn record(interner: &StringInterner, local: u32, eid: &str) -> Record {
        Record {
            sig: SigCode::from_str("ALCH").unwrap(),
            form_key: FormKey {
                plugin: interner.intern(SOURCE_MASTER),
                local,
            },
            eid: Some(interner.intern(eid)),
            flags: RecordFlags::empty(),
            fields: SmallVec::new(),
            warnings: SmallVec::new(),
        }
    }

    fn vanilla_effects(interner: &StringInterner) -> Vec<(u32, Vec<FieldEntry>)> {
        let fallout4 = interner.intern(TARGET_MASTER);
        [
            FO4_STIMPAK_HEALTH_EFFECT_LOCAL,
            0x0001_8A86,
            0x0005_C529,
            0x0005_C52C,
            0x0005_C52D,
            0x0005_C52A,
            0x0005_C52B,
            0x0005_C52E,
        ]
        .into_iter()
        .enumerate()
        .map(|(index, local)| (local, effect(fallout4, local, index as u8)))
        .collect()
    }

    #[test]
    fn super_stimpak_retargets_health_and_adds_support_effects() {
        let interner = StringInterner::new();
        let source = interner.intern(SOURCE_MASTER);
        let fallout4 = interner.intern(TARGET_MASTER);
        let mut record = record(&interner, 0x0011_7DF9, "SuperStimpak");
        record
            .fields
            .extend(effect(source, FO4_STIMPAK_HEALTH_EFFECT_LOCAL, 1));
        record
            .fields
            .extend(effect(source, FO4_STIMPAK_HEALTH_EFFECT_LOCAL, 2));

        assert!(repair_stimpak_effects(
            &mut record,
            RepairMode::SupportEffects,
            &vanilla_effects(&interner),
            fallout4,
            &interner,
        ));

        let effects = effect_form_keys(&record);
        assert_eq!(effects.len(), 9);
        assert_eq!(
            effects
                .iter()
                .filter(|effect| **effect == (fallout4, FO4_STIMPAK_HEALTH_EFFECT_LOCAL))
                .count(),
            2
        );
        assert!(effects.iter().all(|(plugin, _)| *plugin == fallout4));
    }

    #[test]
    fn diffuser_preserves_team_effect_and_adds_full_vanilla_contract() {
        let interner = StringInterner::new();
        let source = interner.intern(SOURCE_MASTER);
        let fallout4 = interner.intern(TARGET_MASTER);
        let mut record = record(&interner, 0x0041_557C, "StimGas");
        record.fields.extend(effect(source, 0x0041_557B, 0));

        assert!(repair_stimpak_effects(
            &mut record,
            RepairMode::FullEffects,
            &vanilla_effects(&interner),
            fallout4,
            &interner,
        ));

        let effects = effect_form_keys(&record);
        assert_eq!(effects.len(), 9);
        assert!(effects.contains(&(source, 0x0041_557B)));
        assert!(effects.contains(&(fallout4, FO4_STIMPAK_HEALTH_EFFECT_LOCAL)));
    }

    #[test]
    fn repair_is_idempotent() {
        let interner = StringInterner::new();
        let source = interner.intern(SOURCE_MASTER);
        let fallout4 = interner.intern(TARGET_MASTER);
        let mut record = record(&interner, 0x0030_78C2, "StimpakDiluted");
        record
            .fields
            .extend(effect(source, FO4_STIMPAK_HEALTH_EFFECT_LOCAL, 1));
        let vanilla = vanilla_effects(&interner);

        assert!(repair_stimpak_effects(
            &mut record,
            RepairMode::SupportEffects,
            &vanilla,
            fallout4,
            &interner,
        ));
        let once = record.fields.clone();
        assert!(!repair_stimpak_effects(
            &mut record,
            RepairMode::SupportEffects,
            &vanilla,
            fallout4,
            &interner,
        ));
        assert_eq!(record.fields, once);
    }

    #[test]
    fn identity_guard_rejects_unrelated_ingestibles() {
        let interner = StringInterner::new();
        let unrelated = record(&interner, 0x0011_7DF9, "NotSuperStimpak");
        assert!(!is_verified_stimpak(&unrelated, "SuperStimpak", &interner));
    }
}
