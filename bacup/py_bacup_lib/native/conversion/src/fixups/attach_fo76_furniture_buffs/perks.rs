use super::*;

const MGEF_ACTOR_VALUE: usize = 68;
const MGEF_PERK: usize = 136;

pub(super) fn repair_buff_effects(
    session: &mut PluginSession,
    mapper: &mut FormKeyMapper,
) -> Result<FixupReport, FixupError> {
    let mut report = FixupReport::empty();
    let schema = session.schema().map_err(handle_error)?;
    let source_schema = session.source_schema().map_err(handle_error)?;
    let Some(source) = session.source_slot_opt() else {
        return Ok(report);
    };
    let source_plugin = mapper.interner.intern(&source.parsed.plugin_name);
    let masters = session.target_masters().to_vec();
    let mut changed = Vec::new();
    for (local, entry_point, tabs) in [(0x76B52D, 22, 3), (0x89ADB4, 123, 1)] {
        let Some(fk) = mapper.lookup(FormKey {
            plugin: source_plugin,
            local,
        }) else {
            continue;
        };
        let Ok(mut effect) = session.record_decoded(&fk, &schema, mapper.interner) else {
            continue;
        };
        let Some(mut data) = session
            .first_subrecord_bytes(&fk, "DATA")
            .map_err(handle_error)?
        else {
            continue;
        };
        if effect.sig != SigCode(*b"MGEF") || data.len() != 152 || read_u32(&data, MGEF_PERK) != 0 {
            continue;
        }
        let actor_value = read_u32(&data, MGEF_ACTOR_VALUE);
        if actor_value == 0 {
            return Err(FixupError::Other(format!(
                "buff effect {local:06X} has no actor value"
            )));
        }
        // FO4's Stabilized perk uses function 14 / EPFT 8: 1 + actor value * scale.
        let eid = format!("B21_FurnitureBuffPerk_{local:06X}");
        let synthetic = FormKey {
            plugin: mapper.interner.intern(&eid),
            local: 1,
        };
        let perk_fk = mapper.allocate_or_resolve(synthetic, None, SigCode(*b"PERK"));
        let perk = percent_perk(
            perk_fk,
            &eid,
            actor_value,
            entry_point,
            tabs,
            mapper.interner,
        );
        session
            .add_record(perk, &schema, mapper.interner)
            .map_err(handle_error)?;
        report.records_added += 1;
        let raw = encode_target_form_id(perk_fk, mapper.interner, &masters)
            .ok_or_else(|| FixupError::Other("cannot encode furniture buff perk".into()))?;
        data[MGEF_PERK..MGEF_PERK + 4].copy_from_slice(&raw.to_le_bytes());
        set_data(&mut effect, data);
        changed.push(effect);
    }

    for (local, tabs, parameter_is_form) in [(0x3CD031, 3, false), (0x8D1C9B, 2, true)] {
        let source_fk = FormKey {
            plugin: source_plugin,
            local,
        };
        let Some(target_fk) = mapper.lookup(source_fk) else {
            continue;
        };
        let Ok(source) = session.source_record_decoded(&source_fk, &source_schema, mapper.interner)
        else {
            continue;
        };
        let Ok(mut target) = session.record_decoded(&target_fk, &schema, mapper.interner) else {
            continue;
        };
        if restore_perk_entry(
            &source,
            &mut target,
            tabs,
            parameter_is_form,
            mapper,
            &masters,
        )? {
            changed.push(target);
        }
    }

    let source_rest = FormKey {
        plugin: source_plugin,
        local: 0x3CD033,
    };
    if let Some(target_fk) = mapper.lookup(source_rest) {
        if let (Ok(source), Ok(mut target)) = (
            session.source_record_decoded(&source_rest, &source_schema, mapper.interner),
            session.record_decoded(&target_fk, &schema, mapper.interner),
        ) {
            if restore_rest_conditions(&source, &mut target, mapper, &masters)? {
                changed.push(target);
            }
        }
    }

    let effect_fk = mapper.lookup(FormKey {
        plugin: source_plugin,
        local: 0x897406,
    });
    let spell_fk = mapper.lookup(FormKey {
        plugin: source_plugin,
        local: 0x897408,
    });
    if let (Some(effect_fk), Some(spell_fk)) = (effect_fk, spell_fk) {
        if let (Ok(mut effect), Ok(mut spell), Some(mut data)) = (
            session.record_decoded(&effect_fk, &schema, mapper.interner),
            session.record_decoded(&spell_fk, &schema, mapper.interner),
            session
                .first_subrecord_bytes(&effect_fk, "DATA")
                .map_err(handle_error)?,
        ) {
            let perk = FormKey {
                plugin: mapper.interner.intern("Fallout4.esm"),
                local: 0x0BA448,
            };
            let perk_raw =
                encode_target_form_id(perk, mapper.interner, &masters).ok_or_else(|| {
                    FixupError::Other("chem duration buff requires Fallout4.esm".into())
                })?;
            if data.len() == 152 && read_u32(&data, MGEF_PERK) == 0 {
                // FO76 stores 0.25; FO4's ChemDurationMod perk consumes percentage points.
                scale_magnitude(&mut spell, effect_fk, 100.0, mapper.interner);
                data[MGEF_PERK..MGEF_PERK + 4].copy_from_slice(&perk_raw.to_le_bytes());
                set_data(&mut effect, data);
                changed.extend([effect, spell]);
            }
        }
    }
    if let Some(fk) = mapper.lookup(FormKey {
        plugin: source_plugin,
        local: 0x8B3A2A,
    }) {
        if session
            .record_decoded(&fk, &schema, mapper.interner)
            .is_ok()
        {
            report.warnings.push(mapper.interner.intern(
                "furniture_buff:8B3A2A:Rip_Bounty_loot_entry_point_169_requires_FO4_adapter",
            ));
        }
    }
    let expected = changed.len();
    let replaced = session
        .replace_records_contents(changed, &schema, mapper.interner)
        .map_err(handle_error)?;
    if replaced != expected {
        return Err(FixupError::Other(format!(
            "buff effects replaced {replaced} of {expected} records"
        )));
    }
    report.records_changed = replaced as u32;
    Ok(report)
}

