use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::record::{FieldValue, Record};

use super::creature_mvp_adapter::SkyrimCreatureConvertedArtifact;

mod ports;
pub(crate) mod traps;

#[derive(serde::Serialize, serde::Deserialize)]
struct AttackScriptReceipt {
    source_script: String,
    source_pex_blake3: String,
    source_manifest: super::papyrus::GeneratedPscManifestRow,
    runtime_path: String,
    pex_blake3: String,
    target_game: String,
}

pub(crate) fn target_script_name(source: &str) -> Option<&'static str> {
    ports::SCRIPT_PORTS
        .iter()
        .chain(traps::SCRIPT_PORTS)
        .find_map(|(original, target)| original.eq_ignore_ascii_case(source).then_some(*target))
}

pub(crate) fn required_attack_scripts(record: &Record) -> Result<BTreeSet<String>, String> {
    let mut required = BTreeSet::new();
    if !matches!(&record.sig.0, b"MGEF" | b"QUST" | b"ACTI") {
        return Ok(required);
    }
    for field in &record.fields {
        if field.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &field.value else {
            return Err("creature attack VMAD is not binary".to_string());
        };
        let mut bytes = bytes.to_vec();
        crate::fixups::quest_script_vmad::map_top_level_script_names(&mut bytes, &mut |name| {
            if target_script_name(name).is_some() {
                required.insert(name.to_ascii_lowercase());
            }
            None
        })
        .ok_or_else(|| "cannot read creature attack VMAD script bindings".to_string())?;
    }
    Ok(required)
}

pub(crate) fn has_unported_attack_scripts(record: &Record) -> bool {
    let mut unsupported = false;
    for field in record.fields.iter().filter(|field| field.sig.0 == *b"VMAD") {
        let FieldValue::Bytes(bytes) = &field.value else {
            return true;
        };
        let mut bytes = bytes.to_vec();
        if crate::fixups::quest_script_vmad::map_top_level_script_names(&mut bytes, &mut |name| {
            unsupported |= target_script_name(name).is_none();
            None
        })
        .is_none()
        {
            return true;
        }
    }
    unsupported
}

pub(crate) fn attack_script_form_references(
    record: &Record,
    plugin_name: &str,
    masters: &[String],
    interner: &crate::sym::StringInterner,
) -> Result<Vec<(String, crate::ids::FormKey)>, String> {
    if required_attack_scripts(record)?.is_empty() {
        return Ok(Vec::new());
    }
    fn collect(
        value: &serde_json::Value,
        path: String,
        interner: &crate::sym::StringInterner,
        output: &mut Vec<(String, crate::ids::FormKey)>,
    ) -> Result<(), String> {
        match value {
            serde_json::Value::Object(object) => {
                if let Some(reference) = object.get("reference") {
                    let plugin = reference
                        .get("plugin")
                        .and_then(serde_json::Value::as_str)
                        .ok_or_else(|| format!("unresolved source VMAD master at {path}"))?;
                    let local = reference
                        .get("object_id")
                        .and_then(serde_json::Value::as_str)
                        .and_then(|value| u32::from_str_radix(value, 16).ok())
                        .ok_or_else(|| format!("invalid source VMAD FormID at {path}"))?;
                    output.push((
                        path,
                        crate::ids::FormKey {
                            plugin: interner.intern(plugin),
                            local,
                        },
                    ));
                } else {
                    for (name, value) in object {
                        collect(value, format!("{path}.{name}"), interner, output)?;
                    }
                }
            }
            serde_json::Value::Array(values) => {
                for (index, value) in values.iter().enumerate() {
                    collect(value, format!("{path}[{index}]"), interner, output)?;
                }
            }
            _ => {}
        }
        Ok(())
    }
    let mut references = Vec::new();
    for (index, field) in record
        .fields
        .iter()
        .enumerate()
        .filter(|(_, field)| field.sig.0 == *b"VMAD")
    {
        let FieldValue::Bytes(bytes) = &field.value else {
            return Err("creature attack VMAD is not binary".to_string());
        };
        let mut filtered = bytes.to_vec();
        if record.sig.0 == *b"QUST" {
            crate::fixups::quest_script_vmad::retain_standalone_scripts(&mut filtered, &|name| {
                name.eq_ignore_ascii_case("MG01QuestScript")
            })
            .ok_or("cannot read creature callback quest VMAD")?;
        }
        let payload = esp_authoring_core::plugin_runtime::authoring::authoring_serialize::compact_vmad_payload_json(&filtered, masters, plugin_name, Some("MGEF"))
            .ok_or_else(|| "cannot resolve creature attack script properties".to_string())?;
        collect(
            &payload,
            format!("VMAD[{index}]"),
            interner,
            &mut references,
        )?;
    }
    Ok(references)
}

