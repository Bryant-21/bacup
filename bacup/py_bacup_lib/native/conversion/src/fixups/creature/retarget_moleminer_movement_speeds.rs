//! Fixup: retarget MoleMiner movement-type speeds to the FO4 SuperMutant profile.
//!
//! Every FO76 MoleMiner stance MOVT carries forward walk 47 / run 255, the only
//! combat humanoid running below 300 in every stance (converted Scorched is
//! 80/320; FO76's crippled SuperMutant runs 331.8). FO4 treats MOVT as the
//! requested locomotion speed, so mole miners crawl. Each stance gets the
//! matching vanilla FO4 SuperMutant profile, the closest gun+melee humanoid:
//!
//! | MoleMiner MT              | FO4 source profile        | walk  | run    |
//! |---------------------------|---------------------------|-------|--------|
//! | Moleminer_Default_MT      | SuperMutant_MT            | 105.0 | 331.84 |
//! | Moleminer_Gun_MT          | SuperMutantGun_MT         |  70.0 | 300.0  |
//! | Moleminer_Melee_MT        | SuperMutantMelee_MT       | 87.55 | 331.84 |
//! | MoleMiner_Sighted_MT      | SuperMutantSighted_MT     | 87.55 | 200.0  |
//! | Moleminer_Blocking_MT     | SuperMutantBlocking_MT    | 105.0 | 105.0  |
//! | MoleMiner_DefaultScreenspace_MT | SuperMutantScreenspace_MT | 105.0 | 331.84 |
//!
//! SuperMutant profiles are direction-uniform, so one walk and one run value go
//! to all four directional slots; rotational speeds are untouched. MOVT.SPED has
//! four layout variants, so byte offsets come from the schema's union-aware
//! layout at form version 131, which every emitted record carries.

use crate::fixups::creature::creature_internal_fixup_applies;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;

/// (lowercase EditorID, walk, run) — vanilla FO4 SuperMutant stance values.
const STANCE_PROFILES: [(&str, f32, f32); 6] = [
    ("moleminer_default_mt", 105.0, 331.84),
    ("moleminer_gun_mt", 70.0, 300.0),
    ("moleminer_melee_mt", 87.55, 331.84),
    ("moleminer_sighted_mt", 87.55, 200.0),
    ("moleminer_blocking_mt", 105.0, 105.0),
    ("moleminer_defaultscreenspace_mt", 105.0, 331.84),
];

/// The eight directional speed fields. Matched by exact id — later SPED
/// variants add pitch/roll/yaw fields whose ids also end in `_walk`/`_run`.
const DIRECTIONAL_WALK_IDS: [&str; 4] = [
    "unnamed_left_walk",
    "unnamed_right_walk",
    "unnamed_forward_walk",
    "unnamed_back_walk",
];
const DIRECTIONAL_RUN_IDS: [&str; 4] = [
    "unnamed_left_run",
    "unnamed_right_run",
    "unnamed_forward_run",
    "unnamed_back_run",
];

/// The conversion stamps every emitted record to this form version; the
/// decoded `Record` does not carry one, so the SPED variant is resolved here.
const CONVERTED_FORM_VERSION: u16 = 131;

pub struct RetargetMoleminerMovementSpeedsFixup;

