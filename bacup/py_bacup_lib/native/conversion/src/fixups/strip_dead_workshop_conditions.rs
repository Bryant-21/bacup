//! Fixup: drop COBJ conditions that gate on FO76-only workshop state.
//!
//! FO76 gates buildables on two states FO4 lacks: the workbench being a C.A.M.P. or
//! Shelter (`HasKeyword` on the condition's Target against `CampWorkshopKeyword` /
//! `Keyword_ShelterWorkshop`), and the player knowing a plan (`GetValue` on a
//! per-player ActorValue that FO76 sets only server-side). Neither can become true
//! after conversion, so such recipes are invisible at the workbench (like the Atom
//! Store gates `strip_atx_cobj_conditions` removes); the Crane Treasure Hunting sign
//! (W05_MQ_002P_Radical) cannot be built at all.
//!
//! Only the always-false form `<function>(<dead parameter>) == 1` is removed:
//! * `HasKeyword(<dead keyword>) == 0` stays: no FO4 workbench carries the keyword,
//!   so it is always true and blocks nothing.
//! * `W05_MQ_002P_Radical_PlayerCompleted002p` is not dead: the quest recipe reads it
//!   `== 0` and the post-quest recipe `== 1`, so leaving it unset selects exactly one
//!   sign. Stripping it would show both.
//!
//! Conditions OR-chain through bit 0 of the operator byte. Members are dropped chain
//! by chain and the OR bit re-stamped on survivors, so no chain ends on a condition
//! pointing at a removed successor.

use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;

const HAS_KEYWORD_FUNCTION: u32 = 560;
const GET_VALUE_FUNCTION: u32 = 14;

/// Workshop-type keywords no Fallout 4 workbench carries.
const DEAD_WORKSHOP_KEYWORDS: &[u32] = &[
    0x05_231A, // CampWorkshopKeyword
    0x5A_F4B2, // Keyword_ShelterWorkshop
];

/// Per-player "has learned / has unlocked" ActorValues only FO76's server sets.
const DEAD_KNOWLEDGE_ACTOR_VALUES: &[u32] = &[
    0x54_80BA, // W05_MQ_002P_Radical_PlayerLearnedSignRecipe
    0x54_31B7, // PETS_PlayerKnowsPlantRecipes
    0x54_31B8, // PETS_PlayerKnowsMeatRecipes
    0x58_56B4, // COMP_AV_CampObjectAvailable_Beggar
    0x58_56C4, // COMP_AV_CampObjectAvailable_Hunter
    0x58_56C5, // COMP_AV_CampObjectAvailable_Scavenger
    0x58_56C6, // COMP_AV_CampObjectAvailable_Wanderer
    0x56_9CC0, // COMP_AV_CampObjectAvailable_Beckett
    0x56_8E4A, // COMP_AV_CampObjectAvailable_RaiderPunk
    0x54_EB68, // COMP_AV_CampObjectAvailable_Astronaut
];

const CTDA_COMPARISON_VALUE_OFFSET: usize = 4;
const CTDA_FUNCTION_OFFSET: usize = 8;
const CTDA_PARAMETER_ONE_OFFSET: usize = 12;
const CTDA_MINIMUM_LENGTH: usize = 16;
const CTDA_OR_FLAG: u8 = 0x01;
const CTDA_OPERATOR_EQUAL: u8 = 0;

pub struct StripDeadWorkshopConditionsFixup;

impl Fixup for StripDeadWorkshopConditionsFixup {
    fn name(&self) -> &'static str {
        "strip_dead_workshop_conditions"
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
        let cobj_sig =
            SigCode::from_str("COBJ").map_err(|e| FixupError::SchemaError(e.to_string()))?;

        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let mut report = FixupReport::empty();
        let mut changed_records = Vec::new();

        let fks = session
            .form_keys_of_sig(cobj_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;

        for fk in fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(r) => r,
                Err(e) => {
                    let w = mapper
                        .interner
                        .intern(&format!("dead_workshop_cobj_read_err:{e}"));
                    report.warnings.push(w);
                    continue;
                }
            };

            if apply_to_record(&mut record) {
                changed_records.push(record);
                report.records_changed += 1;
            }
        }

        let expected = changed_records.len();
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_dead_workshop_conditions replaced {replaced} of {expected} expected records"
            )));
        }

        Ok(report)
    }
}

