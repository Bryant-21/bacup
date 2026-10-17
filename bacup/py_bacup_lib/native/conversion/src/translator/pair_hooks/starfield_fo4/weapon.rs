use smallvec::SmallVec;

#[path = "weapon_omod.rs"]
pub(crate) mod omod;

use super::STARFIELD_METERS_TO_FO4_UNITS;
use crate::fixups::synthesize_weap_data_blocks::{default_fnam, fo4_default_dnam};
use crate::formkey_mapper::FormKeyMapper;
use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::translator::pair_hooks::fnv_mgef::{MgefNormalizeReport, mapped_raw_reference};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub(crate) enum StarfieldFiringType {
    SingleOrBinary,
    Burst,
    Automatic,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct StarfieldWeaponComponents {
    pub weapon_type: u32,
    pub weight: f32,
    pub value: u32,
    pub speed: f32,
    pub attack_oxygen_cost: f32,
    pub barrel_raw: u32,
    pub ammo_raw: u32,
    pub capacity: u32,
    pub ammo_list_raw: u32,
    pub projectile_raw: u32,
    pub shell_casing_raw: u32,
    pub projectiles: u8,
    pub npcs_use_ammo: bool,
    pub impact_sounds: u8,
    pub damage: f32,
    pub min_range: f32,
    pub max_range: f32,
    pub out_of_range_damage_mult: f32,
    pub critical_damage_mult: f32,
    pub critical_spell_raw: u32,
    pub critical_on_death: bool,
    pub hit_behavior: u8,
    pub resistance_raw: u32,
    pub skill_raw: u32,
    pub gravity_range_controls: [f32; 6],
    pub critical_chance_mult: f32,
    pub firing_type: StarfieldFiringType,
    pub burst_count: u8,
    pub repeatable_fire: bool,
    pub attack_seconds: f32,
    pub fire_seconds: f32,
    pub attack_delay_seconds: f32,
    pub bolt_seconds: f32,
    pub bolt_action: bool,
    pub disable_shell_eject: bool,
    pub shots_per_second: f32,
    pub attack_animation_duration: f32,
    pub override_rate_of_fire: bool,
    pub primary_trigger_threshold: f32,
    pub override_trigger_threshold: bool,
    pub secondary_trigger_threshold: f32,
    pub staged_trigger: bool,
    pub dual_trigger: bool,
    pub burst_delay_seconds: f32,
    pub reload_seconds: f32,
    pub reload_speed: f32,
    pub charging_reload: bool,
    pub single_reload: bool,
    pub flags: [bool; 12],
}

fn payload<'a>(record: &'a Record, signature: &[u8; 4], size: usize) -> Result<&'a [u8], String> {
    let mut in_component = false;
    let mut found = None;
    for field in &record.fields {
        match &field.sig.0 {
            b"BFCB" => in_component = true,
            b"BFCE" => in_component = false,
            sig if sig == signature && !in_component => {
                if found.is_some() {
                    return Err(format!("duplicate top-level {}", field.sig.as_str()));
                }
                match &field.value {
                    FieldValue::Bytes(bytes) if bytes.len() == size => {
                        found = Some(bytes.as_slice())
                    }
                    _ => {
                        return Err(format!(
                            "Starfield {} requires {size} raw bytes",
                            field.sig.as_str()
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    found.ok_or_else(|| {
        format!(
            "missing top-level Starfield {}",
            String::from_utf8_lossy(signature)
        )
    })
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}
fn float(bytes: &[u8], offset: usize) -> Result<f32, String> {
    let value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    if value.is_finite() {
        Ok(value)
    } else {
        Err(format!("nonfinite Starfield weapon float at {offset}"))
    }
}
fn boolean(bytes: &[u8], offset: usize) -> Result<bool, String> {
    match bytes[offset] {
        0 => Ok(false),
        1 => Ok(true),
        value => Err(format!("invalid Starfield weapon bool {value} at {offset}")),
    }
}

pub(crate) fn decode_components(record: &Record) -> Result<StarfieldWeaponComponents, String> {
    if record.sig.0 != *b"WEAP" {
        return Err("Starfield weapon decoder requires WEAP".into());
    }
    let general = payload(record, b"WGEN", 24)?;
    let ammo = payload(record, b"WAM2", 23)?;
    let damage = payload(record, b"WDMG", 62)?;
    let firing = payload(record, b"WFIR", 45)?;
    let reload = payload(record, b"WRLO", 10)?;
    let source_flags = payload(record, b"WFLG", 12)?;
    let mut flags = [false; 12];
    for (index, flag) in flags.iter_mut().enumerate() {
        *flag = boolean(source_flags, index)?;
    }
    let mut gravity_range_controls = [0.; 6];
    for (index, value) in gravity_range_controls.iter_mut().enumerate() {
        *value = float(damage, 34 + 4 * index)?;
    }
    Ok(StarfieldWeaponComponents {
        weapon_type: u32_at(general, 0),
        weight: float(general, 4)?,
        value: u32_at(general, 8),
        speed: float(general, 12)?,
        attack_oxygen_cost: float(general, 16)?,
        barrel_raw: u32_at(general, 20),
        ammo_raw: u32_at(ammo, 0),
        capacity: u32_at(ammo, 4),
        ammo_list_raw: u32_at(ammo, 8),
        projectile_raw: u32_at(ammo, 12),
        shell_casing_raw: u32_at(ammo, 16),
        projectiles: ammo[20],
        npcs_use_ammo: boolean(ammo, 21)?,
        impact_sounds: ammo[22],
        damage: float(damage, 0)?,
        min_range: float(damage, 4)?,
        max_range: float(damage, 8)?,
        out_of_range_damage_mult: float(damage, 12)?,
        critical_damage_mult: float(damage, 16)?,
        critical_spell_raw: u32_at(damage, 20),
        critical_on_death: boolean(damage, 24)?,
        hit_behavior: damage[25],
        resistance_raw: u32_at(damage, 26),
        skill_raw: u32_at(damage, 30),
        gravity_range_controls,
        critical_chance_mult: float(damage, 58)?,
        firing_type: match firing[0] {
            0 => StarfieldFiringType::SingleOrBinary,
            1 => StarfieldFiringType::Burst,
            2 => StarfieldFiringType::Automatic,
            value => return Err(format!("unsupported Starfield firing type {value}")),
        },
        burst_count: firing[1],
        repeatable_fire: boolean(firing, 2)?,
        attack_seconds: float(firing, 3)?,
        fire_seconds: float(firing, 7)?,
        attack_delay_seconds: float(firing, 11)?,
        bolt_seconds: float(firing, 15)?,
        bolt_action: boolean(firing, 19)?,
        disable_shell_eject: boolean(firing, 20)?,
        shots_per_second: float(firing, 21)?,
        attack_animation_duration: float(firing, 25)?,
        override_rate_of_fire: boolean(firing, 29)?,
        primary_trigger_threshold: float(firing, 30)?,
        override_trigger_threshold: boolean(firing, 34)?,
        secondary_trigger_threshold: float(firing, 35)?,
        staged_trigger: boolean(firing, 39)?,
        dual_trigger: boolean(firing, 40)?,
        burst_delay_seconds: float(firing, 41)?,
        reload_seconds: float(reload, 0)?,
        reload_speed: float(reload, 4)?,
        charging_reload: boolean(reload, 8)?,
        single_reload: boolean(reload, 9)?,
        flags,
    })
}

fn put_u32(bytes: &mut [u8], offset: usize, value: u32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}
fn put_float(bytes: &mut [u8], offset: usize, value: f32) {
    bytes[offset..offset + 4].copy_from_slice(&value.to_le_bytes());
}

pub(crate) fn normalize(
    record: &mut Record,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<MgefNormalizeReport, String> {
    let mut report = MgefNormalizeReport::default();
    if record.sig.0 != *b"WEAP" {
        return Ok(report);
    }
    let source = decode_components(record)?;
    if source.weapon_type > 11
        || source.hit_behavior > 3
        || source.speed <= 0.
        || source.reload_speed <= 0.
    {
        return Err("Starfield weapon type, hit behavior or playback speed is unsupported".into());
    }
    let capacity =
        u16::try_from(source.capacity).map_err(|_| "Starfield ammo capacity exceeds FO4's u16")?;
    if source.damage < 0. || source.damage > u16::MAX as f32 || source.damage.fract() != 0. {
        return Err("Starfield base damage cannot be represented exactly by FO4's u16".into());
    }
    let mut reference = |name, raw| -> Result<u32, String> {
        let target = match mapped_raw_reference(name, raw, mapper, &mut report) {
            FieldValue::Bytes(mapped) => u32_at(&mapped, 0),
            FieldValue::Uint(0) => 0,
            _ => return Err(format!("invalid mapped weapon reference {name}")),
        };
        if raw != 0 && target == 0 {
            return Err(format!(
                "required Starfield weapon reference {name} {raw:08X} has no target mapping"
            ));
        }
        Ok(target)
    };
    let ammo = reference("ammo", source.ammo_raw)?;
    let projectile = reference("override_projectile", source.projectile_raw)?;
    let resistance = reference("resist", source.resistance_raw)?;
    let skill = reference("skill", source.skill_raw)?;
    let critical_spell = reference("critical_effect", source.critical_spell_raw)?;

    let mut flags = 0u32;
    // Source WFLG is twelve independent booleans, not FO4's bitmask.
    for (index, target) in [
        (0, 0x100000),
        (1, 1),
        (2, 64),
        (3, 0x800000),
        (4, 0x20000),
        (5, 16),
        (6, 0x1000),
        (7, 0x2000),
    ] {
        if source.flags[index] {
            flags |= target;
        }
    }
    for (set, target) in [
        (source.npcs_use_ammo, 2),
        (source.charging_reload, 8),
        (source.critical_on_death, 256),
        (source.firing_type == StarfieldFiringType::Automatic, 0x8000),
        (source.repeatable_fire, 0x10000),
        (source.bolt_action, 0x400000),
        (source.disable_shell_eject, 0x1000000),
    ] {
        if set {
            flags |= target;
        }
    }
    let mut dnam = fo4_default_dnam();
    put_u32(&mut dnam, 0, ammo);
    for (offset, value) in [
        (4, source.speed),
        (8, source.reload_speed),
        (16, source.min_range * STARFIELD_METERS_TO_FO4_UNITS),
        (20, source.max_range * STARFIELD_METERS_TO_FO4_UNITS),
        (24, source.attack_delay_seconds),
        (32, source.out_of_range_damage_mult),
        (59, source.weight),
        (106, source.attack_seconds),
    ] {
        put_float(&mut dnam, offset, value);
    }
    put_u32(&mut dnam, 36, source.hit_behavior as u32);
    put_u32(&mut dnam, 40, skill);
    put_u32(&mut dnam, 44, resistance);
    put_u32(&mut dnam, 48, flags);
    dnam[52..54].copy_from_slice(&capacity.to_le_bytes());
    dnam[54] = source.weapon_type as u8;
    put_u32(&mut dnam, 63, source.value);
    dnam[67..69].copy_from_slice(&(source.damage as u16).to_le_bytes());

    let mut fnam = default_fnam();
    put_float(&mut fnam, 0, source.fire_seconds);
    put_float(&mut fnam, 16, source.reload_seconds);
    put_float(&mut fnam, 20, source.bolt_seconds);
    fnam[28] = source.projectiles;
    put_u32(&mut fnam, 29, projectile);
    let mut critical = [0u8; 12];
    put_float(&mut critical, 0, source.critical_damage_mult);
    put_float(&mut critical, 4, source.critical_chance_mult);
    put_u32(&mut critical, 8, critical_spell);

    let mut in_component = false;
    record.fields.retain(|field| match &field.sig.0 {
        b"BFCB" => {
            in_component = true;
            true
        }
        b"BFCE" => {
            in_component = false;
            true
        }
        b"WGEN" | b"WAM2" | b"WDMG" | b"WFIR" | b"WRLO" | b"WFLG" | b"WCHG" | b"WAIM" | b"WAUD"
        | b"WTUR" | b"WMEL" | b"QNAM" | b"WVAR" | b"WVIS" | b"WTRM" | b"DNAM" | b"FNAM"
        | b"CRDT" => in_component,
        _ => true,
    });
    for (sig, bytes) in [
        (*b"DNAM", dnam.as_slice()),
        (*b"FNAM", fnam.as_slice()),
        (*b"CRDT", critical.as_slice()),
    ] {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(sig),
            value: FieldValue::Bytes(SmallVec::from_slice(bytes)),
        });
    }
    // This rebuilds the stat payload only. Graph-dependent firing modes, modular
    // assembly, gravity controls, oxygen/AP, aim and WWise require other stages.
    report.converted_rows = 3;
    report.unsupported_rows = 1;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::{FormKey, SigCode};
    use crate::schema::AuthoringSchema;
    use crate::sym::StringInterner;
    use crate::target_normalize::{TargetRecordNormalization, TargetRecordNormalizer};
    use crate::translator::pair_hook::PairCtx;
    use crate::translator::{Game, TranslateResult, Translator};
    use bytes::Bytes;
    use esp_authoring_core::plugin_runtime::{ParsedRecord, ParsedSubrecord};
    use smol_str::SmolStr;

    fn key(interner: &StringInterner, plugin: &str, local: u32) -> FormKey {
        FormKey {
            local,
            plugin: interner.intern(plugin),
        }
    }
    fn eon(interner: &StringInterner) -> Record {
        let mut record = Record::new(SigCode(*b"WEAP"), key(interner, "Starfield.esm", 0x476C4));
        for (sig, bytes) in [
            (*b"WGEN", "09000000CDCC0C3FEE0200000000803F0000604226CD2500"),
            (*b"WAM2", "3EAD04000C00000043E800008A21060000000000010001"),
            (
                *b"WDMG",
                "000020410000A041000034420000003F000000400000000001000000000000000000CDCC4C3E0000803F0000C03F1F850B400000003F0000003F0000803F",
            ),
            (
                *b"WFIR",
                "0000000000803F00000034CDCC4C3E0000000000000000A040CDCC4C3E00C3F5A83E00C3F5A83E00009A99993E",
            ),
            (*b"WRLO", "010040400000803F0000"),
            (*b"WFLG", "000000000000000000000000"),
        ] {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(sig),
                value: FieldValue::Bytes(SmallVec::from_vec(hex::decode(bytes).unwrap())),
            });
        }
        record
    }

    #[test]
    fn shipped_eon_split_components_have_source_stats() {
        let interner = StringInterner::new();
        let source = decode_components(&eon(&interner)).unwrap();
        assert_eq!(source.weapon_type, 9);
        assert_eq!(source.capacity, 12);
        assert_eq!(source.ammo_raw, 0x4AD3E);
        assert_eq!(source.projectile_raw, 0x6218A);
        assert_eq!(source.damage, 10.);
        assert_eq!(source.min_range, 20.);
        assert_eq!(source.max_range, 45.);
        assert_eq!(source.firing_type, StarfieldFiringType::SingleOrBinary);
        assert_eq!(source.shots_per_second, 5.);
        assert!((source.reload_seconds - 3.).abs() < 0.00001);
    }

    #[test]
    fn component_wfir_does_not_shadow_weapon_firing_payload() {
        let interner = StringInterner::new();
        let mut record = eon(&interner);
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig(*b"BFCE"),
                value: FieldValue::Bytes(SmallVec::new()),
            },
        );
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig(*b"WFIR"),
                value: FieldValue::Bytes(SmallVec::from_slice(&[0; 4])),
            },
        );
        record.fields.insert(
            0,
            FieldEntry {
                sig: SubrecordSig(*b"BFCB"),
                value: FieldValue::String(interner.intern("BGSWeaponFireEffect_Component")),
            },
        );
        assert_eq!(decode_components(&record).unwrap().shots_per_second, 5.);
        record.fields.pop();
        assert!(decode_components(&record).is_err());
    }

    #[test]
    fn source_refs_and_meters_are_rebuilt_in_target_layout() {
        let interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                output_plugin_name: "Converted.esm".into(),
                source_plugin_name: "Starfield.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            &interner,
        );
        mapper.add_mapping(
            key(&interner, "Starfield.esm", 0x4AD3E),
            key(&interner, "Converted.esm", 0x800),
        );
        mapper.add_mapping(
            key(&interner, "Starfield.esm", 0x6218A),
            key(&interner, "Converted.esm", 0x801),
        );
        let mut record = eon(&interner);
        let report = normalize(&mut record, &mut mapper).unwrap();
        assert_eq!(report.converted_rows, 3);
        assert!(report.unsupported_rows > 0);
        let dnam = payload(&record, b"DNAM", 132).unwrap();
        assert_eq!(u32_at(dnam, 0), 0x01000800);
        assert_eq!(
            float(dnam, 16).unwrap(),
            20. * STARFIELD_METERS_TO_FO4_UNITS
        );
        assert_eq!(
            float(dnam, 20).unwrap(),
            45. * STARFIELD_METERS_TO_FO4_UNITS
        );
        assert_eq!(u16::from_le_bytes(dnam[52..54].try_into().unwrap()), 12);
        assert_eq!(u16::from_le_bytes(dnam[67..69].try_into().unwrap()), 10);
        assert_eq!(
            u32_at(payload(&record, b"FNAM", 41).unwrap(), 29),
            0x01000801
        );
        assert!(record.fields.iter().all(|f| f.sig.0 != *b"WGEN"));
    }

    #[test]
    fn unresolved_ammo_or_malformed_required_payload_is_fatal() {
        let interner = StringInterner::new();
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                source_plugin_name: "Starfield.esm".into(),
                ..MapperOptions::default()
            },
            &interner,
        );
        let mut record = eon(&interner);
        let before = record.fields.clone();
        assert!(
            normalize(&mut record, &mut mapper)
                .unwrap_err()
                .contains("ammo")
        );
        assert_eq!(record.fields, before);
        if let FieldValue::Bytes(bytes) = &mut record.fields[3].value {
            bytes.pop();
        }
        assert!(decode_components(&record).is_err());
    }

    #[test]
    fn raw_split_fields_survive_source_hook_translation_and_serial_boundary() {
        let interner = StringInterner::new();
        let source_schema = AuthoringSchema::for_game("starfield").unwrap();
        let target_schema = AuthoringSchema::for_game("fo4").unwrap();
        let fixture = eon(&interner);
        let subrecord = |signature: &str, data: Vec<u8>| ParsedSubrecord {
            signature: SmolStr::new(signature),
            data: Bytes::from(data),
            semantic_type: None,
        };
        let mut subrecords = vec![
            subrecord("EDID", b"Eon\0".to_vec()),
            subrecord("BFCB", b"BGSWeaponFireEffect_Component\0".to_vec()),
            subrecord("WFIR", vec![0; 4]),
            subrecord("BFCE", vec![]),
        ];
        subrecords.extend(fixture.fields.iter().map(|field| {
            let FieldValue::Bytes(bytes) = &field.value else {
                panic!("raw fixture")
            };
            subrecord(field.sig.as_str(), bytes.to_vec())
        }));
        let raw = ParsedRecord {
            signature: SmolStr::new_static("WEAP"),
            form_id: 0x476C4,
            flags: 0,
            version_control: 0,
            form_version: Some(581),
            version2: Some(7),
            subrecords,
            raw_payload: None,
            parse_error: None,
        };
        let mut source = crate::source_read::decode_record_from_parsed(
            &raw,
            &fixture.form_key,
            &source_schema,
            &[],
            "Starfield.esm",
            None,
            false,
            &interner,
        )
        .unwrap();
        let translator = Translator::new(Game::Starfield, Game::Fo4).unwrap();
        let mut ctx = PairCtx::with_source(&interner, "Starfield.esm", &[]);
        translator.pre_translate(&mut ctx, &mut source).unwrap();
        assert!(source.fields.iter().all(|field| field.sig.0 != *b"BFCB"));
        let TranslateResult::Translated(mut target) = translator.translate(&source, &interner)
        else {
            panic!("WEAP should translate")
        };
        let preserved = decode_components(&target).unwrap();
        assert_eq!(preserved.capacity, 12);
        assert_eq!(preserved.shots_per_second, 5.);
        let mut mapper = FormKeyMapper::new(
            std::iter::empty(),
            MapperOptions {
                source_plugin_name: "Starfield.esm".into(),
                output_plugin_name: "Converted.esm".into(),
                target_master_names: vec!["Fallout4.esm".into()],
                ..MapperOptions::default()
            },
            &interner,
        );
        for (source_id, target_id) in [(0x476C4, 0x802), (0x4AD3E, 0x800), (0x6218A, 0x801)] {
            mapper.add_mapping(
                key(&interner, "Starfield.esm", source_id),
                key(&interner, "Converted.esm", target_id),
            );
        }
        mapper.rewrite_record(&mut target).unwrap();
        translator.post_translate(&mut ctx, &mut target).unwrap();
        let mut state =
            crate::translator::pair_hooks::fnv_fo4::LegacySerialNormalizationState::default();
        translator
            .normalize_serial_mapper_record_once(
                fixture.form_key,
                &mut target,
                &mut mapper,
                &mut state,
            )
            .expect("serial weapon handler")
            .expect("target payload rebuilt");
        assert_eq!(
            u32_at(payload(&target, b"DNAM", 132).unwrap(), 0),
            0x01000800
        );
        let normalizer = TargetRecordNormalizer {
            target_schema: &target_schema,
            source_record_def: source_schema.record_def("WEAP"),
            interner: Some(&interner),
        };
        let TargetRecordNormalization::Keep(written) = normalizer.normalize(target) else {
            panic!("FO4 keeps WEAP")
        };
        assert_eq!(payload(&written, b"DNAM", 132).unwrap()[54], 9);
        assert_eq!(
            u32_at(payload(&written, b"FNAM", 41).unwrap(), 29),
            0x01000801
        );
        assert!(written.fields.iter().all(|field| !matches!(
            &field.sig.0,
            b"WGEN" | b"WAM2" | b"WFIR" | b"WRLO" | b"WFLG" | b"WDMG"
        )));
    }
}
