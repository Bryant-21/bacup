use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::{StringInterner, Sym};
use smallvec::SmallVec;

pub(crate) const FO4_RIGHT_HAND_EQUIP_LOCAL: u32 = 0x01_3F42;
pub(crate) const FO4_BOTH_HANDS_EQUIP_LOCAL: u32 = 0x01_3F45;
pub(crate) const FO4_UNARMED_WEAPON_EQUIP_LOCAL: u32 = 0x04_334D;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Fo4MeleeDependency {
    pub local: u32,
    pub signature: &'static str,
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct Fo4MeleeProfile {
    pub block_impact: u32,
    pub block_material: u32,
    pub keywords: &'static [u32],
    pub swing_sound: u32,
    pub idle_sound: Option<u32>,
    pub equip_sound: u32,
    pub unequip_sound: u32,
    pub impact: u32,
    pub animation_type: u64,
    pub flags: u64,
    pub animation_attack_seconds: f32,
    pub action_point_cost: f32,
    pub stagger: u64,
    pub trailing_unknowns: [u64; 4],
}

#[derive(Clone, Debug)]
pub(crate) struct Fo4MeleeSourcePayload {
    pub editor_id: FieldEntry,
    pub bounds: FieldEntry,
    pub display_name: FieldEntry,
    pub world_model: FieldEntry,
    pub first_person_model: Sym,
    pub value: u32,
    pub weight: f32,
    pub damage: u16,
}

#[derive(Clone, Debug)]
pub(crate) struct Fo4ResolvedMeleeSourcePayload {
    pub editor_id: FieldEntry,
    pub bounds: Option<FieldEntry>,
    pub display_name: Option<FieldEntry>,
    pub world_model: Option<FieldEntry>,
    pub first_person_model: Option<Sym>,
    pub value: u32,
    pub weight: f32,
    pub damage: u16,
}

pub(crate) fn emit_fo4_melee_weapon(
    record: &mut Record,
    source: Fo4MeleeSourcePayload,
    profile: Fo4MeleeProfile,
    interner: &StringInterner,
) {
    emit_fo4_resolved_melee_weapon(
        record,
        Fo4ResolvedMeleeSourcePayload {
            editor_id: source.editor_id,
            bounds: Some(source.bounds),
            display_name: Some(source.display_name),
            world_model: Some(source.world_model),
            first_person_model: Some(source.first_person_model),
            value: source.value,
            weight: source.weight,
            damage: source.damage,
        },
        profile,
        interner,
    );
}

pub(crate) fn emit_fo4_resolved_melee_weapon(
    record: &mut Record,
    source: Fo4ResolvedMeleeSourcePayload,
    profile: Fo4MeleeProfile,
    interner: &StringInterner,
) {
    emit_fo4_resolved_melee_weapon_with_equip(
        record,
        source,
        profile,
        FO4_RIGHT_HAND_EQUIP_LOCAL,
        interner,
    );
}

pub(crate) fn emit_fo4_resolved_melee_weapon_with_equip(
    record: &mut Record,
    source: Fo4ResolvedMeleeSourcePayload,
    profile: Fo4MeleeProfile,
    equip_type: u32,
    interner: &StringInterner,
) {
    let fallout4 = interner.intern("Fallout4.esm");
    let dnam = target_dnam(&source, profile, fallout4, interner);
    let mut fields = vec![source.editor_id];
    fields.extend(source.bounds);
    fields.extend(source.display_name);
    fields.extend(source.world_model);
    fields.extend([
        form_key_field("ETYP", fallout4, equip_type),
        form_key_field("BIDS", fallout4, profile.block_impact),
        form_key_field("BAMT", fallout4, profile.block_material),
        field(
            "KSIZ",
            FieldValue::Uint(
                u64::try_from(profile.keywords.len()).expect("keyword count fits u64"),
            ),
        ),
        field(
            "KWDA",
            FieldValue::List(
                profile
                    .keywords
                    .iter()
                    .copied()
                    .map(|local| {
                        FieldValue::FormKey(FormKey {
                            local,
                            plugin: fallout4,
                        })
                    })
                    .collect(),
            ),
        ),
    ]);
    if let Some(first_person_model) = source.first_person_model {
        fields.extend([
            field("MOD4", FieldValue::String(first_person_model)),
            model_texture_data("MO4T"),
        ]);
    }
    fields.extend([
        field("DNAM", dnam),
        field("CRDT", target_critical_data(interner)),
        form_key_field("INAM", fallout4, profile.impact),
    ]);
    record.fields = SmallVec::from_vec(fields);

    debug_assert!(audit_fo4_melee_dependencies_with_equip(
        record, profile, equip_type, interner
    ));
}

pub(crate) fn emit_fo4_unarmed_weapon(
    record: &mut Record,
    source: Fo4ResolvedMeleeSourcePayload,
    interner: &StringInterner,
) {
    const UNARMED_KEYWORDS: &[u32] = &[0x02_405E, 0x05_240E, 0x23_0326];
    const UNARMED_IMPACT: u32 = 0x0C_190F;

    let fallout4 = interner.intern("Fallout4.esm");
    let dnam = target_unarmed_dnam(&source, interner);
    let mut fields = vec![source.editor_id];
    fields.extend(source.bounds);
    fields.extend(source.display_name);
    fields.extend(source.world_model);
    fields.extend([
        form_key_field("ETYP", fallout4, FO4_BOTH_HANDS_EQUIP_LOCAL),
        field(
            "KSIZ",
            FieldValue::Uint(
                u64::try_from(UNARMED_KEYWORDS.len()).expect("keyword count fits u64"),
            ),
        ),
        field(
            "KWDA",
            FieldValue::List(
                UNARMED_KEYWORDS
                    .iter()
                    .copied()
                    .map(|local| {
                        FieldValue::FormKey(FormKey {
                            local,
                            plugin: fallout4,
                        })
                    })
                    .collect(),
            ),
        ),
    ]);
    if let Some(first_person_model) = source.first_person_model {
        fields.extend([
            field("MOD4", FieldValue::String(first_person_model)),
            model_texture_data("MO4T"),
        ]);
    }
    fields.extend([
        field("DNAM", dnam),
        field("CRDT", target_critical_data(interner)),
        form_key_field("INAM", fallout4, UNARMED_IMPACT),
    ]);
    record.fields = SmallVec::from_vec(fields);
}

pub(crate) fn fo4_melee_dependencies(profile: Fo4MeleeProfile) -> Vec<Fo4MeleeDependency> {
    fo4_melee_dependencies_with_equip(profile, FO4_RIGHT_HAND_EQUIP_LOCAL)
}

fn fo4_melee_dependencies_with_equip(
    profile: Fo4MeleeProfile,
    equip_type: u32,
) -> Vec<Fo4MeleeDependency> {
    let mut dependencies = vec![
        Fo4MeleeDependency {
            local: equip_type,
            signature: "EQUP",
        },
        Fo4MeleeDependency {
            local: profile.block_impact,
            signature: "IPDS",
        },
        Fo4MeleeDependency {
            local: profile.block_material,
            signature: "MATT",
        },
    ];
    dependencies.extend(
        profile
            .keywords
            .iter()
            .copied()
            .map(|local| Fo4MeleeDependency {
                local,
                signature: "KYWD",
            }),
    );
    dependencies.push(Fo4MeleeDependency {
        local: profile.swing_sound,
        signature: "SNDR",
    });
    if let Some(local) = profile.idle_sound {
        dependencies.push(Fo4MeleeDependency {
            local,
            signature: "SNDR",
        });
    }
    dependencies.extend([
        Fo4MeleeDependency {
            local: profile.equip_sound,
            signature: "SNDR",
        },
        Fo4MeleeDependency {
            local: profile.unequip_sound,
            signature: "SNDR",
        },
        Fo4MeleeDependency {
            local: profile.impact,
            signature: "IPDS",
        },
    ]);
    dependencies
}

pub(crate) fn audit_fo4_melee_dependencies(
    record: &Record,
    profile: Fo4MeleeProfile,
    interner: &StringInterner,
) -> bool {
    audit_fo4_melee_dependencies_with_equip(record, profile, FO4_RIGHT_HAND_EQUIP_LOCAL, interner)
}

fn audit_fo4_melee_dependencies_with_equip(
    record: &Record,
    profile: Fo4MeleeProfile,
    equip_type: u32,
    interner: &StringInterner,
) -> bool {
    let fallout4 = interner.intern("Fallout4.esm");
    let mut actual = Vec::new();
    for field in &record.fields {
        collect_form_keys(&field.value, &mut actual);
    }
    if actual
        .iter()
        .any(|form_key| form_key.plugin != fallout4 || form_key.local == 0)
    {
        return false;
    }

    let mut actual = actual
        .into_iter()
        .map(|form_key| form_key.local)
        .collect::<Vec<_>>();
    actual.sort_unstable();
    let mut expected = fo4_melee_dependencies_with_equip(profile, equip_type)
        .into_iter()
        .map(|dependency| dependency.local)
        .collect::<Vec<_>>();
    expected.sort_unstable();
    actual == expected
}

fn target_dnam(
    source: &Fo4ResolvedMeleeSourcePayload,
    profile: Fo4MeleeProfile,
    fallout4: Sym,
    interner: &StringInterner,
) -> FieldValue {
    let zero = || FieldValue::Uint(0);
    let target_ref = |local| {
        FieldValue::FormKey(FormKey {
            local,
            plugin: fallout4,
        })
    };
    let idle_sound = profile.idle_sound.map_or_else(zero, target_ref);
    let values = vec![
        ("ammo", zero()),
        ("speed", FieldValue::Float(1.0)),
        ("reload_speed", FieldValue::Float(1.0)),
        ("reach", FieldValue::Float(0.8)),
        ("min_range", FieldValue::Float(0.0)),
        ("max_range", FieldValue::Float(10.0)),
        ("attack_delay", FieldValue::Float(0.0)),
        ("unused", FieldValue::Float(0.0)),
        ("damage_outofrange_mult", FieldValue::Float(-1.0)),
        ("on_hit", FieldValue::Uint(1)),
        ("skill", zero()),
        ("resist", zero()),
        ("flags", FieldValue::Uint(profile.flags)),
        ("capacity", zero()),
        ("animation_type", FieldValue::Uint(profile.animation_type)),
        ("damage_secondary", FieldValue::Float(0.0)),
        ("weight", FieldValue::Float(source.weight)),
        ("value", FieldValue::Uint(u64::from(source.value))),
        ("damage_base", FieldValue::Uint(u64::from(source.damage))),
        ("sound_level", FieldValue::Uint(4)),
        ("sound_attack", target_ref(profile.swing_sound)),
        ("sound_attack_2d", zero()),
        ("sound_attack_loop", zero()),
        ("sound_attack_fail", zero()),
        ("sound_idle", idle_sound),
        ("sound_equip_sound", target_ref(profile.equip_sound)),
        ("sound_unequip_sound", target_ref(profile.unequip_sound)),
        ("sound_fast_equip_sound", zero()),
        ("accuracy_bonus", zero()),
        (
            "animation_attack_seconds",
            FieldValue::Float(profile.animation_attack_seconds),
        ),
        ("unknown_u8_30", zero()),
        ("unknown_u8_31", zero()),
        (
            "action_point_cost",
            FieldValue::Float(profile.action_point_cost),
        ),
        ("full_power_seconds", FieldValue::Float(0.0)),
        ("min_power_per_shot", FieldValue::Float(0.0)),
        ("stagger", FieldValue::Uint(profile.stagger)),
        (
            "unknown_u8_36",
            FieldValue::Uint(profile.trailing_unknowns[0]),
        ),
        (
            "unknown_u8_37",
            FieldValue::Uint(profile.trailing_unknowns[1]),
        ),
        (
            "unknown_u8_38",
            FieldValue::Uint(profile.trailing_unknowns[2]),
        ),
        (
            "unknown_u8_39",
            FieldValue::Uint(profile.trailing_unknowns[3]),
        ),
    ];
    FieldValue::Struct(
        values
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}

fn target_unarmed_dnam(
    source: &Fo4ResolvedMeleeSourcePayload,
    interner: &StringInterner,
) -> FieldValue {
    let zero = || FieldValue::Uint(0);
    let values = vec![
        ("ammo", zero()),
        ("speed", FieldValue::Float(1.0)),
        ("reload_speed", FieldValue::Float(1.0)),
        ("reach", FieldValue::Float(0.68)),
        ("min_range", FieldValue::Float(500.0)),
        ("max_range", FieldValue::Float(2000.0)),
        ("attack_delay", FieldValue::Float(0.0)),
        ("unused", FieldValue::Float(0.0)),
        ("damage_outofrange_mult", FieldValue::Float(0.5)),
        ("on_hit", FieldValue::Uint(3)),
        ("skill", zero()),
        ("resist", zero()),
        ("flags", FieldValue::Uint(256 | 131_072 | 1_048_576)),
        ("capacity", zero()),
        ("animation_type", zero()),
        ("damage_secondary", FieldValue::Float(0.0)),
        ("weight", FieldValue::Float(source.weight)),
        ("value", FieldValue::Uint(u64::from(source.value))),
        ("damage_base", FieldValue::Uint(u64::from(source.damage))),
        ("sound_level", zero()),
        ("sound_attack", zero()),
        ("sound_attack_2d", zero()),
        ("sound_attack_loop", zero()),
        ("sound_attack_fail", zero()),
        ("sound_idle", zero()),
        ("sound_equip_sound", zero()),
        ("sound_unequip_sound", zero()),
        ("sound_fast_equip_sound", zero()),
        ("accuracy_bonus", zero()),
        ("animation_attack_seconds", FieldValue::Float(1.117_955)),
        ("unknown_u8_30", zero()),
        ("unknown_u8_31", zero()),
        ("action_point_cost", FieldValue::Float(20.0)),
        ("full_power_seconds", FieldValue::Float(0.0)),
        ("min_power_per_shot", FieldValue::Float(0.0)),
        ("stagger", FieldValue::Uint(1)),
        ("unknown_u8_36", zero()),
        ("unknown_u8_37", zero()),
        ("unknown_u8_38", zero()),
        ("unknown_u8_39", zero()),
    ];
    FieldValue::Struct(
        values
            .into_iter()
            .map(|(name, value)| (interner.intern(name), value))
            .collect(),
    )
}

fn target_critical_data(interner: &StringInterner) -> FieldValue {
    FieldValue::Struct(vec![
        (interner.intern("crit_damage_mult"), FieldValue::Float(2.0)),
        (interner.intern("crit_charge_bonus"), FieldValue::Float(1.0)),
        (interner.intern("crit_effect"), FieldValue::Uint(0)),
    ])
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

fn field(signature: &str, value: FieldValue) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig::from_str(signature).expect("valid subrecord signature"),
        value,
    }
}

fn form_key_field(signature: &str, plugin: Sym, local: u32) -> FieldEntry {
    field(signature, FieldValue::FormKey(FormKey { local, plugin }))
}

fn model_texture_data(signature: &str) -> FieldEntry {
    field(
        signature,
        FieldValue::Bytes(SmallVec::from_slice(&[
            4, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0,
        ])),
    )
}

#[cfg(test)]
pub(crate) fn projection_snapshot(record: &Record, interner: &StringInterner) -> String {
    use std::fmt::Write;

    fn write_value(output: &mut String, value: &FieldValue, interner: &StringInterner) {
        use std::fmt::Write;

        match value {
            FieldValue::None => output.push_str("none"),
            FieldValue::Bool(value) => write!(output, "bool:{value}").unwrap(),
            FieldValue::Int(value) => write!(output, "i:{value}").unwrap(),
            FieldValue::Uint(value) => write!(output, "u:{value}").unwrap(),
            FieldValue::Float(value) => write!(output, "f:{:08X}", value.to_bits()).unwrap(),
            FieldValue::String(value) => write!(
                output,
                "s:{}",
                interner.resolve(*value).unwrap_or("<unresolved>")
            )
            .unwrap(),
            FieldValue::Bytes(bytes) => write!(output, "b:{}", hex::encode(bytes)).unwrap(),
            FieldValue::FormKey(form_key) => write!(
                output,
                "r:{:06X}@{}",
                form_key.local,
                interner.resolve(form_key.plugin).unwrap_or("<unresolved>")
            )
            .unwrap(),
            FieldValue::List(values) => {
                output.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    write_value(output, value, interner);
                }
                output.push(']');
            }
            FieldValue::Struct(fields) => {
                output.push('{');
                for (index, (name, value)) in fields.iter().enumerate() {
                    if index != 0 {
                        output.push(',');
                    }
                    write!(
                        output,
                        "{}=",
                        interner.resolve(*name).unwrap_or("<unresolved>")
                    )
                    .unwrap();
                    write_value(output, value, interner);
                }
                output.push('}');
            }
        }
    }

    let mut output = String::new();
    for (index, field) in record.fields.iter().enumerate() {
        if index != 0 {
            output.push('|');
        }
        write!(output, "{}=", field.sig.as_str()).unwrap();
        write_value(&mut output, &field.value, interner);
    }
    output
}
