//! Fixup: attach converted-but-unbound FO76 creature scripts to the records
//! that should run them.
//!
//! A `script_patches/` entry fills a body-stripped FO76 script, but the script
//! stays inert unless some record's `VMAD` names it, and for a few creatures
//! FO76 client data never does. This is not a blanket FO76 property: sibling
//! scripts (`Creatures:EyebotSuiciderScript`, `ScorchedSuiciderScript`,
//! `SuperMutantSuiciderScript`, `ProtectronSuiciderScript`) ship intact `NPC_`
//! bindings and must not be touched.
//!
//! # Floaters
//! `Creatures:FloaterScript` (`explosion Property DeathExplosion Auto
//! mandatory`) drives the death explosion from `OnDying`, but no floater `NPC_`
//! has a VMAD, so the three `crExplosionFloater*Death` records were orphans.
//! Every other floater chains to one of three roots through `DefaultTemplate`
//! with the `Script` template flag, so binding the roots covers all 50
//! descendants (vanilla `CovenantGenericM01`/`F01` inherit `workshopnpcscript`
//! from `CovenantGenericTemplate00` the same way).
//!
//! # Liberators
//! `Creatures:LiberatorRaceScript` (`Extends ActiveMagicEffect`) runs via
//! `LiberatorRace`'s `AbRaceLiberator`, but its effect `abLiberatorRaceEffect`
//! is Archetype `Script` with no VMAD in FO76 data. The script paces the laser:
//! it counts `WeaponFire` anim events and rests after a magazine.

use esp_authoring_core::plugin_runtime::build_vmad_bytes_from_payload;

use crate::fixups::creature::creature_internal_fixup_applies;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::{FormKey, SigCode, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::session::{EditOutcome, PluginSession};
use crate::sym::StringInterner;

const VMAD_VERSION: u16 = 6;
const VMAD_OBJECT_FORMAT: u16 = 2;
const VMAD_PROPERTY_FLAG_EDITED: u8 = 1;

/// One script property to write into the synthesized VMAD.
enum PropertyValue {
    /// An Object property. The target is verified by signature AND EditorID
    /// before anything is written, so a colliding local id in another source
    /// pair cannot satisfy it.
    Object {
        name: &'static str,
        target_sig: &'static str,
        target_local: u32,
        target_editor_id: &'static str,
    },
    Int {
        name: &'static str,
        value: i64,
    },
}

impl PropertyValue {
    fn name(&self) -> &'static str {
        match self {
            PropertyValue::Object { name, .. } | PropertyValue::Int { name, .. } => name,
        }
    }
}

struct ScriptBinding {
    record_sig: &'static str,
    record_editor_id: &'static str,
    script: &'static str,
    properties: &'static [PropertyValue],
}

/// Record signatures the table touches, in the order they are swept.
const SWEPT_SIGS: &[&str] = &["NPC_", "MGEF"];

const BINDINGS: &[ScriptBinding] = &[
    ScriptBinding {
        record_sig: "NPC_",
        record_editor_id: "EncFloaterFlamer01Template",
        script: "Creatures:FloaterScript",
        properties: &[PropertyValue::Object {
            name: "DeathExplosion",
            target_sig: "EXPL",
            target_local: 0x0055_DA71,
            target_editor_id: "crExplosionFloaterFlamerDeath",
        }],
    },
    ScriptBinding {
        record_sig: "NPC_",
        record_editor_id: "EncFloaterFreezer01Template",
        script: "Creatures:FloaterScript",
        properties: &[PropertyValue::Object {
            name: "DeathExplosion",
            target_sig: "EXPL",
            target_local: 0x0055_DA6F,
            target_editor_id: "crExplosionFloaterFreezerDeath",
        }],
    },
    ScriptBinding {
        record_sig: "NPC_",
        record_editor_id: "EncFloaterGnasher01Template",
        script: "Creatures:FloaterScript",
        properties: &[PropertyValue::Object {
            name: "DeathExplosion",
            target_sig: "EXPL",
            target_local: 0x0055_DA72,
            target_editor_id: "crExplosionFloaterGnasherDeath",
        }],
    },
    ScriptBinding {
        record_sig: "MGEF",
        record_editor_id: "abLiberatorRaceEffect",
        script: "Creatures:LiberatorRaceScript",
        properties: &[
            PropertyValue::Object {
                name: "LiberatorRangedWeapon",
                target_sig: "WEAP",
                target_local: 0x0010_D80A,
                target_editor_id: "crLiberatorLaserGun",
            },
            // The weapon's own `Capacity` is 3, so a magazine is three bolts.
            PropertyValue::Int {
                name: "WeaponFireMaxShots",
                value: 3,
            },
            // Seconds. FO76 authored no value anywhere — nothing binds this
            // script in the source game — so this is a chosen pace, roughly two
            // attack animations (`AnimationAttackSeconds` 1.54) of rest.
            PropertyValue::Int {
                name: "WeaponFireRestTime",
                value: 3,
            },
        ],
    },
];

