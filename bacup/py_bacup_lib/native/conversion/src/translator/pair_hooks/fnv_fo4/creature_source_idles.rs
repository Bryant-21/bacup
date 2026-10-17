use std::collections::{BTreeMap, BTreeSet};

use crate::record::FieldValue;
use crate::sym::StringInterner;

use super::creature_catalog::{
    CreatureCorpusPlan, CreatureDisposition, LegacyCreatureGame, LegacyRecordSource, RigFamilyKey,
    StableFormKey,
};
use super::creature_dependencies::FnvFo3CreatureDependencyLedger;
use super::creature_motion::{CreatureMotionFamilyEvidence, IdleClaimEvidence};

pub(crate) fn source_idle_event(game: LegacyCreatureGame, source: &StableFormKey) -> String {
    let game = match game {
        LegacyCreatureGame::Fnv => "fnv",
        LegacyCreatureGame::Fo3 => "fo3",
    };
    let identity = format!(
        "{game}:{:06x}:{}",
        source.local,
        source.plugin.to_ascii_lowercase()
    );
    format!(
        "B21_SourceIdle_{}",
        &blake3::hash(identity.as_bytes()).to_hex()[..24]
    )
}

pub(crate) fn source_family_behavior_path(rig: &RigFamilyKey) -> String {
    let identity = super::source_rig_bridge::canonical_fnv_fo3_family_id(rig);
    let hash = blake3::hash(identity.as_bytes()).to_hex();
    format!(
        "Actors\\FnvFo3Creature_{}\\Behaviors\\FnvFo3RootBehavior.hkx",
        &hash[..16]
    )
}

pub(super) fn reachable_shared_rig_idles(
    catalog: &CreatureCorpusPlan,
    dependencies: &FnvFo3CreatureDependencyLedger,
) -> BTreeMap<RigFamilyKey, BTreeSet<StableFormKey>> {
    let mut reachable = BTreeMap::<_, BTreeSet<_>>::new();
    for family in &catalog.rig_families {
        if !model_key(&family.key.skeleton_path).starts_with("characters/") {
            continue;
        }
        let owners = catalog
            .records
            .iter()
            .filter(|record| match &record.disposition {
                CreatureDisposition::VisualOwner { rig, .. } => rig == &family.key,
                CreatureDisposition::Proxy {
                    terminal_visual_owners,
                    ..
                } => terminal_visual_owners
                    .iter()
                    .any(|owner| family.members.contains(owner)),
                _ => false,
            })
            .map(|record| {
                (
                    record.source.local,
                    record.source.plugin.to_ascii_lowercase(),
                )
            })
            .collect::<BTreeSet<_>>();
        let keys = reachable.entry(family.key.clone()).or_default();
        for candidate in dependencies.candidates.iter().filter(|candidate| {
            candidate.provenance.game == family.key.game
                && owners.contains(&(
                    candidate.source.local,
                    candidate.source.plugin.to_ascii_lowercase(),
                ))
        }) {
            keys.extend(candidate.closure_form_keys.iter().map(|key| StableFormKey {
                local: key.local,
                plugin: key.plugin.to_ascii_lowercase(),
            }));
        }
    }
    reachable
}

pub(super) fn restrict_shared_rig_idle_claims(
    families: &mut [CreatureMotionFamilyEvidence],
    reachable: &BTreeMap<RigFamilyKey, BTreeSet<StableFormKey>>,
) {
    for family in families
        .iter_mut()
        .filter(|family| model_key(&family.rig.skeleton_path).starts_with("characters/"))
    {
        for kf in &mut family.kf_evidence {
            kf.idle_claims.retain(|claim| {
                reachable.get(&family.rig).is_some_and(|keys| {
                    keys.contains(&StableFormKey {
                        local: claim.source_idle.local,
                        plugin: claim.source_idle.plugin.to_ascii_lowercase(),
                    })
                })
            });
        }
    }
}

pub(crate) fn selected_source_idle_routes(
    family: &super::creature_recipe::CreatureFamilyRecipeJob,
) -> Vec<(StableFormKey, String)> {
    family
        .required_motion
        .iter()
        .filter(|role| role.role == super::creature_motion::MotionRole::ScriptedIdle)
        .flat_map(|role| &role.candidates)
        .flat_map(|candidate| {
            candidate
                .idle_claims
                .iter()
                .map(|claim| (claim.source_idle.clone(), candidate.source_kf.clone()))
        })
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect()
}

