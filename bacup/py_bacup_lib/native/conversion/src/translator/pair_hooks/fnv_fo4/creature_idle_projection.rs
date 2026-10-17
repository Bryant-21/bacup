use std::collections::{BTreeSet, HashMap, HashSet};

use crate::formkey_mapper::FormKeyMapper;
use crate::ids::{FormKey, SubrecordSig};
use crate::record::{FieldEntry, FieldValue, Record};
use crate::sym::StringInterner;
use crate::translator::pair_hook::PairCtx;
use crate::translator::pair_hooks::fnv_conditions::{
    LegacyConditionFamily, normalize_legacy_condition_value,
};

use super::creature_catalog::{LegacyCreatureGame, LegacyRecordSource, RigFamilyKey, StableFormKey};
use super::creature_source_idles::{source_family_behavior_path, source_idle_event};
use super::legacy_idles::classify_legacy_idle;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct LegacyCreatureIdleBinding {
    pub rigs: BTreeSet<RigFamilyKey>,
    pub source_kf: String,
    pub previous: Option<FormKey>,
}

impl LegacyCreatureIdleBinding {
    fn rig(&self) -> &RigFamilyKey {
        self.rigs.first().expect("source IDLE bindings have a rig")
    }
}

fn merge_binding(
    output: &mut HashMap<FormKey, LegacyCreatureIdleBinding>,
    key: FormKey,
    binding: LegacyCreatureIdleBinding,
) -> Result<bool, String> {
    if let Some(previous) = output.get_mut(&key) {
        if previous.rig().game != binding.rig().game
            || path_key(&previous.source_kf) != path_key(&binding.source_kf)
        {
            return Err(format!("source IDLE {:06X} has conflicting behavior routes", key.local));
        }
        let count = previous.rigs.len();
        previous.rigs.extend(binding.rigs);
        Ok(previous.rigs.len() != count)
    } else {
        output.insert(key, binding);
        Ok(true)
    }
}

fn path_key(value: &str) -> String {
    let value = value.replace('\\', "/").to_ascii_lowercase();
    value.strip_prefix("meshes/").unwrap_or(&value).to_string()
}

