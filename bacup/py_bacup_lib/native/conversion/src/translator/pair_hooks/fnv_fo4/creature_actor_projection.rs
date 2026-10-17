use super::*;
use crate::source_rig::{CreatureNpcActorData, CreatureNpcFaction};

pub(super) fn prepare_candidate_actor_data(
    profile: &LegacyTemplateProfile,
    records: &BTreeMap<StableFormKey, &Record>,
    dependencies: &BTreeMap<StableFormKey, &RuntimeDependencyRecordReceipt>,
    mapper: &MapperState,
    interner: &StringInterner,
) -> Result<CreatureNpcActorData, String> {
    let owner = |key: &StableFormKey| stable_map_entry(records, key)
        .map(|(_, record)| *record)
        .ok_or_else(|| format!("actor category owner {key} is unavailable"));
    let root = owner(&profile.root)?;
    let traits = owner(&profile.owners[0])?;
    let factions = owner(&profile.owners[2])?;
    let ai = owner(&profile.owners[4])?;
    let packages = owner(&profile.owners[5])?;
    let (_, faction_dependencies) = stable_map_entry(dependencies, &profile.owners[2])
        .ok_or_else(|| format!("faction owner {} has no dependency receipt", profile.owners[2]))?;
    let factions = factions.fields.iter().filter(|field| field.sig.as_str() == "SNAM")
        .enumerate().map(|(index, field)| {
            let references = faction_dependencies.references.iter().filter(|reference|
                reference.source_locators.iter().any(|locator| locator_index(locator, "SNAM[") == Some(index)))
                .collect::<Vec<_>>();
            let [reference] = references.as_slice() else {
                return Err(format!("SNAM[{index}] has {} resolved faction references", references.len()));
            };
            if !matches!(&reference.resolution, DependencyReferenceResolution::Followed { signature } if signature == "FACT") {
                return Err(format!("SNAM[{index}] faction {} is not runtime-followed", reference.target));
            }
            let rank = match &field.value {
                FieldValue::Bytes(bytes) if bytes.len() == 8 => Some(i8::from_le_bytes([bytes[4]])),
                FieldValue::Struct(fields) => fields.iter().find(|(name, _)|
                    matches!(interner.resolve(*name), Some("rank" | "faction_rank" | "factions_rank")))
                    .and_then(|(_, value)| integer_value(value)).and_then(|value| i8::try_from(value).ok()),
                _ => None,
            }.ok_or_else(|| format!("SNAM[{index}].rank is undecodable"))?;
            Ok(CreatureNpcFaction {
                faction: mapped_target_reference(&reference.target, "FACT", mapper, interner)?,
                rank,
            })
        }).collect::<Result<Vec<_>, String>>()?;
    let packages = packages.fields.iter().filter(|field| field.sig.as_str() == "PKID")
        .map(|field| {
            let keys = form_keys_in_value(&field.value);
            let [key] = keys.as_slice() else { return Err("CREA.PKID is not one decoded FormKey".to_string()); };
            let plugin = interner.resolve(key.plugin).ok_or("PKID plugin symbol is unresolved")?;
            mapped_target_reference(&StableFormKey { local: key.local, plugin: plugin.to_string() }, "PACK", mapper, interner)
        }).collect::<Result<Vec<_>, String>>()?;
    let optional_reference = |signature, target_signature| exact_optional_record_form_key(traits, signature, interner)?
        .map(|source| mapped_target_reference(&source, target_signature, mapper, interner)).transpose();
    Ok(CreatureNpcActorData {
        flags: crate::target_normalize::legacy_creature_target_actor_flags(profile.actor_flags),
        disposition_base: source_disposition(root, interner)?,
        factions,
        ai_data: source_ai_data(ai, interner)?,
        packages,
        combat_style: optional_reference("ZNAM", "CSTY")?,
        voice_type: optional_reference("VTCK", "VTYP")?,
        script: None,
    })
}

