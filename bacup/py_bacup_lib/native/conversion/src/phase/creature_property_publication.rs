use super::*;
use nif_core_native::model::{NifFile, NifValue};

pub(super) fn publish(
    run: &crate::run::ConversionRun,
    families: &[EvidenceBoundPreparedFamily],
    mod_root: &Path,
) -> Result<(), String> {
    let mut rows = Vec::new();
    for family in families {
        if family
            .recipe
            .rig
            .property_sequences
            .iter()
            .all(|binding| binding.sequence_names.is_empty())
        {
            continue;
        }
        let mut targets = Vec::new();
        for recipe in &family.candidate_recipes {
            for (signature, target) in [
                (SigCode(*b"NPC_"), &recipe.key_plan.base.npc),
                (SigCode(*b"RACE"), &recipe.key_plan.base.race),
            ] {
                if !target
                    .plugin
                    .eq_ignore_ascii_case(&run.config.output_plugin_name)
                    || target.local == 0
                {
                    return Err(
                        "property sequence recipe target belongs to a different output plugin"
                            .into(),
                    );
                }
                let record = run
                    .read_target_record_if_available(FormKey {
                        local: target.local,
                        plugin: run.interner.intern(&target.plugin),
                    })
                    .map_err(|error| error.to_string())?;
                if record.is_none_or(|record| record.sig != signature) {
                    return Err(format!(
                        "property sequence target {:06X}:{} is not committed as {}",
                        target.local,
                        target.plugin,
                        signature.as_str()
                    ));
                }
                targets.push(format!("{:06X}:{}", target.local, target.plugin));
            }
        }
        if targets.is_empty() {
            return Err("property sequence family has no committed candidate records".into());
        }
        targets.sort();
        targets.dedup();
        let graph = family
            .execution
            .receipt
            .scaffold_artifacts
            .iter()
            .find(|artifact| artifact.runtime_path == family.recipe.rig.paths.core_behavior)
            .ok_or("property sequence family has no prepared core graph")?;
        let graph_path = format!("Meshes/{}", graph.runtime_path.replace('\\', "/"));
        let bytes =
            fs::read(mod_root.join("data").join(&graph_path)).map_err(|error| error.to_string())?;
        if bytes.len() as u64 != graph.byte_len
            || blake3::hash(&bytes).to_hex().as_str() != graph.blake3
        {
            return Err("published property graph differs from its prepared receipt".into());
        }
        let xml = havok_native::api::havok_hkx_to_xml(&bytes).map_err(|error| error.to_string())?;
        let document = roxmltree::Document::parse(&xml).map_err(|error| error.to_string())?;
        let mut sequences = BTreeSet::new();
        for node in document
            .descendants()
            .filter(|node| node.attribute("class") == Some("BGSGamebryoSequenceGenerator"))
        {
            let parameter = |name| {
                node.children()
                    .find(|node| node.attribute("name") == Some(name))
                    .and_then(|node| node.text())
                    .unwrap_or("")
            };
            if parameter("name").starts_with("B21PropertySequence_") {
                let sequence = parameter("pSequence");
                if !sequence.starts_with("B21CP_")
                    || !sequence.contains("__")
                    || !sequences.insert(sequence.to_string())
                {
                    return Err(
                        "published property graph has an invalid or aliased NIF sequence".into(),
                    );
                }
            }
        }
        if sequences.is_empty() {
            return Err("published property graph has no native sequence consumers".into());
        }
        let mut owners = BTreeSet::new();
        let mut nifs = Vec::new();
        for (path, _) in &family.data_files {
            if !path.to_ascii_lowercase().ends_with(".nif") {
                continue;
            }
            let bytes = fs::read(mod_root.join("data").join(path_from_canonical(path)))
                .map_err(|error| error.to_string())?;
            let nif = NifFile::from_bytes(&bytes, None).map_err(|error| error.to_string())?;
            let mut contains_sequence = false;
            for block in nif
                .blocks
                .iter()
                .filter(|block| block.type_name == "NiControllerSequence")
            {
                if let Some(NifValue::String(name)) = block.get_field("Name")
                    && sequences.contains(name)
                {
                    if !owners.insert(name.clone()) {
                        return Err(format!(
                            "published property sequence has multiple NIF owners: {name}"
                        ));
                    }
                    contains_sequence = true;
                }
            }
            if contains_sequence {
                nifs.push(serde_json::json!({"path":path,"blake3":blake3::hash(&bytes).to_hex().to_string()}));
            }
        }
        if sequences != owners {
            return Err("published property graph references missing NIF sequences".into());
        }
        rows.push(serde_json::json!({"family_id":family.family_id,"recipe_blake3":family.execution.receipt.recipe_blake3,"targets":targets,"graph":{"path":graph_path,"blake3":graph.blake3},"nifs":nifs,"sequence_count":sequences.len()}));
    }
    write_receipt(&run.config.output_plugin_name, mod_root, rows)
}

fn write_receipt(
    output_plugin: &str,
    mod_root: &Path,
    families: Vec<serde_json::Value>,
) -> Result<(), String> {
    if output_plugin.is_empty()
        || output_plugin.contains(['/', '\\', ':'])
        || matches!(output_plugin, "." | "..")
    {
        return Err("property runtime output plugin is not a filename".into());
    }
    let path = mod_root
        .join("F4SE/Plugins/B21_ConversionRuntime/PropertySequences")
        .join(format!("{output_plugin}.json"));
    if families.is_empty() {
        return match fs::remove_file(&path) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error.to_string()),
        };
    }
    fs::create_dir_all(path.parent().unwrap()).map_err(|error| error.to_string())?;
    let bytes = serde_json::to_vec_pretty(
        &serde_json::json!({"version":1,"output_plugin":output_plugin,"families":families}),
    )
    .map_err(|error| error.to_string())?;
    fs::write(path, bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn property_runtime_receipt_is_output_owned_and_empty_success_cleans_only_its_owner() {
        let temp = tempfile::tempdir().unwrap();
        let row = serde_json::json!({"family_id":"source_rig","sequence_count":1});
        write_receipt("First.esm", temp.path(), vec![row.clone()]).unwrap();
        write_receipt("Second.esm", temp.path(), vec![row]).unwrap();
        let folder = temp
            .path()
            .join("F4SE/Plugins/B21_ConversionRuntime/PropertySequences");
        let receipt: serde_json::Value =
            serde_json::from_slice(&fs::read(folder.join("First.esm.json")).unwrap()).unwrap();
        assert_eq!(receipt["output_plugin"], "First.esm");
        assert_eq!(receipt["version"], 1);
        write_receipt("First.esm", temp.path(), vec![]).unwrap();
        assert!(!folder.join("First.esm.json").exists());
        assert!(folder.join("Second.esm.json").exists());
        assert!(write_receipt("../Second.esm", temp.path(), vec![]).is_err());
        assert!(write_receipt("C:\\Second.esm", temp.path(), vec![]).is_err());
    }
}
