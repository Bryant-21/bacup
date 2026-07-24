use crate::record::{FieldValue, Record};

const MODEL_EXTENSIONS: &[&str] = &[".nif", ".hkx", ".kf", ".egm", ".egt"];
const KNOWN_ASSET_PREFIXES: &[&str] = &[
    "fo76",
    "fnv",
    "fo3",
    "fo4",
    "skyrim",
    "skyrimse",
    "starfield",
    "oblivion",
];

pub(super) fn normalize_model_paths(interner: &crate::sym::StringInterner, record: &mut Record) {
    for field in record.fields.iter_mut() {
        normalize_model_path_value(interner, &mut field.value);
    }
}

fn normalize_model_path_value(interner: &crate::sym::StringInterner, value: &mut FieldValue) {
    match value {
        FieldValue::String(sym) => {
            let Some(path) = interner.resolve(*sym) else {
                return;
            };
            if let Some(normalized) = normalized_model_path(path) {
                *sym = interner.intern(&normalized);
            }
        }
        FieldValue::List(items) => {
            for item in items {
                normalize_model_path_value(interner, item);
            }
        }
        FieldValue::Struct(fields) => {
            for (_, field_value) in fields {
                normalize_model_path_value(interner, field_value);
            }
        }
        _ => {}
    }
}

fn normalized_model_path(path: &str) -> Option<String> {
    let mut normalized = path.trim().trim_matches('\0').replace('\\', "/");
    normalized = normalized.trim_start_matches('/').to_string();
    if normalized.is_empty()
        || normalized.eq_ignore_ascii_case("none")
        || normalized.contains(':')
        || normalized.starts_with("0x")
        || normalized.starts_with("0X")
    {
        return None;
    }

    let lower = normalized.to_ascii_lowercase();
    if !MODEL_EXTENSIONS.iter().any(|ext| lower.ends_with(ext)) {
        return None;
    }

    if normalized.len() >= 5 && normalized[..5].eq_ignore_ascii_case("data/") {
        normalized = normalized[5..].to_string();
    }
    if normalized.len() >= 7 && normalized[..7].eq_ignore_ascii_case("meshes/") {
        normalized = normalized[7..].to_string();
    }

    let first_component = normalized.split('/').next().unwrap_or_default();
    if KNOWN_ASSET_PREFIXES
        .iter()
        .any(|prefix| first_component.eq_ignore_ascii_case(prefix))
    {
        normalized = normalized
            .split_once('/')
            .map(|(_, rest)| rest.to_string())
            .unwrap_or_default();
    }

    let output = normalized.replace('/', "\\");
    if output == path.trim().trim_matches('\0') {
        return None;
    }
    Some(output)
}

#[cfg(test)]
mod tests {
    use super::super::fo76_fo4::Fo76Fo4Hook;
    use crate::ids::{FormKey, SigCode, SubrecordSig};
    use crate::record::{FieldEntry, FieldValue, Record};
    use crate::sym::StringInterner;
    use crate::translator::pair_hook::{PairCtx, PairHook};

    fn make_ctx(interner: &StringInterner) -> PairCtx<'_> {
        PairCtx { interner }
    }

    fn make_record(sig: &str, interner: &StringInterner) -> Record {
        let fk = FormKey::parse("000800@SeventySix.esm", interner).unwrap();
        Record::new(SigCode::from_str(sig).unwrap(), fk)
    }

    fn push_field(record: &mut Record, sig: &str, value: FieldValue) {
        record.fields.push(FieldEntry {
            sig: SubrecordSig::from_str(sig).unwrap(),
            value,
        });
    }

    #[test]
    fn post_translate_strips_source_prefixed_model_paths() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("fo76\\Landscape\\Plants\\MtnTopCreosote03.nif")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(
            interner.resolve(sym),
            Some("Landscape\\Plants\\MtnTopCreosote03.nif")
        );
    }

    #[test]
    fn post_translate_strips_meshes_and_source_prefix_from_model_paths() {
        let mut interner = StringInterner::new();
        let mut record = make_record("STAT", &mut interner);
        push_field(
            &mut record,
            "MODL",
            FieldValue::String(interner.intern("Meshes\\fo76\\Landscape\\Trees\\Tree.nif")),
        );

        let hook = Fo76Fo4Hook;
        let mut ctx = make_ctx(&mut interner);
        hook.post_translate(&mut ctx, &mut record).unwrap();

        let FieldValue::String(sym) = record.fields[0].value else {
            panic!("expected model path string");
        };
        assert_eq!(interner.resolve(sym), Some("Landscape\\Trees\\Tree.nif"));
    }

    mod fnv {
        use super::super::super::fnv_fo4::FnvFo4Hook;
        use super::*;

        fn make_record(sig: &str, interner: &StringInterner) -> Record {
            let fk = FormKey::parse("000800@FalloutNV.esm", interner).unwrap();
            Record::new(SigCode::from_str(sig).unwrap(), fk)
        }

        #[test]
        fn post_translate_leaves_unprefixed_model_paths_unprefixed() {
            let mut interner = StringInterner::new();
            let mut record = make_record("STAT", &mut interner);
            push_field(
                &mut record,
                "MODL",
                FieldValue::String(interner.intern("Landscape\\Grass\\WastelandGrass01.nif")),
            );

            let hook = FnvFo4Hook;
            let mut ctx = make_ctx(&mut interner);
            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::String(sym) = record.fields[0].value else {
                panic!("expected model path string");
            };
            assert_eq!(
                interner.resolve(sym),
                Some("Landscape\\Grass\\WastelandGrass01.nif")
            );
        }

        #[test]
        fn post_translate_strips_source_prefixed_model_paths() {
            let mut interner = StringInterner::new();
            let mut record = make_record("STAT", &mut interner);
            push_field(
                &mut record,
                "MODL",
                FieldValue::String(interner.intern("fnv\\Landscape\\Grass\\WastelandGrass01.nif")),
            );

            let hook = FnvFo4Hook;
            let mut ctx = make_ctx(&mut interner);
            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::String(sym) = record.fields[0].value else {
                panic!("expected model path string");
            };
            assert_eq!(
                interner.resolve(sym),
                Some("Landscape\\Grass\\WastelandGrass01.nif")
            );
        }

        #[test]
        fn post_translate_strips_meshes_and_source_prefix_from_model_paths() {
            let mut interner = StringInterner::new();
            let mut record = make_record("STAT", &mut interner);
            push_field(
                &mut record,
                "MODL",
                FieldValue::String(
                    interner.intern("Meshes\\fnv\\Landscape\\Grass\\WastelandGrass01.nif"),
                ),
            );

            let hook = FnvFo4Hook;
            let mut ctx = make_ctx(&mut interner);
            hook.post_translate(&mut ctx, &mut record).unwrap();

            let FieldValue::String(sym) = record.fields[0].value else {
                panic!("expected model path string");
            };
            assert_eq!(
                interner.resolve(sym),
                Some("Landscape\\Grass\\WastelandGrass01.nif")
            );
        }
    }

    mod skyrimse {
        use super::super::super::skyrimse_fo4::SkyrimSeFo4Hook;
        use super::*;

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
    }
}
