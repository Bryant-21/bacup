use super::*;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SourceCastAction {
    pub idle: String,
    pub event: String,
    #[serde(default)]
    pub prepare: Option<String>,
    #[serde(default)]
    pub ready: Option<String>,
    #[serde(default)]
    pub interrupt: Option<String>,
    #[serde(default)]
    pub require_in_combat: bool,
}

pub(super) fn family_shout_action(
    family: &str,
    shout: FormKey,
    records: &HashMap<FormKey, &Record>,
    interner: &StringInterner,
) -> Result<SourceCastAction, String> {
    let path = match family {
        "dragon" => return dragon_shout_action(shout, records, interner),
        "draugr" => "actorsdraugrbehaviorsdraugrbehaviorhkx",
        "dwarven_steam" => "actorsdwarvensteamcenturionbehaviorssteambehaviorhkx",
        _ => return Err(format!("{family} has no proven source voice action")),
    };
    let select = |root: u32, event: &str| -> Result<&Record, String> {
        let mut matches = Vec::new();
        for record in records.values().filter(|record| record.sig.0 == *b"IDLE") {
            let text = |sig| match field(record, sig) {
                Some(FieldValue::String(value)) => interner.resolve(*value),
                _ => None,
            };
            if text(b"DNAM").is_none_or(|value| normalize(value) != path)
                || text(b"ENAM") != Some(event)
            {
                continue;
            }
            let mut parent = idle_parent(record, interner)?;
            let mut seen = std::collections::HashSet::new();
            while parent.local != root || interner.resolve(parent.plugin) != Some("Skyrim.esm") {
                if !seen.insert(parent) {
                    return Err("cyclic source voice IDLE ancestry".into());
                }
                let ancestor = records.get(&parent).ok_or_else(|| {
                    format!(
                        "missing source voice IDLE ancestor {}",
                        parent.format(interner)
                    )
                })?;
                if ancestor.sig.0 != *b"IDLE" {
                    return Err("source voice action ancestor is not IDLE".into());
                }
                if ancestor.fields.iter().any(|field| field.sig.0 == *b"CTDA") {
                    return Err("source voice action ancestor has unsupported predicates".into());
                }
                parent = idle_parent(ancestor, interner)?;
            }
            matches.push(*record);
        }
        match matches.as_slice() {
            [record] => Ok(*record),
            _ => Err(format!(
                "{family} source {event} has {} exact voice actions",
                matches.len()
            )),
        }
    };
    let start = select(0x13008, "shoutStart")?;
    let release = select(0x13457, "shoutRelease")?;
    let interrupt = select(0x13459, "shoutStop")?;
    let conditions = start
        .fields
        .iter()
        .filter(|field| field.sig.0 == *b"CTDA")
        .collect::<Vec<_>>();
    let [condition] = conditions.as_slice() else {
        return Err(format!(
            "{family} voice start requires the exact source combat predicate"
        ));
    };
    let bytes = match &condition.value {
        FieldValue::Bytes(bytes) => bytes,
        value => match named(value, "raw", interner) {
            Some(FieldValue::Bytes(bytes)) => bytes,
            _ => return Err("source voice start condition has no raw receipt".into()),
        },
    };
    if bytes.len() != 32
        || bytes[0] != 0
        || f32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 1.0
        || u16::from_le_bytes(bytes[8..10].try_into().unwrap()) != 289
        || bytes[12..28].iter().any(|byte| *byte != 0)
    {
        return Err(format!(
            "{family} voice start is not source IsInCombat == 1"
        ));
    }
    if [release, interrupt]
        .iter()
        .any(|record| record.fields.iter().any(|field| field.sig.0 == *b"CTDA"))
    {
        return Err(format!(
            "{family} voice release/interrupt has unsupported predicates"
        ));
    }
    Ok(SourceCastAction {
        idle: start.form_key.format(interner),
        event: "shoutStart".into(),
        prepare: Some("shoutRelease".into()),
        ready: None,
        interrupt: Some("shoutStop".into()),
        require_in_combat: true,
    })
}

