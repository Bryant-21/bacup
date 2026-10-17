use super::*;
use crate::ids::SigCode;
use crate::record::FieldEntry;
use crate::session::open_session;
use esp_authoring_core::plugin_runtime::{
    plugin_handle_add_master_native, plugin_handle_close_native, plugin_handle_new_native,
};

const OUTPUT: &str = "SeventySix.esm";
const FALLOUT4: &str = "Fallout4.esm";
const ROBOT: &str = "DLCRobot.esm";
const NUKA_WORLD: &str = "DLCNukaWorld.esm";
/// `DLC04EncBrahmiluff02`, its Nuka-World faction and a Fallout4.esm faction.
const BRAHMILUFF: u32 = 0x03F0A2;
const NUKA_WORLD_FACTION: u32 = 0x04A5BD;
const FALLOUT4_FACTION: u32 = 0x0330BB;
/// The leveled list its traits slot templates from.
const TRAITS_LIST: u32 = 0x03F0A0;
const TPTA_SLOTS: usize = 13;

fn bytes_field(sig: &[u8; 4], data: Vec<u8>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value: FieldValue::Bytes(data.into()),
    }
}

/// FO4 NPC_ SNAM: faction FormID, then the rank byte.
fn faction(raw: u32) -> Vec<u8> {
    let mut bytes = raw.to_le_bytes().to_vec();
    bytes.push(0);
    bytes
}

fn raw_fields(record: &Record, sig: &[u8; 4]) -> Vec<Vec<u8>> {
    record
        .fields
        .iter()
        .filter(|field| field.sig.0 == *sig)
        .map(|field| match &field.value {
            FieldValue::Bytes(bytes) => bytes.to_vec(),
            other => panic!("{} decoded as {other:?}", field.sig.as_str()),
        })
        .collect()
}

#[test]
fn dlc_master_record_is_renumbered_into_output_masters() {
    let interner = StringInterner::new();
    let fallout4 = plugin_handle_new_native(FALLOUT4, Some("fo4")).unwrap();
    let robot = plugin_handle_new_native(ROBOT, Some("fo4")).unwrap();
    plugin_handle_add_master_native(robot, FALLOUT4, None).unwrap();
    let nuka_world = plugin_handle_new_native(NUKA_WORLD, Some("fo4")).unwrap();
    plugin_handle_add_master_native(nuka_world, FALLOUT4, None).unwrap();
    let target = plugin_handle_new_native(OUTPUT, Some("fo4")).unwrap();
    for master in [FALLOUT4, ROBOT, NUKA_WORLD] {
        plugin_handle_add_master_native(target, master, None).unwrap();
    }

    // Nuka-World names its own forms as master 01, which the output calls DLCRobot.esm.
    let nuka_world_own = |local: u32| (1 << 24) | local;
    let output_nuka_world = |local: u32| (2 << 24) | local;
    let tpta = |traits: u32| {
        let mut slots = vec![0u8; TPTA_SLOTS * 4];
        slots[0..4].copy_from_slice(&traits.to_le_bytes());
        slots
    };
    let brahmiluff = FormKey {
        plugin: interner.intern(NUKA_WORLD),
        local: BRAHMILUFF,
    };
    {
        let mut session = open_session(nuka_world, None).unwrap();
        let schema = session.schema().unwrap();
        let mut record = Record::new(SigCode(*b"NPC_"), brahmiluff);
        record.fields.push(bytes_field(
            b"SNAM",
            faction(nuka_world_own(NUKA_WORLD_FACTION)),
        ));
        record
            .fields
            .push(bytes_field(b"SNAM", faction(FALLOUT4_FACTION)));
        record
            .fields
            .push(bytes_field(b"TPTA", tpta(nuka_world_own(TRAITS_LIST))));
        session.add_record(record, &schema, &interner).unwrap();
    }

    let mut session = open_session(target, None).unwrap();
    let schema = session.schema().unwrap();
    let record = record_decoded_in_master_for_target(
        &mut session,
        nuka_world,
        &brahmiluff,
        &schema,
        &interner,
    )
    .unwrap();
    assert_eq!(
        raw_fields(&record, b"SNAM"),
        vec![
            faction(output_nuka_world(NUKA_WORLD_FACTION)),
            faction(FALLOUT4_FACTION),
        ]
    );
    assert_eq!(
        raw_fields(&record, b"TPTA"),
        vec![tpta(output_nuka_world(TRAITS_LIST))]
    );

    drop(session);
    for handle in [target, nuka_world, robot, fallout4] {
        assert!(plugin_handle_close_native(handle));
    }
}