pub(crate) fn rewrite_attack_script_bindings(record: &mut Record) -> Result<(), String> {
    if !matches!(&record.sig.0, b"MGEF" | b"QUST" | b"ACTI") {
        return Ok(());
    }
    for field in &mut record.fields {
        if field.sig.0 != *b"VMAD" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &mut field.value else {
            return Err("creature attack VMAD is not binary".to_string());
        };
        let mut rewritten = bytes.to_vec();
        crate::fixups::quest_script_vmad::map_top_level_script_names(&mut rewritten, &mut |name| {
            target_script_name(name).map(str::to_owned)
        })
        .ok_or_else(|| "cannot rewrite creature attack VMAD script bindings".to_string())?;
        *bytes = rewritten.into();
    }
    Ok(())
}

pub(crate) fn retain_creature_callback_quest_script(record: &mut Record) -> Result<bool, String> {
    if record.sig.0 != *b"QUST" {
        return Ok(false);
    }
    let mut retained = false;
    for field in record
        .fields
        .iter_mut()
        .filter(|field| field.sig.0 == *b"VMAD")
    {
        let FieldValue::Bytes(bytes) = &mut field.value else {
            return Err("callback quest VMAD is not binary".into());
        };
        let mut filtered = bytes.to_vec();
        let count =
            crate::fixups::quest_script_vmad::retain_standalone_scripts(&mut filtered, &|name| {
                name.eq_ignore_ascii_case("MG01QuestScript")
            })
            .ok_or("cannot preserve creature callback quest script")?;
        retained |= count != 0;
        *bytes = filtered.into();
    }
    Ok(retained)
}

pub(crate) fn bind_attack_script_actor_values(
    record: &mut Record,
    mapper: &crate::formkey_mapper::FormKeyMapper<'_>,
    interner: &crate::sym::StringInterner,
) -> Result<(), String> {
    if record.sig.0 != *b"MGEF" {
        return Ok(());
    }
    for field in record
        .fields
        .iter_mut()
        .filter(|field| field.sig.0 == *b"VMAD")
    {
        let FieldValue::Bytes(bytes) = &mut field.value else {
            return Err("creature script VMAD is not binary".into());
        };
        let mut rewritten = bytes.to_vec();
        crate::fixups::quest_script_vmad::add_top_level_object_properties(
            &mut rewritten,
            &mut |script| {
                ports::actor_values(script)
                    .iter()
                    .map(|name| {
                        let local = match *name {
                            "Health" => 0x2D4,
                            "Aggression" => 0x2BC,
                            "Confidence" => 0x2BD,
                            "Variable06" => 0x306,
                            "Variable07" => 0x307,
                            "Magicka" => 0,
                            _ => return None,
                        };
                        let target = if local != 0 {
                            crate::ids::FormKey {
                                plugin: interner.intern("Fallout4.esm"),
                                local,
                            }
                        } else {
                            let source = super::magic_resources::magic_resource_source_key(
                                &FieldValue::String(interner.intern(name)),
                                interner,
                            )?;
                            mapper.lookup(source)?
                        };
                        Some((
                            format!("B21AV_{name}"),
                            mapper.raw_formid_for_target(target)?,
                        ))
                    })
                    .collect()
            },
        )
        .ok_or_else(|| {
            "cannot bind creature script actor value properties after FormID mapping".to_string()
        })?;
        *bytes = rewritten.into();
    }
    Ok(())
}

