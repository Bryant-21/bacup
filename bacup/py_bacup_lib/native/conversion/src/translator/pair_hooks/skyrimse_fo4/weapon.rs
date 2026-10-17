use crate::ids::SubrecordSig;
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hook::{HookError, HookResult, PairCtx};

pub(super) fn normalize_skyrim_weapon(record: &mut Record, ctx: &PairCtx<'_>) -> HookResult {
    if record.sig.0 != *b"WEAP" || field(record, b"DATA").is_none() {
        return Ok(());
    }
    let Some(FieldValue::Bytes(source)) = field(record, b"DNAM") else {
        return Err(HookError::Runtime(
            "Skyrim WEAP requires source DNAM bytes".into(),
        ));
    };
    let Some(FieldValue::Bytes(inventory)) = field(record, b"DATA") else {
        return Err(HookError::Runtime(
            "Skyrim WEAP requires source DATA bytes".into(),
        ));
    };
    if source.len() != 100 || inventory.len() != 10 {
        return Err(HookError::Runtime(format!(
            "Skyrim WEAP has unsupported DATA/DNAM lengths {}/{}",
            inventory.len(),
            source.len()
        )));
    }

    let animation_type = source[0];
    if animation_type > 9 {
        return Err(HookError::Runtime(format!(
            "unsupported Skyrim weapon animation type {animation_type}"
        )));
    }
    let source_flags = u16::from_le_bytes(source[12..14].try_into().unwrap());
    let source_flags2 = u32_at(source, 40);
    let mut flags = source_flags2 & (1 | 2 | 16 | 32 | 64 | 4096 | 8192);
    for (source_bit, target_bit) in [(1, 16384), (8, 131072), (32, 524288), (128, 1048576)] {
        if source_flags & source_bit != 0 {
            flags |= target_bit;
        }
    }
    if animation_type == 7 {
        // CalcWeaponPower reads the player's held input only for HoldInputToPower bows.
        flags |= 0x800;
    }

    let mut critical_effect = FieldValue::Uint(0);
    if let Some(FieldValue::Bytes(critical)) = field(record, b"CRDT") {
        let effect_offset = match critical.len() {
            16 => 12,
            24 => 16,
            length => {
                return Err(HookError::Runtime(format!(
                    "unsupported Skyrim WEAP CRDT length {length}"
                )));
            }
        };
        if critical[8] & 1 != 0 {
            flags |= 256;
        }
        let raw_effect = u32_at(critical, effect_offset);
        if raw_effect != 0 {
            critical_effect =
                FieldValue::FormKey(ctx.resolve_source_form_id(raw_effect).ok_or_else(|| {
                    HookError::Runtime(format!(
                        "Skyrim WEAP critical effect {raw_effect:08X} has no source owner"
                    ))
                })?);
        }
    }

    let mut dnam = vec![
        ("ammo", FieldValue::Uint(0)),
        ("speed", FieldValue::Float(f32_at(source, 4))),
        ("reload_speed", FieldValue::Float(1.0)),
        ("reach", FieldValue::Float(f32_at(source, 8))),
        ("min_range", FieldValue::Float(f32_at(source, 28))),
        ("max_range", FieldValue::Float(f32_at(source, 32))),
        ("damage_outofrange_mult", FieldValue::Float(1.0)),
        ("on_hit", FieldValue::Uint(u32_at(source, 36).into())),
        // Skyrim skill/resist are actor-value enum indices; FO4 stores AVIF FormIDs.
        ("skill", FieldValue::Uint(0)),
        ("resist", FieldValue::Uint(0)),
        ("flags", FieldValue::Uint(flags.into())),
        ("capacity", FieldValue::Uint(u64::from(animation_type == 9))),
        // Crossbow uses FO4's gun slot; source bow/staff graph support remains separate.
        ("animation_type", FieldValue::Uint(animation_type.into())),
        ("weight", FieldValue::Float(f32_at(inventory, 4))),
        ("value", FieldValue::Uint(u32_at(inventory, 0).into())),
        (
            "damage_base",
            FieldValue::Uint(u16::from_le_bytes(inventory[8..10].try_into().unwrap()).into()),
        ),
        (
            "sound_level",
            field(record, b"VNAM")
                .cloned()
                .unwrap_or(FieldValue::Uint(0)),
        ),
    ];
    for (signature, name) in [
        (*b"SNAM", "sound_attack"),
        (*b"XNAM", "sound_attack_2d"),
        (*b"NAM7", "sound_attack_loop"),
        (
            *b"TNAM",
            if animation_type <= 6 {
                "sound_attack"
            } else {
                "sound_attack_fail"
            },
        ),
        (*b"UNAM", "sound_idle"),
        (*b"NAM9", "sound_equip_sound"),
        (*b"NAM8", "sound_unequip_sound"),
    ] {
        if let Some(value) = field(record, &signature)
            && !dnam.iter().any(|(existing, _)| *existing == name)
        {
            dnam.push((name, value.clone()));
        }
    }
    for name in [
        "attack_delay",
        "unused",
        "damage_secondary",
        "animation_attack_seconds",
        "action_point_cost",
        "full_power_seconds",
        "min_power_per_shot",
    ] {
        dnam.push((name, FieldValue::Float(0.0)));
    }
    for name in [
        "sound_attack",
        "sound_attack_2d",
        "sound_attack_loop",
        "sound_attack_fail",
        "sound_idle",
        "sound_equip_sound",
        "sound_unequip_sound",
        "sound_fast_equip_sound",
        "accuracy_bonus",
        "unknown_u8_30",
        "unknown_u8_31",
        "stagger",
        "unknown_u8_36",
        "unknown_u8_37",
        "unknown_u8_38",
        "unknown_u8_39",
    ] {
        if !dnam.iter().any(|(existing, _)| *existing == name) {
            dnam.push((name, FieldValue::Uint(0)));
        }
    }
    let fnam = structured(
        ctx.interner,
        [
            ("animation_fire_seconds", FieldValue::Float(0.0)),
            (
                "rumble_left_motor_strength",
                FieldValue::Float(f32_at(source, 52)),
            ),
            (
                "rumble_right_motor_strength",
                FieldValue::Float(f32_at(source, 56)),
            ),
            ("rumble_duration", FieldValue::Float(f32_at(source, 60))),
            ("projectiles", FieldValue::Uint(source[26].into())),
            ("animation_reload_seconds", FieldValue::Float(0.0)),
            ("bolt_anim_seconds_byte_1", FieldValue::Uint(0)),
            ("bolt_anim_seconds_byte_2", FieldValue::Uint(0)),
            ("bolt_anim_seconds_byte_3", FieldValue::Uint(0)),
            ("bolt_anim_seconds_byte_4", FieldValue::Uint(0)),
            ("sighted_transition_seconds", FieldValue::Float(0.0)),
            ("override_projectile", FieldValue::Uint(0)),
            ("pattern", FieldValue::Uint(0)),
            ("rumble_peroid_ms", FieldValue::Uint(0)),
        ],
    );
    let dnam = structured(ctx.interner, dnam);
    // Absolute source critical damage/chance do not represent FO4 multipliers.
    // Match the target baseline while preserving the source critical effect.
    let crdt = structured(
        ctx.interner,
        [
            ("crit_damage_mult", FieldValue::Float(2.0)),
            ("crit_charge_bonus", FieldValue::Float(1.0)),
            ("crit_effect", critical_effect),
        ],
    );
    record.fields.retain(|entry| {
        !matches!(
            &entry.sig.0,
            b"DATA"
                | b"DNAM"
                | b"CRDT"
                | b"FNAM"
                | b"VNAM"
                | b"SNAM"
                | b"XNAM"
                | b"NAM7"
                | b"TNAM"
                | b"UNAM"
                | b"NAM9"
                | b"NAM8"
        )
    });
    for (signature, value) in [(*b"DNAM", dnam), (*b"FNAM", fnam), (*b"CRDT", crdt)] {
        record.fields.push(FieldEntry {
            sig: SubrecordSig(signature),
            value,
        });
    }
    Ok(())
}

fn field<'a>(record: &'a Record, signature: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| &entry.sig.0 == signature)
        .map(|entry| &entry.value)
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn f32_at(bytes: &[u8], offset: usize) -> f32 {
    f32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn structured<'a>(
    interner: &StringInterner,
    values: impl IntoIterator<Item = (&'a str, FieldValue)>,
) -> FieldValue {
    FieldValue::Struct(
        values
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}
