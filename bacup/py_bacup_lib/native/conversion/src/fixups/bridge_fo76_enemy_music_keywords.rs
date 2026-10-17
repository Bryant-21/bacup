//! FO76 picks a combat track by music keywords on the enemy's race. Races FO76 shares with FO4
//! (Human, Super Mutant, Feral Ghoul, Deathclaw...) convert to the FO4 master race, which lacks
//! those keywords, so FO76 combat tracks never qualify against those enemies. The keywords go on
//! the converted NPCs rather than an override of the FO4 race.

use rustc_hash::FxHashMap;

use crate::fixups::rewrite_raw_object_template_formids::encode_target_form_id;
use crate::fixups::{Fixup, FixupConfig, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

/// Parameters of FO76 `GetStrongestEnemyHasKeyword` music-track conditions: the
/// AudioMusicCombatType tiers and the Cryptid, Zetan, Metal, Moonshine and Bigfoot types.
const FO76_ENEMY_MUSIC_KEYWORD_LOCALS: &[u32] = &[
    0x1D_4D53, 0x1D_49BB, 0x1D_4D54, 0x6A_C47B, 0x63_50BC, 0x63_5A6C, 0x65_107B, 0x8A_ADC9,
];

pub struct BridgeFo76EnemyMusicKeywordsFixup;

impl Fixup for BridgeFo76EnemyMusicKeywordsFixup {
    fn name(&self) -> &'static str {
        "bridge_fo76_enemy_music_keywords"
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
        let race_keywords = master_race_music_keywords(session, mapper)?;
        if race_keywords.is_empty() {
            return Ok(report);
        }
        let schema = session.schema().map_err(handle_error)?;
        let target_plugin = session.target_slot().parsed.plugin_name.clone();
        let target_masters = session.target_masters().to_vec();
        let output_plugin = mapper.output_plugin_sym();
        let mut changed = Vec::new();
        for fk in session
            .form_keys_of_sig(SigCode(*b"NPC_"), mapper.interner)
            .map_err(handle_error)?
        {
            if fk.plugin != output_plugin {
                continue;
            }
            let Some(race) = session
                .first_subrecord_bytes(&fk, "RNAM")
                .map_err(handle_error)?
                .and_then(|bytes| Some(u32::from_le_bytes(bytes.get(..4)?.try_into().ok()?)))
                .map(|raw| resolve_raw_form_id(raw, &target_masters, &target_plugin, mapper.interner))
            else {
                continue;
            };
            let Some(keywords) = race_keywords.get(&race) else {
                continue;
            };
            let mut record = session
                .record_decoded(&fk, &schema, mapper.interner)
                .map_err(handle_error)?;
            if append_keywords(&mut record, keywords, &target_masters, &target_plugin, mapper.interner) {
                changed.push(record);
            }
        }
        let expected = changed.len();
        let replaced = session
            .replace_records_contents(changed, &schema, mapper.interner)
            .map_err(handle_error)?;
        if replaced != expected {
            return Err(FixupError::Other(format!(
                "enemy music keywords replaced {replaced} of {expected} NPC records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        report.diagnostics.push(mapper.interner.intern(&format!(
            "enemy_music_keywords:races={}:npcs={replaced}",
            race_keywords.len()
        )));
        Ok(report)
    }
}

/// Target master race -> target music keywords, for every source race whose music keywords
/// would be lost because it converts to a master race instead of an output record.
fn master_race_music_keywords(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
) -> Result<FxHashMap<FormKey, Vec<FormKey>>, FixupError> {
    let mut races = FxHashMap::default();
    let Some(source) = session.source_slot_opt() else {
        return Ok(races);
    };
    let source_plugin = source.parsed.plugin_name.clone();
    let source_masters = source.parsed.header.masters.clone();
    let source_plugin_sym = mapper.interner.intern(&source_plugin);
    let source_schema = session.source_schema().map_err(handle_error)?;
    let output_plugin = mapper.output_plugin_sym();
    for fk in session
        .source_form_keys_of_sig(SigCode(*b"RACE"), mapper.interner)
        .map_err(handle_error)?
    {
        let Ok(race) = session.source_record_decoded(&fk, &source_schema, mapper.interner) else {
            continue;
        };
        let music: Vec<FormKey> =
            keyword_form_keys(&race, &source_masters, &source_plugin, mapper.interner)
                .into_iter()
                .filter(|keyword| {
                    keyword.plugin == source_plugin_sym
                        && FO76_ENEMY_MUSIC_KEYWORD_LOCALS.contains(&keyword.local)
                })
                .filter_map(|keyword| mapper.lookup(keyword))
                .collect();
        if music.is_empty() {
            continue;
        }
        if let Some(target) = mapper.lookup(fk).filter(|target| target.plugin != output_plugin) {
            races.insert(target, music);
        }
    }
    Ok(races)
}

/// `KWDA` is a `formid_array` that arrives typed or as raw bytes depending on how far the
/// record was decoded; both shapes are read and extended in place.
pub(crate) fn keyword_form_keys(
    record: &Record,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
) -> Vec<FormKey> {
    record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"KWDA")
        .flat_map(|entry| match &entry.value {
            FieldValue::List(items) => items
                .iter()
                .filter_map(|item| match item {
                    FieldValue::FormKey(fk) => Some(*fk),
                    _ => None,
                })
                .collect(),
            FieldValue::FormKey(fk) => vec![*fk],
            FieldValue::Bytes(bytes) => bytes
                .chunks_exact(4)
                .map(|chunk| {
                    let raw = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
                    resolve_raw_form_id(raw, masters, plugin, interner)
                })
                .collect(),
            _ => Vec::new(),
        })
        .collect()
}