fn source_disposition(record: &Record, interner: &StringInterner) -> Result<i16, String> {
    let acbs = record.fields.iter().find(|field| field.sig.as_str() == "ACBS")
        .ok_or("CREA.ACBS is absent")?;
    match &acbs.value {
        FieldValue::Bytes(bytes) if bytes.len() == 24 => Ok(i16::from_le_bytes([bytes[20], bytes[21]])),
        FieldValue::Struct(fields) => fields.iter()
            .find(|(name, _)| interner.resolve(*name) == Some("disposition_base"))
            .and_then(|(_, value)| integer_value(value)).and_then(|value| i16::try_from(value).ok())
            .ok_or_else(|| "CREA.ACBS.disposition_base is undecodable".to_string()),
        _ => Err("CREA.ACBS.disposition_base is undecodable".to_string()),
    }
}

fn source_ai_data(record: &Record, interner: &StringInterner) -> Result<Option<[u8;24]>, String> {
    let Some(field) = record.fields.iter().find(|field| field.sig.as_str() == "AIDT") else { return Ok(None); };
    let raw = match &field.value {
        FieldValue::Bytes(bytes) if bytes.len() == 20 => <[u8;20]>::try_from(bytes.as_slice()).expect("checked source AIDT size"),
        FieldValue::Struct(fields) => {
            let integer = |member| fields.iter().find(|(name, _)| interner.resolve(*name) == Some(member))
                .and_then(|(_, value)| integer_value(value)).ok_or_else(|| format!("CREA.AIDT.{member} is undecodable"));
            let mut bytes = [0_u8;20];
            for (offset, member) in [(0,"aggression"),(1,"confidence"),(2,"energy_level"),(15,"aggro_radius_behavior")] {
                bytes[offset] = u8::try_from(integer(member)?).map_err(|_| format!("CREA.AIDT.{member} is outside byte range"))?;
            }
            bytes[14] = i8::try_from(integer("assistance")?).map_err(|_| "CREA.AIDT.assistance is outside signed byte range")? as u8;
            bytes[16..20].copy_from_slice(&i32::try_from(integer("aggro_radius")?)
                .map_err(|_| "CREA.AIDT.aggro_radius is outside signed integer range")?.to_le_bytes());
            bytes
        }
        _ => return Err("CREA.AIDT does not have the source 20-byte layout".to_string()),
    };
    Ok(Some(crate::target_normalize::relayout_legacy_actor_aidt(&raw).try_into()
        .expect("shared target AIDT lowerer produces 24 bytes")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn composed_categories_keep_root_disposition_and_selected_owner_references() {
        use super::super::super::creature_catalog::CreatureProvenance;
        use super::super::super::creature_dependencies::{DependencyReferenceReceipt, PairRecordLoweringDisposition};
        let interner = StringInterner::new();
        let source_plugin = interner.intern("FalloutNV.esm");
        let target_plugin = interner.intern("Converted.esp");
        let key = |local| StableFormKey { local, plugin:"FalloutNV.esm".into() };
        let form = |local| FormKey { local, plugin:source_plugin };
        let field = |sig, value| FieldEntry { sig:SubrecordSig::from_str(sig).unwrap(), value };
        let mut root = Record::new(SigCode(*b"CREA"), form(1));
        let mut root_acbs = [0_u8;24];
        root_acbs[20..22].copy_from_slice(&17_i16.to_le_bytes());
        root.fields.push(field("ACBS", FieldValue::Bytes(root_acbs.to_vec().into())));
        let mut traits = Record::new(SigCode(*b"CREA"), form(2));
        traits.fields.extend([field("ZNAM", FieldValue::FormKey(form(40))), field("VTCK", FieldValue::FormKey(form(41)))]);
        let mut faction = Record::new(SigCode(*b"CREA"), form(3));
        faction.fields.push(field("SNAM", FieldValue::Bytes(vec![42,0,0,0,255,77,88,99].into())));
        let mut ai = Record::new(SigCode(*b"CREA"), form(4));
        let mut ai_bytes = [0_u8;20];
        ai_bytes[0]=2; ai_bytes[1]=4; ai_bytes[14]=1;
        ai.fields.push(field("AIDT", FieldValue::Bytes(ai_bytes.to_vec().into())));
        let mut package = Record::new(SigCode(*b"CREA"), form(5));
        package.fields.extend([field("PKID", FieldValue::FormKey(form(44))),field("PKID", FieldValue::FormKey(form(43)))]);
        let mut profile = LegacyTemplateProfile { root:key(1), owners:std::array::from_fn(|_|key(1)), actor_flags:0x0020_0002 };
        profile.owners[0]=key(2); profile.owners[2]=key(3); profile.owners[4]=key(4); profile.owners[5]=key(5);
        let records = [(&root,1),(&traits,2),(&faction,3),(&ai,4),(&package,5)].into_iter().map(|(record,id)|(key(id),record)).collect();
        let dependency = RuntimeDependencyRecordReceipt {
            source:key(3), signature:"CREA".into(),
            provenance:CreatureProvenance { game:LegacyCreatureGame::Fnv, source_plugin:"FalloutNV.esm".into(), precedence:0 },
            lowering:PairRecordLoweringDisposition::Ready { mechanism:"fixture".into() }, degradations:Vec::new(),
            references:vec![DependencyReferenceReceipt { target:key(42), source_locators:vec!["SNAM[0].faction@0".into()], resolution:DependencyReferenceResolution::Followed {signature:"FACT".into()} }],
        };
        let dependencies = BTreeMap::from([(key(3),&dependency)]);
        let mut mapper = MapperState::new(std::iter::empty(), crate::formkey_mapper::MapperOptions { output_plugin_name:"Converted.esp".into(), ..Default::default() });
        for local in 40..=44 { mapper.source_to_target.insert(form(local), FormKey {local:local+0x800,plugin:target_plugin}); }
        let actual = prepare_candidate_actor_data(&profile,&records,&dependencies,&mapper,&interner).unwrap();
        assert_eq!(actual.disposition_base,17);
        assert_eq!(actual.flags,crate::target_normalize::legacy_creature_target_actor_flags(profile.actor_flags));
        assert_eq!(actual.combat_style.unwrap().form_key.local,0x828);
        assert_eq!(actual.voice_type.unwrap().form_key.local,0x829);
        assert_eq!(actual.factions[0].faction.form_key.local,0x82A);
        assert_eq!(actual.factions[0].rank,-1);
        assert_eq!(actual.ai_data.unwrap()[..2],[2,4]);
        assert_eq!(actual.packages.iter().map(|entry|entry.form_key.local).collect::<Vec<_>>(),[0x82C,0x82B]);
    }

    #[test]
    fn source_ai_struct_and_bytes_have_identical_target_semantics() {
        let interner = StringInterner::new();
        let mut record = Record::new(SigCode::from_str("CREA").unwrap(), FormKey { local:1, plugin:interner.intern("FalloutNV.esm") });
        let mut raw = [0_u8;20];
        raw[0] = 2; raw[1] = 3; raw[2] = 50; raw[14] = 2; raw[15] = 1;
        raw[16..20].copy_from_slice(&1500_i32.to_le_bytes());
        record.fields.push(FieldEntry { sig:SubrecordSig::from_str("AIDT").unwrap(), value:FieldValue::Bytes(raw.to_vec().into()) });
        let expected = source_ai_data(&record, &interner).unwrap().unwrap();
        record.fields[0].value = FieldValue::Struct([
            ("aggression",2), ("confidence",3), ("energy_level",50), ("assistance",2), ("aggro_radius_behavior",1), ("aggro_radius",1500),
        ].into_iter().map(|(name,value)| (interner.intern(name), FieldValue::Int(value))).collect());
        assert_eq!(source_ai_data(&record, &interner).unwrap(), Some(expected));
        assert_eq!(&expected[..7], &[2,3,50,3,0,2,1]);
        for offset in [8,12,16] { assert_eq!(u32::from_le_bytes(expected[offset..offset+4].try_into().unwrap()),1500); }
    }
}