fn raw(sig: &[u8; 4], bytes: impl Into<Vec<u8>>) -> FieldEntry {
    FieldEntry {
        sig: SubrecordSig(*sig),
        value: FieldValue::Bytes(bytes.into().into()),
    }
}

fn read_u32(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().unwrap())
}

fn set_data(record: &mut Record, data: Vec<u8>) {
    record
        .fields
        .iter_mut()
        .find(|entry| entry.sig.0 == *b"DATA")
        .unwrap()
        .value = FieldValue::Bytes(data.into());
}

fn percent_perk(
    fk: FormKey,
    eid: &str,
    actor_value: u32,
    entry_point: u8,
    tabs: u8,
    interner: &StringInterner,
) -> Record {
    let mut perk = Record::new(SigCode(*b"PERK"), fk);
    perk.eid = Some(interner.intern(eid));
    let mut parameter = actor_value.to_le_bytes().to_vec();
    parameter.extend_from_slice(&0.01f32.to_le_bytes());
    perk.fields.extend([
        raw(b"DATA", [0, 0, 1, 0, 1]),
        raw(b"PRKE", [2, 0, 0]),
        raw(b"DATA", [entry_point, 14, tabs]),
        raw(b"EPFT", [8]),
        raw(b"EPFD", parameter),
        raw(b"PRKF", []),
    ]);
    perk
}

