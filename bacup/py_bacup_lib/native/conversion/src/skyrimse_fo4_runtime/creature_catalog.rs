use std::collections::{BTreeMap, HashMap, HashSet};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

const ACTOR_TYPE_NPC_LOCAL: u32 = 0x013794;
const NPC_ACBS_TEMPLATE_FLAGS_OFFSET: usize = 18;
const TEMPLATE_FLAG_USE_TRAITS: u16 = 0x0001;
const LVLN_REFERENCE_OFFSET: usize = 4;
const LVLN_ROW_SIZE: usize = 12;
const RACE_ATTACK_DATA_LEN: usize = 44;
const RACE_ATTACK_SPELL_OFFSET: usize = 8;
const RACE_ATTACK_TYPE_OFFSET: usize = 28;

const CURATED_NON_CREATURE_RACES: [&str; 3] =
    ["defaultrace", "dlc2miraakrace", "dunmiddenemptyrace"];

// Development races that ship in Skyrim.esm but are not shippable creatures.
// testDraugrRace is the only draugr pointing at DLC01\SkeletonWarrior.nif — the
// other eleven use the base draugr skeleton — so converting it would cost the
// family a second rig for content no NPC uses.
const CURATED_TEST_RACES: [&str; 1] = ["testdraugrrace"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CreatureReadiness {
    RecordReady,
    Rejected,
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreatureCatalogIssue {
    CuratedNonCreatureRace,
    MissingProject,
    MissingSkeleton,
    MissingRaceSkin,
    MissingBodyPartData,
    MissingDependency {
        reference: FormKey,
        expected_signature: &'static str,
    },
    MissingArmorAddonLinks {
        skin: FormKey,
    },
    MissingRaceConditionedArmorAddon {
        skin: FormKey,
    },
    MissingBodyModel {
        armor_addon: FormKey,
    },
    MissingDirectRace,
    MissingTemplateTarget,
    EmptyLeveledTemplate {
        leveled_list: FormKey,
    },
    UnresolvedTemplateTarget {
        source: FormKey,
        target: FormKey,
    },
    UnsupportedTemplateTarget {
        target: FormKey,
        signature: String,
    },
    TemplateCycle {
        nodes: Vec<FormKey>,
    },
    AmbiguousEmbeddedFormId {
        owner: FormKey,
        field: &'static str,
        raw: u32,
        candidates: Vec<FormKey>,
    },
    UnresolvedEmbeddedFormId {
        owner: FormKey,
        field: &'static str,
        raw: u32,
    },
    MalformedRaceAttackData {
        ordinal: usize,
        detail: String,
    },
    MixedCreatureAndNonCreatureRaces {
        races: Vec<FormKey>,
    },
    MultipleCreatureFamilies {
        family_ids: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureBodyModelPlan {
    pub armor_addon: FormKey,
    pub body_nif: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureRaceAttackDataPlan {
    pub ordinal: usize,
    pub event: String,
    pub damage_multiplier_bits: u32,
    pub attack_chance_bits: u32,
    pub attack_spell: Option<FormKey>,
    pub attack_spell_signature: Option<String>,
    pub attack_flags: u32,
    pub attack_angle_bits: u32,
    pub strike_angle_bits: u32,
    pub stagger_bits: u32,
    pub attack_type: Option<FormKey>,
    pub attack_type_signature: Option<String>,
    pub knockdown_bits: u32,
    pub recovery_time_bits: u32,
    pub stamina_multiplier_bits: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureRacePlan {
    pub source_race: FormKey,
    pub source_plugin: String,
    pub editor_id: Option<String>,
    pub skin: Option<FormKey>,
    pub armor_addons: Vec<FormKey>,
    pub body_models: Vec<String>,
    pub body_model_parts: Vec<CreatureBodyModelPlan>,
    pub body_part_data: Option<FormKey>,
    pub project_paths: Vec<String>,
    pub skeleton_paths: Vec<String>,
    pub attack_events: Vec<String>,
    pub attack_contract: Vec<String>,
    pub attack_data: Vec<CreatureRaceAttackDataPlan>,
    pub attack_spells: Vec<FormKey>,
    pub issues: Vec<CreatureCatalogIssue>,
}

impl CreatureRacePlan {
    pub fn readiness(&self) -> CreatureReadiness {
        if self.issues.is_empty() {
            CreatureReadiness::RecordReady
        } else {
            CreatureReadiness::Rejected
        }
    }
}

impl CreatureCorpusPlan {
    pub fn race(&self, source_race: FormKey) -> Option<&CreatureRacePlan> {
        self.races
            .iter()
            .find(|race| race.source_race == source_race)
    }

    pub fn npcs_for_race(&self, source_race: FormKey) -> impl Iterator<Item = &CreatureNpcPlan> {
        self.npcs
            .iter()
            .filter(move |npc| npc.effective_races.contains(&source_race))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct CreatureFamilyKey {
    pub project_paths: Vec<String>,
    pub skeleton_paths: Vec<String>,
    pub body_models: Vec<String>,
    pub attack_contract: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureFamilyPlan {
    pub family_id: String,
    pub key: CreatureFamilyKey,
    pub races: Vec<FormKey>,
    pub issues: Vec<CreatureCatalogIssue>,
}

impl CreatureFamilyPlan {
    pub fn readiness(&self) -> CreatureReadiness {
        if self.issues.is_empty() {
            CreatureReadiness::RecordReady
        } else {
            CreatureReadiness::Rejected
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureNpcPlan {
    pub source_npc: FormKey,
    pub effective_races: Vec<FormKey>,
    pub template_records: Vec<FormKey>,
    pub family_ids: Vec<String>,
    pub issues: Vec<CreatureCatalogIssue>,
}

impl CreatureNpcPlan {
    pub fn readiness(&self) -> CreatureReadiness {
        if self.issues.is_empty() {
            CreatureReadiness::RecordReady
        } else {
            CreatureReadiness::Rejected
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CreaturePlanSubject {
    Race(FormKey),
    Npc(FormKey),
    Family(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CreatureReadinessLedgerEntry {
    pub subject: CreaturePlanSubject,
    pub readiness: CreatureReadiness,
    pub issues: Vec<CreatureCatalogIssue>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreatureCorpusSummary {
    pub total_race_records: usize,
    pub actor_type_npc_races: usize,
    pub curated_exclusions: usize,
    pub candidate_races: usize,
    pub record_ready_races: usize,
    pub rejected_races: usize,
    pub family_count: usize,
    pub candidate_npcs: usize,
    pub record_ready_npcs: usize,
    pub rejected_npcs: usize,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreatureCorpusPlan {
    pub races: Vec<CreatureRacePlan>,
    pub npcs: Vec<CreatureNpcPlan>,
    pub families: Vec<CreatureFamilyPlan>,
    pub excluded_races: Vec<FormKey>,
    pub readiness_ledger: Vec<CreatureReadinessLedgerEntry>,
    pub summary: CreatureCorpusSummary,
}

pub fn build_creature_corpus_plan(
    records: &[Record],
    interner: &StringInterner,
) -> CreatureCorpusPlan {
    let index = RecordIndex::new(records, interner);
    let mut plan = CreatureCorpusPlan::default();
    let mut candidate_race_keys = HashSet::new();

    for race in index.records_of_signature("RACE") {
        plan.summary.total_race_records += 1;
        if index.is_actor_type_npc(race, interner) {
            plan.summary.actor_type_npc_races += 1;
            continue;
        }
        if is_curated_exclusion(race, interner) {
            plan.summary.curated_exclusions += 1;
            plan.excluded_races.push(race.form_key);
            plan.readiness_ledger.push(CreatureReadinessLedgerEntry {
                subject: CreaturePlanSubject::Race(race.form_key),
                readiness: CreatureReadiness::Excluded,
                issues: vec![CreatureCatalogIssue::CuratedNonCreatureRace],
            });
            continue;
        }
        candidate_race_keys.insert(race.form_key);
        plan.races.push(build_race_plan(race, &index, interner));
    }

    plan.races
        .sort_by_key(|race| form_key_sort_key(race.source_race, interner));
    plan.excluded_races
        .sort_by_key(|form_key| form_key_sort_key(*form_key, interner));

    let mut grouped = BTreeMap::<CreatureFamilyKey, Vec<usize>>::new();
    for (index, race) in plan.races.iter().enumerate() {
        grouped.entry(family_key(race)).or_default().push(index);
    }

    let mut family_by_race = HashMap::<FormKey, String>::new();
    for (offset, (key, race_indices)) in grouped.into_iter().enumerate() {
        let family_id = format!("skyrim-creature-family-{:04}", offset + 1);
        let mut family_issues = Vec::new();
        let mut races = Vec::with_capacity(race_indices.len());
        for race_index in race_indices {
            let race = &plan.races[race_index];
            races.push(race.source_race);
            family_by_race.insert(race.source_race, family_id.clone());
            extend_unique(&mut family_issues, race.issues.iter().cloned());
        }
        races.sort_by_key(|race| form_key_sort_key(*race, interner));
        plan.families.push(CreatureFamilyPlan {
            family_id,
            key,
            races,
            issues: family_issues,
        });
    }

    let mut resolver = TemplateResolver::new(&index, interner);
    for npc in index.records_of_signature("NPC_") {
        let resolution = resolver.resolve(npc.form_key);
        let effective_races = sorted_form_keys(resolution.races.iter().copied(), interner);
        let creature_races = effective_races
            .iter()
            .copied()
            .filter(|race| candidate_race_keys.contains(race))
            .collect::<Vec<_>>();

        if creature_races.is_empty() {
            if !resolution.issues.is_empty() {
                plan.readiness_ledger.push(CreatureReadinessLedgerEntry {
                    subject: CreaturePlanSubject::Npc(npc.form_key),
                    readiness: CreatureReadiness::Rejected,
                    issues: resolution.issues,
                });
            }
            continue;
        }

        let mut issues = resolution.issues;
        let non_creature_races = effective_races
            .iter()
            .copied()
            .filter(|race| !candidate_race_keys.contains(race))
            .collect::<Vec<_>>();
        if !non_creature_races.is_empty() {
            issues.push(CreatureCatalogIssue::MixedCreatureAndNonCreatureRaces {
                races: effective_races.clone(),
            });
        }

        let mut family_ids = creature_races
            .iter()
            .filter_map(|race| family_by_race.get(race).cloned())
            .collect::<Vec<_>>();
        family_ids.sort();
        family_ids.dedup();
        if family_ids.len() > 1 {
            issues.push(CreatureCatalogIssue::MultipleCreatureFamilies {
                family_ids: family_ids.clone(),
            });
        }
        dedup_issues(&mut issues);

        plan.npcs.push(CreatureNpcPlan {
            source_npc: npc.form_key,
            effective_races,
            template_records: sorted_form_keys(resolution.template_records, interner),
            family_ids,
            issues,
        });
    }
    plan.npcs
        .sort_by_key(|npc| form_key_sort_key(npc.source_npc, interner));

    for race in &plan.races {
        plan.readiness_ledger.push(CreatureReadinessLedgerEntry {
            subject: CreaturePlanSubject::Race(race.source_race),
            readiness: race.readiness(),
            issues: race.issues.clone(),
        });
    }
    for npc in &plan.npcs {
        plan.readiness_ledger.push(CreatureReadinessLedgerEntry {
            subject: CreaturePlanSubject::Npc(npc.source_npc),
            readiness: npc.readiness(),
            issues: npc.issues.clone(),
        });
    }
    for family in &plan.families {
        plan.readiness_ledger.push(CreatureReadinessLedgerEntry {
            subject: CreaturePlanSubject::Family(family.family_id.clone()),
            readiness: family.readiness(),
            issues: family.issues.clone(),
        });
    }

    plan.summary.candidate_races = plan.races.len();
    plan.summary.record_ready_races = plan
        .races
        .iter()
        .filter(|race| race.readiness() == CreatureReadiness::RecordReady)
        .count();
    plan.summary.rejected_races = plan.summary.candidate_races - plan.summary.record_ready_races;
    plan.summary.family_count = plan.families.len();
    plan.summary.candidate_npcs = plan.npcs.len();
    plan.summary.record_ready_npcs = plan
        .npcs
        .iter()
        .filter(|npc| npc.readiness() == CreatureReadiness::RecordReady)
        .count();
    plan.summary.rejected_npcs = plan.summary.candidate_npcs - plan.summary.record_ready_npcs;
    plan
}

fn build_race_plan(
    race: &Record,
    index: &RecordIndex<'_>,
    interner: &StringInterner,
) -> CreatureRacePlan {
    let project_paths = asset_paths_in_record_order(race, &["MODL"], ".hkx", interner);
    let skeleton_paths = asset_paths_in_record_order(race, &["ANAM"], ".nif", interner);
    let attack_events = string_fields_any(race, &["ATKE", "AttackEvent"], interner);
    let attack_contract = attack_contract(race, interner);
    let skin = first_direct_form_key(race, "WNAM")
        .or_else(|| inferred_skin_for_race(race.form_key, index, interner));
    let body_part_data = first_direct_form_key(race, "GNAM");
    let mut issues = Vec::new();

    if project_paths.is_empty() {
        issues.push(CreatureCatalogIssue::MissingProject);
    }
    if skeleton_paths.is_empty() {
        issues.push(CreatureCatalogIssue::MissingSkeleton);
    }
    if body_part_data.is_none() {
        issues.push(CreatureCatalogIssue::MissingBodyPartData);
    } else if let Some(reference) = body_part_data
        && !index.has_signature(reference, "BPTD")
    {
        issues.push(CreatureCatalogIssue::MissingDependency {
            reference,
            expected_signature: "BPTD",
        });
    }

    let mut armor_addons = Vec::new();
    let mut body_models = Vec::new();
    let mut body_model_parts = Vec::new();
    match skin {
        None => issues.push(CreatureCatalogIssue::MissingRaceSkin),
        Some(skin_key) => match index.get(skin_key) {
            None => issues.push(CreatureCatalogIssue::MissingDependency {
                reference: skin_key,
                expected_signature: "ARMO",
            }),
            Some(skin_record) if skin_record.sig.as_str() != "ARMO" => {
                issues.push(CreatureCatalogIssue::MissingDependency {
                    reference: skin_key,
                    expected_signature: "ARMO",
                });
            }
            Some(skin_record) => {
                let linked_addons = direct_form_keys(skin_record, "MODL");
                if linked_addons.is_empty() {
                    issues.push(CreatureCatalogIssue::MissingArmorAddonLinks { skin: skin_key });
                }
                let mut resolved_addons = Vec::new();
                for armor_addon in &linked_addons {
                    match index.get(*armor_addon) {
                        Some(record) if record.sig.as_str() == "ARMA" => {
                            resolved_addons.push((*armor_addon, record));
                        }
                        _ => issues.push(CreatureCatalogIssue::MissingDependency {
                            reference: *armor_addon,
                            expected_signature: "ARMA",
                        }),
                    }
                }
                let mut selected = resolved_addons
                    .iter()
                    .filter(|(_, record)| armor_addon_applies_to_race(record, race.form_key))
                    .copied()
                    .collect::<Vec<_>>();
                if selected.is_empty() {
                    // A quarter of Skyrim's creature races reach a skin whose addons name only
                    // the base race — MG07DogRace, the undead dragons, the Dawnguard and
                    // Creation Club companions. They render in game, so race conditioning
                    // cannot be a requirement. Every competing addon here declares the same
                    // biped slot, meaning later entries overwrite earlier ones rather than
                    // stacking, so the last declared addon is the one the skin ends up wearing.
                    selected.extend(resolved_addons.last().copied());
                }
                if selected.is_empty() && !linked_addons.is_empty() {
                    issues.push(CreatureCatalogIssue::MissingRaceConditionedArmorAddon {
                        skin: skin_key,
                    });
                }
                for (armor_addon, armor_addon_record) in selected {
                    armor_addons.push(armor_addon);
                    let models =
                        asset_paths(armor_addon_record, &["MOD2", "MOD3"], ".nif", interner);
                    if models.is_empty() {
                        issues.push(CreatureCatalogIssue::MissingBodyModel { armor_addon });
                    }
                    body_model_parts.extend(models.iter().cloned().map(|body_nif| {
                        CreatureBodyModelPlan {
                            armor_addon,
                            body_nif,
                        }
                    }));
                    body_models.extend(models);
                }
            }
        },
    }

    sort_form_keys(&mut armor_addons, interner);
    armor_addons.dedup();
    body_models.sort();
    body_models.dedup();
    body_model_parts.sort_by(|left, right| {
        form_key_sort_key(left.armor_addon, interner)
            .cmp(&form_key_sort_key(right.armor_addon, interner))
            .then_with(|| left.body_nif.cmp(&right.body_nif))
    });
    body_model_parts.dedup();

    let attack_data = race_attack_data(race, index, interner, &mut issues);
    let mut attack_spells = Vec::new();
    for attack_data in race
        .fields
        .iter()
        .filter(|field| matches!(field.sig.as_str(), "ATKD" | "AttackData"))
    {
        attack_spells.extend(collect_form_keys(&attack_data.value).into_iter().filter(
            |form_key| form_key.local != 0 && index.has_any_signature(*form_key, &["SPEL", "SHOU"]),
        ));
        for raw in embedded_form_ids(&attack_data.value, RACE_ATTACK_SPELL_OFFSET, 0) {
            if raw == 0 {
                continue;
            }
            match index.resolve_embedded_form_id(race.form_key, raw, interner) {
                EmbeddedFormResolution::Resolved(form_key)
                    if index.has_any_signature(form_key, &["SPEL", "SHOU"]) =>
                {
                    attack_spells.push(form_key)
                }
                EmbeddedFormResolution::Resolved(_)
                | EmbeddedFormResolution::Ambiguous(_)
                | EmbeddedFormResolution::Missing => {}
            }
        }
    }
    for spell in race
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == "SPLO")
    {
        attack_spells.extend(
            collect_form_keys(&spell.value)
                .into_iter()
                .filter(|form_key| {
                    form_key.local != 0 && index.has_any_signature(*form_key, &["SPEL", "SHOU"])
                }),
        );
        for raw in embedded_form_ids(&spell.value, 0, 0) {
            if raw == 0 {
                continue;
            }
            if let EmbeddedFormResolution::Resolved(form_key) =
                index.resolve_embedded_form_id(race.form_key, raw, interner)
                && index.has_any_signature(form_key, &["SPEL", "SHOU"])
            {
                attack_spells.push(form_key);
            }
        }
    }
    sort_form_keys(&mut attack_spells, interner);
    attack_spells.dedup();
    for attack_type in attack_data.iter().filter_map(|attack| attack.attack_type) {
        if !index.has_any_signature(attack_type, &["KYWD"]) {
            issues.push(CreatureCatalogIssue::MissingDependency {
                reference: attack_type,
                expected_signature: "KYWD",
            });
        }
    }
    dedup_issues(&mut issues);

    CreatureRacePlan {
        source_race: race.form_key,
        source_plugin: interner
            .resolve(race.form_key.plugin)
            .unwrap_or_default()
            .to_string(),
        editor_id: record_editor_id(race, interner),
        skin,
        armor_addons,
        body_models,
        body_model_parts,
        body_part_data,
        project_paths,
        skeleton_paths,
        attack_events,
        attack_contract,
        attack_data,
        attack_spells,
        issues,
    }
}

fn inferred_skin_for_race(
    race: FormKey,
    index: &RecordIndex<'_>,
    interner: &StringInterner,
) -> Option<FormKey> {
    let conditioned_addons = index
        .records_of_signature("ARMA")
        .into_iter()
        .filter(|record| {
            first_direct_form_key(record, "RNAM") == Some(race)
                || direct_form_keys(record, "MODL").contains(&race)
        })
        .map(|record| record.form_key)
        .collect::<HashSet<_>>();
    let mut skins = index
        .records_of_signature("ARMO")
        .into_iter()
        .filter(|record| {
            direct_form_keys(record, "MODL")
                .iter()
                .any(|addon| conditioned_addons.contains(addon))
        })
        .map(|record| record.form_key)
        .collect::<Vec<_>>();
    skins.sort_by_key(|skin| form_key_sort_key(*skin, interner));
    skins.dedup();
    (skins.len() == 1).then_some(skins[0])
}

fn family_key(race: &CreatureRacePlan) -> CreatureFamilyKey {
    CreatureFamilyKey {
        project_paths: family_strings(&race.project_paths),
        skeleton_paths: family_strings(&race.skeleton_paths),
        body_models: family_strings(&race.body_models),
        attack_contract: race.attack_contract.clone(),
    }
}

fn attack_contract(record: &Record, interner: &StringInterner) -> Vec<String> {
    record
        .fields
        .iter()
        .filter_map(|field| match field.sig.as_str() {
            "ATKD" | "AttackData" => Some(format!(
                "atkd:{}",
                field_value_fingerprint(&field.value, interner)
            )),
            "ATKE" | "AttackEvent" => Some(format!(
                "atke:{}",
                field_value_fingerprint(&field.value, interner)
            )),
            _ => None,
        })
        .collect()
}

fn race_attack_data(
    record: &Record,
    index: &RecordIndex<'_>,
    interner: &StringInterner,
    issues: &mut Vec<CreatureCatalogIssue>,
) -> Vec<CreatureRaceAttackDataPlan> {
    let mut receipts = Vec::new();
    let mut ordinal = 0;
    for (field_index, field) in record.fields.iter().enumerate() {
        if !matches!(field.sig.as_str(), "ATKD" | "AttackData") {
            continue;
        }
        ordinal += 1;
        let event = record
            .fields
            .get(field_index + 1)
            .filter(|event| matches!(event.sig.as_str(), "ATKE" | "AttackEvent"))
            .and_then(|event| match event.value {
                FieldValue::String(value) => interner.resolve(value).map(str::to_string),
                _ => None,
            });
        let Some(event) = event.filter(|event| !event.trim().is_empty()) else {
            issues.push(CreatureCatalogIssue::MalformedRaceAttackData {
                ordinal,
                detail: "ATKD is not followed by a non-empty ATKE string".to_string(),
            });
            continue;
        };
        match parse_race_attack_data(
            record.form_key,
            ordinal,
            event,
            &field.value,
            index,
            interner,
        ) {
            Ok(receipt) => receipts.push(receipt),
            Err(issue) => issues.push(issue),
        }
    }
    receipts
}

fn parse_race_attack_data(
    owner: FormKey,
    ordinal: usize,
    event: String,
    value: &FieldValue,
    index: &RecordIndex<'_>,
    interner: &StringInterner,
) -> Result<CreatureRaceAttackDataPlan, CreatureCatalogIssue> {
    let malformed =
        |detail: String| CreatureCatalogIssue::MalformedRaceAttackData { ordinal, detail };
    let mut receipt = match value {
        FieldValue::Bytes(bytes) if bytes.len() == RACE_ATTACK_DATA_LEN => {
            let word =
                |offset: usize| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
            Ok(CreatureRaceAttackDataPlan {
                ordinal,
                event,
                damage_multiplier_bits: word(0),
                attack_chance_bits: word(4),
                attack_spell: resolve_attack_form_optional(
                    owner,
                    word(RACE_ATTACK_SPELL_OFFSET),
                    index,
                    interner,
                ),
                attack_spell_signature: None,
                attack_flags: word(12),
                attack_angle_bits: word(16),
                strike_angle_bits: word(20),
                stagger_bits: word(24),
                attack_type: resolve_attack_form(
                    owner,
                    word(RACE_ATTACK_TYPE_OFFSET),
                    "ATKD.attack_type",
                    index,
                    interner,
                )?,
                attack_type_signature: None,
                knockdown_bits: word(32),
                recovery_time_bits: word(36),
                stamina_multiplier_bits: word(40),
            })
        }
        FieldValue::Bytes(bytes) => Err(malformed(format!(
            "ATKD has {} bytes; expected {RACE_ATTACK_DATA_LEN}",
            bytes.len()
        ))),
        FieldValue::Struct(fields) => Ok(CreatureRaceAttackDataPlan {
            ordinal,
            event,
            damage_multiplier_bits: struct_float_bits(fields, "damage_mult", interner)
                .ok_or_else(|| malformed("ATKD.damage_mult is not a decoded f32".to_string()))?,
            attack_chance_bits: struct_float_bits(fields, "attack_chance", interner)
                .ok_or_else(|| malformed("ATKD.attack_chance is not a decoded f32".to_string()))?,
            attack_spell: struct_form_key(fields, "attack_spell", interner).ok_or_else(|| {
                malformed("ATKD.attack_spell is not a decoded FormKey".to_string())
            })?,
            attack_spell_signature: None,
            attack_flags: struct_u32(fields, "attack_flags", interner)
                .ok_or_else(|| malformed("ATKD.attack_flags is not a decoded u32".to_string()))?,
            attack_angle_bits: struct_float_bits(fields, "attack_angle", interner)
                .ok_or_else(|| malformed("ATKD.attack_angle is not a decoded f32".to_string()))?,
            strike_angle_bits: struct_float_bits(fields, "strike_angle", interner)
                .ok_or_else(|| malformed("ATKD.strike_angle is not a decoded f32".to_string()))?,
            stagger_bits: struct_float_bits(fields, "stagger", interner)
                .ok_or_else(|| malformed("ATKD.stagger is not a decoded f32".to_string()))?,
            attack_type: struct_form_key(fields, "attack_type", interner).ok_or_else(|| {
                malformed("ATKD.attack_type is not a decoded FormKey".to_string())
            })?,
            attack_type_signature: None,
            knockdown_bits: struct_float_bits(fields, "knockdown", interner)
                .ok_or_else(|| malformed("ATKD.knockdown is not a decoded f32".to_string()))?,
            recovery_time_bits: struct_float_bits(fields, "recovery_time", interner)
                .ok_or_else(|| malformed("ATKD.recovery_time is not a decoded f32".to_string()))?,
            stamina_multiplier_bits: struct_float_bits(fields, "stamina_mult", interner)
                .ok_or_else(|| malformed("ATKD.stamina_mult is not a decoded f32".to_string()))?,
        }),
        _ => Err(malformed(
            "ATKD is neither bytes nor a decoded struct".to_string(),
        )),
    }?;
    receipt.attack_spell_signature = receipt
        .attack_spell
        .and_then(|form_key| index.get(form_key))
        .map(|record| record.sig.as_str().to_string());
    if !matches!(
        receipt.attack_spell_signature.as_deref(),
        Some("SPEL" | "SHOU")
    ) {
        receipt.attack_spell = None;
        receipt.attack_spell_signature = None;
    }
    receipt.attack_type_signature = receipt
        .attack_type
        .and_then(|form_key| index.get(form_key))
        .map(|record| record.sig.as_str().to_string());
    Ok(receipt)
}

fn resolve_attack_form_optional(
    owner: FormKey,
    raw: u32,
    index: &RecordIndex<'_>,
    interner: &StringInterner,
) -> Option<FormKey> {
    if raw == 0 {
        return None;
    }
    match index.resolve_embedded_form_id(owner, raw, interner) {
        EmbeddedFormResolution::Resolved(form_key) => Some(form_key),
        EmbeddedFormResolution::Ambiguous(_) | EmbeddedFormResolution::Missing => None,
    }
}

fn resolve_attack_form(
    owner: FormKey,
    raw: u32,
    field: &'static str,
    index: &RecordIndex<'_>,
    interner: &StringInterner,
) -> Result<Option<FormKey>, CreatureCatalogIssue> {
    if raw == 0 {
        return Ok(None);
    }
    match index.resolve_embedded_form_id(owner, raw, interner) {
        EmbeddedFormResolution::Resolved(form_key) => Ok(Some(form_key)),
        EmbeddedFormResolution::Ambiguous(candidates) => {
            Err(CreatureCatalogIssue::AmbiguousEmbeddedFormId {
                owner,
                field,
                raw,
                candidates,
            })
        }
        EmbeddedFormResolution::Missing => {
            Err(CreatureCatalogIssue::UnresolvedEmbeddedFormId { owner, field, raw })
        }
    }
}

fn struct_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    fields
        .iter()
        .find(|(field, _)| interner.resolve(*field) == Some(name))
        .map(|(_, value)| value)
}

fn struct_float_bits(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    match struct_value(fields, name, interner)? {
        FieldValue::Float(value) => Some(value.to_bits()),
        _ => None,
    }
}

fn struct_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    match struct_value(fields, name, interner)? {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn struct_form_key(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<Option<FormKey>> {
    match struct_value(fields, name, interner)? {
        FieldValue::FormKey(form_key) => Some((form_key.local != 0).then_some(*form_key)),
        FieldValue::Uint(0) | FieldValue::Int(0) | FieldValue::None => Some(None),
        _ => None,
    }
}

fn field_value_fingerprint(value: &FieldValue, interner: &StringInterner) -> String {
    match value {
        FieldValue::None => "none".to_string(),
        FieldValue::Bool(value) => format!("bool:{value}"),
        FieldValue::Int(value) => format!("int:{value}"),
        FieldValue::Uint(value) => format!("uint:{value}"),
        FieldValue::Float(value) => format!("float:{:08x}", value.to_bits()),
        FieldValue::String(value) => format!(
            "string:{}",
            interner
                .resolve(*value)
                .unwrap_or_default()
                .trim()
                .to_ascii_lowercase()
        ),
        FieldValue::Bytes(value) => format!("bytes:{}", hex::encode(value)),
        FieldValue::FormKey(value) => {
            let (plugin, local) = form_key_sort_key(*value, interner);
            format!("form:{plugin}:{local:06x}")
        }
        FieldValue::List(values) => format!(
            "list:[{}]",
            values
                .iter()
                .map(|value| field_value_fingerprint(value, interner))
                .collect::<Vec<_>>()
                .join(",")
        ),
        FieldValue::Struct(fields) => format!(
            "struct:{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!(
                    "{}={}",
                    interner
                        .resolve(*name)
                        .unwrap_or_default()
                        .to_ascii_lowercase(),
                    field_value_fingerprint(value, interner)
                ))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

fn family_strings(values: &[String]) -> Vec<String> {
    let mut values = values
        .iter()
        .map(|value| value.trim().to_ascii_lowercase())
        .collect::<Vec<_>>();
    values.sort();
    values.dedup();
    values
}

struct RecordIndex<'a> {
    winners: HashMap<FormKey, &'a Record>,
    keys_by_local: HashMap<u32, Vec<FormKey>>,
    ordered_keys: Vec<FormKey>,
}

impl<'a> RecordIndex<'a> {
    fn new(records: &'a [Record], interner: &StringInterner) -> Self {
        let mut winners = HashMap::new();
        for record in records {
            winners.insert(record.form_key, record);
        }
        let mut keys_by_local = HashMap::<u32, Vec<FormKey>>::new();
        for form_key in winners.keys().copied() {
            keys_by_local
                .entry(form_key.local)
                .or_default()
                .push(form_key);
        }
        for form_keys in keys_by_local.values_mut() {
            sort_form_keys(form_keys, interner);
        }
        let ordered_keys = sorted_form_keys(winners.keys().copied(), interner);
        Self {
            winners,
            keys_by_local,
            ordered_keys,
        }
    }

    fn get(&self, form_key: FormKey) -> Option<&'a Record> {
        self.winners.get(&form_key).copied()
    }

    fn records_of_signature(&self, signature: &str) -> Vec<&'a Record> {
        self.ordered_keys
            .iter()
            .filter_map(|form_key| self.get(*form_key))
            .filter(|record| record.sig.as_str() == signature)
            .collect()
    }

    fn has_signature(&self, form_key: FormKey, signature: &str) -> bool {
        self.get(form_key)
            .is_some_and(|record| record.sig.as_str() == signature)
    }

    fn has_any_signature(&self, form_key: FormKey, signatures: &[&str]) -> bool {
        self.get(form_key)
            .is_some_and(|record| signatures.contains(&record.sig.as_str()))
    }

    fn is_actor_type_npc(&self, race: &Record, interner: &StringInterner) -> bool {
        direct_form_keys(race, "KWDA").into_iter().any(|keyword| {
            keyword.local == ACTOR_TYPE_NPC_LOCAL
                || self.get(keyword).is_some_and(|record| {
                    record_editor_id(record, interner)
                        .is_some_and(|editor_id| editor_id.eq_ignore_ascii_case("ActorTypeNPC"))
                })
        })
    }

    fn resolve_embedded_form_id(
        &self,
        owner: FormKey,
        raw: u32,
        interner: &StringInterner,
    ) -> EmbeddedFormResolution {
        let local = raw & 0x00FF_FFFF;
        let exact_owner = FormKey {
            local,
            plugin: owner.plugin,
        };
        if self.winners.contains_key(&exact_owner) {
            return EmbeddedFormResolution::Resolved(exact_owner);
        }
        let Some(candidates) = self.keys_by_local.get(&local) else {
            return EmbeddedFormResolution::Missing;
        };
        if candidates.len() == 1 {
            EmbeddedFormResolution::Resolved(candidates[0])
        } else {
            EmbeddedFormResolution::Ambiguous(sorted_form_keys(
                candidates.iter().copied(),
                interner,
            ))
        }
    }
}

enum EmbeddedFormResolution {
    Resolved(FormKey),
    Ambiguous(Vec<FormKey>),
    Missing,
}

#[derive(Clone, Default)]
struct TemplateResolution {
    races: HashSet<FormKey>,
    template_records: HashSet<FormKey>,
    issues: Vec<CreatureCatalogIssue>,
}

struct TemplateResolver<'a, 'b> {
    index: &'a RecordIndex<'a>,
    interner: &'b StringInterner,
    memo: HashMap<FormKey, TemplateResolution>,
    stack: Vec<FormKey>,
}

impl<'a, 'b> TemplateResolver<'a, 'b> {
    fn new(index: &'a RecordIndex<'a>, interner: &'b StringInterner) -> Self {
        Self {
            index,
            interner,
            memo: HashMap::new(),
            stack: Vec::new(),
        }
    }

    fn resolve(&mut self, form_key: FormKey) -> TemplateResolution {
        if let Some(resolution) = self.memo.get(&form_key) {
            return resolution.clone();
        }
        if let Some(cycle_start) = self.stack.iter().position(|node| *node == form_key) {
            let mut nodes = self.stack[cycle_start..].to_vec();
            nodes.push(form_key);
            return TemplateResolution {
                issues: vec![CreatureCatalogIssue::TemplateCycle { nodes }],
                ..Default::default()
            };
        }

        self.stack.push(form_key);
        let resolution = match self.index.get(form_key) {
            None => TemplateResolution {
                issues: vec![CreatureCatalogIssue::UnresolvedTemplateTarget {
                    source: self.stack.iter().rev().nth(1).copied().unwrap_or(form_key),
                    target: form_key,
                }],
                ..Default::default()
            },
            Some(record) if record.sig.as_str() == "NPC_" => self.resolve_npc(record),
            Some(record) if record.sig.as_str() == "LVLN" => self.resolve_leveled_list(record),
            Some(record) => TemplateResolution {
                issues: vec![CreatureCatalogIssue::UnsupportedTemplateTarget {
                    target: form_key,
                    signature: record.sig.as_str().to_string(),
                }],
                ..Default::default()
            },
        };
        self.stack.pop();
        self.memo.insert(form_key, resolution.clone());
        resolution
    }

    fn resolve_npc(&mut self, npc: &Record) -> TemplateResolution {
        let mut resolution = TemplateResolution::default();
        resolution.template_records.insert(npc.form_key);
        if !npc_uses_traits(npc) {
            match first_direct_form_key(npc, "RNAM") {
                Some(race) => {
                    resolution.races.insert(race);
                }
                None => resolution
                    .issues
                    .push(CreatureCatalogIssue::MissingDirectRace),
            }
            return resolution;
        }

        let Some(template) = first_direct_form_key(npc, "TPLT") else {
            resolution
                .issues
                .push(CreatureCatalogIssue::MissingTemplateTarget);
            return resolution;
        };
        merge_resolution(&mut resolution, self.resolve(template));
        resolution
    }

    fn resolve_leveled_list(&mut self, leveled_list: &Record) -> TemplateResolution {
        let mut resolution = TemplateResolution::default();
        resolution.template_records.insert(leveled_list.form_key);
        let mut targets = Vec::new();
        for field in leveled_list
            .fields
            .iter()
            .filter(|field| field.sig.as_str() == "LVLO")
        {
            let direct = collect_form_keys(&field.value);
            if !direct.is_empty() {
                targets.extend(direct);
                continue;
            }
            for raw in embedded_form_ids(&field.value, LVLN_REFERENCE_OFFSET, LVLN_ROW_SIZE) {
                if raw == 0 {
                    continue;
                }
                match self
                    .index
                    .resolve_embedded_form_id(leveled_list.form_key, raw, self.interner)
                {
                    EmbeddedFormResolution::Resolved(target) => targets.push(target),
                    EmbeddedFormResolution::Ambiguous(candidates) => {
                        resolution
                            .issues
                            .push(CreatureCatalogIssue::AmbiguousEmbeddedFormId {
                                owner: leveled_list.form_key,
                                field: "LVLO.npc",
                                raw,
                                candidates,
                            });
                    }
                    EmbeddedFormResolution::Missing => {
                        resolution
                            .issues
                            .push(CreatureCatalogIssue::UnresolvedEmbeddedFormId {
                                owner: leveled_list.form_key,
                                field: "LVLO.npc",
                                raw,
                            });
                    }
                }
            }
        }
        sort_form_keys(&mut targets, self.interner);
        targets.dedup();
        if targets.is_empty() {
            resolution
                .issues
                .push(CreatureCatalogIssue::EmptyLeveledTemplate {
                    leveled_list: leveled_list.form_key,
                });
            return resolution;
        }
        for target in targets {
            merge_resolution(&mut resolution, self.resolve(target));
        }
        resolution
    }
}

fn merge_resolution(into: &mut TemplateResolution, from: TemplateResolution) {
    into.races.extend(from.races);
    into.template_records.extend(from.template_records);
    extend_unique(&mut into.issues, from.issues);
}

fn npc_uses_traits(npc: &Record) -> bool {
    npc.fields
        .iter()
        .find(|field| field.sig.as_str() == "ACBS")
        .and_then(|field| match &field.value {
            FieldValue::Bytes(bytes) if bytes.len() >= NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2 => {
                Some(u16::from_le_bytes([
                    bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET],
                    bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 1],
                ]))
            }
            _ => None,
        })
        .is_some_and(|flags| flags & TEMPLATE_FLAG_USE_TRAITS != 0)
}

fn is_curated_exclusion(record: &Record, interner: &StringInterner) -> bool {
    record_editor_id(record, interner).is_some_and(|editor_id| {
        let editor_id = editor_id.to_ascii_lowercase();
        CURATED_NON_CREATURE_RACES.contains(&editor_id.as_str())
            || CURATED_TEST_RACES.contains(&editor_id.as_str())
    })
}

fn record_editor_id(record: &Record, interner: &StringInterner) -> Option<String> {
    record
        .eid
        .and_then(|editor_id| interner.resolve(editor_id))
        .map(str::to_string)
}

fn first_direct_form_key(record: &Record, signature: &str) -> Option<FormKey> {
    direct_form_keys(record, signature).into_iter().next()
}

fn armor_addon_applies_to_race(armor_addon: &Record, race: FormKey) -> bool {
    first_direct_form_key(armor_addon, "RNAM") == Some(race)
        || direct_form_keys(armor_addon, "MODL").contains(&race)
}

fn direct_form_keys(record: &Record, signature: &str) -> Vec<FormKey> {
    let mut form_keys = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature)
        .flat_map(|field| collect_form_keys(&field.value))
        .filter(|form_key| form_key.local != 0)
        .collect::<Vec<_>>();
    let mut seen = HashSet::new();
    form_keys.retain(|form_key| seen.insert(*form_key));
    form_keys
}

fn collect_form_keys(value: &FieldValue) -> Vec<FormKey> {
    let mut form_keys = Vec::new();
    collect_form_keys_into(value, &mut form_keys);
    form_keys
}

fn collect_form_keys_into(value: &FieldValue, output: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(form_key) => output.push(*form_key),
        FieldValue::List(values) => {
            for value in values {
                collect_form_keys_into(value, output);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, value) in fields {
                collect_form_keys_into(value, output);
            }
        }
        _ => {}
    }
}

fn asset_paths(
    record: &Record,
    signatures: &[&str],
    extension: &str,
    interner: &StringInterner,
) -> Vec<String> {
    let mut paths = record
        .fields
        .iter()
        .filter(|field| signatures.contains(&field.sig.as_str()))
        .filter_map(|field| match field.value {
            FieldValue::String(value) => interner.resolve(value),
            _ => None,
        })
        .filter(|value| value.trim().to_ascii_lowercase().ends_with(extension))
        .map(canonical_asset_path)
        .collect::<Vec<_>>();
    paths.sort_by_key(|path| path.to_ascii_lowercase());
    paths.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    paths
}

fn asset_paths_in_record_order(
    record: &Record,
    signatures: &[&str],
    extension: &str,
    interner: &StringInterner,
) -> Vec<String> {
    let mut seen = HashSet::new();
    record
        .fields
        .iter()
        .filter(|field| signatures.contains(&field.sig.as_str()))
        .filter_map(|field| match field.value {
            FieldValue::String(value) => interner.resolve(value),
            _ => None,
        })
        .filter(|value| value.trim().to_ascii_lowercase().ends_with(extension))
        .map(canonical_asset_path)
        .filter(|path| seen.insert(path.to_ascii_lowercase()))
        .collect()
}

fn string_fields(record: &Record, signature: &str, interner: &StringInterner) -> Vec<String> {
    string_fields_any(record, &[signature], interner)
}

fn string_fields_any(
    record: &Record,
    signatures: &[&str],
    interner: &StringInterner,
) -> Vec<String> {
    let mut values = record
        .fields
        .iter()
        .filter(|field| signatures.contains(&field.sig.as_str()))
        .filter_map(|field| match field.value {
            FieldValue::String(value) => interner.resolve(value).map(str::to_string),
            _ => None,
        })
        .collect::<Vec<_>>();
    values.sort_by_key(|value| value.to_ascii_lowercase());
    values.dedup_by(|left, right| left.eq_ignore_ascii_case(right));
    values
}

fn canonical_asset_path(value: &str) -> String {
    value.trim().replace('/', "\\")
}

fn embedded_form_ids(value: &FieldValue, offset: usize, stride: usize) -> Vec<u32> {
    match value {
        FieldValue::FormKey(form_key) => vec![form_key.local],
        FieldValue::List(values) => values
            .iter()
            .flat_map(|value| embedded_form_ids(value, offset, stride))
            .collect(),
        FieldValue::Struct(fields) => fields
            .iter()
            .flat_map(|(_, value)| embedded_form_ids(value, offset, stride))
            .collect(),
        FieldValue::Bytes(bytes) => {
            if stride == 0 {
                return (bytes.len() >= offset + 4)
                    .then(|| u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap()))
                    .into_iter()
                    .collect();
            }
            let stride = stride.max(offset + 4);
            bytes
                .chunks(stride)
                .filter(|chunk| chunk.len() >= offset + 4)
                .map(|chunk| u32::from_le_bytes(chunk[offset..offset + 4].try_into().unwrap()))
                .collect()
        }
        _ => Vec::new(),
    }
}

fn form_key_sort_key(form_key: FormKey, interner: &StringInterner) -> (String, u32) {
    (
        interner
            .resolve(form_key.plugin)
            .unwrap_or_default()
            .to_ascii_lowercase(),
        form_key.local,
    )
}

fn sort_form_keys(form_keys: &mut [FormKey], interner: &StringInterner) {
    form_keys.sort_by_key(|form_key| form_key_sort_key(*form_key, interner));
}

fn sorted_form_keys(
    form_keys: impl IntoIterator<Item = FormKey>,
    interner: &StringInterner,
) -> Vec<FormKey> {
    let mut form_keys = form_keys.into_iter().collect::<Vec<_>>();
    sort_form_keys(&mut form_keys, interner);
    form_keys.dedup();
    form_keys
}

fn extend_unique<T: PartialEq>(output: &mut Vec<T>, values: impl IntoIterator<Item = T>) {
    for value in values {
        if !output.contains(&value) {
            output.push(value);
        }
    }
}

fn dedup_issues(issues: &mut Vec<CreatureCatalogIssue>) {
    let mut deduped = Vec::with_capacity(issues.len());
    extend_unique(&mut deduped, issues.drain(..));
    *issues = deduped;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::{FieldEntry, Record};
    use smallvec::{SmallVec, smallvec};

    fn form_key(interner: &StringInterner, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern("CreatureCatalog.esm"),
        }
    }

    fn record(interner: &StringInterner, signature: &str, local: u32, eid: &str) -> Record {
        let mut record = Record::new(
            SigCode::from_str(signature).unwrap(),
            form_key(interner, local),
        );
        record.eid = Some(interner.intern(eid));
        record
    }

    fn push(record: &mut Record, signature: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(signature).unwrap(),
            value,
        });
    }

    fn push_form(record: &mut Record, signature: &str, form_key: FormKey) {
        push(record, signature, FieldValue::FormKey(form_key));
    }

    fn push_string(record: &mut Record, signature: &str, value: &str, interner: &StringInterner) {
        push(
            record,
            signature,
            FieldValue::String(interner.intern(value)),
        );
    }

    fn acbs(use_traits: bool) -> FieldValue {
        let mut bytes = vec![0u8; 24];
        if use_traits {
            bytes[NPC_ACBS_TEMPLATE_FLAGS_OFFSET..NPC_ACBS_TEMPLATE_FLAGS_OFFSET + 2]
                .copy_from_slice(&TEMPLATE_FLAG_USE_TRAITS.to_le_bytes());
        }
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn lvlo(target: FormKey) -> FieldValue {
        let mut bytes = vec![0u8; LVLN_ROW_SIZE];
        bytes[LVLN_REFERENCE_OFFSET..LVLN_REFERENCE_OFFSET + 4]
            .copy_from_slice(&target.local.to_le_bytes());
        FieldValue::Bytes(SmallVec::from_vec(bytes))
    }

    fn complete_race_records(
        interner: &StringInterner,
        base: u32,
        race_eid: &str,
        project: &str,
    ) -> Vec<Record> {
        let race_key = form_key(interner, base);
        let skin_key = form_key(interner, base + 1);
        let addon_key = form_key(interner, base + 2);
        let body_part_key = form_key(interner, base + 3);

        let mut race = record(interner, "RACE", base, race_eid);
        push_form(&mut race, "WNAM", skin_key);
        push_form(&mut race, "GNAM", body_part_key);
        push_string(&mut race, "ANAM", "Actors\\Test\\Skeleton.nif", interner);
        push_string(&mut race, "MODL", project, interner);
        push_string(&mut race, "ATKE", "attackStart_Attack1", interner);

        let mut skin = record(interner, "ARMO", base + 1, "Skin");
        push_form(&mut skin, "MODL", addon_key);
        let mut addon = record(interner, "ARMA", base + 2, "Addon");
        push_form(&mut addon, "RNAM", race_key);
        push_string(
            &mut addon,
            "MOD2",
            "Actors\\Test\\Character Assets\\Body.nif",
            interner,
        );
        let body_part = record(interner, "BPTD", base + 3, "BodyPartData");
        vec![race, skin, addon, body_part]
    }

    #[test]
    fn race_project_paths_preserve_source_sex_order() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x680,
            "DualProjectRace",
            "Actors\\Primary\\PrimaryProject.hkx",
        );
        push_string(
            &mut records[0],
            "MODL",
            "Actors\\Alternate\\AlternateProject.hkx",
            &interner,
        );

        let plan = build_creature_corpus_plan(&records, &interner);

        assert_eq!(
            plan.races[0].project_paths,
            vec![
                "Actors\\Primary\\PrimaryProject.hkx",
                "Actors\\Alternate\\AlternateProject.hkx"
            ]
        );
    }

    #[test]
    fn race_spells_join_the_creature_attack_dependency_set() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x690,
            "SpellCreatureRace",
            "Actors\\SpellCreature\\SpellCreatureProject.hkx",
        );
        let spell = record(&interner, "SPEL", 0x699, "SpellCreatureAttack");
        push_form(&mut records[0], "SPLO", spell.form_key);
        records.push(spell);

        let plan = build_creature_corpus_plan(&records, &interner);

        assert_eq!(
            plan.races[0].attack_spells,
            vec![form_key(&interner, 0x699)]
        );
    }

    #[test]
    fn attack_data_receipt_preserves_all_words_and_form_signatures() {
        let interner = StringInterner::new();
        let owner = form_key(&interner, 0x700);
        let spell = record(&interner, "SHOU", 0x701, "VoiceAttack");
        let attack_type = record(&interner, "KYWD", 0x702, "BiteAttack");
        let records = vec![spell, attack_type];
        let index = RecordIndex::new(&records, &interner);
        let words = [
            1.25_f32.to_bits(),
            0.75_f32.to_bits(),
            0x701,
            0xA5A5_0001,
            45.0_f32.to_bits(),
            30.0_f32.to_bits(),
            0.5_f32.to_bits(),
            0x702,
            0.25_f32.to_bits(),
            1.5_f32.to_bits(),
            2.0_f32.to_bits(),
        ];
        let bytes = words
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .collect::<SmallVec<[u8; 32]>>();
        let receipt = parse_race_attack_data(
            owner,
            3,
            "attackStart_Bite".to_string(),
            &FieldValue::Bytes(bytes),
            &index,
            &interner,
        )
        .unwrap();

        assert_eq!(receipt.ordinal, 3);
        assert_eq!(receipt.event, "attackStart_Bite");
        assert_eq!(receipt.damage_multiplier_bits, words[0]);
        assert_eq!(receipt.attack_chance_bits, words[1]);
        assert_eq!(receipt.attack_spell, Some(form_key(&interner, 0x701)));
        assert_eq!(receipt.attack_spell_signature.as_deref(), Some("SHOU"));
        assert_eq!(receipt.attack_flags, words[3]);
        assert_eq!(receipt.attack_angle_bits, words[4]);
        assert_eq!(receipt.strike_angle_bits, words[5]);
        assert_eq!(receipt.stagger_bits, words[6]);
        assert_eq!(receipt.attack_type, Some(form_key(&interner, 0x702)));
        assert_eq!(receipt.attack_type_signature.as_deref(), Some("KYWD"));
        assert_eq!(receipt.knockdown_bits, words[8]);
        assert_eq!(receipt.recovery_time_bits, words[9]);
        assert_eq!(receipt.stamina_multiplier_bits, words[10]);
    }

    #[test]
    fn nonadjacent_attack_event_is_a_typed_catalog_issue() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x710,
            "WolfRace",
            "Actors\\Canine\\WolfProject.hkx",
        );
        let mut attack = vec![0_u8; RACE_ATTACK_DATA_LEN];
        attack[0..4].copy_from_slice(&1.0_f32.to_le_bytes());
        records[0].fields.insert(
            3,
            FieldEntry {
                sig: SubrecordSig::from_str("ATKD").unwrap(),
                value: FieldValue::Bytes(SmallVec::from_vec(attack)),
            },
        );

        let plan = build_creature_corpus_plan(&records, &interner);
        assert!(plan.races[0].issues.iter().any(|issue| matches!(
            issue,
            CreatureCatalogIssue::MalformedRaceAttackData { ordinal: 1, detail }
                if detail.contains("not followed")
        )));
    }

    #[test]
    fn classifier_excludes_actor_type_npc_and_curated_specials() {
        let interner = StringInterner::new();
        let actor_type_npc = form_key(&interner, ACTOR_TYPE_NPC_LOCAL);
        let mut keyword = record(&interner, "KYWD", ACTOR_TYPE_NPC_LOCAL, "ActorTypeNPC");
        keyword.form_key = actor_type_npc;

        let mut humanoid = record(&interner, "RACE", 0x100, "NordRace");
        push(
            &mut humanoid,
            "KWDA",
            FieldValue::List(vec![FieldValue::FormKey(actor_type_npc)]),
        );
        push_string(
            &mut humanoid,
            "ANAM",
            "Actors\\Character\\Skeleton.nif",
            &interner,
        );
        push_string(
            &mut humanoid,
            "MODL",
            "Actors\\Character\\DefaultMale.hkx",
            &interner,
        );
        let special = record(&interner, "RACE", 0x200, "DefaultRace");
        let mut records = vec![keyword, humanoid, special];
        records.extend(complete_race_records(
            &interner,
            0x300,
            "DwarvenSpiderRace",
            "Actors\\DwarvenSpider\\DwarvenSpiderProject.hkx",
        ));

        let plan = build_creature_corpus_plan(&records, &interner);
        assert_eq!(plan.summary.total_race_records, 3);
        assert_eq!(plan.summary.actor_type_npc_races, 1);
        assert_eq!(plan.summary.curated_exclusions, 1);
        assert_eq!(plan.summary.candidate_races, 1);
        assert_eq!(
            plan.races[0].editor_id.as_deref(),
            Some("DwarvenSpiderRace")
        );
    }

    #[test]
    fn skin_closure_excludes_armor_addons_conditioned_for_other_races() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x400,
            "WolfRace",
            "Actors\\Canine\\WolfProject.hkx",
        );
        let unrelated_key = form_key(&interner, 0x410);
        let unrelated_race = form_key(&interner, 0x411);
        push_form(&mut records[1], "MODL", unrelated_key);
        let mut unrelated = record(&interner, "ARMA", 0x410, "UnrelatedAddon");
        push_form(&mut unrelated, "RNAM", unrelated_race);
        push_string(
            &mut unrelated,
            "MOD2",
            "Actors\\Wrong\\Wrong.nif",
            &interner,
        );
        records.push(unrelated);

        let plan = build_creature_corpus_plan(&records, &interner);
        assert_eq!(plan.races[0].armor_addons, vec![form_key(&interner, 0x402)]);
        assert_eq!(
            plan.races[0].body_models,
            vec!["Actors\\Test\\Character Assets\\Body.nif"]
        );
    }

    #[test]
    fn skin_closure_preserves_addons_with_the_race_in_additional_races() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x440,
            "WolfRace",
            "Actors\\Canine\\WolfProject.hkx",
        );
        let race = form_key(&interner, 0x440);
        records[2]
            .fields
            .retain(|field| field.sig.as_str() != "RNAM");
        push_form(&mut records[2], "RNAM", form_key(&interner, 0x450));
        push_form(&mut records[2], "MODL", race);

        let plan = build_creature_corpus_plan(&records, &interner);

        assert_eq!(plan.races[0].armor_addons, vec![form_key(&interner, 0x442)]);
        assert_eq!(
            plan.races[0].body_models,
            vec!["Actors\\Test\\Character Assets\\Body.nif"]
        );
    }

    #[test]
    fn skin_closure_falls_back_to_the_last_addon_when_none_names_the_race() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x460,
            "MG07DogRace",
            "Actors\\Canine\\DogProject.hkx",
        );
        records[2]
            .fields
            .retain(|field| field.sig.as_str() != "RNAM");
        push_form(&mut records[2], "RNAM", form_key(&interner, 0x470));

        let plan = build_creature_corpus_plan(&records, &interner);

        assert_eq!(plan.races[0].armor_addons, vec![form_key(&interner, 0x462)]);
        assert_eq!(
            plan.races[0].body_models,
            vec!["Actors\\Test\\Character Assets\\Body.nif"]
        );
        assert_eq!(plan.races[0].issues, Vec::new());
    }

    #[test]
    fn skin_closure_fallback_prefers_the_last_of_several_unconditioned_addons() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x480,
            "DLC1_BF_ChaurusRace",
            "Actors\\Chaurus\\ChaurusProject.hkx",
        );
        records[2]
            .fields
            .retain(|field| field.sig.as_str() != "RNAM");
        push_form(&mut records[2], "RNAM", form_key(&interner, 0x490));

        let leftover_key = form_key(&interner, 0x491);
        records[1]
            .fields
            .retain(|field| field.sig.as_str() != "MODL");
        push_form(&mut records[1], "MODL", leftover_key);
        push_form(&mut records[1], "MODL", form_key(&interner, 0x482));
        let mut leftover = record(&interner, "ARMA", 0x491, "NakedCowAA");
        push_form(&mut leftover, "RNAM", form_key(&interner, 0x492));
        push_string(
            &mut leftover,
            "MOD2",
            "Actors\\Cow\\Character Assets\\HighlandCow.nif",
            &interner,
        );
        records.push(leftover);

        let plan = build_creature_corpus_plan(&records, &interner);

        assert_eq!(plan.races[0].armor_addons, vec![form_key(&interner, 0x482)]);
        assert_eq!(
            plan.races[0].body_models,
            vec!["Actors\\Test\\Character Assets\\Body.nif"]
        );
        assert_eq!(plan.races[0].issues, Vec::new());
    }

    #[test]
    fn skin_closure_excludes_first_person_armor_addon_models() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x420,
            "WerewolfRace",
            "Actors\\WerewolfBeast\\WerewolfBeastProject.hkx",
        );
        push_string(
            &mut records[2],
            "MOD3",
            "Actors\\WerewolfBeast\\Character Assets\\FemaleBodyWerewolf_1.nif",
            &interner,
        );
        push_string(
            &mut records[2],
            "MOD4",
            "Actors\\Character\\Character Assets\\1stPersonMaleBody_1.nif",
            &interner,
        );
        push_string(
            &mut records[2],
            "MOD5",
            "Actors\\Character\\Character Assets\\1stPersonFemaleBody_1.nif",
            &interner,
        );

        let plan = build_creature_corpus_plan(&records, &interner);

        assert_eq!(
            plan.races[0].body_models,
            vec![
                "Actors\\Test\\Character Assets\\Body.nif",
                "Actors\\WerewolfBeast\\Character Assets\\FemaleBodyWerewolf_1.nif",
            ]
        );
        assert!(
            plan.races[0]
                .body_models
                .iter()
                .all(|path| !path.to_ascii_lowercase().contains("1stperson"))
        );
    }

    #[test]
    fn traits_template_expands_through_leveled_npc_list() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x500,
            "DwarvenCenturionRace",
            "Actors\\DwarvenCenturion\\CenturionProject.hkx",
        );
        let race_key = form_key(&interner, 0x500);
        let leaf_key = form_key(&interner, 0x510);
        let list_key = form_key(&interner, 0x511);
        let root_key = form_key(&interner, 0x512);

        let mut leaf = record(&interner, "NPC_", 0x510, "CenturionTemplate");
        push(&mut leaf, "ACBS", acbs(false));
        push_form(&mut leaf, "RNAM", race_key);
        let mut list = record(&interner, "LVLN", 0x511, "LCharCenturion");
        push(&mut list, "LVLO", lvlo(leaf_key));
        let mut root = record(&interner, "NPC_", 0x512, "LvlCenturionAmbush");
        push(&mut root, "ACBS", acbs(true));
        push_form(&mut root, "TPLT", list_key);
        push_form(&mut root, "RNAM", form_key(&interner, 0x999));
        records.extend([leaf, list, root]);

        let plan = build_creature_corpus_plan(&records, &interner);
        let root = plan
            .npcs
            .iter()
            .find(|npc| npc.source_npc == root_key)
            .unwrap();
        assert_eq!(root.effective_races, vec![race_key]);
        assert!(root.template_records.contains(&list_key));
        assert!(root.template_records.contains(&leaf_key));
        assert_eq!(root.readiness(), CreatureReadiness::RecordReady);
    }

    #[test]
    fn template_cycle_is_typed_instead_of_dropped() {
        let interner = StringInterner::new();
        let first_key = form_key(&interner, 0x600);
        let second_key = form_key(&interner, 0x601);
        let mut first = record(&interner, "NPC_", 0x600, "CycleA");
        push(&mut first, "ACBS", acbs(true));
        push_form(&mut first, "TPLT", second_key);
        let mut second = record(&interner, "NPC_", 0x601, "CycleB");
        push(&mut second, "ACBS", acbs(true));
        push_form(&mut second, "TPLT", first_key);

        let plan = build_creature_corpus_plan(&[first, second], &interner);
        assert!(plan.readiness_ledger.iter().any(|entry| {
            matches!(entry.subject, CreaturePlanSubject::Npc(key) if key == first_key)
                && entry
                    .issues
                    .iter()
                    .any(|issue| matches!(issue, CreatureCatalogIssue::TemplateCycle { .. }))
        }));
    }

    #[test]
    fn grouping_is_deterministic_and_uses_the_full_runtime_contract() {
        let interner = StringInterner::new();
        let mut records = complete_race_records(
            &interner,
            0x700,
            "WolfRace",
            "Actors\\Canine\\WolfProject.hkx",
        );
        records.extend(complete_race_records(
            &interner,
            0x800,
            "WolfSpiritRace",
            "Actors\\Canine\\WolfProject.hkx",
        ));
        let forward = build_creature_corpus_plan(&records, &interner);
        records.reverse();
        let reversed = build_creature_corpus_plan(&records, &interner);
        assert_eq!(forward.families, reversed.families);
        assert_eq!(forward.families.len(), 1);
        assert_eq!(forward.families[0].races.len(), 2);
    }

    #[test]
    fn optional_live_merged_corpus_accounts_for_every_candidate() {
        let Some(path) =
            std::env::var_os("SKYRIMSE_CREATURE_CORPUS_PLUGIN").map(std::path::PathBuf::from)
        else {
            return;
        };
        if !path.is_file() {
            return;
        }
        let handle = esp_authoring_core::plugin_runtime::plugin_handle_load_no_py(
            path.to_str().unwrap(),
            Some("skyrimse"),
            None,
            None,
            true,
        )
        .unwrap();
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
        let mut records = Vec::new();
        for signature in [
            "KYWD", "RACE", "NPC_", "LVLN", "ARMO", "ARMA", "BPTD", "SPEL", "SHOU",
        ] {
            let sig = SigCode::from_str(signature).unwrap();
            let form_keys =
                crate::source_read::iter_form_keys_of_sig(handle, sig, &interner).unwrap();
            for form_key in form_keys {
                records.push(
                    crate::source_read::read_record_relayout_by_form_key(
                        handle, &form_key, &schema, &interner, None,
                    )
                    .unwrap(),
                );
            }
        }
        let plan = build_creature_corpus_plan(&records, &interner);
        esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);

        let seeker = plan
            .races
            .iter()
            .find(|race| race.editor_id.as_deref() == Some("DLC2SeekerRace"))
            .expect("merged corpus must include DLC2SeekerRace");
        assert_eq!(seeker.attack_spells.len(), 2);

        assert_eq!(plan.summary.total_race_records, 161);
        assert_eq!(plan.summary.actor_type_npc_races, 36);
        assert_eq!(plan.summary.curated_exclusions, 4);
        assert_eq!(plan.summary.candidate_races, 121);
        assert_eq!(plan.races.len(), plan.summary.candidate_races);
        assert_eq!(
            plan.readiness_ledger
                .iter()
                .filter(|entry| matches!(entry.subject, CreaturePlanSubject::Race(_)))
                .count(),
            125
        );
        assert_eq!(
            plan.races.iter().filter(|race| race.skin.is_some()).count(),
            121
        );
    }
}
