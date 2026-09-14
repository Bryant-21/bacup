use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::target_fo4_melee::{Fo4MeleeProfile, Fo4MeleeSourcePayload, emit_fo4_melee_weapon};
use smallvec::SmallVec;

const STEEL_BATTLEAXE_LOCAL: u32 = 0x013984;
const STEEL_BATTLEAXE_FIRST_PERSON_STAT_LOCAL: u32 = 0x020E27;
const SKYRIM_MASTER: &str = "Skyrim.esm";
const STEEL_BATTLEAXE_MODEL: &str = "Weapons\\Steel\\SteelBattleAxe.nif";
const STEEL_BATTLEAXE_FIRST_PERSON_MODEL: &str = "Weapons\\Steel\\1stPersonSteelBattleAxe.nif";
const SKYRIM_BOTH_HANDS_EQUIP_LOCAL: u32 = 0x013F45;

const FO4_BLOCK_BLUNT_MEDIUM_IMPACT_LOCAL: u32 = 0x0D398C;
const FO4_BLOCK_BLUNT_MEDIUM_MATERIAL_LOCAL: u32 = 0x0774C1;
const FO4_TWO_HAND_WIDE_ANIMS_KEYWORD_LOCAL: u32 = 0x0EB4A2;
const FO4_TWO_HAND_MELEE_KEYWORD_LOCAL: u32 = 0x04A0A5;
const FO4_MELEE_QUICKKEY_KEYWORD_LOCAL: u32 = 0x10C89B;
const FO4_AXE_SWING_SOUND_LOCAL: u32 = 0x094307;
const FO4_AXE_IDLE_SOUND_LOCAL: u32 = 0x24989F;
const FO4_MELEE_EQUIP_SOUND_LOCAL: u32 = 0x0C06CF;
const FO4_MELEE_UNEQUIP_SOUND_LOCAL: u32 = 0x1526AC;
const FO4_GROGNAK_AXE_IMPACT_LOCAL: u32 = 0x2313F3;

const FO4_TWO_HAND_MELEE_KEYWORDS: &[u32] = &[
    FO4_TWO_HAND_WIDE_ANIMS_KEYWORD_LOCAL,
    FO4_TWO_HAND_MELEE_KEYWORD_LOCAL,
    FO4_MELEE_QUICKKEY_KEYWORD_LOCAL,
];

const STEEL_BATTLEAXE_PROFILE: Fo4MeleeProfile = Fo4MeleeProfile {
    block_impact: FO4_BLOCK_BLUNT_MEDIUM_IMPACT_LOCAL,
    block_material: FO4_BLOCK_BLUNT_MEDIUM_MATERIAL_LOCAL,
    keywords: FO4_TWO_HAND_MELEE_KEYWORDS,
    swing_sound: FO4_AXE_SWING_SOUND_LOCAL,
    idle_sound: Some(FO4_AXE_IDLE_SOUND_LOCAL),
    equip_sound: FO4_MELEE_EQUIP_SOUND_LOCAL,
    unequip_sound: FO4_MELEE_UNEQUIP_SOUND_LOCAL,
    impact: FO4_GROGNAK_AXE_IMPACT_LOCAL,
    animation_type: 5,
    flags: 16 | 256,
    animation_attack_seconds: 1.764_111_9,
    action_point_cost: 20.0,
    stagger: 2,
    trailing_unknowns: [255, 255, 127, 127],
};

const SUPPORTED_SOURCE_FIELDS: &[[u8; 4]] = &[
    *b"EDID", *b"OBND", *b"FULL", *b"MODL", *b"MODT", *b"ETYP", *b"BIDS", *b"BAMT", *b"KSIZ",
    *b"KWDA", *b"DESC", *b"INAM", *b"WNAM", *b"TNAM", *b"NAM9", *b"NAM8", *b"DATA", *b"DNAM",
    *b"CRDT", *b"VNAM",
];

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct SimpleMeleeWeaponSource {
    pub value: u32,
    pub weight: f32,
    pub damage: u16,
    pub first_person_model: &'static str,
}

