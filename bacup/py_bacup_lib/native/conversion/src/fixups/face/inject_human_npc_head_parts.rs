//! Inject concrete head parts into converted humanoid NPCs that carry face
//! data but no `PNAM` head-part block.
//!
//! The FO76->FO4 whole-plugin path leaves `ConvertFacePhase` off (it rewrites
//! more face payload than the port can validate), so output can hold HumanRace
//! NPCs with `MSDK`/`FMRI` face data and no `PNAM`. FO4 then queues a head load
//! with no head parts and can crash building the head skin. HumanRace NPCs get
//! FO4's raider-template head-part sets; custom humanoid races get the converted
//! race's own head-part list.

use rustc_hash::FxHashMap;

use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::schema::AuthoringSchema;
use crate::session::PluginSession;
use crate::sym::{StringInterner, Sym};

const BASE_MASTER: &str = "Fallout4.esm";
const HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const GHOUL_RACE_LOCAL: u32 = 0x000E_AFB6;
const HUMAN_CHILD_RACE_LOCAL: u32 = 0x0011_D83F;
const NPC_ACBS_FEMALE_FLAG: u32 = 0x0000_0001;

#[derive(Clone, Debug, Default)]
struct RaceHeadParts {
    male: Vec<FormKey>,
    female: Vec<FormKey>,
}

const DEFAULT_MALE_HUMAN_HEAD_PARTS: &[u32] = &[
    0x0005_1631,
    0x0006_39DC,
    0x0006_39DD,
    0x0006_39DE,
    0x0006_39DF,
    0x001D_2A66,
    0x001D_2A65,
    0x0001_F735,
    0x0014_108F,
    0x0008_44A7,
    0x0011_CCBD,
    0x0004_23AF,
    0x0001_EEBB,
];

const DEFAULT_FEMALE_HUMAN_HEAD_PARTS: &[u32] = &[
    0x001D_98DB,
    0x001D_98D8,
    0x001D_98D9,
    0x001D_98DA,
    0x0014_EC29,
    0x0004_D0EC,
    0x000F_159E,
    0x0014_EC22,
    0x0015_A166,
    0x0004_D0E9,
    0x000C_FB3F,
];

const DEFAULT_MALE_GHOUL_HEAD_PARTS: &[u32] = &[
    0x0005_1631,
    0x000E_AFBB,
    0x0017_A5E9,
    0x0005_18FD,
    0x001C_206F,
];
const DEFAULT_FEMALE_GHOUL_HEAD_PARTS: &[u32] = &[
    0x000C_FB4E,
    0x0004_D0E9,
    0x000F_6E5A,
    0x0017_A5EC,
    0x0011_CCBC,
];
const DEFAULT_MALE_CHILD_HEAD_PARTS: &[u32] = &[
    0x0024_7507,
    0x0011_D802,
    0x0011_D804,
    0x0011_E7B0,
    0x0017_9620,
];
const DEFAULT_FEMALE_CHILD_HEAD_PARTS: &[u32] = &[
    0x0024_7508,
    0x0011_D804,
    0x0011_D801,
    0x0011_E7B1,
    0x0017_9D22,
];

/// Fallback hair color for a head-part-injected HumanRace NPC that carries no
/// usable `HCLF`. This is FO76 `HumanRace`'s own default hair color
/// (`HairColor01LightRed`), which shares its local FormID with FO4 — so the
/// fallback is itself a faithful carry of FO76 data, not an invented default.
const DEFAULT_HUMAN_HAIR_COLOR_FO4: u32 = 0x000A_0439;

