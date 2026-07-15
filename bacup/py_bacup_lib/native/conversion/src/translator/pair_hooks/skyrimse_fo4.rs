use super::fo4_layouts::{self, SourceFamily};
use super::model_paths;
use crate::record::{FieldValue, Record};
use crate::translator::pair_hook::{HookResult, PairCtx, PairHook};

pub struct SkyrimSeFo4Hook;

impl SkyrimSeFo4Hook {
    fn drop_incompatible_debr_modt(record: &mut Record) {
        if record.sig.0 == *b"DEBR" {
            record.fields.retain(|entry| entry.sig.0 != *b"MODT");
        }
    }

    fn normalize_refr_map_marker_tnam(record: &mut Record) {
        if record.sig.0 != *b"REFR" || !record.fields.iter().any(|entry| entry.sig.0 == *b"XMRK") {
            return;
        }

        for entry in &mut record.fields {
            if entry.sig.0 != *b"TNAM" {
                continue;
            }
            let Some(source_type) = map_marker_type(&entry.value) else {
                continue;
            };
            write_map_marker_type(&mut entry.value, skyrim_map_marker_type_to_fo4(source_type));
        }
    }
}

impl PairHook for SkyrimSeFo4Hook {
    fn pre_translate(&self, _ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        Self::drop_incompatible_debr_modt(record);
        Self::normalize_refr_map_marker_tnam(record);
        match record.sig.0 {
            sig if sig == *b"REFR" => fo4_layouts::normalize_refr_xloc(record, _ctx.interner),
            sig if sig == *b"EFSH" => {
                fo4_layouts::normalize_efsh(record, SourceFamily::SkyrimSe, _ctx.interner)
            }
            sig if sig == *b"WTHR" => {
                fo4_layouts::normalize_wthr(record, SourceFamily::SkyrimSe, _ctx.interner)
            }
            sig if sig == *b"PROJ" => fo4_layouts::normalize_skyrim_proj(record),
            _ => {}
        }
        Ok(())
    }

    fn post_translate(&self, ctx: &mut PairCtx<'_>, record: &mut Record) -> HookResult {
        model_paths::normalize_model_paths(ctx.interner, record);
        Ok(())
    }

    fn synthesize_records(&self, _ctx: &mut PairCtx<'_>) -> Vec<Record> {
        Vec::new()
    }
}

fn map_marker_type(value: &FieldValue) -> Option<u8> {
    match value {
        FieldValue::Uint(value) => u8::try_from(*value).ok(),
        FieldValue::Int(value) => u8::try_from(*value).ok(),
        FieldValue::Bytes(bytes) => bytes.first().copied(),
        FieldValue::Struct(fields) => fields.first().and_then(|(_, value)| map_marker_type(value)),
        _ => None,
    }
}

fn write_map_marker_type(value: &mut FieldValue, target_type: u8) {
    match value {
        FieldValue::Uint(value) => *value = u64::from(target_type),
        FieldValue::Int(value) => *value = i64::from(target_type),
        FieldValue::Bytes(bytes) if !bytes.is_empty() => bytes[0] = target_type,
        FieldValue::Struct(fields) => {
            if let Some((_, value)) = fields.first_mut() {
                write_map_marker_type(value, target_type);
            }
        }
        _ => {}
    }
}