pub(crate) fn simple_melee_weapon_source(
    record: &Record,
    interner: &StringInterner,
) -> Option<SimpleMeleeWeaponSource> {
    if record.sig.as_str() != "WEAP"
        || record.form_key.local != STEEL_BATTLEAXE_LOCAL
        || interner
            .resolve(record.form_key.plugin)
            .is_none_or(|plugin| !plugin.eq_ignore_ascii_case(SKYRIM_MASTER))
        || record.flags.bits() != 0
        || record
            .eid
            .and_then(|eid| interner.resolve(eid))
            .is_none_or(|eid| !eid.eq_ignore_ascii_case("SteelBattleaxe"))
        || record
            .fields
            .iter()
            .any(|field| !SUPPORTED_SOURCE_FIELDS.contains(&field.sig.0))
        || record
            .fields
            .iter()
            .any(|field| matches!(&field.sig.0, b"VMAD" | b"EITM" | b"EAMT" | b"CNAM"))
    {
        return None;
    }

    let model = exact_field(record, "MODL")?;
    if !string_value(model, interner).is_some_and(|path| path_eq(path, STEEL_BATTLEAXE_MODEL)) {
        return None;
    }
    let equipment_type = form_key_value(exact_field(record, "ETYP")?)?;
    if equipment_type.local != SKYRIM_BOTH_HANDS_EQUIP_LOCAL
        || equipment_type.plugin != record.form_key.plugin
    {
        return None;
    }
    let first_person_stat = form_key_value(exact_field(record, "WNAM")?)?;
    let first_person_model = audited_first_person_model(record.form_key, first_person_stat)?;
    if !is_two_hand_axe(exact_field(record, "DNAM")?, interner) {
        return None;
    }

    let (value, weight, damage) = source_game_data(exact_field(record, "DATA")?, interner)?;
    if !weight.is_finite() || weight < 0.0 || damage == 0 {
        return None;
    }

    Some(SimpleMeleeWeaponSource {
        value,
        weight,
        damage,
        first_person_model,
    })
}

pub(crate) fn project_simple_melee_weapon(record: &mut Record, interner: &StringInterner) -> bool {
    let Some(source) = simple_melee_weapon_source(record, interner) else {
        return false;
    };
    let Some(edid) = clone_field(record, "EDID") else {
        return false;
    };
    let Some(bounds) = clone_field(record, "OBND") else {
        return false;
    };
    let Some(name) = clone_field(record, "FULL") else {
        return false;
    };
    let Some(model) = clone_field(record, "MODL") else {
        return false;
    };
    emit_fo4_melee_weapon(
        record,
        Fo4MeleeSourcePayload {
            editor_id: edid,
            bounds,
            display_name: name,
            world_model: model,
            first_person_model: interner.intern(source.first_person_model),
            value: source.value,
            weight: source.weight,
            damage: source.damage,
        },
        STEEL_BATTLEAXE_PROFILE,
        interner,
    );
    true
}

fn audited_first_person_model(weapon: FormKey, first_person_stat: FormKey) -> Option<&'static str> {
    (first_person_stat.plugin == weapon.plugin
        && first_person_stat.local == STEEL_BATTLEAXE_FIRST_PERSON_STAT_LOCAL)
        .then_some(STEEL_BATTLEAXE_FIRST_PERSON_MODEL)
}

fn source_game_data(value: &FieldValue, interner: &StringInterner) -> Option<(u32, f32, u16)> {
    match value {
        FieldValue::Bytes(bytes) if bytes.len() == 10 => Some((
            u32::from_le_bytes(bytes[0..4].try_into().ok()?),
            f32::from_le_bytes(bytes[4..8].try_into().ok()?),
            u16::from_le_bytes(bytes[8..10].try_into().ok()?),
        )),
        FieldValue::Struct(fields) => Some((
            named_u32(fields, "value", interner)?,
            named_f32(fields, "weight", interner)?,
            named_u16(fields, "damage", interner)?,
        )),
        _ => None,
    }
}