/// FO76 hair-color `CLFM` local FormID → FO4 hair-color `CLFM` local FormID.
///
/// `HairColor01`–`22` share local FormIDs across both games (only the master
/// changes). FO76's `HairColor23`–`32` (`3E48C0`..`3E48C9`) are FO4's
/// `HairColorNN_DLC04` (`24A04E`..`24A058`, in `Fallout4.esm`) with identical
/// `ColorIndex`, so the mapping is exact.
const FO76_TO_FO4_HAIR_COLOR: &[(u32, u32)] = &[
    // HairColor01–06 (identity)
    (0x000A_0439, 0x000A_0439),
    (0x000A_042D, 0x000A_042D),
    (0x000A_042F, 0x000A_042F),
    (0x000A_042E, 0x000A_042E),
    (0x000A_042C, 0x000A_042C),
    (0x000A_0431, 0x000A_0431),
    // HairColor07–22 (identity)
    (0x0019_EE5A, 0x0019_EE5A),
    (0x0019_EE5B, 0x0019_EE5B),
    (0x0019_EE5C, 0x0019_EE5C),
    (0x0019_EE5D, 0x0019_EE5D),
    (0x0019_EE5E, 0x0019_EE5E),
    (0x0019_EE5F, 0x0019_EE5F),
    (0x0019_EE60, 0x0019_EE60),
    (0x0019_EE61, 0x0019_EE61),
    (0x0019_EE62, 0x0019_EE62),
    (0x0019_EE63, 0x0019_EE63),
    (0x0019_EE64, 0x0019_EE64),
    (0x0019_EE65, 0x0019_EE65),
    (0x0019_EE66, 0x0019_EE66),
    (0x0019_EE67, 0x0019_EE67),
    (0x0019_EE68, 0x0019_EE68),
    (0x0019_EE69, 0x0019_EE69),
    // HairColor23–32 (FO76 → FO4 DLC04 renumber)
    (0x003E_48C0, 0x0024_A04E), // Purple
    (0x003E_48C1, 0x0024_A050), // Blue
    (0x003E_48C2, 0x0024_A051), // BlueGreen
    (0x003E_48C3, 0x0024_A052), // Green
    (0x003E_48C4, 0x0024_A053), // YellowGreen
    (0x003E_48C5, 0x0024_A054), // LightPink
    (0x003E_48C6, 0x0024_A055), // Orange
    (0x003E_48C7, 0x0024_A056), // Red
    (0x003E_48C8, 0x0024_A057), // RedViolet
    (0x003E_48C9, 0x0024_A058), // Pink
];

fn map_fo76_hair_color(local: u32) -> Option<u32> {
    FO76_TO_FO4_HAIR_COLOR
        .iter()
        .find_map(|&(fo76, fo4)| (fo76 == local).then_some(fo4))
}

pub struct InjectHumanNpcHeadPartsFixup;

impl Fixup for InjectHumanNpcHeadPartsFixup {
    fn name(&self) -> &'static str {
        "inject_human_npc_head_parts"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        match ctx.config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            None => true,
        }
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        match config.root_sig {
            Some(sig) => is_creature_root_sig(sig),
            None => true,
        }
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

        if !session
            .target_masters()
            .iter()
            .any(|m| m.eq_ignore_ascii_case(BASE_MASTER))
        {
            return Ok(report);
        }

        let npc_order = npc_subrecord_order(target_schema).ok_or_else(|| {
            FixupError::Other("missing NPC_ schema order for PNAM injection".into())
        })?;
        let base_sym = mapper.interner.intern(BASE_MASTER);
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let npc_fks = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if npc_fks.is_empty() {
            return Ok(report);
        }
        let race_sig =
            SigCode::from_str("RACE").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let custom_race_head_parts =
            collect_custom_humanoid_race_head_parts(session, mapper, race_sig)?;

        let mut changed_records = Vec::new();
        for fk in npc_fks {
            if session
                .record_has_any_subrecord(&fk, &["PNAM"])
                .unwrap_or(true)
            {
                continue;
            }
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    report.warnings.push(
                        mapper
                            .interner
                            .intern(&format!("inject_head_parts_read:{e}")),
                    );
                    continue;
                }
            };

            if inject_missing_humanoid_head_parts(
                &mut record,
                base_sym,
                mapper.interner,
                &npc_order,
                &custom_race_head_parts,
            ) {
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
                "inject_human_npc_head_parts replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn npc_subrecord_order(schema: &AuthoringSchema) -> Option<Vec<&str>> {
    let npc = schema.record_def("NPC_")?;
    Some(npc.subrecords.iter().map(|sub| sub.id.as_str()).collect())
}

