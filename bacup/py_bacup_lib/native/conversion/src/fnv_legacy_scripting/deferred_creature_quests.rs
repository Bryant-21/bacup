use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredCreatureQuest {
    pub source_form_key: String,
    pub source_editor_id: String,
    pub capability: String,
    pub source_text: String,
    pub events: Vec<DeferredQuestEvent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeferredQuestEvent {
    pub name: String,
    pub source_text: String,
}

pub(crate) fn partition(
    source: &str,
    source_key: &str,
    editor_id: &str,
    creature_attached: bool,
) -> Result<(String, Option<DeferredCreatureQuest>), String> {
    let audited = match (
        source_key.to_ascii_lowercase().as_str(),
        editor_id.to_ascii_lowercase().as_str(),
    ) {
        ("0fe6cf:falloutnv.esm", "v22kochscript") => Some((
            include_str!("script_synthesizer/fixtures/V22KochScript.geck"),
            "FNV player Repair >= 60, Koch repair dialogue and quest state",
        )),
        ("0e9483:falloutnv.esm", "bmrhondascript") => Some((
            include_str!("script_synthesizer/fixtures/BMRhondaScript.geck"),
            "FNV player Science >= 60, Rhonda repair dialogue and Tabitha quest state",
        )),
        _ => None,
    };
    let Some((expected, capability)) = audited.filter(|_| creature_attached) else {
        return Ok((source.to_string(), None));
    };
    if source.replace('\r', "") != expected.replace('\r', "") {
        return Err(format!(
            "SCPT '{editor_id}' differs from the audited creature quest event partition"
        ));
    }
    let mut retained = String::new();
    let mut events = Vec::new();
    let mut current: Option<DeferredQuestEvent> = None;
    for line in source.split_inclusive('\n') {
        let statement = line.split(';').next().unwrap_or_default().trim();
        let mut words = statement.split_whitespace();
        if words
            .next()
            .is_some_and(|word| word.eq_ignore_ascii_case("begin"))
        {
            let name = words.next().unwrap_or_default();
            if name.eq_ignore_ascii_case("OnActivate") || name.eq_ignore_ascii_case("GameMode") {
                current = Some(DeferredQuestEvent {
                    name: name.into(),
                    source_text: String::new(),
                });
            }
        }
        if let Some(event) = current.as_mut() {
            event.source_text.push_str(line);
            if statement.eq_ignore_ascii_case("end") {
                events.push(current.take().unwrap());
            }
        } else {
            retained.push_str(line);
        }
    }
    if current.is_some() || events.len() != 2 {
        return Err(format!(
            "SCPT '{editor_id}' has an incomplete audited quest event partition"
        ));
    }
    Ok((
        retained,
        Some(DeferredCreatureQuest {
            source_form_key: source_key.into(),
            source_editor_id: editor_id.into(),
            capability: capability.into(),
            source_text: source.into(),
            events,
        }),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quest_partition_preserves_storage_lifecycle_and_original_events() {
        for (id, name, source) in [
            (
                "0FE6CF:FalloutNV.esm",
                "V22KochScript",
                include_str!("script_synthesizer/fixtures/V22KochScript.geck"),
            ),
            (
                "0E9483:FalloutNV.esm",
                "BMRhondaScript",
                include_str!("script_synthesizer/fixtures/BMRhondaScript.geck"),
            ),
        ] {
            let (retained, audit) = partition(source, id, name, true).unwrap();
            let audit = audit.unwrap();
            assert_eq!(audit.source_text, source);
            assert_eq!(audit.events.len(), 2);
            assert!(
                audit
                    .events
                    .iter()
                    .all(|event| source.contains(&event.source_text))
            );
            let script = fnv_script_native::parser::parse_script(&retained).unwrap();
            assert!(
                script
                    .variables
                    .iter()
                    .any(|var| var.name == "RepairedState")
            );
            if name == "V22KochScript" {
                assert_eq!(script.blocks.len(), 1);
                assert!(script.blocks[0].event.eq_ignore_ascii_case("OnLoad"));
                assert!(retained.contains("SetGhost 1"));
            } else {
                assert!(script.blocks.is_empty());
                assert!(script.variables.iter().any(|var| var.name == "HatesPlayer"));
            }
            assert!(
                partition(
                    &(source.to_string() + "\nBegin OnDeath\nEnd\n"),
                    id,
                    name,
                    true
                )
                .is_err()
            );
            assert!(partition(source, id, name, false).unwrap().1.is_none());
            assert!(
                partition(
                    source,
                    &id.replace("FalloutNV.esm", "Other.esm"),
                    name,
                    true
                )
                .unwrap()
                .1
                .is_none()
            );
        }
    }
}
