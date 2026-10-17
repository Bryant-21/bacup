use std::path::Path;

use serde::Serialize;

use crate::ids::{FormKey, SigCode};
use crate::record::{FieldValue, Record, RecordFlags};
use crate::run::ConversionRun;
use crate::source_read::{iter_form_keys_of_sig, read_record_relayout_by_form_key};
use crate::sym::StringInterner;
use crate::translator::Game;

#[derive(Serialize)]
struct SourceWeapon {
    weapon: String,
    animation_type: u8,
    dwarven_crossbow: bool,
}

#[derive(Serialize)]
struct SourceAmmo {
    ammo: String,
    non_bolt: bool,
    damage: f32,
    projectile: String,
}

#[derive(Serialize)]
pub(crate) struct WeaponRuntimeOutput {
    version: u32,
    source_game: &'static str,
    weapons: Vec<SourceWeapon>,
    ammunition: Vec<SourceAmmo>,
    #[serde(skip_serializing_if = "super::bow_stagger::BowStaggerOutput::is_empty")]
    bow_stagger: super::bow_stagger::BowStaggerOutput,
    #[serde(skip)]
    plugin: String,
}

impl WeaponRuntimeOutput {
    pub(crate) fn write(&self, mod_path: &Path) -> Result<u32, String> {
        let directory = mod_path.join("F4SE/Plugins/B21_ConversionRuntime/Weapons");
        let path = directory.join(format!("{}.json", self.plugin));
        std::fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let bytes = serde_json::to_vec_pretty(self).map_err(|error| error.to_string())?;
        std::fs::write(path, bytes).map_err(|error| error.to_string())?;
        Ok(1)
    }
}

pub(crate) fn prepare_weapon_runtime_output(
    run: &ConversionRun,
) -> Result<Option<WeaponRuntimeOutput>, String> {
    if run.source != Game::SkyrimSe || run.target != Game::Fo4 {
        return Ok(None);
    }
    let mapper = run
        .mapper_state
        .as_ref()
        .ok_or("Source weapon mapper is unavailable")?;
    let output_plugin = run.interner.intern(&run.config.output_plugin_name);
    let mut weapons = std::collections::BTreeMap::new();
    for source_key in iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"WEAP"), &run.interner)
        .map_err(|error| error.to_string())?
    {
        let Some(target_key) = mapper
            .source_to_target
            .get(&source_key)
            .filter(|target| target.plugin == output_plugin)
        else {
            continue;
        };
        let Some(target) = run
            .read_target_record_if_available(*target_key)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if target.flags.contains(RecordFlags::DELETED) {
            continue;
        }
        if target.sig.0 != *b"WEAP" {
            return Err(format!(
                "Source weapon {} mapped to a non-WEAP",
                source_key.format(&run.interner)
            ));
        }
        let read_source = |key: FormKey| {
            read_record_relayout_by_form_key(
                run.source_handle_id,
                &key,
                &run.schema_source,
                &run.interner,
                None,
            )
            .map_err(|error| error.to_string())
        };
        let profile = source_weapon_profile(read_source(source_key)?, &run.interner, read_source)
            .map_err(|error| {
            format!(
                "Source weapon {}: {error}",
                source_key.format(&run.interner)
            )
        })?;
        let weapon = runtime_key(*target_key, &run.interner);
        if weapons
            .insert(weapon.clone(), profile)
            .is_some_and(|previous| previous != profile)
        {
            return Err(format!(
                "Converted source weapon {weapon} has conflicting source animation profiles"
            ));
        }
    }
    let mut ammunition = std::collections::BTreeMap::new();
    for source_key in iter_form_keys_of_sig(run.source_handle_id, SigCode(*b"AMMO"), &run.interner)
        .map_err(|error| error.to_string())?
    {
        let Some(target_key) = mapper
            .source_to_target
            .get(&source_key)
            .filter(|target| target.plugin == output_plugin)
        else {
            continue;
        };
        let Some(target) = run
            .read_target_record_if_available(*target_key)
            .map_err(|error| error.to_string())?
        else {
            continue;
        };
        if target.flags.contains(RecordFlags::DELETED) {
            continue;
        }
        let source = read_record_relayout_by_form_key(
            run.source_handle_id,
            &source_key,
            &run.schema_source,
            &run.interner,
            None,
        )
        .map_err(|error| error.to_string())?;
        let (non_bolt, damage) = source_ammo_profile(&source)?;
        let Some(FieldValue::Struct(dnam)) = field(&target, b"DNAM") else {
            return Err("Converted source ammunition has no target DNAM".into());
        };
        let target_field = |name: &str| {
            dnam.iter().find_map(|(key, value)| {
                run.interner
                    .resolve(*key)
                    .is_some_and(|key| normalize(key) == name)
                    .then_some(value)
            })
        };
        let Some(FieldValue::FormKey(projectile)) = target_field("projectile") else {
            return Err("Converted source ammunition has no resolved projectile".into());
        };
        if target.sig.0 != *b"AMMO"
            || target_field("damage") != Some(&FieldValue::Float(damage))
            || run
                .read_target_record_if_available(*projectile)
                .map_err(|error| error.to_string())?
                .is_none_or(|record| {
                    record.sig.0 != *b"PROJ" || record.flags.contains(RecordFlags::DELETED)
                })
        {
            return Err(format!(
                "Converted source ammunition {} changed source damage or lost its projectile",
                target_key.format(&run.interner)
            ));
        }
        let key = runtime_key(*target_key, &run.interner);
        if ammunition
            .insert(
                key.clone(),
                SourceAmmo {
                    ammo: key,
                    non_bolt,
                    damage,
                    projectile: runtime_key(*projectile, &run.interner),
                },
            )
            .is_some()
        {
            return Err("Converted source ammunition has duplicate target mappings".into());
        }
    }
    Ok(Some(WeaponRuntimeOutput {
        version: 1,
        source_game: "skyrimse",
        weapons: weapons
            .into_iter()
            .map(
                |(weapon, (animation_type, dwarven_crossbow))| SourceWeapon {
                    weapon,
                    animation_type,
                    dwarven_crossbow,
                },
            )
            .collect(),
        ammunition: ammunition.into_values().collect(),
        bow_stagger: super::bow_stagger::prepare_bow_stagger_output(run)?,
        plugin: run.config.output_plugin_name.clone(),
    }))
}