fn collect_custom_humanoid_race_head_parts(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
    race_sig: SigCode,
) -> Result<FxHashMap<FormKey, RaceHeadParts>, FixupError> {
    let race_fks = session
        .form_keys_of_sig(race_sig, mapper.interner)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let masters = session.target_masters().to_vec();
    let target_id = session.target_id();
    let scan = session
        .handle_raw_scan(target_id)
        .map_err(|e| FixupError::HandleError(e.to_string()))?;
    let raw_ids = scan.raw_form_ids_of_sig(race_sig);
    let mut result = FxHashMap::default();

    for (race_fk, raw_id) in race_fks.into_iter().zip(raw_ids) {
        let Some(plugin_name) = mapper.interner.resolve(race_fk.plugin) else {
            continue;
        };
        let parts = scan.with_record_subrecords(raw_id, |subrecords| {
            parse_custom_humanoid_race_head_parts(
                subrecords
                    .iter()
                    .map(|sub| (sub.signature.as_str(), sub.data.as_ref())),
                &masters,
                plugin_name,
                mapper.interner,
            )
        });
        if let Some(Some(parts)) = parts {
            result.insert(race_fk, parts);
        }
    }

    Ok(result)
}

fn parse_custom_humanoid_race_head_parts<'a>(
    subrecords: impl IntoIterator<Item = (&'a str, &'a [u8])>,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<RaceHeadParts> {
    let mut armor_race = None;
    // Head parts are keyed by which head-data section they sit in, counted off
    // `NAM0`: the male marker `MNAM` PRECEDES the male `NAM0` (vanilla
    // `MNAM NAM0 HEAD..` / `NAM0 FNAM HEAD..`), and `MNAM`/`FNAM` also mark the
    // unrelated skeletal-model rows earlier in the record, so the markers alone
    // cannot delimit the sections.
    let mut head_section = 0usize;
    let mut parts = RaceHeadParts::default();

    for (signature, data) in subrecords {
        match signature {
            "RNAM" => {
                armor_race = read_raw_form_id(data)
                    .and_then(|raw| resolve_raw_form_id(raw, masters, plugin_name, interner));
            }
            "NAM0" => head_section += 1,
            "FNAM" if head_section >= 1 => head_section = head_section.max(2),
            "HEAD" => {
                let is_female = match head_section {
                    0 => continue,
                    1 => false,
                    _ => true,
                };
                let Some(head_part) = read_raw_form_id(data)
                    .and_then(|raw| resolve_raw_form_id(raw, masters, plugin_name, interner))
                else {
                    continue;
                };
                let target = if is_female {
                    &mut parts.female
                } else {
                    &mut parts.male
                };
                if !target.contains(&head_part) {
                    target.push(head_part);
                }
            }
            _ => {}
        }
    }

    let armor_race = armor_race?;
    if !is_fo4_humanoid_race(armor_race, interner)
        || (parts.male.is_empty() && parts.female.is_empty())
    {
        return None;
    }
    Some(parts)
}

fn read_raw_form_id(data: &[u8]) -> Option<u32> {
    let bytes: [u8; 4] = data.get(..4)?.try_into().ok()?;
    Some(u32::from_le_bytes(bytes))
}

fn resolve_raw_form_id(
    raw: u32,
    masters: &[String],
    plugin_name: &str,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    let index = (raw >> 24) as usize;
    let plugin = if index < masters.len() {
        masters[index].as_str()
    } else if index == masters.len() {
        plugin_name
    } else {
        return None;
    };
    Some(FormKey {
        plugin: interner.intern(plugin),
        local: raw & 0x00FF_FFFF,
    })
}

