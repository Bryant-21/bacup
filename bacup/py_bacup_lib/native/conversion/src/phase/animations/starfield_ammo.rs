use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceBallisticField {
    pub signature: String,
    pub bytes: Vec<u8>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceAmmoContract {
    pub value: u32,
    pub weight: f32,
    pub projectile_raw: u32,
    pub flags: u8,
    pub damage: f32,
    pub health: u32,
    pub casing_model: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SourceProjectileContract {
    pub flags: u32,
    pub gravity: f32,
    pub speed: f32,
    pub range: f32,
    pub light_raw: u32,
    pub muzzle_light_raw: u32,
    pub explosion_proximity: f32,
    pub explosion_timer: f32,
    pub explosion_raw: u32,
    pub muzzle_duration: f32,
    pub fade_duration: f32,
    pub impact_force: f32,
    pub weapon_raw: u32,
    pub cone_spread: f32,
    pub collision_radius: f32,
    pub lifetime: f32,
    pub relaunch_interval: f32,
    pub decal_raw: u32,
    pub collision_layer_raw: u32,
    pub tracer_frequency: u8,
    pub vats_projectile_raw: u32,
    pub muzzle_particle_define: String,
    pub seek_strength: f32,
    pub sound_activation_condition: u32,
    pub projectile_type: u8,
    pub particle_define: String,
    pub tracer_speed: f32,
    pub zero_g_recoil_strength: f32,
    pub zero_g_recoil_length: f32,
    pub explosion_removal_delay: f32,
}

fn top_level<'a>(
    fields: &'a [SourceBallisticField],
    signature: &str,
) -> Result<Option<&'a [u8]>, String> {
    let mut component = false;
    let mut found = None;
    for field in fields {
        match field.signature.as_str() {
            "BFCB" if !component => component = true,
            "BFCB" => return Err("nested Starfield base-form component".into()),
            "BFCE" if component => component = false,
            "BFCE" => return Err("unmatched Starfield base-form component end".into()),
            sig if sig == signature && !component => {
                if found.replace(field.bytes.as_slice()).is_some() {
                    return Err(format!("duplicate top-level Starfield {signature}"));
                }
            }
            _ => {}
        }
    }
    if component {
        return Err("unterminated Starfield base-form component".into());
    }
    Ok(found)
}

fn required<'a>(fields: &'a [SourceBallisticField], signature: &str) -> Result<&'a [u8], String> {
    top_level(fields, signature)?.ok_or_else(|| format!("missing top-level Starfield {signature}"))
}