fn model_key(path: &str) -> String {
    let path = path.replace('\\', "/").to_ascii_lowercase();
    path.strip_prefix("meshes/").unwrap_or(&path).to_string()
}

pub(super) fn attach_source_idle_claims(
    sources: &[LegacyRecordSource<'_>],
    families: &mut [CreatureMotionFamilyEvidence],
    interner: &StringInterner,
) -> Result<(), String> {
    let mut winners = BTreeMap::new();
    for source in sources
        .iter()
        .filter(|source| source.record.sig.as_str() == "IDLE")
    {
        let key = StableFormKey {
            local: source.record.form_key.local,
            plugin: interner
                .resolve(source.record.form_key.plugin)
                .ok_or_else(|| "source IDLE plugin is unavailable".to_string())?
                .to_ascii_lowercase(),
        };
        let previous = winners
            .entry((source.provenance.game, key))
            .or_insert(source);
        if source.provenance.precedence >= previous.provenance.precedence {
            *previous = source;
        }
    }
    let mut claims = BTreeMap::<_, Vec<_>>::new();
    for ((game, key), source) in winners {
        let Some(path) = source
            .record
            .fields
            .iter()
            .find(|field| field.sig.as_str() == "MODL")
            .and_then(|field| match field.value {
                FieldValue::String(value) => interner.resolve(value),
                _ => None,
            })
        else {
            continue;
        };
        if path.to_ascii_lowercase().ends_with(".kf") {
            let data = source.record.fields.iter().find(|field| field.sig.as_str() == "DATA");
            let count = |name: &str, offset: usize| -> Result<u8, String> {
                let value = match data.map(|field| &field.value) {
                    Some(FieldValue::Struct(fields)) => fields.iter().find(|(key, _)| interner.resolve(*key) == Some(name)).map(|(_, value)| value),
                    Some(FieldValue::Bytes(bytes)) => return bytes.get(offset).copied().ok_or_else(|| format!("source IDLE {key} has truncated DATA")),
                    None => return Ok(0),
                    _ => return Err(format!("source IDLE {key} has undecodable DATA")),
                };
                match value {
                    Some(FieldValue::Uint(value)) => u8::try_from(*value).map_err(|_| format!("source IDLE {key} {name} exceeds one byte")),
                    Some(FieldValue::Int(value)) => u8::try_from(*value).map_err(|_| format!("source IDLE {key} {name} exceeds one byte")),
                    None => Ok(0),
                    _ => Err(format!("source IDLE {key} has non-numeric {name}")),
                }
            };
            let claim = IdleClaimEvidence { source_idle:key.clone(), source_group: count("AnimationGroupSection", 0)? & 0x3f, looping_min:count("LoopingMin", 1)?, looping_max:count("LoopingMax", 2)?, ancestry:Vec::new() };
            claims.entry((game, model_key(path))).or_default().push(claim);
        }
    }
    for family in families {
        for kf in &mut family.kf_evidence {
            if let Some(idles) = claims.get(&(family.rig.game, model_key(&kf.source_kf))) {
                for claim in idles {
                    if let Some(existing) = kf
                        .idle_claims
                        .iter_mut()
                        .find(|existing| existing.source_idle == claim.source_idle)
                    {
                        existing.source_group = claim.source_group;
                        existing.looping_min = claim.looping_min;
                        existing.looping_max = claim.looping_max;
                    } else {
                        kf.idle_claims.push(claim.clone());
                    }
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::creature_catalog::CreatureProvenance;
    use super::super::creature_motion::{CreatureKfEvidence, KfParseEvidence};
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, Record};

    #[test]
    fn shared_kf_keeps_each_source_idle_loop_contract() {
        let interner = StringInterner::new();
        let idle = |local, min, max| {
            let mut record = Record::new(
                SigCode::from_str("IDLE").unwrap(),
                FormKey { local, plugin: interner.intern("FalloutNV.esm") },
            );
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"MODL"),
                value: FieldValue::String(interner.intern("creatures\\dog\\idleanims\\specialidle_sit.kf")),
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(vec![20, min, max, 0, 0, 0].into()),
            });
            record
        };
        let finite = idle(0x0298E0, 4, 8);
        let infinite = idle(0x16AEA8, 255, 255);
        let source = |record| LegacyRecordSource {
            record,
            provenance: CreatureProvenance {
                game: LegacyCreatureGame::Fnv,
                precedence: 0,
                source_plugin: "FalloutNV.esm".to_string(),
            },
        };
        let mut families = vec![CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/dog/skeleton.nif".to_string(),
            },
            referenced_kfs: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: Vec::new(),
            kf_evidence: vec![CreatureKfEvidence {
                source_game: LegacyCreatureGame::Fnv,
                source_kf: "creatures/dog/idleanims/specialidle_sit.kf".to_string(),
                sequence: KfParseEvidence::MissingAsset,
                idle_claims: Vec::new(),
            }],
        }];
        attach_source_idle_claims(&[source(&finite), source(&infinite)], &mut families, &interner)
            .unwrap();
        let claims = &families[0].kf_evidence[0].idle_claims;
        assert_eq!(claims.len(), 2);
        assert_eq!((claims[0].source_idle.local, claims[0].looping_min, claims[0].looping_max), (0x0298E0, 4, 8));
        assert_eq!((claims[1].source_idle.local, claims[1].looping_min, claims[1].looping_max), (0x16AEA8, 255, 255));
        assert!(claims.iter().all(|claim| claim.source_group == 20));
        assert_ne!(
            source_idle_event(LegacyCreatureGame::Fnv, &claims[0].source_idle),
            source_idle_event(LegacyCreatureGame::Fnv, &claims[1].source_idle),
        );
    }

    #[test]
    fn direct_idle_claims_follow_game_exact_model_and_winning_override() {
        let interner = StringInterner::new();
        let mut original = Record::new(
            SigCode::from_str("IDLE").unwrap(),
            FormKey {
                local: 0x1424D2,
                plugin: interner.intern("FalloutNV.esm"),
            },
        );
        original.fields.push(FieldEntry {
            sig: SubrecordSig(*b"MODL"),
            value: FieldValue::String(
                interner.intern("creatures\\NVRaven\\IdleAnims\\SpecialIdle_FlyAway.kf"),
            ),
        });
        let mut winning = original.clone();
        winning.fields[0].value = FieldValue::String(
            interner.intern("Meshes/CREATURES/NVRAVEN/IDLEANIMS/SpecialIdle_TurnLeft.kf"),
        );
        let source = |record, game, precedence| LegacyRecordSource {
            record,
            provenance: CreatureProvenance {
                game,
                precedence,
                source_plugin: "FalloutNV.esm".to_string(),
            },
        };
        let mut families = vec![CreatureMotionFamilyEvidence {
            rig: RigFamilyKey {
                game: LegacyCreatureGame::Fnv,
                skeleton_path: "creatures/nvraven/skeleton.nif".to_string(),
            },
            referenced_kfs: Vec::new(),
            creature_traits: Vec::new(),
            weapon_attacks: Vec::new(),
            kf_evidence: ["specialidle_flyaway", "specialidle_turnleft"]
                .map(|name| CreatureKfEvidence {
                    source_game: LegacyCreatureGame::Fnv,
                    source_kf: format!("creatures/nvraven/idleanims/{name}.kf"),
                    sequence: KfParseEvidence::MissingAsset,
                    idle_claims: Vec::new(),
                })
                .to_vec(),
        }];
        attach_source_idle_claims(
            &[
                source(&original, LegacyCreatureGame::Fnv, 0),
                source(&winning, LegacyCreatureGame::Fnv, 1),
                source(&original, LegacyCreatureGame::Fo3, 2),
            ],
            &mut families,
            &interner,
        )
        .unwrap();
        assert!(families[0].kf_evidence[0].idle_claims.is_empty());
        assert_eq!(families[0].kf_evidence[1].idle_claims.len(), 1);
        let key = &families[0].kf_evidence[1].idle_claims[0].source_idle;
        assert_eq!(key.local, 0x1424D2);
        assert_eq!(key.plugin, "falloutnv.esm");
        assert_ne!(
            source_idle_event(LegacyCreatureGame::Fnv, key),
            source_idle_event(LegacyCreatureGame::Fo3, key)
        );
    }
}