#[cfg(test)]
fn inject_default_human_head_parts(
    record: &mut Record,
    base_sym: Sym,
    interner: &StringInterner,
    npc_order: &[&str],
) -> bool {
    inject_missing_humanoid_head_parts(record, base_sym, interner, npc_order, &FxHashMap::default())
}

fn inject_missing_humanoid_head_parts(
    record: &mut Record,
    base_sym: Sym,
    interner: &StringInterner,
    npc_order: &[&str],
    custom_race_head_parts: &FxHashMap<FormKey, RaceHeadParts>,
) -> bool {
    if record
        .fields
        .iter()
        .any(|entry| entry.sig.as_str() == "PNAM")
    {
        return false;
    }
    if !record_has_face_customization(record) {
        return false;
    }
    let Some(race) = npc_race(record) else {
        return false;
    };
    let is_female = is_female_npc(record);
    let (head_parts, inject_hair_color) = if is_fo4_race(race, HUMAN_RACE_LOCAL, interner) {
        let defaults = if is_female {
            DEFAULT_FEMALE_HUMAN_HEAD_PARTS
        } else {
            DEFAULT_MALE_HUMAN_HEAD_PARTS
        };
        (
            defaults
                .iter()
                .copied()
                .map(|local| FormKey {
                    plugin: base_sym,
                    local,
                })
                .collect::<Vec<_>>(),
            true,
        )
    } else if is_fo4_race(race, GHOUL_RACE_LOCAL, interner) {
        let defaults = if is_female {
            DEFAULT_FEMALE_GHOUL_HEAD_PARTS
        } else {
            DEFAULT_MALE_GHOUL_HEAD_PARTS
        };
        (
            defaults
                .iter()
                .copied()
                .map(|local| FormKey {
                    plugin: base_sym,
                    local,
                })
                .collect::<Vec<_>>(),
            false,
        )
    } else if is_fo4_race(race, HUMAN_CHILD_RACE_LOCAL, interner) {
        let defaults = if is_female {
            DEFAULT_FEMALE_CHILD_HEAD_PARTS
        } else {
            DEFAULT_MALE_CHILD_HEAD_PARTS
        };
        (
            defaults
                .iter()
                .copied()
                .map(|local| FormKey {
                    plugin: base_sym,
                    local,
                })
                .collect::<Vec<_>>(),
            true,
        )
    } else {
        let Some(parts) = custom_race_head_parts.get(&race) else {
            return false;
        };
        let selected = if is_female {
            &parts.female
        } else {
            &parts.male
        };
        if selected.is_empty() {
            return false;
        }
        (selected.clone(), false)
    };

    let Some(pnam_idx) = schema_order_index(npc_order, "PNAM") else {
        return false;
    };
    let insert_at = record
        .fields
        .iter()
        .position(|entry| {
            schema_order_index(npc_order, entry.sig.as_str()).is_some_and(|idx| idx > pnam_idx)
        })
        .unwrap_or(record.fields.len());

    let entries = head_parts.iter().copied().map(|form_key| FieldEntry {
        sig: SubrecordSig::from_str("PNAM").expect("PNAM sig"),
        value: FieldValue::FormKey(form_key),
    });
    for (offset, entry) in entries.enumerate() {
        record.fields.insert(insert_at + offset, entry);
    }

    if inject_hair_color {
        apply_human_hair_color(record, base_sym, interner, insert_at + head_parts.len());
    }
    true
}

