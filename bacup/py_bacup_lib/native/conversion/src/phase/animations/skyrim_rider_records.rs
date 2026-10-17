use crate::fixups::FixupConfig;
use crate::fixups::face::build_additive_race_record::{SubgraphBlock, build_additive_race_record};
use crate::fixups::face::generate_additive_races::load_target_race_template_from_masters;
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

use super::{RIDER_ANIMATIONS, RIDER_GRAPH};

#[derive(Clone, Debug)]
pub(crate) struct RiderRegistration {
    pub keyword: FormKey,
    pub additive_race: FormKey,
    pub enter_idle: FormKey,
    pub exit_idle: FormKey,
    pub quick_exit_idle: FormKey,
    pub horse_enter_idle: FormKey,
    pub horse_exit_idle: FormKey,
    pub horse_quick_exit_idle: FormKey,
    pub records: Vec<Record>,
}

pub(crate) fn prepare_registration(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper<'_>,
    config: &FixupConfig,
) -> Result<RiderRegistration, String> {
    let schema = session.schema().map_err(|e| e.to_string())?;
    let interner = mapper.interner;
    let parent = fo4(0x166729, interner);
    let template = load_target_race_template_from_masters(session, &schema, mapper, config, parent)
        .ok_or("FO4 HumanRaceSubGraphData template unavailable for riding")?;
    let source_plugin = interner.intern("B21_SkyrimRiderGenerated.esm");
    let specifications = [
        ("KYWD", "B21_SkyrimRidingKeyword"),
        ("RACE", "B21_SkyrimRiderHumanAdditive"),
        ("IDLE", "B21_SkyrimRiderEnter"),
        ("IDLE", "B21_SkyrimRiderExit"),
        ("IDLE", "B21_SkyrimRiderQuickExit"),
        ("IDLE", "B21_SkyrimHorseEnter"),
        ("IDLE", "B21_SkyrimHorseExit"),
        ("IDLE", "B21_SkyrimHorseQuickExit"),
    ];
    let keys = std::array::from_fn::<_, 8, _>(|index| {
        mapper.allocate_or_resolve(
            FormKey {
                plugin: source_plugin,
                local: index as u32 + 1,
            },
            Some(interner.intern(specifications[index].1)),
            SigCode::from_str(specifications[index].0).unwrap(),
        )
    });
    let mut keyword = Record::new(SigCode(*b"KYWD"), keys[0]);
    set_eid(&mut keyword, specifications[0].1, interner);
    let block = SubgraphBlock {
        behaviour_graph: interner.intern(RIDER_GRAPH),
        paths: vec![interner.intern(RIDER_ANIMATIONS)],
        subgraph_keywords: Vec::new(),
        target_keywords: vec![fo4(0x0EF219, interner), keys[0]],
        flags_bytes: Some(vec![2, 0, 0, 0].into()),
    };
    let mut race = build_additive_race_record(template, parent, &[block]);
    race.form_key = keys[1];
    set_eid(&mut race, specifications[1].1, interner);
    let mut records = vec![keyword, race];
    let horse_graph =
        crate::skyrimse_fo4_runtime::creature_mvp_live::skyrim_family_root_behavior_path("horse");
    for (index, (parent_id, event)) in [
        (0x02248F, "sitStartFromStand"),
        (0x02248D, "standStart"),
        (0x02248C, "QuickExitToStand"),
        (0x02248F, "sitStartFromStand"),
        (0x02248D, "standStart"),
        (0x02248C, "QuickExitToStand"),
    ]
    .into_iter()
    .enumerate()
    {
        let mut idle = Record::new(SigCode(*b"IDLE"), keys[index + 2]);
        set_eid(&mut idle, specifications[index + 2].1, interner);
        idle.fields.extend([
            field(
                *b"CTDA",
                structure(
                    interner,
                    &[
                        ("type", FieldValue::Uint(0)),
                        ("unknown_u8_1", FieldValue::Uint(0)),
                        ("unknown_u8_2", FieldValue::Uint(0)),
                        ("unknown_u8_3", FieldValue::Uint(0)),
                        ("comparison_value", FieldValue::Float(1.0)),
                        ("function", FieldValue::Uint(560)),
                        ("unknown_u8_6", FieldValue::Uint(0)),
                        ("unknown_u8_7", FieldValue::Uint(0)),
                        ("parameter_1", FieldValue::FormKey(keys[0])),
                        ("parameter_2", FieldValue::Uint(0)),
                        ("run_on", FieldValue::Uint(if index < 3 { 1 } else { 0 })),
                        ("reference", FieldValue::Uint(0)),
                        ("parameter_3", FieldValue::Int(-1)),
                    ],
                ),
            ),
            field(
                *b"DNAM",
                FieldValue::String(interner.intern(if index < 3 {
                    RIDER_GRAPH
                } else {
                    &horse_graph
                })),
            ),
            field(*b"ENAM", FieldValue::String(interner.intern(event))),
            field(
                *b"ANAM",
                structure(
                    interner,
                    &[
                        ("parent", FieldValue::FormKey(fo4(parent_id, interner))),
                        ("previous", FieldValue::Uint(0)),
                    ],
                ),
            ),
            field(
                *b"DATA",
                structure(
                    interner,
                    &[
                        ("looping_seconds_both_255_forever_min", FieldValue::Uint(0)),
                        ("looping_seconds_both_255_forever_max", FieldValue::Uint(0)),
                        ("flags", FieldValue::Uint(0)),
                        ("animation_group_section", FieldValue::Uint(0)),
                        ("replay_delay", FieldValue::Uint(0)),
                    ],
                ),
            ),
        ]);
        records.push(idle);
    }
    Ok(RiderRegistration {
        keyword: keys[0],
        additive_race: keys[1],
        enter_idle: keys[2],
        exit_idle: keys[3],
        quick_exit_idle: keys[4],
        horse_enter_idle: keys[5],
        horse_exit_idle: keys[6],
        horse_quick_exit_idle: keys[7],
        records,
    })
}

