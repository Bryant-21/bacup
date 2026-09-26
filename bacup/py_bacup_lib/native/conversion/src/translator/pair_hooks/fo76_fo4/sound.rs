use super::*;
use crate::sym::StringInterner;

impl Fo76Fo4Hook {
    pub(super) fn normalize_boss_sound_paths(interner: &StringInterner, record: &mut Record) {
        if record.sig.0 != *b"SNDR"
            || !interner
                .resolve(record.form_key.plugin)
                .is_some_and(|name| name.eq_ignore_ascii_case(FO76_MASTER_NAME))
        {
            return;
        }
        let (eid, prefix, count) = match record.form_key.local {
            0x55DBC9 => ("NPCWendigoColossusVomit", "npc_wendigocolossus_vomit_", 6),
            0x67F0A2 => (
                "NPCUltraciteAbominationBelchLayerA",
                "npc_ultracite_abomination_belch_",
                2,
            ),
            _ => return,
        };
        if record.eid.and_then(|value| interner.resolve(value)) != Some(eid) {
            return;
        }
        let paths: Vec<_> = record
            .fields
            .iter()
            .filter_map(|field| {
                if field.sig.0 != *b"ANAM" {
                    return None;
                }
                match field.value {
                    FieldValue::String(value) => interner.resolve(value).map(str::to_owned),
                    _ => None,
                }
            })
            .collect();
        let expected = (1..=count)
            .map(|index| {
                if record.form_key.local == 0x55DBC9 {
                    format!("data\\sound\\fx\\npc\\wendigocolossus\\{prefix}{index:02}.wav")
                } else {
                    format!("sound\\fx\\npc\\ultraciteabomination\\{prefix}{index:02}_layera.wav")
                }
            })
            .collect::<Vec<_>>();
        if paths.len() != count
            || !paths
                .iter()
                .zip(&expected)
                .all(|(path, expected)| path.replace('/', "\\").to_ascii_lowercase() == *expected)
        {
            record.warnings.push(
                interner.intern("nuclear boss SNDR source paths changed; XWM repair skipped"),
            );
            return;
        }
        for field in &mut record.fields {
            if field.sig.0 == *b"ANAM" {
                if let FieldValue::String(value) = field.value {
                    let path = interner.resolve(value).unwrap();
                    field.value = FieldValue::String(
                        interner.intern(&format!("{}.xwm", &path[..path.len() - 4])),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn titan_sound_descriptors_follow_installed_xwm_paths() {
        let interner = StringInterner::new();
        let plugin = interner.intern(FO76_MASTER_NAME);
        for (id, eid, dir, stem, count) in [
            (
                0x55DBC9,
                "NPCWendigoColossusVomit",
                "Data\\SOUND\\FX\\NPC\\WendigoColossus",
                "NPC_WendigoColossus_Vomit",
                6,
            ),
            (
                0x67F0A2,
                "NPCUltraciteAbominationBelchLayerA",
                "Sound\\FX\\NPC\\UltraciteAbomination",
                "NPC_Ultracite_Abomination_Belch",
                2,
            ),
        ] {
            let mut record = Record::new(SigCode(*b"SNDR"), FormKey { plugin, local: id });
            record.eid = Some(interner.intern(eid));
            for index in 1..=count {
                let suffix = if id == 0x67F0A2 { "_LayerA" } else { "" };
                let path = format!("{dir}\\{stem}_{index:02}{suffix}.wav");
                record.fields.push(FieldEntry {
                    sig: SubrecordSig(*b"ANAM"),
                    value: FieldValue::String(interner.intern(&path)),
                });
            }
            Fo76Fo4Hook::normalize_boss_sound_paths(&interner, &mut record);
            assert!(record.fields.iter().all(|field| match field.value {
                FieldValue::String(value) => interner.resolve(value).unwrap().ends_with(".xwm"),
                _ => false,
            }));
        }
    }
}