fn is_two_hand_axe(value: &FieldValue, interner: &StringInterner) -> bool {
    match value {
        FieldValue::Bytes(bytes) => bytes.first() == Some(&6),
        FieldValue::Struct(fields) => fields.iter().any(|(key, value)| {
            canonical_key(interner.resolve(*key).unwrap_or_default()) == "animationtype"
                && match value {
                    FieldValue::Uint(6) | FieldValue::Int(6) => true,
                    FieldValue::String(value) => interner
                        .resolve(*value)
                        .is_some_and(|value| value.eq_ignore_ascii_case("TwoHandAxe")),
                    _ => false,
                }
        }),
        _ => false,
    }
}

fn exact_field<'a>(record: &'a Record, signature: &str) -> Option<&'a FieldValue> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let value = &fields.next()?.value;
    fields.next().is_none().then_some(value)
}

fn clone_field(record: &Record, signature: &str) -> Option<FieldEntry> {
    let mut fields = record
        .fields
        .iter()
        .filter(|field| field.sig.as_str() == signature);
    let value = fields.next()?.clone();
    fields.next().is_none().then_some(value)
}

fn form_key_value(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(form_key) => Some(*form_key),
        _ => None,
    }
}

fn string_value<'a>(value: &FieldValue, interner: &'a StringInterner) -> Option<&'a str> {
    match value {
        FieldValue::String(value) => interner.resolve(*value),
        _ => None,
    }
}

fn named_u32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u32> {
    let value = named_value(fields, name, interner)?;
    match value {
        FieldValue::Uint(value) => u32::try_from(*value).ok(),
        FieldValue::Int(value) => u32::try_from(*value).ok(),
        _ => None,
    }
}

