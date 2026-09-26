//! Copy FO76 encounter-wave spawn pools onto the quests that run them.
//!
//! FO76 `DefaultQuestEncounterWaveScript` resolves each `EncounterWaves` entry
//! to WAVE records, through its `StoredEncounterWave` or else every WAVE whose
//! keywords carry its `EncounterTypeKeyword`. FO4 has no WAVE record type, so
//! the WAVE entries are attached to the quest as `B21:EncounterWaveCatalog`
//! parallel arrays, one row per WAVE entry.
//!
//! Event-specific wave scripts (Spin the Wheel's `E09B_MobWaves`, which extends
//! `EncounterWaveParentScript`) carry the same `EncounterWaves` struct, so any
//! root script with that property is read. `DefaultQuestEncounterWaveScript`
//! wins when a quest has more than one.

use esp_authoring_core::plugin_runtime::ParsedSubrecord;
use esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json;
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value;

use crate::fixups::quest_script_binding::{
    SourcePlugin, attach_quest_script, emitted_target_form, int_array_property,
    object_array_property, script_vmad,
};
use crate::fixups::quest_script_vmad::AttachResult;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode};
use crate::session::{HandleRawScan, PluginSession};
use crate::sym::StringInterner;

const SOURCE_SCRIPT_NAME: &str = "DefaultQuestEncounterWaveScript";
const SOURCE_WAVES_PROPERTY: &str = "EncounterWaves";
const STORED_WAVE_MEMBER: &str = "StoredEncounterWave";
const WAVE_KEYWORD_MEMBER: &str = "EncounterTypeKeyword";
const CATALOG_SCRIPT_NAME: &str = "B21:EncounterWaveCatalog";
const WAVD_ROW_LEN: usize = 12;
const BROAD_KEYWORD_MATCHES: usize = 8;

pub struct AttachFo76EncounterWaveCatalogFixup;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct WaveEntry {
    spawn_slot: i32,
    spawn: FormKey,
}

#[derive(Default)]
struct WaveIndex {
    entries: FxHashMap<FormKey, Vec<WaveEntry>>,
    by_keyword: FxHashMap<FormKey, Vec<FormKey>>,
}

impl WaveIndex {
    fn insert(&mut self, wave: FormKey, keywords: &[FormKey], entries: Vec<WaveEntry>) {
        for keyword in keywords {
            let waves = self.by_keyword.entry(*keyword).or_default();
            if !waves.contains(&wave) {
                waves.push(wave);
            }
        }
        self.entries.insert(wave, entries);
    }

