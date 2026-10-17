use std::collections::{HashMap, HashSet};

use crate::ids::FormKey;
use crate::record::{FieldValue, Record};
use crate::sym::StringInterner;

use super::creature_mvp_adapter::{embedded_form_ids, resolve_embedded_form_id};

pub(crate) fn actor_flags(record: &Record, interner: &StringInterner) -> Result<u32, String> {
    config_word(record, "flags", 0, 4, interner)
}

pub(crate) fn template_flags(record: &Record, interner: &StringInterner) -> Result<u16, String> {
    Ok(config_word(record, "templateflags", 18, 2, interner)? as u16)
}

fn config_word(
    record: &Record,
    name: &str,
    offset: usize,
    width: usize,
    interner: &StringInterner,
) -> Result<u32, String> {
    let data = record
        .fields
        .iter()
        .find(|field| field.sig.0 == *b"ACBS")
        .ok_or("source NPC has no ACBS")?;
    match &data.value {
        FieldValue::Bytes(bytes) if bytes.len() == 24 => {
            let mut word = [0; 4];
            word[..width].copy_from_slice(&bytes[offset..offset + width]);
            Ok(u32::from_le_bytes(word))
        }
        FieldValue::Struct(fields) => fields
            .iter()
            .find_map(|(key, value)| {
                let canonical = interner
                    .resolve(*key)?
                    .chars()
                    .filter(char::is_ascii_alphanumeric)
                    .flat_map(char::to_lowercase)
                    .collect::<String>();
                if canonical != name {
                    return None;
                }
                match value {
                    FieldValue::Uint(value) => u32::try_from(*value).ok(),
                    FieldValue::Int(value) => u32::try_from(*value).ok(),
                    _ => None,
                }
            })
            .ok_or_else(|| format!("source NPC ACBS has no numeric {name}")),
        _ => Err("source NPC ACBS has an unsupported layout".into()),
    }
}