pub(super) fn dragon_shout_action(
    shout: FormKey,
    records: &HashMap<FormKey, &Record>,
    interner: &StringInterner,
) -> Result<SourceCastAction, String> {
    let mut defaults = Vec::new();
    let mut conditional = Vec::new();
    for record in records.values().filter(|record| record.sig.0 == *b"IDLE") {
        let text = |sig| match field(record, sig) {
            Some(FieldValue::String(value)) => interner.resolve(*value),
            _ => None,
        };
        if !text(b"DNAM").is_some_and(|path| {
            normalize(&path).ends_with("actorsdragonbehaviorsdragonbehaviorhkx")
        }) {
            continue;
        }
        let Some(event) =
            text(b"ENAM").filter(|event| matches!(*event, "shoutStart" | "shoutStartFireball"))
        else {
            continue;
        };
        let conditions = record
            .fields
            .iter()
            .filter(|field| field.sig.0 == *b"CTDA")
            .collect::<Vec<_>>();
        if conditions.is_empty() && event == "shoutStart" {
            defaults.push(*record);
            continue;
        }
        let [condition] = conditions.as_slice() else {
            return Err(format!(
                "Dragon source IDLE {} has unsupported shout selection conditions",
                record.form_key.format(interner)
            ));
        };
        let Some(FieldValue::Bytes(bytes)) = named(&condition.value, "raw", interner) else {
            return Err("Dragon shout IDLE has no source condition receipt".into());
        };
        if bytes.len() != 32
            || bytes[0] & !0x01 != 0
            || f32::from_le_bytes(bytes[4..8].try_into().unwrap()) != 1.0
            || u16::from_le_bytes(bytes[8..10].try_into().unwrap()) != 579
            || u32::from_le_bytes(bytes[16..20].try_into().unwrap()) != 0
            || u32::from_le_bytes(bytes[20..24].try_into().unwrap()) != 0
        {
            return Err(format!(
                "Dragon source IDLE {} is not a proven GetEquippedShout equality",
                record.form_key.format(interner)
            ));
        }
        let target = named(&condition.value, "shout", interner)
            .and_then(form_key)
            .ok_or_else(|| {
                "Dragon source IDLE shout is unresolved in its source master context".to_string()
            })?;
        conditional.push((*record, target, event.to_string()));
    }
    let [default] = defaults.as_slice() else {
        return Err("missing unique unconditional source Dragon voice action".into());
    };
    let parent = idle_parent(default, interner)?;
    if conditional
        .iter()
        .any(|(idle, _, _)| idle_parent(idle, interner).ok() != Some(parent))
    {
        return Err("Dragon voice action siblings have different source parents".into());
    }
    let selected = conditional
        .iter()
        .filter(|(_, target, _)| *target == shout)
        .collect::<Vec<_>>();
    match selected.as_slice() {
        [] => Ok(SourceCastAction {
            idle: default.form_key.format(interner),
            event: "shoutStart".into(),
            prepare: Some("shoutRelease".into()),
            ready: None,
            interrupt: Some("shoutStop".into()),
            require_in_combat: false,
        }),
        [(idle, _, event)] => Ok(SourceCastAction {
            idle: idle.form_key.format(interner),
            event: event.clone(),
            prepare: Some("shoutRelease".into()),
            ready: None,
            interrupt: Some("shoutStop".into()),
            require_in_combat: false,
        }),
        _ => Err("ambiguous source Dragon shout action".into()),
    }
}

fn idle_parent(record: &Record, interner: &StringInterner) -> Result<FormKey, String> {
    let value = field(record, b"ANAM").ok_or("source voice IDLE lacks parent")?;
    if let Some(key) = named(value, "parent", interner).and_then(form_key) {
        return Ok(key);
    }
    Err("source voice IDLE parent is unresolved in its source master context".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore = "requires the installed Skyrim source shout corpus"]
    fn source_draugr_centurion_voice_actions_keep_ancestry_and_combat_predicate() {
        use esp_authoring_core::plugin_runtime::{
            plugin_handle_close_native, plugin_handle_load_no_py,
        };
        let manifest: serde_json::Value = serde_json::from_slice(
            &std::fs::read(std::env::var("MODKIT_SKYRIM_SHOUT_INPUTS").unwrap()).unwrap(),
        )
        .unwrap();
        let path = manifest
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|entry| entry["path"].as_str())
            .find(|path| path.to_ascii_lowercase().ends_with("skyrim.esm"))
            .unwrap();
        let handle = plugin_handle_load_no_py(path, Some("skyrimse"), None, None, true).unwrap();
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
        for (family, first, shout) in [
            ("draugr", 0xC18FB, 0x9DE93),
            ("dwarven_steam", 0xC2B6D, 0xA82BC),
        ] {
            let mut source = (first..=first + 4)
                .map(|local| {
                    crate::source_read::read_record_relayout(
                        handle,
                        &format!("{local:06X}@Skyrim.esm"),
                        &schema,
                        &interner,
                        None,
                    )
                    .unwrap()
                })
                .collect::<Vec<_>>();
            let shout = FormKey {
                local: shout,
                plugin: interner.intern("Skyrim.esm"),
            };
            let action = family_shout_action(
                family,
                shout,
                &source
                    .iter()
                    .map(|record| (record.form_key, record))
                    .collect(),
                &interner,
            )
            .unwrap();
            assert_eq!(action.idle, format!("{:06X}@Skyrim.esm", first + 1));
            assert_eq!(action.event, "shoutStart");
            assert_eq!(action.prepare.as_deref(), Some("shoutRelease"));
            assert_eq!(action.interrupt.as_deref(), Some("shoutStop"));
            assert!(action.require_in_combat);
            assert!(action.ready.is_none());
            let entry = source
                .iter_mut()
                .find(|record| record.form_key.local == first + 1)
                .unwrap();
            let condition = entry
                .fields
                .iter_mut()
                .find(|field| field.sig.0 == *b"CTDA")
                .unwrap();
            let FieldValue::Bytes(bytes) = &mut condition.value else {
                panic!("source IsInCombat remains raw");
            };
            bytes[4..8].copy_from_slice(&0.0_f32.to_le_bytes());
            assert!(
                family_shout_action(
                    family,
                    shout,
                    &source
                        .iter()
                        .map(|record| (record.form_key, record))
                        .collect(),
                    &interner
                )
                .is_err()
            );
        }
        assert!(plugin_handle_close_native(handle));
    }
}
