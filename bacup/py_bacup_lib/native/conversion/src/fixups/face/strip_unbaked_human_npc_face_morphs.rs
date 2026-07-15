//! Strip unsafe humanoid face deformation payloads when no baked FaceGeom is known.
//!
//! The whole-plugin FO76->FO4 regen path does not run `convert_face`, but can
//! still carry NPC `MSDK`/`FMRI` sculpt data. FO4 then has to build the converted
//! head at runtime. For converted FO4 humanoid NPCs without a matching FaceGeom
//! NIF or without enough path context to prove one exists, that path can crash
//! in `BSFaceGenNiNodeSkinned`. Keep the FO4-safe head parts, hair color, and
//! head texture, but drop the deformation rows that require a baked compatible
//! face mesh.

use std::path::{Path, PathBuf};

use crate::fixups::prune_orphaned_records::is_creature_root_sig;
use crate::fixups::{Fixup, FixupConfig, FixupContext, FixupError, FixupReport};
use crate::formkey_mapper::FormKeyMapper;
use crate::full_plugin::FixupScope;
use crate::ids::SigCode;
use crate::record::{FieldValue, Record};
use crate::session::PluginSession;
use crate::sym::StringInterner;

const BASE_MASTER: &str = "Fallout4.esm";
const HUMAN_RACE_LOCAL: u32 = 0x0001_3746;
const GHOUL_RACE_LOCAL: u32 = 0x000E_AFB6;
const FACE_DEFORMATION_SIGS: &[&str] = &["MSDK", "MSDV", "MRSV", "FMRI", "FMRS", "FMIN"];

pub struct StripUnbakedHumanNpcFaceMorphsFixup;

impl Fixup for StripUnbakedHumanNpcFaceMorphsFixup {
    fn name(&self) -> &'static str {
        "strip_unbaked_human_npc_face_morphs"
    }

    fn scope(&self) -> FixupScope {
        FixupScope::WholePluginSafe
    }

    fn uses_session(&self) -> bool {
        true
    }

    fn applies_to(&self, ctx: &FixupContext) -> bool {
        applies_for_root(ctx.config.root_sig)
    }

    fn applies_to_session(&self, _session: &PluginSession, config: &FixupConfig) -> bool {
        applies_for_root(config.root_sig)
    }

    fn run_with_session(
        &self,
        session: &mut PluginSession,
        mapper: &mut FormKeyMapper,
        config: &FixupConfig,
    ) -> Result<FixupReport, FixupError> {
        let mut report = FixupReport::empty();
        let target_schema = config
            .target_schema
            .as_deref()
            .ok_or_else(|| FixupError::Other("missing target schema in fixup config".into()))?;
        let npc_sig =
            SigCode::from_str("NPC_").map_err(|e| FixupError::SchemaError(e.to_string()))?;
        let npc_fks = session
            .form_keys_of_sig(npc_sig, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if npc_fks.is_empty() {
            return Ok(report);
        }

        let mut changed_records = Vec::new();
        for fk in npc_fks {
            let mut record = match session.record_decoded(&fk, target_schema, mapper.interner) {
                Ok(record) => record,
                Err(e) => {
                    report.warnings.push(
                        mapper
                            .interner
                            .intern(&format!("strip_unbaked_human_face_read:{e}")),
                    );
                    continue;
                }
            };
            let plugin_name = mapper
                .interner
                .resolve(record.form_key.plugin)
                .unwrap_or_default();
            let has_facegeom = known_matching_facegeom_exists(
                config.mod_path.as_deref(),
                plugin_name,
                record.form_key.local & 0x00FF_FFFF,
            );
            if strip_unbaked_human_face_morphs(&mut record, mapper.interner, has_facegeom) {
                changed_records.push(record);
            }
        }

        let expected = changed_records.len();
        if expected == 0 {
            return Ok(report);
        }
        let replaced = session
            .replace_records_contents(changed_records, target_schema, mapper.interner)
            .map_err(|e| FixupError::HandleError(e.to_string()))?;
        if replaced != expected {
            return Err(FixupError::HandleError(format!(
                "strip_unbaked_human_npc_face_morphs replaced {replaced} of {expected} expected records"
            )));
        }
        report.records_changed = replaced.try_into().unwrap_or(u32::MAX);
        Ok(report)
    }
}