fn restore_perk_entry(
    source: &Record,
    target: &mut Record,
    tabs: u8,
    parameter_is_form: bool,
    mapper: &FormKeyMapper,
    masters: &[String],
) -> Result<bool, FixupError> {
    let Some(FieldValue::Bytes(source_data)) = source
        .fields
        .iter()
        .skip_while(|entry| entry.sig.0 != *b"PRKE")
        .find(|entry| entry.sig.0 == *b"DATA")
        .map(|entry| &entry.value)
    else {
        return Ok(false);
    };
    let Some(FieldValue::Bytes(parameter)) = field(source, b"EPFD") else {
        return Ok(false);
    };
    if source_data.len() != 4 || parameter.len() != 4 {
        return Ok(false);
    }
    let parameter = if parameter_is_form {
        let fk = FormKey {
            plugin: source.form_key.plugin,
            local: read_u32(parameter, 0),
        };
        let mapped = mapper
            .lookup(fk)
            .and_then(|fk| encode_target_form_id(fk, mapper.interner, masters))
            .ok_or_else(|| FixupError::Other("sharpening stone loot list is unmapped".into()))?;
        mapped.to_le_bytes().to_vec()
    } else {
        parameter.to_vec()
    };
    let before = target.fields.clone();
    let mut in_entry = false;
    target.fields.retain(|entry| entry.sig.0 != *b"EPFD");
    for entry in &mut target.fields {
        if entry.sig.0 == *b"PRKE" {
            in_entry = true;
        }
        if entry.sig.0 == *b"DATA" {
            entry.value = if in_entry {
                FieldValue::Bytes(vec![source_data[0], source_data[1], tabs].into())
            } else {
                FieldValue::Bytes(vec![0, 0, 1, 0, 1].into())
            };
        }
    }
    let Some(end) = target
        .fields
        .iter()
        .position(|entry| entry.sig.0 == *b"PRKF")
    else {
        return Ok(false);
    };
    target.fields.insert(end, raw(b"EPFD", parameter));
    Ok(target.fields != before)
}

fn scale_magnitude(spell: &mut Record, effect: FormKey, factor: f32, interner: &StringInterner) {
    let mut matching = false;
    for entry in &mut spell.fields {
        if entry.sig.0 == *b"EFID" {
            matching = entry.value == FieldValue::FormKey(effect);
        }
        if entry.sig.0 != *b"EFIT" || !matching {
            continue;
        }
        match &mut entry.value {
            FieldValue::Bytes(bytes) if bytes.len() >= 4 => {
                let value = f32::from_le_bytes(bytes[..4].try_into().unwrap()) * factor;
                bytes[..4].copy_from_slice(&value.to_le_bytes());
            }
            FieldValue::Struct(fields) => {
                for (name, value) in fields {
                    if interner
                        .resolve(*name)
                        .is_some_and(|name| name.eq_ignore_ascii_case("magnitude"))
                    {
                        if let FieldValue::Float(value) = value {
                            *value *= factor;
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn restore_rest_conditions(
    source: &Record,
    target: &mut Record,
    mapper: &FormKeyMapper,
    masters: &[String],
) -> Result<bool, FixupError> {
    let mut conditions: Vec<Vec<FieldEntry>> = Vec::new();
    for entry in &source.fields {
        if entry.sig.0 == *b"EFID" {
            conditions.push(Vec::new());
        }
        if entry.sig.0 != *b"CTDA" {
            continue;
        }
        let FieldValue::Bytes(bytes) = &entry.value else {
            return Ok(false);
        };
        if bytes.len() != 32 || conditions.is_empty() {
            return Ok(false);
        }
        let function = u16::from_le_bytes(bytes[8..10].try_into().unwrap());
        if !matches!(function, 74 | 448) {
            return Ok(false);
        }
        let source_fk = FormKey {
            plugin: source.form_key.plugin,
            local: read_u32(bytes, 12),
        };
        let mapped = mapper
            .lookup(source_fk)
            .and_then(|fk| encode_target_form_id(fk, mapper.interner, masters))
            .ok_or_else(|| {
                FixupError::Other("resting buff condition reference is unmapped".into())
            })?;
        let mut bytes = bytes.to_vec();
        bytes[12..16].copy_from_slice(&mapped.to_le_bytes());
        conditions.last_mut().unwrap().push(raw(b"CTDA", bytes));
    }
    if conditions.len()
        != target
            .fields
            .iter()
            .filter(|entry| entry.sig.0 == *b"EFID")
            .count()
    {
        return Ok(false);
    }
    let before = target.fields.clone();
    let mut output = Vec::new();
    let mut in_effects = false;
    let mut index = 0;
    for entry in &target.fields {
        if entry.sig.0 == *b"EFID" {
            in_effects = true;
        }
        if in_effects && entry.sig.0 == *b"CTDA" {
            continue;
        }
        output.push(entry.clone());
        if entry.sig.0 == *b"EFIT" {
            output.extend(conditions[index].clone());
            index += 1;
        }
    }
    target.fields = output.into();
    Ok(target.fields != before)
}

#[cfg(test)]
mod tests;