fn integer(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn float(bytes: &[u8], offset: usize) -> Result<f32, String> {
    let value = f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
    value
        .is_finite()
        .then_some(value)
        .ok_or_else(|| format!("nonfinite Starfield ballistic float at {offset}"))
}

fn zstring(bytes: &[u8]) -> Result<String, String> {
    let text = bytes
        .strip_suffix(&[0])
        .ok_or("Starfield ballistic string lacks terminator")?;
    if text.contains(&0) {
        return Err("Starfield ballistic string contains an embedded terminator".into());
    }
    String::from_utf8(text.to_vec()).map_err(|error| error.to_string())
}

pub(crate) fn decode_ammo(fields: &[SourceBallisticField]) -> Result<SourceAmmoContract, String> {
    // xEdit SF1/FO4 AMMO agree on these layouts; generic SF DNAM dispatch shadows them with a component string.
    let data = required(fields, "DATA")?;
    let dnam = required(fields, "DNAM")?;
    if data.len() != 8 || dnam.len() != 16 {
        return Err("Starfield AMMO requires DATA8 and DNAM16".into());
    }
    if dnam[4] & !7 != 0 || dnam[5..8] != [0; 3] {
        return Err("unsupported Starfield AMMO flags or unused bytes".into());
    }
    Ok(SourceAmmoContract {
        value: integer(data, 0),
        weight: float(data, 4)?,
        projectile_raw: integer(dnam, 0),
        flags: dnam[4],
        damage: float(dnam, 8)?,
        health: integer(dnam, 12),
        casing_model: top_level(fields, "NAM1")?.map(zstring).transpose()?,
    })
}

pub(crate) fn lower_ammo(
    source: &SourceAmmoContract,
    mut map_reference: impl FnMut(u32) -> Result<u32, String>,
) -> Result<([u8; 8], [u8; 16]), String> {
    let projectile = if source.projectile_raw == 0 {
        0
    } else {
        map_reference(source.projectile_raw)?
    };
    if source.projectile_raw != 0 && projectile == 0 {
        return Err("Starfield AMMO projectile mapping resolved to null".into());
    }
    let mut data = [0; 8];
    data[..4].copy_from_slice(&source.value.to_le_bytes());
    data[4..].copy_from_slice(&source.weight.to_le_bytes());
    let mut dnam = [0; 16];
    dnam[..4].copy_from_slice(&projectile.to_le_bytes());
    dnam[4] = source.flags;
    dnam[8..12].copy_from_slice(&source.damage.to_le_bytes());
    dnam[12..].copy_from_slice(&source.health.to_le_bytes());
    Ok((data, dnam))
}

struct Cursor<'a> {
    bytes: &'a [u8],
    offset: usize,
}
impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Result<&'a [u8], String> {
        let end = self
            .offset
            .checked_add(count)
            .ok_or("Starfield PROD length overflow")?;
        let bytes = self
            .bytes
            .get(self.offset..end)
            .ok_or("truncated Starfield PROD")?;
        self.offset = end;
        Ok(bytes)
    }
    fn u32(&mut self) -> Result<u32, String> {
        Ok(integer(self.take(4)?, 0))
    }
    fn float(&mut self) -> Result<f32, String> {
        float(self.take(4)?, 0)
    }
    fn string(&mut self) -> Result<String, String> {
        let length = self.u32()? as usize;
        if length == 0 {
            return Ok(String::new());
        }
        zstring(self.take(length)?)
    }
}