impl Fixup for RetargetMoleminerMovementSpeedsFixup {
    fn name(&self) -> &'static str {
        "retarget_moleminer_movement_speeds"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::CreatureGated
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        creature_internal_fixup_applies(ctx.config)
    }

    fn applies_to_session(&self, session: &PluginSession, config: &FixupConfig) -> bool {
        creature_internal_fixup_applies(config) && !crate::fo76_behaviors::enabled(session, config)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let movt_sig =
            SigCode::from_str("MOVT").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;

        let mut report = FixupReport::empty();
        let (walk_offsets, run_offsets) = directional_speed_offsets(target_schema);
        if walk_offsets.len() != 4 || run_offsets.len() != 4 {
            let w = mapper.interner.intern(&format!(
                "retarget_mm_movt: SPED layout resolved {}walk/{}run directional fields, expected 4/4",
                walk_offsets.len(),
                run_offsets.len()
            ));
            report.warnings.push(w);
            return Ok(report);
        }

        let movt_fks = session
            .form_keys_of_sig(movt_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in movt_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("retarget_mm_movt_read:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            let eid_lower = record
                .eid
                .and_then(|sym| mapper.interner.resolve(sym))
                .map(|s| s.to_ascii_lowercase())
                .unwrap_or_default();
            let Some(&(_, walk, run)) =
                STANCE_PROFILES.iter().find(|(eid, _, _)| *eid == eid_lower)
            else {
                continue;
            };

            if apply_to_record(&mut record, &walk_offsets, &run_offsets, walk, run) {
                session
                    .replace_record(record, target_schema, mapper.interner)
                    .map_err(|e| FixupError::HandleError(e.to_string()))?;
                report.records_changed += 1;
            }
        }

        Ok(report)
    }
}

// ---------------------------------------------------------------------------
// SPED layout resolution
// ---------------------------------------------------------------------------

/// Byte offsets of the four directional (walk, run) f32 fields, resolved from
/// the schema's union-aware SPED layout for the converted form version.
pub fn directional_speed_offsets(schema: &AuthoringSchema) -> (Vec<usize>, Vec<usize>) {
    let mut walks = Vec::new();
    let mut runs = Vec::new();
    for field in schema.struct_field_layout_versioned("MOVT", "SPED", Some(CONVERTED_FORM_VERSION))
    {
        if field.width != 4 {
            continue;
        }
        if DIRECTIONAL_WALK_IDS.contains(&field.field_id) {
            walks.push(field.offset);
        } else if DIRECTIONAL_RUN_IDS.contains(&field.field_id) {
            runs.push(field.offset);
        }
    }
    (walks, runs)
}

// ---------------------------------------------------------------------------
// Record-level mutation
// ---------------------------------------------------------------------------

/// Write `walk`/`run` into the given directional speed slots of the SPED
/// subrecord. Returns `true` if any byte changed.
pub fn apply_to_record(
    record: &mut Record,
    walk_offsets: &[usize],
    run_offsets: &[usize],
    walk: f32,
    run: f32,
) -> bool {
    let sped_sig = match SubrecordSig::from_str("SPED") {
        Ok(s) => s,
        Err(_) => return false,
    };
    let min_len = walk_offsets
        .iter()
        .chain(run_offsets)
        .map(|o| o + 4)
        .max()
        .unwrap_or(0);

    let mut mutated = false;
    for entry in &mut record.fields {
        if entry.sig != sped_sig {
            continue;
        }
        if let FieldValue::Bytes(ref mut data) = entry.value {
            if data.len() < min_len {
                break;
            }
            for &off in walk_offsets {
                mutated |= write_f32(data, off, walk);
            }
            for &off in run_offsets {
                mutated |= write_f32(data, off, run);
            }
        }
        break; // Only one SPED.
    }

    mutated
}