/// Give a head-part-injected human NPC a usable FO4 hair color. FO4 needs an
/// explicit NPC `HCLF` to draw hair (it does not fall back to the race default),
/// so a hair head part with no `HCLF` renders bald — the missing-hair symptom.
/// Remap a carried (FO76) `HCLF` through [`FO76_TO_FO4_HAIR_COLOR`] in place, or
/// inject the race-default color when the NPC has none. A carried own-plugin
/// `HCLF` would otherwise dangle (FO76 `CLFM` records are not emitted to the
/// output), so any non-Fallout4 reference is retargeted.
fn apply_human_hair_color(
    record: &mut Record,
    base_sym: Sym,
    interner: &StringInterner,
    insert_at: usize,
) {
    if let Some(idx) = record
        .fields
        .iter()
        .position(|entry| entry.sig.as_str() == "HCLF")
    {
        if let FieldValue::FormKey(fk) = &record.fields[idx].value {
            let (plugin, local) = (fk.plugin, fk.local);
            let already_fo4 = interner
                .resolve(plugin)
                .is_some_and(|p| p.eq_ignore_ascii_case(BASE_MASTER));
            if !already_fo4 {
                let fo4_local = map_fo76_hair_color(local).unwrap_or(DEFAULT_HUMAN_HAIR_COLOR_FO4);
                record.fields[idx].value = FieldValue::FormKey(FormKey {
                    plugin: base_sym,
                    local: fo4_local,
                });
            }
        }
        return;
    }

    let hclf = FieldEntry {
        sig: SubrecordSig::from_str("HCLF").expect("HCLF sig"),
        value: FieldValue::FormKey(FormKey {
            plugin: base_sym,
            local: DEFAULT_HUMAN_HAIR_COLOR_FO4,
        }),
    };
    let at = insert_at.min(record.fields.len());
    record.fields.insert(at, hclf);
}

fn schema_order_index(order: &[&str], sig: &str) -> Option<usize> {
    order.iter().rposition(|candidate| *candidate == sig)
}

fn record_has_face_customization(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        matches!(
            entry.sig.as_str(),
            "HCLF" | "FTST" | "MSDK" | "MSDV" | "MRSV" | "FMRI" | "FMRS"
        )
    })
}

fn is_female_npc(record: &Record) -> bool {
    record.fields.iter().any(|entry| {
        if entry.sig.as_str() != "ACBS" {
            return false;
        }
        match &entry.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                let flags = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
                flags & NPC_ACBS_FEMALE_FLAG != 0
            }
            _ => false,
        }
    })
}

fn npc_race(record: &Record) -> Option<FormKey> {
    record.fields.iter().find_map(|entry| {
        if entry.sig.as_str() != "RNAM" {
            return None;
        }
        let FieldValue::FormKey(fk) = &entry.value else {
            return None;
        };
        Some(*fk)
    })
}

fn is_fo4_humanoid_race(race: FormKey, interner: &StringInterner) -> bool {
    is_fo4_race(race, HUMAN_RACE_LOCAL, interner)
        || is_fo4_race(race, GHOUL_RACE_LOCAL, interner)
        || is_fo4_race(race, HUMAN_CHILD_RACE_LOCAL, interner)
}