fn applies_for_root(root_sig: Option<SigCode>) -> bool {
    match root_sig {
        Some(sig) => is_creature_root_sig(sig),
        None => true,
    }
}

fn strip_unbaked_human_face_morphs(
    record: &mut Record,
    interner: &StringInterner,
    has_facegeom: bool,
) -> bool {
    if has_facegeom
        || !has_fo4_runtime_facegen_humanoid_race(record, interner)
        || !has_face_deformation(record)
    {
        return false;
    }

    let before = record.fields.len();
    record
        .fields
        .retain(|entry| !FACE_DEFORMATION_SIGS.contains(&entry.sig.as_str()));
    record.fields.len() != before
}

fn has_face_deformation(record: &Record) -> bool {
    record
        .fields
        .iter()
        .any(|entry| FACE_DEFORMATION_SIGS.contains(&entry.sig.as_str()))
}

fn has_fo4_runtime_facegen_humanoid_race(record: &Record, interner: &StringInterner) -> bool {
    record.fields.iter().any(|entry| {
        if entry.sig.as_str() != "RNAM" {
            return false;
        }
        let FieldValue::FormKey(fk) = &entry.value else {
            return false;
        };
        matches!(fk.local, HUMAN_RACE_LOCAL | GHOUL_RACE_LOCAL)
            && interner
                .resolve(fk.plugin)
                .is_some_and(|plugin| plugin.eq_ignore_ascii_case(BASE_MASTER))
    })
}

fn known_matching_facegeom_exists(mod_path: Option<&Path>, plugin_name: &str, local: u32) -> bool {
    mod_path.is_some_and(|path| facegeom_exists(path, plugin_name, local))
}

fn facegeom_exists(mod_path: &Path, plugin_name: &str, local: u32) -> bool {
    facegeom_path_candidates(mod_path, plugin_name, local)
        .iter()
        .any(|path| path.exists())
}