pub(crate) fn append_keywords(
    record: &mut Record,
    keywords: &[FormKey],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> bool {
    let present = keyword_form_keys(record, target_masters, target_plugin, interner);
    let missing: Vec<FormKey> = keywords
        .iter()
        .copied()
        .filter(|keyword| !present.contains(keyword))
        .collect();
    if missing.is_empty() {
        return false;
    }
    let raw_missing = || {
        missing
            .iter()
            .filter_map(|keyword| encode_target_form_id(*keyword, interner, target_masters))
            .flat_map(u32::to_le_bytes)
    };
    match record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"KWDA")
        .map(|entry| &mut entry.value)
    {
        Some(FieldValue::List(items)) => items.extend(missing.iter().copied().map(FieldValue::FormKey)),
        Some(FieldValue::Bytes(bytes)) => bytes.extend(raw_missing()),
        Some(_) => return false,
        None => record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"KWDA"),
            value: FieldValue::Bytes(raw_missing().collect()),
        }),
    }
    sync_keyword_count(record);
    true
}

fn sync_keyword_count(record: &mut Record) {
    let count = record
        .fields
        .iter()
        .filter(|entry| entry.sig.0 == *b"KWDA")
        .map(|entry| match &entry.value {
            FieldValue::List(items) => items.len(),
            FieldValue::Bytes(bytes) => bytes.len() / 4,
            FieldValue::FormKey(_) => 1,
            _ => 0,
        })
        .sum::<usize>() as u64;
    if let Some(entry) = record.fields.iter_mut().find(|entry| entry.sig.0 == *b"KSIZ") {
        entry.value = match &entry.value {
            FieldValue::Bytes(_) => FieldValue::Bytes((count as u32).to_le_bytes().to_vec().into()),
            _ => FieldValue::Uint(count),
        };
    } else if let Some(index) = record.fields.iter().position(|entry| entry.sig.0 == *b"KWDA") {
        record.fields.insert(
            index,
            FieldEntry {
                sig: SubrecordSig(*b"KSIZ"),
                value: FieldValue::Uint(count),
            },
        );
    }
}

fn resolve_raw_form_id(
    raw: u32,
    masters: &[String],
    plugin: &str,
    interner: &StringInterner,
) -> FormKey {
    let plugin_name = masters
        .get(((raw >> 24) & 0xFF) as usize)
        .map(String::as_str)
        .unwrap_or(plugin);
    FormKey {
        local: raw & 0x00FF_FFFF,
        plugin: interner.intern(plugin_name),
    }
}

