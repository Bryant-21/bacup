use super::*;

pub(super) struct SpellAction {
    pub event: String,
    pub casting_source: u32,
    pub stage: String,
    ancestry: Vec<FormKey>,
    conditions: Vec<Vec<(u16, u8, f32, bool, Option<FormKey>)>>,
}

impl SpellAction {
    pub fn matches(&self, spell: &Record, records: &HashMap<FormKey, &Record>, interner: &StringInterner) -> bool {
        let Some(data) = spell_data(spell, interner) else { return false; };
        let cast = semantic(named(&data, "cast_type", interner), &["constanteffect", "fireandforget", "concentration"], interner);
        let delivery = semantic(named(&data, "target_type", interner).or_else(|| named(&data, "delivery", interner)), &["self", "touch", "aimed", "targetactor", "targetlocation"], interner).or(Some(0));
        self.conditions.iter().all(|conditions| {
            let mut group = false;
            for &(function, comparison, value, or_next, keyword) in conditions {
                let Some(actual) = (match function { 570 => Some(1), 571 => cast, 572 => delivery, 596 => keyword.map(|keyword| u32::from(spell_has_keyword(spell, keyword, records))), _ => None }) else { return false; };
                let actual = actual as f32;
                group |= match comparison { 0 => actual == value, 1 => actual != value, 2 => actual > value, 3 => actual >= value, 4 => actual < value, 5 => actual <= value, _ => false };
                if !or_next {
                    if !group { return false; }
                    group = false;
                }
            }
            conditions.last().is_none_or(|condition| !condition.3 || group)
        })
    }
}

pub(super) fn collect(
    paths: &[String],
    events: &BTreeSet<String>,
    records: &HashMap<FormKey, &Record>,
    interner: &StringInterner,
) -> Result<Vec<SpellAction>, String> {
    let mut result = Vec::new();
    for idle in records.values().filter(|record| record.sig.0 == *b"IDLE") {
        let Some(FieldValue::String(path)) = field(idle, b"DNAM") else { continue; };
        let Some(path) = interner.resolve(*path) else { continue; };
        if !paths.iter().any(|expected| expected == &normalize(path)) { continue; }
        let Some(FieldValue::String(event)) = field(idle, b"ENAM") else { continue; };
        let Some(event) = interner.resolve(*event) else { continue; };
        let Some(event) = events.iter().find(|candidate| candidate.eq_ignore_ascii_case(event)) else { continue; };
        let mut current = *idle;
        let mut visited = std::collections::HashSet::new();
        let mut chain = Vec::new();
        let root = loop {
            if !visited.insert(current.form_key) { return Err("cyclic source spell IDLE ancestry".into()); }
            if current.sig.0 == *b"AACT" {
                break current.eid.and_then(|name| interner.resolve(name)).unwrap_or("");
            }
            chain.push(current);
            let Some(parent) = field(current, b"ANAM").and_then(|value| named(value, "parent", interner)).and_then(form_key) else { break ""; };
            let Some(parent) = records.get(&parent) else { break ""; };
            current = parent;
        };
        let Some((casting_source, stage)) = [
            ("ActionLeft", 0), ("ActionRight", 1),
        ].into_iter().find_map(|(prefix, source)| root.strip_prefix(prefix).filter(|stage| matches!(*stage, "Attack" | "Ready" | "Release" | "Interrupt")).map(|stage| (source, stage))) else { continue; };
        let mut groups = Vec::new();
        for record in &chain {
            let mut group = Vec::new();
            for condition in record.fields.iter().filter(|field| field.sig.0 == *b"CTDA") {
                let value = &condition.value;
                let keyword = named(value, "keyword", interner).and_then(form_key);
                let FieldValue::Bytes(bytes) = named(value, "raw", interner).unwrap_or(value) else { return Err(format!("ordinary spell IDLE {} has an unsupported structured condition", record.form_key.format(interner))); };
                if bytes.len() != 32 { return Err("ordinary spell IDLE condition has invalid source length".into()); }
                let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
                let source = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
                let value = f32::from_le_bytes(bytes[4..8].try_into().unwrap());
                if !matches!(function, 570..=572 | 596) || bytes[0] & 0x1E != 0 || bytes[0] >> 5 > 5 || !value.is_finite()
                    || source != casting_source || (function == 596 && keyword.is_none())
                    || (function != 596 && u32::from_le_bytes(bytes[16..20].try_into().unwrap()) != 0)
                    || u32::from_le_bytes(bytes[20..24].try_into().unwrap()) != 0 {
                    return Err(format!("ordinary spell IDLE {} has unsupported source predicate {function}", record.form_key.format(interner)));
                }
                group.push((function, bytes[0] >> 5, value, bytes[0] & 1 != 0, keyword));
            }
            groups.push(group);
        }
        result.push(SpellAction { event: event.clone(), casting_source, stage: stage.into(), ancestry: chain.iter().map(|record| record.form_key).collect(), conditions: groups });
    }
    Ok(result)
}