pub struct BindCreatureScriptsFixup;

impl Fixup for BindCreatureScriptsFixup {
    fn name(&self) -> &'static str {
        "bind_creature_scripts"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::CreatureGated
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        creature_internal_fixup_applies(ctx.config)
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        creature_internal_fixup_applies(config)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let interner = mapper.interner;

        let target_master_names = session.target_masters().to_vec();
        let target_plugin_name = session.target_slot().parsed.plugin_name.clone();

        let mut total = FixupReport::empty();
        let mut warnings = Vec::new();

        for sig_str in SWEPT_SIGS {
            let sig =
                SigCode::from_str(sig_str).map_err(|e| FixupError::SchemaError(e.to_string()))?;
            let report = session.map_apply_by_sig(
                sig,
                mapper,
                |view, _snapshot, fk| {
                    let record = view.record_decoded(fk, target_schema, interner).ok()?;
                    let binding = binding_for(&record, sig_str, interner)?;
                    if record_has_vmad(&record) {
                        return Some(BindEdit::Warn(format!(
                            "bind_creature_scripts:{}:existing_vmad_left_alone",
                            binding.record_editor_id
                        )));
                    }
                    // Resolve every Object target first. Writing a VMAD whose
                    // `mandatory` property dangles is worse than no script.
                    let mut resolved = Vec::new();
                    for property in binding.properties {
                        let PropertyValue::Object {
                            target_sig,
                            target_local,
                            target_editor_id,
                            ..
                        } = property
                        else {
                            continue;
                        };
                        let target = FormKey {
                            local: *target_local,
                            plugin: record.form_key.plugin,
                        };
                        match view.record_decoded(&target, target_schema, interner) {
                            Ok(found)
                                if is_named_record(
                                    &found,
                                    target_sig,
                                    target_editor_id,
                                    interner,
                                ) =>
                            {
                                resolved.push((property.name(), target));
                            }
                            _ => {
                                return Some(BindEdit::Warn(format!(
                                    "bind_creature_scripts:{}:{}_unavailable",
                                    binding.record_editor_id, target_editor_id
                                )));
                            }
                        }
                    }
                    Some(BindEdit::Bind {
                        record,
                        binding,
                        resolved,
                    })
                },
                |session, mapper, _fk, edit| match edit {
                    BindEdit::Bind {
                        mut record,
                        binding,
                        resolved,
                    } => {
                        let Some(vmad) = script_vmad(
                            binding,
                            &resolved,
                            &target_master_names,
                            &target_plugin_name,
                            mapper.interner,
                        ) else {
                            warnings.push(mapper.interner.intern(&format!(
                                "bind_creature_scripts:{}:vmad_encode_failed",
                                binding.record_editor_id
                            )));
                            return Ok(EditOutcome::NoOp);
                        };
                        let vmad_sig = SubrecordSig::from_str("VMAD")
                            .map_err(|e| FixupError::SchemaError(e.to_string()))?;
                        // FO4 carries VMAD ahead of every other subrecord.
                        record.fields.insert(
                            0,
                            FieldEntry {
                                sig: vmad_sig,
                                value: FieldValue::Bytes(vmad.into()),
                            },
                        );
                        session
                            .replace_record(record, target_schema, mapper.interner)
                            .map_err(|e| FixupError::HandleError(e.to_string()))?;
                        Ok(EditOutcome::Changed)
                    }
                    BindEdit::Warn(message) => {
                        warnings.push(mapper.interner.intern(&message));
                        Ok(EditOutcome::NoOp)
                    }
                },
            )?;
            total.records_changed += report.records_changed;
            total.records_dropped += report.records_dropped;
            total.records_added += report.records_added;
            total.warnings.extend(report.warnings);
            total.diagnostics.extend(report.diagnostics);
        }

