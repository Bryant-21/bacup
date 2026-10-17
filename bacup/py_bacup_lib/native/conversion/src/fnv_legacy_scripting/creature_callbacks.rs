use serde_json::{Value, json};

use super::quest_ir::LegacyQuestIr;

pub(crate) fn nellis_callback_quest(
    record: &Value,
    source_key: &str,
) -> Result<Option<Value>, String> {
    let Some((local, plugin)) = source_key.split_once(':') else {
        return Ok(None);
    };
    if !plugin.eq_ignore_ascii_case("FalloutNV.esm") {
        return Ok(None);
    }
    let expected = match u32::from_str_radix(local, 16).ok() {
        Some(0x0FED44) => "VFreeformNellis",
        Some(0x1083DE) => "VNellisInfestation",
        _ => return Ok(None),
    };
    if !record
        .get("eid")
        .and_then(Value::as_str)
        .is_some_and(|eid| eid.eq_ignore_ascii_case(expected))
    {
        return Err(format!(
            "creature callback quest {source_key} has an unexpected source EditorID"
        ));
    }
    let source = LegacyQuestIr::parse(record).map_err(|error| error.to_string())?;
    if expected == "VNellisInfestation" {
        for index in [15, 20, 35, 45] {
            if !source
                .objectives
                .iter()
                .any(|objective| objective.index == index)
            {
                return Err(format!(
                    "creature callback quest {source_key} lacks source objective {index}"
                ));
            }
        }
    }
    let mut fields = Vec::new();
    if let Some(name) = source.name {
        fields.push(json!({"FULL": name}));
    }
    fields.push(json!({"DNAM": {
        "Flags": source.general.flags & 9, "Priority": source.general.priority,
        "UnknownByte3": 0, "DelayTime": source.general.delay_time, "Type": 0,
        "UnknownByte6": 0, "UnknownByte7": 0, "UnknownByte8": 0
    }}));
    for objective in source.objectives {
        fields.push(json!({"QOBJ": objective.index}));
        fields.push(json!({"FNAM": objective.flags}));
        if let Some(text) = objective.display_text {
            fields.push(json!({"NNAM": text}));
        }
    }
    fields.push(json!({"ANAM": 0}));
    Ok(Some(
        json!({"signature": "QUST", "form_id": source_key, "eid": source.editor_id,
        "form_version": 131, "version2": 5, "fields": fields}),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn source_objective_states_and_storage_quest_are_bounded_to_exact_identity() {
        let mut source = json!({"eid": "VNellisInfestation", "fields": [
            {"FULL": "Ant Misbehavin'"}, {"DATA": {"Priority": 60}},
            {"QOBJ": 15}, {"NNAM": "Restart the generators"},
            {"QOBJ": 20}, {"NNAM": "Kill the ants"}, {"QOBJ": 35}, {"QOBJ": 45}
        ]});
        let target = nellis_callback_quest(&source, "1083DE:FalloutNV.esm")
            .unwrap()
            .unwrap();
        assert_eq!(target["fields"][1]["DNAM"]["Priority"], 60);
        assert_eq!(
            target["fields"]
                .as_array()
                .unwrap()
                .iter()
                .filter_map(|field| field.get("QOBJ").and_then(Value::as_u64))
                .collect::<Vec<_>>(),
            [15, 20, 35, 45]
        );
        assert!(
            nellis_callback_quest(&source, "1083DE:Other.esm")
                .unwrap()
                .is_none()
        );
        source["fields"].as_array_mut().unwrap().pop();
        assert!(
            nellis_callback_quest(&source, "1083DE:FalloutNV.esm")
                .unwrap_err()
                .contains("45")
        );
        let storage = nellis_callback_quest(&json!({"eid": "VFreeformNellis", "fields": [{"DATA": {"Flags": 17, "Priority": 55}}, {"SCRI": "0FED40:FalloutNV.esm"}]}), "0FED44:FalloutNV.esm").unwrap().unwrap();
        assert_eq!(storage["fields"][0]["DNAM"]["Flags"], 1);
    }
}