fn condition_bytes(entry: &FieldEntry) -> Option<&[u8]> {
    match &entry.value {
        FieldValue::Bytes(bytes) if bytes.len() >= CTDA_MINIMUM_LENGTH => Some(bytes.as_slice()),
        _ => None,
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes([
        bytes[offset],
        bytes[offset + 1],
        bytes[offset + 2],
        bytes[offset + 3],
    ])
}

/// Does this condition test a FO76-only fact that can never hold in FO4?
///
/// Parameter #1 is compared on its low three bytes only: the CTDA blob is
/// copied verbatim out of the source plugin, so its high byte still indexes
/// *that* plugin's master table. None of the object ids listed above exist in
/// Fallout4.esm, so a low-24 match cannot collide with a target-game record.
fn is_always_false(bytes: &[u8]) -> bool {
    if bytes.len() < CTDA_MINIMUM_LENGTH {
        return false;
    }
    if (bytes[0] >> 5) & 0x07 != CTDA_OPERATOR_EQUAL {
        return false;
    }
    if f32::from_bits(read_u32(bytes, CTDA_COMPARISON_VALUE_OFFSET)) != 1.0 {
        return false;
    }
    let parameter_one = read_u32(bytes, CTDA_PARAMETER_ONE_OFFSET) & 0x00FF_FFFF;
    match read_u32(bytes, CTDA_FUNCTION_OFFSET) {
        HAS_KEYWORD_FUNCTION => DEAD_WORKSHOP_KEYWORDS.contains(&parameter_one),
        GET_VALUE_FUNCTION => DEAD_KNOWLEDGE_ACTOR_VALUES.contains(&parameter_one),
        _ => false,
    }
}

/// Drop every always-false condition from `record`, re-stamping OR chains.
///
/// Returns `true` when at least one condition was removed.
pub fn apply_to_record(record: &mut Record) -> bool {
    let ctda_sig = match SubrecordSig::from_str("CTDA") {
        Ok(sig) => sig,
        Err(_) => return false,
    };

    // Split the conditions into OR chains: every member but the last carries
    // the OR flag.
    let mut chains: Vec<Vec<usize>> = Vec::new();
    let mut chain: Vec<usize> = Vec::new();
    for (index, entry) in record.fields.iter().enumerate() {
        if entry.sig != ctda_sig {
            continue;
        }
        chain.push(index);
        let continues = condition_bytes(entry).is_some_and(|bytes| bytes[0] & CTDA_OR_FLAG != 0);
        if !continues {
            chains.push(std::mem::take(&mut chain));
        }
    }
    if !chain.is_empty() {
        chains.push(chain);
    }

    let mut doomed: Vec<usize> = Vec::new();
    let mut restamp: Vec<(usize, bool)> = Vec::new();
    for chain in &chains {
        let survivors: Vec<usize> = chain
            .iter()
            .copied()
            .filter(|&index| !condition_bytes(&record.fields[index]).is_some_and(is_always_false))
            .collect();
        if survivors.len() == chain.len() {
            continue;
        }
        doomed.extend(chain.iter().copied().filter(|i| !survivors.contains(i)));
        for (position, &index) in survivors.iter().enumerate() {
            restamp.push((index, position + 1 < survivors.len()));
        }
    }

    if doomed.is_empty() {
        return false;
    }

    for (index, continues) in restamp {
        if let FieldValue::Bytes(bytes) = &mut record.fields[index].value {
            if continues {
                bytes[0] |= CTDA_OR_FLAG;
            } else {
                bytes[0] &= !CTDA_OR_FLAG;
            }
        }
    }

    let mut index = 0usize;
    record.fields.retain(|_| {
        let keep = !doomed.contains(&index);
        index += 1;
        keep
    });
    // No-op unless this COBJ carries a CITC, but keeps the count in lockstep
    // with the surviving CTDA rows if it does.
    record.sync_condition_count();
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::FormKey;
    use crate::record::RecordFlags;
    use crate::sym::StringInterner;
    use smallvec::SmallVec;

    const CAMP_KEYWORD: u32 = 0x05_231A;
    const SHELTER_KEYWORD: u32 = 0x5A_F4B2;
    const LEARNED_SIGN_RECIPE: u32 = 0x54_80BA;
    const COMPLETED_002P: u32 = 0x5A_20E9;

    /// Build a CTDA blob the way the source plugin ships one: parameter #1
    /// still carries the source master index in its high byte.
    fn ctda(function: u32, parameter_one: u32, value: f32, or_next: bool) -> FieldEntry {
        let mut bytes = [0u8; 32];
        bytes[0] = if or_next { CTDA_OR_FLAG } else { 0 };
        bytes[4..8].copy_from_slice(&value.to_bits().to_le_bytes());
        bytes[8..12].copy_from_slice(&function.to_le_bytes());
        bytes[12..16].copy_from_slice(&(0x0800_0000 | parameter_one).to_le_bytes());
        FieldEntry {
            sig: SubrecordSig::from_str("CTDA").unwrap(),
            value: FieldValue::Bytes(SmallVec::from_slice(&bytes)),
        }
    }

    fn cobj(entries: Vec<FieldEntry>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("COBJ").unwrap(),
            form_key: FormKey {
                local: 0x800,
                plugin: interner.intern("Output.esp"),
            },
            eid: Some(interner.intern("workshop_co_Thing")),
            flags: RecordFlags::empty(),
            fields: entries.into_iter().collect(),
            warnings: SmallVec::new(),
        }
    }

    fn or_flag(record: &Record, position: usize) -> bool {
        match &record.fields[position].value {
            FieldValue::Bytes(bytes) => bytes[0] & CTDA_OR_FLAG != 0,
            _ => panic!("expected a CTDA blob"),
        }
    }

    #[test]
    fn strips_camp_keyword_gate() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![ctda(HAS_KEYWORD_FUNCTION, CAMP_KEYWORD, 1.0, false)],
            &interner,
        );

        assert!(apply_to_record(&mut record));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn strips_learned_recipe_gate() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![ctda(GET_VALUE_FUNCTION, LEARNED_SIGN_RECIPE, 1.0, false)],
            &interner,
        );

        assert!(apply_to_record(&mut record));
        assert!(record.fields.is_empty());
    }

    #[test]
    fn collapses_a_camp_or_shelter_chain_whole() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![
                ctda(HAS_KEYWORD_FUNCTION, CAMP_KEYWORD, 1.0, true),
                ctda(HAS_KEYWORD_FUNCTION, SHELTER_KEYWORD, 1.0, false),
            ],
            &interner,
        );

        assert!(apply_to_record(&mut record));
        assert!(
            record.fields.is_empty(),
            "both members of the OR chain are always false"
        );
    }

    #[test]
    fn clears_the_or_flag_when_the_chain_tail_is_removed() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![
                // A live disjunct that ORs into a dead one.
                ctda(HAS_KEYWORD_FUNCTION, 0x00_1234, 1.0, true),
                ctda(HAS_KEYWORD_FUNCTION, CAMP_KEYWORD, 1.0, false),
            ],
            &interner,
        );

        assert!(apply_to_record(&mut record));
        assert_eq!(record.fields.len(), 1);
        assert!(
            !or_flag(&record, 0),
            "the surviving disjunct must not still point at a removed successor"
        );
    }

    #[test]
    fn keeps_the_negative_shelter_test() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![ctda(HAS_KEYWORD_FUNCTION, SHELTER_KEYWORD, 0.0, false)],
            &interner,
        );

        assert!(
            !apply_to_record(&mut record),
            "HasKeyword(Shelter) == 0 is always true in FO4, so it blocks nothing"
        );
        assert_eq!(record.fields.len(), 1);
    }

    #[test]
    fn keeps_both_sides_of_the_completed_quest_pair() {
        let interner = StringInterner::new();
        let mut quest = cobj(
            vec![ctda(GET_VALUE_FUNCTION, COMPLETED_002P, 0.0, false)],
            &interner,
        );
        let mut post_quest = cobj(
            vec![ctda(GET_VALUE_FUNCTION, COMPLETED_002P, 1.0, false)],
            &interner,
        );

        assert!(!apply_to_record(&mut quest));
        assert!(
            !apply_to_record(&mut post_quest),
            "leaving PlayerCompleted002p unset already selects exactly one recipe"
        );
    }

    #[test]
    fn leaves_unrelated_conditions_alone() {
        let interner = StringInterner::new();
        let mut record = cobj(
            vec![
                ctda(74, 0x3F_C7E7, 1.0, false),
                ctda(HAS_KEYWORD_FUNCTION, CAMP_KEYWORD, 1.0, false),
            ],
            &interner,
        );

        assert!(apply_to_record(&mut record));
        assert_eq!(record.fields.len(), 1, "the GetGlobalValue gate survives");
    }
}