pub(crate) fn source_idle_bindings(
    sources: &[LegacyRecordSource<'_>],
    routes: &[(RigFamilyKey, StableFormKey, String)],
    load_orders: &[super::creature_dependencies::LegacyPluginLoadOrder],
    interner: &StringInterner,
) -> Result<HashMap<FormKey, LegacyCreatureIdleBinding>, String> {
    let mut winners = HashMap::new();
    for source in sources.iter().filter(|source| source.record.sig.as_str() == "IDLE") {
        let slot = winners.entry(source.record.form_key).or_insert(source);
        if source.provenance.precedence >= slot.provenance.precedence { *slot = source; }
    }
    let mut output = HashMap::new();
    for (rig, source_key, source_kf) in routes {
        let source = winners.values().find(|source| source.provenance.game == rig.game
            && source.record.form_key.local == source_key.local
            && interner.resolve(source.record.form_key.plugin).is_some_and(|plugin| plugin.eq_ignore_ascii_case(&source_key.plugin)))
            .ok_or_else(|| format!("source IDLE {source_key} is unavailable"))?;
        let key = source.record.form_key;
        if source.provenance.game != rig.game { return Err(format!("source IDLE {source_key} game differs from its rig")); }
        let Some(path) = source.record.fields.iter().find(|field| field.sig.as_str() == "MODL")
            .and_then(|field| match field.value { FieldValue::String(value) => interner.resolve(value), _ => None })
        else { return Err(format!("source IDLE {source_key} has no model")); };
        if path_key(path) != path_key(source_kf) { return Err(format!("source IDLE {source_key} does not own selected KF {source_kf}")); }
        let support = classify_legacy_idle(source.record);
        if !support.is_ready() {
            return Err(format!("source creature IDLE {:06X}: {:?}", key.local, support.reason_codes));
        }
        let binding = LegacyCreatureIdleBinding { rigs: BTreeSet::from([rig.clone()]), source_kf: path.to_string(), previous: None };
        merge_binding(&mut output, key, binding)?;
    }
    let mut queue = output.keys().copied().collect::<Vec<_>>();
    queue.sort_by_key(|key| (interner.resolve(key.plugin).unwrap_or(""), key.local));
    while let Some(key) = queue.pop() {
        let source = winners[&key];
        let binding = output[&key].clone();
        let order = load_orders.iter().find(|order| order.game == source.provenance.game && order.source_plugin.eq_ignore_ascii_case(&source.provenance.source_plugin))
            .ok_or_else(|| format!("source IDLE {:06X} load order is missing", key.local))?;
        let family = match binding.rig().game { LegacyCreatureGame::Fnv => LegacyConditionFamily::Fnv, LegacyCreatureGame::Fo3 => LegacyConditionFamily::Fo3 };
        let references = super::legacy_idles::legacy_idle_embedded_references(source.record, family, &order.source_plugin, &order.source_master_names, interner)?;
        for mut reference in references.into_iter().filter(|reference| reference.locator.starts_with("ANAM[")) {
            let is_previous = reference.locator.ends_with(".previous@4");
            let mut visited = HashSet::new();
            while is_previous && !output.contains_key(&reference.form_key) {
                if !visited.insert(reference.form_key) {
                    return Err(format!("source IDLE {:06X} has a cyclic previous chain", key.local));
                }
                let linked = winners.get(&reference.form_key).ok_or_else(|| format!("source IDLE {:06X} topology record is missing", reference.form_key.local))?;
                let path = linked.record.fields.iter().find(|field| field.sig.as_str() == "MODL")
                    .and_then(|field| match field.value { FieldValue::String(path) => interner.resolve(path), _ => None }).unwrap_or("");
                if !path.to_ascii_lowercase().ends_with(".kf") { break; }
                // Previous orders siblings; it does not require an unselected clip to play.
                let linked_order = load_orders.iter().find(|order| order.game == linked.provenance.game && order.source_plugin.eq_ignore_ascii_case(&linked.provenance.source_plugin))
                    .ok_or_else(|| format!("source IDLE {:06X} load order is missing", reference.form_key.local))?;
                let previous = super::legacy_idles::legacy_idle_embedded_references(linked.record, family, &linked_order.source_plugin, &linked_order.source_master_names, interner)?
                    .into_iter().find(|reference| reference.locator.ends_with(".previous@4"));
                let Some(previous) = previous else { break; };
                reference = previous;
            }
            if is_previous {
                let linked = winners[&reference.form_key];
                let is_kf = linked.record.fields.iter().find(|field| field.sig.as_str() == "MODL")
                    .is_some_and(|field| matches!(field.value, FieldValue::String(path) if interner.resolve(path).is_some_and(|path| path.to_ascii_lowercase().ends_with(".kf"))));
                if is_kf && !output.contains_key(&reference.form_key) { continue; }
                output.get_mut(&key).unwrap().previous = Some(reference.form_key);
            }
            if let Some(existing) = output.get(&reference.form_key) {
                if existing.source_kf.to_ascii_lowercase().ends_with(".kf") { continue; }
                let linked_binding = LegacyCreatureIdleBinding { rigs: binding.rigs.clone(), source_kf: existing.source_kf.clone(), previous: existing.previous };
                if merge_binding(&mut output, reference.form_key, linked_binding)? {
                    queue.push(reference.form_key);
                }
                continue;
            }
            let linked = winners.get(&reference.form_key).ok_or_else(|| format!("source IDLE {:06X} topology record is missing", reference.form_key.local))?;
            let path = linked.record.fields.iter().find(|field| field.sig.as_str() == "MODL")
                .and_then(|field| match field.value { FieldValue::String(path) => interner.resolve(path), _ => None }).unwrap_or("");
            if path.to_ascii_lowercase().ends_with(".kf") {
                return Err(format!("source IDLE {:06X} links to a KF without a selected graph route: {path}", reference.form_key.local));
            }
            let support = classify_legacy_idle(linked.record);
            if !support.is_ready() { return Err(format!("source IDLE container {:06X}: {:?}", reference.form_key.local, support.reason_codes)); }
            output.insert(reference.form_key, LegacyCreatureIdleBinding { rigs: binding.rigs.clone(), source_kf: path.to_string(), previous: None });
            queue.push(reference.form_key);
        }
    }
    Ok(output)
}

pub(crate) fn lower_source_idle(
    record: &mut Record,
    binding: &LegacyCreatureIdleBinding,
    ctx: &PairCtx<'_>,
) -> Result<(), String> {
    if record.sig.as_str() != "IDLE" { return Err("source idle binding has a non-IDLE record".into()); }
    let rig = binding.rig();
    let source = StableFormKey {
        local: record.form_key.local,
        plugin: ctx.interner.resolve(record.form_key.plugin).ok_or("source IDLE plugin is unavailable")?.to_string(),
    };
    let container = !binding.source_kf.to_ascii_lowercase().ends_with(".kf");
    let mut fields = Vec::new();
    for field in &record.fields {
        match field.sig.as_str() {
            "EDID" => fields.push(field.clone()),
            "CTDA" | "CTDT" => {
                let FieldValue::Bytes(bytes) = &field.value else { return Err("source IDLE condition is not raw".into()); };
                if !matches!(bytes.len(), 20 | 28) { return Err("source IDLE condition length is unsupported".into()); }
                let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
                let family = match rig.game { LegacyCreatureGame::Fnv => LegacyConditionFamily::Fnv, LegacyCreatureGame::Fo3 => LegacyConditionFamily::Fo3 };
                if crate::translator::pair_hooks::fnv_conditions::translate_condition_function(family, function).is_none() {
                    return Err(format!("source IDLE condition {function} has no equivalent target function"));
                }
                fields.push(field.clone());
            }
            "MODL" => {
                let FieldValue::String(path) = field.value else { return Err("source IDLE model is not a path".into()); };
                if ctx.interner.resolve(path).is_none_or(|path| path_key(path) != path_key(&binding.source_kf)) {
                    return Err("source IDLE path changed after admission".into());
                }
            }
            "ANAM" => {
                let FieldValue::Bytes(bytes) = &field.value else { return Err("source IDLE ANAM is not raw".into()); };
                if bytes.len() != 8 { return Err("source IDLE ANAM must contain parent and previous".into()); }
                let mut links = Vec::new();
                for (offset, name) in [(0, "parent"), (4, "previous")] {
                    let raw = u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap());
                    let value = if name == "previous" {
                        binding.previous.map(FieldValue::FormKey).unwrap_or(FieldValue::Uint(0))
                    } else if raw == 0 { FieldValue::Uint(0) } else {
                        FieldValue::FormKey(ctx.resolve_source_form_id(raw).ok_or_else(|| format!("source IDLE {name} {raw:08X} has no load-order owner"))?)
                    };
                    links.push((ctx.interner.intern(name), value));
                }
                fields.push(FieldEntry { sig: SubrecordSig(*b"ANAM"), value: FieldValue::Struct(links) });
            }
            "DATA" => {
                let FieldValue::Bytes(bytes) = &field.value else { return Err("source IDLE DATA is not raw".into()); };
                if !matches!(bytes.len(), 6 | 8) { return Err("source IDLE DATA length is unsupported".into()); }
                let flags = bytes.get(6).copied().unwrap_or(0);
                let target = vec![bytes[1], bytes[2], ((flags & 1) << 2) | u8::from(container), 0, bytes[4], bytes[5]];
                fields.push(FieldEntry { sig: SubrecordSig(*b"DATA"), value: FieldValue::Bytes(target.into()) });
            }
            "MODB" => {}
            other => return Err(format!("source IDLE field {other} is unsupported")),
        }
    }
    let insertion = fields.iter().position(|field| field.sig.as_str() == "ANAM").unwrap_or(fields.len());
    // Explicit FO4 idles dispatch ENAM on the actor's graph (AIProcess::SetupSpecialIdle);
    // every claiming rig installs this same event, so DNAM can use a stable representative.
    fields.splice(insertion..insertion, [
        FieldEntry { sig: SubrecordSig(*b"DNAM"), value: FieldValue::String(ctx.interner.intern(&if container { String::new() } else { source_family_behavior_path(rig) })) },
        FieldEntry { sig: SubrecordSig(*b"ENAM"), value: FieldValue::String(ctx.interner.intern(&if container { String::new() } else { source_idle_event(rig.game, &source) })) },
    ]);
    record.fields = fields.into_iter().collect();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formkey_mapper::MapperOptions;
    use crate::ids::SigCode;

    fn raven(interner: &StringInterner) -> (Record, LegacyCreatureIdleBinding) {
        let binding = LegacyCreatureIdleBinding {
            previous: None,
            rigs: BTreeSet::from([RigFamilyKey { game: LegacyCreatureGame::Fnv, skeleton_path: "creatures/nvraven/skeleton.nif".into() }]),
            source_kf: "creatures\\NVRaven\\IdleAnims\\SpecialIdle_TurnLeft.kf".into(),
        };
        let mut record = Record::new(SigCode::from_str("IDLE").unwrap(), FormKey { local: 0x140DBA, plugin: interner.intern("FalloutNV.esm") });
        record.fields.extend([
            FieldEntry { sig: SubrecordSig(*b"EDID"), value: FieldValue::String(interner.intern("NVRavenLookRight")) },
            FieldEntry { sig: SubrecordSig(*b"MODL"), value: FieldValue::String(interner.intern(&binding.source_kf)) },
        ]);
        for (function, operator) in [(77u16, 0xa0), (289, 0x20)] {
            let mut bytes = vec![0; 28];
            bytes[0] = operator;
            bytes[4..8].copy_from_slice(&1.0f32.to_le_bytes());
            bytes[8..10].copy_from_slice(&function.to_le_bytes());
            record.fields.push(FieldEntry { sig: SubrecordSig(*b"CTDA"), value: FieldValue::Bytes(bytes.into()) });
        }
        record.fields.extend([
            FieldEntry { sig: SubrecordSig(*b"ANAM"), value: FieldValue::Bytes(vec![0; 8].into()) },
            FieldEntry { sig: SubrecordSig(*b"DATA"), value: FieldValue::Bytes(vec![0x87, 0, 0, 0, 0, 0, 0, 86].into()) },
        ]);
        (record, binding)
    }

    #[test]
    fn previous_siblings_without_graph_routes_are_bypassed_but_parents_are_required() {
        use super::super::creature_catalog::CreatureProvenance;
        use super::super::creature_dependencies::LegacyPluginLoadOrder;

        let interner = StringInterner::new();
        let (mut left, binding) = raven(&interner);
        left.form_key.local = 0x187EA7;
        left.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([0x187EAEu32.to_le_bytes(), 0x187EA6u32.to_le_bytes()].concat().into());
        let mut right = left.clone();
        right.form_key.local = 0x187EA6;
        right.fields.iter_mut().find(|field| field.sig.as_str() == "MODL").unwrap().value = FieldValue::String(interner.intern("NVDLC03\\Creatures\\Roboscorpion\\IdleAnims\\mtspecialidle_knockdownfaceright.kf"));
        let mut parent = left.clone();
        parent.form_key.local = 0x187EAE;
        parent.fields.iter_mut().find(|field| field.sig.as_str() == "MODL").unwrap().value = FieldValue::String(interner.intern("NVDLC03\\Creatures\\Roboscorpion\\IdleAnims"));
        parent.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes(vec![0; 8].into());
        let routes = [(binding.rig().clone(), StableFormKey { local: left.form_key.local, plugin: "FalloutNV.esm".into() }, binding.source_kf.clone())];
        let orders = [LegacyPluginLoadOrder { game: LegacyCreatureGame::Fnv, source_plugin: "FalloutNV.esm".into(), source_master_names: Vec::new() }];
        for previous in [0, parent.form_key.local] {
            right.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([parent.form_key.local.to_le_bytes(), previous.to_le_bytes()].concat().into());
            let sources = [&left, &right, &parent].map(|record| LegacyRecordSource {
                record,
                provenance: CreatureProvenance { game: LegacyCreatureGame::Fnv, source_plugin: "FalloutNV.esm".into(), precedence: 0 },
            });
            let bindings = source_idle_bindings(&sources, &routes, &orders, &interner).unwrap();
            assert_eq!(bindings.len(), 2);
            assert!(!bindings.contains_key(&right.form_key));
            let mut lowered = left.clone();
            lower_source_idle(&mut lowered, &bindings[&left.form_key], &PairCtx::with_source(&interner, "FalloutNV.esm", &[])).unwrap();
            let FieldValue::Struct(links) = &lowered.fields.iter().find(|field| field.sig.as_str() == "ANAM").unwrap().value else { panic!("typed topology"); };
            assert_eq!(links[0].1, FieldValue::FormKey(parent.form_key));
            assert_eq!(links[1].1, if previous == 0 { FieldValue::Uint(0) } else { FieldValue::FormKey(parent.form_key) });
        }
        left.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([right.form_key.local.to_le_bytes(), 0u32.to_le_bytes()].concat().into());
        let sources = [&left, &right, &parent].map(|record| LegacyRecordSource {
            record,
            provenance: CreatureProvenance { game: LegacyCreatureGame::Fnv, source_plugin: "FalloutNV.esm".into(), precedence: 0 },
        });
        assert!(source_idle_bindings(&sources, &routes, &orders, &interner).unwrap_err().contains("without a selected graph route"));
    }

    #[test]
    fn shared_source_idle_keeps_all_rig_owners_and_converges_through_container_cycles() {
        use super::super::creature_catalog::CreatureProvenance;
        use super::super::creature_dependencies::LegacyPluginLoadOrder;

        let interner = StringInterner::new();
        let (mut idle, _) = raven(&interner);
        idle.form_key.local = 0x03397A;
        let source_kf = "Creatures\\Centaur\\IdleAnims\\SpecialIdle_HitArmRight.kf";
        idle.fields.iter_mut().find(|field| field.sig.as_str() == "MODL").unwrap().value = FieldValue::String(interner.intern(source_kf));
        idle.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([0x033977u32.to_le_bytes(), 0u32.to_le_bytes()].concat().into());
        let mut parent = idle.clone();
        parent.form_key.local = 0x033977;
        parent.fields.iter_mut().find(|field| field.sig.as_str() == "MODL").unwrap().value = FieldValue::String(interner.intern(""));
        parent.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([0u32.to_le_bytes(), 0x033978u32.to_le_bytes()].concat().into());
        let mut previous = parent.clone();
        previous.form_key.local = 0x033978;
        previous.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([0x033977u32.to_le_bytes(), 0u32.to_le_bytes()].concat().into());
        let sources = [&idle, &parent, &previous].map(|record| LegacyRecordSource {
            record,
            provenance: CreatureProvenance { game: LegacyCreatureGame::Fnv, source_plugin: "FalloutNV.esm".into(), precedence: 0 },
        });
        let first = RigFamilyKey { game: LegacyCreatureGame::Fnv, skeleton_path: "creatures/centaur/skeleton.nif".into() };
        let second = RigFamilyKey { game: LegacyCreatureGame::Fnv, skeleton_path: "creatures/centaur/variant/skeleton.nif".into() };
        let source = StableFormKey { local: idle.form_key.local, plugin: "FalloutNV.esm".into() };
        let mut routes = [second.clone(), first.clone(), second.clone()].map(|rig| (rig, source.clone(), source_kf.into()));
        let orders = [LegacyPluginLoadOrder { game: LegacyCreatureGame::Fnv, source_plugin: "FalloutNV.esm".into(), source_master_names: Vec::new() }];
        let bindings = source_idle_bindings(&sources, &routes, &orders, &interner).unwrap();
        assert_eq!(bindings.len(), 3);
        for binding in bindings.values() {
            assert_eq!(binding.rigs, BTreeSet::from([first.clone(), second.clone()]));
        }
        routes.reverse();
        assert_eq!(source_idle_bindings(&sources, &routes, &orders, &interner).unwrap(), bindings);
        let mut lowered = idle.clone();
        lower_source_idle(&mut lowered, &bindings[&idle.form_key], &PairCtx::with_source(&interner, "FalloutNV.esm", &[])).unwrap();
        for (signature, expected) in [("DNAM", source_family_behavior_path(&first)), ("ENAM", source_idle_event(LegacyCreatureGame::Fnv, &source))] {
            let FieldValue::String(value) = lowered.fields.iter().find(|field| field.sig.as_str() == signature).unwrap().value else { panic!("source idle path/event"); };
            assert_eq!(interner.resolve(value), Some(expected.as_str()));
        }
        routes[0].2 = "Creatures\\Centaur\\Other.kf".into();
        assert!(source_idle_bindings(&sources, &routes, &orders, &interner).unwrap_err().contains("does not own selected KF"));
    }

    #[test]
    fn shared_idle_binding_rejects_different_games_and_kfs() {
        let interner = StringInterner::new();
        let (record, binding) = raven(&interner);
        let mut output = HashMap::from([(record.form_key, binding.clone())]);
        let mut different_kf = binding.clone();
        different_kf.source_kf = "other.kf".into();
        assert!(merge_binding(&mut output, record.form_key, different_kf).is_err());
        let mut different_game = binding.clone();
        different_game.rigs = BTreeSet::from([RigFamilyKey { game: LegacyCreatureGame::Fo3, skeleton_path: binding.rig().skeleton_path.clone() }]);
        assert!(merge_binding(&mut output, record.form_key, different_game).is_err());
        assert_eq!(output[&record.form_key], binding);
    }

    #[test]
    fn raven_source_idle_keeps_condition_operators_and_uses_its_exact_graph_event() {
        let interner = StringInterner::new();
        let (mut record, binding) = raven(&interner);
        let conditions = record.fields.iter().filter(|field| field.sig.as_str() == "CTDA").map(|field| field.value.clone()).collect::<Vec<_>>();
        lower_source_idle(&mut record, &binding, &PairCtx::with_source(&interner, "FalloutNV.esm", &[])).unwrap();
        let mut mapper = FormKeyMapper::new(std::iter::empty(), MapperOptions { source_plugin_name: "FalloutNV.esm".into(), output_plugin_name: "Converted.esm".into(), ..MapperOptions::default() }, &interner);
        normalize_source_idle_conditions(&mut record, LegacyCreatureGame::Fnv, &mut mapper).unwrap();
        for (field, source) in record.fields.iter().filter(|field| field.sig.as_str() == "CTDA").zip(conditions) {
            let (FieldValue::Bytes(target), FieldValue::Bytes(source)) = (&field.value, source) else { panic!("raw conditions"); };
            assert_eq!(&target[..28], source.as_slice());
            assert_eq!(&target[28..], &(-1i32).to_le_bytes());
        }
        for (signature, expected) in [("DNAM", source_family_behavior_path(binding.rig())), ("ENAM", source_idle_event(binding.rig().game, &StableFormKey { local: 0x140DBA, plugin: "FalloutNV.esm".into() }))] {
            let FieldValue::String(value) = record.fields.iter().find(|field| field.sig.as_str() == signature).unwrap().value else { panic!("string"); };
            assert_eq!(interner.resolve(value), Some(expected.as_str()));
        }
        assert!(!record.fields.iter().any(|field| field.sig.as_str() == "MODL"));
    }

    #[test]
    fn source_idle_links_are_load_order_aware_and_no_attacking_changes_bit() {
        let interner = StringInterner::new();
        let (mut record, mut binding) = raven(&interner);
        binding.previous = Some(FormKey { local: 0x5678, plugin: interner.intern("FalloutNV.esm") });
        record.fields.iter_mut().find(|field| field.sig.as_str() == "ANAM").unwrap().value = FieldValue::Bytes([0x00001234u32.to_le_bytes(), 0x01005678u32.to_le_bytes()].concat().into());
        record.fields.iter_mut().find(|field| field.sig.as_str() == "DATA").unwrap().value = FieldValue::Bytes(vec![7, 2, 3, 0, 4, 0, 1, 0].into());
        lower_source_idle(&mut record, &binding, &PairCtx::with_source(&interner, "FalloutNV.esm", &["Master.esm".into()])).unwrap();
        let FieldValue::Struct(links) = &record.fields.iter().find(|field| field.sig.as_str() == "ANAM").unwrap().value else { panic!("links"); };
        assert_eq!(links[0].1, FieldValue::FormKey(FormKey { local: 0x1234, plugin: interner.intern("Master.esm") }));
        assert_eq!(links[1].1, FieldValue::FormKey(FormKey { local: 0x5678, plugin: interner.intern("FalloutNV.esm") }));
        assert_eq!(record.fields.iter().find(|field| field.sig.as_str() == "DATA").unwrap().value, FieldValue::Bytes(vec![2, 3, 4, 0, 4, 0].into()));
    }
}

pub(crate) fn normalize_source_idle_conditions(
    record: &mut Record,
    game: LegacyCreatureGame,
    mapper: &mut FormKeyMapper<'_>,
) -> Result<(), String> {
    let family = match game { LegacyCreatureGame::Fnv => LegacyConditionFamily::Fnv, LegacyCreatureGame::Fo3 => LegacyConditionFamily::Fo3 };
    for field in &mut record.fields {
        if !matches!(field.sig.as_str(), "CTDA" | "CTDT") { continue; }
        let mut value = field.value.clone();
        if let FieldValue::Bytes(bytes) = &mut value
            && bytes.len() == 20
        { bytes.resize(28, 0); }
        field.value = normalize_legacy_condition_value(&value, family, mapper)
            .map_err(|error| format!("source creature IDLE condition cannot be preserved: {error:?}"))?.0;
        field.sig = SubrecordSig(*b"CTDA");
    }
    Ok(())
}
