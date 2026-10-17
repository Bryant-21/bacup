use std::collections::BTreeMap;
use std::path::{Component, Path};

use serde::{Deserialize, Serialize};

use super::script_synthesizer::TranslatedScript;
use super::vmad::CompiledScriptEvidence;

#[derive(Serialize, Deserialize)]
struct Receipt {
    version: u32,
    sources: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
    #[serde(default)]
    deferred_quest_capabilities:
        BTreeMap<String, super::deferred_creature_quests::DeferredCreatureQuest>,
}

fn read_relative(root: &Path, relative: &str) -> Result<Vec<u8>, String> {
    let path = Path::new(relative);
    if path.is_absolute()
        || path
            .components()
            .any(|part| !matches!(part, Component::Normal(_)))
    {
        return Err(format!(
            "creature script artifact has an invalid relative path: {relative}"
        ));
    }
    std::fs::read(root.join(path))
        .map_err(|error| format!("read creature script artifact {relative}: {error}"))
}

pub(crate) fn write(
    root: &Path,
    receipt_path: &Path,
    scripts: &[&TranslatedScript],
    compiled: &[CompiledScriptEvidence],
) -> Result<(), String> {
    let mut receipt = Receipt {
        version: 1,
        sources: BTreeMap::new(),
        files: BTreeMap::new(),
        deferred_quest_capabilities: BTreeMap::new(),
    };
    let mut classes = BTreeMap::new();
    for script in scripts {
        if let Some(audit) = &script.deferred_creature_quest {
            receipt
                .deferred_quest_capabilities
                .insert(script.source_form_key.clone(), audit.clone());
        }
        receipt.sources.insert(
            script.source_form_key.clone(),
            script.script_class_name.clone(),
        );
        classes.insert(
            script.script_class_name.as_str(),
            Some(script.psc_text.as_str()),
        );
        for helper in ["B21_ConversionClock", "B21_ConversionCreature"] {
            if script.psc_text.contains(&format!("{helper}.")) {
                classes.insert(helper, None);
            }
        }
    }
    let native = classes
        .keys()
        .any(|class| matches!(*class, "B21_ConversionClock" | "B21_ConversionCreature"));
    for (class, expected_source) in classes {
        let source_path = format!("Scripts/Source/User/{class}.psc");
        let source = read_relative(root, &source_path)?;
        if expected_source.is_some_and(|expected| source != expected.as_bytes()) {
            return Err(format!(
                "creature script {class} source changed after translation"
            ));
        }
        let evidence = compiled
            .iter()
            .find(|item| item.compiled_success && item.class_name.eq_ignore_ascii_case(class))
            .ok_or_else(|| format!("creature script {class} lacks compiler evidence"))?;
        let pex = read_relative(root, &evidence.relative_pex_path)?;
        let parsed = papyrus_core::pex::parse_pex_bytes(&pex)
            .map_err(|error| format!("creature script {class} PEX is invalid: {error}"))?;
        if parsed.game_id != 2
            || parsed.objects.len() != 1
            || !parsed.objects[0].name.eq_ignore_ascii_case(class)
        {
            return Err(format!(
                "creature script {class} is not its required Fallout 4 PEX"
            ));
        }
        receipt
            .files
            .insert(source_path, blake3::hash(&source).to_hex().to_string());
        receipt.files.insert(
            evidence.relative_pex_path.clone(),
            blake3::hash(&pex).to_hex().to_string(),
        );
    }
    if native {
        let path = "F4SE/Plugins/B21_ConversionRuntime.dll";
        let bytes = read_relative(root, path)?;
        if bytes.is_empty() {
            return Err("creature script runtime DLL is empty".into());
        }
        receipt
            .files
            .insert(path.into(), blake3::hash(&bytes).to_hex().to_string());
    }
    if let Some(parent) = receipt_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    std::fs::write(
        receipt_path,
        serde_json::to_vec_pretty(&receipt).map_err(|error| error.to_string())?,
    )
    .map_err(|error| error.to_string())
}

pub(crate) fn validate(root: &Path, receipt_path: &Path) -> Result<(), String> {
    let receipt: Receipt = serde_json::from_slice(
        &std::fs::read(receipt_path)
            .map_err(|error| format!("read creature script receipt: {error}"))?,
    )
    .map_err(|error| error.to_string())?;
    if receipt.version != 1 {
        return Err("unsupported creature script receipt version".into());
    }
    for (path, expected) in receipt.files {
        let bytes = read_relative(root, &path)?;
        if blake3::hash(&bytes).to_hex().as_str() != expected {
            return Err(format!(
                "creature script artifact changed after compilation: {path}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn publication_rejects_changed_missing_and_escaping_artifacts() {
        let root = tempfile::tempdir().unwrap();
        let receipt = root.path().join("receipt.json");
        std::fs::write(root.path().join("artifact.pex"), b"compiled").unwrap();
        std::fs::write(
            &receipt,
            serde_json::to_vec(&Receipt {
                version: 1,
                sources: BTreeMap::new(),
                deferred_quest_capabilities: BTreeMap::new(),
                files: BTreeMap::from([(
                    "artifact.pex".into(),
                    blake3::hash(b"compiled").to_hex().to_string(),
                )]),
            })
            .unwrap(),
        )
        .unwrap();
        validate(root.path(), &receipt).unwrap();
        std::fs::write(root.path().join("artifact.pex"), b"changed").unwrap();
        assert!(
            validate(root.path(), &receipt)
                .unwrap_err()
                .contains("changed")
        );
        std::fs::remove_file(root.path().join("artifact.pex")).unwrap();
        assert!(
            validate(root.path(), &receipt)
                .unwrap_err()
                .contains("read creature script artifact")
        );
        assert!(read_relative(root.path(), "../artifact.pex").is_err());
    }
}