fn named_u16(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<u16> {
    named_u32(fields, name, interner).and_then(|value| u16::try_from(value).ok())
}

fn named_f32(
    fields: &[(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<f32> {
    match named_value(fields, name, interner)? {
        FieldValue::Float(value) => Some(*value),
        _ => None,
    }
}

fn named_value<'a>(
    fields: &'a [(crate::sym::Sym, FieldValue)],
    name: &str,
    interner: &StringInterner,
) -> Option<&'a FieldValue> {
    let name = canonical_key(name);
    fields.iter().find_map(|(key, value)| {
        (canonical_key(interner.resolve(*key).unwrap_or_default()) == name).then_some(value)
    })
}

fn canonical_key(value: &str) -> String {
    value
        .chars()
        .filter(|character| character.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn path_eq(left: &str, right: &str) -> bool {
    left.replace('/', "\\").eq_ignore_ascii_case(right)
}

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("valid subrecord signature"),
        value,
    }
}

fn form_key_field(signature: &str, plugin: crate::sym::Sym, local: u32) -> FieldEntry {
    field(signature, FieldValue::FormKey(FormKey { local, plugin }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SigCode;
    use crate::record::RecordFlags;
    use crate::schema::AuthoringSchema;
    use crate::target_fo4_melee::{
        FO4_RIGHT_HAND_EQUIP_LOCAL, audit_fo4_melee_dependencies, fo4_melee_dependencies,
        projection_snapshot,
    };
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};

    fn source_weapon(interner: &StringInterner) -> Record {
        let plugin = interner.intern("Skyrim.esm");
        let mut record = Record::new(
            SigCode::from_str("WEAP").unwrap(),
            FormKey {
                local: STEEL_BATTLEAXE_LOCAL,
                plugin,
            },
        );
        record.eid = Some(interner.intern("SteelBattleaxe"));
        record.fields = SmallVec::from_vec(vec![
            field(
                "EDID",
                FieldValue::String(interner.intern("SteelBattleaxe")),
            ),
            field("OBND", FieldValue::Bytes(SmallVec::from_slice(&[0; 12]))),
            field(
                "FULL",
                FieldValue::String(interner.intern("Steel Battleaxe")),
            ),
            field(
                "MODL",
                FieldValue::String(interner.intern(STEEL_BATTLEAXE_MODEL)),
            ),
            form_key_field("ETYP", plugin, SKYRIM_BOTH_HANDS_EQUIP_LOCAL),
            form_key_field("BIDS", plugin, 0x0193C7),
            form_key_field("BAMT", plugin, 0x0E64A9),
            field("KSIZ", FieldValue::Uint(3)),
            field("KWDA", FieldValue::List(Vec::new())),
            field("DESC", FieldValue::String(interner.intern(""))),
            form_key_field("INAM", plugin, 0x0193B8),
            form_key_field("WNAM", plugin, STEEL_BATTLEAXE_FIRST_PERSON_STAT_LOCAL),
            form_key_field("TNAM", plugin, 0x0E4CAC),
            form_key_field("NAM9", plugin, 0x06036A),
            form_key_field("NAM8", plugin, 0x0605D3),
            field(
                "DATA",
                FieldValue::Bytes(SmallVec::from_slice(&[100, 0, 0, 0, 0, 0, 168, 65, 18, 0])),
            ),
            field(
                "DNAM",
                FieldValue::Bytes(SmallVec::from_slice(&{
                    let mut bytes = [0_u8; 100];
                    bytes[0] = 6;
                    bytes[4..8].copy_from_slice(&0.7_f32.to_le_bytes());
                    bytes[8..12].copy_from_slice(&1.3_f32.to_le_bytes());
                    bytes
                })),
            ),
            field("CRDT", FieldValue::Bytes(SmallVec::from_slice(&[0; 24]))),
            field("VNAM", FieldValue::Uint(0)),
        ]);
        record
    }

    #[test]
    fn steel_battleaxe_projects_to_target_native_melee_contract() {
        let interner = StringInterner::new();
        let mut record = source_weapon(&interner);

        assert!(project_simple_melee_weapon(&mut record, &interner));

        assert_eq!(record.form_key.local, STEEL_BATTLEAXE_LOCAL);
        assert_eq!(
            record.eid.and_then(|eid| interner.resolve(eid)),
            Some("SteelBattleaxe")
        );
        assert_eq!(
            string_value(exact_field(&record, "MODL").unwrap(), &interner),
            Some(STEEL_BATTLEAXE_MODEL)
        );
        assert_eq!(
            string_value(exact_field(&record, "MOD4").unwrap(), &interner),
            Some(STEEL_BATTLEAXE_FIRST_PERSON_MODEL)
        );
        assert_eq!(
            form_key_value(exact_field(&record, "ETYP").unwrap())
                .unwrap()
                .local,
            FO4_RIGHT_HAND_EQUIP_LOCAL
        );
        assert!(
            !record
                .fields
                .iter()
                .any(|field| { matches!(&field.sig.0, b"WNAM" | b"EITM" | b"VMAD" | b"CNAM") })
        );

        let dnam = exact_field(&record, "DNAM").unwrap();
        let FieldValue::Struct(dnam) = dnam else {
            panic!("target DNAM must be structured");
        };
        assert_eq!(named_u32(dnam, "animation_type", &interner), Some(5));
        assert_eq!(named_u32(dnam, "value", &interner), Some(100));
        assert_eq!(named_u32(dnam, "damage_base", &interner), Some(18));
        assert_eq!(named_f32(dnam, "weight", &interner), Some(21.0));

        let mut emitted = Vec::new();
        for field in &record.fields {
            collect_form_keys(&field.value, &mut emitted);
        }
        let fallout4 = interner.intern("Fallout4.esm");
        assert!(emitted.iter().all(|form_key| form_key.plugin == fallout4));
        assert!(audit_fo4_melee_dependencies(
            &record,
            STEEL_BATTLEAXE_PROFILE,
            &interner
        ));
        for dependency in fo4_melee_dependencies(STEEL_BATTLEAXE_PROFILE) {
            assert!(
                emitted
                    .iter()
                    .any(|form_key| form_key.local == dependency.local),
                "missing {} {:06X}",
                dependency.signature,
                dependency.local
            );
        }
        assert_eq!(
            projection_snapshot(&record, &interner),
            r#"EDID=s:SteelBattleaxe|OBND=b:000000000000000000000000|FULL=s:Steel Battleaxe|MODL=s:Weapons\Steel\SteelBattleAxe.nif|ETYP=r:013F42@Fallout4.esm|BIDS=r:0D398C@Fallout4.esm|BAMT=r:0774C1@Fallout4.esm|KSIZ=u:3|KWDA=[r:0EB4A2@Fallout4.esm,r:04A0A5@Fallout4.esm,r:10C89B@Fallout4.esm]|MOD4=s:Weapons\Steel\1stPersonSteelBattleAxe.nif|MO4T=b:0400000000000000000000000000000000000000|DNAM={ammo=u:0,speed=f:3F800000,reload_speed=f:3F800000,reach=f:3F4CCCCD,min_range=f:00000000,max_range=f:41200000,attack_delay=f:00000000,unused=f:00000000,damage_outofrange_mult=f:BF800000,on_hit=u:1,skill=u:0,resist=u:0,flags=u:272,capacity=u:0,animation_type=u:5,damage_secondary=f:00000000,weight=f:41A80000,value=u:100,damage_base=u:18,sound_level=u:4,sound_attack=r:094307@Fallout4.esm,sound_attack_2d=u:0,sound_attack_loop=u:0,sound_attack_fail=u:0,sound_idle=r:24989F@Fallout4.esm,sound_equip_sound=r:0C06CF@Fallout4.esm,sound_unequip_sound=r:1526AC@Fallout4.esm,sound_fast_equip_sound=u:0,accuracy_bonus=u:0,animation_attack_seconds=f:3FE1CE6B,unknown_u8_30=u:0,unknown_u8_31=u:0,action_point_cost=f:41A00000,full_power_seconds=f:00000000,min_power_per_shot=f:00000000,stagger=u:2,unknown_u8_36=u:255,unknown_u8_37=u:255,unknown_u8_38=u:127,unknown_u8_39=u:127}|CRDT={crit_damage_mult=f:40000000,crit_charge_bonus=f:3F800000,crit_effect=u:0}|INAM=r:2313F3@Fallout4.esm"#
        );
    }

    #[test]
    fn projection_normalizes_to_complete_fo4_struct_widths() {
        let interner = StringInterner::new();
        let mut record = source_weapon(&interner);
        assert!(project_simple_melee_weapon(&mut record, &interner));

        let source_schema = AuthoringSchema::for_game("skyrimse").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("WEAP"),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(record) = normalizer.normalize(record) else {
            panic!("projected WEAP must remain target-supported");
        };
        assert_eq!(
            structured_encoded_width(exact_field(&record, "DNAM").unwrap()),
            132
        );
        assert_eq!(
            structured_encoded_width(exact_field(&record, "CRDT").unwrap()),
            12
        );
        assert_eq!(
            record
                .fields
                .iter()
                .filter(|field| field.sig.as_str() == "MOD4")
                .count(),
            1
        );
    }

    #[test]
    fn unsupported_weapon_variants_fail_closed() {
        let interner = StringInterner::new();

        let mut ranged = source_weapon(&interner);
        let FieldValue::Bytes(bytes) = &mut ranged
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "DNAM")
            .unwrap()
            .value
        else {
            panic!("fixture DNAM");
        };
        bytes[0] = 7;
        assert!(simple_melee_weapon_source(&ranged, &interner).is_none());

        let mut enchanted = source_weapon(&interner);
        enchanted
            .fields
            .push(form_key_field("EITM", enchanted.form_key.plugin, 0x000800));
        assert!(simple_melee_weapon_source(&enchanted, &interner).is_none());

        let mut scripted = source_weapon(&interner);
        scripted
            .fields
            .push(field("VMAD", FieldValue::Bytes(SmallVec::new())));
        assert!(simple_melee_weapon_source(&scripted, &interner).is_none());

        let mut templated = source_weapon(&interner);
        templated.fields.push(form_key_field(
            "CNAM",
            templated.form_key.plugin,
            STEEL_BATTLEAXE_LOCAL,
        ));
        assert!(simple_melee_weapon_source(&templated, &interner).is_none());

        let mut unrelated = source_weapon(&interner);
        unrelated.form_key.local += 1;
        assert!(simple_melee_weapon_source(&unrelated, &interner).is_none());

        let mut unresolved_first_person = source_weapon(&interner);
        let wnam = unresolved_first_person
            .fields
            .iter_mut()
            .find(|field| field.sig.as_str() == "WNAM")
            .unwrap();
        wnam.value = FieldValue::None;
        assert!(simple_melee_weapon_source(&unresolved_first_person, &interner).is_none());
    }

    #[test]
    fn exact_clone_in_another_source_plugin_fails_closed() {
        let interner = StringInterner::new();
        let mut clone = source_weapon(&interner);
        let wrong_owner = interner.intern("SteelWeapons.esm");
        clone.form_key.plugin = wrong_owner;
        for field in &mut clone.fields {
            if let FieldValue::FormKey(form_key) = &mut field.value {
                form_key.plugin = wrong_owner;
            }
        }

        assert!(simple_melee_weapon_source(&clone, &interner).is_none());
    }

    #[test]
    fn skyrim_master_owner_match_is_case_insensitive() {
        let interner = StringInterner::new();
        let mut clone = source_weapon(&interner);
        let owner = interner.intern("SKYRIM.ESM");
        clone.form_key.plugin = owner;
        for field in &mut clone.fields {
            if let FieldValue::FormKey(form_key) = &mut field.value {
                form_key.plugin = owner;
            }
        }

        assert!(simple_melee_weapon_source(&clone, &interner).is_some());
    }

    #[test]
    fn optional_real_corpus_steel_battleaxe_projection() {
        let Some(path) = std::env::var_os("SKYRIMSE_WEAPON_CORPUS_PLUGIN") else {
            return;
        };
        let path = std::path::PathBuf::from(path);
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
        let plugin_name = crate::source_read::plugin_name_for_handle(handle).unwrap();
        let interner = StringInterner::new();
        let schema = AuthoringSchema::for_game("skyrimse").unwrap();
        let source = crate::source_read::read_record(
            handle,
            &format!("{STEEL_BATTLEAXE_LOCAL:06X}@{plugin_name}"),
            &schema,
            &interner,
        )
        .unwrap();
        assert!(simple_melee_weapon_source(&source, &interner).is_some());

        let first_person = crate::source_read::read_record(
            handle,
            &format!("{STEEL_BATTLEAXE_FIRST_PERSON_STAT_LOCAL:06X}@{plugin_name}"),
            &schema,
            &interner,
        )
        .unwrap();
        assert_eq!(first_person.sig.as_str(), "STAT");
        assert!(
            string_value(exact_field(&first_person, "MODL").unwrap(), &interner)
                .is_some_and(|path| path_eq(path, STEEL_BATTLEAXE_FIRST_PERSON_MODEL))
        );

        let mut projected = source;
        assert!(project_simple_melee_weapon(&mut projected, &interner));
        assert_eq!(
            string_value(exact_field(&projected, "MOD4").unwrap(), &interner),
            Some(STEEL_BATTLEAXE_FIRST_PERSON_MODEL)
        );
        esp_authoring_core::plugin_runtime::plugin_handle_close_native(handle);
    }

    fn structured_encoded_width(value: &FieldValue) -> usize {
        let FieldValue::Struct(fields) = value else {
            return 0;
        };
        fields
            .iter()
            .map(|(_, value)| match value {
                FieldValue::Bytes(bytes) => bytes.len(),
                FieldValue::FormKey(_) => 4,
                _ => 0,
            })
            .sum()
    }

    fn collect_form_keys(value: &FieldValue, output: &mut Vec<FormKey>) {
        match value {
            FieldValue::FormKey(form_key) if form_key.local != 0 => output.push(*form_key),
            FieldValue::List(values) => {
                for value in values {
                    collect_form_keys(value, output);
                }
            }
            FieldValue::Struct(fields) => {
                for (_, value) in fields {
                    collect_form_keys(value, output);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn fixture_has_no_record_flags() {
        let interner = StringInterner::new();
        assert_eq!(source_weapon(&interner).flags, RecordFlags::empty());
    }
}
