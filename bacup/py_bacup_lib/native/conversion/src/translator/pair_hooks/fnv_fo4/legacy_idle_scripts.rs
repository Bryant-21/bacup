use std::collections::BTreeSet;

use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

pub(crate) fn source_playidle_names(record: &Record, interner: &StringInterner) -> Result<BTreeSet<String>, String> {
    let mut output = BTreeSet::new();
    if let Some(FieldValue::Bytes(bytes)) = record.fields.iter().find(|field| field.sig.as_str() == "SCDA").map(|field| &field.value) {
        let mut offset = 0;
        while offset + 4 <= bytes.len() {
            let opcode = u16::from_le_bytes(bytes[offset..offset + 2].try_into().unwrap());
            if opcode == 0x1c {
                offset += 4;
                continue;
            }
            let length = u16::from_le_bytes(bytes[offset + 2..offset + 4].try_into().unwrap()) as usize;
            offset += 4;
            let end = offset.checked_add(length).filter(|end| *end <= bytes.len()).ok_or("source SCDA command is truncated")?;
            if opcode == 0x1190 {
                let args = &bytes[offset..end];
                if args.len() < 4 || u16::from_le_bytes(args[..2].try_into().unwrap()) != 1 {
                    return Err("source PlayIdle command argument layout is unsupported".into());
                }
                let name_len = u16::from_le_bytes(args[2..4].try_into().unwrap()) as usize;
                if name_len + 4 != args.len() { return Err("source PlayIdle name length is invalid".into()); }
                let name = std::str::from_utf8(&args[4..]).map_err(|_| "source PlayIdle EditorID is not UTF-8")?;
                output.insert(name.to_ascii_lowercase());
            }
            offset = end;
        }
        if offset != bytes.len() { return Err("source SCDA trailing command is truncated".into()); }
        return Ok(output);
    }
    let source = record.fields.iter().find(|field| field.sig.as_str() == "SCTX").and_then(|field| match &field.value {
        FieldValue::String(value) => interner.resolve(*value).map(str::to_string),
        FieldValue::Bytes(bytes) => std::str::from_utf8(bytes).ok().map(|text| text.trim_end_matches('\0').to_string()),
        _ => None,
    });
    if let Some(source) = source {
        let script = fnv_script_native::parser::parse_script(&source).map_err(|error| error.to_string())?;
        output.extend(crate::fnv_legacy_scripting::script_synthesizer::source_idle_argument_symbols(&script));
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    #[test]
    fn compiled_playidle_name_is_not_a_scro_reference_or_an_arbitrary_string_scan() {
        let interner = StringInterner::new();
        let mut record = Record::new(SigCode::from_str("SCPT").unwrap(), FormKey { local: 0x14c124, plugin: interner.intern("FalloutNV.esm") });
        let name = b"NVRavenFlyAway";
        let mut bytes = vec![0x1d, 0, 0, 0, 0x1c, 0, 1, 0];
        bytes.extend_from_slice(&0x1190u16.to_le_bytes());
        bytes.extend_from_slice(&((name.len() + 4) as u16).to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
        bytes.extend_from_slice(name);
        record.fields.push(FieldEntry { sig: SubrecordSig(*b"SCDA"), value: FieldValue::Bytes(bytes.into()) });
        assert_eq!(source_playidle_names(&record, &interner).unwrap().into_iter().collect::<Vec<_>>(), ["nvravenflyaway"]);
        let FieldValue::Bytes(bytes) = &mut record.fields[0].value else { unreachable!() };
        bytes[8..10].copy_from_slice(&0x1000u16.to_le_bytes());
        assert!(source_playidle_names(&record, &interner).unwrap().is_empty());
    }

    #[test]
    #[ignore = "requires exact CLI creature SCPT record corpus"]
    fn source_creature_scda_playidle_corpus() {
        let path = std::env::var("MODKIT_FNV_CREATURE_SCRIPT_RECORDS").unwrap();
        let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let records = data["data"].as_array().unwrap();
        assert_eq!(records.len(), 108);
        let interner = StringInterner::new();
        let mut scripted_idles = 0;
        for source in records {
            let hex = source["fields"]["CompiledScript"].as_str().unwrap_or("");
            let bytes = hex.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect::<Vec<_>>();
            let mut record = Record::new(SigCode::from_str("SCPT").unwrap(), FormKey { local: u32::from_str_radix(source["form_id"].as_str().unwrap(), 16).unwrap(), plugin: interner.intern("FalloutNV.esm") });
            record.fields.push(FieldEntry { sig: SubrecordSig(*b"SCDA"), value: FieldValue::Bytes(bytes.into()) });
            let names = source_playidle_names(&record, &interner).unwrap_or_else(|error| panic!("{}: {error}", source["eid"]));
            scripted_idles += usize::from(!names.is_empty());
        }
        assert_eq!(scripted_idles, 11);
    }

    #[test]
    #[ignore = "requires exact CLI source record cache"]
    fn source_raven_scda_playidle_corpus() {
        let path = std::env::var("MODKIT_FNV_BLACKBIRD_SOURCE_RECORD").unwrap();
        let data: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let source: serde_json::Value = serde_saphyr::from_str(data["yaml_content"].as_str().unwrap()).unwrap();
        let hex = source["fields"].as_array().unwrap().iter().find_map(|field| field["CompiledScript"].as_str()).unwrap();
        let bytes = hex.as_bytes().chunks_exact(2).map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap()).collect::<Vec<_>>();
        assert_eq!(bytes.len(), 378);
        let interner = StringInterner::new();
        let mut record = Record::new(SigCode::from_str("SCPT").unwrap(), FormKey { local: 0x14c124, plugin: interner.intern("FalloutNV.esm") });
        record.fields.push(FieldEntry { sig: SubrecordSig(*b"SCDA"), value: FieldValue::Bytes(bytes.into()) });
        assert_eq!(source_playidle_names(&record, &interner).unwrap().into_iter().collect::<Vec<_>>(), ["nvravenflyaway"]);
    }
}