pub(crate) fn decode_projectile(
    fields: &[SourceBallisticField],
) -> Result<SourceProjectileContract, String> {
    let prod = required(fields, "PROD")?;
    if prod.len() < 81 {
        return Err("truncated Starfield PROD fixed prefix".into());
    }
    // SF1.pas defines the variable tail after an unaligned FormID at byte77; FO4 DNAM is a different layout.
    let mut tail = Cursor {
        bytes: prod,
        offset: 81,
    };
    let muzzle_particle_define = tail.string()?;
    let seek_strength = tail.float()?;
    let sound_activation_condition = tail.u32()?;
    let projectile_type = tail.take(1)?[0];
    let particle_define = tail.string()?;
    let tracer_speed = tail.float()?;
    let zero_g_recoil_strength = tail.float()?;
    let zero_g_recoil_length = tail.float()?;
    let explosion_removal_delay = tail.float()?;
    if tail.offset != prod.len() {
        return Err("unconsumed Starfield PROD tail".into());
    }
    if projectile_type > 8 || sound_activation_condition > 2 {
        return Err("unknown Starfield PROD type or sound condition".into());
    }
    Ok(SourceProjectileContract {
        flags: integer(prod, 0),
        gravity: float(prod, 4)?,
        speed: float(prod, 8)?,
        range: float(prod, 12)?,
        light_raw: integer(prod, 16),
        muzzle_light_raw: integer(prod, 20),
        explosion_proximity: float(prod, 24)?,
        explosion_timer: float(prod, 28)?,
        explosion_raw: integer(prod, 32),
        muzzle_duration: float(prod, 36)?,
        fade_duration: float(prod, 40)?,
        impact_force: float(prod, 44)?,
        weapon_raw: integer(prod, 48),
        cone_spread: float(prod, 52)?,
        collision_radius: float(prod, 56)?,
        lifetime: float(prod, 60)?,
        relaunch_interval: float(prod, 64)?,
        decal_raw: integer(prod, 68),
        collision_layer_raw: integer(prod, 72),
        tracer_frequency: prod[76],
        vats_projectile_raw: integer(prod, 77),
        muzzle_particle_define,
        seek_strength,
        sound_activation_condition,
        projectile_type,
        particle_define,
        tracer_speed,
        zero_g_recoil_strength,
        zero_g_recoil_length,
        explosion_removal_delay,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn field(signature: &str, bytes: &[u8]) -> SourceBallisticField {
        SourceBallisticField {
            signature: signature.into(),
            bytes: bytes.into(),
        }
    }
    fn ammo_fields() -> Vec<SourceBallisticField> {
        let mut data = vec![0; 8];
        data[..4].copy_from_slice(&9_u32.to_le_bytes());
        data[4..].copy_from_slice(&0.003_f32.to_le_bytes());
        let mut dnam = vec![0; 16];
        dnam[..4].copy_from_slice(&0x481F_u32.to_le_bytes());
        dnam[4] = 7;
        dnam[8..12].copy_from_slice(&5.5_f32.to_le_bytes());
        dnam[12..].copy_from_slice(&123456789_u32.to_le_bytes());
        vec![
            field("DATA", &data),
            field("DNAM", &dnam),
            field("NAM1", b"Ammo/Casing.nif\0"),
        ]
    }
    #[test]
    fn ammo_health_is_integer_and_projectile_mapping_is_strict() {
        let source = decode_ammo(&ammo_fields()).unwrap();
        assert_eq!(source.health, 123456789);
        let (data, dnam) = lower_ammo(&source, |raw| {
            assert_eq!(raw, 0x481F);
            Ok(0x01000800)
        })
        .unwrap();
        assert_eq!(integer(&data, 0), 9);
        assert_eq!(integer(&dnam, 0), 0x01000800);
        assert_eq!(integer(&dnam, 12), 123456789);
        assert!(lower_ammo(&source, |_| Ok(0)).is_err());
        assert!(lower_ammo(&source, |_| Err("unmapped".into())).is_err());
    }
    #[test]
    fn ammo_component_shadow_does_not_replace_raw_contract() {
        let mut fields = vec![
            field("BFCB", b"Component\0"),
            field("DNAM", b"response.txt\0"),
            field("BFCE", &[]),
        ];
        fields.extend(ammo_fields());
        assert_eq!(decode_ammo(&fields).unwrap().projectile_raw, 0x481F);
        fields.push(field("DNAM", &[0; 16]));
        assert!(decode_ammo(&fields).unwrap_err().contains("duplicate"));
    }
    #[test]
    fn invalid_ammo_and_component_boundaries_are_rejected() {
        for offset in [4, 5] {
            let mut fields = ammo_fields();
            fields[1].bytes[offset] = 0x80;
            assert!(decode_ammo(&fields).is_err());
        }
        let mut fields = ammo_fields();
        fields[0].bytes[4..].copy_from_slice(&f32::NAN.to_le_bytes());
        assert!(decode_ammo(&fields).is_err());
        assert!(decode_ammo(&[field("BFCB", &[])]).is_err());
        assert!(decode_ammo(&[field("BFCE", &[])]).is_err());
    }
    fn projectile_fields() -> Vec<SourceBallisticField> {
        let mut bytes = vec![0; 81];
        bytes[8..12].copy_from_slice(&1000_f32.to_le_bytes());
        bytes[77..81].copy_from_slice(&0x12345678_u32.to_le_bytes());
        bytes.extend_from_slice(&7_u32.to_le_bytes());
        bytes.extend_from_slice(b"Muzzle\0");
        bytes.extend_from_slice(&0.003_f32.to_le_bytes());
        bytes.extend_from_slice(&1_u32.to_le_bytes());
        bytes.push(2);
        bytes.extend_from_slice(&0_u32.to_le_bytes());
        bytes.extend_from_slice(&[0; 16]);
        vec![field("PROD", &bytes)]
    }
    #[test]
    fn projectile_variable_tail_and_unaligned_reference_are_decoded() {
        let source = decode_projectile(&projectile_fields()).unwrap();
        assert_eq!(source.vats_projectile_raw, 0x12345678);
        assert_eq!(source.speed, 1000.);
        assert_eq!(source.projectile_type, 2);
        assert_eq!(source.muzzle_particle_define, "Muzzle");
        assert_eq!(source.sound_activation_condition, 1);
    }
    #[test]
    fn malformed_projectile_strings_trailing_bytes_and_nonfinite_values_are_rejected() {
        let mut fields = projectile_fields();
        fields[0].bytes[81..85].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_projectile(&fields).is_err());
        let mut fields = projectile_fields();
        fields[0].bytes.push(0);
        assert!(decode_projectile(&fields).is_err());
        let mut fields = projectile_fields();
        fields[0].bytes[8..12].copy_from_slice(&f32::INFINITY.to_le_bytes());
        assert!(decode_projectile(&fields).is_err());
    }

    #[test]
    #[ignore = "requires source Starfield.esm"]
    fn real_source_ammo_and_projectile_corpus_preserves_binary_contracts() {
        use esp_authoring_core::plugin_runtime::{
            effective_subrecords_for_record, ensure_core_section, ensure_records_section,
            plugin_handle_close_native, plugin_handle_load_lazy_records_no_py,
            plugin_handle_store_ref,
        };
        let handle = plugin_handle_load_lazy_records_no_py(
            "N:/Steam Games/steamapps/common/Starfield/Data/Starfield.esm",
            "starfield",
            None,
            &[*b"AMMO", *b"PROJ"],
        )
        .unwrap();
        let mut errors = Vec::new();
        let mut counts = [0; 2];
        let mut eon_ammo = None;
        let mut eon_projectile = None;
        {
            let store_ref = plugin_handle_store_ref();
            let mut store = store_ref.lock().unwrap();
            let slot = store.get_mut(&handle).unwrap();
            let core = ensure_core_section(slot);
            let records = ensure_records_section(slot);
            for (index, signature) in ["AMMO", "PROJ"].into_iter().enumerate() {
                for key in core.by_signature_form_keys.get(signature).unwrap() {
                    let entry = core.by_form_key.get(key).unwrap();
                    let record = if slot.is_lazy() {
                        slot.lazy_record(entry.raw_form_id).unwrap()
                    } else {
                        records
                            .record(&slot.parsed, entry.raw_form_id)
                            .unwrap()
                            .clone()
                    };
                    let fields = effective_subrecords_for_record(&record)
                        .iter()
                        .map(|field| SourceBallisticField {
                            signature: field.signature.to_string(),
                            bytes: field.data.to_vec(),
                        })
                        .collect::<Vec<_>>();
                    let result = if index == 0 {
                        decode_ammo(&fields).map(|value| {
                            if entry.raw_form_id == 0x4AD3E {
                                eon_ammo = Some(value);
                            }
                        })
                    } else {
                        decode_projectile(&fields).map(|value| {
                            if entry.raw_form_id == 0x481F {
                                eon_projectile = Some(value);
                            }
                        })
                    };
                    counts[index] += 1;
                    if let Err(error) = result {
                        errors.push(format!("{}: {error}", key.render()));
                    }
                }
            }
        }
        assert!(plugin_handle_close_native(handle));
        println!(
            "Starfield ballistic source corpus: AMMO={} PROJ={} errors={}",
            counts[0],
            counts[1],
            errors.len()
        );
        assert!(errors.is_empty(), "{errors:?}");
        let ammo = eon_ammo.unwrap();
        assert_eq!(ammo.projectile_raw, 0x481F);
        assert_eq!(ammo.value, 9);
        assert_eq!(ammo.health, 0);
        assert!((ammo.weight - 0.003).abs() < 1e-8);
        let projectile = eon_projectile.unwrap();
        assert_eq!(projectile.projectile_type, 2);
        assert_eq!(projectile.muzzle_particle_define, "Muzzle");
        assert_eq!(projectile.particle_define, "");
    }
}