        total.warnings.extend(warnings);
        Ok(total)
    }
}

enum BindEdit {
    Bind {
        record: Record,
        binding: &'static ScriptBinding,
        resolved: Vec<(&'static str, FormKey)>,
    },
    Warn(String),
}

fn binding_for(
    record: &Record,
    sig: &str,
    interner: &StringInterner,
) -> Option<&'static ScriptBinding> {
    let eid = interner.resolve(record.eid?)?;
    BINDINGS.iter().find(|binding| {
        binding.record_sig == sig && binding.record_editor_id.eq_ignore_ascii_case(eid)
    })
}

fn record_has_vmad(record: &Record) -> bool {
    let Ok(vmad_sig) = SubrecordSig::from_str("VMAD") else {
        return false;
    };
    record.fields.iter().any(|entry| entry.sig == vmad_sig)
}

fn is_named_record(record: &Record, sig: &str, editor_id: &str, interner: &StringInterner) -> bool {
    let Ok(expected) = SigCode::from_str(sig) else {
        return false;
    };
    record.sig == expected
        && record
            .eid
            .and_then(|sym| interner.resolve(sym))
            .is_some_and(|eid| eid.eq_ignore_ascii_case(editor_id))
}

fn script_vmad(
    binding: &ScriptBinding,
    resolved: &[(&str, FormKey)],
    target_masters: &[String],
    target_plugin: &str,
    interner: &StringInterner,
) -> Option<Vec<u8>> {
    let mut properties = Vec::with_capacity(binding.properties.len());
    for property in binding.properties {
        properties.push(match property {
            PropertyValue::Object { name, .. } => {
                let form_key = resolved
                    .iter()
                    .find_map(|(n, fk)| (n == name).then_some(*fk))?;
                let plugin = interner.resolve(form_key.plugin)?;
                serde_json::json!({
                    "propertyName": name,
                    "Type": "Object",
                    "Flags": VMAD_PROPERTY_FLAG_EDITED,
                    "Value": {
                        "Alias": -1,
                        "FormID": {
                            "reference": {
                                "plugin": plugin,
                                "object_id": format!("{:06X}", form_key.local),
                            },
                        },
                    },
                })
            }
            PropertyValue::Int { name, value } => serde_json::json!({
                "propertyName": name,
                "Type": "Int32",
                "Flags": VMAD_PROPERTY_FLAG_EDITED,
                "Value": value,
            }),
        });
    }
    build_vmad_bytes_from_payload(
        &serde_json::json!({
            "Version": VMAD_VERSION,
            "Object Format": VMAD_OBJECT_FORMAT,
            "Scripts": [{
                "ScriptName": binding.script,
                "Flags": 0,
                "Properties": properties,
            }],
        }),
        target_masters,
        target_plugin,
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::record::RecordFlags;
    use smallvec::SmallVec;

    fn record(sig: &str, eid: &str, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str(sig).unwrap(),
            form_key: FormKey {
                local: 0x0055_DA58,
                plugin: interner.intern("SeventySix.esm"),
            },
            eid: Some(interner.intern(eid)),
            flags: RecordFlags::empty(),
            fields: SmallVec::new(),
            warnings: SmallVec::new(),
        }
    }

    fn vmad_for(eid: &str, sig: &str, interner: &StringInterner) -> Vec<u8> {
        let binding = binding_for(&record(sig, eid, interner), sig, interner)
            .unwrap_or_else(|| panic!("no binding for {eid}"));
        let plugin = interner.intern("SeventySix.esm");
        let resolved: Vec<(&str, FormKey)> = binding
            .properties
            .iter()
            .filter_map(|p| match p {
                PropertyValue::Object {
                    name, target_local, ..
                } => Some((
                    *name,
                    FormKey {
                        local: *target_local,
                        plugin,
                    },
                )),
                PropertyValue::Int { .. } => None,
            })
            .collect();
        script_vmad(
            binding,
            &resolved,
            &["Fallout4.esm".to_string()],
            "SeventySix.esm",
            interner,
        )
        .expect("vmad encodes")
    }

    fn as_text(bytes: &[u8]) -> String {
        bytes.iter().map(|&b| b as char).collect()
    }

    #[test]
    fn matches_each_floater_template_to_its_own_explosion() {
        let interner = StringInterner::new();
        let pairs = [
            ("EncFloaterFlamer01Template", 0x0055_DA71),
            ("EncFloaterFreezer01Template", 0x0055_DA6F),
            ("EncFloaterGnasher01Template", 0x0055_DA72),
        ];
        for (eid, expected) in pairs {
            let binding = binding_for(&record("NPC_", eid, &interner), "NPC_", &interner)
                .unwrap_or_else(|| panic!("no binding for {eid}"));
            let PropertyValue::Object { target_local, .. } = binding.properties[0] else {
                panic!("{eid} should carry an Object property");
            };
            assert_eq!(target_local, expected);
        }
    }

    #[test]
    fn leaves_templated_children_and_unrelated_npcs_alone() {
        let interner = StringInterner::new();
        for eid in ["EncFloaterFlamer02", "LvlFloaterFlamer", "EncRaider01"] {
            assert!(
                binding_for(&record("NPC_", eid, &interner), "NPC_", &interner).is_none(),
                "{eid} should not be bound directly"
            );
        }
    }

    #[test]
    fn a_binding_only_matches_its_own_record_signature() {
        let interner = StringInterner::new();
        // The liberator row is an MGEF; an NPC_ of the same EditorID must miss.
        assert!(
            binding_for(
                &record("MGEF", "abLiberatorRaceEffect", &interner),
                "MGEF",
                &interner
            )
            .is_some()
        );
        assert!(
            binding_for(
                &record("NPC_", "abLiberatorRaceEffect", &interner),
                "NPC_",
                &interner
            )
            .is_none()
        );
    }

    #[test]
    fn object_target_check_rejects_a_colliding_local_id() {
        let interner = StringInterner::new();
        assert!(is_named_record(
            &record("EXPL", "crExplosionFloaterFlamerDeath", &interner),
            "EXPL",
            "crExplosionFloaterFlamerDeath",
            &interner,
        ));
        // Right EditorID, wrong record type.
        assert!(!is_named_record(
            &record("HAZD", "crExplosionFloaterFlamerDeath", &interner),
            "EXPL",
            "crExplosionFloaterFlamerDeath",
            &interner,
        ));
        // Right record type, some other record at that local id.
        assert!(!is_named_record(
            &record("EXPL", "SomeOtherExplosion", &interner),
            "EXPL",
            "crExplosionFloaterFlamerDeath",
            &interner,
        ));
    }

    #[test]
    fn floater_vmad_carries_the_script_and_its_mandatory_property() {
        let interner = StringInterner::new();
        let text = as_text(&vmad_for("EncFloaterFlamer01Template", "NPC_", &interner));
        assert!(
            text.contains("Creatures:FloaterScript"),
            "script name missing"
        );
        assert!(
            text.contains("DeathExplosion"),
            "mandatory property missing"
        );
    }

    #[test]
    fn liberator_vmad_carries_all_three_mandatory_properties() {
        let interner = StringInterner::new();
        let bytes = vmad_for("abLiberatorRaceEffect", "MGEF", &interner);
        let text = as_text(&bytes);
        assert!(text.contains("Creatures:LiberatorRaceScript"));
        for name in [
            "LiberatorRangedWeapon",
            "WeaponFireMaxShots",
            "WeaponFireRestTime",
        ] {
            assert!(text.contains(name), "{name} missing from the encoded VMAD");
        }
        // The magazine size has to survive as an Int32, not silently drop.
        assert!(
            bytes.windows(4).any(|w| w == 3i32.to_le_bytes()),
            "WeaponFireMaxShots value not encoded"
        );
    }
}
