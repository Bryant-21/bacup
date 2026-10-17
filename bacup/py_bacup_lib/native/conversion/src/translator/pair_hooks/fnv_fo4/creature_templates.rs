use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::record::{FieldValue, Record};
use crate::source_rig::SourceCreatureIdentity;
use crate::sym::StringInterner;

use super::creature_catalog::{LegacyCreatureGame, StableFormKey};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LegacyTemplateProfile {
    pub root: StableFormKey,
    pub owners: [StableFormKey; 10],
    pub actor_flags: u32,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) enum LegacyTemplatePlan {
    Creature(LegacyTemplateProfile),
    Leveled {
        source: StableFormKey,
        entries: Vec<LegacyTemplatePlan>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct LegacyTemplateBranchReceipt {
    pub plan: LegacyTemplatePlan,
    pub entry_indices: Vec<usize>,
    pub generated_identity: SourceCreatureIdentity,
    pub wrapper: bool,
}

impl LegacyTemplateBranchReceipt {
    pub(crate) fn new(
        plan: &LegacyTemplatePlan,
        entry_indices: Vec<usize>,
        game: LegacyCreatureGame,
        wrapper: bool,
    ) -> Result<Self, String> {
        let profile = plan.profile_at(&entry_indices)?;
        let namespace = match game {
            LegacyCreatureGame::Fnv => "fnv",
            LegacyCreatureGame::Fo3 => "fo3",
        };
        let bytes = serde_json::to_vec(&(namespace, profile, &entry_indices))
            .map_err(|error| error.to_string())?;
        let generated_identity = SourceCreatureIdentity {
            namespace: namespace.to_string(),
            plugin: format!("B21_LegacyTemplate_{}.esm", blake3::hash(&bytes).to_hex()),
            local_form_id: profile.root.local,
        };
        Ok(Self {
            plan: plan.clone(),
            entry_indices,
            generated_identity,
            wrapper,
        })
    }

    pub(crate) fn profile(&self) -> Result<&LegacyTemplateProfile, String> {
        self.plan.profile_at(&self.entry_indices)
    }

    pub(crate) fn validate(&self, game: LegacyCreatureGame) -> Result<(), String> {
        let expected = Self::new(&self.plan, self.entry_indices.clone(), game, self.wrapper)?;
        if expected.generated_identity != self.generated_identity {
            return Err(
                "legacy template branch identity does not match its source profile".to_string(),
            );
        }
        Ok(())
    }
}

impl LegacyTemplatePlan {
    pub(crate) fn profile_at(&self, path: &[usize]) -> Result<&LegacyTemplateProfile, String> {
        match (self, path.split_first()) {
            (Self::Creature(profile), None) => Ok(profile),
            (Self::Leveled { entries, .. }, Some((index, remainder))) => entries
                .get(*index)
                .ok_or_else(|| {
                    "legacy template branch index is outside its source list".to_string()
                })?
                .profile_at(remainder),
            _ => Err("legacy template branch path does not terminate at a creature".to_string()),
        }
    }

    pub(crate) fn branch_paths(&self) -> Vec<Vec<usize>> {
        match self {
            Self::Creature(_) => vec![Vec::new()],
            Self::Leveled { entries, .. } => entries
                .iter()
                .enumerate()
                .flat_map(|(index, entry)| {
                    entry.branch_paths().into_iter().map(move |mut path| {
                        path.insert(0, index);
                        path
                    })
                })
                .collect(),
        }
    }
}

pub(crate) fn resolve_template_plan(
    root: &StableFormKey,
    game: LegacyCreatureGame,
    records: &BTreeMap<StableFormKey, &Record>,
    interner: &StringInterner,
) -> Result<LegacyTemplatePlan, String> {
    let canonical = records
        .iter()
        .map(|(key, record)| ((key.plugin.to_ascii_lowercase(), key.local), (key, *record)))
        .collect::<BTreeMap<_, _>>();
    resolve(root, game, &canonical, interner, &mut BTreeSet::new())
}

fn resolve(
    source: &StableFormKey,
    game: LegacyCreatureGame,
    records: &BTreeMap<(String, u32), (&StableFormKey, &Record)>,
    interner: &StringInterner,
    visiting: &mut BTreeSet<(String, u32)>,
) -> Result<LegacyTemplatePlan, String> {
    let canonical = (source.plugin.to_ascii_lowercase(), source.local);
    if !visiting.insert(canonical.clone()) {
        return Err(format!("creature template cycle at {source}"));
    }
    let (source, record) = records
        .get(&canonical)
        .copied()
        .ok_or_else(|| format!("missing creature template record {source}"))?;
    let result = match record.sig.as_str() {
        "CREA" => {
            let flags = acbs_integer(record, "flags", 0, 4, interner)? as u32;
            let template_flags = acbs_integer(record, "template_flags", 22, 2, interner)? as u16;
            let local = LegacyTemplateProfile {
                root: source.clone(),
                owners: std::array::from_fn(|_| source.clone()),
                actor_flags: flags,
            };
            let template = record
                .fields
                .iter()
                .find(|field| field.sig.as_str() == "TPLT")
                .map(|field| single_key(&field.value, interner))
                .transpose()?
                .flatten();
            if let Some(template) = template.filter(|_| template_flags != 0) {
                let mut inherited = resolve(&template, game, records, interner, visiting)?;
                overlay(&mut inherited, &local, template_flags, game);
                inherited
            } else {
                LegacyTemplatePlan::Creature(local)
            }
        }
        "LVLC" | "LVLN" => {
            let entries = record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "LVLO")
                .map(|field| {
                    let target = single_key(&field.value, interner)?
                        .ok_or_else(|| format!("{source} LVLO has no decoded source FormKey"))?;
                    resolve(&target, game, records, interner, visiting)
                })
                .collect::<Result<Vec<_>, String>>()?;
            if entries.is_empty() {
                return Err(format!("creature template list {source} is empty"));
            }
            LegacyTemplatePlan::Leveled {
                source: source.clone(),
                entries,
            }
        }
        signature => {
            return Err(format!(
                "creature template {source} has unsupported type {signature}"
            ));
        }
    };
    visiting.remove(&canonical);
    Ok(result)
}

fn overlay(
    plan: &mut LegacyTemplatePlan,
    local: &LegacyTemplateProfile,
    inherited: u16,
    game: LegacyCreatureGame,
) {
    match plan {
        LegacyTemplatePlan::Creature(selected) => {
            for category in 0..10 {
                if inherited & (1 << category) == 0 {
                    selected.owners[category] = local.owners[category].clone();
                }
            }
            let mut flag_mask = 0;
            if inherited & (1 << 1) != 0 {
                flag_mask |= 0x80;
            }
            if inherited & (1 << 7) != 0 {
                flag_mask |= 0x1070_220a;
            }
            if inherited & (1 << 6) != 0 {
                flag_mask |= match game {
                    LegacyCreatureGame::Fnv => 0x4b8f_8071,
                    LegacyCreatureGame::Fo3 => 0x038f_8071,
                };
            }
            selected.actor_flags =
                (local.actor_flags & !flag_mask) | (selected.actor_flags & flag_mask);
            selected.root = local.root.clone();
        }
        LegacyTemplatePlan::Leveled { entries, .. } => {
            for entry in entries {
                overlay(entry, local, inherited, game);
            }
        }
    }
}

fn acbs_integer(
    record: &Record,
    name: &str,
    offset: usize,
    width: usize,
    interner: &StringInterner,
) -> Result<u64, String> {
    let value = record
        .fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .ok_or_else(|| "CREA.ACBS is absent".to_string())?;
    match &value.value {
        FieldValue::Bytes(bytes) if bytes.len() == 24 => {
            let mut scalar = [0u8; 8];
            scalar[..width].copy_from_slice(&bytes[offset..offset + width]);
            Ok(u64::from_le_bytes(scalar))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find(|(key, _)| interner.resolve(*key) == Some(name))
            .and_then(|(_, value)| match value {
                FieldValue::Uint(value) => Some(*value),
                FieldValue::Int(value) => u64::try_from(*value).ok(),
                _ => None,
            })
            .ok_or_else(|| format!("CREA.ACBS.{name} is undecodable")),
        _ => Err("CREA.ACBS has an unsupported layout".to_string()),
    }
}

fn single_key(
    value: &FieldValue,
    interner: &StringInterner,
) -> Result<Option<StableFormKey>, String> {
    match value {
        FieldValue::FormKey(key) => Ok(Some(StableFormKey {
            local: key.local,
            plugin: interner
                .resolve(key.plugin)
                .ok_or("unresolved template plugin")?
                .to_string(),
        })),
        FieldValue::Struct(fields) => {
            let mut result = None;
            for (_, value) in fields {
                if !matches!(value, FieldValue::FormKey(_) | FieldValue::Struct(_)) {
                    continue;
                }
                if let Some(key) = single_key(value, interner)? {
                    if result.replace(key).is_some() {
                        return Err("multiple template entry FormKeys".to_string());
                    }
                }
            }
            Ok(result)
        }
        FieldValue::Uint(0) | FieldValue::Int(0) => Ok(None),
        FieldValue::Bytes(bytes) if bytes.iter().all(|byte| *byte == 0) => Ok(None),
        FieldValue::Bytes(_) => {
            Err("raw template FormID requires source master decoding".to_string())
        }
        _ => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn key(local: u32) -> StableFormKey {
        StableFormKey {
            local,
            plugin: "FalloutNV.esm".into(),
        }
    }
    fn record(
        local: u32,
        signature: &str,
        flags: u32,
        inherited: u16,
        parent: Option<u32>,
        i: &StringInterner,
    ) -> Record {
        let mut r = Record::new(
            SigCode::from_str(signature).unwrap(),
            FormKey {
                local,
                plugin: i.intern("FalloutNV.esm"),
            },
        );
        let mut acbs = vec![0; 24];
        acbs[..4].copy_from_slice(&flags.to_le_bytes());
        acbs[22..24].copy_from_slice(&inherited.to_le_bytes());
        r.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ACBS"),
            value: FieldValue::Bytes(acbs.into()),
        });
        if let Some(local) = parent {
            r.fields.push(FieldEntry {
                sig: SubrecordSig(*b"TPLT"),
                value: FieldValue::FormKey(FormKey {
                    local,
                    plugin: i.intern("FalloutNV.esm"),
                }),
            });
        }
        r
    }
    fn entry(local: u32, i: &StringInterner) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*b"LVLO"),
            value: FieldValue::Struct(vec![
                (
                    i.intern("level"),
                    FieldValue::Bytes([1, 0].as_slice().into()),
                ),
                (
                    i.intern("unknown_u8_1"),
                    FieldValue::Bytes([8].as_slice().into()),
                ),
                (
                    i.intern("creature"),
                    FieldValue::FormKey(FormKey {
                        local,
                        plugin: i.intern("FalloutNV.esm"),
                    }),
                ),
                (
                    i.intern("count"),
                    FieldValue::Bytes([1, 0].as_slice().into()),
                ),
                (
                    i.intern("unknown_u8_5"),
                    FieldValue::Bytes([4].as_slice().into()),
                ),
            ]),
        }
    }

    #[test]
    fn shared_legacy_template_choice_keeps_branch_and_local_category_identity() {
        let i = StringInterner::new();
        let root = record(1, "CREA", 0x2000_0000, 0x42, Some(2), &i);
        let middle = record(2, "CREA", 0, 0x40, Some(3), &i);
        let mut list = record(3, "LVLC", 0, 0, None, &i);
        list.fields
            .extend([entry(4, &i), entry(5, &i), entry(4, &i)]);
        let a = record(4, "CREA", 0x4800_0000, 0, None, &i);
        let b = record(5, "CREA", 0, 0, None, &i);
        let records = BTreeMap::from([
            (key(1), &root),
            (key(2), &middle),
            (key(3), &list),
            (key(4), &a),
            (key(5), &b),
        ]);
        let LegacyTemplatePlan::Leveled { source, entries } =
            resolve_template_plan(&key(1), LegacyCreatureGame::Fnv, &records, &i).unwrap()
        else {
            panic!()
        };
        assert_eq!(source, key(3));
        assert_eq!(entries.len(), 3);
        assert_eq!(entries[0], entries[2]);
        for (entry, owner) in entries.iter().zip([4, 5, 4]) {
            let LegacyTemplatePlan::Creature(profile) = entry else {
                panic!()
            };
            assert_eq!(profile.root, key(1));
            assert_eq!(profile.owners[0], key(1));
            assert_eq!(profile.owners[1], key(2));
            assert_eq!(profile.owners[6], key(owner));
            assert_eq!(profile.owners[7], key(1));
        }
        let LegacyTemplatePlan::Creature(fnv) = &entries[0] else {
            panic!()
        };
        assert_eq!(fnv.actor_flags, 0x6800_0000);
        let LegacyTemplatePlan::Leveled { entries, .. } =
            resolve_template_plan(&key(1), LegacyCreatureGame::Fo3, &records, &i).unwrap()
        else {
            panic!()
        };
        let LegacyTemplatePlan::Creature(fo3) = &entries[0] else {
            panic!()
        };
        assert_eq!(fo3.actor_flags, 0x2000_0000);
    }

    #[test]
    fn legacy_template_cycles_fail_instead_of_selecting_a_partial_branch() {
        let i = StringInterner::new();
        let root = record(1, "CREA", 0, 0x40, Some(2), &i);
        let mut list = record(2, "LVLC", 0, 0, None, &i);
        list.fields.push(entry(1, &i));
        let records = BTreeMap::from([(key(1), &root), (key(2), &list)]);
        assert!(
            resolve_template_plan(&key(1), LegacyCreatureGame::Fnv, &records, &i)
                .unwrap_err()
                .contains("cycle")
        );
    }
}