fn handle_error(error: impl std::fmt::Display) -> FixupError {
    FixupError::HandleError(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::{MapperOptions, MapperState};
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    const MASTERS: &[&str] = &["Fallout4.esm"];

    fn masters() -> Vec<String> {
        MASTERS.iter().map(|name| name.to_string()).collect()
    }

    fn npc(interner: &StringInterner, kwda: Option<FieldValue>, ksiz: Option<FieldValue>) -> Record {
        let mut record = Record::new(
            SigCode(*b"NPC_"),
            FormKey {
                local: 0x00_1089,
                plugin: interner.intern("SeventySix.esm"),
            },
        );
        if let Some(value) = ksiz {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"KSIZ"),
                value,
            });
        }
        if let Some(value) = kwda {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"KWDA"),
                value,
            });
        }
        record
    }

    fn keyword_count(record: &Record) -> Option<u64> {
        record.fields.iter().find(|entry| entry.sig.0 == *b"KSIZ").map(|entry| match &entry.value {
            FieldValue::Uint(count) => *count,
            FieldValue::Bytes(bytes) => u32::from_le_bytes(bytes[..4].try_into().unwrap()) as u64,
            other => panic!("unexpected KSIZ {other:?}"),
        })
    }

    #[test]
    fn appends_music_keyword_to_raw_keyword_list() {
        let interner = StringInterner::new();
        let vanilla = 0x0002_4593u32;
        let mut record = npc(
            &interner,
            Some(FieldValue::Bytes(vanilla.to_le_bytes().to_vec().into())),
            Some(FieldValue::Uint(1)),
        );
        let low = FormKey {
            local: 0x1D_4D53,
            plugin: interner.intern("SeventySix.esm"),
        };

        assert!(append_keywords(&mut record, &[low], &masters(), "SeventySix.esm", &interner));

        let keywords = keyword_form_keys(&record, &masters(), "SeventySix.esm", &interner);
        assert_eq!(keywords.len(), 2);
        assert_eq!(keywords[1], low);
        assert_eq!(interner.resolve(keywords[0].plugin), Some("Fallout4.esm"));
        assert_eq!(keyword_count(&record), Some(2));
    }

    #[test]
    fn appends_to_typed_keyword_list_once() {
        let interner = StringInterner::new();
        let low = FormKey {
            local: 0x1D_4D53,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mut record = npc(
            &interner,
            Some(FieldValue::List(vec![])),
            Some(FieldValue::Bytes(0u32.to_le_bytes().to_vec().into())),
        );

        assert!(append_keywords(&mut record, &[low], &masters(), "SeventySix.esm", &interner));
        assert!(!append_keywords(&mut record, &[low], &masters(), "SeventySix.esm", &interner));

        assert_eq!(keyword_form_keys(&record, &masters(), "SeventySix.esm", &interner), vec![low]);
        assert_eq!(keyword_count(&record), Some(1));
    }

    fn field(sig: &[u8; 4], value: FieldValue) -> FieldEntry {
        FieldEntry {
            sig: SubrecordSig(*sig),
            value,
        }
    }

    #[test]
    fn restores_master_race_music_keyword_on_its_converted_npcs_only() {
        let interner = StringInterner::new();
        let fallout4 = interner.intern("Fallout4.esm");
        let source = interner.intern("SeventySixSource.esm");
        let output = interner.intern("SeventySix.esm");
        let fk = |local, plugin| FormKey { local, plugin };
        let human_source = fk(0x01_3746, source);
        let scorched_source = fk(0x10_CA5F, source);
        let low_source = fk(0x1D_4D53, source);
        let unrelated_source = fk(0x02_4593, source);
        let human = fk(0x01_3746, fallout4);
        let scorched = fk(0x10_CA5F, output);
        let low = fk(0x1D_4D53, output);
        let raider = fk(0x00_1089, output);
        let scorched_npc = fk(0x00_108A, output);

        let source_handle = plugin_handle_new_native("SeventySixSource.esm", Some("fo76")).unwrap();
        let target_handle = plugin_handle_new_native("SeventySix.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target_handle, "Fallout4.esm", None).unwrap();
        let fo76_schema = crate::schema::AuthoringSchema::for_game("fo76").unwrap();
        let fo4_schema = crate::schema::AuthoringSchema::for_game("fo4").unwrap();
        {
            let mut session = open_session(source_handle, None).unwrap();
            for race in [human_source, scorched_source] {
                let mut record = Record::new(SigCode(*b"RACE"), race);
                record.fields.push(field(b"KSIZ", FieldValue::Uint(2)));
                record.fields.push(field(
                    b"KWDA",
                    FieldValue::List(vec![
                        FieldValue::FormKey(unrelated_source),
                        FieldValue::FormKey(low_source),
                    ]),
                ));
                session.add_record(record, fo76_schema.as_ref(), &interner).unwrap();
            }
        }
        {
            let mut session = open_session(target_handle, None).unwrap();
            for (npc, race) in [(raider, human), (scorched_npc, scorched)] {
                let mut record = Record::new(SigCode(*b"NPC_"), npc);
                record.fields.push(field(b"RNAM", FieldValue::FormKey(race)));
                session.add_record(record, fo4_schema.as_ref(), &interner).unwrap();
            }
        }

        let mut state = MapperState::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "SeventySix.esm".into(),
                ..Default::default()
            },
        );
        let mut mapper = FormKeyMapper::from_state(&mut state, &interner);
        mapper.add_mapping(human_source, human);
        mapper.add_mapping(scorched_source, scorched);
        mapper.add_mapping(low_source, low);
        let mut session = open_session(target_handle, Some(source_handle)).unwrap();
        let report = BridgeFo76EnemyMusicKeywordsFixup
            .run_with_session(&mut session, &mut mapper, &FixupConfig::default())
            .unwrap();

        assert_eq!(report.records_changed, 1);
        let masters = masters();
        let keywords_of = |session: &mut PluginSession, npc| {
            let record = session.record_decoded(&npc, fo4_schema.as_ref(), &interner).unwrap();
            keyword_form_keys(&record, &masters, "SeventySix.esm", &interner)
        };
        assert_eq!(keywords_of(&mut session, raider), vec![low]);
        assert!(keywords_of(&mut session, scorched_npc).is_empty());

        drop(session);
        assert!(plugin_handle_close_native(target_handle));
        assert!(plugin_handle_close_native(source_handle));
    }

    #[test]
    fn creates_keyword_list_and_count_when_missing() {
        let interner = StringInterner::new();
        let med = FormKey {
            local: 0x1D_49BB,
            plugin: interner.intern("SeventySix.esm"),
        };
        let mut record = npc(&interner, None, None);

        assert!(append_keywords(&mut record, &[med], &masters(), "SeventySix.esm", &interner));

        assert_eq!(keyword_form_keys(&record, &masters(), "SeventySix.esm", &interner), vec![med]);
        assert_eq!(keyword_count(&record), Some(1));
        let order: Vec<_> = record.fields.iter().map(|entry| entry.sig.0).collect();
        assert_eq!(order, vec![*b"KSIZ", *b"KWDA"]);
    }
}