fn facegeom_path_candidates(mod_path: &Path, plugin_name: &str, local: u32) -> Vec<PathBuf> {
    let lower_plugin = plugin_name.to_ascii_lowercase();
    let file_lower = format!("{local:08x}.nif");
    let file_upper = format!("{local:08X}.nif");
    let roots: &[&[&str]] = &[
        &[
            "data",
            "Meshes",
            "Actors",
            "Character",
            "FaceGenData",
            "FaceGeom",
        ],
        &[
            "data",
            "Meshes",
            "actors",
            "character",
            "facegendata",
            "facegeom",
        ],
        &[
            "data",
            "meshes",
            "actors",
            "character",
            "facegendata",
            "facegeom",
        ],
        &["Meshes", "Actors", "Character", "FaceGenData", "FaceGeom"],
        &["meshes", "actors", "character", "facegendata", "facegeom"],
    ];

    let mut candidates = Vec::new();
    for root_parts in roots {
        let root = root_parts
            .iter()
            .fold(mod_path.to_path_buf(), |path, part| path.join(part));
        for plugin_dir in [plugin_name, lower_plugin.as_str()] {
            for file_name in [file_lower.as_str(), file_upper.as_str()] {
                candidates.push(root.join(plugin_dir).join(file_name));
            }
        }
    }
    candidates
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SubrecordSig};
    use crate::record::{FieldEntry, RecordFlags};

    fn record(fields: Vec<(&str, FieldValue)>, interner: &StringInterner) -> Record {
        Record {
            sig: SigCode::from_str("NPC_").unwrap(),
            form_key: FormKey {
                plugin: interner.intern("SeventySix.esm"),
                local: 0x0058_58E7,
            },
            eid: None,
            flags: RecordFlags::empty(),
            fields: fields
                .into_iter()
                .map(|(sig, value)| FieldEntry {
                    sig: SubrecordSig::from_str(sig).unwrap(),
                    value,
                })
                .collect(),
            warnings: smallvec::SmallVec::new(),
        }
    }

    fn human_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: HUMAN_RACE_LOCAL,
        })
    }

    fn ghoul_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: GHOUL_RACE_LOCAL,
        })
    }

    fn other_race(interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(BASE_MASTER),
            local: 0x0002_0000,
        })
    }

    fn formkey(plugin: &str, local: u32, interner: &StringInterner) -> FieldValue {
        FieldValue::FormKey(FormKey {
            plugin: interner.intern(plugin),
            local,
        })
    }

    fn bytes() -> FieldValue {
        FieldValue::Bytes(smallvec::SmallVec::new())
    }

    fn sigs(record: &Record) -> Vec<&str> {
        record
            .fields
            .iter()
            .map(|entry| entry.sig.as_str())
            .collect()
    }

    #[test]
    fn strips_deformation_fields_from_unbaked_human_npc() {
        let interner = StringInterner::new();
        let mut npc = record(
            vec![
                ("EDID", FieldValue::String(interner.intern("TestNpc"))),
                ("RNAM", human_race(&interner)),
                ("PNAM", formkey(BASE_MASTER, 0x0005_1631, &interner)),
                ("HCLF", formkey(BASE_MASTER, 0x0019_EE5E, &interner)),
                ("FTST", formkey(BASE_MASTER, 0x0016_F83D, &interner)),
                ("MSDK", bytes()),
                ("MSDV", bytes()),
                ("MRSV", bytes()),
                ("FMRI", FieldValue::Uint(0)),
                ("FMRS", bytes()),
                ("FMIN", FieldValue::Float(1.2)),
            ],
            &interner,
        );

        assert!(strip_unbaked_human_face_morphs(&mut npc, &interner, false));
        assert_eq!(sigs(&npc), vec!["EDID", "RNAM", "PNAM", "HCLF", "FTST"]);
    }

    #[test]
    fn preserves_human_npc_when_matching_facegeom_exists() {
        let interner = StringInterner::new();
        let mut npc = record(
            vec![("RNAM", human_race(&interner)), ("MSDK", bytes())],
            &interner,
        );

        assert!(!strip_unbaked_human_face_morphs(&mut npc, &interner, true));
        assert_eq!(sigs(&npc), vec!["RNAM", "MSDK"]);
    }

    #[test]
    fn strips_deformation_fields_from_unbaked_ghoul_npc() {
        let interner = StringInterner::new();
        let mut npc = record(
            vec![
                ("RNAM", ghoul_race(&interner)),
                ("MSDK", bytes()),
                ("MSDV", bytes()),
                ("FMRI", FieldValue::Uint(100005)),
                ("FMRS", bytes()),
                ("FMIN", FieldValue::Float(2.0)),
            ],
            &interner,
        );

        assert!(strip_unbaked_human_face_morphs(&mut npc, &interner, false));
        assert_eq!(sigs(&npc), vec!["RNAM"]);
    }

    #[test]
    fn ignores_non_human_npc() {
        let interner = StringInterner::new();
        let mut npc = record(
            vec![("RNAM", other_race(&interner)), ("MSDK", bytes())],
            &interner,
        );

        assert!(!strip_unbaked_human_face_morphs(&mut npc, &interner, false));
        assert_eq!(sigs(&npc), vec!["RNAM", "MSDK"]);
    }

    #[test]
    fn runs_without_mod_path_context() {
        assert!(applies_for_root(None));
        assert!(!known_matching_facegeom_exists(
            None,
            "SeventySix.esm",
            0x0058_58E7
        ));
    }

    #[test]
    fn finds_lowercase_loose_facegeom_path() {
        let dir = tempfile::tempdir().unwrap();
        let facegeom_dir = dir
            .path()
            .join("data")
            .join("Meshes")
            .join("actors")
            .join("character")
            .join("facegendata")
            .join("facegeom")
            .join("seventysix.esm");
        std::fs::create_dir_all(&facegeom_dir).unwrap();
        std::fs::write(facegeom_dir.join("005858e7.nif"), b"nif").unwrap();

        assert!(facegeom_exists(dir.path(), "SeventySix.esm", 0x0058_58E7));
    }
}