pub(crate) fn owners<'a>(
    record: &'a Record,
    category: u16,
    records: &HashMap<FormKey, &'a Record>,
    interner: &StringInterner,
) -> Result<Vec<&'a Record>, String> {
    fn visit<'a>(
        record: &'a Record,
        category: u16,
        records: &HashMap<FormKey, &'a Record>,
        interner: &StringInterner,
        path: &mut HashSet<FormKey>,
        output: &mut Vec<&'a Record>,
    ) -> Result<(), String> {
        if !path.insert(record.form_key) {
            return Err(format!(
                "NPC template cycle at {}",
                record.form_key.format(interner)
            ));
        }
        let mut targets = Vec::new();
        match &record.sig.0 {
            b"NPC_" if template_flags(record, interner)? & category == 0 => output.push(record),
            b"NPC_" => {
                let target = record
                    .fields
                    .iter()
                    .find(|field| field.sig.0 == *b"TPLT")
                    .and_then(|field| first_key(&field.value))
                    .ok_or("NPC inherits a template category without TPLT")?;
                targets.push(target);
            }
            b"LVLN" => {
                for field in record.fields.iter().filter(|field| field.sig.0 == *b"LVLO") {
                    let previous_count = targets.len();
                    collect_keys(&field.value, &mut targets);
                    if targets.len() == previous_count {
                        for raw in embedded_form_ids(&field.value, 4, 12) {
                            targets.push(
                                resolve_embedded_form_id(record.form_key, raw, records)
                                    .ok_or_else(|| {
                                        format!("unresolved LVLN template entry {raw:08X}")
                                    })?,
                            );
                        }
                    }
                }
                if targets.is_empty() {
                    return Err("LVLN template has no NPC entries".into());
                }
            }
            _ => {
                return Err(format!(
                    "invalid NPC template target {}",
                    record.sig.as_str()
                ));
            }
        }
        for target in targets {
            let child = records
                .get(&target)
                .copied()
                .ok_or_else(|| format!("missing NPC template {}", target.format(interner)))?;
            visit(child, category, records, interner, path, output)?;
        }
        path.remove(&record.form_key);
        Ok(())
    }
    let mut output = Vec::new();
    visit(
        record,
        category,
        records,
        interner,
        &mut HashSet::new(),
        &mut output,
    )?;
    output.sort_by_key(|record| record.form_key.format(interner));
    output.dedup_by_key(|record| record.form_key);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{SigCode, SubrecordSig};
    use crate::record::FieldEntry;

    fn npc(interner: &StringInterner, local: u32, flags: u16, template: Option<FormKey>) -> Record {
        let mut record = Record::new(
            SigCode(*b"NPC_"),
            FormKey {
                local,
                plugin: interner.intern("Skyrim.esm"),
            },
        );
        let mut config = vec![0; 24];
        config[18..20].copy_from_slice(&flags.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig(*b"ACBS"),
            value: FieldValue::Bytes(config.into_iter().collect()),
        });
        if let Some(template) = template {
            record.fields.push(FieldEntry {
                sig: SubrecordSig(*b"TPLT"),
                value: FieldValue::FormKey(template),
            });
        }
        record
    }

    #[test]
    fn leveled_templates_keep_distinct_category_owners_and_reject_cycles() {
        let interner = StringInterner::new();
        let first = npc(&interner, 0x800, 0, None);
        let second = npc(&interner, 0x801, 0x2, Some(first.form_key));
        let mut list = Record::new(
            SigCode(*b"LVLN"),
            FormKey {
                local: 0x900,
                plugin: first.form_key.plugin,
            },
        );
        for key in [first.form_key, second.form_key] {
            let mut bytes = vec![0; 12];
            bytes[4..8].copy_from_slice(&key.local.to_le_bytes());
            list.fields.push(FieldEntry {
                sig: SubrecordSig(*b"LVLO"),
                value: FieldValue::Bytes(bytes.into_iter().collect()),
            });
        }
        let child = npc(&interner, 0x802, 0xA, Some(list.form_key));
        let records = [&first, &second, &list, &child]
            .into_iter()
            .map(|record| (record.form_key, record))
            .collect();
        assert_eq!(
            owners(&child, 0x8, &records, &interner)
                .unwrap()
                .iter()
                .map(|record| record.form_key)
                .collect::<Vec<_>>(),
            [first.form_key, second.form_key]
        );
        assert_eq!(
            owners(&child, 0x2, &records, &interner)
                .unwrap()
                .iter()
                .map(|record| record.form_key)
                .collect::<Vec<_>>(),
            [first.form_key]
        );
        assert_eq!(
            owners(&child, 0x100, &records, &interner).unwrap()[0].form_key,
            child.form_key
        );
        let cycle = npc(&interner, first.form_key.local, 0x8, Some(child.form_key));
        let records = [&cycle, &second, &list, &child]
            .into_iter()
            .map(|record| (record.form_key, record))
            .collect();
        assert!(
            owners(&child, 0x8, &records, &interner)
                .unwrap_err()
                .contains("cycle")
        );
    }
}

pub(crate) fn first_key(value: &FieldValue) -> Option<FormKey> {
    match value {
        FieldValue::FormKey(key) if key.local != 0 => Some(*key),
        FieldValue::List(values) => values.iter().find_map(first_key),
        FieldValue::Struct(fields) => fields.iter().find_map(|(_, value)| first_key(value)),
        _ => None,
    }
}

fn collect_keys(value: &FieldValue, output: &mut Vec<FormKey>) {
    match value {
        FieldValue::FormKey(key) if key.local != 0 => output.push(*key),
        FieldValue::List(values) => values.iter().for_each(|value| collect_keys(value, output)),
        FieldValue::Struct(fields) => fields
            .iter()
            .for_each(|(_, value)| collect_keys(value, output)),
        _ => {}
    }
}