fn port_source(source_name: &str, bytes: &[u8]) -> Result<String, String> {
    let target = target_script_name(source_name)
        .ok_or_else(|| format!("unsupported Skyrim attack script {source_name}"))?;
    let pex = papyrus_core::pex::parse_pex_bytes(bytes)?;
    if pex.game_id != 1
        || pex.objects.len() != 1
        || !pex.objects[0].name.eq_ignore_ascii_case(source_name)
        || !pex.objects[0].parent.eq_ignore_ascii_case(
            if let Some(parent) = traps::source_parent(source_name) {
                parent
            } else if source_name.eq_ignore_ascii_case("MG01QuestScript") {
                "Quest"
            } else {
                "ActiveMagicEffect"
            },
        )
    {
        return Err(format!(
            "unexpected Skyrim attack PEX class for {source_name}"
        ));
    }
    let mut ast = papyrus_core::decompiler::decompile(
        &pex,
        &papyrus_core::decompiler::DecompileOptions {
            type_aliases: ports::SCRIPT_PORTS
                .iter()
                .chain(traps::SCRIPT_PORTS)
                .map(|(source, target)| (source.to_ascii_lowercase(), target.to_string()))
                .collect(),
            skip_internal_functions: true,
            fo4_api_compat: true,
            ..Default::default()
        },
    )?;
    ast.name = target.to_string();
    if traps::source_parent(source_name).is_some() {
        traps::adapt(&mut ast)?;
    } else {
        ports::adapt(&mut ast)?;
    }
    Ok(papyrus_core::emitter::emit_script(&ast))
}