fn source_ammo_profile(source: &Record) -> Result<(bool, f32), String> {
    if source.sig.0 != *b"AMMO" || source.flags.contains(RecordFlags::DELETED) {
        return Err("Source ammunition must be a live source AMMO".into());
    }
    let Some(FieldValue::Bytes(data)) = field(source, b"DATA") else {
        return Err("Source ammunition requires source DATA bytes".into());
    };
    if !matches!(data.len(), 16 | 20) {
        return Err("Source ammunition has an unsupported source DATA layout".into());
    }
    let flags = u32::from_le_bytes(data[4..8].try_into().unwrap());
    let damage = f32::from_le_bytes(data[8..12].try_into().unwrap());
    if !damage.is_finite() || damage < 0.0 {
        return Err("Source ammunition has invalid source damage".into());
    }
    Ok((flags & 4 != 0, damage))
}

fn source_weapon_profile(
    mut source: Record,
    interner: &StringInterner,
    mut read_source: impl FnMut(FormKey) -> Result<Record, String>,
) -> Result<(u8, bool), String> {
    let mut seen = std::collections::HashSet::new();
    loop {
        if source.sig.0 != *b"WEAP"
            || source.flags.contains(RecordFlags::DELETED)
            || !seen.insert(source.form_key)
        {
            return Err(
                "Weapon template chain is cyclic or does not resolve to a live WEAP".into(),
            );
        }
        match field(&source, b"CNAM") {
            Some(FieldValue::FormKey(key)) if key.local != 0 => {
                source = read_source(*key)?;
                continue;
            }
            None
            | Some(FieldValue::None | FieldValue::Uint(0) | FieldValue::Int(0))
            | Some(FieldValue::FormKey(_)) => {}
            _ => return Err("Weapon template is not a resolved source FormKey".into()),
        }
        let animation_type = match field(&source, b"DNAM") {
            Some(FieldValue::Bytes(bytes)) if bytes.len() == 100 => Some(u64::from(bytes[0])),
            Some(FieldValue::Struct(fields)) => fields.iter().find_map(|(key, value)| {
                if !interner
                    .resolve(*key)
                    .is_some_and(|key| normalize(key) == "animationtype")
                {
                    return None;
                }
                match value {
                    FieldValue::Uint(value) => Some(*value),
                    FieldValue::Int(value) => u64::try_from(*value).ok(),
                    _ => None,
                }
            }),
            _ => None,
        }
        .filter(|value| *value <= 9)
        .ok_or("Weapon DNAM has no valid source animation type (0..9)")?;
        let dwarven_crossbow = match field(&source, b"KWDA") {
            None => false,
            Some(FieldValue::List(keywords)) => keywords.iter().any(|keyword| {
                matches!(keyword, FieldValue::FormKey(key) if key.local == 0x15992 &&
                    interner.resolve(key.plugin).is_some_and(|plugin| plugin.eq_ignore_ascii_case("Dawnguard.esm")))
            }),
            _ => return Err("Weapon keywords are not resolved source FormKeys".into()),
        };
        if dwarven_crossbow && animation_type != 9 {
            return Err(
                "Dwarven crossbow keyword is attached to a non-crossbow source category".into(),
            );
        }
        return Ok((animation_type as u8, dwarven_crossbow));
    }
}

fn runtime_key(key: FormKey, interner: &StringInterner) -> String {
    format!(
        "{:06X}:{}",
        key.local,
        interner.resolve(key.plugin).unwrap()
    )
}

fn field<'a>(record: &'a Record, signature: &[u8; 4]) -> Option<&'a FieldValue> {
    record
        .fields
        .iter()
        .find(|entry| &entry.sig.0 == signature)
        .map(|entry| &entry.value)
}

fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .flat_map(char::to_lowercase)
        .collect()
}

#[cfg(test)]
#[path = "weapons_runtime_publication_tests.rs"]
mod publication_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SubrecordSig;
    use crate::record::FieldEntry;

    #[test]
    fn source_ammunition_preserves_source_arrow_bolt_type_and_damage() {
        let interner = StringInterner::new();
        let mut ammo = Record::new(
            SigCode(*b"AMMO"),
            FormKey {
                local: 0x1397d,
                plugin: interner.intern("Skyrim.esm"),
            },
        );
        for (length, flags, non_bolt) in [(16, 4u32, true), (20, 0, false), (20, 7, true)] {
            let mut bytes = vec![0; length];
            bytes[4..8].copy_from_slice(&flags.to_le_bytes());
            bytes[8..12].copy_from_slice(&8.0f32.to_le_bytes());
            ammo.fields = vec![FieldEntry {
                sig: SubrecordSig(*b"DATA"),
                value: FieldValue::Bytes(bytes.into()),
            }]
            .into();
            assert_eq!(source_ammo_profile(&ammo), Ok((non_bolt, 8.0)));
        }
        for damage in [f32::NAN, f32::INFINITY, -1.0] {
            let mut bytes = vec![0; 16];
            bytes[8..12].copy_from_slice(&damage.to_le_bytes());
            ammo.fields[0].value = FieldValue::Bytes(bytes.into());
            assert!(source_ammo_profile(&ammo).is_err());
        }
        ammo.fields[0].value = FieldValue::Bytes(vec![0; 15].into());
        assert!(source_ammo_profile(&ammo).is_err());
    }

    #[test]
    fn source_weapon_types_use_source_data_and_reject_other_layouts() {
        let interner = StringInterner::new();
        let key = FormKey {
            local: 0x12eb7,
            plugin: interner.intern("Skyrim.esm"),
        };
        let mut weapon = Record::new(SigCode(*b"WEAP"), key);
        for animation_type in 0..=9 {
            let mut bytes = vec![0; 100];
            bytes[0] = animation_type;
            weapon.fields = vec![FieldEntry {
                sig: SubrecordSig(*b"DNAM"),
                value: FieldValue::Bytes(bytes.into()),
            }]
            .into();
            assert_eq!(
                source_weapon_profile(weapon.clone(), &interner, |_| panic!("No template")),
                Ok((animation_type, false))
            );
        }
        for bytes in [vec![1; 99], vec![1; 204], vec![10; 100]] {
            weapon.fields[0].value = FieldValue::Bytes(bytes.into());
            assert!(
                source_weapon_profile(weapon.clone(), &interner, |_| panic!("No template"))
                    .is_err()
            );
        }
        weapon.fields[0].value = FieldValue::Struct(vec![(
            interner.intern("Animation Type"),
            FieldValue::Uint(7),
        )]);
        assert_eq!(
            source_weapon_profile(weapon, &interner, |_| panic!("No template")),
            Ok((7, false))
        );
    }

    #[test]
    fn source_weapon_templates_override_stale_local_data_and_reject_cycles() {
        let interner = StringInterner::new();
        let key = |local| FormKey {
            local,
            plugin: interner.intern("Skyrim.esm"),
        };
        let mut child = Record::new(SigCode(*b"WEAP"), key(1));
        child.fields = vec![
            FieldEntry {
                sig: SubrecordSig(*b"DNAM"),
                value: FieldValue::Struct(vec![(
                    interner.intern("animation_type"),
                    FieldValue::Uint(1),
                )]),
            },
            FieldEntry {
                sig: SubrecordSig(*b"CNAM"),
                value: FieldValue::FormKey(key(2)),
            },
        ]
        .into();
        let mut parent = Record::new(SigCode(*b"WEAP"), key(2));
        parent.fields = vec![FieldEntry {
            sig: SubrecordSig(*b"DNAM"),
            value: FieldValue::Struct(vec![(
                interner.intern("animation_type"),
                FieldValue::Uint(9),
            )]),
        }]
        .into();
        parent.fields.push(FieldEntry {
            sig: SubrecordSig(*b"KWDA"),
            value: FieldValue::List(vec![FieldValue::FormKey(FormKey {
                local: 0x15992,
                plugin: interner.intern("Dawnguard.esm"),
            })]),
        });
        assert_eq!(
            source_weapon_profile(child.clone(), &interner, |requested| {
                assert_eq!(requested, key(2));
                Ok(parent.clone())
            }),
            Ok((9, true))
        );
        parent.fields.push(FieldEntry {
            sig: SubrecordSig(*b"CNAM"),
            value: FieldValue::FormKey(key(1)),
        });
        assert!(
            source_weapon_profile(child.clone(), &interner, |requested| {
                Ok(if requested == key(1) {
                    child.clone()
                } else {
                    parent.clone()
                })
            })
            .is_err()
        );
    }
}