fn stage_event(actions: &[SpellAction], source: u32, stage: &str, spell: &Record, records: &HashMap<FormKey, &Record>, interner: &StringInterner) -> Result<Option<String>, String> {
    let matches = actions.iter().filter(|action| action.casting_source == source && action.stage == stage && action.matches(spell, records, interner)).collect::<Vec<_>>();
    let events = matches.iter().filter(|action| !matches.iter().any(|other| precedes(other, action, records, interner))).map(|action| action.event.clone()).collect::<BTreeSet<_>>();
    if events.len() > 1 { return Err(format!("{} has {} source {stage} events for casting source {source}", spell.form_key.format(interner), events.len())); }
    Ok(events.into_iter().next())
}

fn spell_has_keyword(spell: &Record, keyword: FormKey, records: &HashMap<FormKey, &Record>) -> bool {
    fn contains(value: &FieldValue, keyword: FormKey) -> bool {
        match value { FieldValue::FormKey(key) => *key == keyword, FieldValue::List(values) => values.iter().any(|value| contains(value, keyword)), _ => false }
    }
    spell.fields.iter().filter(|field| field.sig.0 == *b"KWDA").any(|field| contains(&field.value, keyword))
        || spell.fields.iter().filter(|field| field.sig.0 == *b"EFID").filter_map(|field| form_key(&field.value))
        .filter_map(|key| records.get(&key)).any(|effect| effect.fields.iter().filter(|field| field.sig.0 == *b"KWDA").any(|field| contains(&field.value, keyword)))
}

fn generator_events(generator: &crate::source_rig::CapabilityRoleGenerator) -> BTreeSet<String> {
    fn visit(value: &serde_json::Value, events: &mut BTreeSet<String>) {
        match value {
            serde_json::Value::Object(fields) => {
                if let Some(name) = fields.get("event_name").and_then(serde_json::Value::as_str) { events.insert(name.into()); }
                for value in fields.values() { visit(value, events); }
            }
            serde_json::Value::Array(values) => { for value in values { visit(value, events); } }
            _ => {}
        }
    }
    let mut events = BTreeSet::new();
    if let crate::source_rig::CapabilityRoleGenerator::SourceCasting { stop_events, .. } = generator {
        events.extend(stop_events.iter().cloned());
        visit(&serde_json::to_value(generator).expect("source generator serializes"), &mut events);
    }
    events
}

fn graph_events(family: &SkyrimCreatureFamilyJob) -> BTreeSet<String> {
    let mut events = BTreeSet::new();
    for role in &family.graph.roles {
        if matches!(role.generator, crate::source_rig::CapabilityRoleGenerator::SourceCasting { .. }) {
            events.extend(generator_events(&role.generator));
            events.extend(role.trigger_event.iter().cloned());
        }
    }
    events
}

fn precedes(left: &SpellAction, right: &SpellAction, records: &HashMap<FormKey, &Record>, interner: &StringInterner) -> bool {
    right.ancestry.iter().any(|key| {
        let mut cursor = Some(*key);
        let mut seen = std::collections::HashSet::new();
        while let Some(key) = cursor.filter(|key| seen.insert(*key)) {
            cursor = records.get(&key).and_then(|record| field(record, b"ANAM")).and_then(|value| named(value, "previous", interner)).and_then(form_key).filter(|key| key.local != 0);
            if cursor.is_some_and(|key| left.ancestry.contains(&key)) { return true; }
        }
        false
    })
}