pub(crate) fn stage_attack_scripts(
    names: &BTreeSet<String>,
    source_data_root: &Path,
    staging_root: &Path,
    imports: &[String],
    flags: Option<&str>,
) -> Result<BTreeMap<String, SkyrimCreatureConvertedArtifact>, String> {
    let mut artifacts = BTreeMap::new();
    let mut receipts = Vec::new();
    std::fs::create_dir_all(staging_root).map_err(|error| error.to_string())?;
    let mut names = names.clone();
    if names
        .iter()
        .any(|name| name.eq_ignore_ascii_case("MG01FireEffectScript"))
    {
        names.insert("mg01questscript".into());
    }
    loop {
        let dependencies = names
            .iter()
            .flat_map(|name| traps::dependencies(name))
            .map(|name| name.to_string())
            .collect::<Vec<_>>();
        let previous_count = names.len();
        names.extend(dependencies);
        if names.len() == previous_count {
            break;
        }
    }
    let mut sources = BTreeMap::new();
    for source_name in &names {
        let target_name = target_script_name(source_name)
            .ok_or_else(|| format!("unsupported Skyrim attack script {source_name}"))?;
        let source_path = source_data_root.join(format!("Scripts/{source_name}.pex"));
        let source_bytes = std::fs::read(&source_path)
            .map_err(|error| format!("read {}: {error}", source_path.display()))?;
        let source = port_source(source_name, &source_bytes)?;
        let source_artifact = super::papyrus::SkyrimPscSourceArtifact {
            class_name: target_name.to_string(),
            source,
            kind: super::papyrus::SkyrimPscKind::RecordScript,
            required: true,
        };
        sources.insert(source_name.clone(), (source_bytes, source_artifact));
    }
    let artifacts_to_emit: Vec<_> = sources
        .values()
        .map(|(_, artifact)| artifact.clone())
        .collect();
    let manifests = super::papyrus::emit_script_sources(staging_root, &artifacts_to_emit)?;
    let mut batch_imports = vec![
        staging_root
            .join("Scripts/Source/User")
            .to_string_lossy()
            .into_owned(),
    ];
    batch_imports.extend_from_slice(imports);
    for ((source_name, (source_bytes, source_artifact)), manifest) in
        sources.into_iter().zip(manifests)
    {
        let target_name = &source_artifact.class_name;
        let compiled = papyrus_core::compiler::compile_source_with_path(
            &source_artifact.source,
            &batch_imports,
            papyrus_core::profile::Game::Fo4,
            flags,
            Some(
                &staging_root
                    .join(&manifest.relative_source_path)
                    .to_string_lossy(),
            ),
        );
        let bytes = compiled.pex_bytes.filter(|_| compiled.ok).ok_or_else(|| {
            format!(
                "compile Skyrim attack script {source_name}: {:?}",
                compiled.diagnostics
            )
        })?;
        let pex = papyrus_core::pex::parse_pex_bytes(&bytes)?;
        if pex.game_id != 2 || pex.objects.len() != 1 || pex.objects[0].name != *target_name {
            return Err(format!(
                "invalid Fallout 4 attack script output for {source_name}"
            ));
        }
        let runtime_path = format!("Scripts/{target_name}.pex");
        let output = staging_root.join("data").join(&runtime_path);
        std::fs::create_dir_all(output.parent().unwrap()).map_err(|error| error.to_string())?;
        std::fs::write(&output, &bytes).map_err(|error| error.to_string())?;
        let receipt = AttackScriptReceipt {
            source_script: source_name.clone(),
            source_pex_blake3: blake3::hash(&source_bytes).to_hex().to_string(),
            source_manifest: manifest,
            runtime_path: runtime_path.clone(),
            pex_blake3: blake3::hash(&bytes).to_hex().to_string(),
            target_game: "fo4".to_string(),
        };
        std::fs::write(
            staging_root.join(format!("{target_name}.json")),
            serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        receipts.push(receipt);
        artifacts.insert(
            source_name.clone(),
            SkyrimCreatureConvertedArtifact {
                runtime_path,
                source_path: output,
            },
        );
    }
    std::fs::write(
        staging_root.join("attack_scripts.json"),
        serde_json::to_vec_pretty(&receipts).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())?;
    Ok(artifacts)
}

pub(crate) fn publish_attack_scripts(staging_root: &Path, mod_root: &Path) -> Result<(), String> {
    validate_attack_script_publication(staging_root, staging_root)?;
    for receipt in read_receipts(staging_root)? {
        for relative in [
            receipt.source_manifest.relative_source_path,
            format!("data/{}", receipt.runtime_path),
        ] {
            let output = mod_root.join(&relative);
            std::fs::create_dir_all(output.parent().unwrap()).map_err(|error| error.to_string())?;
            std::fs::copy(staging_root.join(relative), &output)
                .map_err(|error| format!("publish attack script {}: {error}", output.display()))?;
        }
    }
    validate_attack_script_publication(staging_root, mod_root)
}

fn read_receipts(staging_root: &Path) -> Result<Vec<AttackScriptReceipt>, String> {
    let path = staging_root.join("attack_scripts.json");
    serde_json::from_slice(
        &std::fs::read(&path).map_err(|error| format!("read {}: {error}", path.display()))?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn validate_attack_script_publication(
    staging_root: &Path,
    mod_root: &Path,
) -> Result<(), String> {
    for receipt in read_receipts(staging_root)? {
        let target_name = target_script_name(&receipt.source_script)
            .ok_or_else(|| format!("unknown attack script receipt {}", receipt.source_script))?;
        if receipt.target_game != "fo4"
            || receipt.runtime_path != format!("Scripts/{target_name}.pex")
            || receipt.source_manifest.relative_source_path
                != format!("Scripts/Source/User/{target_name}.psc")
        {
            return Err(format!("invalid attack script receipt for {target_name}"));
        }
        for (relative, expected) in [
            (
                receipt.source_manifest.relative_source_path,
                receipt.source_manifest.source_blake3,
            ),
            (format!("data/{}", receipt.runtime_path), receipt.pex_blake3),
        ] {
            let bytes = std::fs::read(mod_root.join(&relative))
                .map_err(|error| format!("required creature attack script {relative}: {error}"))?;
            if blake3::hash(&bytes).to_hex().as_str() != expected {
                return Err(format!(
                    "required creature attack script changed: {relative}"
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