    fn sort_keyword_matches(&mut self) {
        for waves in self.by_keyword.values_mut() {
            waves.sort_unstable_by_key(|wave| wave.local);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WaveSelector {
    Stored(FormKey),
    Keyword(FormKey),
    Unresolved,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct SourceRow {
    wave_index: i32,
    source_variant: i32,
    spawn_slot: i32,
    spawn: FormKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct CatalogRow {
    wave_index: i32,
    variant: i32,
    spawn_slot: i32,
    form: FormKey,
}

struct SourceQuest {
    quest: FormKey,
    selectors: Vec<WaveSelector>,
}

impl Fixup for AttachFo76EncounterWaveCatalogFixup {
    fn name(&self) -> &'static str {
        "attach_fo76_encounter_wave_catalog"
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to_session(&self, session: &PluginSession, _config: &FixupConfig) -> bool {
        session
            .source_slot_opt()
            .and_then(|slot| slot.parsed.game.as_deref())
            == Some("fo76")
            && session.target_slot().parsed.game.as_deref() == Some("fo4")
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        _config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let Some(source) = SourcePlugin::of(session, mapper.interner) else {
            return Ok(report);
        };
        let (wave_index, source_quests) = {
            let scan = session.handle_raw_scan(source.id).map_err(handle_error)?;
            (
                build_wave_index(&scan, &source, mapper.interner),
                source_wave_quests(&scan, &source, mapper.interner),
            )
        };

        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let target_quests = session
            .form_keys_of_sig(SigCode(*b"QUST"), mapper.interner)
            .map_err(handle_error)?
            .into_iter()
            .collect::<FxHashSet<_>>();

        let mut attached = 0u32;
        let mut present = 0u32;
        let mut unresolved = 0u32;
        let mut without_rows = 0u32;
        let mut total_rows = 0usize;
        let mut total_dropped = 0u32;
        for source_quest in source_quests {
            let quest_local = source_quest.quest.local;
            for (wave_index, keyword, matches) in
                broad_keyword_matches(&source_quest.selectors, &wave_index)
            {
                report.diagnostics.push(mapper.interner.intern(&format!(
                    "attach_fo76_encounter_wave_catalog:{quest_local:06X}:wave={wave_index}:keyword={:06X}:wave_records={matches}",
                    keyword.local
                )));
            }
            let source_rows = resolve_source_rows(&source_quest.selectors, &wave_index);
            let mut lookup_error = None;
            let (rows, dropped) = map_catalog_rows(&source_rows, |spawn| {
                emitted_target_form(session, mapper, spawn).unwrap_or_else(|error| {
                    lookup_error = Some(error);
                    None
                })
            });
            if let Some(error) = lookup_error {
                return Err(error);
            }
            if dropped > 0 {
                total_dropped += dropped;
                warn(
                    &mut report,
                    mapper.interner,
                    quest_local,
                    &format!("dropped_unmapped_spawns={dropped}"),
                );
            }
            if rows.is_empty() {
                without_rows += 1;
                continue;
            }

            let Some(target_quest) = mapper
                .lookup(source_quest.quest)
                .filter(|target| target_quests.contains(target))
            else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest_local,
                    "target_quest_missing",
                );
                continue;
            };
            let Some(vmad) =
                catalog_script_vmad(&rows, &target_masters, &target_plugin, mapper.interner)
            else {
                unresolved += 1;
                warn(
                    &mut report,
                    mapper.interner,
                    quest_local,
                    "catalog_vmad_encode_failed",
                );
                continue;
            };

            match attach_quest_script(session, &target_quest, CATALOG_SCRIPT_NAME, &vmad)? {
                Some(AttachResult::Changed) => {
                    report.records_changed += 1;
                    attached += 1;
                    total_rows += rows.len();
                }
                Some(AttachResult::AlreadyPresent) => {
                    present += 1;
                    total_rows += rows.len();
                }
                Some(AttachResult::Conflict(reason)) => {
                    unresolved += 1;
                    warn(&mut report, mapper.interner, quest_local, reason);
                }
                None => {
                    unresolved += 1;
                    warn(
                        &mut report,
                        mapper.interner,
                        quest_local,
                        "quest_vmad_missing",
                    );
                }
            }
        }

        report.message = Some(mapper.interner.intern(&format!(
            "fo76_encounter_wave_catalog:attached={attached};present={present};unresolved={unresolved};without_rows={without_rows};rows={total_rows};dropped_rows={total_dropped}"
        )));
        Ok(report)
    }
}

fn build_wave_index(
    scan: &HandleRawScan<'_>,
    source: &SourcePlugin,
    interner: &StringInterner,
) -> WaveIndex {
    let mut index = WaveIndex::default();
    for raw_form_id in source.own_raw_form_ids(scan, SigCode(*b"WAVE")) {
        let Some(wave) = source.form_key(raw_form_id, interner) else {
            continue;
        };
        let parsed = scan.with_record_subrecords(raw_form_id, |subrecords| {
            parse_wave_subrecords(subrecords, source, interner)
        });
        if let Some((keywords, entries)) = parsed {
            index.insert(wave, &keywords, entries);
        }
    }
    index.sort_keyword_matches();
    index
}

fn parse_wave_subrecords(
    subrecords: &[ParsedSubrecord],
    source: &SourcePlugin,
    interner: &StringInterner,
) -> (Vec<FormKey>, Vec<WaveEntry>) {
    let mut keywords = Vec::new();
    let mut entries = Vec::new();
    for subrecord in subrecords {
        match subrecord.signature.as_str() {
            "KWDA" => keywords.extend(
                (0..subrecord.data.len() / 4)
                    .filter_map(|row| source.form_key_at(&subrecord.data, row * 4, interner)),
            ),
            "WAVD" => entries.extend(subrecord.data.chunks_exact(WAVD_ROW_LEN).filter_map(|row| {
                let spawn_slot =
                    i32::try_from(u32::from_le_bytes(row[0..4].try_into().ok()?)).ok()?;
                let spawn = source.form_key_at(row, 4, interner)?;
                Some(WaveEntry { spawn_slot, spawn })
            })),
            _ => {}
        }
    }
    (keywords, entries)
}

fn source_wave_quests(
    scan: &HandleRawScan<'_>,
    source: &SourcePlugin,
    interner: &StringInterner,
) -> Vec<SourceQuest> {
    source
        .own_raw_form_ids(scan, SigCode(*b"QUST"))
        .into_iter()
        .filter_map(|raw_form_id| {
            let quest = source.form_key(raw_form_id, interner)?;
            let selectors = scan.with_record_subrecords(raw_form_id, |subrecords| {
                subrecords
                    .iter()
                    .filter(|subrecord| subrecord.signature.as_str() == "VMAD")
                    .filter(|subrecord| {
                        contains_ignore_ascii_case(
                            &subrecord.data,
                            SOURCE_WAVES_PROPERTY.as_bytes(),
                        )
                    })
                    .find_map(|subrecord| {
                        let payload = compact_vmad_payload_json(
                            &subrecord.data,
                            source.masters(),
                            &source.name,
                            Some("QUST"),
                        )?;
                        encounter_wave_selectors(&payload, interner)
                    })
            })??;
            Some(SourceQuest { quest, selectors })
        })
        .collect()
}

fn encounter_wave_selectors(
    payload: &Value,
    interner: &StringInterner,
) -> Option<Vec<WaveSelector>> {
    let candidates = payload
        .get("Scripts")?
        .as_array()?
        .iter()
        .filter_map(|script| {
            let waves = named_value(
                script.get("Properties")?,
                "propertyName",
                SOURCE_WAVES_PROPERTY,
            )?;
            let is_default = script
                .get("ScriptName")
                .and_then(Value::as_str)
                .is_some_and(|name| name.eq_ignore_ascii_case(SOURCE_SCRIPT_NAME));
            Some((is_default, waves))
        })
        .collect::<Vec<_>>();
    let waves = candidates
        .iter()
        .find(|(is_default, _)| *is_default)
        .or_else(|| candidates.first())?
        .1
        .as_array()?;
    Some(
        waves
            .iter()
            .map(|wave| {
                let member_form = |name| {
                    named_value(wave, "memberName", name)
                        .and_then(|value| object_form(value, interner))
                };
                if let Some(stored) = member_form(STORED_WAVE_MEMBER) {
                    WaveSelector::Stored(stored)
                } else if let Some(keyword) = member_form(WAVE_KEYWORD_MEMBER) {
                    WaveSelector::Keyword(keyword)
                } else {
                    WaveSelector::Unresolved
                }
            })
            .collect(),
    )
}

fn wave_records_for(selector: WaveSelector, index: &WaveIndex) -> &[FormKey] {
    match selector {
        WaveSelector::Stored(wave) => index
            .entries
            .get_key_value(&wave)
            .map_or(&[], |(wave, _)| std::slice::from_ref(wave)),
        WaveSelector::Keyword(keyword) => index.by_keyword.get(&keyword).map_or(&[], Vec::as_slice),
        WaveSelector::Unresolved => &[],
    }
}

fn resolve_source_rows(selectors: &[WaveSelector], index: &WaveIndex) -> Vec<SourceRow> {
    let mut rows = Vec::new();
    for (wave_index, selector) in selectors.iter().enumerate() {
        let Ok(wave_index) = i32::try_from(wave_index) else {
            break;
        };
        for (source_variant, wave) in wave_records_for(*selector, index).iter().enumerate() {
            let Ok(source_variant) = i32::try_from(source_variant) else {
                break;
            };
            rows.extend(
                index
                    .entries
                    .get(wave)
                    .into_iter()
                    .flatten()
                    .map(|entry| SourceRow {
                        wave_index,
                        source_variant,
                        spawn_slot: entry.spawn_slot,
                        spawn: entry.spawn,
                    }),
            );
        }
    }
    rows
}

fn broad_keyword_matches(
    selectors: &[WaveSelector],
    index: &WaveIndex,
) -> Vec<(usize, FormKey, usize)> {
    selectors
        .iter()
        .enumerate()
        .filter_map(|(wave_index, selector)| match selector {
            WaveSelector::Keyword(keyword) => {
                let matches = wave_records_for(*selector, index).len();
                (matches > BROAD_KEYWORD_MATCHES).then_some((wave_index, *keyword, matches))
            }
            _ => None,
        })
        .collect()
}

/// Map spawn forms to the output, dropping rows whose form was not emitted.
///
/// Variants are renumbered densely per wave over the WAVE records that kept at
/// least one row: the catalog picks a variant below `VariantCount`, so a WAVE
/// record whose whole pool was dropped must not leave an empty variant behind.
fn map_catalog_rows(
    source_rows: &[SourceRow],
    mut resolve: impl FnMut(FormKey) -> Option<FormKey>,
) -> (Vec<CatalogRow>, u32) {
    let mut rows = Vec::with_capacity(source_rows.len());
    let mut dropped = 0u32;
    let mut variants = FxHashMap::<(i32, i32), i32>::default();
    let mut variant_counts = FxHashMap::<i32, i32>::default();
    for row in source_rows {
        let Some(form) = resolve(row.spawn) else {
            dropped += 1;
            continue;
        };
        let variant = *variants
            .entry((row.wave_index, row.source_variant))
            .or_insert_with(|| {
                let count = variant_counts.entry(row.wave_index).or_default();
                *count += 1;
                *count - 1
            });
        rows.push(CatalogRow {
            wave_index: row.wave_index,
            variant,
            spawn_slot: row.spawn_slot,
            form,
        });
    }
    (rows, dropped)
}

fn catalog_script_vmad(
    rows: &[CatalogRow],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    if rows.is_empty() {
        return None;
    }
    let properties = vec![
        int_array_property(
            "CandidateWaveIndices",
            &rows.iter().map(|row| row.wave_index).collect::<Vec<_>>(),
        ),
        int_array_property(
            "CandidateVariants",
            &rows.iter().map(|row| row.variant).collect::<Vec<_>>(),
        ),
        int_array_property(
            "CandidateSpawnSlots",
            &rows.iter().map(|row| row.spawn_slot).collect::<Vec<_>>(),
        ),
        object_array_property(
            "CandidateForms",
            &rows.iter().map(|row| row.form).collect::<Vec<_>>(),
            interner,
        )?,
    ];
    script_vmad(
        CATALOG_SCRIPT_NAME,
        properties,
        target_masters,
        target_plugin,
    )
}

fn named_value<'a>(items: &'a Value, name_key: &str, name: &str) -> Option<&'a Value> {
    items
        .as_array()?
        .iter()
        .find(|item| {
            item.get(name_key)
                .and_then(Value::as_str)
                .is_some_and(|value| value.eq_ignore_ascii_case(name))
        })?
        .get("Value")
}

fn object_form(value: &Value, interner: &StringInterner) -> Option<FormKey> {
    let reference = value.get("FormID")?.get("reference")?;
    let local = u32::from_str_radix(reference.get("object_id")?.as_str()?, 16).ok()?;
    if local == 0 {
        return None;
    }
    Some(FormKey {
        plugin: interner.intern(reference.get("plugin")?.as_str()?),
        local,
    })
}

fn contains_ignore_ascii_case(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle))
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

fn warn(report: &mut FixupReport, interner: &StringInterner, source_quest: u32, reason: &str) {
    report.warnings.push(interner.intern(&format!(
        "attach_fo76_encounter_wave_catalog:{source_quest:06X}:{reason}"
    )));
}

#[cfg(test)]
mod tests;