fn skyrim_map_marker_type_to_fo4(source_type: u8) -> u8 {
    match source_type {
        0 => 77,
        1 => 1,
        2 => 49,
        3 => 13,
        4 => 0,
        5 => 3,
        6 => 53,
        7 => 10,
        8 => 11,
        9 => 45,
        10 => 28,
        11 => 8,
        12 => 0,
        13 => 26,
        14 => 4,
        15 => 40,
        16 | 17 => 3,
        18 => 8,
        19 | 20 => 4,
        21 => 26,
        22 => 7,
        23 => 28,
        24 => 8,
        25 => 12,
        26 => 8,
        27 => 38,
        28 => 13,
        29 => 3,
        30 => 56,
        31 => 10,
        32 => 13,
        33 => 38,
        34 => 12,
        35 | 37 | 39 | 41 | 43 | 45 | 47 | 49 | 51 => 53,
        36 | 38 | 40 | 42 | 44 | 46 | 48 | 50 | 52 => 1,
        53 => 12,
        54 => 49,
        55 => 8,
        56 => 13,
        57 | 58 => 77,
        59 => 53,
        _ => 77,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue};
    use crate::sym::StringInterner;

    #[test]
    fn normalizes_skyrim_prefixed_model_paths() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("STAT").unwrap(), form_key);
        let model = interner.intern("Meshes/SkyrimSE/Architecture/Whiterun/Test.nif");
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODL").unwrap(),
            value: FieldValue::String(model),
        });

        let hook = SkyrimSeFo4Hook;
        let mut ctx = PairCtx {
            interner: &interner,
        };
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(model) = &record.fields[0].value else {
            panic!("model path should remain a string");
        };
        assert_eq!(
            interner.resolve(*model),
            Some("Architecture\\Whiterun\\Test.nif")
        );
        assert!(hook.synthesize_records(&mut ctx).is_empty());
    }

    #[test]
    fn drops_skyrim_debr_legacy_72_byte_modt() {
        let interner = StringInterner::new();
        let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("DEBR").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("DATA").unwrap(),
            value: FieldValue::Struct(vec![(
                interner.intern("model_file_name"),
                FieldValue::String(interner.intern("Effects\\IceShard.nif")),
            )]),
        });
        let mut legacy = smallvec::SmallVec::<[u8; 32]>::from_slice(&[0u8; 72]);
        legacy[..4].copy_from_slice(&0x85f3_0f60_u32.to_le_bytes());
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODT").unwrap(),
            value: FieldValue::Bytes(legacy),
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("MODT").unwrap(),
            value: FieldValue::Bytes(smallvec::smallvec![1, 2, 3]),
        });

        SkyrimSeFo4Hook
            .pre_translate(
                &mut PairCtx {
                    interner: &interner,
                },
                &mut record,
            )
            .unwrap();

        assert_eq!(record.fields.len(), 1);
        assert_eq!(record.fields[0].sig.as_str(), "DATA");
    }

    #[test]
    fn maps_skyrim_refr_marker_types_to_safe_fo4_icons() {
        let interner = StringInterner::new();
        let hook = SkyrimSeFo4Hook;
        let mut ctx = PairCtx {
            interner: &interner,
        };

        for (source_type, target_type) in [(4_u8, 0_u8), (49, 53), (255, 77)] {
            let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
            let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("XMRK").unwrap(),
                value: FieldValue::None,
            });
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("TNAM").unwrap(),
                value: FieldValue::Bytes(smallvec::smallvec![source_type, 7]),
            });

            hook.pre_translate(&mut ctx, &mut record).unwrap();

            assert_eq!(
                record.fields[1].value,
                FieldValue::Bytes(smallvec::smallvec![target_type, 7])
            );
        }
    }

    #[test]
    fn maps_decoded_marker_type_without_overwriting_unknown_byte() {
        let interner = StringInterner::new();
        let hook = SkyrimSeFo4Hook;
        let mut ctx = PairCtx {
            interner: &interner,
        };
        let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
        let mut record = Record::new(SigCode::from_str("REFR").unwrap(), form_key);
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("XMRK").unwrap(),
            value: FieldValue::None,
        });
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str("TNAM").unwrap(),
            value: FieldValue::Struct(vec![
                (interner.intern("type"), FieldValue::Uint(4)),
                (interner.intern("unknown_u8_1"), FieldValue::Uint(9)),
            ]),
        });

        hook.pre_translate(&mut ctx, &mut record).unwrap();

        assert_eq!(
            record.fields[1].value,
            FieldValue::Struct(vec![
                (interner.intern("type"), FieldValue::Uint(0)),
                (interner.intern("unknown_u8_1"), FieldValue::Uint(9)),
            ])
        );
    }

    #[test]
    fn leaves_non_marker_tnam_contexts_unchanged() {
        let interner = StringInterner::new();
        let hook = SkyrimSeFo4Hook;
        let mut ctx = PairCtx {
            interner: &interner,
        };

        for signature in ["REFR", "TERM"] {
            let form_key = FormKey::parse("000800@Skyrim_Merged.esm", &interner).unwrap();
            let mut record = Record::new(SigCode::from_str(signature).unwrap(), form_key);
            record.fields.push(FieldEntry {
                sig: SubrecordSig::from_str("TNAM").unwrap(),
                value: FieldValue::Bytes(smallvec::smallvec![49, 3]),
            });

            hook.pre_translate(&mut ctx, &mut record).unwrap();

            assert_eq!(
                record.fields[0].value,
                FieldValue::Bytes(smallvec::smallvec![49, 3])
            );
        }
    }
}