fn is_fo4_race(race: FormKey, local: u32, interner: &StringInterner) -> bool {
    race.local == local
        && interner
            .resolve(race.plugin)
            .is_some_and(|plugin| plugin.eq_ignore_ascii_case(BASE_MASTER))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RecordFlags;

    const NPC_ORDER: &[&str] = &[
        "EDID", "ACBS", "RNAM", "DATA", "DNAM", "PNAM", "HCLF", "FTST", "MSDK", "MSDV", "MRSV",
        "FMRI", "FMRS",
    ];

    fn record(fields: Vec<(&str, FieldValue)>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                plugin: interner.intern("SeventySix.esm"),
                local: 0x000800,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(sig, value)| FieldEntry {
                    sig: SubrecordSig::from_str(sig).unwrap(),
                    value,
                })
                .collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn acbs(flags: u32) -> FieldValue {
        let mut bytes = vec![0u8; 20];
        bytes[0..4].copy_from_slice(&flags.to_le_bytes());
        FieldValue::Bytes(smallvec::SmallVec::from_vec(bytes))
    }

    fn human_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: HUMAN_RACE_LOCAL,
        })
    }

    fn ghoul_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: GHOUL_RACE_LOCAL,
        })
    }

    fn child_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: HUMAN_CHILD_RACE_LOCAL,
        })
    }

    fn other_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: 0x0002_0000,
        })
    }

    fn bytes() -> FieldValue {
        FieldValue::Bytes(smallvec::SmallVec::new())
    }

    fn raw_form_id(raw: u32) -> Vec<u8> {
        raw.to_le_bytes().to_vec()
    }

    fn order(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect()
    }

    fn count_sig(record: &Record, sig: &str) -> usize {
        record
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == sig)
            .count()
    }

    #[test]
    fn injects_default_male_head_parts_before_hclf() {
        let interner = StringInterner::new();
        let base_sym = interner.intern(BASE_MASTER);
        let mut npc = record(
            vec![
                ("EDID", bytes()),
                ("ACBS", acbs(0)),
                ("RNAM", human_race(&interner)),
                ("DATA", bytes()),
                ("DNAM", bytes()),
                ("HCLF", bytes()),
                ("MSDK", bytes()),
            ],
            &interner,
        );

        assert!(inject_default_human_head_parts(
            &mut npc, base_sym, &interner, NPC_ORDER
        ));
        assert_eq!(count_sig(&npc, "PNAM"), DEFAULT_MALE_HUMAN_HEAD_PARTS.len());
        let got = order(&npc);
        let first_pnam = got.iter().position(|sig| *sig == "PNAM").unwrap();
        let hclf = got.iter().position(|sig| *sig == "HCLF").unwrap();
        assert!(first_pnam < hclf);
        for entry in npc
            .fields
            .iter()
            .filter(|entry| entry.sig.as_str() == "PNAM")
        {
            let FieldValue::FormKey(fk) = &entry.value else {
                panic!("PNAM must be FormKey");
            };
            assert_eq!(fk.plugin, base_sym);
        }

        let mut no_hclf = record(
            vec![
                ("ACBS", acbs(0)),
                ("RNAM", human_race(&interner)),
                ("MSDK", bytes()),
            ],
            &interner,
        );
        assert!(inject_default_human_head_parts(
            &mut no_hclf,
            base_sym,
            &interner,
            NPC_ORDER
        ));
        assert_eq!(
            hclf_target(&no_hclf),
            Some((base_sym, DEFAULT_HUMAN_HAIR_COLOR_FO4)),
            "default hair color is injected when absent"
        );
        let got = order(&no_hclf);
        let last_pnam = got.iter().rposition(|sig| *sig == "PNAM").unwrap();
        let hclf = got.iter().position(|sig| *sig == "HCLF").unwrap();
        assert!(
            last_pnam < hclf,
            "HCLF follows the head parts in schema order"
        );
    }

    #[test]
    fn injects_default_head_parts_per_race_and_sex() {
        let interner = StringInterner::new();
        let base_sym = interner.intern(BASE_MASTER);
        for (name, flags, race, expected, hair_color) in [
            (
                "female_human",
                NPC_ACBS_FEMALE_FLAG,
                human_race(&interner),
                DEFAULT_FEMALE_HUMAN_HEAD_PARTS,
                true,
            ),
            (
                "female_ghoul",
                NPC_ACBS_FEMALE_FLAG,
                ghoul_race(&interner),
                DEFAULT_FEMALE_GHOUL_HEAD_PARTS,
                false,
            ),
            (
                "male_child",
                0,
                child_race(&interner),
                DEFAULT_MALE_CHILD_HEAD_PARTS,
                true,
            ),
        ] {
            let mut npc = record(
                vec![("ACBS", acbs(flags)), ("RNAM", race), ("FMRI", bytes())],
                &interner,
            );

            assert!(
                inject_default_human_head_parts(&mut npc, base_sym, &interner, NPC_ORDER),
                "{name}"
            );
            let pnams: Vec<u32> = npc
                .fields
                .iter()
                .filter_map(|entry| match (&*entry.sig.as_str(), &entry.value) {
                    ("PNAM", FieldValue::FormKey(fk)) => {
                        assert_eq!(fk.plugin, base_sym, "{name}");
                        Some(fk.local)
                    }
                    _ => None,
                })
                .collect();
            assert_eq!(pnams, expected, "{name}");
            assert_eq!(
                hclf_target(&npc),
                hair_color.then_some((base_sym, DEFAULT_HUMAN_HAIR_COLOR_FO4)),
                "{name}: ghouls get no human hair color"
            );
        }
    }

    #[test]
    fn leaves_npcs_with_head_parts_or_non_human_races_alone() {
        let interner = StringInterner::new();
        let base_sym = interner.intern(BASE_MASTER);
        let existing = FieldValue::FormKey(FormKey {
            plugin: base_sym,
            local: 0x0000_1234,
        });
        for (name, flags, race, pnam, expected_pnams) in [
            (
                "male_with_pnam",
                0,
                human_race(&interner),
                Some(existing.clone()),
                1,
            ),
            (
                "female_with_pnam",
                NPC_ACBS_FEMALE_FLAG,
                human_race(&interner),
                Some(existing.clone()),
                1,
            ),
            ("non_human", 0, other_race(&interner), None, 0),
        ] {
            let mut fields = vec![("ACBS", acbs(flags)), ("RNAM", race)];
            if let Some(pnam) = pnam {
                fields.push(("PNAM", pnam));
            }
            fields.push(("HCLF", bytes()));
            let mut npc = record(fields, &interner);

            assert!(
                !inject_default_human_head_parts(&mut npc, base_sym, &interner, NPC_ORDER),
                "{name}"
            );
            assert_eq!(count_sig(&npc, "PNAM"), expected_pnams, "{name}");
        }
    }

    /// Real FO4/FO76 races put the skeletal-model `MNAM`/`FNAM` markers first
    /// and the male head marker BEFORE the male `NAM0`, so the male section can
    /// only be identified by counting `NAM0`. Reading it off the markers gave
    /// every head part to the female bucket and left male NPCs headless.
    #[test]
    fn parses_custom_race_head_parts_by_nam0_sections() {
        let interner = StringInterner::new();
        let masters = vec![BASE_MASTER.to_string()];
        let own_sym = interner.intern("SeventySix.esm");
        let own = |local| FormKey {
            plugin: own_sym,
            local,
        };
        let female_tail = [
            ("NAM0", Vec::new()),
            ("FNAM", Vec::new()),
            ("HEAD", raw_form_id(0x0111_CCBC)),
            ("HEAD", raw_form_id(0x0176_9B9D)),
        ];
        for (name, male_section, male) in [
            (
                "ghoul_armor",
                vec![
                    ("RNAM", raw_form_id(GHOUL_RACE_LOCAL)),
                    ("NAM0", Vec::new()),
                    ("MNAM", Vec::new()),
                    ("HEAD", raw_form_id(0x0176_9BA4)),
                    ("HEAD", raw_form_id(0x0176_9B9E)),
                ],
                vec![own(0x0076_9BA4), own(0x0076_9B9E)],
            ),
            (
                "male_marker_before_head_data",
                vec![
                    ("MNAM", Vec::new()),
                    ("FNAM", Vec::new()),
                    ("RNAM", raw_form_id(GHOUL_RACE_LOCAL)),
                    ("MNAM", Vec::new()),
                    ("NAM0", Vec::new()),
                    ("HEAD", raw_form_id(0x011C_206F)),
                    ("HEAD", raw_form_id(0x0176_9BA4)),
                ],
                vec![own(0x001C_206F), own(0x0076_9BA4)],
            ),
        ] {
            let parts = parse_custom_humanoid_race_head_parts(
                male_section
                    .iter()
                    .chain(female_tail.iter())
                    .map(|(signature, data)| (*signature, data.as_slice())),
                &masters,
                "SeventySix.esm",
                &interner,
            )
            .expect("custom humanoid head parts");
            assert_eq!(parts.male, male, "{name}");
            assert_eq!(
                parts.female,
                vec![own(0x0011_CCBC), own(0x0076_9B9D)],
                "{name}"
            );
        }
    }

    #[test]
    fn injects_custom_race_female_head_parts_without_human_hair_default() {
        let interner = StringInterner::new();
        let base_sym = interner.intern(BASE_MASTER);
        let own_sym = interner.intern("SeventySix.esm");
        let lost_race = FormKey {
            plugin: own_sym,
            local: 0x0072_7CCF,
        };
        let female_head_parts = vec![
            FormKey {
                plugin: base_sym,
                local: 0x0011_CCBC,
            },
            FormKey {
                plugin: own_sym,
                local: 0x0077_69B9,
            },
        ];
        let mut custom_races = FxHashMap::default();
        custom_races.insert(
            lost_race,
            RaceHeadParts {
                male: Vec::new(),
                female: female_head_parts.clone(),
            },
        );
        let mut npc = record(
            vec![
                ("ACBS", acbs(NPC_ACBS_FEMALE_FLAG)),
                ("RNAM", FieldValue::FormKey(lost_race)),
                ("MRSV", bytes()),
            ],
            &interner,
        );

        assert!(inject_missing_humanoid_head_parts(
            &mut npc,
            base_sym,
            &interner,
            NPC_ORDER,
            &custom_races,
        ));
        let actual: Vec<FormKey> = npc
            .fields
            .iter()
            .filter_map(|entry| {
                if entry.sig.as_str() != "PNAM" {
                    return None;
                }
                match &entry.value {
                    FieldValue::FormKey(fk) => Some(*fk),
                    _ => None,
                }
            })
            .collect();
        assert_eq!(actual, female_head_parts);
        assert_eq!(count_sig(&npc, "HCLF"), 0);
    }

    fn hclf_target(record: &Record) -> Option<(Sym, u32)> {
        record.fields.iter().find_map(|entry| {
            if entry.sig.as_str() != "HCLF" {
                return None;
            }
            match &entry.value {
                FieldValue::FormKey(fk) => Some((fk.plugin, fk.local)),
                _ => None,
            }
        })
    }

    fn formkey(plugin: Sym, local: u32) -> FieldValue {
        FieldValue::FormKey(FormKey { plugin, local })
    }

    /// A carried hair color keeps its local id but is retargeted to Fallout4 when
    /// the id is shared, remapped when FO76 moved it (3E48C0 is FO4 DLC04
    /// 24A04E), defaulted when it is not a hair CLFM (it would dangle), and left
    /// alone when already a valid FO4 hair color.
    #[test]
    fn carried_hair_color_is_retargeted_remapped_or_defaulted() {
        let interner = StringInterner::new();
        let base_sym = interner.intern(BASE_MASTER);
        let source_sym = interner.intern("SeventySix.esm");
        for (name, carried, expected) in [
            ("shared_id", formkey(source_sym, 0x0019_EE60), 0x0019_EE60),
            (
                "fo76_dlc04_color",
                formkey(source_sym, 0x003E_48C0),
                0x0024_A04E,
            ),
            (
                "unknown_color",
                formkey(source_sym, 0x0011_1111),
                DEFAULT_HUMAN_HAIR_COLOR_FO4,
            ),
            (
                "valid_fo4_color",
                formkey(base_sym, 0x0019_EE61),
                0x0019_EE61,
            ),
        ] {
            let mut npc = record(
                vec![
                    ("ACBS", acbs(0)),
                    ("RNAM", human_race(&interner)),
                    ("HCLF", carried),
                ],
                &interner,
            );
            assert!(
                inject_default_human_head_parts(&mut npc, base_sym, &interner, NPC_ORDER),
                "{name}"
            );
            assert_eq!(hclf_target(&npc), Some((base_sym, expected)), "{name}");
            assert_eq!(count_sig(&npc, "HCLF"), 1, "{name}");
        }
    }
}