pub(super) fn bind(family: &SkyrimCreatureFamilyJob, abilities: &[CreatureCastAbility], records: &HashMap<FormKey, &Record>, interner: &StringInterner, animations: &mut Vec<CreatureCastAnimation>) -> Result<(), String> {
    if abilities.iter().all(|ability| ability.shout) { return Ok(()); }
    let paths = family.inventory.behavior_paths.iter().map(|path| normalize(path)).collect::<Vec<_>>();
    let actions = collect(&paths, &graph_events(family), records, interner)?;
    let mut bound = animations.iter().filter(|animation| animation.voice).cloned().collect::<Vec<_>>();
    for word in abilities.iter().filter(|ability| !ability.shout).flat_map(|ability| &ability.words) {
        let spell = records.values().find(|record| record.form_key.format(interner) == word.source_spell).ok_or("missing source spell for action binding")?;
        for source in 0..=1 {
            let Some(entry) = stage_event(&actions, source, "Attack", spell, records, interner)? else { continue; };
            let ready = stage_event(&actions, source, "Ready", spell, records, interner)?;
            let release = stage_event(&actions, source, "Release", spell, records, interner)?;
            let interrupt = stage_event(&actions, source, "Interrupt", spell, records, interner)?;
            for animation in animations.iter().filter(|animation| !animation.voice && animation.event_driven && animation.event == entry) {
                let mut animation = animation.clone();
                let role = family.graph.roles.iter().find(|role| role.trigger_event.as_ref() == Some(&entry) && matches!(role.generator, crate::source_rig::CapabilityRoleGenerator::SourceCasting { .. })).ok_or("missing source casting role")?;
                let events = generator_events(&role.generator);
                animation.casting_source = source;
                animation.ready = ready.clone().filter(|event| events.contains(event));
                animation.prepare = release.clone().filter(|event| events.contains(event));
                animation.interrupt = interrupt.clone().filter(|event| events.contains(event));
                animation.finish_event.clear();
                animation.allowed_spells = vec![word.source_spell.clone()];
                bound.push(animation);
            }
        }
    }
    *animations = bound;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn source_spell_keyword_predicate_includes_effect_keywords_and_ordered_fallback() {
        let interner = StringInterner::new();
        let key = |local| FormKey { local, plugin: interner.intern("Source.esm") };
        let mut spell = Record::new(SigCode(*b"SPEL"), key(1));
        spell.fields.push(FieldEntry { sig: SubrecordSig(*b"EFID"), value: FieldValue::FormKey(key(2)) });
        let mut effect = Record::new(SigCode(*b"MGEF"), key(2));
        effect.fields.push(FieldEntry { sig: SubrecordSig(*b"KWDA"), value: FieldValue::List(vec![FieldValue::FormKey(key(3))]) });
        let mut fallback = Record::new(SigCode(*b"IDLE"), key(5));
        fallback.fields.push(FieldEntry { sig: SubrecordSig(*b"ANAM"), value: FieldValue::Struct(vec![(interner.intern("previous"), FieldValue::FormKey(key(4)))]) });
        let records = HashMap::from([(effect.form_key, &effect), (fallback.form_key, &fallback)]);
        assert!(spell_has_keyword(&spell, key(3), &records));
        assert!(!spell_has_keyword(&spell, key(9), &records));
        let action = |key| SpellAction { event: String::new(), casting_source: 0, stage: "Attack".into(), ancestry: vec![key], conditions: Vec::new() };
        assert!(precedes(&action(key(4)), &action(key(5)), &records, &interner));
        assert!(!precedes(&action(key(5)), &action(key(4)), &records, &interner));
    }

    #[test]
    #[ignore = "requires the installed source IDLE corpus and CLI cache"]
    fn source_ordinary_actions_preserve_native_ancestry_delivery_and_ward_precedence() {
        use esp_authoring_core::plugin_runtime::{plugin_handle_close_native, plugin_handle_load_no_py};
        let inputs: serde_json::Value = serde_json::from_slice(&std::fs::read(std::env::var("MODKIT_SKYRIM_SHOUT_INPUTS").unwrap()).unwrap()).unwrap();
        let path = inputs.as_array().unwrap().iter().filter_map(|item| item["path"].as_str()).find(|path| path.to_lowercase().ends_with("skyrim.esm")).unwrap();
        let handle = plugin_handle_load_no_py(path, Some("skyrimse"), None, None, true).unwrap();
        let interner = StringInterner::new();
        let schema = crate::schema::AuthoringSchema::for_game("skyrimse").unwrap();
        let cache = std::path::PathBuf::from(std::env::var("MODKIT_SKYRIM_CAST_IDLE_CACHE").unwrap());
        for (family, expected) in [
            ("hagraven", [(1, 2, "Spell_FireForget_LH"), (1, 0, "Spell_FireForget_Self"), (2, 2, "Spell_Concentration_LH"), (2, 0, "Spell_Concentration_Self")].as_slice()),
            ("atronachflame", [(1, 2, "Spell_FireForget_LH"), (2, 2, "Spell_Concentration_LH")].as_slice()),
            ("draugr", [(1, 2, "MLh_SpellAimedStart"), (2, 2, "MLh_SpellAimedConcentrationStart"), (2, 0, "MLh_SpellSelfConcentrationStart")].as_slice()),
        ] {
            let data: serde_json::Value = serde_json::from_slice(&std::fs::read(cache.join(format!("{family}_idles.json"))).unwrap()).unwrap();
            let mut pending = data["data"].as_array().unwrap().iter().map(|item| format!("{}@Skyrim.esm", item["form_id"].as_str().unwrap())).collect::<Vec<_>>();
            let mut loaded = HashMap::new();
            while let Some(key) = pending.pop() {
                let key = FormKey::parse(&key, &interner).unwrap();
                if key.local == 0 || loaded.contains_key(&key) { continue; }
                let record = crate::source_read::read_record_relayout(handle, &key.format(&interner), &schema, &interner, None).unwrap();
                if let Some(value) = field(&record, b"ANAM") {
                    for name in ["parent", "previous"] { if let Some(key) = named(value, name, &interner).and_then(form_key) { pending.push(key.format(&interner)); } }
                }
                loaded.insert(key, record);
            }
            let paths = data["data"].as_array().unwrap().iter().filter_map(|item| item["fields"]["FileName"].as_str()).map(normalize).collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
            let mut events = expected.iter().map(|(_, _, event)| event.to_string()).collect::<BTreeSet<_>>();
            events.extend(["MLh_WardStart", "spellReady", "spellRelease", "Spell_Ready", "Spell_Release", "MLh_SpellReady_Event", "MLh_SpellRelease_Event"].map(str::to_string));
            let records = loaded.iter().map(|(key, record)| (*key, record)).collect::<HashMap<_, _>>();
            let actions = collect(&paths, &events, &records, &interner).unwrap();
            for &(cast, delivery, event) in expected {
                let mut spell = Record::new(SigCode(*b"SPEL"), FormKey::parse("000800@Test.esp", &interner).unwrap());
                spell.fields.push(FieldEntry { sig: SubrecordSig(*b"SPIT"), value: FieldValue::Struct(vec![(interner.intern("type"), FieldValue::Uint(0)), (interner.intern("cast_type"), FieldValue::Uint(cast)), (interner.intern("target_type"), FieldValue::Uint(delivery))]) });
                assert_eq!(stage_event(&actions, 0, "Attack", &spell, &records, &interner).unwrap().as_deref(), Some(event), "{family} cast={cast} delivery={delivery}");
                let (ready, release) = match family { "hagraven" => ("spellReady", "spellRelease"), "atronachflame" => ("Spell_Ready", "Spell_Release"), _ => ("MLh_SpellReady_Event", "MLh_SpellRelease_Event") };
                assert_eq!(stage_event(&actions, 0, "Ready", &spell, &records, &interner).unwrap().as_deref(), Some(ready));
                assert_eq!(stage_event(&actions, 0, "Release", &spell, &records, &interner).unwrap().as_deref(), Some(release));
                if family == "draugr" && cast == 2 && delivery == 0 {
                    let keyword = actions.iter().find(|action| action.event == "MLh_WardStart").unwrap().conditions.iter().flatten().find_map(|condition| condition.4).unwrap();
                    spell.fields.push(FieldEntry { sig: SubrecordSig(*b"KWDA"), value: FieldValue::List(vec![FieldValue::FormKey(keyword)]) });
                    assert_eq!(stage_event(&actions, 0, "Attack", &spell, &records, &interner).unwrap().as_deref(), Some("MLh_WardStart"));
                }
            }
        }
        assert!(plugin_handle_close_native(handle));
    }
}