fn write_f32(data: &mut [u8], offset: usize, value: f32) -> bool {
    let bytes = value.to_le_bytes();
    if data[offset..offset + 4] == bytes {
        return false;
    }
    data[offset..offset + 4].copy_from_slice(&bytes);
    true
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
    use std::sync::Arc;

    const WALK_OFFSETS: [usize; 4] = [4, 20, 36, 52];
    const RUN_OFFSETS: [usize; 4] = [8, 24, 40, 56];

    /// Build a 124-byte SPED with the converted MoleMiner profile:
    /// L 45/207, R 45/225, F 47/255, B 35/150.
    fn make_moleminer_sped() -> smallvec::SmallVec<[u8; 32]> {
        let mut data: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        data.resize(124, 0u8);
        for (off, v) in [
            (4usize, 45.0f32),
            (8, 207.0),
            (20, 45.0),
            (24, 225.0),
            (36, 47.0),
            (40, 255.0),
            (52, 35.0),
            (56, 150.0),
        ] {
            data[off..off + 4].copy_from_slice(&v.to_le_bytes());
        }
        data
    }

    fn make_movt(interner: &StringInterner, sped: smallvec::SmallVec<[u8; 32]>) -> Record {
        Record {
            sig: SigCode::from_str("MOVT").unwrap(),
            form_key: FormKey {
                local: 0x0616FD,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: smallvec::smallvec![FieldEntry {
                sig: SubrecordSig::from_str("SPED").unwrap(),
                value: FieldValue::Bytes(sped),
            }],
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn read_f32(data: &[u8], off: usize) -> f32 {
        f32::from_le_bytes([data[off], data[off + 1], data[off + 2], data[off + 3]])
    }

    #[test]
    fn schema_resolves_converted_variant_directional_offsets() {
        // Oracle: the fv131 SPED variant must place the directional floats at
        // the offsets the byte-tests use — and exclude pitch/roll/yaw fields.
        let schema = Arc::new(AuthoringSchema::for_game("fo4").unwrap());
        let (walks, runs) = directional_speed_offsets(&schema);
        assert_eq!(walks, WALK_OFFSETS.to_vec());
        assert_eq!(runs, RUN_OFFSETS.to_vec());
    }

    #[test]
    fn stamps_all_four_direction_pairs() {
        let interner = StringInterner::new();
        let mut record = make_movt(&interner, make_moleminer_sped());
        assert!(apply_to_record(
            &mut record,
            &WALK_OFFSETS,
            &RUN_OFFSETS,
            105.0,
            331.84
        ));

        let FieldValue::Bytes(ref data) = record.fields[0].value else {
            panic!("SPED must stay Bytes");
        };
        for off in WALK_OFFSETS {
            assert_eq!(read_f32(data, off), 105.0);
        }
        for off in RUN_OFFSETS {
            assert_eq!(read_f32(data, off), 331.84);
        }
        // Rotational tail bytes must stay untouched (zeroed in the fixture).
        assert_eq!(read_f32(data, 68), 0.0);
        assert_eq!(read_f32(data, 72), 0.0);
    }

    #[test]
    fn second_application_is_no_op() {
        let interner = StringInterner::new();
        let mut record = make_movt(&interner, make_moleminer_sped());
        assert!(apply_to_record(
            &mut record,
            &WALK_OFFSETS,
            &RUN_OFFSETS,
            70.0,
            300.0
        ));
        assert!(!apply_to_record(
            &mut record,
            &WALK_OFFSETS,
            &RUN_OFFSETS,
            70.0,
            300.0
        ));
    }

    #[test]
    fn short_sped_is_skipped() {
        let interner = StringInterner::new();
        let mut short: smallvec::SmallVec<[u8; 32]> = smallvec::SmallVec::new();
        short.resize(16, 0u8);
        let mut record = make_movt(&interner, short);
        assert!(!apply_to_record(
            &mut record,
            &WALK_OFFSETS,
            &RUN_OFFSETS,
            105.0,
            331.84
        ));
    }

    #[test]
    fn stance_table_covers_every_moleminer_movt() {
        // The six stance records shipped by the conversion (form ids
        // 0616FD..061704) must all resolve to a profile.
        for eid in [
            "Moleminer_Default_MT",
            "Moleminer_Gun_MT",
            "Moleminer_Melee_MT",
            "MoleMiner_Sighted_MT",
            "Moleminer_Blocking_MT",
            "MoleMiner_DefaultScreenspace_MT",
        ] {
            let lower = eid.to_ascii_lowercase();
            assert!(
                STANCE_PROFILES.iter().any(|(e, _, _)| *e == lower),
                "no profile for {eid}"
            );
        }
    }
}