fn fo4(local: u32, interner: &StringInterner) -> FormKey {
    FormKey {
        local,
        plugin: interner.intern("Fallout4.esm"),
    }
}
fn field(sig: [u8; 4], value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(sig),
        value,
    }
}
fn structure(interner: &StringInterner, values: &[(&str, FieldValue)]) -> FieldValue {
    FieldValue::Struct(
        values
            .iter()
            .map(|(name, value)| (interner.intern(name), value.clone()))
            .collect(),
    )
}
fn set_eid(record: &mut Record, name: &str, interner: &StringInterner) {
    let eid = interner.intern(name);
    record.eid = Some(eid);
    record.fields.retain(|entry| entry.sig.0 != *b"EDID");
    record
        .fields
        .insert(0, field(*b"EDID", FieldValue::String(eid)));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixups::face::build_additive_race_record::parse_canonical_subgraphs;
    use crate::formkey_mapper::MapperOptions;
    use crate::session::open_session;
    use esp_authoring_core::plugin_runtime::{
        plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
    };

    #[test]
    fn rider_registration_encodes_and_rereads_scoped_native_interaction_records() {
        let mut interner = StringInterner::new();
        let target = plugin_handle_new_native("B21_RiderTest.esp", Some("fo4")).unwrap();
        let master = plugin_handle_new_native("Fallout4.esm", Some("fo4")).unwrap();
        plugin_handle_add_master_native(target, "Fallout4.esm", None).unwrap();
        {
            let mut session = open_session(master, None).unwrap();
            let schema = session.schema().unwrap();
            let mut race = Record::new(SigCode(*b"RACE"), fo4(0x166729, &interner));
            set_eid(&mut race, "HumanRaceSubGraphData", &interner);
            session.add_record(race, &schema, &interner).unwrap();
        }
        {
            let mut mapper = FormKeyMapper::new(
                [],
                MapperOptions {
                    output_plugin_name: "B21_RiderTest.esp".into(),
                    target_master_names: vec!["Fallout4.esm".into()],
                    ..Default::default()
                },
                &mut interner,
            );
            let mut session = open_session(target, None).unwrap();
            let schema = session.schema().unwrap();
            let registration = prepare_registration(
                &mut session,
                &mut mapper,
                &FixupConfig {
                    target_master_handle_ids: vec![master],
                    ..Default::default()
                },
            )
            .unwrap();
            assert!(
                session
                    .form_keys_of_sig(SigCode(*b"IDLE"), mapper.interner)
                    .unwrap()
                    .is_empty()
            );
            let batch = crate::target_write::prepare_record_batch_in_slot(
                session.target_slot(),
                registration.records.clone(),
                &schema,
                mapper.interner,
            )
            .unwrap_or_else(|error| {
                panic!("record {} failed: {}", error.record_index, error.source)
            });
            assert_eq!(session.commit_prepared_record_batch(batch), 8);
            let race = session
                .record_decoded(&registration.additive_race, &schema, mapper.interner)
                .unwrap();
            let blocks = parse_canonical_subgraphs(&race);
            assert_eq!(blocks.len(), 1);
            assert_eq!(
                blocks[0].target_keywords,
                [fo4(0x0EF219, mapper.interner), registration.keyword]
            );
            assert_eq!(
                blocks[0].flags_bytes.as_deref(),
                Some([2, 0, 0, 0].as_slice())
            );
            for (key, event, run_on) in [
                (registration.enter_idle, "sitStartFromStand", 1),
                (registration.exit_idle, "standStart", 1),
                (registration.quick_exit_idle, "QuickExitToStand", 1),
                (registration.horse_enter_idle, "sitStartFromStand", 0),
                (registration.horse_exit_idle, "standStart", 0),
                (registration.horse_quick_exit_idle, "QuickExitToStand", 0),
            ] {
                let idle = session
                    .record_decoded(&key, &schema, mapper.interner)
                    .unwrap();
                assert!(idle.fields.iter().any(|field| field.sig.0 == *b"ENAM" && matches!(field.value,FieldValue::String(value) if mapper.interner.resolve(value)==Some(event))));
                let condition = idle
                    .fields
                    .iter()
                    .find(|field| field.sig.0 == *b"CTDA")
                    .unwrap();
                let FieldValue::Bytes(values) = &condition.value else {
                    panic!("condition did not decode to its native payload")
                };
                assert_eq!(values.len(), 32);
                assert_eq!(values[0], 0);
                assert_eq!(f32::from_le_bytes(values[4..8].try_into().unwrap()), 1.0);
                assert_eq!(u16::from_le_bytes(values[8..10].try_into().unwrap()), 560);
                assert_eq!(
                    u32::from_le_bytes(values[20..24].try_into().unwrap()),
                    run_on
                );
                assert_eq!(
                    u32::from_le_bytes(values[12..16].try_into().unwrap()),
                    session
                        .raw_form_id_for_form_key(&registration.keyword)
                        .unwrap()
                );
            }
        }
        assert!(plugin_handle_close_native(target));
        assert!(plugin_handle_close_native(master));
    }
}
